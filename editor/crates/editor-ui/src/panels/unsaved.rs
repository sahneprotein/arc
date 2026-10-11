//! Leaving a level with unsaved changes: the one question the editor asks.
//!
//! Three things leave the open level: closing the window, opening another level, and
//! beginning a new one. Each asks to [`Leave`]. If the level is saved it happens at once. If
//! not, a dialog offers to save first, to discard the changes, or to keep editing. Saving
//! goes through the core like any save; if it fails the editor stays where it is and the
//! notice says why.
//!
//! The window does not close by itself: the application turns that off, and a request to
//! close arrives here. Closing while a game the editor started runs goes on in
//! `closing.rs`: the game is stopped first.

use std::path::PathBuf;

use bevy::{
    feathers::{
        containers::pane_body,
        controls::{ButtonVariant, FeathersButton},
        display::{label, label_dim},
        theme::{ThemeBackgroundColor, ThemeBorderColor, ThemedText},
    },
    prelude::*,
    ui_widgets::Activate,
    window::WindowCloseRequested,
};

use crate::{
    panels::closing::{self, Shutdown},
    session::Editor,
    theme::{
        feathers::{BORDER, SCRIM},
        radius, spacing,
    },
};

/// What leaves the open level.
#[derive(Event, Clone, PartialEq, Eq, Debug, Default)]
pub enum Leave {
    /// The editor closes.
    #[default]
    Close,
    /// The level in this file is opened.
    Open(PathBuf),
    /// A new, empty level is begun, to be saved to this file.
    New(PathBuf),
}

impl Leave {
    /// What the button that saves first says.
    fn save_caption(&self) -> &'static str {
        match self {
            Self::Close => "Save and close",
            Self::Open(_) => "Save and open",
            Self::New(_) => "Save and begin",
        }
    }
}

/// Where the question stands.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Question {
    /// Nothing is being left.
    #[default]
    No,
    /// The dialog is open.
    Asking,
    /// Saving first was chosen; the save is under way.
    Saving,
}

/// The question about unsaved changes, and what happens once it is answered.
#[derive(Resource, Default, Clone, PartialEq, Eq, Debug)]
pub struct Unsaved {
    pub question: Question,
    then: Leave,
}

/// The dialog and the layer that covers the window behind it.
#[derive(Component, Default, Clone)]
pub struct UnsavedDialog;

/// The caption of the button that saves first; it names what happens afterwards.
#[derive(Component, Default, Clone)]
pub struct SaveFirstCaption;

/// The line that says a game is running and is stopped before the editor closes. Shown only
/// when the editor is closing and a game runs.
#[derive(Component, Default, Clone)]
pub struct GameRunsLine;

pub fn unsaved_dialog() -> impl Scene {
    bsn! {
        UnsavedDialog
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            top: px(0),
            bottom: px(0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            display: Display::None,
        }
        // In front of everything, and it takes every click: nothing behind can be used.
        GlobalZIndex(110)
        ThemeBackgroundColor(SCRIM)
        Children [(
            pane_body()
            Node {
                max_width: px(420),
                row_gap: px(spacing::SPACE_5),
                padding: UiRect::all(px(spacing::SPACE_6)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(radius::RADIUS_FIELD)),
            }
            ThemeBorderColor(BORDER)
            Children [
                label("Save changes to the level?"),
                label_dim("The level has changes that are not saved. Discarding them cannot be undone."),
                (
                    label_dim("A game is running. It is stopped before the editor closes.")
                    GameRunsLine
                    Node { display: Display::None }
                ),
                (
                    Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        justify_content: JustifyContent::End,
                        column_gap: px(spacing::SPACE_4),
                        row_gap: px(spacing::SPACE_2),
                    }
                    Children [
                        (
                            @FeathersButton { @caption: bsn! { Text("Discard changes") ThemedText } }
                            on(|_: On<Activate>,
                                mut unsaved: ResMut<Unsaved>,
                                mut shutdown: ResMut<Shutdown>,
                                mut editor: NonSendMut<Editor>,
                                mut exit: MessageWriter<AppExit>| {
                                unsaved.question = Question::No;
                                leave(&unsaved.then, &mut editor, &mut shutdown, &mut exit, true);
                            })
                        ),
                        (
                            @FeathersButton { @caption: bsn! { Text("Keep editing") ThemedText } }
                            on(|_: On<Activate>, mut unsaved: ResMut<Unsaved>| {
                                unsaved.question = Question::No;
                            })
                        ),
                        (
                            @FeathersButton {
                                @caption: bsn! { Text("Save and close") ThemedText SaveFirstCaption },
                                @variant: ButtonVariant::Primary,
                            }
                            on(|_: On<Activate>, mut unsaved: ResMut<Unsaved>, mut editor: NonSendMut<Editor>| {
                                editor.save();
                                unsaved.question = Question::Saving;
                            })
                        ),
                    ]
                ),
            ]
        )]
    }
}

/// Does what was asked for, the unsaved changes being saved or given up. `asked` says that
/// the person answered the question about them. Closing goes on in `closing.rs`: with a game
/// running the editor does not close yet.
fn leave(
    then: &Leave,
    editor: &mut Editor,
    shutdown: &mut Shutdown,
    exit: &mut MessageWriter<AppExit>,
    asked: bool,
) {
    match then {
        Leave::Close => closing::close(editor, shutdown, exit, asked),
        Leave::Open(file) => editor.open(file.clone()),
        Leave::New(file) => editor.new_level(file.clone()),
    }
}

/// Something wants to leave the level: at once if it is saved, after the question if not.
/// While the question is open or a save is under way, a second request changes nothing.
pub(crate) fn leave_or_ask(
    request: On<Leave>,
    mut unsaved: ResMut<Unsaved>,
    mut shutdown: ResMut<Shutdown>,
    mut editor: NonSendMut<Editor>,
    mut exit: MessageWriter<AppExit>,
) {
    // A second request changes nothing either while the editor is closing around a game.
    if unsaved.question != Question::No || *shutdown != Shutdown::No {
        return;
    }
    if editor.snapshot().status.dirty {
        unsaved.then = request.event().clone();
        unsaved.question = Question::Asking;
    } else {
        leave(
            request.event(),
            &mut editor,
            &mut shutdown,
            &mut exit,
            false,
        );
    }
}

/// Turns the desktop's request to close the window into a request to leave.
pub(crate) fn close_requested(
    mut commands: Commands,
    mut requests: MessageReader<WindowCloseRequested>,
) {
    if requests.read().count() > 0 {
        commands.trigger(Leave::Close);
    }
}

/// Finishes a save that was chosen in the dialog, and shows or hides the dialog.
pub(crate) fn answer_unsaved(
    mut unsaved: ResMut<Unsaved>,
    mut shutdown: ResMut<Shutdown>,
    mut editor: NonSendMut<Editor>,
    mut dialog: Query<&mut Node, (With<UnsavedDialog>, Without<GameRunsLine>)>,
    mut game_runs: Query<&mut Node, With<GameRunsLine>>,
    mut caption: Query<&mut Text, With<SaveFirstCaption>>,
    mut exit: MessageWriter<AppExit>,
) {
    if unsaved.question == Question::Saving && !editor.busy() {
        unsaved.question = Question::No;
        // A save that failed leaves everything as it is; the notice says why.
        if !editor.snapshot().status.dirty && editor.problem.is_none() {
            let then = unsaved.then.clone();
            leave(&then, &mut editor, &mut shutdown, &mut exit, true);
        }
    }
    let line = if unsaved.then == Leave::Close
        && (editor.active_run().is_some() || editor.play_requested())
    {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut game_runs {
        if node.display != line {
            node.display = line;
        }
    }
    let wanted = if unsaved.question == Question::Asking {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut dialog {
        if node.display != wanted {
            node.display = wanted;
        }
    }
    if unsaved.is_changed() {
        for mut text in &mut caption {
            if text.0 != unsaved.then.save_caption() {
                unsaved.then.save_caption().clone_into(&mut text.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_button_that_saves_first_says_what_follows() {
        assert_eq!(Leave::Close.save_caption(), "Save and close");
        assert_eq!(Leave::Open("a".into()).save_caption(), "Save and open");
        assert_eq!(Leave::New("a".into()).save_caption(), "Save and begin");
    }
}
