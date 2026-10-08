//! What the 3D view says while the level has nothing in it.

use bevy::{
    feathers::{
        controls::{ButtonVariant, FeathersButton},
        display::{label, label_dim},
        theme::ThemedText,
    },
    prelude::*,
    ui_widgets::Activate,
};

use crate::{
    panels::{
        levels::LevelDialog,
        toolbar::{LANDSCAPE_CELLS, LANDSCAPE_SPACING},
    },
    session::Editor,
    theme::spacing,
};

/// The words in the middle of the view of an empty level.
#[derive(Component, Default, Clone)]
pub struct EmptyHint;

/// The hint, to be placed inside the node of the 3D view. Only its two buttons take clicks.
pub fn empty_hint() -> impl Scene {
    bsn! {
        EmptyHint
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            top: px(0),
            bottom: px(0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(spacing::SPACE_2),
            padding: UiRect::all(px(spacing::SPACE_6)),
            display: Display::None,
        }
        Pickable { should_block_lower: false, is_hoverable: false }
        Children [
            (label("This level is empty") Pickable { should_block_lower: false, is_hoverable: false }),
            (
                label_dim("Add a landscape to begin, or open another level.")
                Pickable { should_block_lower: false, is_hoverable: false }
            ),
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: px(spacing::SPACE_4),
                    padding: UiRect::top(px(spacing::SPACE_4)),
                }
                Pickable { should_block_lower: false, is_hoverable: false }
                Children [
                    (
                        @FeathersButton {
                            @caption: bsn! { Text("Add a landscape") ThemedText },
                            @variant: ButtonVariant::Primary,
                        }
                        on(|_: On<Activate>, mut editor: NonSendMut<Editor>| {
                            editor.add_landscape(LANDSCAPE_CELLS, LANDSCAPE_SPACING);
                        })
                    ),
                    (
                        @FeathersButton { @caption: bsn! { Text("Open a level") ThemedText } }
                        on(|_: On<Activate>, mut dialog: ResMut<LevelDialog>| {
                            *dialog = LevelDialog::Open;
                        })
                    ),
                ]
            ),
        ]
    }
}

/// Shows the hint while the level has no objects.
pub(crate) fn show_empty_hint(
    editor: NonSend<Editor>,
    mut hint: Query<&mut Node, With<EmptyHint>>,
) {
    let wanted = if editor.snapshot().hierarchy.is_empty() {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut hint {
        if node.display != wanted {
            node.display = wanted;
        }
    }
}
