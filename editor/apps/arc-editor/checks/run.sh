#!/bin/sh
# Checks the built editor by playing input into its window.
#
# Each case starts the editor, plays the steps as the cursor, mouse buttons, wheel and keys the
# desktop would deliver (src/input_script.rs), and matches parts of the result line the editor
# prints. Each expectation is an extended regular expression. How far a drag of so many pixels
# moves an object depends on the size of the view, so distances are matched as ranges. Every case opens one window for a few seconds and closes it. Run from anywhere:
#
#   cargo build -p arc-editor && apps/arc-editor/checks/run.sh
#
# The windows are not put on the desktop, so a run does not cover the screen of whoever is at
# the machine; no picture is taken of them (`picture=none`). To watch a case or to have its
# picture, show them: `ARC_EDITOR_CHECK_VISIBLE=1 run.sh stroke`.
#
# With an argument only the cases whose name contains it are run, for instance `run.sh stroke`.
#
# The cases assume the game's current map as the editor core converts it, in a window at
# least 600 logical pixels wide. Row 11 is "Tall box", a box of half size 0.5 at (0, 0.5, -8).
#
# What the toolbar offers is reached with `choose=Menu>Caption`: Level and Add are menus at
# every width, and the tools and the panel switches are buttons in a wide window and menus in a
# narrow one. The step takes whichever is on the window, so a case holds at either width.
set -u
# The editor to check: the dev build, or the one named by ARC_EDITOR_BIN.
editor="${ARC_EDITOR_BIN:-$(cd "$(dirname "$0")/../../.." && pwd)/target/debug/arc-editor}"
# Each case's level folder, pictures and cooked packages go under a temporary folder that is
# removed at the end: in ARC_EDITOR_TMP if that is set, else in the system's.
work="$(mktemp -d "${ARC_EDITOR_TMP:-${TMPDIR:-/tmp}}/arc-editor-check.XXXXXX")"
trap 'rm -rf "$work"' EXIT
# limit <seconds> <command>...: runs the command and ends it after so many seconds. With GNU
# timeout where there is one; macOS has none, and perl's alarm does the same there.
if command -v timeout >/dev/null 2>&1; then
    limit() { timeout "$@"; }
else
    limit() { perl -e 'alarm shift; exec @ARGV or die "cannot run $ARGV[0]: $!\n"' "$@"; }
fi
failed=0
only="${1:-}"
# The one argument the editor is started with, as a person starts it on a project file;
# none unless a case names one (with_arg).
editor_arg=""

# wanted <name>: whether the case is to be run.
wanted() {
    case "$1" in *"$only"*) return 0 ;; *) return 1 ;; esac
}

# What every case expects besides what it names: that each of its steps could be played.
no_failed_steps='failed_steps=\[\]'
# with_arg <path> <check function> <name> <steps> <expected>...: the case's editor is started
# with this one argument.
with_arg() {
    editor_arg="$1"; shift
    "$@"
    editor_arg=""
}
# check_failing <name> <steps> <expected>...: the same for a case one of whose steps is meant
# to fail. It says itself which failure it expects.
check_failing() {
    no_failed_steps='failed_steps='
    check "$@"
    no_failed_steps='failed_steps=\[\]'
}

# check <name> <steps> <expected part of the result>...
check() {
    name="$1"; steps="$2"; shift 2
    wanted "$name" || return 0
    # Each case has its own folder, so that no case finds another's saved level. A case named
    # like an earlier one with "-again" and anything after it runs in that one's folder, on
    # purpose.
    folder="$work/${name%%-again*}"
    mkdir -p "$folder"
    case "$name" in
        # A file where the levels folder should be: saving there must fail.
        nosave*) [ -e "$folder/levels" ] || : > "$folder/levels" ;;
        *) mkdir -p "$folder/levels" ;;
    esac
    result="$(cd "$folder" && ARC_EDITOR_CHECK="$folder/$name.png" ARC_EDITOR_CHECK_STEPS="$steps" \
        limit 180 "$editor" ${editor_arg:+"$editor_arg"} 2>&1 | grep '^ARC_EDITOR_CHECK_RESULT')"
    # Kept for the comparisons between cases further down, and in ARC_EDITOR_RESULTS if set.
    printf '%s\n' "$result" > "$work/$name.result"
    [ -z "${ARC_EDITOR_RESULTS:-}" ] || printf '%s\n' "$result" > "$ARC_EDITOR_RESULTS/$name.result"
    # A case that asks for a window width says nothing about that width where the desktop
    # gave another. It is not run as a pass or a failure of the editor, and it says so.
    if [ -n "${ARC_EDITOR_CHECK_WINDOW:-}" ] && [ -n "$result" ]; then
        asked="${ARC_EDITOR_CHECK_WINDOW%%x*}"
        got="$(printf '%s\n' "$result" | sed -n 's/.* window=\([0-9]*\).*/\1/p')"
        if [ "$got" != "$asked" ]; then
            echo "$name: NOT CHECKED AT $asked, the desktop gave a window $got logical pixels wide"
            overridden=1
            return 0
        fi
    fi
    verdict=ok
    for expected in "$@" "$no_failed_steps"; do
        if ! printf '%s\n' "$result" | grep -Eq -- "$expected"; then
            verdict="FAILED, no '$expected'"
        fi
    done
    echo "$name: $verdict"
    if [ "$verdict" != ok ]; then
        echo "  steps:  $steps"
        echo "  result: $result"
        failed=1
    fi
}

# check_reactive <name> <steps> <expected>...: the same, but the check wakes the editor only
# with the input events it plays and reads the result once the editor is at rest. What is
# laid out then is what the editor showed by its own doing. A step takes about a second.
check_reactive() {
    # Set for this one case only. Written before the call it would stay set afterwards in some
    # shells (bash as sh), and every later case would be a reactive one.
    ARC_EDITOR_CHECK_REACTIVE=1; export ARC_EDITOR_CHECK_REACTIVE
    check "$@"
    unset ARC_EDITOR_CHECK_REACTIVE
}

# check_sized <width>x<height> <name> <steps> <expected>...: the same in a window of that size
# in logical pixels. A desktop that sizes windows itself (a tiling one) may give another width;
# the case then says so instead of a verdict, and the run ends by naming how many did.
overridden=0
check_sized() {
    ARC_EDITOR_CHECK_WINDOW="$1"; export ARC_EDITOR_CHECK_WINDOW
    shift
    check "$@"
    unset ARC_EDITOR_CHECK_WINDOW
}
# with_game <script> <check function> <name> <steps> <expected>...: the same with a scripted
# stand-in for the game (runs, hangs, crashes, window-closed, server-stays, needs-kill,
# unavailable), so
# that the Play control can be checked without the game's programs.
with_game() {
    ARC_EDITOR_CHECK_GAME="$1"; export ARC_EDITOR_CHECK_GAME
    shift
    "$@"
    unset ARC_EDITOR_CHECK_GAME
}
# The two widths the one-row toolbar is accepted at: the wide and the narrow window the tiling
# desktop of the first checks gave. At 1261 the tools and panel switches are buttons, at 621
# they are menus.
wide=1261x900
narrow=621x746

# field <case> <name>: the value of a field of a case's result line. For values without spaces.
field() {
    sed -n "s/.* $2=\([^ ]*\).*/\1/p" "$work/$1.result" 2>/dev/null
}

# same <case> <other case> <field>...: the two cases' results agree in these fields.
# differ <case> <other case> <field>...: they do not.
compare() {
    wanted_same="$1"; first="$2"; second="$3"; shift 3
    wanted "$first" && wanted "$second" || return 0
    for name in "$@"; do
        a="$(field "$first" "$name")"; b="$(field "$second" "$name")"
        verdict=ok
        if [ -z "$a" ] || [ -z "$b" ]; then
            verdict="FAILED, no $name in a result"
        elif [ "$wanted_same" = yes ] && [ "$a" != "$b" ]; then
            verdict="FAILED, $a is not $b"
        elif [ "$wanted_same" = no ] && [ "$a" = "$b" ]; then
            verdict="FAILED, both are $a"
        fi
        if [ "$wanted_same" = yes ]; then relation=same; else relation=differs; fi
        echo "$first/$second $relation $name: $verdict"
        [ "$verdict" = ok ] || failed=1
    done
}
same() { compare yes "$@"; }
differ() { compare no "$@"; }

# The box moved some metres along -X, and where it starts. How many metres a drag of so many
# pixels is depends on the size the desktop gives the window.
moved='position=Some\(\[-[1-9][0-9]*\.[0-9]+, 0\.5, -8\.0\]\)'
home='position=Some\(\[0\.0, 0\.5, -8\.0\]\)'

check row-click       'click-row=11'                      'selected=Some\("Tall box"\)'
check object-click    'click-object=11'                   'selected=Some\("Tall box"\)'
check gizmo-move      'click-row=11,drag-gizmo=-60'       "$moved" 'undo=Some\("drag"\)' 'dragging=false'
check undo-key        'click-row=11,drag-gizmo=-60,key=ctrl+z' "$home" 'redo=Some\("drag"\)'
check redo-key        'click-row=11,drag-gizmo=-60,key=ctrl+z,key=ctrl+shift+z' "$moved" 'undo=Some\("drag"\)'
check escape-cancels  'click-row=11,hold-gizmo=-60,key=escape,release' "$home" 'undo=None' 'dragging=false'
check unfocus-cancels 'click-row=11,hold-gizmo=-60,unfocus,release'    "$home" 'undo=None' 'dragging=false'
# The cursor leaves the window in the middle of a drag, comes back, and the button is released
# late. The drag must be over at once, in the core and in the gizmo, and a new drag must work
# and be the only undo step.
check leave-cancels   'click-row=11,hold-gizmo=-60,leave'  "$home" 'undo=None' 'dragging=false' 'gizmo_active=false'
check drag-after-leave 'click-row=11,hold-gizmo=-60,leave,release,drag-gizmo=-60' "$moved" 'undo=Some\("drag"\)' 'dragging=false' 'gizmo_active=false'
check undo-after-leave 'click-row=11,hold-gizmo=-60,leave,release,drag-gizmo=-60,key=ctrl+z' "$home" 'undo=None' 'redo=Some\("drag"\)'
check gizmo-scale     'click-row=11,key=r,drag-gizmo=-30' 'tool=Scale' "$home" 'half_size=Some\(\[0\.[0-4][0-9]*, 0\.5, 0\.5\]\)' 'undo=Some\("drag"\)'
# E, then a drag a sixth of the way round the gizmo's ring about Y: the box turns about Y only.
check gizmo-rotate    'click-row=11,key=e,rotate-gizmo=60' 'tool=Rotate' "$home" 'rotation=Some\(\[0\.0, -?[1-9][0-9]*\.[0-9], 0\.0\]\)' 'undo=Some\("drag"\)'
check tool-keys       'click-row=11,key=e'                'tool=Rotate'
check tool-button     'choose=Tool>Scale'                'tool=Scale'
# The two panel switches each flip their panel, whatever the window's width shows by default.
check panel-buttons   'inspector,choose=Panels>Hierarchy,choose=Panels>Inspector' 'panels=\(true, false\)'
check camera          'drag-camera=-80:30,wheel=3'        'camera=\(1\.080,0\.730,26\.60\)'
check toolbar         'click-row=11,drag-gizmo=-60,click-button=Undo,click-button=Redo,choose=Level>Save,choose=Level>Cook' \
                      "$moved" 'dirty=false' 'cook_needed=false' 'problem=none'
check type-position   'click-row=11,inspector,type-field=translation.1:5.5' \
                      'position=Some\(\[0\.0, 5\.5, -8\.0\]\)' 'undo=Some\("edit"\)'
check type-size       'click-row=11,inspector,type-field=half_extents.0:2' \
                      'half_size=Some\(\[2\.0, 0\.5, 0\.5\]\)' 'undo=Some\("edit"\)'
check save-shortcut   'click-row=11,drag-gizmo=-60,key=ctrl+s' 'dirty=false' 'problem=none'
# A second editor in the same folder finds the saved level, opens it, and can save it again.
check save-shortcut-again 'click-row=11,drag-gizmo=-60,key=ctrl+s' \
                      "$moved" 'has_file=true' 'dirty=false' 'problem=none'
check type-rotation   'click-row=11,inspector,type-field=rotation.1:45' \
                      'rotation=Some\(\[0\.0, 45\.0, 0\.0\]\)' "$home" 'undo=Some\("edit"\)'
# The level is drawn once (22 objects); choosing one object redraws that object only.
check select-redraws-one 'click-row=11'                   'redrawn=23'
check move-redraws-one 'click-row=11,click-row=12'        'redrawn=25'
# A landscape is drawn from the geometry the core cooks, and adding it is one undo step.
check add-landscape   'choose=Add>Landscape'        'objects=23' 'ground_tiles=4' 'undo=Some\("create"\)' 'problem=none'
check undo-landscape  'choose=Add>Landscape,key=ctrl+z' 'objects=22' 'ground_tiles=0' 'redo=Some\("create"\)'

# The toolbar is one row at every width: as high as the design's toolbar, with everything in it
# inside it. Level and Add are menus.
row='toolbar_height=Some\(36\.0\)'
check toolbar-one-row 'click-row=11'                      "$row" 'toolbar_fits=Some\(true\)' 'menu_open=false'
# A menu opens from its button and does not change the row. Escape closes it; so does a second
# press on its button.
check menu-opens      'click-button=Level'                'menu_open=true' "$row" 'level_dialog=Closed'
check menu-escape     'click-button=Level,key=escape'     'menu_open=false' 'level_dialog=Closed'
check menu-toggle     'click-button=Add,click-button=Add' 'menu_open=false' 'objects=22'
# A row does what the button it replaces did, and the menu is closed afterwards.
check menu-row        'click-button=Add,click-item=Box'   'objects=23' 'selected=Some\("Box 15"\)' 'menu_open=false' 'undo=Some\("create"\)'
# With the keyboard: the menu opens on its first row, the arrow moves down past the divider,
# Enter chooses. The third row of Level is Save.
check menu-keys       'click-button=Level,key=down,key=down,key=enter' 'has_file=true' 'dirty=false' 'menu_open=false' 'problem=none'
# While a menu is open it has the keyboard: a tool key chooses no tool.
check menu-modal      'click-button=Level,key=t'          'menu_open=true' 'sculpt=false'
# A click beside an open menu closes it and is nothing else: the object under the cursor is not
# chosen. The next click there chooses it.
check menu-click-outside 'click-button=Add,click-object=11' 'menu_open=false' 'selected=None' 'objects=22'
check menu-click-after 'click-button=Add,click-object=11,click-object=11' 'selected=Some\("Tall box"\)'
# A press on another menu's button closes the open menu and opens that one.
check menu-switch     'click-button=Level,click-button=Add,click-item=Spawn point' 'objects=23' 'selected=Some\("Spawn 9"\)' 'menu_open=false' 'level_dialog=Closed'
# At 1261 pixels: one row, every tool and panel switch a button, the chosen tool by its button.
check_sized "$wide" wide-one-row 'click-row=11'           'window=1261x' 'toolbar_folded=false' "$row" 'toolbar_fits=Some\(true\)'
check_sized "$wide" wide-tool    'click-button=Rotate'    'window=1261x' 'toolbar_folded=false' 'tool=Rotate' "$row"
check_sized "$wide" wide-menu    'click-button=Level'     'window=1261x' 'menu_open=true' "$row" 'toolbar_fits=Some\(true\)'
# At 621 pixels: the tools are one menu, the panel switches another, still one row.
check_sized "$narrow" narrow-one-row 'click-row=11'       'window=621x' 'toolbar_folded=true' "$row" 'toolbar_fits=Some\(true\)'
check_sized "$narrow" narrow-tool   'choose=Tool>Rotate'  'window=621x' 'toolbar_folded=true' 'tool=Rotate' 'sculpt=false' 'menu_open=false'
check_sized "$narrow" narrow-paint  'choose=Tool>Paint'   'window=621x' 'toolbar_folded=true' 'paint=true' 'menu_open=false'
check_sized "$narrow" narrow-panels 'inspector,choose=Panels>Hierarchy,choose=Panels>Inspector' 'window=621x' 'toolbar_folded=true' 'panels=\(true, false\)'
check_sized "$narrow" narrow-add    'choose=Add>Landscape,choose=Level>Save' 'window=621x' 'toolbar_folded=true' 'objects=23' 'ground_tiles=4' 'dirty=false' 'problem=none'
check_sized "$narrow" narrow-menu   'click-button=Panels' 'window=621x' 'menu_open=true' "$row" 'toolbar_fits=Some\(true\)'

# Several objects. Shift with a click adds the object to the selection or takes it out, in the
# hierarchy and in the 3D view; a plain click selects one again. The inspector says how many
# are selected and shows a dash for each number they differ in: the Tall box and the Low box
# differ in X and in height and share their depth.
two='selection=2'
apart='offsets=\[\[0\.0, 0\.0, 0\.0\], \[3\.0, -0\.35, 0\.0\]\]'
both_home='places=\[\[0\.0, 0\.5, -8\.0\], \[3\.0, 0\.15, -8\.0\]\]'
check multi-rows      'click-row=11,shift-click-row=12'   "$two" 'selected=Some\("Tall box"\)' 'several=Some\("2 objects"\)' 'dashes=Some\(\[true, true, false\]\)'
# A Shift-click on a selected row does the next thing: on the row the gizmo is on, which is
# the one clicked last, it takes that object out; on another selected row it moves the gizmo
# there and both stay selected.
check multi-row-off   'click-row=11,shift-click-row=12,shift-click-row=12' 'selection=1' 'selected=Some\("Tall box"\)' 'several=None' 'dashes=Some\(\[false, false, false\]\)'
check multi-row-active 'click-row=11,shift-click-row=12,shift-click-row=11' "$two" 'several=Some\("2 objects"\)' 'dashes=Some\(\[true, true, false\]\)'
check multi-row-active-off 'click-row=11,shift-click-row=12,shift-click-row=11,shift-click-row=11' 'selection=1' 'selected=Some\("Low box"\)' 'several=None'
# In the 3D view the two are the Wall and the Tall box, which stand clear of each other's
# gizmo. The Low box and the Pillar are seen behind the X and Y handles of the gizmo on the
# Tall box, and a click on a handle is a hold on the gizmo and no choice. The core gives a
# selection in the order of the objects' IDs, so the Wall is its first whichever was clicked
# first; the gizmo is on the one clicked last. One more Shift-click on that one takes it out.
wall_and_box='places=\[\[-8\.5, 3\.0, 0\.0\], \[0\.0, 0\.5, -8\.0\]\]'
check multi-objects   'click-object=11,shift-click-object=10' "$two" 'selected=Some\("Wall"\)' "$wall_and_box" 'several=Some\("2 objects"\)'
check multi-objects-other 'click-object=10,shift-click-object=11' "$two" 'selected=Some\("Wall"\)' "$wall_and_box"
# The Tall box, clicked last, has the gizmo, and the middle of an object that has the gizmo is
# behind the gizmo's handles: a click there is a hold on a handle. Its row takes it out.
check multi-object-off 'click-object=10,shift-click-object=11,shift-click-row=11' 'selection=1' 'selected=Some\("Wall"\)' 'active=Some\("Wall"\)' 'several=None'
check multi-object-held 'click-object=10,shift-click-object=11,shift-click-object=11' "$two" 'active=Some\("Tall box"\)' 'gizmo_hover=Some' 'undo=None'
check multi-plain     'click-row=11,shift-click-row=12,click-row=13' 'selection=1' 'selected=Some\("Pillar"\)' 'several=None'
# The gizmo holds the one clicked last and the other goes with it: one move, so that they
# stand to each other as before, and one undo step that takes both back.
check multi-move      'click-row=11,shift-click-row=12,drag-gizmo=-60' "$two" "$moved" "$apart" 'undo=Some\("drag"\)' 'dragging=false'
check multi-undo      'click-row=11,shift-click-row=12,drag-gizmo=-60,key=ctrl+z' "$two" "$both_home" 'undo=None' 'redo=Some\("drag"\)'
check multi-cancel    'click-row=11,shift-click-row=12,hold-gizmo=-60,key=escape,release' "$both_home" 'undo=None' 'dragging=false'
# A number typed into a field they share goes to both, as one undo step.
check multi-type      'click-row=11,shift-click-row=12,inspector,type-field=translation.2:-6' \
                      'places=\[\[0\.0, 0\.5, -6\.0\], \[3\.0, 0\.15, -6\.0\]\]' 'undo=Some\("edit"\)' 'problem=none'
check multi-type-undo 'click-row=11,shift-click-row=12,inspector,type-field=translation.2:-6,key=ctrl+z' "$both_home" 'undo=None'
# Remove takes all of them, and one undo step brings all of them back.
check multi-remove    'click-row=11,shift-click-row=12,key=delete' 'objects=20' 'selection=0' 'undo=Some\("delete"\)'
check multi-remove-undo 'click-row=11,shift-click-row=12,key=delete,key=ctrl+z' 'objects=22' 'undo=None'

# Snapping. Off until its switch in the toolbar is pressed, and off again with the next press.
check snap-off        'click-row=11,drag-gizmo=-60'       'snap=false' "$moved"
check snap-on         'click-button=Snap'                 'snap=true' "$row" 'toolbar_fits=Some\(true\)'
check snap-on-off     'click-button=Snap,click-button=Snap' 'snap=false'
# A box that stands on no grid line is put on one along the axis it is dragged on, and stays
# where it was along the others.
on_grid='position=Some\(\[-[0-9]+\.[05], 0\.5, -8\.0\]\)'
check snap-move       'click-row=11,inspector,type-field=translation.0:0.2,click-button=Snap,drag-gizmo=-60' \
                      "$on_grid" 'undo=Some\("drag"\)' 'problem=none'
check snap-move-undo  'click-row=11,inspector,type-field=translation.0:0.2,click-button=Snap,drag-gizmo=-60,key=ctrl+z' \
                      'position=Some\(\[0\.2, 0\.5, -8\.0\]\)' 'redo=Some\("drag"\)'
# With a second object selected, the held one goes to the grid and the other moves by exactly
# as much. The Wall is put 8.3 m from the Tall box along X, on no grid line. A Shift-click on
# the Tall box adds it and puts the gizmo there; one more on the Wall puts the gizmo on the
# Wall again. After the drag the Wall is on the grid, and the Tall box is still 8.3 m from it,
# so it has left the grid by as much as the Wall came onto it. One undo step puts both back.
SG='click-row=10,inspector,type-field=translation.0:-8.3,shift-click-object=11,shift-click-object=10,click-button=Snap,drag-gizmo=-60'
check snap-group      "$SG" \
                      "$two" 'selected=Some\("Wall"\)' 'active=Some\("Wall"\)' 'position=Some\(\[-[0-9]+\.[05], 3\.0, 0\.0\]\)' \
                      'offsets=\[\[0\.0, 0\.0, 0\.0\], \[8\.3, -2\.5, -8\.0\]\]' 'undo=Some\("drag"\)'
check snap-group-undo "$SG,key=ctrl+z" \
                      "$two" 'places=\[\[-8\.3, 3\.0, 0\.0\], \[0\.0, 0\.5, -8\.0\]\]' 'redo=Some\("drag"\)'
# Left on the Tall box, which was clicked last, the gizmo puts that one on the grid, where it
# was already: the Wall goes by as many whole steps and stays 8.3 m off.
check snap-group-other 'click-row=10,inspector,type-field=translation.0:-8.3,shift-click-object=11,click-button=Snap,drag-gizmo=-60' \
                      "$two" 'active=Some\("Tall box"\)' 'places=\[\[-[0-9]+\.[38], 3\.0, 0\.0\], \[-[0-9]+\.[05], 0\.5, -8\.0\]\]' \
                      'offsets=\[\[0\.0, 0\.0, 0\.0\], \[8\.3, -2\.5, -8\.0\]\]' 'undo=Some\("drag"\)'
# A turn goes in steps of fifteen degrees.
check snap-rotate     'click-button=Snap,click-row=11,key=e,rotate-gizmo=60' 'tool=Rotate' "$home" \
                      'rotation=Some\(\[0\.0, -?(15|30|45|60|75|90)\.0, 0\.0\]\)' 'undo=Some\("drag"\)'
# A scaled box gets a width of whole half metres: its half size along X is a quarter metre or
# a multiple of it.
check snap-scale      'click-button=Snap,click-row=11,key=r,drag-gizmo=-30' 'tool=Scale' "$home" \
                      'half_size=Some\(\[[0-9]+\.(0|25|5|75), 0\.5, 0\.5\]\)' 'undo=Some\("drag"\)'
# At 1261 pixels the switch is a button beside the tools and the row still holds everything; at
# 621 it is a row of the tool menu. Several objects are chosen and moved the same at both.
check_sized "$wide" wide-snap     'click-button=Snap'     'window=1261x' 'toolbar_folded=false' 'snap=true' "$row" 'toolbar_fits=Some\(true\)'
check_sized "$wide" wide-multi    'click-row=11,shift-click-row=12,drag-gizmo=-60' 'window=1261x' "$two" "$moved" "$apart" "$row" 'toolbar_fits=Some\(true\)'
check_sized "$narrow" narrow-snap 'choose=Tool>Snap'      'window=621x' 'toolbar_folded=true' 'snap=true' 'menu_open=false' "$row" 'toolbar_fits=Some\(true\)'
# In the narrow window the Low box's X handle reaches past the view's edge, so the gizmo is
# put on the Tall box, in the middle of the view, by clicking that one last.
check_sized "$narrow" narrow-multi 'click-row=12,shift-click-row=11,drag-gizmo=-60' 'window=621x' 'toolbar_folded=true' "$two" 'active=Some\("Tall box"\)' "$moved" "$apart" 'undo=Some\("drag"\)' "$row" 'toolbar_fits=Some\(true\)'

# The Play control. It is in the toolbar at every width. Its foot names the cooked level and
# offers to cook; a cooked level can be checked on disk by the editor's core. Started without
# a game, as in these cases, the ways to play are off and choosing one does nothing.
check play-menu       'click-button=Play'                 'menu_open=true' 'play_cooked=none' 'play_check=none' 'play_stop=false' "$row"
check play-off        'click-button=Play,click-item=Open Game Window,click-item=Preview in Editor' 'menu_open=true' 'play_check=none' 'play_stop=false' 'problem=none'
check play-no-check   'click-button=Play,click-item=Check cooked level' 'menu_open=true' 'play_cooked=none' 'play_check=none' 'problem=none'
# The foot's button cooks, and the menu stays open on the new cooked level.
check play-cooks      'click-button=Play,click-button=Cook' 'menu_open=true' 'play_cooked=[0-9a-f]{8}' 'play_older=false' 'cook_needed=false' 'problem=none'
# A check of the cooked level passes, and is asked for from the menu.
check play-check      'cook,click-button=Play,click-item=Check cooked level' 'menu_open=false' 'play_cooked=[0-9a-f]{8}' 'play_check=passed' 'play_stop=false' 'problem=none' 'notice=false'
# Something else wrote into the cooked level: the core refuses it and the notice says so.
check play-refused    'cook,spoil-cooked,click-button=Play,click-item=Check cooked level' 'play_check=refused:payload\.hash_mismatch' 'problem=payload\.hash_mismatch' 'notice=true' 'play_stop=false'
# The level changes and is cooked first: that is a new cooked level, and it passes. (Cooking the
# same level again would not help: the core leaves a cooked folder that is there as it is.)
check play-recooked   'cook,spoil-cooked,click-button=Play,click-item=Check cooked level,click-row=11,drag-gizmo=-60,click-button=Play,click-button=Cook first,click-item=Check cooked level' 'play_check=passed' 'play_older=false' 'problem=none' 'notice=false'
# The level moves on after a cook: the control warns, the foot offers to cook first, and the
# older cook can still be checked.
check play-older      'cook,click-row=11,drag-gizmo=-60,click-button=Play' 'play_older=true' 'cook_needed=true' 'menu_open=true'
check play-cook-first 'cook,click-row=11,drag-gizmo=-60,click-button=Play,click-button=Cook first' 'play_older=false' 'cook_needed=false' 'menu_open=true' 'problem=none'
check play-older-check 'cook,click-row=11,drag-gizmo=-60,click-button=Play,click-item=Check cooked level' 'play_older=true' 'play_check=passed' 'problem=none'
# At both accepted widths the control is in the row, and its menu opens inside the window.
check_sized "$wide" wide-play     'click-button=Play'     'window=1261x' 'menu_open=true' "$row" 'toolbar_fits=Some\(true\)'
check_sized "$narrow" narrow-play 'click-button=Play'     'window=621x' 'menu_open=true' 'toolbar_folded=true' "$row" 'toolbar_fits=Some\(true\)'
check_sized "$narrow" narrow-play-check 'cook,click-button=Play,click-item=Check cooked level' 'window=621x' 'play_check=passed' 'problem=none'

# Of several selected objects the gizmo is on the one clicked last, and its row carries the
# dot. A Shift-click on another selected one moves the gizmo there; with one object selected
# no row carries a dot.
tall='gizmo_at=Some\(\[0\.0, 0\.5, -8\.0\]\)'
low='gizmo_at=Some\(\[3\.0, 0\.15, -8\.0\]\)'
check active-last     'click-row=11,shift-click-row=12'   "$two" 'active=Some\("Low box"\)' 'gizmo_row=Some\("Low box"\)' "$low" 'pivot=Active'
check active-moved    'click-row=11,shift-click-row=12,shift-click-row=11' "$two" 'active=Some\("Tall box"\)' 'gizmo_row=Some\("Tall box"\)' "$tall" 'undo=None'
check active-one      'click-row=11'                      'active=Some\("Tall box"\)' 'gizmo_row=None' "$tall"
check active-view     'click-object=10,shift-click-object=11,shift-click-object=10' "$two" 'active=Some\("Wall"\)' 'gizmo_row=Some\("Wall"\)' 'gizmo_at=Some\(\[-8\.5, 3\.0, 0\.0\]\)'
check active-left     'click-row=11,shift-click-row=12,shift-click-row=12' 'selection=1' 'active=Some\("Tall box"\)' 'gizmo_row=None' "$tall"
# The inspector's two buttons put the gizmo on one object or at the middle of them all: the
# mean of where they are. It is a setting: no undo step.
M='click-row=11,shift-click-row=12,click-button=Their middle'
middle='gizmo_at=Some\(\[1\.5, 0\.325, -8\.0\]\)'
check pivot-middle    "$M"                                "$two" 'pivot=Center' "$middle" 'gizmo_row=None' 'undo=None' 'problem=none'
check pivot-back      "$M,click-button=One object"        "$two" 'pivot=Active' "$low" 'gizmo_row=Some\("Low box"\)'
# It stays chosen for the next selection, and one object has no middle but itself.
check pivot-kept      "$M,click-row=13,shift-click-row=11" 'pivot=Center' 'gizmo_at=Some\(\[-1\.5, 1\.0, -10\.0\]\)' 'gizmo_row=None'
check pivot-one       "$M,click-row=13"                   'selection=1' 'pivot=Center' 'gizmo_at=Some\(\[-3\.0, 1\.5, -12\.0\]\)'
# A drag at the middle moves both alike, as one undo step.
check pivot-move      "$M,drag-gizmo=-60"                 "$two" "$moved" "$apart" 'undo=Some\("drag"\)' 'dragging=false' 'gizmo_at=Some\(\[-?[0-9]+\.[0-9]+, 0\.325, -8\.0\]\)'
check pivot-move-undo "$M,drag-gizmo=-60,key=ctrl+z"      "$two" "$both_home" "$middle" 'redo=Some\("drag"\)'
# A turn at the middle is about the middle: both objects are turned and are elsewhere, and
# their middle is where it was. About one of them it would not be.
check pivot-turn      "$M,key=e,rotate-gizmo=60"          "$two" 'tool=Rotate' "$middle" 'turns=\[\[0\.0, -?[1-9][0-9]*\.[0-9], 0\.0\], \[0\.0, -?[1-9][0-9]*\.[0-9], 0\.0\]\]' 'undo=Some\("drag"\)'
check pivot-turn-one  'click-row=11,shift-click-row=12,key=e,rotate-gizmo=60' "$two" 'tool=Rotate' "$low" 'turns=\[\[0\.0, -?[1-9][0-9]*\.[0-9], 0\.0\], \[0\.0, -?[1-9][0-9]*\.[0-9], 0\.0\]\]'
# With snapping, the middle goes by whole steps: a box that stood 0.2 m off the grid stands
# 0.2 m off it afterwards, and the box that was on the grid is on it.
check pivot-snap      'click-row=11,type-field=translation.0:0.2,shift-click-row=12,click-button=Their middle,click-button=Snap,drag-gizmo=-60' \
                      "$two" 'places=\[\[-?[0-9]+\.[2378], 0\.5, -8\.0\], \[-?[0-9]+\.[05], 0\.15, -8\.0\]\]' \
                      'offsets=\[\[0\.0, 0\.0, 0\.0\], \[2\.8, -0\.35, 0\.0\]\]' 'undo=Some\("drag"\)'
# A dash is a field: a number typed over it goes to every selected object, which then share
# it, as one undo step. The other numbers of each stay its own.
check dash-type       'click-row=11,shift-click-row=12,type-field=translation.0:5' \
                      'places=\[\[5\.0, 0\.5, -8\.0\], \[5\.0, 0\.15, -8\.0\]\]' 'dashes=Some\(\[false, true, false\]\)' 'undo=Some\("edit"\)' 'problem=none'
check dash-type-undo  'click-row=11,shift-click-row=12,type-field=translation.0:5,key=ctrl+z' "$both_home" 'dashes=Some\(\[true, true, false\]\)' 'undo=None'
check dash-type-two   'click-row=11,shift-click-row=12,type-field=translation.0:5,type-field=translation.1:1' \
                      'places=\[\[5\.0, 1\.0, -8\.0\], \[5\.0, 1\.0, -8\.0\]\]' 'dashes=Some\(\[false, false, false\]\)'
# Nothing typed over a dash changes nothing.
check dash-empty      'click-row=11,shift-click-row=12,type-field=translation.0:' "$both_home" 'dashes=Some\(\[true, true, false\]\)' 'undo=None' 'problem=none'
# A size: the boxes differ in height, and get one. A turn: one box is tilted, and both get
# the same tilt.
check dash-size       'click-row=11,shift-click-row=12,type-field=half_extents.1:1' \
                      'sizes=\[\[0\.5, 1\.0, 0\.5\], \[[0-9.]+, 1\.0, [0-9.]+\]\]' 'undo=Some\("edit"\)' 'problem=none'
check dash-turn       'tilt=12:30,click-row=11,shift-click-row=12,type-field=rotation.0:10' \
                      'turns=\[\[10\.0, 0\.0, 0\.0\], \[10\.0, 0\.0, 0\.0\]\]' 'undo=Some\("edit"\)' 'problem=none'
# The same in a wide and in a narrow window. In the narrow one the inspector takes the
# hierarchy's place once the two are selected.
check_sized "$wide" wide-pivot   "$M"                     'window=1261x' 'pivot=Center' "$middle" "$row" 'toolbar_fits=Some\(true\)'
check_sized "$wide" wide-active  'click-row=11,shift-click-row=12,shift-click-row=11' 'window=1261x' 'active=Some\("Tall box"\)' "$tall" "$row" 'toolbar_fits=Some\(true\)'
check_sized "$wide" wide-dash    'click-row=11,shift-click-row=12,type-field=translation.0:5' 'window=1261x' 'places=\[\[5\.0, 0\.5, -8\.0\], \[5\.0, 0\.15, -8\.0\]\]' 'problem=none'
check_sized "$narrow" narrow-pivot 'click-row=11,shift-click-row=12,inspector,click-button=Their middle' 'window=621x' 'pivot=Center' "$middle" "$row" 'toolbar_fits=Some\(true\)'
check_sized "$narrow" narrow-active 'click-row=11,shift-click-row=12,shift-click-row=11' 'window=621x' 'active=Some\("Tall box"\)' 'gizmo_row=Some\("Tall box"\)' "$tall" "$row" 'toolbar_fits=Some\(true\)'
check_sized "$narrow" narrow-dash 'click-row=11,shift-click-row=12,inspector,type-field=translation.0:5' 'window=621x' 'places=\[\[5\.0, 0\.5, -8\.0\], \[5\.0, 0\.15, -8\.0\]\]' 'dashes=Some\(\[false, true, false\]\)' 'problem=none'

# A row of the hierarchy is dragged to put its object under another. Dropped on a row it goes
# under that row's object and stays where it is in the level: the Tall box under the Wall has
# another position under it, and the same place. One undo step.
check drag-under      'click-row=11,drag-row=11:10'       'selected=Some\("Tall box"\)' 'under=\["Wall"\]' 'worlds=\[\[0\.0, 0\.5, -8\.0\]\]' 'position=Some\(\[8\.5, -2\.5, -8\.0\]\)' 'kids=\["Tall box"\]' 'undo=Some\("reparent"\)' 'rows_shown=22' 'drop_target=None' 'problem=none'
check drag-under-undo 'click-row=11,drag-row=11:10,key=ctrl+z' 'under=\["top"\]' "$home" 'redo=Some\("reparent"\)' 'problem=none'
# A row that is not selected goes alone and leaves the selection as it is.
check drag-unselected 'click-row=13,drag-row=11:10'       'selection=1' 'selected=Some\("Pillar"\)' 'under=\["top"\]' 'undo=Some\("reparent"\)' 'problem=none'
# Where it went is read by selecting it directly: a click played the moment the drop is over
# would be aimed at a list that is being built anew.
check drag-unselected-went 'click-row=13,drag-row=11:10,select=11' 'selected=Some\("Tall box"\)' 'under=\["Wall"\]' 'worlds=\[\[0\.0, 0\.5, -8\.0\]\]' 'position=Some\(\[8\.5, -2\.5, -8\.0\]\)'
# A selected row takes the other selected rows with it, in the list's order whichever was
# clicked or dragged, as one undo step.
both_under='under=\["Wall", "Wall"\]'
both_placed='worlds=\[\[0\.0, 0\.5, -8\.0\], \[3\.0, 0\.15, -8\.0\]\]'
check drag-two        'click-row=11,shift-click-row=12,drag-row=12:10' "$two" "$both_under" "$both_placed" 'kids=\["Tall box", "Low box"\]' 'undo=Some\("reparent"\)' 'problem=none'
check drag-two-other  'click-row=12,shift-click-row=11,drag-row=11:10' "$two" "$both_under" "$both_placed" 'kids=\["Tall box", "Low box"\]'
check drag-two-undo   'click-row=11,shift-click-row=12,drag-row=12:10,key=ctrl+z' "$two" 'under=\["top", "top"\]' "$both_home" 'undo=None' 'redo=Some\("reparent"\)'
# Dropped on the list where no row is, a row goes to the top of the level and stays where it
# is in the level. The Tall box is first put under the Wall as it is, which moves it.
check drag-top        'reparent=11:10,click-row=11,drag-row=11:top' 'under=\["top"\]' 'worlds=\[\[-8\.5, 3\.5, -8\.0\]\]' 'position=Some\(\[-8\.5, 3\.5, -8\.0\]\)' 'undo=Some\("reparent"\)' 'problem=none'
# A row dropped on itself, and a row at the top dropped on the list, change nothing.
check drag-self       'click-row=11,drag-row=11:11'       'under=\["top"\]' "$home" 'undo=None' 'problem=none'
check drag-top-stays  'click-row=11,drag-row=11:top'      'under=\["top"\]' "$home" 'undo=None' 'problem=none'
# An object cannot go under what is under it: the core refuses, in a notice, and nothing moves.
check drag-inside     'reparent=11:10,click-row=10,drag-row=10:11' 'selected=Some\("Wall"\)' 'under=\["top"\]' 'problem=hierarchy\.cycle' 'notice=true' 'worlds=\[\[-8\.5, 3\.0, 0\.0\]\]'
# While a row is dragged, the place a drop would move it to is ringed: a row, or the list for
# a row that is under something. Where a drop would change nothing, nothing is ringed.
check drag-ring-row   'click-row=11,hold-row=11:10'       'drop_target=Some\(Row\(' 'under=\["top"\]' 'undo=None'
check drag-ring-top   'reparent=11:10,click-row=11,hold-row=11:top' 'drop_target=Some\(Top\)' 'under=\["Wall"\]'
check drag-ring-none  'click-row=11,hold-row=11:top'      'drop_target=None' 'under=\["top"\]'
check drag-ring-drop  'click-row=11,hold-row=11:10,release' 'drop_target=None' 'under=\["Wall"\]' 'undo=Some\("reparent"\)'
# A row dropped on a folded row unfolds it, so that what was dropped is seen.
check drag-unfolds    'reparent=11:10,click-toggle=10,drag-row=12:10' 'folded=0' 'rows_shown=22' 'problem=none'
check drag-folded     'reparent=11:10,click-toggle=10'    'folded=1' 'rows_shown=21'
# The same in a wide and in a narrow window.
check_sized "$wide" wide-drag     'click-row=11,drag-row=11:10' 'window=1261x' 'under=\["Wall"\]' 'worlds=\[\[0\.0, 0\.5, -8\.0\]\]' "$row" 'toolbar_fits=Some\(true\)'
check_sized "$narrow" narrow-drag 'click-row=11,drag-row=11:10' 'window=621x' 'under=\["Wall"\]' 'worlds=\[\[0\.0, 0\.5, -8\.0\]\]' "$row" 'toolbar_fits=Some\(true\)'
check_sized "$narrow" narrow-drag-top 'reparent=11:10,click-row=11,drag-row=11:top' 'window=621x' 'under=\["top"\]' 'worlds=\[\[-8\.5, 3\.5, -8\.0\]\]'

# Review is a tool: its panel, in the inspector's place, lists what the core's review finds.
# The game's map has nothing to review.
EMPTY='key=ctrl+n,type=7,key=enter,click-button=Discard changes'
RULE='choose=Add>Landscape,inspector,choose=Tool>Rules,click-button=Add grass'
check review-clean    'choose=Tool>Review'                'workspace=Review' 'review=\[\]' 'review_rows=0' "$row" 'toolbar_fits=Some\(true\)' 'problem=none'
# A level without a spawn point: one problem under Play, about no object. With a spawn point
# added the list is empty again.
check review-no-spawn "$EMPTY,choose=Tool>Review"         'objects=0' 'workspace=Review' 'review=\["Play:play\.spawn_required"\]' 'review_rows=1' 'problem=none'
check review-spawn-added "$EMPTY,choose=Tool>Review,choose=Add>Spawn point" 'objects=1' 'review=\[\]' 'review_rows=0' 'workspace=Review'
# A landscape with a rule: one problem under Play, about the landscape, whose button selects
# it. Nothing was selected before.
check review-rule     "$RULE,choose=Tool>Review"          'workspace=Review' 'review=\["Play:play\.terrain_unsupported"\]' 'review_rows=1' 'selection=0'
check review-rule-select "$RULE,choose=Tool>Review,click-button=Select Landscape" 'selection=1' 'selected=Some\("Landscape"\)' 'workspace=Review' 'review_rows=1' 'problem=none'
# Both at once, in the core's order: the landscape's rules first, then the missing spawn point.
check review-both     "$EMPTY,$RULE,choose=Tool>Review"   'review=\["Play:play\.terrain_unsupported", "Play:play\.spawn_required"\]' 'review_rows=2'
# The one key on such a level cooks nothing, says so, and chooses the Review tool; with the
# level mended it plays.
with_game runs check review-key     "$RULE,key=f5"        'play_run=None' 'play_refusal=play\.blocked' 'play_cooked=none' 'workspace=Review' 'review_rows=1'
with_game runs check review-key-menu "$RULE,click-button=Play,click-item=Cook and Play" 'play_run=None' 'play_refusal=play\.blocked' 'workspace=Review' 'menu_open=false'
with_game runs check review-key-mended "$EMPTY,key=f5,choose=Add>Spawn point,key=f5,wait-run=running" 'play_run=Running' 'play_refusal=none' 'review=\[\]' 'workspace=Review'
# Another tool brings the inspector back, and Review's panel goes.
check review-left     "$RULE,choose=Tool>Review,key=w"    'workspace=Objects' 'tool=Translate' 'review_rows=0' 'review=\["Play:play\.terrain_unsupported"\]'
# At 1261 pixels Review is one more button of the row. The row with every tool as a button is
# longer than 900 pixels, the narrowest window that has the panel switches as buttons: there,
# and at 1000 and 1100, the tools are in their menu, which the row holds, and the panel
# switches are buttons still; at 1150 the whole row fits, with the Particles tool. At 621 Review is a row of the tool menu as well, and the panel shows when
# the right-hand place is shown.
check_sized "$wide" wide-review   "$RULE,choose=Tool>Review" 'window=1261x' 'toolbar_folded=false' 'workspace=Review' 'review_rows=1' "$row" 'toolbar_fits=Some\(true\)'
check_sized 900x700 fold-review   'choose=Tool>Review'    'window=900x' 'toolbar_folded=true' 'workspace=Review' "$row" 'toolbar_fits=Some\(true\)'
check_sized 900x700 fold-panels   'click-button=Inspector,choose=Tool>Rotate' 'window=900x' 'toolbar_folded=true' 'tool=Rotate' "$row" 'toolbar_fits=Some\(true\)'
check_sized 1000x700 fold-tools   'choose=Tool>Review'    'window=1000x' 'toolbar_folded=true' 'workspace=Review' "$row" 'toolbar_fits=Some\(true\)'
check_sized 1150x700 fold-not     'click-button=Review'   'window=1150x' 'toolbar_folded=false' 'workspace=Review' "$row" 'toolbar_fits=Some\(true\)'
with_game runs check_sized 1100x700 fold-game 'cook,click-button=Play,click-item=Open Game Window,wait-run=running' 'window=1100x' 'play_run=Running' "$row" 'toolbar_fits=Some\(true\)'
check_sized "$narrow" narrow-review "$RULE,choose=Tool>Review" 'window=621x' 'toolbar_folded=true' 'workspace=Review' 'review_rows=1' "$row" 'toolbar_fits=Some\(true\)'
check_sized "$narrow" narrow-review-select "$RULE,choose=Tool>Review,click-button=Select Landscape" 'window=621x' 'selected=Some\("Landscape"\)' 'selection=1'

# The 3D view's corner has one control: how the level is drawn. "Editor" is the editor's own
# picture and the default. "Game look" is drawn from the game's look profile; a check names a
# file in the profile's place (with_look), and without one, with one that is not there, or with
# one that is no profile of this schema, the row is off and the view stays the editor's.
look="$(cd "$(dirname "$0")" && pwd)/look-v1.json"
with_look() {
    ARC_EDITOR_CHECK_LOOK="$1"; export ARC_EDITOR_CHECK_LOOK
    shift
    "$@"
    unset ARC_EDITOR_CHECK_LOOK
}
printf '{"schema": 2}' > "$work/look-v2.json"
check view-default    'click-row=11'                      'view=Editor' 'look=none' 'look_sun=None' 'ground_look=None' 'selected=Some\("Tall box"\)'
check view-no-game    'click-button=View: Editor,click-item=Game look' 'view=Editor' 'look=none' 'look_sun=None' 'menu_open=true' 'problem=none'
with_look "$work/absent.json" check view-missing 'click-button=View: Editor,click-item=Game look' 'view=Editor' 'look=refused:look\.missing' 'look_sun=None' 'menu_open=true' 'problem=none'
with_look "$work/look-v2.json" check view-other-schema 'click-button=View: Editor,click-item=Game look' 'view=Editor' 'look=refused:look\.schema' 'look_sun=None' 'menu_open=true'
# With the profile the row is on. Game look puts the profile's sun in the view and leaves the
# level, the selection and what can be undone as they were; Editor takes the sun out again.
with_look "$look" check view-game-look 'click-row=11,choose=View>Game look' 'view=GameLook' 'look=loaded:3abf9a2f' 'look_sun=Some\(130000' 'ground_look=Some\(\(\[1\.0, 1\.0, 1\.0, 1\.0\], 0\.9\)\)' 'selected=Some\("Tall box"\)' 'selection=1' 'undo=None' 'menu_open=false' 'problem=none' "$row" 'toolbar_fits=Some\(true\)'
# The view's menu opens again after it was closed, however it was closed: by choosing a row,
# by Escape, and by a click beside it, which closes the menu and chooses nothing.
with_look "$look" check view-reopen    'click-button=View: Editor,key=escape,click-button=View: Editor' 'menu_open=true' 'view=Editor' 'problem=none'
with_look "$look" check view-outside   'click-button=View: Editor,click-object=11,click-button=View: Editor' 'menu_open=true' 'selected=None' 'view=Editor'
with_look "$look" check view-thrice    'choose=View>Game look,choose=View>Editor,choose=View>Game look' 'view=GameLook' 'look_sun=Some\(130000' 'menu_open=false' 'problem=none'
# The Particles tool shows its stage in the editor's own picture and gives the level its
# Game look back afterwards.
with_look "$look" check view-game-stage 'choose=View>Game look,choose=Tool>Particles' 'workspace=Particles' 'view=Editor' 'stage=\(true, 0\)' 'look_sun=None' 'problem=none'
with_look "$look" check view-game-stage-back 'choose=View>Game look,choose=Tool>Particles,choose=Tool>Move' 'workspace=Objects' 'view=GameLook' 'stage=\(false, 0\)' 'look_sun=Some\(130000' 'problem=none'
with_look "$look" check view-back      'choose=View>Game look,choose=View>Editor' 'view=Editor' 'look=loaded:3abf9a2f' 'look_sun=None' 'ground_look=None' 'undo=None'
with_look "$look" check view-game-move 'click-row=11,choose=View>Game look,drag-gizmo=-60' 'view=GameLook' "$moved" 'undo=Some\("drag"\)' 'dragging=false'
with_look "$look" check_sized "$narrow" narrow-view 'choose=View>Game look' 'window=621x' 'view=GameLook' 'look_sun=Some\(130000' "$row" 'toolbar_fits=Some\(true\)'

# Duplicate copies the selection: with Ctrl+D and from the Add menu. The copy stands a metre
# beside what it was copied from, is called after it, is what is selected afterwards, and is
# one undo step. What is under an object is copied with it.
dup='selected=Some\("Tall box copy"\)'
check dup-key         'click-row=11,key=ctrl+d'           'objects=23' "$dup" 'position=Some\(\[1\.0, 0\.5, -8\.0\]\)' 'selection=1' 'undo=Some\("duplicate"\)' 'problem=none'
check dup-menu        'click-row=11,choose=Add>Duplicate' 'objects=23' "$dup" 'position=Some\(\[1\.0, 0\.5, -8\.0\]\)' 'menu_open=false' 'problem=none'
check dup-undo        'click-row=11,key=ctrl+d,key=ctrl+z' 'objects=22' 'redo=Some\("duplicate"\)' 'problem=none'
check dup-twice       'click-row=11,key=ctrl+d,key=ctrl+d' 'objects=24' 'selected=Some\("Tall box copy copy"\)' 'position=Some\(\[2\.0, 0\.5, -8\.0\]\)'
check dup-two         'click-row=10,shift-click-row=11,key=ctrl+d' 'objects=24' 'selection=2' 'duplicate=Offered' 'problem=none'
check dup-children    'reparent=11:10,click-row=10,key=ctrl+d' 'objects=24' 'selected=Some\("Wall copy"\)' 'selection=1' 'problem=none'
# Nothing selected: the row is off and says why, and the key does nothing.
check dup-nothing     'key=ctrl+d'                        'objects=22' 'duplicate=NothingSelected' 'problem=none' 'undo=None'
check dup-nothing-row 'click-button=Add,click-item=Duplicate' 'objects=22' 'menu_open=true' 'problem=none'
# A landscape is not copied. With the new landscape selected (it is the last row), the row is
# off, and the key says why in a notice; nothing is added either way.
check dup-landscape   'choose=Add>Landscape,click-row=22,key=ctrl+d' 'selected=Some\("Landscape"\)' 'objects=23' 'duplicate=Landscape' 'problem=duplicate\.terrain_unsupported' 'notice=true' 'undo=Some\("create"\)'
check dup-landscape-row 'choose=Add>Landscape,click-row=22,click-button=Add,click-item=Duplicate' 'selected=Some\("Landscape"\)' 'objects=23' 'duplicate=Landscape' 'menu_open=true' 'problem=none' 'undo=Some\("create"\)'
# With a box selected beside it, the selection still holds a landscape: still off.
check dup-landscape-and-box 'choose=Add>Landscape,click-row=22,shift-click-row=11,key=ctrl+d' 'selection=2' 'objects=23' 'duplicate=Landscape' 'problem=duplicate\.terrain_unsupported'
check_sized "$narrow" narrow-dup 'click-row=11,choose=Add>Duplicate' 'window=621x' 'objects=23' "$dup" "$row" 'toolbar_fits=Some\(true\)'
# The steps of snapping are two number fields at the foot of the inspector. The core keeps
# them; the 3D view snaps by them; a step the core does not take is refused in words.
check snap-steps      'inspector,type-field=snap.0:0.25,type-field=snap.1:45' 'snap_steps=\(0\.25, 45\)' 'snap=false' 'problem=none' 'undo=None'
check snap-steps-on   'inspector,type-field=snap.0:0.25,click-button=Snap' 'snap_steps=\(0\.25, 15\)' 'snap=true' 'problem=none'
check snap-step-move  'click-row=11,inspector,type-field=translation.0:0.1,type-field=snap.0:4,click-button=Snap,drag-gizmo=-60' 'position=Some\(\[(-4\.0|0\.0), 0\.5, -8\.0\]\)' 'snap=true' 'dragging=false'
check snap-step-refused 'inspector,type-field=snap.0:0' 'snap_steps=\(0\.5, 15\)' 'problem=snap\.range' 'notice=true'
check snap-turn-refused 'inspector,type-field=snap.1:120' 'snap_steps=\(0\.5, 15\)' 'problem=snap\.range' 'notice=true'

# Playing, with a scripted stand-in for the game. "Open Game Window" checks the cooked level
# and starts the game from that check; the control shows it running only once it is ready.
PLAY='cook,click-button=Play,click-item=Open Game Window'
check play-no-game    "$PLAY"                                'game=false' 'play_run=None' 'play_check=none' 'menu_open=true' 'problem=none'
with_game runs check game-runs "$PLAY,wait-run=running" 'game=true' 'play_run=Running' 'play_check=passed' 'play_stop=true' 'menu_open=false' 'run_notice=false' 'problem=none' "$row" 'toolbar_fits=Some\(true\)'
# Stop ends it, and that is what was asked for: no notice.
with_game runs check game-stops "$PLAY,wait-run=running,click-button=Stop,wait-run=none" 'play_run=None' 'play_end=play\.stopped' 'play_stop=false' 'run_notice=false' 'left_notice=false'
# A game that never becomes ready stays "starting", and Stop cancels the start.
with_game hangs check game-starting "$PLAY,wait-run=starting" 'play_run=Starting' 'play_stop=true' 'play_end=none'
with_game hangs check game-start-stopped "$PLAY,wait-run=starting,click-button=Stop,wait-run=none" 'play_run=None' 'play_end=play\.stopped' 'run_notice=false'
# One key does all a change needs to be played. F5 on a level that was never cooked cooks it,
# checks the cooked level and plays it; the Play menu's row "Cook and Play" is the same. With
# a current cooked level nothing is cooked again: the cook's source hash stays.
with_game runs check key-play       'key=f5,wait-run=running' 'play_run=Running' 'play_cooked=[0-9a-f]{8}' 'play_older=false' 'play_check=passed' 'cook_needed=false' 'play_stop=true' 'problem=none' 'play_refusal=none'
with_game runs check key-play-menu  'click-button=Play,click-item=Cook and Play,wait-run=running' 'play_run=Running' 'play_check=passed' 'menu_open=false' 'problem=none'
with_game runs check key-play-current 'cook,key=f5,wait-run=running' 'play_run=Running' 'play_older=false' 'problem=none'
same key-play key-play-current cooked cooked_source
# A level that has changed since its cook is cooked again first, and that is what runs.
with_game runs check key-play-changed 'cook,click-row=11,drag-gizmo=-60,key=f5,wait-run=running' 'play_run=Running' 'play_older=false' 'cook_needed=false' 'moved_notice=false' 'problem=none'
differ key-play key-play-changed cooked_source
# Shift+F5 is Stop: of a game that runs, and of one that is still starting.
with_game runs check key-stop       'key=f5,wait-run=running,key=shift+f5,wait-run=none' 'play_run=None' 'play_end=play\.stopped' 'play_stop=false' 'run_notice=false'
with_game hangs check key-stop-start 'key=f5,wait-run=starting,key=shift+f5,wait-run=none' 'play_run=None' 'play_end=play\.stopped' 'run_notice=false'
# The key says why it does nothing, in the status bar, and cooks nothing then: without a
# game, with a game that runs already, and with a level that has no spawn point.
check key-no-game     'key=f5'                              'game=false' 'play_run=None' 'play_refusal=play\.no_game' 'play_cooked=none' 'problem=none'
with_game runs check key-twice      'key=f5,wait-run=running,key=f5' 'play_run=Running' 'play_refusal=play\.already_active' 'problem=none'
with_game runs check key-blocked    'key=ctrl+n,type=7,key=enter,click-button=Discard changes,key=f5' 'objects=0' 'play_run=None' 'play_refusal=play\.blocked' 'play_cooked=none' 'problem=none' 'workspace=Review' 'review=\["Play:play\.spawn_required"\]' 'review_rows=1'
# The keys act while a field has the keyboard, and not behind a menu.
with_game runs check key-in-field   'click-row=11,inspector,type-field=translation.0:2,key=f5,wait-run=running' 'play_run=Running' 'position=Some\(\[2\.0, 0\.5, -8\.0\]\)'
with_game runs check key-behind-menu 'click-button=Play,key=f5' 'menu_open=true' 'play_run=None' 'play_cooked=none'

# The level moves on while the game runs: one notice offers to cook and play again, and that
# stops the run, cooks the level as it is now and plays it. The run that was stopped for it
# is no notice.
MOVED="$PLAY,wait-run=running,click-row=11,drag-gizmo=-60"
with_game runs check game-moved-on  "$MOVED" 'play_run=Running' 'cook_needed=true' 'moved_notice=true' 'notice_actions=\["Cook and play again"\]' 'run_notice=false'
with_game runs check game-moved-dismissed "$MOVED,click-button=Dismiss" 'play_run=Running' 'moved_notice=false' 'cook_needed=true'
with_game runs check game-replayed  "$MOVED,click-button=Cook and play again,wait-run=running" 'play_run=Running' 'play_older=false' 'cook_needed=false' 'moved_notice=false' 'run_notice=false' 'problem=none'
differ game-moved-on game-replayed cooked_source
# A game that stopped by itself offers its log and to play again.
with_game crashes check game-crash-offers "$PLAY,wait-run=none" 'play_end=play\.crashed' 'run_notice=true' 'notice_actions=\["Play again", "Open log"\]' 'opened=none'
with_game crashes check game-open-log "$PLAY,wait-run=none,click-button=Open log" 'run_notice=true' 'opened=client\.log' 'problem=none'
ARC_EDITOR_CHECK_OPEN=missing; export ARC_EDITOR_CHECK_OPEN
with_game crashes check game-log-gone "$PLAY,wait-run=none,click-button=Open log" 'run_notice=true' 'opened=none' 'problem=path\.missing' 'notice=true'
unset ARC_EDITOR_CHECK_OPEN
# A run that ended as asked offers nothing: there is no notice.
with_game hangs check game-asked-offers-nothing "$PLAY,wait-run=starting,click-button=Stop,wait-run=none" 'play_end=play\.stopped' 'notice_actions=\[\]'
with_game unavailable check game-again-offered "$PLAY,wait-run=none" 'play_end=play\.start_failed' 'run_notice=true' 'notice_actions=\["Play again"\]'
with_game unavailable check game-again "$PLAY,wait-run=none,click-button=Play again,wait-run=none" 'play_end=play\.start_failed' 'run_notice=true' 'play_check=passed' 'problem=none'
# The same at the two widths the toolbar is accepted at: the word and Stop keep the one row.
with_game runs check_sized "$wide" wide-key-play     'key=f5,wait-run=running' 'window=1261x' 'play_run=Running' "$row" 'toolbar_fits=Some\(true\)'
with_game runs check_sized "$narrow" narrow-key-play 'key=f5,wait-run=running' 'window=621x' 'play_run=Running' 'toolbar_folded=true' "$row" 'toolbar_fits=Some\(true\)'
with_game runs check_sized "$narrow" narrow-moved-on "$MOVED" 'window=621x' 'moved_notice=true' "$row" 'toolbar_fits=Some\(true\)'
# The game ends by itself: a notice that stays until it is dismissed.
with_game crashes check game-crashes "$PLAY,wait-run=running,wait-run=none" 'play_run=None' 'play_end=play\.crashed' 'run_notice=true' 'left_notice=false' 'notice=false'
with_game crashes check game-crash-dismissed "$PLAY,wait-run=running,wait-run=none,click-button=Dismiss" 'play_end=play\.crashed' 'run_notice=false'
# The person closes the game's window: said in the status bar, no notice.
with_game window-closed check game-window-closed "$PLAY,wait-run=running,wait-run=none" 'play_end=play\.window_closed' 'run_notice=false' 'left_notice=false'
# The server will not go: a notice of its own, and the end the person asked for stays said.
with_game server-stays check game-server-stays "$PLAY,wait-run=running,click-button=Stop,wait-run=none" 'play_end=play\.stopped' 'run_notice=false' 'left_notice=true'
# A server that goes only when it is killed is gone all the same: nothing was left behind.
with_game needs-kill check game-killed-server "$PLAY,wait-run=running,click-button=Stop,wait-run=none" 'play_end=play\.stopped' 'run_notice=false' 'left_notice=false'
# The game cannot be launched: it did not start, and the notice says so. It was never running.
with_game unavailable check game-unavailable "$PLAY,wait-run=none" 'play_run=None' 'play_end=play\.start_failed' 'run_notice=true' 'play_check=passed' 'problem=none'
# The result says how the two processes of a run left, so that a run that ended as asked is
# also seen to have ended cleanly. Stopped on request, both leave with status 0 and the
# cleanup has nothing to say. A client that ends by itself leaves with its own status, and
# the reason for the end is named. A server that goes only when it is killed is said to have
# been ended by a signal, forced; one that does not go at all has no exit, and the cleanup
# says twice that it failed.
STOPPED="$PLAY,wait-run=running,click-button=Stop,wait-run=none"
with_game runs check play-proof-stopped "$STOPPED" 'play_end=play\.stopped' \
    'play_cause=none play_server_exit=0 play_client_exit=0 play_cleanup=\[\]'
with_game crashes check play-proof-crashed "$PLAY,wait-run=running,wait-run=none" 'play_end=play\.crashed' \
    'play_cause=play\.unexpected_exit play_server_exit=0 play_client_exit=101 play_cleanup=\[\]'
with_game needs-kill check play-proof-killed "$STOPPED" 'play_end=play\.stopped' \
    'play_server_exit=signal9,forced play_client_exit=0 play_cleanup=\[play\.server_cleanup_failed:grace_deadline\]'
with_game server-stays check play-proof-stayed "$STOPPED" 'play_end=play\.stopped' \
    'play_server_exit=none play_client_exit=0 play_cleanup=\[play\.server_cleanup_failed:grace_deadline,play\.server_cleanup_failed:reap_deadline\]'
check play-proof-none 'click-row=11' 'play_end=none' 'play_cause=none play_server_exit=none play_client_exit=none play_cleanup=none'
# A wait for a run to be running fails at once when there is no run to wait for, and says
# why: the game could not be started, the run ended by itself before, or nothing was started
# because no game is set up. Before, such a wait went on for as long as a wait may, minutes.
# Each case must be over well within a minute.
wait_begun="$(date +%s)"
with_game unavailable check_failing play-proof-wait-failed "$PLAY,wait-run=running" 'play_run=None' 'play_end=play\.start_failed' \
    'failed_steps=\["wait-run=running: the run ended as play\.start_failed'
with_game crashes check_failing play-proof-wait-crashed "$PLAY,wait-run=running,wait-run=none,wait-run=running" 'play_end=play\.crashed' \
    'failed_steps=\["wait-run=running: the run ended as play\.crashed \(play\.unexpected_exit\)"\]'
check_failing play-proof-wait-nothing 'key=f5,wait-run=running' 'play_run=None' 'play_end=none' \
    'failed_steps=\["wait-run=running: no run was started"\]'
if wanted play-proof-wait && [ "$(( $(date +%s) - wait_begun ))" -gt 60 ]; then
    echo "play-proof-wait: FAILED, the waits took $(( $(date +%s) - wait_begun )) seconds"; failed=1
fi
# A cooked level the core refuses is not played.
with_game runs check game-refused-package 'cook,spoil-cooked,click-button=Play,click-item=Open Game Window' 'play_run=None' 'play_end=none' 'play_check=refused:payload\.hash_mismatch' 'notice=true'
# In the narrow window the word is a dot and Stop stays; the row is still one row.
with_game runs check_sized "$narrow" narrow-game "$PLAY,wait-run=running" 'window=621x' 'play_run=Running' 'play_stop=true' "$row" 'toolbar_fits=Some\(true\)'
with_game runs check_sized "$wide" wide-game "$PLAY,wait-run=running" 'window=1261x' 'play_run=Running' "$row" 'toolbar_fits=Some\(true\)'

# Closing the editor while the game runs. With unsaved changes the usual question comes first
# and says the game is stopped; without, the question is only about the game.
CLOSING="key=ctrl+s,$PLAY,wait-run=running,close"
with_game runs check close-asks-game "$CLOSING" 'shutdown=AskRun' 'question=No' 'play_run=Running'
with_game runs check close-keeps-playing "$CLOSING,click-button=Keep playing" 'shutdown=No' 'play_run=Running'
with_game runs check close-unsaved-first "$PLAY,wait-run=running,close" 'question=Asking' 'shutdown=No' 'play_run=Running'
# The server will not go: closing waits, then asks once more; the editor can be kept open.
with_game server-stays check close-left-behind "$CLOSING,click-button=Stop and close,wait-run=none" 'shutdown=LeftBehind' 'left_notice=true'
with_game server-stays check close-kept-open "$CLOSING,click-button=Stop and close,wait-run=none,click-button=Keep the editor open" 'shutdown=No' 'left_notice=true'

# Leaving the project for the project browser, from the Level menu. It asks what closing
# asks, in its own words, and where closing ends the editor it shows the browser, in front of
# an editor that holds no project. A scripted editor that started without the browser shows
# it without a list, so that no case reads or writes the list of whoever is at the machine.
SWITCH='choose=Level>Switch project'
LEFT='launcher=shown launcher_rows=\[\]'
check project-switch "key=ctrl+s,$SWITCH" \
    "$LEFT" 'launcher_list=none' 'launcher_entered=0' 'launcher_left=1' 'question=No' 'shutdown=No' \
    'project=NoFolder' 'has_file=false' 'game=false' 'runtime=none' 'problem=none'
# A level nobody saved counts as unsaved: the question, with the button that saves first
# saying what follows. Kept editing, nothing is left; discarded, nothing is written; saved
# first, the level is written and the project is left.
check project-switch-asks "$SWITCH" 'question=Asking' 'launcher=off' 'launcher_left=0'
check project-switch-kept "$SWITCH,click-button=Keep editing" 'question=No' 'launcher=off' 'launcher_left=0' 'dirty=true'
check project-switch-discards "$SWITCH,click-button=Discard changes" "$LEFT" 'launcher_left=1' 'question=No' 'has_file=false'
check project-switch-saves "$SWITCH,click-button=Save and leave" "$LEFT" 'launcher_left=1' 'question=No' 'problem=none'
if wanted project-switch-discards; then
    [ ! -e "$work/project-switch-discards/levels/current-map.level.json" ] \
        || { echo "project-switch-discards: FAILED, the level was saved though its changes were discarded"; failed=1; }
fi
if wanted project-switch-saves; then
    [ -s "$work/project-switch-saves/levels/current-map.level.json" ] \
        || { echo "project-switch-saves: FAILED, the level was not saved before the project was left"; failed=1; }
fi
# With a game running: the question is about the game, in the words of leaving, and the
# browser is shown only when the game and its server have shut down. Kept playing, the game
# goes on. With unsaved changes the usual question comes first and says the game is stopped.
# A server that will not go is asked about once more, and the person can stay in the project.
SWITCHING="key=ctrl+s,$PLAY,wait-run=running,$SWITCH"
with_game runs check project-switch-asks-game "$SWITCHING" 'shutdown=AskRun' 'question=No' 'play_run=Running' 'launcher=off'
with_game runs check project-switch-keeps-playing "$SWITCHING,click-button=Keep playing" 'shutdown=No' 'play_run=Running' 'launcher=off' 'launcher_left=0'
with_game runs check project-switch-stops-game "$SWITCHING,click-button=Stop and leave,wait-run=none" \
    "$LEFT" 'launcher_left=1' 'shutdown=No' 'play_run=None' 'game=false' 'left_notice=false'
with_game runs check project-switch-unsaved-game "$PLAY,wait-run=running,$SWITCH" 'question=Asking' 'shutdown=No' 'play_run=Running' 'launcher=off'
with_game runs check project-switch-discards-game "$PLAY,wait-run=running,$SWITCH,click-button=Discard changes,wait-run=none" \
    "$LEFT" 'launcher_left=1' 'question=No' 'shutdown=No' 'play_run=None'
with_game server-stays check project-switch-left-behind "$SWITCHING,click-button=Stop and leave,wait-run=none" \
    'shutdown=LeftBehind' 'launcher=off' 'launcher_left=0' 'left_notice=true'
with_game server-stays check project-switch-stays "$SWITCHING,click-button=Stop and leave,wait-run=none,click-button=Stay in the project" \
    'shutdown=No' 'launcher=off' 'launcher_left=0' 'left_notice=true' 'has_file=true'
with_game server-stays check project-switch-anyway "$SWITCHING,click-button=Stop and leave,wait-run=none,click-button=Leave anyway" \
    "$LEFT" 'launcher_left=1' 'shutdown=No'

# The landscape tool: T chooses it, its panel takes the inspector's place, a mode button
# chooses the brush, and the brush points at the ground under the cursor.
check sculpt-key      'key=t'                             'sculpt=true'
check sculpt-leaves   'key=t,key=w'                       'sculpt=false' 'tool=Translate'
check brush-mode      'inspector,choose=Tool>Sculpt,click-button=Smooth' \
                      'sculpt=true' 'brush=\(Smooth'
check brush-aims      'choose=Add>Landscape,key=t,aim-ground=2:4' \
                      'brush_aim=Some\(Vec3\((1\.9[0-9]?|2\.0[0-9]?), 0\.0, (3\.9[0-9]?|4\.0[0-9]?)\)\)'
# Without a landscape under the cursor the brush points at nothing.
check brush-off-ground 'key=t,aim-ground=2:4'              'brush_aim=None'
# A stroke raises the ground and is one undo step; undoing it leaves the flat landscape.
stroke='choose=Add>Landscape,key=t,stroke=-4:0:4:4'
flat='ground_height=\(0\.000,0\.000\)'
check stroke-raises   "$stroke"                           'ground_height=\(0\.000,0\.[1-9]' 'undo=Some\("sculpt"\)' 'stroke_active=false' 'dragging=false'
check stroke-undo     "$stroke,key=ctrl+z"                "$flat" 'objects=23' 'undo=Some\("create"\)' 'redo=Some\("sculpt"\)'
check stroke-lower    'choose=Add>Landscape,inspector,choose=Tool>Sculpt,click-button=Lower,stroke=-4:0:-2:2' \
                      'ground_height=\(-0\.[1-9][0-9]*,0\.000\)' 'brush=\(Lower'
# Escape, and the cursor leaving the window, cancel a stroke and put the ground back.
check stroke-escape   'choose=Add>Landscape,key=t,hold-stroke=-4:0:4:4,key=escape,release' "$flat" 'undo=Some\("create"\)' 'dragging=false' 'stroke_active=false'
check stroke-leave    'choose=Add>Landscape,key=t,hold-stroke=-4:0:4:4,leave,release' "$flat" 'undo=Some\("create"\)' 'dragging=false' 'stroke_active=false'

# A click on the ground chooses the landscape.
check pick-landscape  'choose=Add>Landscape,click-ground=2:4' 'selected=Some\("Landscape"\)'
# The tie between landscape and slab, away from the tie: beside the landscape the slab is
# chosen; where the ground was raised the landscape is; where it was dug below the slab's top
# the slab covers it and is chosen.
check pick-slab-beside 'choose=Add>Landscape,click-ground=-4:-20' 'selected=Some\("Ground"\)'
check pick-raised     'choose=Add>Landscape,key=t,stroke=-4:0:4:4,key=w,click-ground=0:2' 'selected=Some\("Landscape"\)' 'ground_height=\(0\.000,0\.[1-9]'
check pick-dug        'choose=Add>Landscape,inspector,choose=Tool>Sculpt,click-button=Lower,stroke=-4:0:4:4,key=w,click-ground=0:2' 'selected=Some\("Ground"\)' 'ground_height=\(-0\.[1-9]'
# Typing a name and Enter renames; Ctrl+Z gives the old name back.
check rename          'click-row=11,inspector,type-name=42' 'selected=Some\("42"\)' 'undo=Some\("rename"\)'
check rename-undo     'click-row=11,inspector,type-name=42,key=ctrl+z' 'selected=Some\("Tall box"\)' 'redo=Some\("rename"\)'
# A save that fails shows the notice; Dismiss puts it away; the level stays unsaved.
check nosave-notice   'click-row=11,drag-gizmo=-60,key=ctrl+s' 'problem=save\.parent_create' 'notice=true' 'dirty=true'
check nosave-dismiss  'click-row=11,drag-gizmo=-60,key=ctrl+s,click-button=Dismiss' 'problem=none' 'notice=false' 'dirty=true'
# Closing with unsaved changes asks; Keep editing closes the question.
check close-asks      'click-row=11,drag-gizmo=-60,close' 'question=Asking'
check close-keeps     'click-row=11,drag-gizmo=-60,close,click-button=Keep editing' 'question=No' 'dirty=true'

# The level dialog. Ctrl+N asks for a name and begins an empty level there; nothing is written
# until it is saved.
check new-asks        'key=ctrl+n' 'level_dialog=New' 'objects=22'
check new-level       'key=ctrl+n,type=7,key=enter,click-button=Discard changes' 'level_dialog=Closed' 'level="7"' 'objects=0' 'has_file=false' 'empty_hint=true' 'undo=None'
check new-saved       'key=ctrl+n,type=7,key=enter,click-button=Discard changes,choose=Add>Landscape,key=ctrl+s' 'level="7"' 'objects=1' 'has_file=true' 'dirty=false' 'empty_hint=false' 'problem=none'
# The same folder again: the saved level is in the list. The open level is unsaved, so
# choosing it asks first; Keep editing stays, Discard changes opens it.
check new-saved-again-asks  'key=ctrl+o,click-button=7' 'level_dialog=Closed' 'question=Asking' 'level="current-map"' 'objects=22'
check new-saved-again-keeps 'key=ctrl+o,click-button=7,click-button=Keep editing' 'question=No' 'level="current-map"' 'dirty=true'
check new-saved-again 'key=ctrl+o,click-button=7,click-button=Discard changes' 'question=No' 'level="7"' 'objects=1' 'has_file=true' 'dirty=false' 'ground_tiles=4'
# Save and open saves the open level to its own file first, then opens the other.
check new-saved-again-saves 'click-row=11,drag-gizmo=-60,key=ctrl+o,click-button=7,click-button=Save and open' 'question=No' 'level="7"' 'objects=1' 'dirty=false' 'problem=none'
# And once more: the level saved a moment ago is there, with the box where it was dragged.
check new-saved-again-reopens 'click-row=11' 'level="current-map"' 'has_file=true' 'dirty=false' 'position=Some\(\[-[1-9][0-9.]*, 0\.5, -8\.0\]\)'
# A new level with unsaved changes: Save and begin saves first, then begins the empty level.
check new-saves       'key=ctrl+n,type=7,key=enter,click-button=Save and begin' 'question=No' 'level="7"' 'objects=0' 'has_file=false'
check new-saves-again 'click-row=11' 'level="current-map"' 'has_file=true' 'objects=22'
# The empty level's own two buttons.
check empty-adds      'key=ctrl+n,type=7,key=enter,click-button=Discard changes,click-button=Add a landscape' 'objects=1' 'empty_hint=false' 'ground_tiles=4'
check empty-opens     'key=ctrl+n,type=7,key=enter,click-button=Discard changes,click-button=Open a level' 'level_dialog=Open' 'objects=0'
# Adding and removing objects. A new object stands where the view looks and is selected.
check add-box         'choose=Add>Box' 'objects=23' 'selected=Some\("Box 15"\)' 'position=Some\(\[0\.0, 0\.5, 2\.0\]\)' 'half_size=Some\(\[0\.5, 0\.5, 0\.5\]\)' 'undo=Some\("create"\)'
check add-box-undo    'choose=Add>Box,key=ctrl+z' 'objects=22' 'redo=Some\("create"\)'
check add-spawn       'choose=Add>Spawn point' 'objects=23' 'selected=Some\("Spawn 9"\)' 'position=Some\(\[0\.0, 2\.0, 2\.0\]\)'
# The Remove button is there in a wide window; in a narrow one the Delete key removes.
check_sized "$wide" remove-button 'click-row=11,click-button=Remove' 'window=1261x' 'objects=21' 'selected=None' 'undo=Some\("delete"\)'
check delete-key      'click-row=11,key=delete' 'objects=21' 'undo=Some\("delete"\)'
check delete-undo     'click-row=11,key=delete,key=ctrl+z' 'objects=22' 'redo=Some\("delete"\)'
# The landscape's layers, in the Paint part of the landscape tool.
L='choose=Add>Landscape,inspector,choose=Tool>Paint'
check paint-tool      "$L" 'paint=true' 'layers=0' 'paint_layer=None'
check layer-add       "$L,click-button=Add layer" 'layers=1' 'paint_layer=Some\("Grass"\)' 'undo=Some\("layers"\)' 'problem=none'
check layer-add-two   "$L,click-button=Add layer,click-button=Add layer" 'layers=2' 'paint_layer=Some\("Dirt"\)' 'problem=none'
check layer-choose    "$L,click-button=Add layer,click-button=Add layer,click-layer=1" 'layers=2' 'paint_layer=Some\("Grass"\)'
check layer-rename    "$L,click-button=Add layer,type-layer=42" 'layers=1' 'paint_layer=Some\("42"\)' 'problem=none'
check layer-remove    "$L,click-button=Add layer,click-button=Add layer,click-button=Remove layer" 'layers=1' 'paint_layer=Some\("Grass"\)' 'problem=none'
check layer-remove-first "$L,click-button=Add layer,click-button=Add layer,click-layer=1,click-button=Remove layer" 'layers=1' 'paint_layer=Some\("Dirt"\)' 'problem=none'
check layer-undo      "$L,click-button=Add layer,key=ctrl+z" 'layers=0' 'paint_layer=None' 'redo=Some\("layers"\)'
check layer-no-ground 'inspector,choose=Tool>Paint,click-button=Add layer' 'layers=0' 'problem=none' 'objects=22'
# Painting. With one layer there is nothing to paint against: the core refuses, the notice
# says so, and the editor is not left holding the stroke (the next command works).
check paint-single    "$L,click-button=Add layer,stroke=-4:0:4:4" 'problem=layer\.single_weight' 'notice=true' 'undo=Some\("layers"\)' 'dragging=false' 'stroke_active=false'
check paint-single-undo "$L,click-button=Add layer,stroke=-4:0:4:4,key=ctrl+z" 'layers=0' 'problem=none' 'redo=Some\("layers"\)'
# With two layers a stroke paints the chosen one, as one undo step; the other gives way.
P="$L,click-button=Add layer,click-button=Add layer"
check paint-stroke    "$P,stroke=-4:0:4:4" 'paint_layer=Some\("Dirt"\)' 'layer_weights=\[0\.9[0-9]*, 0\.0[4-9][0-9]*\]' 'undo=Some\("paint"\)' 'problem=none' 'dragging=false' "$flat"
check paint-undo      "$P,stroke=-4:0:4:4,key=ctrl+z" 'layer_weights=\[1\.0, 0\.0\]' 'redo=Some\("paint"\)' 'layers=2'
check paint-escape    "$P,hold-stroke=-4:0:4:4,key=escape,release" 'layer_weights=\[1\.0, 0\.0\]' 'undo=Some\("layers"\)' 'dragging=false' 'stroke_active=false'
# Shift erases: painting and then erasing the same stroke leaves less of the layer than painting.
check paint-erase     "$P,stroke=-4:0:4:4,erase-stroke=-4:0:4:4" 'undo=Some\("paint"\)' 'problem=none' 'layer_weights=\[0\.9[0-9]*, 0\.0[0-3][0-9]*\]'
# Erasing a layer that is nowhere changes nothing and is no undo step.
check paint-erase-nothing "$P,erase-stroke=-4:0:4:4" 'layer_weights=\[1\.0, 0\.0\]' 'undo=Some\("layers"\)' 'problem=none' 'dragging=false'
# A third layer added after painting keeps what was painted, and can be painted itself.
check paint-third     "$P,stroke=-6:-2:6:6,click-button=Add layer,stroke=-8:6:2:-6" 'layers=3' 'paint_layer=Some\("Rock"\)' 'layer_weights=\[0\.[0-9]+, 0\.[0-9]*[1-9][0-9]*, 0\.[0-9]*[1-9][0-9]*\]' 'undo=Some\("paint"\)' 'problem=none'
# Without a layer the paint brush does nothing.
check paint-no-layer  "$L,stroke=-4:0:4:4" 'undo=Some\("create"\)' 'problem=none' 'dragging=false' 'stroke_active=false'
# With a landscape brush a click on an object chooses nothing.
check brush-no-choose 'key=t,click-object=11' 'selected=None' 'sculpt=true'
# The hierarchy as a tree. Set-up puts "Tall box" (row 11) and "Low box" (row 12) under
# "Wall" (row 10); the toggle of "Wall" folds them away and brings them back.
T='reparent=11:10,reparent=12:10'
check tree-flat       'click-row=11' 'rows_shown=22' 'folded=0'
check tree-nested     "$T" 'objects=22' 'rows_shown=22' 'folded=0' 'undo=Some\("reparent"\)' 'problem=none'
check tree-fold       "$T,click-toggle=10" 'rows_shown=20' 'folded=1' 'selected=None' 'objects=22'
check tree-unfold     "$T,click-toggle=10,click-toggle=10" 'rows_shown=22' 'folded=0'
# With rows folded away the rows after them are still the right ones to click.
check tree-fold-click "$T,click-toggle=10,click-row=13" 'rows_shown=20' 'selected=Some\("Pillar"\)'
# A folded parent hides grandchildren too, and undoing the nesting leaves nothing folded away.
check tree-deep       "$T,reparent=12:11,click-toggle=10" 'rows_shown=20' 'folded=1'
check tree-undo       "$T,click-toggle=10,key=ctrl+z,key=ctrl+z" 'rows_shown=22' 'undo=None'
# Placing on the landscape. A new object stands where the view's line of sight meets the
# landscape, by the core's account of the surface. The view looks at (0, 1, 2) from yaw 0.6,
# pitch 0.55, so on flat ground at height 0 that is (-0.921, 0, 0.654).
A='choose=Add>Landscape'
check place-flat      "$A,choose=Add>Box" 'selected=Some\("Box 15"\)' 'position=Some\(\[-0\.92[0-9]*, 0\.5, 0\.65[0-9]*\]\)' 'problem=none'
check place-spawn     "$A,choose=Add>Spawn point" 'selected=Some\("Spawn 9"\)' 'position=Some\(\[-0\.92[0-9]*, 2\.0, 0\.65[0-9]*\]\)'
# The line of sight passes beside the landscape: the object goes to the point the view turns
# about, on height 0, as without a landscape.
check place-miss      "$A,look-at=40:1:2,choose=Add>Box" 'position=Some\(\[40\.0, 0\.5, 2\.0\]\)' 'problem=none'
# A landscape turned 10 degrees about X: the surface there is at -tan(10 deg) * 0.528 = -0.093.
check place-tilted    "$A,tilt=22:10,choose=Add>Box" 'position=Some\(\[-1\.00[0-9]*, 0\.40[0-9]*, 0\.52[0-9]*\]\)' 'problem=none'
# Ground raised by a stroke under the view: the box stands on the raised ground.
check place-raised    "$A,key=t,stroke=-2:2:2:6,key=w,choose=Add>Box" 'position=Some\(\[-0\.[0-9]+, 0\.9[0-9]*, 1\.[0-9]+\]\)' 'ground_height=\(0\.000,0\.[1-9]' 'undo=Some\("create"\)'
# The landscape's rules. Grass: a rule scatters it, four tries in each of the landscape's 1024
# cells; the blades drawn come from the core's geometry.
R='choose=Add>Landscape,inspector,choose=Tool>Rules'
check rules-tool      "$R" 'tool=Translate' 'rules=\[\]' 'grass=0' 'problem=none'
check rule-grass      "$R,click-button=Add grass" 'rules=\["Grass everywhere"\]' 'grass=4096' 'undo=Some\("rule"\)' 'problem=none'
check rule-grass-undo "$R,click-button=Add grass,key=ctrl+z" 'rules=\[\]' 'grass=0' 'redo=Some\("rule"\)'
check rule-remove     "$R,click-button=Add grass,click-button=Remove rule" 'rules=\[\]' 'grass=0' 'undo=Some\("rule"\)'
# A typed setting is one undo step and changes what grows: half the density, about half the grass.
check rule-density    "$R,click-button=Add grass,type-rule=0:0.5" 'rule=Some\(\[0\.5, 4\.0, 0\.4, 0\.08, 1\.0\]\)' 'grass=(19|20|21)[0-9][0-9] ' 'problem=none'
# With layers, grass grows on the first one and less where another is painted over it; a
# layer that keeps it out takes more away; on a layer that is nowhere nothing grows.
G="choose=Add>Landscape,inspector,choose=Tool>Paint,click-button=Add layer,click-button=Add layer"
check rule-on-layer   "$G,stroke=-6:-2:6:6,choose=Tool>Rules,click-button=Add grass" 'rules=\["Grass on Grass"\]' 'grass=3[78][0-9][0-9] ' 'problem=none'
check rule-kept-out   "$G,stroke=-6:-2:6:6,choose=Tool>Rules,click-button=Add grass,step-rule-layer=kept-out-by" 'rules=\["Grass on Grass"\]' 'grass=3[56][0-9][0-9] ' 'problem=none'
check rule-on-empty   "$G,choose=Tool>Rules,click-button=Add grass,step-rule-layer=grows-on" 'rules=\["Grass on Dirt"\]' 'grass=0 ' 'problem=none'
check rule-everywhere "$G,choose=Tool>Rules,click-button=Add grass,step-rule-layer=grows-on,step-rule-layer=grows-on" 'rules=\["Grass everywhere"\]' 'grass=4096'
# A layer that fills itself on steep ground comes with its rule, as one undo step. On flat
# ground it stays empty, and the panel says why.
check rule-steep      "$R,click-button=Add steep layer" 'rules=\["Rock by steepness"\]' 'layers=2' 'layer_weights=\[1\.0, -1\.0\]' 'filled=\[0\.0\]' 'flat_note=below-band' 'undo=Some\("rule"\)' 'problem=none'
check rule-steep-undo "$R,click-button=Add steep layer,key=ctrl+z" 'rules=\[\]' 'layers=0'
# A band that starts at flat still leaves perfectly flat ground empty while it fades in, and
# the panel says so; with the fade at 0 flat ground is filled.
check rule-flat-fades "$R,click-button=Add steep layer,type-rule=0:0" 'rule=Some\(\[0\.0, 1\.0, 0\.1,' 'filled=\[0\.0\]' 'flat_note=fades-in'
check rule-flat-in    "$R,click-button=Add steep layer,type-rule=0:0,type-rule=2:0" 'rule=Some\(\[0\.0, 1\.0, 0\.0,' 'filled=\[1\.0\]' 'flat_note=none'
# On a hill three metres high the layer fills on the flanks.
check rule-hill       'choose=Add>Landscape,key=t,stroke=0:2:1:2,stroke=0:2:1:2,stroke=0:2:1:2,stroke=0:2:1:2,stroke=0:2:1:2,stroke=0:2:1:2,inspector,choose=Tool>Rules,click-button=Add steep layer' 'ground_height=\(0\.000,[23]\.' 'filled=\[0\.0[0-9]*[1-9][0-9]*\]' 'problem=none'
# A setting the core refuses leaves the rule as it was and is said in the notice.
check rule-refused    "$R,click-button=Add steep layer,type-rule=1:0.1" 'problem=rule\.parameter_range' 'notice=true' 'rule=Some\(\[0\.25, 1\.0,'
# A layer a rule fills cannot be painted, and removing it takes the rule with it.
check rule-no-paint   "$R,click-button=Add steep layer,choose=Tool>Paint,click-layer=2,stroke=-4:0:4:4" 'problem=layer\.derived_output' 'notice=true' 'undo=Some\("rule"\)' 'dragging=false' 'stroke_active=false'
check rule-layer-gone "$R,click-button=Add steep layer,choose=Tool>Paint,click-layer=2,click-button=Remove layer" 'rules=\[\]' 'layers=1' 'problem=none' 'undo=Some\("layers"\)'
# Two rules: the second is chosen when added, and a click chooses the first again.
check rule-two        "$R,click-button=Add grass,click-button=Add steep layer,click-rule=1" 'rules=\["Grass everywhere", "Rock by steepness"\]' 'rule=Some\(\[1\.0, 4\.0,' 'grass=4096'
# Round trip through the UI's own jobs. One editor sculpts a hill, makes two layers, paints
# the second, adds a grass rule and a steep-ground rule (with its layer), saves and cooks. A
# fresh editor in the same folder opens that level file at start and must hold the same
# landscape: the digest covers every height, layer, painted weight and rule.
RT='choose=Add>Landscape,key=t,stroke=0:2:1:2,stroke=0:2:1:2,inspector,choose=Tool>Paint,click-button=Add layer,click-button=Add layer,stroke=-6:-2:6:6,choose=Tool>Rules,click-button=Add grass,click-button=Add steep layer,key=ctrl+s,cook'
check roundtrip       "$RT" 'dirty=false' 'cook_needed=false' 'problem=none' 'has_file=true' 'layers=3' 'rules=\["Grass on Grass", "Rock by steepness"\]' 'ground_height=\(0\.000,[01]\.' 'layer_weights=\[0\.9[0-9]*, 0\.0[0-9]*[1-9][0-9]*, -1\.0\]' 'grass=[1-9][0-9]{3} ' 'cooked=[0-9a-f]{8}' 'cooked_source=[0-9a-f]{8}'
check roundtrip-again 'cook' 'dirty=false' 'has_file=true' 'problem=none' 'objects=23' 'layers=3' 'rules=\["Grass on Grass", "Rock by steepness"\]' 'ground_tiles=4' 'undo=None'
same roundtrip roundtrip-again authored ground_height grass objects layers
# Cooking the reopened level gives the hashes the first editor's cook gave; cooking it once
# more, in a third editor, gives them again.
same roundtrip roundtrip-again cooked cooked_source
check roundtrip-again-cooks 'cook' 'problem=none' 'dirty=false'
same roundtrip roundtrip-again-cooks cooked cooked_source authored
# A change of paint only: the level's source and its picture change, what can be walked on
# does not. The simulation hash stays, the source hash and the digest move.
check roundtrip-again-paint 'inspector,choose=Tool>Paint,click-layer=2,stroke=-6:6:6:-2,key=ctrl+s,cook' 'undo=Some\("paint"\)' 'dirty=false' 'cook_needed=false' 'problem=none'
same roundtrip roundtrip-again-paint cooked ground_height
differ roundtrip roundtrip-again-paint cooked_source authored
# And a change of height does move what can be walked on.
check roundtrip-again-sculpt 'key=t,stroke=4.3:-4.4:5.3:-4.4,key=ctrl+s,cook' 'undo=Some\("sculpt"\)' 'dirty=false' 'problem=none'
differ roundtrip roundtrip-again-sculpt cooked authored

# The editor's own frames. In these cases nothing asks for a frame except the input events
# played and the editor itself; the result is read once the editor has come to rest. Rows
# laid out must match what the level holds: a panel that only caught up at the next mouse
# move would be one short here.
check_reactive reactive-select  'click-row=11' 'selected=Some\("Tall box"\)' 'rows_shown=22'
check_reactive reactive-add-box 'choose=Add>Box' 'objects=23' 'rows_shown=23' 'selected=Some\("Box 15"\)'
check_reactive reactive-undo    'choose=Add>Box,key=ctrl+z' 'objects=22' 'rows_shown=22'
check_reactive reactive-layers  'choose=Add>Landscape,inspector,choose=Tool>Paint,click-button=Add layer,click-button=Add layer' 'layers=2' 'layer_rows=2' 'ground_tiles=4'
check_reactive reactive-rules   'choose=Add>Landscape,inspector,choose=Tool>Rules,click-button=Add grass' 'rules=\["Grass everywhere"\]' 'rule_rows=1' 'grass=4096'
check_reactive reactive-rename  'click-row=11,inspector,type-name=42' 'selected=Some\("42"\)'
# The game changes on its own time, while the editor sleeps: the Play coordinator wakes the
# editor for each change. Here nothing asks for a frame but the input played and the editor
# itself. The count is of wakes the coordinator sent and the window system took. (A reactive
# check looks only once the editor is at rest, so it sees where a run has come to, not the
# states it passed through on the way.)
with_game runs check_reactive reactive-game "$PLAY,wait-run=running" 'play_run=Running' 'play_stop=true' 'game_wakes=[1-9]' 'problem=none'
with_game crashes check_reactive reactive-game-crash "$PLAY,wait-run=none" 'play_end=play\.crashed' 'run_notice=true' 'game_wakes=[1-9]'
# Without a game there is nobody to wake the editor.
check play-no-wakes   'click-button=Play'                 'game=false' 'game_wakes=0'
# The brush finds the ground on the corners of its cells too. A ray exactly through a corner
# can slip between the triangles; the brush then looks a fraction of a pixel aside.
check aim-on-corners  'choose=Add>Landscape,key=t,aim-ground=3:-3' 'brush_aim=Some\(Vec3\((2\.9[0-9]*|3\.0[0-9]*), 0\.0, -(2\.9[0-9]*|3\.0[0-9]*)\)\)'
check aim-on-corner-2 'choose=Add>Landscape,key=t,aim-ground=4:-4' 'brush_aim=Some\(Vec3\((3\.9[0-9]*|4\.0[0-9]*), 0\.0, -(3\.9[0-9]*|4\.0[0-9]*)\)\)'
# The window's first frames. The editor reports from where frames are drawn what could write
# the window in each of its first twelve: the engine's own clear (no camera aimed at the
# window), the cameras (their copy step ready), or nothing. "Nothing" is a frame that goes to
# the desktop as the graphics driver handed it over, which showed as a pink flash at start.
# The first three frames must be the engine's clear, in the editor's window colour; from the
# fourth the cameras must write; and no frame may be written by nothing.
check_first_frames() {
    name="$1"
    wanted "$name" || return 0
    folder="$work/$name"
    mkdir -p "$folder/levels"
    lines="$(cd "$folder" && ARC_EDITOR_CHECK="$folder/$name.png" ARC_EDITOR_CHECK_STEPS='' \
        limit 180 "$editor" 2>&1 | grep '^ARC_EDITOR_FIRST_FRAME')"
    printf '%s\n' "$lines" > "$work/$name.result"
    [ -z "${ARC_EDITOR_RESULTS:-}" ] || printf '%s\n' "$lines" > "$ARC_EDITOR_RESULTS/$name.result"
    verdict=ok
    [ "$(printf '%s\n' "$lines" | grep -c .)" -eq 12 ] || verdict="FAILED, not twelve frames reported"
    if printf '%s\n' "$lines" | grep -q 'written=nothing'; then
        verdict="FAILED, a frame went out unwritten"
    fi
    for n in 1 2 3; do
        printf '%s\n' "$lines" | grep -Eq "frame=$n cameras=0 .* written=clear clear_colour=\[31, 31, 36\]" \
            || verdict="FAILED, frame $n is not the engine's clear in the window colour"
    done
    for n in 4 5 6 7 8 9 10 11 12; do
        printf '%s\n' "$lines" | grep -Eq "frame=$n cameras=4 orders=\[-1, 0, 1, 2\] .* blit_waiting=0 written=cameras" \
            || verdict="FAILED, frame $n is not written by all four cameras"
    done
    echo "$name: $verdict"
    if [ "$verdict" != ok ]; then
        printf '%s\n' "$lines" | sed 's/^/  /'
        failed=1
    fi
}
check_first_frames first-frames
check_first_frames first-frames-2
check_first_frames first-frames-3
# Typing in a field keeps Delete for the text.
check delete-in-field 'click-row=11,inspector,type-name=42,key=delete' 'objects=22'
# Save As writes the level under the new name and keeps working there.
check saveas          'key=ctrl+shift+s,type=8,key=enter' 'level_dialog=Closed' 'level="8"' 'objects=22' 'has_file=true' 'dirty=false' 'problem=none'
# A name another level has is refused, and the dialog stays.
check saveas-again    'key=ctrl+shift+s,type=8,key=enter' 'level_dialog=SaveAs' 'level="current-map"' 'has_file=false'
# Escape and Cancel close the dialog; behind it neither keys nor the view act.
check dialog-escape   'choose=Level>Open level,key=escape' 'level_dialog=Closed'
check dialog-cancel   'choose=Level>New level,click-button=Cancel' 'level_dialog=Closed' 'objects=22'
check dialog-modal    'click-row=11,drag-gizmo=-60,key=ctrl+o,key=t,key=ctrl+z' 'level_dialog=Open' 'sculpt=false' 'undo=Some\("drag"\)'
# The Particles tool, with a folder of the case's own in the game's place for the emitters
# (ARC_EDITOR_CHECK_PARTICLES; the emitters go under its assets/particles). One run through
# what the tool is for: it is chosen, an emitter is made, a number of it is typed, it is
# saved, and the preview is paused, begun again and stepped. The emitter is the game's file
# afterwards and the level is untouched: its undo history is empty and it was never saved.
# The preview is the editor's reference simulation, not the game's particle runtime: at
# twenty-five particles a second, fifty-seven steps of a sixtieth of a second are 0.95 s and
# twenty-three particles, none of which has lived its shortest life of a second yet.
P='choose=Tool>Particles,click-button=New emitter'
ARC_EDITOR_CHECK_PARTICLES=game; export ARC_EDITOR_CHECK_PARTICLES
check particles-smoke "$P,type-emitter=emission.rate_per_second.0:25,click-button=Save emitter,click-button=Pause,click-button=Restart,preview-steps=57" \
    'workspace=Particles' 'emitters=\[\("New Emitter", false, false\)\]' 'emitter=Some\("New Emitter"\)' \
    'emitter_game=true' 'emitter_files=1' 'emitter_rows=1' 'emitter_rate=Some\(25\.0\)' 'emitter_problem=none' \
    'emitter_history=\(true, false\)' 'preview=\(23, 0\.95, false\)' 'stage=\(true, 23\)' \
    'undo=None' 'has_file=false' 'problem=none' "$row" 'toolbar_fits=Some\(true\)'
# Back to the level: the stage is gone, the camera is where it was in the level, and the
# emitter that was made and not saved is still in the list.
check particles-back "$P,choose=Tool>Move" \
    'workspace=Objects' 'stage=\(false, 0\)' 'camera=\(0\.600,0\.550,38\.00\)' \
    'emitters=\[\("New Emitter", true, false\)\]' 'emitter_files=0' 'objects=22' 'problem=none'
# The GPU preview: the game's particle runtime draws the emitter on the graphics card. It
# is said to run only once the graphics card has said that it draws the effect; the
# reference simulation is then not drawn. An emitter the game's adapter does not draw, a
# cone here, is said as that, and the reference simulation stays.
# Twenty more clicks on the speed that is chosen already change nothing and take about two
# seconds: the effect is still running after them, and the saved picture is of an effect
# that has been emitting for that long, not of its first frame.
X5='click-button=1x,click-button=1x,click-button=1x,click-button=1x,click-button=1x'
check particles-gpu "$P,click-button=GPU preview,wait-gpu,$X5,$X5,$X5,$X5" \
    'workspace=Particles' 'preview_mode=Gpu' 'gpu=Running' 'stage=\(true, 0\)' 'problem=none'
check particles-refused-by-gpu "$P,click-button=Cone,click-button=GPU preview,wait-gpu" \
    'emitter_shape=Some\(Cone\)' 'preview_mode=Gpu' 'gpu=Unsupported \{ field: "shape"' 'stage=\(true, [0-9]+\)'
# An emitter that is saved is placed in the level from the Particles tool: the editor goes
# to the object tools with the new object selected, named after the emitter, and the level
# is of its one schema. The core is told what the game's folder holds, so the placement
# is no problem of the review, and the level is cooked with it. An emitter that was never
# saved has no file a level could name: nothing is placed.
check particles-place "$P,click-button=Save emitter,click-button=Place in level,cook" \
    'workspace=Objects' 'selected=Some\("New Emitter"\)' 'objects=23' 'emitter_placed=1' 'level_schema=1' \
    'stage=\(false, 0\)' 'review=\[\]' 'undo=Some\("create"\)' 'cook_needed=false' 'problem=none'
check particles-place-unsaved "$P,click-button=Place in level" \
    'workspace=Particles' 'objects=22' 'emitter_placed=0' 'level_schema=1' 'undo=None' 'problem=none'
# A level saved before it placed an emitter, and saved again with one placed, is a file of
# the format's one schema both times, and nothing else is written beside it.
check level-emitter-save 'save' 'has_file=true' 'dirty=false' 'level_schema=1' 'problem=none'
check level-emitter-save-again "$P,click-button=Save emitter,click-button=Place in level,save" \
    'has_file=true' 'dirty=false' 'emitter_placed=1' 'level_schema=1' 'problem=none'
if wanted level-emitter-save-again; then
    levels="$work/level-emitter-save/levels"
    [ "$(ls -A "$levels")" = "current-map.level.json" ] \
        && grep -q '"schema": 1,' "$levels/current-map.level.json" \
        && grep -q '"kind": "Emitter"' "$levels/current-map.level.json" \
        || { echo "level-emitter-save-again: FAILED, the level is not the one file of schema 1 with its emitter"; failed=1; }
fi
unset ARC_EDITOR_CHECK_PARTICLES

# The project's settings. Without a project folder they are to be had from the Level menu and
# say that there is none. With a folder that has no project file they open by themselves:
# setting up writes project.aproj, a starter level (the arena of 32 objects, not the
# game's map of 22 the editor has without a level) and the look profile, and the editor
# opens that level;
# started again on the same folder the editor finds the project, opens its start level by
# itself and shows nothing unasked. Looking at the project never changes its files.
check project-none 'choose=Level>Project settings' 'project=NoFolder' 'project_dialog=true' 'objects=22' 'problem=none'
ARC_EDITOR_CHECK_PROJECT=project; export ARC_EDITOR_CHECK_PROJECT
check project-setup 'click-button=Set up project,click-button=Close settings' \
    'project=Ready' 'project_dialog=false' 'has_file=true' 'dirty=false' 'objects=32' 'question=No' 'problem=none'
[ -s "$work/project-setup/project/project.aproj" ] && [ -s "$work/project-setup/project/levels/first-level.level.json" ] \
    && [ -s "$work/project-setup/project/assets/render/look-v1.json" ] \
    || { wanted project-setup && { echo "project-setup: FAILED, the project file, the starter level or the look profile was not written"; failed=1; }; }
# First-run setup must make more than a readable level: the starter has to cook into a
# package that passes the same check Play requires before opening the game window.
check project-starter-cooks 'click-button=Set up project,click-button=Close settings,choose=Level>Cook,click-button=Play,click-item=Check cooked level' \
    'project=Ready' 'has_file=true' 'cook_needed=false' 'play_check=passed' 'problem=none'
check project-setup-again 'click-row=7' 'project=Ready' 'project_dialog=false' 'has_file=true' 'dirty=false' 'selected=Some\("Platform"\)'
# A folder whose only level file cannot be read is not set up over it: the button says that
# it makes a new starter level, and the old file is as it was afterwards. A project file of
# an unsupported version is refused, shown, and unchanged.
if wanted project-legacy; then
    mkdir -p "$work/project-legacy/project/levels"
    printf 'not a level' > "$work/project-legacy/project/levels/current-map.level.json"
fi
check project-legacy 'click-button=Set up with a new starter level' 'project=Ready' 'project_dialog=true' 'has_file=true' 'objects=32' 'problem=none'
if wanted project-legacy; then
    [ "$(cat "$work/project-legacy/project/levels/current-map.level.json")" = 'not a level' ] \
        && [ -s "$work/project-legacy/project/levels/first-level.level.json" ] \
        || { echo "project-legacy: FAILED, the old level was changed or no starter was made"; failed=1; }
fi
if wanted project-newer; then
    mkdir -p "$work/project-newer/project"
    printf '%s' '{"schema":3,"project":{"name":"Newer"},"startup":{"start_level":"levels/a.level.json"},"rendering":{"profile":"assets/render/look-v1.json"},"controls":{"move_forward":"W","move_backward":"S","move_left":"A","move_right":"D","jump":"Space","toggle_view":"V","release_cursor":"Escape","look":{"mouse":true,"gamepad_right_stick":true}},"planned":{"multiplayer":{"clients":null,"bots":null,"network_conditions":null,"start_at":null,"zone":null},"gameplay":{"mode":null,"team_count":null,"respawn_seconds":null,"weapon_set":null}}}' \
        > "$work/project-newer/project/project.aproj"
    cp "$work/project-newer/project/project.aproj" "$work/project-newer.before"
fi
check project-newer 'click-row=11' 'project=Refused:project\.schema_unsupported' 'project_dialog=true' 'has_file=false'
if wanted project-newer; then
    cmp -s "$work/project-newer/project/project.aproj" "$work/project-newer.before" \
        && [ "$(ls "$work/project-newer/project" | wc -l | tr -d ' ')" = 1 ] \
        || { echo "project-newer: FAILED, the newer project file was changed or something was written beside it"; failed=1; }
fi
# The settings of a project are changed in the Project settings and saved: another look
# profile of the folder is chosen, moving forward is put on another key by pressing its
# button and then the key, and the right stick is no longer looked with. The dialog then
# shows them as the file's, and the file holds them, with what was not changed as it was.
# Started again on the folder the editor shows the same as read, with nothing to save, and
# looking at them does not write the file.
if wanted project-settings; then
    mkdir -p "$work/project-settings/project/assets/render"
    cp "$look" "$work/project-settings/project/assets/render/look-v1.json"
    cp "$look" "$work/project-settings/project/assets/render/dusk.json"
fi
SAVED_SETTINGS='project_keys=T,S,A,D,Space,V,Escape project_look=mouse project_profile=assets/render/dusk\.json'
check project-settings 'click-button=Set up project,click-button=Use,scroll-project=300,click-button=W,key=t,click-checkbox=Gamepad right stick,click-button=Save project settings' \
    'project=Ready' 'project_dialog=true' 'project_draft=Unchanged project_saved=true' "$SAVED_SETTINGS" 'problem=none'
if wanted project-settings; then
    manifest="$work/project-settings/project/project.aproj"
    grep -q '"move_forward": "T"' "$manifest" && grep -q '"jump": "Space"' "$manifest" \
        && grep -q '"mouse": true' "$manifest" && grep -q '"gamepad_right_stick": false' "$manifest" \
        && grep -q '"profile": "assets/render/dusk.json"' "$manifest" && grep -q '"clients": null' "$manifest" \
        || { echo "project-settings: FAILED, the project file does not hold the saved settings"; failed=1; }
    cp "$manifest" "$work/project-settings.saved"
fi
check project-settings-again 'choose=Level>Project settings' \
    'project=Ready' 'project_dialog=true' 'project_draft=Unchanged project_saved=false' "$SAVED_SETTINGS" 'problem=none'
if wanted project-settings-again; then
    cmp -s "$work/project-settings/project/project.aproj" "$work/project-settings.saved" \
        || { echo "project-settings-again: FAILED, looking at the settings changed the project file"; failed=1; }
fi
# Game look is drawn from the look profile the project names, and a profile that is saved in
# the Project settings is the one at once, without another start. The folder has the game's
# profile and another, "dusk", with a weaker sun, no light from the sky, another colour and
# roughness of the boxes and the ground, and a box texture that repeats every two metres.
#
# The first start has no project and so no profile. The project is set up, dusk is chosen
# and saved, and Game look, which was off, is drawn from it. Started again, with dusk the
# project's, Game look is chosen first and the game's profile is saved while it is drawn:
# the sun, the ground and the boxes are the game's, and every box was drawn anew, for its
# texture repeats otherwise. A save of a key alone reads the same profile and draws nothing
# anew. And dusk saved over the game's profile while that is drawn is what the view is
# drawn from, and still is after the editor's own picture was chosen in between.
if wanted project-look; then
    mkdir -p "$work/project-look/project/assets/render"
    cp "$look" "$work/project-look/project/assets/render/look-v1.json"
    cat > "$work/project-look/project/assets/render/dusk.json" <<'DUSK'
{
  "schema": 1,
  "camera": {
    "hdr": true,
    "exposure_ev100": 11,
    "tonemapping": "aces_fitted",
    "bloom": {"preset": "natural", "intensity": 0.2}
  },
  "sky": {
    "atmosphere": "earth",
    "scattering_lut_size": [128, 128],
    "environment_map": false,
    "ambient_light": "none",
    "clear_color_linear_rgba": [0.02, 0.01, 0.03, 1]
  },
  "sun": {
    "illuminance_lux": 40000,
    "color_linear_rgba": [1, 0.8, 0.6, 1],
    "position_m": [1, 0.3, 0.6],
    "look_at_m": [0, 0, 0],
    "shadows": {
      "enabled": true,
      "map_size": 2048,
      "first_cascade_far_bound_m": 8,
      "maximum_distance_m": 60
    }
  },
  "boxes": {
    "base_color_linear_rgba": [0.5, 0.25, 0.125, 1],
    "roughness": 0.75,
    "texture": "procedural_brick_v1",
    "texture_repeat_m": 2
  },
  "ground": {
    "base_color_linear_rgba": [0.25, 0.5, 0.125, 1],
    "roughness": 0.5,
    "paint": "linear_vertex_rgba"
  }
}
DUSK
    dusk_hash="$(shasum -a 256 "$work/project-look/project/assets/render/dusk.json" | cut -c1-8)"
fi
DUSK_LOOK='look_sun=Some\(40000'
DUSK_GROUND='ground_look=Some\(\(\[0\.25, 0\.5, 0\.125, 1\.0\], 0\.5\)\)'
DUSK_BOX='box_look=Some\(\(\[0\.5, 0\.25, 0\.125, 1\.0\], 0\.75, 2\.0\)\)'
GAME_LOOK='look_sun=Some\(130000'
GAME_GROUND='ground_look=Some\(\(\[1\.0, 1\.0, 1\.0, 1\.0\], 0\.9\)\)'
GAME_BOX='box_look=Some\(\(\[1\.0, 1\.0, 1\.0, 1\.0\], 1\.0, 1\.0\)\)'
SAVE_OTHER_LOOK='choose=Level>Project settings,click-button=Use,click-button=Save project settings'
check project-look 'click-button=Set up project,click-button=Use,click-button=Save project settings,click-button=Close settings,choose=View>Game look' \
    'project=Ready' 'project_saved=true' 'project_profile=assets/render/dusk\.json' 'view=GameLook' \
    "look=loaded:${dusk_hash:-none}" "$DUSK_LOOK" "$DUSK_GROUND" "$DUSK_BOX" 'problem=none'
check project-look-again "choose=View>Game look,$SAVE_OTHER_LOOK" \
    'project_dialog=true' 'project_saved=true' 'project_profile=assets/render/look-v1\.json' 'view=GameLook' \
    'look=loaded:3abf9a2f' "$GAME_LOOK" "$GAME_GROUND" "$GAME_BOX" 'problem=none'
check project-look-again-keys 'choose=View>Game look,choose=Level>Project settings,scroll-project=300,click-button=W,key=t,click-button=Save project settings' \
    'project_saved=true' 'project_keys=T,S,A,D,Space,V,Escape' 'view=GameLook' \
    'look=loaded:3abf9a2f' "$GAME_LOOK" "$GAME_GROUND" "$GAME_BOX" 'problem=none'
check project-look-again-back "choose=View>Game look,$SAVE_OTHER_LOOK,click-button=Close settings,choose=View>Editor,choose=View>Game look" \
    'project_dialog=false' 'project_profile=assets/render/dusk\.json' 'view=GameLook' \
    "look=loaded:${dusk_hash:-none}" "$DUSK_LOOK" "$DUSK_GROUND" "$DUSK_BOX" 'problem=none'
# A box is drawn once at the start and once for Game look. The profile of another repeat
# draws each a third time; the save of a key draws none.
same project-look-again-keys project-look-again objects
if wanted project-look-again; then
    drawn="$(field project-look-again redrawn)"; plain="$(field project-look-again-keys redrawn)"
    objects="$(field project-look-again objects)"
    if [ -n "$objects" ] && [ "$plain" = "$((objects * 2))" ] && [ "$drawn" = "$((objects * 3))" ]; then
        echo "project-look-again redraws every object for the other profile, the save of a key none: ok"
    else
        echo "project-look-again: FAILED, $objects objects were redrawn $drawn times for another profile and $plain times for a key"
        failed=1
    fi
fi
# The number of local test clients is set in the Project settings, in the Multiplayer group,
# with a button for each number from 1 to 4, and Play starts that many clients beside the one
# server. The game is the stand-in: no real game is started. A folder is set up, 3 is chosen
# and saved, and a run started from the editor has three clients, each a process of its own
# with its number, and each leaves with status 0 when the run is stopped. Started again on
# the folder, the editor shows 3 as the file's and starts three again. A project whose file
# names no number starts one client, and looking at the setting does not write a number.
CLIENTS_RUN="$PLAY,wait-run=running,click-button=Stop,wait-run=none"
THREE_CLIENTS='play_clients=3 play_client_ids=\[1,2,3\] play_client_pids=3 play_client_exits=\[1:0,2:0,3:0\]'
with_game runs check project-clients "click-button=Set up project,scroll-project=300,click-button=3,click-button=Save project settings,click-button=Close settings,$CLIENTS_RUN" \
    'project=Ready' 'project_draft=Unchanged project_saved=true' 'project_clients=3' \
    'play_end=play\.stopped' 'play_server_exit=0' "$THREE_CLIENTS" 'play_cleanup=\[\]' 'problem=none'
if wanted project-clients; then
    grep -q '"clients": 3' "$work/project-clients/project/project.aproj" \
        && grep -q '"bots": null' "$work/project-clients/project/project.aproj" \
        || { echo "project-clients: FAILED, the project file does not hold three clients and the planned settings as they were"; failed=1; }
fi
with_game runs check project-clients-again "$CLIENTS_RUN" \
    'project=Ready' 'project_draft=Unchanged project_saved=false' 'project_clients=3' \
    'play_end=play\.stopped' "$THREE_CLIENTS" 'problem=none'
with_game runs check project-clients-unnamed "click-button=Set up project,scroll-project=300,click-button=1,click-button=Close settings,$CLIENTS_RUN" \
    'project=Ready' 'project_draft=Unchanged project_saved=false' 'project_clients=unnamed' \
    'play_end=play\.stopped' 'play_clients=1 play_client_ids=\[1\] play_client_pids=1 play_client_exits=\[1:0\]' 'problem=none'
if wanted project-clients-unnamed; then
    grep -q '"clients": null' "$work/project-clients-unnamed/project/project.aproj" \
        || { echo "project-clients-unnamed: FAILED, the project file names a number nobody set"; failed=1; }
fi
# Closing the editor with project settings that were changed and not saved asks first, as
# it does for a level: the Project settings are closed with the change in them, and then the
# window. "Keep editing" leaves the editor open, the change still held and the project file
# as it was. Started again on the folder and changed again, "Discard changes" closes the
# editor without writing the file, and "Save and close" writes it and closes.
CHANGE_KEY='scroll-project=300,click-button=W,key=t,click-button=Close settings,close'
check project-close "click-button=Set up project,$CHANGE_KEY,click-button=Keep editing" \
    'project=Ready' 'project_dialog=false' 'question=No' 'shutdown=No' 'project_draft=Changed' \
    'project_keys=T,S,A,D,Space,V,Escape'
if wanted project-close; then
    grep -q '"move_forward": "W"' "$work/project-close/project/project.aproj" \
        || { echo "project-close: FAILED, the project file was written though nothing was saved"; failed=1; }
    cp "$work/project-close/project/project.aproj" "$work/project-close.before"
fi
# Leaving the project with settings that were changed and not saved asks as closing does:
# kept editing, the change is held and the file is as it was; discarded, the project is left
# and the file is not written; saved first, the file holds the change.
SWITCH_KEY='scroll-project=300,click-button=W,key=t,click-button=Close settings,choose=Level>Switch project'
check project-switch-settings "click-button=Set up project,key=ctrl+s,$SWITCH_KEY" \
    'question=Asking' 'launcher=off' 'project_draft=Changed'
check project-switch-settings-kept "click-button=Set up project,key=ctrl+s,$SWITCH_KEY,click-button=Keep editing" \
    'question=No' 'launcher=off' 'project=Ready' 'project_draft=Changed' 'project_keys=T,S,A,D,Space,V,Escape'
check project-switch-settings-discards "click-button=Set up project,key=ctrl+s,$SWITCH_KEY,click-button=Discard changes" \
    'question=No' 'launcher=shown' 'launcher_left=1' 'project=NoFolder'
check project-switch-settings-saves "click-button=Set up project,key=ctrl+s,$SWITCH_KEY,click-button=Save and leave" \
    'question=No' 'launcher=shown' 'launcher_left=1' 'project=NoFolder'
for name in project-switch-settings-kept project-switch-settings-discards; do
    if wanted "$name"; then
        grep -q '"move_forward": "W"' "$work/$name/project/project.aproj" \
            || { echo "$name: FAILED, the project file was written though nothing was saved"; failed=1; }
    fi
done
if wanted project-switch-settings-saves; then
    grep -q '"move_forward": "T"' "$work/project-switch-settings-saves/project/project.aproj" \
        || { echo "project-switch-settings-saves: FAILED, the project file does not hold the key that was saved"; failed=1; }
fi
# project_exits <name> <steps> <written: yes|no>: the editor, started on the folder of
# project-close, must end by itself with status 0 before the check's own end, and the
# project file must be as it was, or hold the key that was changed.
project_exits() {
    name="$1"; steps="$2"; written="$3"
    wanted "$name" || return 0
    folder="$work/${name%%-again*}"
    manifest="$folder/project/project.aproj"
    output="$(cd "$folder" && ARC_EDITOR_CHECK="$folder/$name.png" ARC_EDITOR_CHECK_STEPS="$steps" \
        limit 120 "$editor" 2>&1)"
    status=$?
    verdict=ok
    [ "$status" -eq 0 ] || verdict="FAILED, exit status $status"
    if printf '%s\n' "$output" | grep -q '^ARC_EDITOR_CHECK_RESULT'; then
        verdict="FAILED, the editor did not close"
    fi
    if [ "$written" = no ] && ! cmp -s "$manifest" "$work/project-close.before"; then
        verdict="FAILED, the project file was changed"
    fi
    if [ "$written" = yes ] && ! { grep -q '"move_forward": "T"' "$manifest" && grep -q '"jump": "Space"' "$manifest"; }; then
        verdict="FAILED, the project settings were not saved"
    fi
    echo "$name: $verdict"
    if [ "$verdict" != ok ]; then
        echo "  steps:  $steps"
        printf '%s\n' "$output" | grep 'check step\|^ARC_EDITOR_CHECK_RESULT' | cut -c1-240 | sed 's/^/  /'
        failed=1
    fi
}
project_exits project-close-again-discard "choose=Level>Project settings,$CHANGE_KEY,click-button=Discard changes" no
project_exits project-close-again-save "choose=Level>Project settings,$CHANGE_KEY,click-button=Save and close" yes
# The other ways out of a change to the project's settings, each on a folder of its own.
# Discarding in the dialog puts back what the file has and writes nothing. Jump put on the
# key of moving forward is held and cannot be saved: the Save button does nothing. The file
# is as it was after both.
SET_UP='click-button=Set up project,scroll-project=300'
check project-draft-discard "$SET_UP,click-button=W,key=t,click-button=Discard changes" \
    'project=Ready' 'project_draft=Unchanged project_saved=false' 'project_keys=W,S,A,D,Space,V,Escape'
check project-draft-shared "$SET_UP,click-button=Space,key=w,click-button=Save project settings" \
    'project=Ready' 'project_draft=SharedKey project_saved=false' 'project_keys=W,S,A,D,W,V,Escape'
for name in project-draft-discard project-draft-shared; do
    if wanted "$name"; then
        grep -q '"move_forward": "W"' "$work/$name/project/project.aproj" \
            && grep -q '"jump": "Space"' "$work/$name/project/project.aproj" \
            || { echo "$name: FAILED, the project file was written though nothing was saved"; failed=1; }
    fi
done
# renamed_meanwhile <name>: another program writes the project file of this case while the
# editor has it open. In the background: it waits for the editor to have set the folder up
# and read the file, and then writes the file with another project name. The steps that
# follow in the case wait about three seconds before they save, by putting the dialog's
# content where it is thirty times, so that the file is written first.
renamed_meanwhile() {
    wanted "$1" || return 0
    manifest="$work/$1/project/project.aproj"
    (
        tries=0
        while [ ! -s "$manifest" ] && [ "$tries" -lt 600 ]; do sleep 0.05; tries=$((tries + 1)); done
        sleep 0.2
        sed 's/"name": "[^"]*"/"name": "Renamed"/' "$manifest" > "$manifest.other" && mv "$manifest.other" "$manifest"
    ) &
}
# renamed_and <name> <key>: whether the project file of the case holds the other program's
# name and this key for moving forward.
renamed_and() {
    grep -q '"name": "Renamed"' "$work/$1/project/project.aproj" \
        && grep -q "\"move_forward\": \"$2\"" "$work/$1/project/project.aproj"
}
MEANWHILE=''
count=0
while [ "$count" -lt 30 ]; do MEANWHILE="${MEANWHILE}scroll-project=300,"; count=$((count + 1)); done
CHANGED_MEANWHILE="$SET_UP,click-button=W,key=t,$MEANWHILE"
# A save over a project file another program has written meanwhile is refused: the change
# stays unsaved and the file is the other program's. The dialog then offers to read the
# file again; read again, the change is still there and is saved onto what the other
# program wrote.
renamed_meanwhile project-draft-conflict
check project-draft-conflict "${CHANGED_MEANWHILE}click-button=Save project settings" \
    'project=Ready' 'project_draft=Changed project_saved=false' 'project_keys=T,S,A,D,Space,V,Escape'
if wanted project-draft-conflict; then
    renamed_and project-draft-conflict W \
        || { echo "project-draft-conflict: FAILED, the other program's project file was written over, or was not written in time"; failed=1; }
fi
renamed_meanwhile project-draft-reread
check project-draft-reread "${CHANGED_MEANWHILE}click-button=Save project settings,scroll-project=0,click-button=Read project file again,click-button=Save project settings" \
    'project=Ready' 'project_draft=Unchanged project_saved=true' 'project_keys=T,S,A,D,Space,V,Escape'
if wanted project-draft-reread; then
    renamed_and project-draft-reread T \
        || { echo "project-draft-reread: FAILED, the project file does not hold both the other program's name and the saved key"; failed=1; }
fi
# "Save and close" does not close over settings that cannot be saved: with two actions on
# one key, and with a project file another program has written meanwhile, the editor stays
# open, the Project settings are opened again to say why, and the file is not written.
check project-draft-close-shared "$SET_UP,click-button=Space,key=w,click-button=Close settings,close,click-button=Save and close" \
    'project=Ready' 'project_dialog=true' 'question=No' 'shutdown=No' 'project_draft=SharedKey project_saved=false'
if wanted project-draft-close-shared; then
    grep -q '"jump": "Space"' "$work/project-draft-close-shared/project/project.aproj" \
        || { echo "project-draft-close-shared: FAILED, the project file was written though nothing was saved"; failed=1; }
fi
renamed_meanwhile project-draft-close-conflict
check project-draft-close-conflict "${CHANGED_MEANWHILE}click-button=Close settings,close,click-button=Save and close" \
    'project=Ready' 'project_dialog=true' 'question=No' 'shutdown=No' 'project_draft=Changed project_saved=false'
if wanted project-draft-close-conflict; then
    renamed_and project-draft-close-conflict W \
        || { echo "project-draft-close-conflict: FAILED, the other program's project file was written over, or was not written in time"; failed=1; }
fi
# What of a project's own files stops the game. The game is started with the project's
# folder and opens there the level the project starts with and its look profile, so a project
# in which one of them is missing or cannot be used is not played: both ways to play are off,
# the Play menu's foot says what it is (`play_blocked=` is the refusal's code, with the code
# behind it after a colon), and F5 cooks nothing, starts nothing and opens the Project
# settings, where it is mended. Nothing is made or changed by looking or by the key. A
# stand-in is in the game's place throughout, so there is a game and only the project is in
# the way; the editor is started anew for each case, on the same folder.
PP="$work/play-project/project"
PP_LOOK="$PP/assets/render/look-v1.json"
PP_LEVEL="$PP/levels/first-level.level.json"
PP_RUN="$PLAY,wait-run=running,click-button=Stop,wait-run=none"
NOT_STARTED='play_run=None play_end=none play_refusal=none'
# How many files the folder's cooked levels are: F5 on a project that is not played cooks none.
pp_cooked() { find "$work/play-project" -type f -path '*cooked*' 2>/dev/null | wc -l | tr -d ' '; }
# A project as a setup makes it has all the game opens: it is played and stopped.
with_game runs check play-project "click-button=Set up project,click-button=Close settings,$PP_RUN" \
    'project=Ready' 'game=true' 'play_blocked=none' 'play_end=play\.stopped' 'problem=none'
# Its look profile is taken away. F5 opens the Project settings; nothing is cooked or started,
# and the profile is not made by that.
if wanted play-project-again-nolook; then
    cp "$PP_LOOK" "$work/play-project/look-kept.json" && rm -f "$PP_LOOK"
    pp_before="$(pp_cooked)"
fi
with_game runs check play-project-again-nolook 'key=f5' \
    'project=Ready' 'game=true' 'play_blocked=play\.project_profile_unavailable:look\.missing' \
    'project_dialog=true' "$NOT_STARTED"
if wanted play-project-again-nolook; then
    [ ! -e "$PP_LOOK" ] && [ "$(pp_cooked)" = "$pp_before" ] \
        || { echo "play-project-again-nolook: FAILED, the missing look profile was made, or a level was cooked for a project that is not played"; failed=1; }
fi
# Made on the person's word in the Project settings, the profile is there and the game is
# started in the same editor: the project is asked again when the profile is made.
with_game runs check play-project-again-look-made "choose=Level>Project settings,click-button=Make the default look profile,click-button=Close settings,$PP_RUN" \
    'project=Ready' 'play_blocked=none' 'play_end=play\.stopped' 'problem=none'
if wanted play-project-again-look-made; then
    cmp -s "$PP_LOOK" "$look" \
        || { echo "play-project-again-look-made: FAILED, the default look profile was not made"; failed=1; }
fi
# A file under the profile's name that is no look profile, and a link there that leads out of
# the project's folder: each is said as what it is, and the file is left as it is.
if wanted play-project-again-badlook; then
    printf 'not a look profile' > "$PP_LOOK"
fi
with_game runs check play-project-again-badlook 'key=f5' \
    'project=Ready' 'play_blocked=play\.project_profile_unavailable:look\.decode' \
    'project_dialog=true' "$NOT_STARTED"
if wanted play-project-again-badlook; then
    [ "$(cat "$PP_LOOK")" = 'not a look profile' ] \
        || { echo "play-project-again-badlook: FAILED, the file under the profile's name was changed"; failed=1; }
fi
if wanted play-project-again-outside-look; then
    rm -f "$PP_LOOK" && ln -s "$look" "$PP_LOOK"
fi
with_game runs check play-project-again-outside-look 'key=f5' \
    'project=Ready' 'play_blocked=play\.project_profile_unavailable:project\.outside_folder' \
    'project_dialog=true' "$NOT_STARTED"
# The profile is put back and the level the project starts with is taken away. The project
# is opened all the same, on an unsaved map with its settings open; closed, F5 opens them
# again, and the level's file is not made.
if wanted play-project-again-nolevel; then
    rm -f "$PP_LOOK" && cp "$work/play-project/look-kept.json" "$PP_LOOK"
    cp "$PP_LEVEL" "$work/play-project/level-kept.json" && rm -f "$PP_LEVEL"
    pp_before="$(pp_cooked)"
fi
with_game runs check play-project-again-nolevel 'click-button=Close settings,key=f5' \
    'project=Ready' 'has_file=false' 'play_blocked=play\.project_start_level_missing' \
    'project_dialog=true' "$NOT_STARTED"
if wanted play-project-again-nolevel; then
    [ ! -e "$PP_LEVEL" ] && [ "$(pp_cooked)" = "$pp_before" ] \
        || { echo "play-project-again-nolevel: FAILED, the missing level was made, or a level was cooked for a project that is not played"; failed=1; }
fi
# A file under the level's name that is no level: said as a level that cannot be used, under
# the reader's own refusal, and left as it is.
if wanted play-project-again-badlevel; then
    printf 'not a level' > "$PP_LEVEL"
fi
with_game runs check play-project-again-badlevel 'key=f5' \
    'project=Ready' 'play_blocked=source\.decode' 'project_dialog=true' "$NOT_STARTED" \
    'project_level=Some\("The level the project starts with cannot be used"\)'
if wanted play-project-again-badlevel; then
    [ "$(cat "$PP_LEVEL")" = 'not a level' ] \
        || { echo "play-project-again-badlevel: FAILED, the file under the level's name was changed"; failed=1; }
fi
# With the level put back the project is played again.
if wanted play-project-again-mended; then
    cp "$work/play-project/level-kept.json" "$PP_LEVEL"
fi
with_game runs check play-project-again-mended "$PP_RUN" \
    'project=Ready' 'has_file=true' 'play_blocked=none' 'play_end=play\.stopped' 'problem=none' \
    'project_level=None'
# A file that goes bad under an open editor, by another hand: the editor that found the
# project playable when it was read finds out when the Play menu is opened, without a key
# and without a start, and the Project settings that are opened then say it too. The level
# is put back afterwards.
if wanted play-project-again-spoiled-menu; then
    pp_before="$(pp_cooked)"
fi
with_game runs check play-project-again-spoiled-menu 'spoil-start-level,click-button=Play' \
    'project=Ready' 'play_blocked=source\.decode' 'menu_open=true' 'project_dialog=false' "$NOT_STARTED"
if wanted play-project-again-spoiled-menu; then
    [ "$(pp_cooked)" = "$pp_before" ] \
        || { echo "play-project-again-spoiled-menu: FAILED, opening the Play menu cooked something"; failed=1; }
    cp "$work/play-project/level-kept.json" "$PP_LEVEL"
fi
with_game runs check play-project-again-spoiled-settings 'spoil-start-level,choose=Level>Project settings' \
    'project=Ready' 'play_blocked=source\.decode' 'project_dialog=true' "$NOT_STARTED" \
    'project_level=Some\("The level the project starts with cannot be used"\)'
if wanted play-project-again-spoiled-settings; then
    cp "$work/play-project/level-kept.json" "$PP_LEVEL"
fi
unset ARC_EDITOR_CHECK_PROJECT

# The project browser. A person's editor starts in it; a scripted one only when the case
# names the file the list of projects is kept in (with_browser), which is in the case's own
# folder, never the list of whoever is at the machine. The desktop's folder chooser is not
# shown: `pick-folder=PATH` is what it answers the next time it is asked.
#
# with_browser <check function> <name> <steps> <expected>...
with_browser() {
    ARC_EDITOR_CHECK_LAUNCHER="$work/${2%%-again*}/projects.json"; export ARC_EDITOR_CHECK_LAUNCHER
    "$@"
    unset ARC_EDITOR_CHECK_LAUNCHER
}
NO_BROWSER_STATE='launcher_pending=none launcher_refusal=none launcher_list=ok'
# Without a list the browser is the window's screen, with nothing listed and no project; the
# editor's own keys do nothing behind it, and looking writes no list.
with_browser check browser-empty 'key=t,key=ctrl+s' \
    'launcher=shown launcher_rows=\[\] launcher_selected=none' "$NO_BROWSER_STATE" 'launcher_entered=0' \
    'project=NoFolder' 'sculpt=false' 'has_file=false' 'game=false' 'problem=none'
if wanted browser-empty; then
    [ ! -e "$work/browser-empty/projects.json" ] && [ -z "$(ls "$work/browser-empty/levels")" ] \
        || { echo "browser-empty: FAILED, looking at the browser wrote a list or a level"; failed=1; }
fi
# Choosing no folder changes nothing.
with_browser check browser-none 'pick-folder=,click-button=Open Project' \
    'launcher=shown launcher_rows=\[\]' "$NO_BROWSER_STATE" 'launcher_entered=0' 'project=NoFolder'
# A new project: the folder that is chosen is where the project's own folder goes, and it
# holds other things, as a person's documents do. The browser asks for the project's name and
# says where it will be made; nothing is written until the person says so. On that word the
# project is made in a folder named after it, and the editor is entered on it in the same
# window: the chosen folder is given that one folder and nothing else. The project has its
# file, its first level, which is the starter arena and not the game's old map, and its look
# profile, so Game look is read from the project at once. It is content only: no game folder
# is set for this editor, so there is nothing to play with, which is said without a flag;
# the level is cooked and checked all the same. The project is in the list.
DOCS="$work/browser-new/Documents"
if wanted browser-new; then
    mkdir -p "$DOCS" && : > "$DOCS/letter.txt"
fi
HARBOUR="$DOCS/Harbour"
ENTERED='launcher=off launcher_rows=\[Harbour:ready\]'
NO_RUNTIME='game_folder=unset game_refusal=none runtime=unset runtime_source=unset runtime_folder=none game_missing=\[\]'
with_browser check browser-new "pick-folder=$DOCS,click-button=New Project" \
    'launcher=shown launcher_rows=\[\] launcher_selected=none launcher_pending=new launcher_refusal=none' \
    'launcher_entered=0' 'game_folder=unset' 'runtime=none' 'project=NoFolder'
if wanted browser-new; then
    [ "$(ls -A "$DOCS")" = "letter.txt" ] && [ ! -e "$work/browser-new/projects.json" ] \
        && [ ! -e "$work/browser-new/game-folder.json" ] \
        || { echo "browser-new: FAILED, the chosen folder, the list or the game's setting was written before the person said so"; failed=1; }
fi
with_browser check browser-new-again-made "pick-folder=$DOCS,click-button=New Project,project-name=Harbour,click-button=Create project,cook,click-button=Play,click-item=Check cooked level" \
    "$ENTERED" "$NO_BROWSER_STATE" 'launcher_entered=1' 'project=Ready' 'project_dialog=false' \
    'has_file=true' 'game=false' "$NO_RUNTIME" 'play_cooked=[0-9a-f]{8}' 'play_check=passed' \
    'look=loaded:3abf9a2f' 'view=Editor' 'problem=none'
if wanted browser-new-again-made; then
    [ "$(ls -A "$DOCS" | tr '\n' ' ')" = "Harbour letter.txt " ] && [ ! -s "$DOCS/letter.txt" ] \
        && [ -f "$HARBOUR/project.aproj" ] && [ -f "$HARBOUR/assets/render/look-v1.json" ] \
        && [ -d "$HARBOUR/cooked" ] && [ ! -e "$work/browser-new/cooked" ] && [ ! -e "$DOCS/cooked" ] \
        && grep -q 'Harbour' "$work/browser-new/projects.json" \
        || { echo "browser-new-again-made: FAILED, the project, its look profile, its cooked level or the list is not where it belongs, or the chosen folder was given more than the project's folder"; failed=1; }
    grep -q '"North spawn 1"' "$HARBOUR"/levels/first-level.level.json \
        && ! grep -q '"Tall box"' "$HARBOUR"/levels/first-level.level.json \
        && ! grep -q '00000000000000000000000000000a7c' "$HARBOUR"/levels/first-level.level.json \
        || { echo "browser-new-again-made: FAILED, the first level is not the starter arena under an ID of its own"; failed=1; }
    cmp -s "$HARBOUR/assets/render/look-v1.json" "$look" \
        || { echo "browser-new-again-made: FAILED, the project's look profile is not the default one"; failed=1; }
fi
# Started again, the browser lists the project, chosen, and Enter opens it. A second project
# of that name is not made over it, nor is one made inside it: nothing is written, and the
# name that is taken is said.
with_browser check browser-new-again-listed 'key=t' \
    'launcher=shown launcher_rows=\[Harbour:ready\] launcher_selected=0' "$NO_BROWSER_STATE" 'launcher_entered=0' 'project=NoFolder'
with_browser check browser-new-again-opened 'key=enter' \
    "$ENTERED" 'launcher_entered=1' 'project=Ready' 'has_file=true' "$NO_RUNTIME" 'look=loaded:3abf9a2f' 'problem=none'
# From the open project the browser is shown again: the project that was left is listed and
# chosen, the editor holds no project behind it, and Enter goes back to the project.
with_browser check browser-new-again-left 'key=enter,choose=Level>Switch project' \
    'launcher=shown launcher_rows=\[Harbour:ready\] launcher_selected=0' "$NO_BROWSER_STATE" \
    'launcher_entered=1' 'launcher_left=1' 'question=No' 'project=NoFolder' 'has_file=false' 'runtime=none' 'problem=none'
with_browser check browser-new-again-back 'key=enter,choose=Level>Switch project,key=enter' \
    "$ENTERED" 'launcher_entered=2' 'launcher_left=1' 'project=Ready' 'has_file=true' 'look=loaded:3abf9a2f' 'problem=none'
# A project is opened by its file, in the browser and on the command line. A file that is no
# project file is refused in the browser, which names it; nothing is written either way.
with_browser check browser-new-again-file "pick-folder=$HARBOUR/project.aproj,click-button=Open Project" \
    "$ENTERED" 'launcher_entered=1' 'project=Ready' 'has_file=true' 'problem=none'
with_browser check browser-new-again-letter "pick-folder=$DOCS/letter.txt,click-button=Open Project" \
    'launcher=shown' 'launcher_refusal=project\.path_invalid' 'launcher_entered=0' 'project=NoFolder'
with_arg "$HARBOUR/project.aproj" with_browser check browser-new-again-cli 'key=t' \
    "$ENTERED" 'launcher_entered=1' 'project=Ready' 'has_file=true' 'problem=none'
# Without a list of its own a scripted editor enters the project all the same, and keeps no
# list: it never reads or writes the person's.
with_arg "$HARBOUR/project.aproj" check browser-new-again-cli-nolist 'key=t' \
    'launcher=off launcher_rows=\[\]' 'launcher_list=none' 'launcher_entered=1' 'project=Ready' 'has_file=true' 'problem=none'
with_arg "$DOCS/letter.txt" with_browser check browser-new-again-cli-refused 'key=t' \
    'launcher=shown' 'launcher_refusal=project\.path_invalid' 'launcher_entered=0' 'project=NoFolder'
if wanted browser-new-again-file || wanted browser-new-again-cli; then
    [ ! -s "$DOCS/letter.txt" ] && [ "$(ls -A "$DOCS" | tr '\n' ' ')" = "Harbour letter.txt " ] \
        || { echo "browser-new-again-file: FAILED, opening by file wrote into the chosen folder"; failed=1; }
fi
with_browser check browser-new-again-over "pick-folder=$DOCS,click-button=New Project,project-name=Harbour,click-button=Create project" \
    'launcher=shown' 'launcher_pending=new launcher_refusal=project\.folder_taken' 'launcher_entered=0' 'project=NoFolder'
with_browser check browser-new-again-inside "pick-folder=$HARBOUR,click-button=New Project" \
    'launcher=shown' 'launcher_pending=none launcher_refusal=already' 'launcher_entered=0' 'project=NoFolder'
if wanted browser-new-again-over || wanted browser-new-again-inside; then
    [ "$(ls -A "$DOCS" | tr '\n' ' ')" = "Harbour letter.txt " ] \
        && [ "$(ls -A "$HARBOUR" | tr '\n' ' ')" = "assets cooked levels project.aproj " ] \
        || { echo "browser-new-again-over: FAILED, a new project that was refused wrote something"; failed=1; }
fi
# A project whose look profile is not in its folder: it is entered with the Project settings
# opened, which say so, Game look is refused, and nothing is made by opening. On the
# person's word the default profile is made where the project file names it, the project
# file is not written, and Game look is read from the project at once.
if wanted browser-new-again-nolook; then
    rm -f "$HARBOUR/assets/render/look-v1.json"
    cp "$HARBOUR/project.aproj" "$work/browser-new/project-before.json"
fi
with_browser check browser-new-again-nolook 'key=enter' \
    "$ENTERED" 'launcher_entered=1' 'project=Ready' 'project_dialog=true' 'look=refused:look\.missing' 'problem=none'
if wanted browser-new-again-nolook; then
    [ ! -e "$HARBOUR/assets/render/look-v1.json" ] \
        || { echo "browser-new-again-nolook: FAILED, opening the project made its missing look profile"; failed=1; }
fi
with_browser check browser-new-again-nolook-made 'key=enter,click-button=Make the default look profile' \
    "$ENTERED" 'project=Ready' 'look=loaded:3abf9a2f' 'problem=none'
if wanted browser-new-again-nolook-made; then
    cmp -s "$HARBOUR/assets/render/look-v1.json" "$look" \
        && cmp -s "$HARBOUR/project.aproj" "$work/browser-new/project-before.json" \
        || { echo "browser-new-again-nolook-made: FAILED, the default look profile was not made, or the project file was written"; failed=1; }
fi
# The game's folder is the editor's one setting for every project, kept beside the list. A
# folder that holds no game is refused and nothing is kept. A folder that holds one is taken
# from the browser, and the project is then entered with that game: its folder is the game's,
# the project's stays the content's. From the open project the folder can be changed in the
# Play menu; a folder that is refused there leaves the setting and the game as they were.
GAME="$work/browser-new/game"
if wanted browser-new-again-game; then
    mkdir -p "$GAME/target/debug" "$GAME/assets/characters/pip"
    for program in server client; do
        printf '#!/bin/sh\nexit 0\n' > "$GAME/target/debug/$program" && chmod +x "$GAME/target/debug/$program"
    done
    # The player model a game folder must hold: the twelve bytes of a GLB file's header
    # that says it is twelve bytes long. It is whole as far as the editor looks, and no
    # model the game could draw.
    printf 'glTF\002\000\000\000\014\000\000\000' > "$GAME/assets/characters/pip/pip_robe_lod0.glb"
fi
with_browser check browser-new-again-game-refused "pick-folder=$DOCS,click-button=Choose game folder" \
    'launcher=shown' 'game_folder=unset game_refusal=runtime\.requirements_missing' 'runtime=none' 'launcher_entered=0'
if wanted browser-new-again-game-refused; then
    [ ! -e "$work/browser-new/game-folder.json" ] \
        || { echo "browser-new-again-game-refused: FAILED, a folder that holds no game was kept as the game's"; failed=1; }
fi
with_browser check browser-new-again-game-set "pick-folder=$GAME,click-button=Choose game folder" \
    'launcher=shown launcher_rows=\[Harbour:ready\]' 'game_folder=ready game_refusal=none' 'runtime=none' 'launcher_entered=0'
if wanted browser-new-again-game-set; then
    grep -q "browser-new/game" "$work/browser-new/game-folder.json" \
        || { echo "browser-new-again-game-set: FAILED, the game's folder was not kept"; failed=1; }
fi
with_browser check browser-new-again-game-opened 'key=enter' \
    "$ENTERED" 'launcher_entered=1' 'project=Ready' 'game=true' \
    'game_folder=ready game_refusal=none runtime=ready runtime_source=editor runtime_folder=[^ ]*browser-new/game game_missing=\[\]' 'problem=none'
with_browser check browser-new-again-game-kept "key=enter,pick-folder=$DOCS,click-button=Play,click-button=Change game folder" \
    "$ENTERED" 'game=true' \
    'game_folder=ready game_refusal=runtime\.requirements_missing runtime=ready runtime_source=editor runtime_folder=[^ ]*browser-new/game' 'problem=none'
if wanted browser-new-again-game; then
    [ "$(find "$GAME" -type f | wc -l | tr -d ' ')" = 3 ] && [ "$(ls -A "$GAME" | tr '\n' ' ')" = "assets target " ] \
        || { echo "browser-new-again-game: FAILED, something was written into the game's folder"; failed=1; }
fi
# The project's folder is moved away: it is still listed, as missing, and opens nothing.
# Opened where it is now, through the folder chooser, it is entered; taken out of the list,
# the list has only the project where it is.
if wanted browser-new-again-moved; then
    mv "$HARBOUR" "$work/browser-new/Quay"
fi
with_browser check browser-new-again-moved 'key=enter' \
    'launcher=shown launcher_rows=\[Harbour:missing\]' 'launcher_refusal=missing' 'launcher_entered=0' 'project=NoFolder'
with_browser check browser-new-again-moved-forgotten 'click-button=Remove from list' \
    'launcher=shown launcher_rows=\[\] launcher_selected=none' 'launcher_list=ok' 'launcher_entered=0'
with_browser check browser-new-again-moved-opened "pick-folder=$work/browser-new/Quay,click-button=Open Project" \
    "$ENTERED" 'launcher_entered=1' 'project=Ready' 'has_file=true' 'problem=none'
# The level the project starts with is taken out of its folder. The project is listed as one
# without its start level and opens all the same: on an unsaved map, with the Project
# settings opened to say what is missing. Opening does not make the level's file.
if wanted browser-new-again-nolevel; then
    rm -f "$work/browser-new/Quay/levels/"*.level.json
fi
with_browser check browser-new-again-nolevel 'key=enter' \
    'launcher=off launcher_rows=\[Harbour:nolevel\]' 'launcher_entered=1' 'project=Ready' 'project_dialog=true' \
    'has_file=false' 'problem=none'
if wanted browser-new-again-nolevel; then
    [ -z "$(ls "$work/browser-new/Quay/levels")" ] \
        || { echo "browser-new-again-nolevel: FAILED, opening the project made its missing level"; failed=1; }
fi
# A project's whole round in one run, all through its file: it is made, played and stopped,
# left for a second new project, and opened again by its `.aproj`. Both are listed, the
# first is the one the editor is on, and its cooked level is in its own folder.
CYCLE_DOCS="$work/project-cycle/Documents"
CYCLE="$CYCLE_DOCS/Cycle"
if wanted project-cycle; then
    mkdir -p "$CYCLE_DOCS"
fi
with_game runs with_browser check project-cycle "pick-folder=$CYCLE_DOCS,click-button=New Project,project-name=Cycle,click-button=Create project,$PLAY,wait-run=running,click-button=Stop,wait-run=none,$SWITCH,pick-folder=$CYCLE_DOCS,click-button=New Project,project-name=Other,click-button=Create project,$SWITCH,pick-folder=$CYCLE/project.aproj,click-button=Open Project" \
    'launcher=off launcher_rows=\[(Cycle|Other):ready,(Cycle|Other):ready\]' "$NO_BROWSER_STATE" \
    'launcher_entered=3' 'launcher_left=2' 'question=No' 'project=Ready' 'has_file=true' 'game=true' 'problem=none'
if wanted project-cycle; then
    [ -f "$CYCLE/project.aproj" ] && [ -f "$CYCLE_DOCS/Other/project.aproj" ] && [ -d "$CYCLE/cooked" ] \
        && [ ! -e "$CYCLE_DOCS/Other/cooked" ] \
        || { echo "project-cycle: FAILED, a project's file is missing, or a cooked level is not in the project that was played"; failed=1; }
fi
# An editor that is told what to open, as every other case of this file, has no browser.
check browser-off 'click-row=11' 'launcher=off launcher_rows=\[\]' 'launcher_list=none' 'selected=Some\("Tall box"\)'

# check_exit <name> <steps> <saved: yes|no>: the editor must end by itself before the check's
# own end, with status 0, and the level file must or must not have been written.
check_exit() {
    name="$1"; steps="$2"; saved="$3"
    wanted "$name" || return 0
    folder="$work/$name"
    mkdir -p "$folder/levels"
    output="$(cd "$folder" && ARC_EDITOR_CHECK="$folder/$name.png" ARC_EDITOR_CHECK_STEPS="$steps" \
        limit 120 "$editor" 2>&1)"
    status=$?
    verdict=ok
    [ "$status" -eq 0 ] || verdict="FAILED, exit status $status"
    if printf '%s\n' "$output" | grep -q '^ARC_EDITOR_CHECK_RESULT'; then
        verdict="FAILED, the editor did not close"
    fi
    if [ "$saved" = yes ] && [ ! -s "$folder/levels/current-map.level.json" ]; then
        verdict="FAILED, the level was not saved"
    fi
    if [ "$saved" = no ] && [ -e "$folder/levels/current-map.level.json" ]; then
        verdict="FAILED, the level was saved"
    fi
    echo "$name: $verdict"
    [ "$verdict" = ok ] || { echo "  steps:  $steps"; failed=1; }
}

# A new level that nobody saved counts as unsaved, so closing asks. Discard closes without
# writing; Save and close writes and then closes.
check_exit close-discard 'close,click-button=Discard changes' no
check_exit close-saves   'close,click-button=Save and close' yes
# With a game running the editor closes by itself once the game has stopped: after the
# question about the game, after the question about unsaved changes, and after "Close anyway".
with_game runs check_exit close-stops-game "key=ctrl+s,$PLAY,wait-run=running,close,click-button=Stop and close" yes
with_game runs check_exit close-discard-stops-game "$PLAY,wait-run=running,close,click-button=Discard changes" no
with_game server-stays check_exit close-anyway "key=ctrl+s,$PLAY,wait-run=running,close,click-button=Stop and close,wait-run=none,click-button=Close anyway" yes

# The browser holds no level of anyone's: closing the window there asks nothing, also after
# a project was left for it, and writes no level.
with_browser check_exit browser-closes 'close' no
check_exit project-switch-closes "key=ctrl+s,choose=Level>Switch project,close" yes

[ "$overridden" -eq 0 ] || echo "The desktop gave another window size than a case asked for: see NOT CHECKED AT above."
exit "$failed"
