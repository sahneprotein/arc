//! The Play control of the design system: the one place the game is started and stopped from.
//!
//! What is built is its first step. The control is in the toolbar at every width. Its menu
//! names the cooked level that would run and how it stands to the level being edited, offers
//! to cook, and lets the person ask the editor's core whether the cooked level on disk is still
//! the one this editor cooked. The two ways to play are in the menu and are off: the core
//! cannot start the game yet (`crates/editor-services/docs/play-boundary.md`), and the menu
//! says so. Nothing here shows a game as starting or running.
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
};

use crate::{
    panels::menu::{menu_item, menu_popup_end},
    session::{Editor, PackageCheck},
    theme::{
        feathers::{BORDER, SIGNAL_ERROR, SIGNAL_WARN},
        size, spacing, text,
    },
    wording,
};

/// The two ways to play, as the menu names them.
pub const PREVIEW_IN_EDITOR: &str = "Preview in Editor";
pub const OPEN_GAME_WINDOW: &str = "Open Game Window";
/// The row that asks the core about the cooked level on disk.
pub const CHECK_COOKED_LEVEL: &str = "Check cooked level";

/// Why the two ways to play are off. It stays in the menu's foot as long as they are.
pub const NOT_YET: &str = "The editor cannot start the game yet. \
    Until it can, the cooked level can be cooked and checked here.";

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
        Self {
            older: cooked.is_some() && status.cook_needed,
            can_check: receipt.is_some(),
            cooked,
            check: editor.package_check().clone(),
        }
    }

    fn checking(&self) -> bool {
        self.check == PackageCheck::Checking
    }

    /// The word the control shows in place of its button while something is under way: the
    /// state and the cooked level's hash. `None` at rest, when the control is its button.
    pub fn word(&self) -> Option<String> {
        match (&self.cooked, self.checking()) {
            (Some(hash), true) => Some(format!("Checking {hash}")),
            (None, true) => Some("Checking".into()),
            _ => None,
        }
    }

    /// Whether the control carries the warning dot: there is a cooked level, and the level
    /// has moved on from it.
    pub fn warns(&self) -> bool {
        self.older
    }

    /// Whether the control shows Stop. While a check runs it cancels the check; there is
    /// nothing else to stop yet.
    pub fn stop_shown(&self) -> bool {
        self.checking()
    }

    /// Whether the row that checks the cooked level can be chosen.
    pub fn check_offered(&self) -> bool {
        self.can_check && !self.checking()
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
    /// The warning dot on the control's button.
    Dot,
    /// The Stop button beside the control.
    Stop,
    /// The row that checks the cooked level.
    CheckRow,
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
                Text(NOT_YET)
                TextFont {
                    font: FontSourceTemplate::Handle(fonts::REGULAR),
                    font_size: {FontSize::Px(text::SMALL.size)},
                }
                PropagateOver<TextFont>
                ThemeTextColor(tokens::TEXT_DIM)
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
                            (menu_item(OPEN_GAME_WINDOW, "") InteractionDisabled),
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
                @FeathersButton {
                    @caption: bsn! { Text("Stop") ThemedText },
                    @variant: ButtonVariant::Primary,
                }
                PlayPiece::Stop
                Node { display: Display::None }
                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.cancel_check())
            ),
        ]
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
    mut shown: Local<Option<PlayView>>,
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
    let view = PlayView::of(&editor);
    if shown.as_ref() == Some(&view) {
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
                let laid_out = word.is_some();
                (Some(word.unwrap_or_default()), laid_out)
            }
            PlayPiece::Dot => (None, view.warns()),
            PlayPiece::Stop => (None, view.stop_shown()),
            PlayPiece::CheckRow => {
                // The row stays in its place; it is only on or off.
                let wanted_off = !view.check_offered();
                if wanted_off && !off {
                    commands.entity(entity).insert(InteractionDisabled);
                } else if !wanted_off && off {
                    commands.entity(entity).remove::<InteractionDisabled>();
                }
                (None, true)
            }
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
    *shown = Some(view);
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
            cooked: Some("9f3a61c2".into()),
            older: false,
            can_check: false,
            check: PackageCheck::NotAsked,
        };
        assert!(!view.check_offered());
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
    fn the_reason_the_game_cannot_be_played_names_what_can_be_done_instead() {
        assert!(NOT_YET.contains("cannot start the game yet"));
        assert!(NOT_YET.contains("cooked and checked"));
    }
}
