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

# wanted <name>: whether the case is to be run.
wanted() {
    case "$1" in *"$only"*) return 0 ;; *) return 1 ;; esac
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
        limit 180 "$editor" 2>&1 | grep '^ARC_EDITOR_CHECK_RESULT')"
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
    for expected in "$@" 'failed_steps=\[\]'; do
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

# The Play control. It is in the toolbar at every width. Nothing can be played yet: the two
# ways to play are in its menu and are off, and choosing one does nothing. Its foot names the
# cooked level and offers to cook; a cooked level can be checked on disk by the editor's core.
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

[ "$overridden" -eq 0 ] || echo "The desktop gave another window size than a case asked for: see NOT CHECKED AT above."
exit "$failed"
