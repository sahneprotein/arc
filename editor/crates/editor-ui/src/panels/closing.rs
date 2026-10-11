//! Closing the editor while a game it started runs.
//!
//! The window closes only after the game and its server have shut down. The steps are those
//! of the layout "Closing the editor during a run", one dialog after the other:
//!
//! 1. The question. If the level has unsaved changes it is the usual question, with a line
//!    that a game is running (`unsaved.rs`). If nothing is unsaved the question is only about
//!    the game: keep playing, or stop and close.
//! 2. Waiting. The game is asked to stop; the dialog stays until the core's Play coordinator
//!    reports that the run has ended and been cleaned up.
//! 3. Only if a server, or the game, will not go: one more question, because closing now
//!    would leave it behind.
//!
//! This is for closing only. Opening or beginning another level leaves a game running: it
//! plays its own cooked level, whatever is open in the editor.

use bevy::{
    feathers::{
        containers::pane_body,
        controls::{ButtonVariant, FeathersButton},
        display::{label, label_dim},
        theme::{ThemeBackgroundColor, ThemeBorderColor, ThemedText},
    },
    prelude::*,
    ui_widgets::Activate,
};

use crate::{
    panels::{notice::left_behind_words, play::PlayView},
    session::Editor,
    theme::{
        feathers::{BORDER, SCRIM},
        radius, spacing,
    },
};

/// Where closing the editor stands while a game runs.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Shutdown {
    /// The editor is not closing, or nothing stands in its way.
    #[default]
    No,
    /// Nothing is unsaved; the question is whether to stop the game and close.
    AskRun,
    /// The game was asked to stop, and the editor closes when it has.
    Stopping,
    /// The run ended and left a process behind; the question is whether to close anyway.
    LeftBehind,
}

/// What a request to close does when the level's own question is out of the way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CloseStep {
    /// Nothing runs: the editor closes.
    Exit,
    /// A game runs and the person has not been asked anything yet: ask.
    Ask,
    /// A game runs and the person already chose to close, in the question about unsaved
    /// changes, which said that the game is stopped first: stop it.
    Stop,
}

pub fn close_step(game_runs: bool, already_asked: bool) -> CloseStep {
    match (game_runs, already_asked) {
        (false, _) => CloseStep::Exit,
        (true, false) => CloseStep::Ask,
        (true, true) => CloseStep::Stop,
    }
}

/// What follows once the game has been asked to stop: nothing while it still runs, then the
/// editor closes, unless the run left something behind.
pub fn after_stopping(game_runs: bool, left_behind: bool) -> Option<Shutdown> {
    match (game_runs, left_behind) {
        (true, _) => None,
        (false, true) => Some(Shutdown::LeftBehind),
        (false, false) => Some(Shutdown::No),
    }
}

/// Carries a request to close as far as it can go now. `already_asked` says that the person
/// has answered the question about unsaved changes.
pub(crate) fn close(
    editor: &mut Editor,
    shutdown: &mut Shutdown,
    exit: &mut MessageWriter<AppExit>,
    already_asked: bool,
) {
    let game_runs = editor.active_run().is_some() || editor.play_requested();
    match close_step(game_runs, already_asked) {
        CloseStep::Exit => {
            exit.write(AppExit::Success);
        }
        CloseStep::Ask => *shutdown = Shutdown::AskRun,
        CloseStep::Stop => {
            editor.stop_play();
            *shutdown = Shutdown::Stopping;
        }
    }
}

/// A part of the dialog that changes with the step.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, FromTemplate)]
pub enum ShutdownPiece {
    #[default]
    Dialog,
    Title,
    Body,
    Facts,
    KeepPlaying,
    StopAndClose,
    DoNotClose,
    CloseAnyway,
    KeepOpen,
}

/// What the dialog says at a step. `facts` names the cooked level that plays, or what was
/// left behind.
pub fn words(step: Shutdown, facts: &str) -> (&'static str, &'static str, String) {
    match step {
        Shutdown::No => ("", "", String::new()),
        Shutdown::AskRun => (
            "A game is still running",
            "Closing the editor stops the game and shuts down its server.",
            facts.to_owned(),
        ),
        Shutdown::Stopping => (
            "Stopping the game",
            "The editor closes when the game and its server have shut down.",
            String::new(),
        ),
        Shutdown::LeftBehind => (
            "The game's server did not shut down",
            "It may still be running. Closing the editor now leaves it behind.",
            facts.to_owned(),
        ),
    }
}

/// Which buttons the dialog has at a step.
pub fn offers(step: Shutdown, piece: ShutdownPiece) -> bool {
    use ShutdownPiece::{CloseAnyway, DoNotClose, KeepOpen, KeepPlaying, StopAndClose};
    match step {
        Shutdown::No => false,
        Shutdown::AskRun => matches!(piece, KeepPlaying | StopAndClose),
        Shutdown::Stopping => piece == DoNotClose,
        Shutdown::LeftBehind => matches!(piece, CloseAnyway | KeepOpen),
    }
}

fn button(caption: &'static str, primary: bool) -> impl Scene {
    let variant = if primary {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Normal
    };
    bsn! {
        @FeathersButton {
            @caption: bsn! { Text(caption) ThemedText },
            @variant: {variant},
        }
        Node { display: Display::None }
    }
}

pub fn shutdown_dialog() -> impl Scene {
    bsn! {
        ShutdownPiece::Dialog
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
        // In front of everything, as the question about unsaved changes is.
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
                (label("") ShutdownPiece::Title),
                (label_dim("") ShutdownPiece::Body),
                (label_dim("") ShutdownPiece::Facts),
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
                            button("Keep playing", false)
                            ShutdownPiece::KeepPlaying
                            on(|_: On<Activate>, mut shutdown: ResMut<Shutdown>| *shutdown = Shutdown::No)
                        ),
                        (
                            button("Stop and close", true)
                            ShutdownPiece::StopAndClose
                            on(|_: On<Activate>, mut shutdown: ResMut<Shutdown>, mut editor: NonSendMut<Editor>| {
                                editor.stop_play();
                                *shutdown = Shutdown::Stopping;
                            })
                        ),
                        (
                            button("Do not close the editor", false)
                            ShutdownPiece::DoNotClose
                            on(|_: On<Activate>, mut shutdown: ResMut<Shutdown>| *shutdown = Shutdown::No)
                        ),
                        (
                            button("Close anyway", false)
                            ShutdownPiece::CloseAnyway
                            on(|_: On<Activate>, mut exit: MessageWriter<AppExit>| {
                                exit.write(AppExit::Success);
                            })
                        ),
                        (
                            button("Keep the editor open", true)
                            ShutdownPiece::KeepOpen
                            on(|_: On<Activate>, mut shutdown: ResMut<Shutdown>| *shutdown = Shutdown::No)
                        ),
                    ]
                ),
            ]
        )]
    }
}

/// Moves closing on when the game has stopped, and shows the dialog of the step.
pub(crate) fn drive_shutdown(
    mut shutdown: ResMut<Shutdown>,
    editor: NonSend<Editor>,
    mut pieces: Query<(&ShutdownPiece, &mut Node, Option<&mut Text>)>,
    mut shown: Local<Option<(Shutdown, String)>>,
    mut exit: MessageWriter<AppExit>,
) {
    let game_runs = editor.active_run().is_some() || editor.play_requested();
    let status = editor.play_status();
    let left = editor
        .last_end()
        .and_then(|end| left_behind_words(end, status.as_deref()));
    match *shutdown {
        // The game ended by itself while the question stood: nothing is in the way now.
        Shutdown::AskRun if !game_runs => {
            *shutdown = Shutdown::No;
            exit.write(AppExit::Success);
        }
        Shutdown::Stopping => match after_stopping(game_runs, left.is_some()) {
            Some(Shutdown::No) => {
                *shutdown = Shutdown::No;
                exit.write(AppExit::Success);
            }
            Some(next) => *shutdown = next,
            None => {}
        },
        _ => {}
    }
    let facts = match *shutdown {
        Shutdown::AskRun => PlayView::of(&editor).run_words(),
        Shutdown::LeftBehind => left.map(|words| words.facts).unwrap_or_default(),
        _ => String::new(),
    };
    let now = (*shutdown, facts);
    if shown.as_ref() == Some(&now) || pieces.is_empty() {
        return;
    }
    let (title, body, facts) = words(now.0, &now.1);
    let display = |shown: bool| if shown { Display::Flex } else { Display::None };
    for (piece, mut node, text) in &mut pieces {
        let (line, laid_out) = match piece {
            ShutdownPiece::Dialog => (None, now.0 != Shutdown::No),
            ShutdownPiece::Title => (Some(title.to_owned()), true),
            ShutdownPiece::Body => (Some(body.to_owned()), true),
            ShutdownPiece::Facts => (Some(facts.clone()), !facts.is_empty()),
            button => (None, offers(now.0, *button)),
        };
        if let (Some(line), Some(mut text)) = (line, text)
            && text.0 != line
        {
            text.0 = line;
        }
        if node.display != display(laid_out) {
            node.display = display(laid_out);
        }
    }
    *shown = Some(now);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_game_closing_is_as_it_always_was() {
        assert_eq!(close_step(false, false), CloseStep::Exit);
        assert_eq!(close_step(false, true), CloseStep::Exit);
    }

    #[test]
    fn a_running_game_is_asked_about_once_and_never_twice() {
        // Nothing unsaved: the question is about the game.
        assert_eq!(close_step(true, false), CloseStep::Ask);
        // The question about unsaved changes already said the game is stopped first.
        assert_eq!(close_step(true, true), CloseStep::Stop);
    }

    #[test]
    fn the_editor_waits_for_the_game_and_closes_only_when_nothing_is_left_behind() {
        assert_eq!(after_stopping(true, false), None);
        assert_eq!(after_stopping(true, true), None, "it has not ended yet");
        assert_eq!(after_stopping(false, false), Some(Shutdown::No));
        assert_eq!(after_stopping(false, true), Some(Shutdown::LeftBehind));
    }

    #[test]
    fn every_step_has_its_own_buttons_and_a_way_not_to_close() {
        use ShutdownPiece::{CloseAnyway, DoNotClose, KeepOpen, KeepPlaying, StopAndClose};
        let buttons = [KeepPlaying, StopAndClose, DoNotClose, CloseAnyway, KeepOpen];
        let offered = |step| {
            buttons
                .iter()
                .copied()
                .filter(|button| offers(step, *button))
                .collect::<Vec<_>>()
        };
        assert_eq!(offered(Shutdown::No), vec![]);
        assert_eq!(offered(Shutdown::AskRun), vec![KeepPlaying, StopAndClose]);
        assert_eq!(offered(Shutdown::Stopping), vec![DoNotClose]);
        assert_eq!(offered(Shutdown::LeftBehind), vec![CloseAnyway, KeepOpen]);
    }

    #[test]
    fn the_dialog_names_what_plays_and_what_was_left() {
        let (title, body, facts) = words(Shutdown::AskRun, "playing cooked 6105ad50");
        assert_eq!(title, "A game is still running");
        assert!(body.contains("stops the game"));
        assert_eq!(facts, "playing cooked 6105ad50");
        let (title, _, facts) = words(Shutdown::LeftBehind, "process 4312");
        assert!(title.contains("did not shut down"));
        assert_eq!(facts, "process 4312");
        assert_eq!(words(Shutdown::Stopping, "ignored").2, "");
    }
}
