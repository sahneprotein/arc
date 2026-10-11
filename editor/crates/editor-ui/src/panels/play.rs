//! The Play control of the design system: the one place the game is started and stopped from.
//!
//! The control is in the toolbar at every width. At rest it is a button with a menu: the menu
//! names the cooked level that would run and how it stands to the level being edited, offers
//! to cook, lets the person ask the editor's core whether the cooked level on disk is still
//! the one this editor cooked, and holds the two ways to play. "Open Game Window" asks the
//! core's Play coordinator to start the game in a window of its own, from the receipt of a
//! check made just before. "Preview in Editor" is off: it is not built. "Cook and Play", which
//! F5 does too, cooks the level first if it has changed, and then does the same; Shift+F5 is
//! Stop.
//!
//! While something is under way the control is a word and one Stop: checking, starting, the
//! game window open, stopping. The words follow what the coordinator reports and nothing
//! else: a run is shown as running when the coordinator says the game is ready, never because
//! a start was asked for. Without a game to start, the way to play is off and the menu says
//! why.
//!
//! What the control and its menu say is worked out by plain functions of [`PlayView`], so
//! that every sentence has a test without a window.

use bevy::{
    app::PropagateOver,
    feathers::{
        constants::{fonts, icons},
        controls::{
            ButtonVariant, FeathersButton, FeathersMenu, FeathersMenuButton, FeathersMenuDivider,
        },
        display::icon,
        theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeTextColor, ThemedText},
        tokens,
    },
    prelude::*,
    text::FontSourceTemplate,
    ui::InteractionDisabled,
    ui_widgets::Activate,
    window::PrimaryWindow,
};
use editor_services::{PlayRole, PlayRun, PlayStage, PlayState};

use crate::{
    panels::{
        menu::{menu_item, menu_popup_end},
        toolbar::toolbar_folds,
    },
    session::{play_blockers, Editor, PackageCheck},
    theme::{
        feathers::{BORDER, SIGNAL_ERROR, SIGNAL_PLAY, SIGNAL_WARN},
        size, spacing, text,
    },
    tool::Tool,
    wording,
};

/// The two ways to play, as the menu names them.
pub const PREVIEW_IN_EDITOR: &str = "Preview in Editor";
pub const OPEN_GAME_WINDOW: &str = "Open Game Window";
/// The row that asks the core about the cooked level on disk.
pub const CHECK_COOKED_LEVEL: &str = "Check cooked level";
/// The row that does everything a change needs to be played: cook, check, play.
pub const COOK_AND_PLAY: &str = "Cook and Play";

/// Why nothing can be played, when this editor was started without a game.
pub const NO_GAME: &str = "No game is set up for this editor: start the editor with --game \
    and the game's folder to play from here. The cooked level can be cooked and checked.";
/// Why the one way to play that is always off is off.
pub const NO_PREVIEW: &str = "Preview in Editor is not built yet.";

/// Where a run of the game stands, as the coordinator reports it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RunView {
    /// No run: none was started, or the last one has ended.
    #[default]
    None,
    /// The start was asked for, or the processes are starting and not yet ready.
    Starting,
    /// The game is ready and can be played.
    Running,
    /// The run is being stopped and cleaned up.
    Stopping,
}

/// Everything the Play control shows, as plain values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayView {
    /// The cooked level by the short form of its identity, if the level was cooked: see
    /// [`package_id`].
    pub cooked: Option<String>,
    /// Whether the level has changed since that cook.
    pub older: bool,
    /// Whether this editor holds the package's receipt, so that it can be checked. A level
    /// cooked before this editor started has a hash in the level's status and no receipt.
    pub can_check: bool,
    pub check: PackageCheck,
    /// Whether this editor has a game to start.
    pub game: bool,
    pub run: RunView,
    /// The cooked level the run plays, by the same short name.
    pub run_package: Option<String>,
    /// Whether the level has moved on from the cooked level the run plays.
    pub run_older: bool,
    /// How far each process has come while the run starts, in words. Empty when none has
    /// reported anything: a step nobody measured is not shown as done.
    pub steps: String,
    /// The code of what the coordinator last refused.
    pub refusal: Option<String>,
    /// The code of how the last run ended, once no run is under way.
    pub ended: Option<String>,
    /// Whether the level is being cooked because playing it was asked for.
    pub cooking: bool,
    /// Whether the run is being stopped to cook the level and play it again.
    pub restarting: bool,
    /// How many things in the open level stop the game from running it, by the core's review.
    pub blockers: usize,
}

/// A step a process reports while it starts, in words.
pub fn stage_words(stage: PlayStage) -> &'static str {
    match stage {
        PlayStage::PackageVerified => "level verified",
        PlayStage::CollisionReady => "collision ready",
        PlayStage::HandshakeAccepted => "accepted by the server",
        PlayStage::Simulating => "simulating",
        PlayStage::LevelDrawn => "level drawn",
        PlayStage::FirstControllableFrame => "first controllable frame",
    }
}

/// How far each process of a run has come: its latest step, for each that reported one.
fn steps_of(run: &PlayRun) -> String {
    run.processes
        .iter()
        .filter_map(|process| {
            let role = match process.role {
                PlayRole::Server => "server",
                PlayRole::Client => "client",
            };
            let latest = process.stages.last()?;
            Some(format!("{role} {}", stage_words(*latest)))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The short name a cooked level is shown by: the first eight digits of the hash of its
/// manifest. That hash is the package's identity everywhere else too: the cooked folder is
/// named by it, the game is started with it, and client and server compare it when they
/// meet. So the digits in the editor are the digits in a refusal from the game.
///
/// The editor knows the manifest's hash from the receipt of its own cook. Without a receipt
/// the level's status still says that it is cooked, by the hash of the simulation data alone;
/// that is shown then, for want of the other.
pub fn package_id(manifest_hash: Option<&str>, simulation_hash: Option<&str>) -> Option<String> {
    manifest_hash
        .or(simulation_hash)
        .map(|hash| wording::short_hash(hash).to_owned())
}

impl PlayView {
    pub fn of(editor: &Editor) -> Self {
        let snapshot = editor.snapshot();
        let status = &snapshot.status;
        let simulation = status.simulation_hash.as_ref().map(|hash| hash.hex());
        // The receipt, if it is of the cooked level the status speaks of.
        let receipt = editor.cooked().filter(|receipt| {
            simulation.as_deref() == Some(receipt.identity().simulation_hash.hex().as_str())
        });
        let manifest = receipt.map(|receipt| receipt.identity().manifest_hash.hex());
        let cooked = simulation
            .as_deref()
            .and_then(|simulation| package_id(manifest.as_deref(), Some(simulation)));
        let blockers = play_blockers(&snapshot);
        let play = editor.play_status();
        let active = play
            .as_ref()
            .and_then(|status| status.run.as_ref())
            .filter(|run| !matches!(run.state, PlayState::Ended(_)));
        let run = match active.map(|run| &run.state) {
            Some(PlayState::Running) => RunView::Running,
            Some(PlayState::Stopping) => RunView::Stopping,
            Some(PlayState::Starting) => RunView::Starting,
            // Asked for and not yet answered: the coordinator has it, or the check before it.
            Some(PlayState::Ended(_)) | None if editor.play_requested() => RunView::Starting,
            Some(PlayState::Ended(_)) | None => RunView::None,
        };
        Self {
            older: cooked.is_some() && status.cook_needed,
            can_check: receipt.is_some(),
            cooked,
            check: editor.package_check().clone(),
            game: editor.play_connected(),
            run,
            run_package: active
                .and_then(|run| package_id(Some(&run.identity.manifest_hash.hex()), None)),
            // The run's cooked level is current only if it is the last cook and the level
            // has not changed since that cook.
            run_older: active.is_some_and(|run| {
                status.cook_needed
                    || editor.cooked().map(|receipt| receipt.identity()) != Some(&run.identity)
            }),
            steps: active.map(steps_of).unwrap_or_default(),
            refusal: editor
                .play_refusal()
                .map(|problem| problem.code.clone())
                // Said only as long as it is true: the level may have been mended since.
                .filter(|code| code != "play.blocked" || blockers > 0),
            ended: match active {
                Some(_) => None,
                None => editor.last_end().map(|end| end.primary.code.clone()),
            },
            cooking: editor.cooking_to_play(),
            restarting: editor.restarting(),
            blockers,
        }
    }

    fn checking(&self) -> bool {
        self.check == PackageCheck::Checking
    }

    /// The word the control shows in place of its button while something is under way.
    /// `None` at rest, when the control is its button. While the cooked level is checked the
    /// word carries its hash; a run is named by where the coordinator says it stands.
    pub fn word(&self) -> Option<String> {
        if self.checking() {
            return Some(match &self.cooked {
                Some(hash) => format!("Checking {hash}"),
                None => "Checking".into(),
            });
        }
        match self.run {
            RunView::None => None,
            // The cook that comes before the start that was asked for.
            RunView::Starting if self.cooking => Some("Cooking".into()),
            RunView::Starting => Some("Starting".into()),
            RunView::Running => Some("Game window open".into()),
            RunView::Stopping => Some("Stopping".into()),
        }
    }

    /// Whether the word is the one for a game that runs, which is shown in the play colour.
    pub fn word_is_running(&self) -> bool {
        !self.checking() && self.run == RunView::Running
    }

    /// Whether the control carries the warning dot: there is a cooked level, and the level
    /// has moved on from it.
    pub fn warns(&self) -> bool {
        self.older
    }

    /// Whether the control shows Stop: while the cooked level is checked, and for a run in
    /// every state. There is one Stop, in the same place.
    pub fn stop_shown(&self) -> bool {
        self.checking() || self.run != RunView::None
    }

    /// Whether Stop can be pressed. Not while the run is already stopping: it is shown, off,
    /// so that it cannot be pressed twice.
    pub fn stop_offered(&self) -> bool {
        self.checking() || matches!(self.run, RunView::Starting | RunView::Running)
    }

    /// Whether the row that checks the cooked level can be chosen.
    pub fn check_offered(&self) -> bool {
        self.can_check && !self.checking() && self.run == RunView::None
    }

    /// Whether "Open Game Window" can be chosen: there is a game, a cooked level this editor
    /// holds the receipt of, and nothing under way.
    pub fn game_window_offered(&self) -> bool {
        self.game && self.check_offered()
    }

    /// Whether "Cook and Play" can be chosen: there is a game and nothing under way. It needs
    /// no cooked level, it makes one; and with something in the level that the game cannot
    /// run it is still offered, and says so when chosen.
    pub fn cook_and_play_offered(&self) -> bool {
        self.game && !self.checking() && self.run == RunView::None
    }

    /// Why a way to play is off: in the menu's foot as long as one is.
    pub fn reason_line(&self) -> &'static str {
        if self.game {
            NO_PREVIEW
        } else {
            NO_GAME
        }
    }

    /// The status bar's words about the game: a refusal, where the run stands, or how the
    /// last run ended if the person asked for that end. Empty when there is nothing to say.
    pub fn run_words(&self) -> String {
        if let Some(code) = &self.refusal {
            return wording::play_refusal(code);
        }
        let package = self.run_package.as_deref().unwrap_or_default();
        match self.run {
            RunView::Starting if self.cooking => "cooking the level to play it".into(),
            RunView::Starting if package.is_empty() => "starting the game".into(),
            RunView::Starting if self.steps.is_empty() => format!("starting cooked {package}"),
            RunView::Starting => format!("starting cooked {package}: {}", self.steps),
            RunView::Running if self.run_older => {
                format!("playing cooked {package}, older than the level")
            }
            RunView::Running => format!("playing cooked {package}"),
            RunView::Stopping if self.restarting => {
                "stopping the game to cook the level and play it again".into()
            }
            RunView::Stopping => "stopping the game".into(),
            RunView::None => self
                .ended
                .as_deref()
                .and_then(wording::play_end_asked)
                .unwrap_or_default()
                .to_owned(),
        }
    }

    /// Whether the status bar's words about the game are a refusal, which needs attention.
    pub fn run_words_warn(&self) -> bool {
        self.refusal.is_some()
    }

    /// The first line of the menu's foot: which cooked level would run.
    pub fn cooked_line(&self) -> String {
        match &self.cooked {
            Some(hash) => format!("cooked {hash}"),
            None => "not cooked".into(),
        }
    }

    /// How that cooked level stands to the level being edited. Empty when they agree.
    pub fn standing_line(&self) -> &'static str {
        match (&self.cooked, self.older) {
            (None, _) => "Cook the level to play it.",
            (Some(_), true) => {
                "The level has changed since this cook. \
                 Play runs the cooked level, without those changes."
            }
            (Some(_), false) => "",
        }
    }

    /// What the last check of the cooked level on disk found. Empty when nobody asked.
    pub fn check_line(&self) -> String {
        match &self.check {
            PackageCheck::NotAsked => String::new(),
            PackageCheck::Checking => "Checking the cooked level on disk.".into(),
            PackageCheck::Passed { .. } => {
                "Checked: the cooked level on disk is the one this editor cooked.".into()
            }
            PackageCheck::Refused { code } => {
                let problem = wording::problem(code, |_| None);
                format!("{}. {}", problem.title, problem.remedy)
            }
        }
    }

    /// Whether the check found something wrong.
    pub fn check_failed(&self) -> bool {
        matches!(self.check, PackageCheck::Refused { .. })
    }

    /// The foot's one button, if it has one: "Cook" without a cooked level, "Cook first" when
    /// the level has moved on. Both cook the level as it is open, saved or not: the core
    /// cooks the open level, not its file, so nothing is saved by this button.
    pub fn foot_button(&self) -> Option<&'static str> {
        match (&self.cooked, self.older) {
            (None, _) => Some("Cook"),
            (Some(_), true) => Some("Cook first"),
            (Some(_), false) => None,
        }
    }

    /// The status bar's words about the cooked level.
    pub fn status_words(&self) -> String {
        let Some(hash) = &self.cooked else {
            return "not cooked".into();
        };
        match (&self.check, self.older) {
            (PackageCheck::Checking, _) => format!("checking cooked {hash}"),
            (PackageCheck::Refused { .. }, _) => format!("cooked {hash}, check failed"),
            (_, true) => format!("cooked {hash}, older than the level"),
            (PackageCheck::Passed { .. }, false) => format!("cooked {hash}, checked"),
            (PackageCheck::NotAsked, false) => format!("cooked {hash}"),
        }
    }
}

/// A part of the Play control that changes with [`PlayView`].
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, FromTemplate)]
pub enum PlayPiece {
    /// The control at rest: its button with the menu.
    #[default]
    Menu,
    /// The word that takes the button's place while something is under way.
    Word,
    /// The dot the word shortens to in a narrow window.
    WordDot,
    /// The warning dot on the control's button.
    Dot,
    /// The Stop button beside the control.
    Stop,
    /// The row that opens the game in its own window.
    GameWindowRow,
    /// The row that checks the cooked level.
    CheckRow,
    /// The row that cooks, checks and plays.
    CookAndPlayRow,
    /// Why a way to play is off.
    ReasonLine,
    /// The foot's lines, and its button with that button's caption.
    CookedLine,
    StandingLine,
    CheckLine,
    FootButton,
    FootButtonCaption,
}

/// A line of the menu's foot, in the design's small text style. `mono` is for a hash.
fn foot_line(mono: bool) -> impl Scene {
    let font = if mono { fonts::MONO } else { fonts::REGULAR };
    bsn! {
        Text("")
        TextFont {
            font: FontSourceTemplate::Handle(font),
            font_size: {FontSize::Px(text::SMALL.size)},
        }
        PropagateOver<TextFont>
        ThemeTextColor(tokens::TEXT_MAIN)
    }
}

/// The menu's foot: which cooked level would run, how it stands to the level, what the last
/// check found, why nothing can be played yet, and at most one button.
fn foot() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(spacing::SPACE_2),
            margin: UiRect::top(px(spacing::SPACE_1)),
            padding: UiRect::all(px(spacing::SPACE_4)),
            border: UiRect::top(px(1)),
        }
        ThemeBorderColor(BORDER)
        Children [
            (foot_line(true) PlayPiece::CookedLine),
            (foot_line(false) PlayPiece::StandingLine),
            (foot_line(false) PlayPiece::CheckLine),
            (
                Text(NO_GAME)
                TextFont {
                    font: FontSourceTemplate::Handle(fonts::REGULAR),
                    font_size: {FontSize::Px(text::SMALL.size)},
                }
                PropagateOver<TextFont>
                ThemeTextColor(tokens::TEXT_DIM)
                PlayPiece::ReasonLine
            ),
            (
                @FeathersButton {
                    @caption: bsn! { Text("Cook") ThemedText PlayPiece::FootButtonCaption },
                }
                PlayPiece::FootButton
                Node { align_self: AlignSelf::FlexStart, margin: UiRect::top(px(spacing::SPACE_1)) }
                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.cook())
            ),
        ]
    }
}

/// The Play control: its button with the menu, and Stop beside it while there is something
/// to stop.
pub fn play_control() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(spacing::SPACE_2),
            flex_shrink: 0.0,
        }
        Children [
            (
                @FeathersMenu
                PlayPiece::Menu
                Children [
                    (
                        @FeathersMenuButton {
                            @caption: bsn! { Text("Play") ThemedText },
                            @arrow: false,
                        }
                        Node { column_gap: px(spacing::SPACE_3) }
                        Children [
                            (
                                // The design's dot: eight pixels across, round.
                                Node {
                                    width: px(spacing::SPACE_4),
                                    height: px(spacing::SPACE_4),
                                    border_radius: BorderRadius::all(px(spacing::SPACE_2)),
                                    display: Display::None,
                                }
                                ThemeBackgroundColor(SIGNAL_WARN)
                                PlayPiece::Dot
                            ),
                            icon(icons::CHEVRON_DOWN),
                        ]
                    ),
                    (
                        menu_popup_end()
                        // As wide as a menu is at least, so that the foot's sentences wrap.
                        // The design draws this menu wider, and has no token for that width.
                        Node { width: px(size::MENU_WIDTH) }
                        Children [
                            (menu_item(PREVIEW_IN_EDITOR, "") InteractionDisabled),
                            (
                                menu_item(OPEN_GAME_WINDOW, "")
                                PlayPiece::GameWindowRow
                                InteractionDisabled
                                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.play_external())
                            ),
                            (
                                menu_item(COOK_AND_PLAY, "F5")
                                PlayPiece::CookAndPlayRow
                                InteractionDisabled
                                on(|_: On<Activate>,
                                    mut tool: ResMut<Tool>,
                                    mut editor: NonSendMut<Editor>| {
                                    cook_and_play(&mut editor, &mut tool);
                                })
                            ),
                            @FeathersMenuDivider,
                            (
                                menu_item(CHECK_COOKED_LEVEL, "")
                                PlayPiece::CheckRow
                                InteractionDisabled
                                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.check_cooked())
                            ),
                            foot(),
                        ]
                    ),
                ]
            ),
            (
                Text("")
                TextFont {
                    font: FontSourceTemplate::Handle(fonts::REGULAR),
                    font_size: {FontSize::Px(text::COMPACT.size)},
                }
                PropagateOver<TextFont>
                ThemeTextColor(tokens::TEXT_MAIN)
                PlayPiece::Word
                Node { display: Display::None }
            ),
            (
                // The design's dot, for the word in a narrow window.
                Node {
                    width: px(spacing::SPACE_4),
                    height: px(spacing::SPACE_4),
                    border_radius: BorderRadius::all(px(spacing::SPACE_2)),
                    display: Display::None,
                }
                ThemeBackgroundColor(tokens::TEXT_DIM)
                PlayPiece::WordDot
            ),
            (
                @FeathersButton {
                    @caption: bsn! { Text("Stop") ThemedText },
                    @variant: ButtonVariant::Primary,
                }
                PlayPiece::Stop
                Node { display: Display::None }
                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.stop_play())
            ),
        ]
    }
}

/// Cooks, checks and plays, as the row "Cook and Play" and its key do. When the level has
/// something the game cannot run, nothing is cooked, and the Review tool is chosen: its panel
/// lists what that is.
pub fn cook_and_play(editor: &mut Editor, tool: &mut Tool) {
    editor.play_now();
    let blocked = editor
        .play_refusal()
        .is_some_and(|refusal| refusal.code == "play.blocked");
    if blocked && *tool != Tool::Review {
        *tool = Tool::Review;
    }
}

/// Sets whether a row or a button can be chosen. It stays in its place either way.
fn set_offered(commands: &mut Commands, entity: Entity, offered: bool, off: bool) {
    if !offered && !off {
        commands.entity(entity).insert(InteractionDisabled);
    } else if offered && off {
        commands.entity(entity).remove::<InteractionDisabled>();
    }
}

/// Shows the Play control as the editor's state has it. Rewritten only when something it
/// shows changed.
#[expect(
    clippy::type_complexity,
    reason = "every piece of the control is looked at once, whatever it is made of"
)]
pub(crate) fn show_play(
    mut commands: Commands,
    editor: NonSend<Editor>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut shown: Local<Option<(PlayView, bool)>>,
    mut pieces: Query<(
        Entity,
        &PlayPiece,
        &mut Node,
        Option<&mut Text>,
        Has<InteractionDisabled>,
    )>,
) {
    if pieces.is_empty() {
        return;
    }
    // In a narrow window the word shortens to its dot; Stop stays, and the status bar
    // carries the words.
    let narrow = toolbar_folds(window.width());
    let view = PlayView::of(&editor);
    if shown.as_ref() == Some(&(view.clone(), narrow)) {
        return;
    }
    let display = |shown: bool| if shown { Display::Flex } else { Display::None };
    for (entity, piece, mut node, text, off) in &mut pieces {
        // What the piece says, if it is a text, and whether it is laid out.
        let (words, laid_out): (Option<String>, bool) = match piece {
            // At rest the control is its button; while something is under way, a word.
            PlayPiece::Menu => (None, view.word().is_none()),
            PlayPiece::Word => {
                let word = view.word();
                let laid_out = word.is_some() && !narrow;
                let color = if view.word_is_running() {
                    SIGNAL_PLAY
                } else {
                    tokens::TEXT_MAIN
                };
                commands.entity(entity).insert(ThemeTextColor(color));
                (Some(word.unwrap_or_default()), laid_out)
            }
            PlayPiece::WordDot => {
                let color = if view.word_is_running() {
                    SIGNAL_PLAY
                } else {
                    tokens::TEXT_DIM
                };
                commands.entity(entity).insert(ThemeBackgroundColor(color));
                (None, view.word().is_some() && narrow)
            }
            PlayPiece::Dot => (None, view.warns()),
            PlayPiece::Stop => {
                set_offered(&mut commands, entity, view.stop_offered(), off);
                (None, view.stop_shown())
            }
            // The rows stay in their places; they are only on or off.
            PlayPiece::GameWindowRow => {
                set_offered(&mut commands, entity, view.game_window_offered(), off);
                (None, true)
            }
            PlayPiece::CheckRow => {
                set_offered(&mut commands, entity, view.check_offered(), off);
                (None, true)
            }
            PlayPiece::CookAndPlayRow => {
                set_offered(&mut commands, entity, view.cook_and_play_offered(), off);
                (None, true)
            }
            PlayPiece::ReasonLine => (Some(view.reason_line().to_owned()), true),
            PlayPiece::CookedLine => (Some(view.cooked_line()), true),
            PlayPiece::StandingLine => {
                let line = view.standing_line();
                let color = if view.cooked.is_some() {
                    SIGNAL_WARN
                } else {
                    tokens::TEXT_MAIN
                };
                commands.entity(entity).insert(ThemeTextColor(color));
                (Some(line.to_owned()), !line.is_empty())
            }
            PlayPiece::CheckLine => {
                let line = view.check_line();
                let color = if view.check_failed() {
                    SIGNAL_ERROR
                } else {
                    tokens::TEXT_MAIN
                };
                commands.entity(entity).insert(ThemeTextColor(color));
                let laid_out = !line.is_empty();
                (Some(line), laid_out)
            }
            PlayPiece::FootButton => (None, view.foot_button().is_some()),
            PlayPiece::FootButtonCaption => (view.foot_button().map(str::to_owned), true),
        };
        if let (Some(words), Some(mut text)) = (words, text)
            && text.0 != words
        {
            text.0 = words;
        }
        if node.display != display(laid_out) {
            node.display = display(laid_out);
        }
    }
    *shown = Some((view, narrow));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(cooked: Option<&str>, older: bool, check: PackageCheck) -> PlayView {
        PlayView {
            cooked: cooked.map(str::to_owned),
            older,
            can_check: cooked.is_some(),
            check,
            game: true,
            run: RunView::None,
            run_package: None,
            run_older: false,
            steps: String::new(),
            refusal: None,
            ended: None,
            cooking: false,
            restarting: false,
            blockers: 0,
        }
    }

    /// A view of a run of the cooked level `6105ad50` in this state.
    fn running(run: RunView) -> PlayView {
        PlayView {
            run,
            run_package: Some("6105ad50".into()),
            ..view(Some("6105ad50"), false, PackageCheck::Passed { micros: 1 })
        }
    }

    fn refused(code: &str) -> PackageCheck {
        PackageCheck::Refused { code: code.into() }
    }

    #[test]
    fn without_a_cooked_level_the_menu_says_to_cook_and_offers_it() {
        let view = view(None, false, PackageCheck::NotAsked);
        assert_eq!(view.word(), None, "at rest the control is its button");
        assert_eq!(view.cooked_line(), "not cooked");
        assert_eq!(view.standing_line(), "Cook the level to play it.");
        assert_eq!(view.foot_button(), Some("Cook"));
        assert!(!view.warns());
        assert!(!view.check_offered(), "there is nothing to check");
        assert_eq!(view.status_words(), "not cooked");
    }

    #[test]
    fn a_current_cooked_level_is_named_by_its_hash_and_needs_no_button() {
        let view = view(Some("9f3a61c2"), false, PackageCheck::NotAsked);
        assert_eq!(view.cooked_line(), "cooked 9f3a61c2");
        assert_eq!(view.standing_line(), "");
        assert_eq!(view.check_line(), "");
        assert_eq!(view.foot_button(), None);
        assert!(view.check_offered());
        assert_eq!(view.status_words(), "cooked 9f3a61c2");
    }

    #[test]
    fn a_level_that_moved_on_warns_and_offers_to_cook_first_and_can_still_be_checked() {
        let view = view(Some("9f3a61c2"), true, PackageCheck::NotAsked);
        assert!(view.warns());
        assert!(view.standing_line().contains("has changed since this cook"));
        assert_eq!(view.foot_button(), Some("Cook first"));
        assert!(
            view.check_offered(),
            "the older cook is still what would run"
        );
        assert_eq!(view.status_words(), "cooked 9f3a61c2, older than the level");
    }

    #[test]
    fn while_checking_the_control_says_so_with_the_hash_and_shows_stop() {
        let view = view(Some("9f3a61c2"), false, PackageCheck::Checking);
        assert_eq!(view.word().as_deref(), Some("Checking 9f3a61c2"));
        assert!(view.stop_shown());
        assert!(!view.check_offered(), "one check at a time");
        assert_eq!(view.status_words(), "checking cooked 9f3a61c2");
        // And at no other time: there is nothing else to stop yet.
        for check in [
            PackageCheck::NotAsked,
            PackageCheck::Passed { micros: 1 },
            refused("payload.hash_mismatch"),
        ] {
            let view = self::view(Some("9f3a61c2"), false, check);
            assert!(!view.stop_shown());
            assert_eq!(view.word(), None);
        }
    }

    #[test]
    fn a_check_that_passed_is_said_in_the_foot_and_the_status_bar() {
        let view = view(
            Some("9f3a61c2"),
            false,
            PackageCheck::Passed { micros: 420 },
        );
        assert!(view.check_line().starts_with("Checked:"));
        assert!(!view.check_failed());
        assert_eq!(view.status_words(), "cooked 9f3a61c2, checked");
    }

    #[test]
    fn a_refused_package_is_said_in_words_with_its_remedy_and_never_as_a_code() {
        for code in [
            "payload.hash_mismatch",
            "package.identity_mismatch",
            "package.directory_identity",
            "package.decode",
        ] {
            let view = view(Some("9f3a61c2"), false, refused(code));
            assert!(view.check_failed());
            let line = view.check_line();
            assert!(!line.contains(code), "{code}: {line}");
            assert!(line.contains("Cook the level again"), "{code}: {line}");
            assert_eq!(view.status_words(), "cooked 9f3a61c2, check failed");
            // The package can be checked again, for instance after it was put back.
            assert!(view.check_offered());
        }
    }

    #[test]
    fn a_level_cooked_before_this_editor_started_cannot_be_checked() {
        let view = PlayView {
            can_check: false,
            ..view(Some("9f3a61c2"), false, PackageCheck::NotAsked)
        };
        assert!(!view.check_offered());
        assert!(
            !view.game_window_offered(),
            "nor played: there is no receipt"
        );
    }

    #[test]
    fn a_cooked_level_is_shown_by_its_manifests_hash_and_by_the_simulations_only_without_it() {
        let manifest = "6105ad50c97caf0b488b5147c68f30e7";
        let simulation = "14571ca6aa00bb11cc22dd33ee44ff55";
        assert_eq!(
            package_id(Some(manifest), Some(simulation)).as_deref(),
            Some("6105ad50")
        );
        assert_eq!(
            package_id(None, Some(simulation)).as_deref(),
            Some("14571ca6")
        );
        assert_eq!(package_id(None, None), None);
    }

    #[test]
    fn without_a_game_the_way_to_play_is_off_and_the_foot_says_how_to_get_one() {
        let view = PlayView {
            game: false,
            ..view(Some("6105ad50"), false, PackageCheck::NotAsked)
        };
        assert!(!view.game_window_offered());
        assert!(
            view.check_offered(),
            "the cooked level can still be checked"
        );
        assert!(view.reason_line().contains("--game"));
        assert!(view.reason_line().contains("cooked and checked"));
    }

    #[test]
    fn with_a_game_and_a_cooked_level_the_game_window_is_offered_and_preview_is_not_built() {
        let view = view(Some("6105ad50"), false, PackageCheck::NotAsked);
        assert!(view.game_window_offered());
        assert_eq!(view.reason_line(), NO_PREVIEW);
        // Not without a cooked level, and not while the cooked level is being checked.
        assert!(!self::view(None, false, PackageCheck::NotAsked).game_window_offered());
        assert!(!self::view(Some("6105ad50"), false, PackageCheck::Checking).game_window_offered());
    }

    #[test]
    fn a_run_is_a_word_and_one_stop_in_every_state() {
        for (run, word) in [
            (RunView::Starting, "Starting"),
            (RunView::Running, "Game window open"),
            (RunView::Stopping, "Stopping"),
        ] {
            let view = running(run);
            assert_eq!(view.word().as_deref(), Some(word));
            assert!(view.stop_shown(), "{run:?}");
            assert!(!view.game_window_offered(), "{run:?}: one run at a time");
            assert!(!view.check_offered(), "{run:?}");
        }
        assert_eq!(running(RunView::None).word(), None);
        assert!(!running(RunView::None).stop_shown());
    }

    #[test]
    fn stop_cannot_be_pressed_twice_and_a_start_can_be_stopped() {
        assert!(running(RunView::Starting).stop_offered());
        assert!(running(RunView::Running).stop_offered());
        let stopping = running(RunView::Stopping);
        assert!(stopping.stop_shown() && !stopping.stop_offered());
    }

    #[test]
    fn only_a_game_that_is_ready_is_shown_in_the_play_colour() {
        assert!(running(RunView::Running).word_is_running());
        assert!(!running(RunView::Starting).word_is_running());
        assert!(!running(RunView::Stopping).word_is_running());
        let checking = PlayView {
            check: PackageCheck::Checking,
            ..running(RunView::Running)
        };
        assert!(!checking.word_is_running());
    }

    #[test]
    fn the_status_bar_says_where_the_run_stands_with_its_cooked_level() {
        assert_eq!(
            running(RunView::Starting).run_words(),
            "starting cooked 6105ad50"
        );
        let starting = PlayView {
            steps: "server collision ready, client level verified".into(),
            ..running(RunView::Starting)
        };
        assert_eq!(
            starting.run_words(),
            "starting cooked 6105ad50: server collision ready, client level verified"
        );
        assert_eq!(
            running(RunView::Running).run_words(),
            "playing cooked 6105ad50"
        );
        let older = PlayView {
            run_older: true,
            ..running(RunView::Running)
        };
        assert_eq!(
            older.run_words(),
            "playing cooked 6105ad50, older than the level"
        );
        assert_eq!(running(RunView::Stopping).run_words(), "stopping the game");
        assert_eq!(running(RunView::None).run_words(), "");
    }

    #[test]
    fn an_end_the_person_asked_for_is_in_the_status_bar_and_any_other_is_not() {
        let ended = |code: &str| PlayView {
            ended: Some(code.into()),
            ..running(RunView::None)
        };
        assert_eq!(ended("play.stopped").run_words(), "Stopped");
        assert_eq!(
            ended("play.window_closed").run_words(),
            "The game window was closed"
        );
        // A crash and a failed start are notices: the status bar does not repeat them.
        assert_eq!(ended("play.crashed").run_words(), "");
        assert_eq!(ended("play.start_failed").run_words(), "");
    }

    #[test]
    fn a_refusal_is_one_sentence_in_the_status_bar_and_needs_attention() {
        let refused = PlayView {
            refusal: Some("play.already_active".into()),
            ..running(RunView::Running)
        };
        assert_eq!(
            refused.run_words(),
            "A game is already running. Stop it first."
        );
        assert!(refused.run_words_warn());
        assert!(!running(RunView::Running).run_words_warn());
    }

    #[test]
    fn cook_and_play_needs_a_game_and_nothing_under_way_but_no_cooked_level() {
        assert!(view(None, false, PackageCheck::NotAsked).cook_and_play_offered());
        assert!(view(Some("6105ad50"), true, PackageCheck::NotAsked).cook_and_play_offered());
        let blocked = PlayView {
            blockers: 2,
            ..view(None, false, PackageCheck::NotAsked)
        };
        assert!(
            blocked.cook_and_play_offered(),
            "it is offered, and says what stops it when chosen"
        );
        let no_game = PlayView {
            game: false,
            ..view(Some("6105ad50"), false, PackageCheck::NotAsked)
        };
        assert!(!no_game.cook_and_play_offered());
        assert!(!view(Some("6105ad50"), false, PackageCheck::Checking).cook_and_play_offered());
        assert!(!running(RunView::Running).cook_and_play_offered());
    }

    #[test]
    fn the_cook_before_a_start_is_a_word_of_its_own_with_stop() {
        let cooking = PlayView {
            cooking: true,
            run: RunView::Starting,
            ..view(None, false, PackageCheck::NotAsked)
        };
        assert_eq!(cooking.word().as_deref(), Some("Cooking"));
        assert!(cooking.stop_shown() && cooking.stop_offered());
        assert_eq!(cooking.run_words(), "cooking the level to play it");
        assert!(!cooking.word_is_running());
    }

    #[test]
    fn a_run_stopped_to_be_played_again_says_what_follows() {
        let restarting = PlayView {
            restarting: true,
            ..running(RunView::Stopping)
        };
        assert_eq!(restarting.word().as_deref(), Some("Stopping"));
        assert_eq!(
            restarting.run_words(),
            "stopping the game to cook the level and play it again"
        );
        assert!(!restarting.stop_offered(), "Stop cannot be pressed twice");
    }

    #[test]
    fn the_refusals_of_the_one_key_are_sentences_and_never_codes() {
        for code in ["play.no_game", "play.blocked", "play.already_active"] {
            let refused = PlayView {
                refusal: Some(code.into()),
                ..view(None, false, PackageCheck::NotAsked)
            };
            assert!(refused.run_words_warn());
            assert!(!refused.run_words().contains(code), "{code}");
        }
    }

    #[test]
    fn every_step_a_process_reports_has_words() {
        for stage in [
            PlayStage::PackageVerified,
            PlayStage::CollisionReady,
            PlayStage::HandshakeAccepted,
            PlayStage::Simulating,
            PlayStage::LevelDrawn,
            PlayStage::FirstControllableFrame,
        ] {
            assert!(!stage_words(stage).is_empty());
        }
    }
}
