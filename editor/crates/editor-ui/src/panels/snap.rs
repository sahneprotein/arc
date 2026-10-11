//! Snapping: the toolbar's switch and what the 3D view says about it.
//!
//! As the design's ToolButton page has it, snap is a single toggle in the toolbar, on in the
//! accent colour; in a narrow window it is a row of the tool menu. The layout gives the 3D
//! view a small overlay with the snap state, so that it can be read at every window width
//! and while the pointer is on the gizmo. The overlay shows with the object tools, which are
//! what snapping acts on.
//!
//! The setting itself is the editor core's. The switch asks the core to change it, and what
//! the core then says is copied into [`Snap`] for the view and the toolbar. The two steps are
//! number fields at the foot of the inspector (`inspector.rs`).

use bevy::{
    feathers::{
        controls::{ButtonVariant, FeathersButton},
        display::label_dim,
        theme::{ThemeBackgroundColor, ThemedText},
        tokens,
    },
    prelude::*,
    ui_widgets::Activate,
};

use editor_core::SnapSettings;

use crate::{
    panels::menu::menu_item,
    session::Editor,
    theme::{radius, spacing},
    tool::{Snap, Tool},
};

/// The toolbar's button, or the tool menu's row, that switches snapping.
#[derive(Component, Default, Clone)]
pub struct SnapSwitch;

/// The overlay in the 3D view.
#[derive(Component, Default, Clone)]
pub struct SnapOverlay;

/// The overlay's words.
#[derive(Component, Default, Clone)]
pub struct SnapWords;

fn switch_snap(_: On<Activate>, mut editor: NonSendMut<Editor>) {
    let now = editor.snapshot().snap;
    editor.set_snap(SnapSettings {
        enabled: !now.enabled,
        ..now
    });
}

/// Copies the core's snap setting to where the 3D view and the toolbar read it.
pub(crate) fn mirror_snap(editor: NonSend<Editor>, mut snap: ResMut<Snap>) {
    let wanted = Snap::from(editor.snapshot().snap);
    if *snap != wanted {
        *snap = wanted;
    }
}

/// The switch as a toolbar button.
pub fn snap_button() -> impl Scene {
    bsn! {
        @FeathersButton {
            @caption: bsn! { Text("Snap") ThemedText },
            @variant: ButtonVariant::Plain,
        }
        SnapSwitch
        on(switch_snap)
    }
}

/// The switch as a row of the tool menu.
pub fn snap_item() -> impl Scene {
    bsn! {
        menu_item("Snap", "")
        SnapSwitch
        on(switch_snap)
    }
}

/// The overlay, for the node the 3D view is in. It takes no clicks: what is under it is
/// still the view.
pub fn snap_overlay() -> impl Scene {
    bsn! {
        SnapOverlay
        Node {
            position_type: PositionType::Absolute,
            left: px(spacing::SPACE_4),
            bottom: px(spacing::SPACE_4),
            padding: UiRect::axes(px(spacing::SPACE_4), px(spacing::SPACE_1)),
            border_radius: BorderRadius::all(px(radius::RADIUS_CONTROL)),
        }
        ThemeBackgroundColor(tokens::PANE_HEADER_BG)
        Pickable { should_block_lower: false, is_hoverable: false }
        Children [(
            label_dim("")
            SnapWords
            Pickable { should_block_lower: false, is_hoverable: false }
        )]
    }
}

/// Shows whether snapping is on: the button in the accent colour, and the overlay's words.
pub(crate) fn show_snap(
    snap: Res<Snap>,
    tool: Res<Tool>,
    mut buttons: Query<&mut ButtonVariant, With<SnapSwitch>>,
    mut words: Query<&mut Text, With<SnapWords>>,
    mut overlays: Query<&mut Node, With<SnapOverlay>>,
) {
    // Compared each frame and written only on a change: the button may be made after the
    // setting was.
    for mut variant in &mut buttons {
        let wanted = if snap.on {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Plain
        };
        if *variant != wanted {
            *variant = wanted;
        }
    }
    let said = snap.words();
    for mut text in &mut words {
        if text.0 != said {
            text.0.clone_from(&said);
        }
    }
    let display = if *tool == Tool::Objects {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut overlays {
        if node.display != display {
            node.display = display;
        }
    }
}
