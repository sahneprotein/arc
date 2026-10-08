//! The toolbar: the actions that work today. A button is added when what it does exists.

use bevy::{
    feathers::{
        containers::pane_header_divider,
        controls::{ButtonVariant, FeathersButton},
        theme::{ThemeBackgroundColor, ThemedText},
        tokens,
    },
    gizmos::transform_gizmo::{TransformGizmoMode, TransformGizmoSettings},
    prelude::*,
    ui_widgets::Activate,
};

use crate::{
    panels::levels::LevelDialog,
    session::Editor,
    shell::PanelChoice,
    theme::{size, spacing},
    tool::{PlaceAt, Tool},
};

/// A new landscape is two by two tiles of this many cells of this many metres each, 32 m on
/// a side in all. A starting size, not a limit: the level's size is not decided yet.
pub(crate) const LANDSCAPE_CELLS: u32 = 16;
pub(crate) const LANDSCAPE_SPACING: f32 = 1.0;

pub fn toolbar() -> impl Scene {
    bsn! {
        Node {
            min_height: px(size::TOOL_SIZE + spacing::SPACE_4),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            column_gap: px(spacing::SPACE_2),
            row_gap: px(spacing::SPACE_2),
            padding: UiRect::axes(px(spacing::SPACE_4), px(spacing::SPACE_2)),
        }
        ThemeBackgroundColor(tokens::PANE_HEADER_BG)
        Children [
            (
                @FeathersButton { @caption: bsn! { Text("New level") ThemedText } }
                on(|_: On<Activate>, mut dialog: ResMut<LevelDialog>| *dialog = LevelDialog::New)
            ),
            (
                @FeathersButton { @caption: bsn! { Text("Open level") ThemedText } }
                on(|_: On<Activate>, mut dialog: ResMut<LevelDialog>| *dialog = LevelDialog::Open)
            ),
            (
                @FeathersButton { @caption: bsn! { Text("Save as") ThemedText } }
                on(|_: On<Activate>, mut dialog: ResMut<LevelDialog>| *dialog = LevelDialog::SaveAs)
            ),
            pane_header_divider(),
            (
                @FeathersButton { @caption: bsn! { Text("Undo") ThemedText } }
                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.undo())
            ),
            (
                @FeathersButton { @caption: bsn! { Text("Redo") ThemedText } }
                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.redo())
            ),
            (
                @FeathersButton {
                    @caption: bsn! { Text("Save level") ThemedText },
                    @variant: ButtonVariant::Primary,
                }
                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.save())
            ),
            (
                @FeathersButton { @caption: bsn! { Text("Cook") ThemedText } }
                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.cook())
            ),
            pane_header_divider(),
            (
                @FeathersButton { @caption: bsn! { Text("Move") ThemedText } }
                on(|_: On<Activate>, mut gizmo: ResMut<TransformGizmoSettings>, mut tool: ResMut<Tool>| {
                    gizmo.mode = TransformGizmoMode::Translate;
                    *tool = Tool::Objects;
                })
            ),
            (
                @FeathersButton { @caption: bsn! { Text("Rotate") ThemedText } }
                on(|_: On<Activate>, mut gizmo: ResMut<TransformGizmoSettings>, mut tool: ResMut<Tool>| {
                    gizmo.mode = TransformGizmoMode::Rotate;
                    *tool = Tool::Objects;
                })
            ),
            (
                @FeathersButton { @caption: bsn! { Text("Scale") ThemedText } }
                on(|_: On<Activate>, mut gizmo: ResMut<TransformGizmoSettings>, mut tool: ResMut<Tool>| {
                    gizmo.mode = TransformGizmoMode::Scale;
                    *tool = Tool::Objects;
                })
            ),
            (
                @FeathersButton { @caption: bsn! { Text("Sculpt") ThemedText } }
                on(|_: On<Activate>, mut tool: ResMut<Tool>| *tool = Tool::Sculpt)
            ),
            (
                @FeathersButton { @caption: bsn! { Text("Paint") ThemedText } }
                on(|_: On<Activate>, mut tool: ResMut<Tool>| *tool = Tool::Paint)
            ),
            (
                @FeathersButton { @caption: bsn! { Text("Rules") ThemedText } }
                on(|_: On<Activate>, mut tool: ResMut<Tool>| *tool = Tool::Rules)
            ),
            pane_header_divider(),
            (
                @FeathersButton { @caption: bsn! { Text("Add landscape") ThemedText } }
                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| {
                    editor.add_landscape(LANDSCAPE_CELLS, LANDSCAPE_SPACING);
                })
            ),
            (
                @FeathersButton { @caption: bsn! { Text("Add box") ThemedText } }
                on(|_: On<Activate>, place: Res<PlaceAt>, mut editor: NonSendMut<Editor>| {
                    editor.add_box(&place);
                })
            ),
            (
                @FeathersButton { @caption: bsn! { Text("Add spawn") ThemedText } }
                on(|_: On<Activate>, place: Res<PlaceAt>, mut editor: NonSendMut<Editor>| {
                    editor.add_spawn(&place);
                })
            ),
            (
                @FeathersButton { @caption: bsn! { Text("Remove") ThemedText } }
                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.delete_selected())
            ),
            Node { flex_grow: 1.0 },
            (
                @FeathersButton { @caption: bsn! { Text("Hierarchy") ThemedText } }
                on(|_: On<Activate>, mut choice: ResMut<PanelChoice>| choice.toggle_hierarchy())
            ),
            (
                @FeathersButton { @caption: bsn! { Text("Inspector") ThemedText } }
                on(|_: On<Activate>, mut choice: ResMut<PanelChoice>| choice.toggle_inspector())
            ),
        ]
    }
}
