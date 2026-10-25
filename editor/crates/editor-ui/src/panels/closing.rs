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
//! Leaving the open project for the project browser goes the same way, with its own words:
//! the game is the project's, so it is stopped before the project is left, and where closing
//! would end the editor the browser is shown instead ([`Goal`]).
//!
//! Opening or beginning another level leaves a game running: it plays its own cooked level,
//! whatever is open in the editor.

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

use editor_services::{PlayEnd, PlayState, PlayStatus};

use crate::{
    panels::{launcher::ToBrowser, notice::left_behind_words, play::PlayView},
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

/// Where the editor goes once nothing stands in the way: it closes, or it leaves the open
/// project for the project browser.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Goal {
    #[default]
    Exit,
    Browser,
}

/// Goes where the request was to: the editor closes, or the project browser is shown.
pub(crate) fn finish(goal: Goal, exit: &mut MessageWriter<AppExit>, commands: &mut Commands) {
    match goal {
        Goal::Exit => {
            exit.write(AppExit::Success);
        }
        Goal::Browser => commands.trigger(ToBrowser),
    }
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

/// What [`close`] works with: where closing stands, where it goes, and the two ways of
/// getting there.
pub(crate) struct Closing<'a, 'e, 'w, 's> {
    pub shutdown: &'a mut Shutdown,
    pub goal: &'a mut Goal,
    pub exit: &'a mut MessageWriter<'e, AppExit>,
    pub commands: &'a mut Commands<'w, 's>,
}

/// Carries a request to close, or to leave the project, as far as it can go now.
/// `already_asked` says that the person has answered the question about unsaved changes.
pub(crate) fn close(
    editor: &mut Editor,
    closing: Closing<'_, '_, '_, '_>,
    goal: Goal,
    already_asked: bool,
) {
    let Closing {
        shutdown,
        goal: kept,
        exit,
        commands,
    } = closing;
    *kept = goal;
    let game_runs = editor.active_run().is_some() || editor.play_requested();
    match close_step(game_runs, already_asked) {
        CloseStep::Exit => finish(goal, exit, commands),
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
    StopAndLeave,
    StayInProject,
    LeaveAnyway,
    KeepProject,
}

/// What the dialog says at a step, on the way to `goal`. `facts` names the cooked level that
/// plays, or what was left behind.
pub fn words(step: Shutdown, goal: Goal, facts: &str) -> (&'static str, &'static str, String) {
    match (step, goal) {
        (Shutdown::No, _) => ("", "", String::new()),
        (Shutdown::AskRun, Goal::Browser) => (
            "A game is still running",
            "Leaving the project stops the game and shuts down its server.",
            facts.to_owned(),
        ),
        (Shutdown::Stopping, Goal::Browser) => (
            "Stopping the game",
            "The project browser opens when the game and its server have shut down.",
            String::new(),
        ),
        (Shutdown::LeftBehind, Goal::Browser) => (
            "The game's server did not shut down",
            "It may still be running. Leaving the project now leaves it behind.",
            facts.to_owned(),
        ),
        (Shutdown::AskRun, Goal::Exit) => (
            "A game is still running",
            "Closing the editor stops the game and shuts down its server.",
            facts.to_owned(),
        ),
        (Shutdown::Stopping, Goal::Exit) => (
            "Stopping the game",
            "The editor closes when the game and its server have shut down.",
            String::new(),
        ),
        (Shutdown::LeftBehind, Goal::Exit) => (
            "The game's server did not shut down",
            "It may still be running. Closing the editor now leaves it behind.",
            facts.to_owned(),
        ),
    }
}

/// Which buttons the dialog has at a step, on the way to `goal`.
pub fn offers(step: Shutdown, goal: Goal, piece: ShutdownPiece) -> bool {
    use ShutdownPiece::{
        CloseAnyway, DoNotClose, KeepOpen, KeepPlaying, KeepProject, LeaveAnyway, StayInProject,
        StopAndClose, StopAndLeave,
    };
    match (step, goal) {
        (Shutdown::No, _) => false,
        (Shutdown::AskRun, Goal::Exit) => matches!(piece, KeepPlaying | StopAndClose),
        (Shutdown::Stopping, Goal::Exit) => piece == DoNotClose,
        (Shutdown::LeftBehind, Goal::Exit) => matches!(piece, CloseAnyway | KeepOpen),
        (Shutdown::AskRun, Goal::Browser) => matches!(piece, KeepPlaying | StopAndLeave),
        (Shutdown::Stopping, Goal::Browser) => piece == StayInProject,
        (Shutdown::LeftBehind, Goal::Browser) => matches!(piece, LeaveAnyway | KeepProject),
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
                            button("Stop and leave", true)
                            ShutdownPiece::StopAndLeave
                            on(|_: On<Activate>, mut shutdown: ResMut<Shutdown>, mut editor: NonSendMut<Editor>| {
                                editor.stop_play();
                                *shutdown = Shutdown::Stopping;
                            })
                        ),
                        (
                            button("Stay in the project", false)
                            ShutdownPiece::StayInProject
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
                            button("Leave anyway", false)
                            ShutdownPiece::LeaveAnyway
                            on(|_: On<Activate>, mut shutdown: ResMut<Shutdown>, mut commands: Commands| {
                                *shutdown = Shutdown::No;
                                commands.trigger(ToBrowser);
                            })
                        ),
                        (
                            button("Keep the editor open", true)
                            ShutdownPiece::KeepOpen
                            on(|_: On<Activate>, mut shutdown: ResMut<Shutdown>| *shutdown = Shutdown::No)
                        ),
                        (
                            button("Stay in the project", true)
                            ShutdownPiece::KeepProject
                            on(|_: On<Activate>, mut shutdown: ResMut<Shutdown>| *shutdown = Shutdown::No)
                        ),
                    ]
                ),
            ]
        )]
    }
}

/// The end of the run the status has, once it has ended.
fn ended(status: Option<&PlayStatus>) -> Option<&PlayEnd> {
    match &status?.run.as_ref()?.state {
        PlayState::Ended(end) => Some(end),
        _ => None,
    }
}

/// Moves closing on when the game has stopped, and shows the dialog of the step.
pub(crate) fn drive_shutdown(
    (mut shutdown, goal): (ResMut<Shutdown>, Res<Goal>),
    editor: NonSend<Editor>,
    mut pieces: Query<(&ShutdownPiece, &mut Node, Option<&mut Text>)>,
    mut shown: Local<Option<(Shutdown, Goal, String)>>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    let goal = *goal;
    let game_runs = editor.active_run().is_some() || editor.play_requested();
    let status = editor.play_status();
    // What the run left behind is judged from its end as the status has it: the status can
    // say that the run ended a frame before the event of that end is taken, and in that
    // frame the last end taken is an older run's, or none, which would let the editor go
    // as if nothing were left.
    let left = ended(status.as_deref())
        .or_else(|| editor.last_end())
        .and_then(|end| left_behind_words(end, status.as_deref()));
    match *shutdown {
        // The game ended by itself while the question stood: nothing is in the way now.
        Shutdown::AskRun if !game_runs => {
            *shutdown = Shutdown::No;
            finish(goal, &mut exit, &mut commands);
        }
        Shutdown::Stopping => match after_stopping(game_runs, left.is_some()) {
            Some(Shutdown::No) => {
                *shutdown = Shutdown::No;
                finish(goal, &mut exit, &mut commands);
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
    let now = (*shutdown, goal, facts);
    if shown.as_ref() == Some(&now) || pieces.is_empty() {
        return;
    }
    let (title, body, facts) = words(now.0, goal, &now.2);
    let display = |shown: bool| if shown { Display::Flex } else { Display::None };
    for (piece, mut node, text) in &mut pieces {
        let (line, laid_out) = match piece {
            ShutdownPiece::Dialog => (None, now.0 != Shutdown::No),
            ShutdownPiece::Title => (Some(title.to_owned()), true),
            ShutdownPiece::Body => (Some(body.to_owned()), true),
            ShutdownPiece::Facts => (Some(facts.clone()), !facts.is_empty()),
            button => (None, offers(now.0, goal, *button)),
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
                .filter(|button| offers(step, Goal::Exit, *button))
                .collect::<Vec<_>>()
        };
        assert_eq!(offered(Shutdown::No), vec![]);
        assert_eq!(offered(Shutdown::AskRun), vec![KeepPlaying, StopAndClose]);
        assert_eq!(offered(Shutdown::Stopping), vec![DoNotClose]);
        assert_eq!(offered(Shutdown::LeftBehind), vec![CloseAnyway, KeepOpen]);
    }

    #[test]
    fn the_dialog_names_what_plays_and_what_was_left() {
        let (title, body, facts) = words(Shutdown::AskRun, Goal::Exit, "playing cooked 6105ad50");
        assert_eq!(title, "A game is still running");
        assert!(body.contains("stops the game"));
        assert_eq!(facts, "playing cooked 6105ad50");
        let (title, _, facts) = words(Shutdown::LeftBehind, Goal::Exit, "process 4312");
        assert!(title.contains("did not shut down"));
        assert_eq!(facts, "process 4312");
        assert_eq!(words(Shutdown::Stopping, Goal::Exit, "ignored").2, "");
    }

    #[test]
    fn leaving_the_project_has_its_own_buttons_and_never_offers_to_close() {
        use ShutdownPiece::{
            CloseAnyway, DoNotClose, KeepOpen, KeepPlaying, KeepProject, LeaveAnyway,
            StayInProject, StopAndClose, StopAndLeave,
        };
        let buttons = [
            KeepPlaying,
            StopAndClose,
            DoNotClose,
            CloseAnyway,
            KeepOpen,
            StopAndLeave,
            StayInProject,
            LeaveAnyway,
            KeepProject,
        ];
        let offered = |step, goal| {
            buttons
                .iter()
                .copied()
                .filter(|button| offers(step, goal, *button))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            offered(Shutdown::AskRun, Goal::Browser),
            vec![KeepPlaying, StopAndLeave]
        );
        assert_eq!(
            offered(Shutdown::Stopping, Goal::Browser),
            vec![StayInProject]
        );
        assert_eq!(
            offered(Shutdown::LeftBehind, Goal::Browser),
            vec![LeaveAnyway, KeepProject]
        );
        // And closing offers none of leaving's.
        for step in [Shutdown::AskRun, Shutdown::Stopping, Shutdown::LeftBehind] {
            for button in [StopAndLeave, StayInProject, LeaveAnyway, KeepProject] {
                assert!(!offers(step, Goal::Exit, button), "{step:?} {button:?}");
            }
        }
    }

    #[test]
    fn leaving_the_project_says_the_project_and_the_browser_not_the_editor() {
        let (title, body, facts) =
            words(Shutdown::AskRun, Goal::Browser, "playing cooked 6105ad50");
        assert_eq!(title, "A game is still running");
        assert_eq!(
            body,
            "Leaving the project stops the game and shuts down its server."
        );
        assert_eq!(facts, "playing cooked 6105ad50");
        let (_, body, _) = words(Shutdown::Stopping, Goal::Browser, "");
        assert!(body.contains("project browser opens"));
        let (_, body, facts) = words(Shutdown::LeftBehind, Goal::Browser, "process 4312");
        assert!(body.contains("Leaving the project now leaves it behind"));
        assert_eq!(facts, "process 4312");
        for step in [Shutdown::AskRun, Shutdown::Stopping, Shutdown::LeftBehind] {
            let (_, body, _) = words(step, Goal::Browser, "");
            assert!(
                !body.contains("editor closes") && !body.contains("Closing"),
                "{body}"
            );
        }
    }
}
