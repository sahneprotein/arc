//! The hierarchy panel: the level's objects as a tree, in a list indented by depth.
//!
//! The rows come from the core's snapshot and are rebuilt when its hierarchy region changes.
//! Choosing a row asks the core to select that object; with Shift held the object joins the
//! selection or leaves it. The highlight follows the snapshot, never the click.
//!
//! An object with children has a toggle that folds them away. What is folded is the panel's
//! own business: it is not part of the level and is forgotten with the session.

use std::collections::BTreeSet;

use bevy::{
    feathers::{
        containers::{pane, pane_body, pane_header},
        controls::{FeathersDisclosureToggle, FeathersListRow, FeathersListView},
        theme::ThemedText,
    },
    picking::pointer::PointerButton,
    prelude::*,
    ui::{Checked, Selected},
    ui_widgets::ValueChange,
};
use editor_core::{HierarchyRow, StableId};

use crate::{
    session::Editor,
    theme::{size, spacing},
};

/// The hierarchy panel.
#[derive(Component, Default, Clone)]
pub struct HierarchyPanel;

/// The node the list is rebuilt in.
#[derive(Component, Default, Clone)]
pub(crate) struct HierarchyBody;

/// A row and the stable ID of the object it shows.
#[derive(Component, Default, Clone)]
pub struct RowObject(pub String);

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
            ),
        ]
    }
}

fn row(row: &HierarchyRow) -> impl Scene {
    let id = row.id.0.clone();
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
        Children [ {toggle}, (Text({name}) ThemedText) ]
    }
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
