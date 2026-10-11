//! How the 3D view draws the level: a small control in the view's corner.
//!
//! The layout gives the 3D view a small overlay; this is its one control. It is a menu whose
//! button names the way that is chosen: "Editor", the editor's own plain picture, which is
//! what the view always was, or "Game look", the level lit and coloured as the game's client
//! draws it. The choice is the window's: it is not part of the level, of the core's session or
//! of what can be undone.
//!
//! Game look is drawn from the game's look profile, the one description of sun, sky, tone and
//! materials that the client draws from too. Until the editor holds that profile the row is
//! off and the menu says why, in words: without the profile the view could only guess at the
//! game's look, and a guess is not offered under the game's name.

use bevy::{
    app::PropagateOver,
    feathers::{
        constants::fonts,
        controls::{FeathersMenu, FeathersMenuButton},
        theme::{ThemeTextColor, ThemedText},
        tokens,
    },
    prelude::*,
    text::FontSourceTemplate,
    ui::InteractionDisabled,
    ui_widgets::Activate,
};

use crate::{
    panels::menu::{menu_item, menu_popup},
    theme::{size, spacing, text},
    tool::{LookStatus, Tool, ViewMode},
    wording,
};

/// The control in the view's corner.
#[derive(Component, Default, Clone)]
pub struct ViewControl;

/// Where the control is among the layers of the window: over the layer of an open menu
/// (`menu::menu_scrim`, 90) and under a menu's popup (100).
const VIEW_CONTROL_LAYER: i32 = 95;

/// The caption of the control's button, which names the chosen way.
#[derive(Component, Default, Clone)]
pub struct ViewCaption;

/// The menu's row of one way to draw the level.
#[derive(Component, Default, Clone)]
pub struct ViewRow(pub ViewMode);

/// The line under the rows: why Game look is off, or what it shows.
#[derive(Component, Default, Clone)]
pub struct ViewNote;

/// What the control's button says.
pub fn view_caption(mode: ViewMode) -> String {
    format!("View: {}", mode.name())
}

/// The way the view is drawn after `wanted` was asked for: Game look only with a profile.
pub fn view_after(wanted: ViewMode, look: &LookStatus) -> ViewMode {
    match wanted {
        ViewMode::GameLook if !look.loaded() => ViewMode::Editor,
        wanted => wanted,
    }
}

/// The line under the rows. Without a profile it says why Game look is off and what to do;
/// with one, what Game look is and is not: the open level, which Play does not run until it
/// is cooked.
pub fn view_note(look: &LookStatus) -> String {
    match look {
        LookStatus::NoGame => "Game look needs the game's look profile. Start the editor with \
            --game and the game's folder."
            .into(),
        LookStatus::Refused { code, path, field } => {
            let problem = wording::problem(code, |_| None);
            let field = match field {
                Some(field) => format!(" Field: {field}."),
                None => String::new(),
            };
            format!(
                "Game look is off. {}. {}{field} File: {path}",
                problem.title, problem.remedy
            )
        }
        LookStatus::Loaded { identity } => {
            let profile = match identity {
                Some(identity) => format!("look profile {}", wording::short_hash(identity)),
                None => "look profile".into(),
            };
            format!(
                "Game look draws the level as it is open with the game's {profile}: its sun, \
                 sky, tone and materials. Spawn points, and what a landscape's rules make, are \
                 drawn here and not by the game. Play runs the cooked level."
            )
        }
    }
}

fn choose_view(
    activate: On<Activate>,
    rows: Query<&ViewRow>,
    look: Res<LookStatus>,
    mut mode: ResMut<ViewMode>,
) {
    let Ok(row) = rows.get(activate.entity) else {
        return;
    };
    let wanted = view_after(row.0, &look);
    if *mode != wanted {
        *mode = wanted;
    }
}

fn view_row(mode: ViewMode) -> impl Scene {
    let name = mode.name();
    bsn! {
        menu_item(name, "")
        ViewRow({mode})
        on(choose_view)
    }
}

/// The control, for the node the 3D view is in: in its upper left corner.
pub fn view_control() -> impl Scene {
    bsn! {
        ViewControl
        Node {
            position_type: PositionType::Absolute,
            left: px(spacing::SPACE_4),
            top: px(spacing::SPACE_4),
        }
        // In front of the layer that lies over the window while a menu is open, and behind
        // the menus' popups. A menu's button opens its menu when it is pressed, and that
        // layer is there before the button is let go: a button under it never hears that it
        // was let go, stays pressed, and takes no second press. The toolbar's menus are
        // clear of the layer; this one is in the 3D view, which the layer covers. It opened
        // once and never again, so that Game look could be chosen and not left.
        GlobalZIndex(VIEW_CONTROL_LAYER)
        // Only the button is something to click on: what is beside it is still the view.
        Pickable { should_block_lower: false, is_hoverable: false }
        Children [(
            @FeathersMenu
            Children [
                (@FeathersMenuButton { @caption: bsn! { Text("View: Editor") ThemedText ViewCaption } }),
                (
                    menu_popup()
                    Children [
                        view_row(ViewMode::Editor),
                        (view_row(ViewMode::GameLook) InteractionDisabled),
                        (
                            Text("")
                            TextFont {
                                font: FontSourceTemplate::Handle(fonts::REGULAR),
                                font_size: {FontSize::Px(text::SMALL.size)},
                            }
                            PropagateOver<TextFont>
                            ThemeTextColor(tokens::TEXT_DIM)
                            ViewNote
                            Node {
                                max_width: px(size::MENU_WIDTH),
                                padding: UiRect::axes(px(spacing::SPACE_4), px(spacing::SPACE_1)),
                            }
                        ),
                    ]
                ),
            ]
        )]
    }
}

/// Shows the chosen way on the button, switches the Game look row on or off with the profile,
/// and says under the rows why it is off or what it shows. A profile that is lost while Game
/// look is chosen takes the view back to the editor's own picture. The control is not there
/// while the Particles tool is chosen: the view shows no level then, and the preview's own
/// controls are in its corner.
pub(crate) fn show_view(
    mut commands: Commands,
    (look, tool): (Res<LookStatus>, Res<Tool>),
    mut controls: Query<&mut Node, With<ViewControl>>,
    mut mode: ResMut<ViewMode>,
    mut captions: Query<&mut Text, (With<ViewCaption>, Without<ViewNote>)>,
    rows: Query<(Entity, &ViewRow, Has<InteractionDisabled>)>,
    mut notes: Query<&mut Text, (With<ViewNote>, Without<ViewCaption>)>,
) {
    let display = if *tool == Tool::Particles {
        Display::None
    } else {
        Display::Flex
    };
    for mut node in &mut controls {
        if node.display != display {
            node.display = display;
        }
    }
    let allowed = view_after(*mode, &look);
    if *mode != allowed {
        *mode = allowed;
    }
    // Compared each frame and written only on a change: the control may be made after the
    // profile was looked for.
    let caption = view_caption(*mode);
    for mut text in &mut captions {
        if text.0 != caption {
            text.0.clone_from(&caption);
        }
    }
    for (row, shown, off) in &rows {
        let offered = view_after(shown.0, &look) == shown.0;
        if offered && off {
            commands.entity(row).remove::<InteractionDisabled>();
        } else if !offered && !off {
            commands.entity(row).insert(InteractionDisabled);
        }
    }
    let note = view_note(&look);
    for mut text in &mut notes {
        if text.0 != note {
            text.0.clone_from(&note);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(code: &str) -> LookStatus {
        LookStatus::Refused {
            code: code.into(),
            path: "/game/assets/render/look-v1.json".into(),
            field: (code == "look.value").then(|| "sun.illuminance_lux".into()),
        }
    }

    fn loaded() -> LookStatus {
        LookStatus::Loaded {
            identity: Some("9f3a61c2d4e5f60718293a4b5c6d7e8f".into()),
        }
    }

    #[test]
    fn the_editors_own_picture_is_the_default_and_is_always_offered() {
        assert_eq!(ViewMode::default(), ViewMode::Editor);
        assert_eq!(view_caption(ViewMode::Editor), "View: Editor");
        for look in [LookStatus::NoGame, refused("look.missing"), loaded()] {
            assert_eq!(view_after(ViewMode::Editor, &look), ViewMode::Editor);
        }
    }

    #[test]
    fn game_look_is_refused_until_the_profile_is_loaded() {
        assert_eq!(
            view_after(ViewMode::GameLook, &LookStatus::NoGame),
            ViewMode::Editor
        );
        assert_eq!(
            view_after(ViewMode::GameLook, &refused("look.missing")),
            ViewMode::Editor
        );
        assert_eq!(
            view_after(ViewMode::GameLook, &loaded()),
            ViewMode::GameLook
        );
        assert_eq!(view_caption(ViewMode::GameLook), "View: Game look");
    }

    #[test]
    fn the_note_says_why_game_look_is_off_in_words_and_what_it_is_when_it_is_on() {
        let no_game = view_note(&LookStatus::NoGame);
        assert!(no_game.contains("--game"), "{no_game}");
        // With a profile: its short name, and that the open level is drawn, not the cooked.
        let on = view_note(&loaded());
        assert!(on.contains("9f3a61c2") && !on.contains("9f3a61c2d"), "{on}");
        assert!(on.contains("Play runs the cooked level"), "{on}");
        assert!(view_note(&LookStatus::Loaded { identity: None }).contains("look profile"));
        // Each refusal of the reader is a sentence with a remedy, names the file, and is
        // never a code.
        for code in [
            "look.missing",
            "look.read_failed",
            "look.limit",
            "look.decode",
            "look.schema",
            "look.value",
        ] {
            let note = view_note(&refused(code));
            assert!(note.starts_with("Game look is off."), "{note}");
            assert!(!note.contains(code), "{note}");
            assert!(!note.contains("Something went wrong"), "{code}: {note}");
            assert!(note.contains("look-v1.json"), "{note}");
        }
        assert!(view_note(&refused("look.value")).contains("sun.illuminance_lux"));
    }
}
