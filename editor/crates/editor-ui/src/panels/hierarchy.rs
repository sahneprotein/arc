//! The hierarchy panel: the level's objects as a tree, in a list indented by depth.
//!
//! The rows come from the core's snapshot and are rebuilt when its hierarchy region changes.
//! Choosing a row asks the core to select that object; with Shift held the object joins the
//! selection or leaves it. The highlight follows the snapshot, never the click. Of several
//! selected rows, the one whose object the gizmo is on carries a dot.
//!
//! An object with children has a toggle that folds them away. What is folded is the panel's
//! own business: it is not part of the level and is forgotten with the session.
//!
//! A row is dragged to put its object under another: dropped on a row it goes under that
//! row's object, after its other children; dropped on the list where no row is, to the top of
//! the level. A dragged row that is selected takes the other selected objects with it, in the
//! list's order. Each stays where it is in the level, and the whole is one undo step. While a
//! row is dragged, the row or the list it would be dropped on is ringed. A drop the level
//! cannot take, an object under what is under it, is asked of the core all the same and
//! refused by it in words.

use std::collections::BTreeSet;

use bevy::{
    feathers::{
        containers::{pane, pane_body, pane_header},
        controls::{FeathersDisclosureToggle, FeathersListRow, FeathersListView},
        theme::{ThemeBackgroundColor, ThemedText, UiTheme},
        tokens,
    },
    picking::pointer::PointerButton,
    prelude::*,
    ui::{Checked, Selected},
    ui_widgets::ValueChange,
};
use editor_core::{GizmoPivot, HierarchyRow, ObjectId, StableId, ViewSnapshot};

use crate::{
    panels::inspector::{gizmo_object, gizmo_roots},
    session::Editor,
    theme::{size, spacing},
};

/// The hierarchy panel.
#[derive(Component, Default, Clone)]
pub struct HierarchyPanel;

/// The node the list is rebuilt in.
#[derive(Component, Default, Clone)]
pub struct HierarchyBody;

/// Where a dragged row would be dropped.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DropPlace {
    /// On the row of the object with this stable ID.
    Row(String),
    /// On the list where no row is.
    Top,
}

/// The row that is being dragged, and where the pointer has it now.
#[derive(Resource, Default, Clone, PartialEq, Eq, Debug)]
pub struct RowDrag {
    /// The stable ID of the dragged row's object.
    pub dragged: Option<String>,
    pub over: Option<DropPlace>,
}

/// What a drop means.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Dropped {
    /// These objects go under `parent`, or to the top of the level, in this order.
    Move {
        objects: Vec<ObjectId>,
        parent: Option<ObjectId>,
    },
    /// Nothing would change: the row was dropped on itself, or everything dragged is there
    /// already.
    Stays,
    /// The place is under one of the dragged objects. The level cannot take that; the core
    /// is asked and says so.
    Inside {
        objects: Vec<ObjectId>,
        parent: ObjectId,
    },
}

/// What dropping the row of `dragged` on the row of `onto`, or on the list (`None`), means.
///
/// What is moved is the dragged object, or every selected object if it is one of them: in the
/// order of the list, and without those under another moved one, which go with that one.
pub fn dropped(snapshot: &ViewSnapshot, dragged: &ObjectId, onto: Option<&ObjectId>) -> Dropped {
    let with_selection = snapshot.selection.contains(dragged);
    let wanted =
        |id: &ObjectId| id == dragged || (with_selection && snapshot.selection.contains(id));
    let mut moved: Vec<&HierarchyRow> = Vec::new();
    let mut inside = false;
    // The rows are in the core's order: each is followed by everything under it. Rows before
    // this one are under a moved row.
    let mut moved_until = 0;
    for (index, row) in snapshot.hierarchy.iter().enumerate() {
        if index < moved_until {
            inside |= onto == Some(&row.id);
        } else if wanted(&row.id) {
            moved_until = row.subtree_end_exclusive;
            moved.push(row);
        }
    }
    let objects: Vec<ObjectId> = moved.iter().map(|row| row.id.clone()).collect();
    match onto {
        Some(parent) if inside => Dropped::Inside {
            objects,
            parent: parent.clone(),
        },
        Some(parent) if objects.contains(parent) => Dropped::Stays,
        _ if moved.is_empty() || moved.iter().all(|row| row.parent.as_ref() == onto) => {
            Dropped::Stays
        }
        _ => Dropped::Move {
            objects,
            parent: onto.cloned(),
        },
    }
}

/// A row and the stable ID of the object it shows.
#[derive(Component, Default, Clone)]
pub struct RowObject(pub String);

/// The dot of a row, shown while the gizmo of several selected objects is on its object.
#[derive(Component, Default, Clone)]
pub struct GizmoMark(pub String);

/// The toggle of a row whose object has children, and that object's stable ID.
#[derive(Component, Default, Clone)]
pub struct RowToggle(pub String);

/// The objects whose children are folded away, by stable ID.
#[derive(Resource, Default)]
pub struct Collapsed {
    pub objects: BTreeSet<String>,
    /// The core's session these belong to; a new level starts with nothing folded.
    session: u64,
}

/// Which rows are folded away: every row under a collapsed one, however deep. The rows are
/// in the core's order, each followed by its descendants up to `subtree_end_exclusive`.
pub fn folded(rows: &[HierarchyRow], collapsed: &BTreeSet<String>) -> Vec<bool> {
    let mut hidden = vec![false; rows.len()];
    for (index, row) in rows.iter().enumerate() {
        if collapsed.contains(&row.id.0) {
            let end = row.subtree_end_exclusive.min(rows.len());
            for folded in hidden.iter_mut().take(end).skip(index + 1) {
                *folded = true;
            }
        }
    }
    hidden
}

/// The width a toggle takes, kept free in rows without one so that names line up.
const TOGGLE_WIDTH: f32 = 12.0;

pub fn hierarchy_panel() -> impl Scene {
    bsn! {
        pane()
        HierarchyPanel
        Node { width: px(size::HIERARCHY_WIDTH), flex_shrink: 0.0 }
        Children [
            (pane_header() Children [ (Text("Hierarchy") ThemedText) ]),
            (
                pane_body()
                HierarchyBody
                Node { flex_grow: 1.0, min_height: px(0), padding: px(0) }
                on(begin_row_drag)
                on(drag_over)
                on(drag_away)
                on(drop_row)
                on(end_row_drag)
            ),
        ]
    }
}

/// The row an event belongs to: the row itself, or the row a part of it is in.
fn row_of(target: Entity, rows: &Query<&RowObject>, parents: &Query<&ChildOf>) -> Option<String> {
    std::iter::once(target)
        .chain(parents.iter_ancestors(target))
        .find_map(|entity| rows.get(entity).ok())
        .map(|row| row.0.clone())
}

/// Where on the list an event is: on a row, or where no row is.
fn place_of(target: Entity, rows: &Query<&RowObject>, parents: &Query<&ChildOf>) -> DropPlace {
    row_of(target, rows, parents).map_or(DropPlace::Top, DropPlace::Row)
}

fn begin_row_drag(
    start: On<Pointer<DragStart>>,
    rows: Query<&RowObject>,
    parents: Query<&ChildOf>,
    mut drag: ResMut<RowDrag>,
) {
    if start.button != PointerButton::Primary {
        return;
    }
    // A drag that begins where no row is drags nothing.
    drag.dragged = row_of(start.original_event_target(), &rows, &parents);
    drag.over = None;
}

fn drag_over(
    over: On<Pointer<DragOver>>,
    rows: Query<&RowObject>,
    parents: Query<&ChildOf>,
    mut drag: ResMut<RowDrag>,
) {
    if drag.dragged.is_none() {
        return;
    }
    let place = Some(place_of(over.original_event_target(), &rows, &parents));
    if drag.over != place {
        drag.over = place;
    }
}

/// The pointer left what it was over. If that was the last place noted, the row is over
/// nothing now; a place it has come to since stays.
fn drag_away(
    leave: On<Pointer<DragLeave>>,
    rows: Query<&RowObject>,
    parents: Query<&ChildOf>,
    mut drag: ResMut<RowDrag>,
) {
    let left = place_of(leave.original_event_target(), &rows, &parents);
    if drag.over.as_ref() == Some(&left) {
        drag.over = None;
    }
}

fn drop_row(
    drop: On<Pointer<DragDrop>>,
    rows: Query<&RowObject>,
    parents: Query<&ChildOf>,
    mut drag: ResMut<RowDrag>,
    mut collapsed: ResMut<Collapsed>,
    mut editor: NonSendMut<Editor>,
) {
    let Some(dragged) = drag.dragged.take() else {
        return;
    };
    drag.over = None;
    if drop.button != PointerButton::Primary {
        return;
    }
    let onto = row_of(drop.original_event_target(), &rows, &parents).map(StableId);
    match dropped(&editor.snapshot(), &StableId(dragged), onto.as_ref()) {
        Dropped::Move { objects, parent } => {
            // What was dropped shows: its new parent is unfolded.
            if let Some(parent) = &parent {
                collapsed.objects.remove(&parent.0);
            }
            editor.move_under(objects, parent);
        }
        // Asked of the core, which refuses it and says why.
        Dropped::Inside { objects, parent } => editor.move_under(objects, Some(parent)),
        Dropped::Stays => {}
    }
}

/// The drag is over, wherever it ended: outside the list nothing is dropped.
fn end_row_drag(_: On<Pointer<DragEnd>>, mut drag: ResMut<RowDrag>) {
    if drag.dragged.is_some() || drag.over.is_some() {
        *drag = RowDrag::default();
    }
}

/// The width of the ring round the place a dragged row would be dropped on, in logical
/// pixels, drawn inside it.
const DROP_RING_WIDTH: f32 = 2.0;

/// A row, or the list, that carries the ring of a place to drop on. Only what this panel
/// ringed is un-ringed by it.
#[derive(Component, Default, Clone)]
pub struct DropRing;

/// The place a dragged row would be dropped on and moved by it, if it is over one.
pub fn drop_target(snapshot: &ViewSnapshot, drag: &RowDrag) -> Option<DropPlace> {
    let dragged = StableId(drag.dragged.clone()?);
    let over = drag.over.clone()?;
    let onto = match &over {
        DropPlace::Row(id) => Some(StableId(id.clone())),
        DropPlace::Top => None,
    };
    matches!(
        dropped(snapshot, &dragged, onto.as_ref()),
        Dropped::Move { .. }
    )
    .then_some(over)
}

/// Rings the row, or the list, that the dragged row would be dropped on. A place a drop
/// would change nothing at, or that the level cannot take, is not ringed.
pub(crate) fn show_drop_target(
    mut commands: Commands,
    editor: NonSend<Editor>,
    drag: Res<RowDrag>,
    theme: Res<UiTheme>,
    rows: Query<(Entity, &RowObject, Has<DropRing>)>,
    body: Query<(Entity, Has<DropRing>), With<HierarchyBody>>,
    added: Query<(), Added<RowObject>>,
) {
    if !drag.is_changed() && added.is_empty() {
        return;
    }
    let target = drop_target(&editor.snapshot(), &drag);
    let ring = Outline {
        width: px(DROP_RING_WIDTH),
        offset: px(-DROP_RING_WIDTH),
        color: theme.color(&tokens::FOCUS_RING),
    };
    let mut show = |entity: Entity, wanted: bool, ringed: bool| {
        if wanted && !ringed {
            commands.entity(entity).insert((ring, DropRing));
        } else if !wanted && ringed {
            commands.entity(entity).remove::<(Outline, DropRing)>();
        }
    };
    for (entity, row, ringed) in &rows {
        let wanted = target == Some(DropPlace::Row(row.0.clone()));
        show(entity, wanted, ringed);
    }
    for (entity, ringed) in &body {
        show(entity, target == Some(DropPlace::Top), ringed);
    }
}

fn row(row: &HierarchyRow) -> impl Scene {
    let id = row.id.0.clone();
    let mark = id.clone();
    let name = row.name.clone();
    let indent = spacing::SPACE_4 + row.depth as f32 * spacing::SPACE_5;
    let toggle: Box<dyn SceneList> = if row.child_count > 0 {
        let id = id.clone();
        Box::new(bsn_list![(
            @FeathersDisclosureToggle
            RowToggle({id})
            Node { flex_shrink: 0.0 }
            on(|change: On<ValueChange<bool>>,
                toggles: Query<&RowToggle>,
                mut collapsed: ResMut<Collapsed>| {
                if let Ok(toggle) = toggles.get(change.source) {
                    // Checked is open: the children show.
                    if change.value {
                        collapsed.objects.remove(&toggle.0);
                    } else {
                        collapsed.objects.insert(toggle.0.clone());
                    }
                }
            })
        )])
    } else {
        Box::new(bsn_list![Node {
            width: px(TOGGLE_WIDTH),
            flex_shrink: 0.0
        }])
    };
    bsn! {
        @FeathersListRow
        RowObject({id})
        Node {
            column_gap: px(spacing::SPACE_2),
            padding: UiRect {
                left: px(indent),
                right: px(spacing::SPACE_4),
                top: px(spacing::SPACE_1),
                bottom: px(spacing::SPACE_1),
            },
        }
        Children [
            {toggle},
            (Text({name}) ThemedText),
            Node { flex_grow: 1.0 },
            (
                // The design's dot: eight pixels across, round, in the colour of the text.
                GizmoMark({mark})
                Node {
                    width: px(spacing::SPACE_4),
                    height: px(spacing::SPACE_4),
                    border_radius: BorderRadius::all(px(spacing::SPACE_2)),
                    flex_shrink: 0.0,
                    align_self: AlignSelf::Center,
                    display: Display::None,
                }
                ThemeBackgroundColor(tokens::TEXT_MAIN)
            ),
        ]
    }
}

/// The object whose row carries the dot: the one the gizmo is on, when several are selected
/// and the gizmo is on one of them and not at their middle.
pub fn marked_object(snapshot: &ViewSnapshot) -> Option<&ObjectId> {
    if snapshot.selection.len() < 2 {
        return None;
    }
    if snapshot.pivot == GizmoPivot::Center && gizmo_roots(snapshot).len() > 1 {
        return None;
    }
    gizmo_object(snapshot).map(|object| &object.id)
}

fn list(rows: Vec<impl Scene>) -> impl Scene {
    let rows: Box<dyn SceneList> = Box::new(rows);
    bsn! {
        @FeathersListView { @rows: {rows} }
        Node { flex_grow: 1.0, min_height: px(0) }
        on(click_row)
        on(choose_row)
    }
}

/// Rebuilds the list when the level's hierarchy changed.
pub(crate) fn show_hierarchy(
    mut commands: Commands,
    editor: NonSend<Editor>,
    body: Query<Entity, With<HierarchyBody>>,
    mut shown: Local<Option<(u64, u64)>>,
) {
    let Ok(body) = body.single() else {
        return;
    };
    let snapshot = editor.snapshot();
    let key = (snapshot.session, snapshot.regions.hierarchy);
    if *shown == Some(key) {
        return;
    }
    *shown = Some(key);
    let rows: Vec<_> = snapshot.hierarchy.iter().map(row).collect();
    commands
        .entity(body)
        .despawn_children()
        .queue_spawn_related_scenes::<Children>(bsn_list![list(rows)]);
}

/// Marks the rows of the selected objects. Runs when the selection changed or rows appeared.
pub(crate) fn show_selection(
    mut commands: Commands,
    editor: NonSend<Editor>,
    rows: Query<(Entity, &RowObject, Has<Selected>)>,
    mut marks: Query<(&GizmoMark, &mut Node)>,
    added: Query<(), Added<RowObject>>,
    mut shown: Local<Option<(u64, u64)>>,
) {
    let snapshot = editor.snapshot();
    let key = (snapshot.session, snapshot.regions.selection);
    if *shown == Some(key) && added.is_empty() {
        return;
    }
    *shown = Some(key);
    for (entity, object, marked) in &rows {
        let selected = snapshot.selection.iter().any(|id| id.0 == object.0);
        if selected && !marked {
            commands.entity(entity).insert(Selected);
        } else if !selected && marked {
            commands.entity(entity).remove::<Selected>();
        }
    }
    let gizmo_on = marked_object(&snapshot);
    for (mark, mut node) in &mut marks {
        let wanted = if gizmo_on.is_some_and(|id| id.0 == mark.0) {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != wanted {
            node.display = wanted;
        }
    }
}

/// Folds away the rows under a collapsed object and shows each toggle open or closed. Runs
/// when something was folded or unfolded, when rows appeared, and when the hierarchy changed.
pub(crate) fn fold_rows(
    mut commands: Commands,
    editor: NonSend<Editor>,
    mut collapsed: ResMut<Collapsed>,
    mut rows: Query<(&RowObject, &mut Node)>,
    toggles: Query<(Entity, &RowToggle, Has<Checked>)>,
    added: Query<(), Added<RowObject>>,
    mut shown: Local<Option<(u64, u64)>>,
) {
    let snapshot = editor.snapshot();
    if collapsed.session != snapshot.session {
        collapsed.session = snapshot.session;
        collapsed.objects.clear();
    }
    let key = (snapshot.session, snapshot.regions.hierarchy);
    if *shown == Some(key) && added.is_empty() && !collapsed.is_changed() {
        return;
    }
    *shown = Some(key);
    let hidden = folded(&snapshot.hierarchy, &collapsed.objects);
    for (object, mut node) in &mut rows {
        let Some(index) = snapshot
            .hierarchy
            .iter()
            .position(|row| row.id.0 == object.0)
        else {
            continue;
        };
        let wanted = if hidden[index] {
            Display::None
        } else {
            Display::Flex
        };
        if node.display != wanted {
            node.display = wanted;
        }
    }
    for (entity, toggle, open) in &toggles {
        let wanted = !collapsed.objects.contains(&toggle.0);
        if wanted && !open {
            commands.entity(entity).insert(Checked);
        } else if !wanted && open {
            commands.entity(entity).remove::<Checked>();
        }
    }
}

/// Whether a click adds to the selection instead of replacing it: with Shift held, or with
/// Control or Command, which mean the same in a list on one desktop or another.
pub fn adds_to_selection(keys: &ButtonInput<KeyCode>) -> bool {
    keys.any_pressed([
        KeyCode::ShiftLeft,
        KeyCode::ShiftRight,
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ])
}

/// A click on a row picks its object: alone, or with Shift into or out of the selection.
///
/// The toolkit's list tells of a click only when it lands on another row than the one it holds
/// as chosen, which it takes to be the first selected row. With several rows selected that is
/// not enough: a Shift-click on that row must take it out, and a plain click on it must leave
/// it alone in the selection. So the rows' clicks are read here, where the list is.
fn click_row(
    click: On<Pointer<Click>>,
    rows: Query<&RowObject>,
    parents: Query<&ChildOf>,
    keys: Res<ButtonInput<KeyCode>>,
    mut editor: NonSendMut<Editor>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let target = click.original_event_target();
    let row = std::iter::once(target)
        .chain(parents.iter_ancestors(target))
        .find_map(|entity| rows.get(entity).ok());
    if let Some(object) = row {
        editor.pick(StableId(object.0.clone()), adds_to_selection(&keys));
    }
}

/// The list's own word that a row was chosen: with the arrow keys, and after a click as well.
/// It selects that row alone, which after a plain click is what `click_row` has asked for
/// already. With Shift held it says nothing new: the click has been dealt with, and the
/// selection is not thrown away for the row the keyboard is on.
fn choose_row(
    change: On<ValueChange<Entity>>,
    rows: Query<&RowObject>,
    keys: Res<ButtonInput<KeyCode>>,
    mut editor: NonSendMut<Editor>,
) {
    if adds_to_selection(&keys) {
        return;
    }
    if let Ok(object) = rows.get(change.value) {
        editor.select(vec![StableId(object.0.clone())]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows as the core gives them: (name, depth), each followed by its descendants.
    fn rows(shape: &[(&str, u32)]) -> Vec<HierarchyRow> {
        (0..shape.len())
            .map(|index| {
                let depth = shape[index].1;
                let end = (index + 1..shape.len())
                    .find(|later| shape[*later].1 <= depth)
                    .unwrap_or(shape.len());
                HierarchyRow {
                    id: StableId(shape[index].0.to_owned()),
                    parent: None,
                    name: shape[index].0.to_owned(),
                    depth,
                    child_count: 0,
                    subtree_end_exclusive: end,
                }
            })
            .collect()
    }

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    /// The game's map as the core shows it, with the objects of these rows selected.
    fn map_with_selected(selected: &[usize]) -> ViewSnapshot {
        use editor_core::{Command, CommandRequest, EditorSession};
        let mut session = EditorSession::new(scene_data::current_map()).unwrap();
        let before = session.snapshot();
        let selection = selected
            .iter()
            .map(|row| before.hierarchy[*row].id.clone())
            .collect();
        session
            .submit(CommandRequest {
                request: 1,
                session: before.session,
                expected_revision: before.document_revision,
                expected_inspector_revision: None,
                command: Command::Select(selection),
            })
            .outcome
            .unwrap();
        (*session.snapshot()).clone()
    }

    fn id_of(snapshot: &ViewSnapshot, row: usize) -> ObjectId {
        snapshot.hierarchy[row].id.clone()
    }

    #[test]
    fn a_row_dropped_on_another_goes_under_it_and_on_the_list_to_the_top() {
        let snapshot = map_with_selected(&[]);
        let (wall, tall) = (id_of(&snapshot, 10), id_of(&snapshot, 11));
        assert_eq!(
            dropped(&snapshot, &tall, Some(&wall)),
            Dropped::Move {
                objects: vec![tall.clone()],
                parent: Some(wall.clone()),
            }
        );
        // Everything in the map is at the top already: dropped on the list, it stays.
        assert_eq!(dropped(&snapshot, &tall, None), Dropped::Stays);
        // And a row dropped on itself stays: a click that slipped is no drag.
        assert_eq!(dropped(&snapshot, &tall, Some(&tall)), Dropped::Stays);
    }

    #[test]
    fn a_selected_row_takes_the_other_selected_rows_with_it_in_the_lists_order() {
        // Selected in another order than the list's.
        let snapshot = map_with_selected(&[13, 11, 12]);
        let wall = id_of(&snapshot, 10);
        let moved = dropped(&snapshot, &id_of(&snapshot, 12), Some(&wall));
        assert_eq!(
            moved,
            Dropped::Move {
                objects: vec![
                    id_of(&snapshot, 11),
                    id_of(&snapshot, 12),
                    id_of(&snapshot, 13)
                ],
                parent: Some(wall.clone()),
            }
        );
        // A row that is not selected goes alone, and the selection is left as it is.
        assert_eq!(
            dropped(&snapshot, &id_of(&snapshot, 9), Some(&wall)),
            Dropped::Move {
                objects: vec![id_of(&snapshot, 9)],
                parent: Some(wall.clone()),
            }
        );
        // Dropped on one of the rows that are dragged: nothing changes.
        assert_eq!(
            dropped(
                &snapshot,
                &id_of(&snapshot, 12),
                Some(&id_of(&snapshot, 13))
            ),
            Dropped::Stays
        );
    }

    /// Rows as the core gives them for a small tree: a with b under it and c under b, and d.
    fn tree() -> ViewSnapshot {
        let mut snapshot = map_with_selected(&[]);
        let row = |name: &str, depth: u32, parent: Option<&str>, end: usize| HierarchyRow {
            id: StableId(name.to_owned()),
            parent: parent.map(|parent| StableId(parent.to_owned())),
            name: name.to_owned(),
            depth,
            child_count: 0,
            subtree_end_exclusive: end,
        };
        snapshot.hierarchy = vec![
            row("a", 0, None, 3),
            row("b", 1, Some("a"), 3),
            row("c", 2, Some("b"), 3),
            row("d", 0, None, 4),
        ]
        .into();
        snapshot.selection = Vec::new().into();
        snapshot
    }

    fn named(name: &str) -> ObjectId {
        StableId(name.to_owned())
    }

    #[test]
    fn a_drop_under_what_is_dragged_is_left_to_the_core_to_refuse() {
        let snapshot = tree();
        assert_eq!(
            dropped(&snapshot, &named("a"), Some(&named("c"))),
            Dropped::Inside {
                objects: vec![named("a")],
                parent: named("c"),
            }
        );
        // Under where it is already: nothing changes. Out of it to the top: a move.
        assert_eq!(
            dropped(&snapshot, &named("b"), Some(&named("a"))),
            Dropped::Stays
        );
        assert_eq!(
            dropped(&snapshot, &named("c"), None),
            Dropped::Move {
                objects: vec![named("c")],
                parent: None,
            }
        );
    }

    #[test]
    fn of_two_dragged_rows_one_under_the_other_only_the_upper_is_moved() {
        let mut snapshot = tree();
        snapshot.selection = vec![named("a"), named("c")].into();
        assert_eq!(
            dropped(&snapshot, &named("c"), Some(&named("d"))),
            Dropped::Move {
                objects: vec![named("a")],
                parent: Some(named("d")),
            }
        );
    }

    #[test]
    fn only_a_place_a_drop_would_move_something_to_is_ringed() {
        let snapshot = tree();
        let drag = |dragged: &str, over: Option<DropPlace>| RowDrag {
            dragged: Some(dragged.to_owned()),
            over,
        };
        let row = |name: &str| Some(DropPlace::Row(name.to_owned()));
        assert_eq!(drop_target(&snapshot, &drag("c", row("d"))), row("d"));
        assert_eq!(
            drop_target(&snapshot, &drag("c", Some(DropPlace::Top))),
            Some(DropPlace::Top)
        );
        // On itself, where it is already, under itself, and over nothing: no ring.
        assert_eq!(drop_target(&snapshot, &drag("c", row("c"))), None);
        assert_eq!(drop_target(&snapshot, &drag("c", row("b"))), None);
        assert_eq!(drop_target(&snapshot, &drag("a", row("c"))), None);
        assert_eq!(
            drop_target(&snapshot, &drag("d", Some(DropPlace::Top))),
            None
        );
        assert_eq!(drop_target(&snapshot, &drag("c", None)), None);
        assert_eq!(drop_target(&snapshot, &RowDrag::default()), None);
    }

    #[test]
    fn nothing_is_folded_when_nothing_is_collapsed() {
        let rows = rows(&[("a", 0), ("b", 1), ("c", 0)]);
        assert_eq!(folded(&rows, &set(&[])), [false, false, false]);
    }

    #[test]
    fn a_collapsed_row_stays_and_everything_under_it_goes_however_deep() {
        let rows = rows(&[("a", 0), ("b", 1), ("c", 2), ("d", 1), ("e", 0)]);
        assert_eq!(
            folded(&rows, &set(&["a"])),
            [false, true, true, true, false]
        );
        assert_eq!(
            folded(&rows, &set(&["b"])),
            [false, false, true, false, false]
        );
    }

    #[test]
    fn a_row_folded_by_an_ancestor_stays_folded_when_its_own_children_are_open() {
        let rows = rows(&[("a", 0), ("b", 1), ("c", 2)]);
        assert_eq!(folded(&rows, &set(&["a", "b"])), [false, true, true]);
    }

    #[test]
    fn a_collapsed_object_that_is_gone_folds_nothing() {
        let rows = rows(&[("a", 0), ("b", 1)]);
        assert_eq!(folded(&rows, &set(&["gone"])), [false, false]);
    }
}
