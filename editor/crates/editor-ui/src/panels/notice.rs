//! The notices: what stays on screen until it is dealt with.
//!
//! They sit in a corner of the 3D view, one under the other, and the level stays as it was
//! behind them. There are three, each with its own place, so that none takes another's:
//!
//! - a problem with the level: a save, a cook or an edit the core refused;
//! - how a run of the game ended, when the person did not ask for that end: it stopped
//!   unexpectedly, or it did not start;
//! - what a run left behind: a server, or the game, that did not shut down. It stands beside
//!   the notice of how the run ended and never in its place.
//!
//! Each says what happened and what to do, and is put away by its own button. The words come
//! from `wording.rs`.

use bevy::{
    feathers::{
        containers::pane_body,
        controls::{ButtonVariant, FeathersButton},
        display::{label, label_dim},
        theme::{ThemeBorderColor, ThemedText},
    },
    prelude::*,
    ui_widgets::Activate,
};
use editor_services::{PlayEnd, PlayProcess, PlayRole, PlayStatus};

use crate::{
    session::Editor,
    theme::{
        feathers::{SIGNAL_ERROR, SIGNAL_WARN},
        radius, spacing,
    },
    wording,
};

/// Which notice.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NoticeSlot {
    /// A problem with the level.
    #[default]
    Problem,
    /// How a run of the game ended.
    RunEnd,
    /// What a run of the game left behind.
    LeftBehind,
}

impl NoticeSlot {
    pub const ALL: [NoticeSlot; 3] = [Self::Problem, Self::RunEnd, Self::LeftBehind];
}

/// The box of one notice.
#[derive(Component, Default, Clone)]
pub struct Notice(pub NoticeSlot);

/// One of a notice's texts.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NoticePart {
    #[default]
    Title,
    Remedy,
    /// Facts to look something up by: an exit code, a process number, a log's path.
    Facts,
}

#[derive(Component, Default, Clone)]
pub struct NoticeText(pub NoticeSlot, pub NoticePart);

/// The button that tries the failed save again. Shown only for a failed save.
#[derive(Component, Default, Clone)]
pub struct RetryButton;

/// The button that puts a notice away.
#[derive(Component, Default, Clone)]
pub struct DismissButton(pub NoticeSlot);

fn dismiss(activate: On<Activate>, buttons: Query<&DismissButton>, mut editor: NonSendMut<Editor>) {
    match buttons.get(activate.entity).map(|button| button.0) {
        Ok(NoticeSlot::Problem) => editor.dismiss_problem(),
        Ok(NoticeSlot::RunEnd) => editor.dismiss_run_end(),
        Ok(NoticeSlot::LeftBehind) => editor.dismiss_cleanup(),
        Err(_) => {}
    }
}

/// The box of one notice. A problem's and a failed run's are edged in the error colour; what
/// a run left behind is a warning.
fn notice_box(slot: NoticeSlot) -> impl Scene {
    let edge = if slot == NoticeSlot::LeftBehind {
        SIGNAL_WARN
    } else {
        SIGNAL_ERROR
    };
    let retry = if slot == NoticeSlot::Problem {
        Display::Flex
    } else {
        Display::None
    };
    bsn! {
        pane_body()
        Notice({slot})
        Node {
            max_width: px(380),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(radius::RADIUS_CONTROL)),
            display: Display::None,
        }
        ThemeBorderColor(edge)
        Children [
            (label("") NoticeText({slot}, {NoticePart::Title})),
            (label_dim("") NoticeText({slot}, {NoticePart::Remedy})),
            (label_dim("") NoticeText({slot}, {NoticePart::Facts})),
            (
                Node { flex_direction: FlexDirection::Row, column_gap: px(spacing::SPACE_4) }
                Children [
                    (
                        @FeathersButton {
                            @caption: bsn! { Text("Try again") ThemedText },
                            @variant: ButtonVariant::Primary,
                        }
                        RetryButton
                        Node { display: {retry} }
                        on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.retry_save())
                    ),
                    (
                        @FeathersButton { @caption: bsn! { Text("Dismiss") ThemedText } }
                        DismissButton({slot})
                        on(dismiss)
                    ),
                ]
            ),
        ]
    }
}

/// The notices, to be placed inside the node of the 3D view.
pub fn notice() -> impl Scene {
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            top: px(spacing::SPACE_4),
            right: px(spacing::SPACE_4),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            row_gap: px(spacing::SPACE_4),
        }
        // The column itself is nothing to click on: only the notices in it are.
        Pickable { should_block_lower: false, is_hoverable: false }
        Children [
            notice_box(NoticeSlot::Problem),
            notice_box(NoticeSlot::RunEnd),
            notice_box(NoticeSlot::LeftBehind),
        ]
    }
}

/// What a notice says; a slot without words is hidden.
#[derive(PartialEq, Eq, Clone, Debug, Default)]
pub struct NoticeWords {
    pub title: String,
    pub remedy: String,
    pub facts: String,
    /// Whether the notice offers to try a failed save again.
    pub retry: bool,
}

/// The process of the last run that played this part.
fn process(status: Option<&PlayStatus>, role: PlayRole) -> Option<&PlayProcess> {
    status?
        .run
        .as_ref()?
        .processes
        .iter()
        .find(|process| process.role == role)
}

/// The facts of a process for a notice: how it ended, its number, its log.
fn facts(process: Option<&PlayProcess>, with_pid: bool) -> String {
    let Some(process) = process else {
        return String::new();
    };
    let mut facts = Vec::new();
    if let Some(exit) = &process.exit {
        facts.extend(wording::exit_facts(exit.code, exit.signal));
    }
    if with_pid && let Some(pid) = process.pid {
        facts.push(format!("process {pid}"));
    }
    if let Some(log) = &process.log_path {
        facts.push(log.display().to_string());
    }
    facts.join("   ")
}

/// The notice of how the last run ended, if it is an end the person did not ask for.
pub fn run_end_words(end: &PlayEnd, status: Option<&PlayStatus>) -> Option<NoticeWords> {
    let cause = end.cause.as_ref().map(|cause| cause.code.as_str());
    let shown = wording::play_end(&end.primary.code, cause)?;
    // The game is the client; a server that died is named by the cause.
    let client = process(status, PlayRole::Client);
    let which = match client {
        Some(process) if process.pid.is_some() => client,
        _ => process(status, PlayRole::Server),
    };
    Some(NoticeWords {
        title: shown.title,
        remedy: shown.remedy,
        facts: facts(which, false),
        retry: false,
    })
}

/// The notice of what the last run left behind: a process that was started and of which
/// the coordinator never saw the end. A server that is left is named before a game that is.
///
/// A process that had to be killed because it did not stop when asked is not left behind:
/// the coordinator saw it end. Its cleanup is reported as having overrun, and that shows in
/// how it ended ("ended by signal 9"), not as something that may still be running.
pub fn left_behind_words(end: &PlayEnd, status: Option<&PlayStatus>) -> Option<NoticeWords> {
    let left = |role: PlayRole| {
        let ended = match role {
            PlayRole::Server => end.cleanup.server.is_some(),
            PlayRole::Client => end.cleanup.client.is_some(),
        };
        let started = process(status, role).is_some_and(|process| process.pid.is_some());
        started && !ended
    };
    let (code, role) = if left(PlayRole::Server) {
        ("play.server_cleanup_failed", PlayRole::Server)
    } else if left(PlayRole::Client) {
        ("play.client_cleanup_failed", PlayRole::Client)
    } else {
        return None;
    };
    let shown = wording::play_left_behind(code)?;
    Some(NoticeWords {
        title: shown.title,
        remedy: shown.remedy,
        facts: facts(process(status, role), true),
        retry: false,
    })
}

/// What each notice says now, by slot.
pub fn notices(editor: &Editor) -> [Option<NoticeWords>; 3] {
    let problem = editor.problem.as_ref().map(|diagnostic| {
        let words = wording::problem(&diagnostic.code, |key| {
            diagnostic.parameters.get(key).map(String::as_str)
        });
        NoticeWords {
            title: words.title,
            remedy: words.remedy,
            facts: String::new(),
            retry: editor.problem_is_from_save(),
        }
    });
    let status = editor.play_status();
    let (end_dismissed, left_dismissed) = editor.end_notices_dismissed();
    // A run that is under way has not ended: its notices belong to the run before it.
    let end = editor.last_end().filter(|_| editor.active_run().is_none());
    let run_end = end
        .filter(|_| !end_dismissed)
        .and_then(|end| run_end_words(end, status.as_deref()));
    let left_behind = end
        .filter(|_| !left_dismissed)
        .and_then(|end| left_behind_words(end, status.as_deref()));
    [problem, run_end, left_behind]
}

/// Shows the notices that have something to say and hides the others.
#[expect(
    clippy::type_complexity,
    reason = "a notice's texts are told from its box and its button by what they are not"
)]
pub(crate) fn show_notice(
    editor: NonSend<Editor>,
    mut boxes: Query<(&Notice, &mut Node), Without<RetryButton>>,
    mut texts: Query<(&NoticeText, &mut Text, &mut Node), (Without<Notice>, Without<RetryButton>)>,
    mut retry: Query<&mut Node, (With<RetryButton>, Without<Notice>)>,
    mut shown: Local<Option<[Option<NoticeWords>; 3]>>,
) {
    if boxes.is_empty() {
        return;
    }
    let now = notices(&editor);
    if shown.as_ref() == Some(&now) {
        return;
    }
    let display = |visible: bool| {
        if visible {
            Display::Flex
        } else {
            Display::None
        }
    };
    let words = |slot: NoticeSlot| {
        now[NoticeSlot::ALL
            .iter()
            .position(|other| *other == slot)
            .unwrap_or_default()]
        .as_ref()
    };
    for (notice, mut node) in &mut boxes {
        let wanted = display(words(notice.0).is_some());
        if node.display != wanted {
            node.display = wanted;
        }
    }
    for (which, mut text, mut node) in &mut texts {
        let Some(words) = words(which.0) else {
            continue;
        };
        let line = match which.1 {
            NoticePart::Title => &words.title,
            NoticePart::Remedy => &words.remedy,
            NoticePart::Facts => &words.facts,
        };
        if text.0 != *line {
            text.0.clone_from(line);
        }
        // A line with nothing to say takes no room.
        let wanted = display(!line.is_empty());
        if node.display != wanted {
            node.display = wanted;
        }
    }
    let retry_wanted = display(words(NoticeSlot::Problem).is_some_and(|words| words.retry));
    for mut node in &mut retry {
        if node.display != retry_wanted {
            node.display = retry_wanted;
        }
    }
    *shown = Some(now);
}
