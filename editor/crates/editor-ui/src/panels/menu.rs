//! The Menu of the design system: a short list of actions that opens from a button and closes
//! when one is chosen.
//!
//! The widget is the toolkit's: a menu button, a popup and rows, with the arrow keys, Enter and
//! Escape, and a popup that closes when the keyboard focus leaves it. This module adds what the
//! editor needs around it: rows with their shortcut keys, the design's sizes, and the rule that
//! an open menu has the pointer and the keyboard, as a dialog has.
//!
//! A menu is written where it is used:
//!
//! ```ignore
//! @FeathersMenu
//! Children [
//!     (@FeathersMenuButton { @caption: bsn! { Text("Level") ThemedText } }),
//!     (menu_popup() Children [
//!         (menu_item("Save", "Ctrl+S") on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.save())),
//!     ]),
//! ]
//! ```

use bevy::{
    app::PropagateOver,
    feathers::{
        constants::fonts,
        controls::{FeathersMenuItem, FeathersMenuPopup},
        theme::{ThemeTextColor, ThemedText},
        tokens,
    },
    prelude::*,
    text::FontSourceTemplate,
    ui_widgets::popover::{Popover, PopoverAlign, PopoverPlacement, PopoverSide},
};

use crate::theme::{radius, size, spacing, text};

/// Whether a menu has the pointer and the keyboard: one is open, or the press that closed it
/// is still held. While it has them the 3D view and the shortcuts do nothing.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct MenuOpen(pub bool);

/// The layer between an open menu and the rest of the window, below the toolbar. It takes the
/// click that closes the menu, so that the click does nothing else: it chooses no object and
/// begins no stroke. The toolbar stays free, so that a press on another menu's button closes
/// this menu and opens that one.
#[derive(Component, Default, Clone)]
pub struct MenuScrim;

pub fn menu_scrim() -> impl Scene {
    bsn! {
        MenuScrim
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            top: px(size::TOOLBAR_HEIGHT),
            bottom: px(0),
            display: Display::None,
        }
        // Behind the menu's popup, in front of everything else.
        GlobalZIndex(90)
    }
}

/// The popup of a menu that opens below its button, their left edges in line.
pub fn menu_popup() -> impl Scene {
    bsn! {
        @FeathersMenuPopup
        Node {
            min_width: px(size::MENU_WIDTH),
            padding: UiRect::all(px(spacing::SPACE_2)),
            row_gap: px(spacing::SPACE_1),
            border_radius: BorderRadius::all(px(radius::RADIUS_FIELD)),
        }
    }
}

/// The popup of a menu at the right end of the toolbar: it opens below its button with their
/// right edges in line, so that it stays inside the window.
pub fn menu_popup_end() -> impl Scene {
    bsn! {
        menu_popup()
        Popover {
            positions: vec![
                PopoverPlacement {
                    side: PopoverSide::Bottom,
                    align: PopoverAlign::End,
                    gap: 2.0,
                },
                PopoverPlacement {
                    side: PopoverSide::Top,
                    align: PopoverAlign::End,
                    gap: 2.0,
                },
            ],
            window_margin: 10.0,
        }
    }
}

/// One row of a menu: its caption and, at the right, the keys that do the same. `keys` is
/// empty for a row without a shortcut.
pub fn menu_item(caption: &'static str, keys: &'static str) -> impl Scene {
    bsn! {
        @FeathersMenuItem { @caption: bsn! { Text(caption) ThemedText } }
        Node { border_radius: BorderRadius::all(px(radius::RADIUS_CONTROL)) }
        Children [
            Node { flex_grow: 1.0, min_width: px(spacing::SPACE_6) },
            (
                Text(keys)
                TextFont {
                    font: FontSourceTemplate::Handle(fonts::MONO),
                    font_size: {FontSize::Px(text::NUMBER.size)},
                }
                PropagateOver<TextFont>
                ThemeTextColor(tokens::TEXT_DIM)
            ),
        ]
    }
}

/// Whether a menu has the pointer now. It has it while a popup is on screen. A press outside
/// the popup closes the menu at once, and that press still belongs to the menu until the
/// button is up: what is under the cursor then must not take the release as a click, and the
/// gizmo, which reads the mouse itself, must not take the press as the start of a drag.
pub fn menu_has_the_pointer(had_it: bool, popup_on_screen: bool, button_down: bool) -> bool {
    popup_on_screen || (had_it && button_down)
}

/// Keeps [`MenuOpen`] and the scrim in step with the popups.
pub(crate) fn mark_menu_open(
    popups: Query<&Visibility, With<FeathersMenuPopup>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut open: ResMut<MenuOpen>,
    mut scrims: Query<&mut Node, With<MenuScrim>>,
) {
    let on_screen = popups
        .iter()
        .any(|visibility| *visibility == Visibility::Visible);
    let now = menu_has_the_pointer(open.0, on_screen, mouse.get_pressed().len() > 0);
    if open.0 != now {
        open.0 = now;
    }
    let wanted = if now { Display::Flex } else { Display::None };
    for mut scrim in &mut scrims {
        if scrim.display != wanted {
            scrim.display = wanted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_open_menu_has_the_pointer() {
        assert!(menu_has_the_pointer(false, true, false));
        assert!(menu_has_the_pointer(true, true, true));
    }

    #[test]
    fn the_press_that_closes_a_menu_stays_the_menus_until_the_button_is_up() {
        // The popup is gone, the button that closed it is still down.
        assert!(menu_has_the_pointer(true, false, true));
        // Released: the window has the pointer again.
        assert!(!menu_has_the_pointer(true, false, false));
    }

    #[test]
    fn a_press_without_a_menu_is_not_the_menus() {
        assert!(!menu_has_the_pointer(false, false, true));
        assert!(!menu_has_the_pointer(false, false, false));
    }
}
