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
set -u
# The editor to check: the dev build, or the one named by ARC_EDITOR_BIN.
editor="${ARC_EDITOR_BIN:-$(cd "$(dirname "$0")/../../.." && pwd)/target/debug/arc-editor}"
# Each case's level folder, pictures and cooked packages go under a temporary folder that is
# removed at the end: in ARC_EDITOR_TMP if that is set, else in the system's.
work="$(mktemp -d -p "${ARC_EDITOR_TMP:-${TMPDIR:-/tmp}}")"
trap 'rm -rf "$work"' EXIT
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
        timeout 180 "$editor" 2>&1 | grep '^ARC_EDITOR_CHECK_RESULT')"
    # Kept for the comparisons between cases further down, and in ARC_EDITOR_RESULTS if set.
    printf '%s\n' "$result" > "$work/$name.result"
    [ -z "${ARC_EDITOR_RESULTS:-}" ] || printf '%s\n' "$result" > "$ARC_EDITOR_RESULTS/$name.result"
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
    ARC_EDITOR_CHECK_REACTIVE=1 check "$@"
}

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
check tool-button     'click-button=Scale'                'tool=Scale'
# The two panel buttons each flip their panel, whatever the window's width shows by default.
check panel-buttons   'inspector,click-button=Hierarchy,click-button=Inspector' 'panels=\(true, false\)'
check camera          'drag-camera=-80:30,wheel=3'        'camera=\(1\.080,0\.730,26\.60\)'
check toolbar         'click-row=11,drag-gizmo=-60,click-button=Undo,click-button=Redo,click-button=Save level,click-button=Cook' \
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
check add-landscape   'click-button=Add landscape'        'objects=23' 'ground_tiles=4' 'undo=Some\("create"\)' 'problem=none'
check undo-landscape  'click-button=Add landscape,key=ctrl+z' 'objects=22' 'ground_tiles=0' 'redo=Some\("create"\)'

# The landscape tool: T chooses it, its panel takes the inspector's place, a mode button
# chooses the brush, and the brush points at the ground under the cursor.
check sculpt-key      'key=t'                             'sculpt=true'
check sculpt-leaves   'key=t,key=w'                       'sculpt=false' 'tool=Translate'
check brush-mode      'inspector,click-button=Sculpt,click-button=Smooth' \
                      'sculpt=true' 'brush=\(Smooth'
check brush-aims      'click-button=Add landscape,key=t,aim-ground=2:4' \
                      'brush_aim=Some\(Vec3\((1\.9[0-9]?|2\.0[0-9]?), 0\.0, (3\.9[0-9]?|4\.0[0-9]?)\)\)'
# Without a landscape under the cursor the brush points at nothing.
check brush-off-ground 'key=t,aim-ground=2:4'              'brush_aim=None'
# A stroke raises the ground and is one undo step; undoing it leaves the flat landscape.
stroke='click-button=Add landscape,key=t,stroke=-4:0:4:4'
flat='ground_height=\(0\.000,0\.000\)'
check stroke-raises   "$stroke"                           'ground_height=\(0\.000,0\.[1-9]' 'undo=Some\("sculpt"\)' 'stroke_active=false' 'dragging=false'
check stroke-undo     "$stroke,key=ctrl+z"                "$flat" 'objects=23' 'undo=Some\("create"\)' 'redo=Some\("sculpt"\)'
check stroke-lower    'click-button=Add landscape,inspector,click-button=Sculpt,click-button=Lower,stroke=-4:0:-2:2' \
                      'ground_height=\(-0\.[1-9][0-9]*,0\.000\)' 'brush=\(Lower'
# Escape, and the cursor leaving the window, cancel a stroke and put the ground back.
check stroke-escape   'click-button=Add landscape,key=t,hold-stroke=-4:0:4:4,key=escape,release' "$flat" 'undo=Some\("create"\)' 'dragging=false' 'stroke_active=false'
check stroke-leave    'click-button=Add landscape,key=t,hold-stroke=-4:0:4:4,leave,release' "$flat" 'undo=Some\("create"\)' 'dragging=false' 'stroke_active=false'

# A click on the ground chooses the landscape.
check pick-landscape  'click-button=Add landscape,click-ground=2:4' 'selected=Some\("Landscape"\)'
# The tie between landscape and slab, away from the tie: beside the landscape the slab is
# chosen; where the ground was raised the landscape is; where it was dug below the slab's top
# the slab covers it and is chosen.
check pick-slab-beside 'click-button=Add landscape,click-ground=-4:-20' 'selected=Some\("Ground"\)'
check pick-raised     'click-button=Add landscape,key=t,stroke=-4:0:4:4,key=w,click-ground=0:2' 'selected=Some\("Landscape"\)' 'ground_height=\(0\.000,0\.[1-9]'
check pick-dug        'click-button=Add landscape,inspector,click-button=Sculpt,click-button=Lower,stroke=-4:0:4:4,key=w,click-ground=0:2' 'selected=Some\("Ground"\)' 'ground_height=\(-0\.[1-9]'
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
check new-saved       'key=ctrl+n,type=7,key=enter,click-button=Discard changes,click-button=Add landscape,key=ctrl+s' 'level="7"' 'objects=1' 'has_file=true' 'dirty=false' 'empty_hint=false' 'problem=none'
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
check add-box         'click-button=Add box' 'objects=23' 'selected=Some\("Box 15"\)' 'position=Some\(\[0\.0, 0\.5, 2\.0\]\)' 'half_size=Some\(\[0\.5, 0\.5, 0\.5\]\)' 'undo=Some\("create"\)'
check add-box-undo    'click-button=Add box,key=ctrl+z' 'objects=22' 'redo=Some\("create"\)'
check add-spawn       'click-button=Add spawn' 'objects=23' 'selected=Some\("Spawn 9"\)' 'position=Some\(\[0\.0, 2\.0, 2\.0\]\)'
check remove-button   'click-row=11,click-button=Remove' 'objects=21' 'selected=None' 'undo=Some\("delete"\)'
check delete-key      'click-row=11,key=delete' 'objects=21' 'undo=Some\("delete"\)'
check delete-undo     'click-row=11,key=delete,key=ctrl+z' 'objects=22' 'redo=Some\("delete"\)'
# The landscape's layers, in the Paint part of the landscape tool.
L='click-button=Add landscape,inspector,click-button=Paint'
check paint-tool      "$L" 'paint=true' 'layers=0' 'paint_layer=None'
check layer-add       "$L,click-button=Add layer" 'layers=1' 'paint_layer=Some\("Grass"\)' 'undo=Some\("layers"\)' 'problem=none'
check layer-add-two   "$L,click-button=Add layer,click-button=Add layer" 'layers=2' 'paint_layer=Some\("Dirt"\)' 'problem=none'
check layer-choose    "$L,click-button=Add layer,click-button=Add layer,click-layer=1" 'layers=2' 'paint_layer=Some\("Grass"\)'
check layer-rename    "$L,click-button=Add layer,type-layer=42" 'layers=1' 'paint_layer=Some\("42"\)' 'problem=none'
check layer-remove    "$L,click-button=Add layer,click-button=Add layer,click-button=Remove layer" 'layers=1' 'paint_layer=Some\("Grass"\)' 'problem=none'
check layer-remove-first "$L,click-button=Add layer,click-button=Add layer,click-layer=1,click-button=Remove layer" 'layers=1' 'paint_layer=Some\("Dirt"\)' 'problem=none'
check layer-undo      "$L,click-button=Add layer,key=ctrl+z" 'layers=0' 'paint_layer=None' 'redo=Some\("layers"\)'
check layer-no-ground 'inspector,click-button=Paint,click-button=Add layer' 'layers=0' 'problem=none' 'objects=22'
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
A='click-button=Add landscape'
check place-flat      "$A,click-button=Add box" 'selected=Some\("Box 15"\)' 'position=Some\(\[-0\.92[0-9]*, 0\.5, 0\.65[0-9]*\]\)' 'problem=none'
check place-spawn     "$A,click-button=Add spawn" 'selected=Some\("Spawn 9"\)' 'position=Some\(\[-0\.92[0-9]*, 2\.0, 0\.65[0-9]*\]\)'
# The line of sight passes beside the landscape: the object goes to the point the view turns
# about, on height 0, as without a landscape.
check place-miss      "$A,look-at=40:1:2,click-button=Add box" 'position=Some\(\[40\.0, 0\.5, 2\.0\]\)' 'problem=none'
# A landscape turned 10 degrees about X: the surface there is at -tan(10 deg) * 0.528 = -0.093.
check place-tilted    "$A,tilt=22:10,click-button=Add box" 'position=Some\(\[-1\.00[0-9]*, 0\.40[0-9]*, 0\.52[0-9]*\]\)' 'problem=none'
# Ground raised by a stroke under the view: the box stands on the raised ground.
check place-raised    "$A,key=t,stroke=-2:2:2:6,key=w,click-button=Add box" 'position=Some\(\[-0\.[0-9]+, 0\.9[0-9]*, 1\.[0-9]+\]\)' 'ground_height=\(0\.000,0\.[1-9]' 'undo=Some\("create"\)'
# The landscape's rules. Grass: a rule scatters it, four tries in each of the landscape's 1024
# cells; the blades drawn come from the core's geometry.
R='click-button=Add landscape,inspector,click-button=Rules'
check rules-tool      "$R" 'tool=Translate' 'rules=\[\]' 'grass=0' 'problem=none'
check rule-grass      "$R,click-button=Add grass" 'rules=\["Grass everywhere"\]' 'grass=4096' 'undo=Some\("rule"\)' 'problem=none'
check rule-grass-undo "$R,click-button=Add grass,key=ctrl+z" 'rules=\[\]' 'grass=0' 'redo=Some\("rule"\)'
check rule-remove     "$R,click-button=Add grass,click-button=Remove rule" 'rules=\[\]' 'grass=0' 'undo=Some\("rule"\)'
# A typed setting is one undo step and changes what grows: half the density, about half the grass.
check rule-density    "$R,click-button=Add grass,type-rule=0:0.5" 'rule=Some\(\[0\.5, 4\.0, 0\.4, 0\.08, 1\.0\]\)' 'grass=(19|20|21)[0-9][0-9] ' 'problem=none'
# With layers, grass grows on the first one and less where another is painted over it; a
# layer that keeps it out takes more away; on a layer that is nowhere nothing grows.
G="click-button=Add landscape,inspector,click-button=Paint,click-button=Add layer,click-button=Add layer"
check rule-on-layer   "$G,stroke=-6:-2:6:6,click-button=Rules,click-button=Add grass" 'rules=\["Grass on Grass"\]' 'grass=3[78][0-9][0-9] ' 'problem=none'
check rule-kept-out   "$G,stroke=-6:-2:6:6,click-button=Rules,click-button=Add grass,step-rule-layer=kept-out-by" 'rules=\["Grass on Grass"\]' 'grass=3[56][0-9][0-9] ' 'problem=none'
check rule-on-empty   "$G,click-button=Rules,click-button=Add grass,step-rule-layer=grows-on" 'rules=\["Grass on Dirt"\]' 'grass=0 ' 'problem=none'
check rule-everywhere "$G,click-button=Rules,click-button=Add grass,step-rule-layer=grows-on,step-rule-layer=grows-on" 'rules=\["Grass everywhere"\]' 'grass=4096'
# A layer that fills itself on steep ground comes with its rule, as one undo step. On flat
# ground it stays empty, and the panel says why.
check rule-steep      "$R,click-button=Add steep layer" 'rules=\["Rock by steepness"\]' 'layers=2' 'layer_weights=\[1\.0, -1\.0\]' 'filled=\[0\.0\]' 'flat_note=below-band' 'undo=Some\("rule"\)' 'problem=none'
check rule-steep-undo "$R,click-button=Add steep layer,key=ctrl+z" 'rules=\[\]' 'layers=0'
# A band that starts at flat still leaves perfectly flat ground empty while it fades in, and
# the panel says so; with the fade at 0 flat ground is filled.
check rule-flat-fades "$R,click-button=Add steep layer,type-rule=0:0" 'rule=Some\(\[0\.0, 1\.0, 0\.1,' 'filled=\[0\.0\]' 'flat_note=fades-in'
check rule-flat-in    "$R,click-button=Add steep layer,type-rule=0:0,type-rule=2:0" 'rule=Some\(\[0\.0, 1\.0, 0\.0,' 'filled=\[1\.0\]' 'flat_note=none'
# On a hill three metres high the layer fills on the flanks.
check rule-hill       'click-button=Add landscape,key=t,stroke=0:2:1:2,stroke=0:2:1:2,stroke=0:2:1:2,stroke=0:2:1:2,stroke=0:2:1:2,stroke=0:2:1:2,inspector,click-button=Rules,click-button=Add steep layer' 'ground_height=\(0\.000,[23]\.' 'filled=\[0\.0[0-9]*[1-9][0-9]*\]' 'problem=none'
# A setting the core refuses leaves the rule as it was and is said in the notice.
check rule-refused    "$R,click-button=Add steep layer,type-rule=1:0.1" 'problem=rule\.parameter_range' 'notice=true' 'rule=Some\(\[0\.25, 1\.0,'
# A layer a rule fills cannot be painted, and removing it takes the rule with it.
check rule-no-paint   "$R,click-button=Add steep layer,click-button=Paint,click-layer=2,stroke=-4:0:4:4" 'problem=layer\.derived_output' 'notice=true' 'undo=Some\("rule"\)' 'dragging=false' 'stroke_active=false'
check rule-layer-gone "$R,click-button=Add steep layer,click-button=Paint,click-layer=2,click-button=Remove layer" 'rules=\[\]' 'layers=1' 'problem=none' 'undo=Some\("layers"\)'
# Two rules: the second is chosen when added, and a click chooses the first again.
check rule-two        "$R,click-button=Add grass,click-button=Add steep layer,click-rule=1" 'rules=\["Grass everywhere", "Rock by steepness"\]' 'rule=Some\(\[1\.0, 4\.0,' 'grass=4096'
# Round trip through the UI's own jobs. One editor sculpts a hill, makes two layers, paints
# the second, adds a grass rule and a steep-ground rule (with its layer), saves and cooks. A
# fresh editor in the same folder opens that level file at start and must hold the same
# landscape: the digest covers every height, layer, painted weight and rule.
RT='click-button=Add landscape,key=t,stroke=0:2:1:2,stroke=0:2:1:2,inspector,click-button=Paint,click-button=Add layer,click-button=Add layer,stroke=-6:-2:6:6,click-button=Rules,click-button=Add grass,click-button=Add steep layer,key=ctrl+s,cook'
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
check roundtrip-again-paint 'inspector,click-button=Paint,click-layer=2,stroke=-6:6:6:-2,key=ctrl+s,cook' 'undo=Some\("paint"\)' 'dirty=false' 'cook_needed=false' 'problem=none'
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
check_reactive reactive-add-box 'click-button=Add box' 'objects=23' 'rows_shown=23' 'selected=Some\("Box 15"\)'
check_reactive reactive-undo    'click-button=Add box,key=ctrl+z' 'objects=22' 'rows_shown=22'
check_reactive reactive-layers  'click-button=Add landscape,inspector,click-button=Paint,click-button=Add layer,click-button=Add layer' 'layers=2' 'layer_rows=2' 'ground_tiles=4'
check_reactive reactive-rules   'click-button=Add landscape,inspector,click-button=Rules,click-button=Add grass' 'rules=\["Grass everywhere"\]' 'rule_rows=1' 'grass=4096'
check_reactive reactive-rename  'click-row=11,inspector,type-name=42' 'selected=Some\("42"\)'
# The brush finds the ground on the corners of its cells too. A ray exactly through a corner
# can slip between the triangles; the brush then looks a fraction of a pixel aside.
check aim-on-corners  'click-button=Add landscape,key=t,aim-ground=3:-3' 'brush_aim=Some\(Vec3\((2\.9[0-9]*|3\.0[0-9]*), 0\.0, -(2\.9[0-9]*|3\.0[0-9]*)\)\)'
check aim-on-corner-2 'click-button=Add landscape,key=t,aim-ground=4:-4' 'brush_aim=Some\(Vec3\((3\.9[0-9]*|4\.0[0-9]*), 0\.0, -(3\.9[0-9]*|4\.0[0-9]*)\)\)'
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
        timeout 180 "$editor" 2>&1 | grep '^ARC_EDITOR_FIRST_FRAME')"
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
check dialog-escape   'click-button=Open level,key=escape' 'level_dialog=Closed'
check dialog-cancel   'click-button=New level,click-button=Cancel' 'level_dialog=Closed' 'objects=22'
check dialog-modal    'click-row=11,drag-gizmo=-60,key=ctrl+o,key=t,key=ctrl+z' 'level_dialog=Open' 'sculpt=false' 'undo=Some\("drag"\)'
# check_exit <name> <steps> <saved: yes|no>: the editor must end by itself before the check's
# own end, with status 0, and the level file must or must not have been written.
check_exit() {
    name="$1"; steps="$2"; saved="$3"
    wanted "$name" || return 0
    folder="$work/$name"
    mkdir -p "$folder/levels"
    output="$(cd "$folder" && ARC_EDITOR_CHECK="$folder/$name.png" ARC_EDITOR_CHECK_STEPS="$steps" \
        timeout 120 "$editor" 2>&1)"
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

exit "$failed"
