//! Leaving a level with unsaved changes: the one question the editor asks.
//!
//! Four things leave the open level: closing the window, leaving the project for the project
//! browser, opening another level, and beginning a new one. Each asks to [`Leave`]. If the level is saved it happens at once. If
//! not, a dialog offers to save first, to discard the changes, or to keep editing. Saving
//! goes through the core like any save; if it fails the editor stays where it is and the
//! notice says why.
//!
//! Closing the editor, or leaving the project, also leaves the project's particle emitters
//! that have changes: they are asked about in the same dialog, and saving first saves them
//! too. Opening another level or beginning a new one does not leave them; they are the
//! project's, not the level's.
//!
//! The same holds for the project's settings that were changed in the Project settings and
//! not saved. Saving first saves them as the dialog's own button does, with what protects
//! that save: a project file another program changed is not written over, and settings that
//! cannot be saved are not. The editor then stays open on the Project settings, which say
//! why.
//!
//! The window does not close by itself: the application turns that off, and a request to
//! close arrives here. Closing, or leaving the project, while a game the editor started runs
//! goes on in `closing.rs`: the game is stopped first.

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
    launcher::Launcher,
    panels::{
        closing::{self, Closing, Goal, Shutdown},
        project::ProjectDialog,
    },
    particles::Particles,
    project::Project,
    session::Editor,
    theme::{
        feathers::{BORDER, SCRIM},
        radius, spacing,
    },
    tool::Tool,
};

/// What leaves the open level.
#[derive(Event, Clone, PartialEq, Eq, Debug, Default)]
pub enum Leave {
    /// The editor closes.
    #[default]
    Close,
    /// The open project is left for the project browser, where another is chosen.
    Project,
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
            Self::Project => "Save and leave",
            Self::Open(_) => "Save and open",
            Self::New(_) => "Save and begin",
        }
    }

    /// Whether the project is left with the level: the editor closes, or another project is
    /// chosen. Its emitters, its settings and a game it plays go with it.
    fn leaves_project(&self) -> bool {
        matches!(self, Self::Close | Self::Project)
    }

    /// The line of the dialog that says a game runs and is stopped first.
    fn game_runs_line(&self) -> &'static str {
        match self {
            Self::Project => "A game is running. It is stopped before the project is left.",
            _ => "A game is running. It is stopped before the editor closes.",
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

/// The dialog's question and the line under it, which say what has changes.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, FromTemplate)]
pub enum UnsavedLine {
    #[default]
    Question,
    Consequence,
}

/// What the dialog asks and what it says is at stake: the level, emitters of the game, the
/// project's settings, or several of them. `emitters` is how many emitters have changes that
/// leaving would lose, `project` whether the project's settings have.
pub fn unsaved_words(level: bool, emitters: usize, project: bool) -> (String, String) {
    let count = match emitters {
        1 => "1 emitter has".to_owned(),
        emitters => format!("{emitters} emitters have"),
    };
    // What has changes, as the question names it and as the line under it says it.
    let (mut named, mut said): (Vec<&str>, String) = match (level, emitters) {
        (false, 0) if project => (Vec::new(), String::new()),
        (_, 0) => (
            vec!["the level"],
            "The level has changes that are not saved".into(),
        ),
        (false, _) => (
            vec!["the emitters"],
            format!("{count} changes that are not saved"),
        ),
        (true, _) => (
            vec!["the level", "the emitters"],
            format!("The level has changes that are not saved, and {count}"),
        ),
    };
    if project {
        said = if named.is_empty() {
            "The project settings have changes that are not saved".into()
        } else {
            format!("{said}, and so have the project settings")
        };
        named.push("the project settings");
    }
    let named = match named.as_slice() {
        [one] => (*one).to_owned(),
        [first, second] => format!("{first} and {second}"),
        [first, second, third] => format!("{first}, {second} and {third}"),
        _ => "the level".to_owned(),
    };
    (
        format!("Save changes to {named}?"),
        format!("{said}. Discarding them cannot be undone."),
    )
}

/// How many emitters leaving would lose the changes of: the project's when the editor closes
/// or the project is left, none when only the level is left.
fn emitters_left(then: &Leave, particles: &Particles) -> usize {
    match then {
        Leave::Close | Leave::Project => particles.unsaved_count(),
        Leave::Open(_) | Leave::New(_) => 0,
    }
}

/// Whether leaving would lose changes to the project's settings: when the editor closes or
/// the project is left, and they were changed and not saved. Opening another level or
/// beginning one leaves them.
fn project_left(then: &Leave, project: &Project) -> bool {
    then.leaves_project() && project.settings_unsaved()
}

/// Saves the project's settings before the project is left, if leaving would lose them.
/// Whether closing can go on. If they are not saved, because another program changed the
/// project file or because they are settings that cannot be saved, nothing is written and
/// the Project settings are opened, which say why.
fn save_project_first(then: &Leave, project: &mut Project, dialog: &mut ProjectDialog) -> bool {
    if !project_left(then, project) || project.save_settings() {
        return true;
    }
    dialog.open = true;
    false
}

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
                (label("Save changes to the level?") UnsavedLine::Question),
                (
                    label_dim("The level has changes that are not saved. Discarding them cannot be undone.")
                    UnsavedLine::Consequence
                ),
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
                                (mut shutdown, mut goal): (ResMut<Shutdown>, ResMut<Goal>),
                                mut project: ResMut<Project>,
                                mut editor: NonSendMut<Editor>,
                                mut exit: MessageWriter<AppExit>,
                                mut commands: Commands| {
                                unsaved.question = Question::No;
                                // The project's settings are given up with the rest, also
                                // if a game that will not stop keeps the editor open.
                                if project_left(&unsaved.then, &project) {
                                    project.discard_settings();
                                }
                                let closing = Closing {
                                    shutdown: &mut shutdown,
                                    goal: &mut goal,
                                    exit: &mut exit,
                                    commands: &mut commands,
                                };
                                leave(&unsaved.then, &mut editor, closing, true);
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
                            on(|_: On<Activate>,
                                mut unsaved: ResMut<Unsaved>,
                                mut particles: ResMut<Particles>,
                                mut tool: ResMut<Tool>,
                                (mut project, mut project_dialog): (
                                    ResMut<Project>,
                                    ResMut<ProjectDialog>,
                                ),
                                mut editor: NonSendMut<Editor>| {
                                // The emitters first: they are written at once. One that
                                // cannot be written keeps the editor where it is, on the
                                // Particles tool, whose panel says why.
                                if emitters_left(&unsaved.then, &particles) > 0 && !particles.save_all() {
                                    unsaved.question = Question::No;
                                    *tool = Tool::Particles;
                                    return;
                                }
                                // Then the project's settings, with what protects their
                                // save. Settings that are not saved keep the editor open
                                // on the Project settings.
                                if !save_project_first(
                                    &unsaved.then,
                                    &mut project,
                                    &mut project_dialog,
                                ) {
                                    unsaved.question = Question::No;
                                    return;
                                }
                                if editor.snapshot().status.dirty {
                                    editor.save();
                                }
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
/// the person answered the question about them. Closing and leaving the project go on in
/// `closing.rs`: with a game running the editor does not close or leave yet.
fn leave(then: &Leave, editor: &mut Editor, closing: Closing<'_, '_, '_, '_>, asked: bool) {
    match then {
        Leave::Close => closing::close(editor, closing, Goal::Exit, asked),
        Leave::Project => closing::close(editor, closing, Goal::Browser, asked),
        Leave::Open(file) => editor.open(file.clone()),
        Leave::New(file) => editor.new_level(file.clone()),
    }
}

/// Something wants to leave the level: at once if it is saved, after the question if not.
/// While the question is open or a save is under way, a second request changes nothing.
/// While the project browser is shown there is nothing to ask about: the level behind it is
/// in no project and was never anyone's to save, so the window closes at once.
pub(crate) fn leave_or_ask(
    request: On<Leave>,
    mut unsaved: ResMut<Unsaved>,
    (mut shutdown, mut goal): (ResMut<Shutdown>, ResMut<Goal>),
    (particles, project, launcher): (Res<Particles>, Res<Project>, Res<Launcher>),
    mut editor: NonSendMut<Editor>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    // A second request changes nothing either while the editor is closing around a game.
    if unsaved.question != Question::No || *shutdown != Shutdown::No {
        return;
    }
    if launcher.shown() {
        if *request.event() == Leave::Close {
            exit.write(AppExit::Success);
        }
        return;
    }
    if editor.snapshot().status.dirty
        || emitters_left(request.event(), &particles) > 0
        || project_left(request.event(), &project)
    {
        unsaved.then = request.event().clone();
        unsaved.question = Question::Asking;
    } else {
        let closing = Closing {
            shutdown: &mut shutdown,
            goal: &mut goal,
            exit: &mut exit,
            commands: &mut commands,
        };
        leave(request.event(), &mut editor, closing, false);
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

/// The dialog's texts that change with what is left and what has changes, and the emitters
/// that say whether they have any.
type DialogWords<'w, 's> = (
    Query<
        'w,
        's,
        &'static mut Text,
        (
            With<SaveFirstCaption>,
            Without<UnsavedLine>,
            Without<GameRunsLine>,
        ),
    >,
    Query<
        'w,
        's,
        (&'static UnsavedLine, &'static mut Text),
        (Without<SaveFirstCaption>, Without<GameRunsLine>),
    >,
    Res<'w, Particles>,
    Res<'w, Project>,
);

/// The line that says a game runs, with the words of what is left.
type GameRunsLines<'w, 's> = Query<
    'w,
    's,
    (&'static mut Node, &'static mut Text),
    (
        With<GameRunsLine>,
        Without<UnsavedLine>,
        Without<SaveFirstCaption>,
    ),
>;

/// Finishes a save that was chosen in the dialog, shows or hides the dialog, and has it say
/// what has changes.
pub(crate) fn answer_unsaved(
    mut unsaved: ResMut<Unsaved>,
    (mut shutdown, mut goal): (ResMut<Shutdown>, ResMut<Goal>),
    mut editor: NonSendMut<Editor>,
    mut dialog: Query<&mut Node, (With<UnsavedDialog>, Without<GameRunsLine>)>,
    mut game_runs: GameRunsLines,
    (mut caption, mut lines, particles, project): DialogWords,
    (mut exit, mut commands): (MessageWriter<AppExit>, Commands),
) {
    if unsaved.question == Question::Saving && !editor.busy() {
        unsaved.question = Question::No;
        // A save that failed leaves everything as it is; the notice says why.
        if !editor.snapshot().status.dirty && editor.problem.is_none() {
            let then = unsaved.then.clone();
            let closing = Closing {
                shutdown: &mut shutdown,
                goal: &mut goal,
                exit: &mut exit,
                commands: &mut commands,
            };
            leave(&then, &mut editor, closing, true);
        }
    }
    let line = if unsaved.then.leaves_project()
        && (editor.active_run().is_some() || editor.play_requested())
    {
        Display::Flex
    } else {
        Display::None
    };
    let line_words = unsaved.then.game_runs_line();
    for (mut node, mut text) in &mut game_runs {
        if node.display != line {
            node.display = line;
        }
        if text.0 != line_words {
            line_words.clone_into(&mut text.0);
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
    if unsaved.question == Question::Asking {
        let (question, consequence) = unsaved_words(
            editor.snapshot().status.dirty,
            emitters_left(&unsaved.then, &particles),
            project_left(&unsaved.then, &project),
        );
        for (line, mut text) in &mut lines {
            let words = match line {
                UnsavedLine::Question => question.as_str(),
                UnsavedLine::Consequence => consequence.as_str(),
            };
            if text.0 != words {
                words.clone_into(&mut text.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_question_says_what_has_changes_the_level_the_emitters_or_both() {
        // The level alone: as the dialog always was.
        let (question, consequence) = unsaved_words(true, 0, false);
        assert_eq!(question, "Save changes to the level?");
        assert_eq!(
            consequence,
            "The level has changes that are not saved. Discarding them cannot be undone."
        );
        let (question, consequence) = unsaved_words(false, 1, false);
        assert_eq!(question, "Save changes to the emitters?");
        assert_eq!(
            consequence,
            "1 emitter has changes that are not saved. Discarding them cannot be undone."
        );
        let (question, consequence) = unsaved_words(true, 3, false);
        assert_eq!(question, "Save changes to the level and the emitters?");
        assert_eq!(
            consequence,
            "The level has changes that are not saved, and 3 emitters have. Discarding them \
             cannot be undone."
        );
    }

    #[test]
    fn the_question_names_the_project_settings_alone_and_with_the_rest() {
        let (question, consequence) = unsaved_words(false, 0, true);
        assert_eq!(question, "Save changes to the project settings?");
        assert_eq!(
            consequence,
            "The project settings have changes that are not saved. Discarding them cannot be \
             undone."
        );
        let (question, consequence) = unsaved_words(true, 0, true);
        assert_eq!(
            question,
            "Save changes to the level and the project settings?"
        );
        assert_eq!(
            consequence,
            "The level has changes that are not saved, and so have the project settings. \
             Discarding them cannot be undone."
        );
        let (question, consequence) = unsaved_words(false, 2, true);
        assert_eq!(
            question,
            "Save changes to the emitters and the project settings?"
        );
        assert!(consequence.starts_with("2 emitters have changes that are not saved, and so"));
        let (question, consequence) = unsaved_words(true, 1, true);
        assert_eq!(
            question,
            "Save changes to the level, the emitters and the project settings?"
        );
        assert!(consequence.contains("and 1 emitter has, and so have the project settings."));
    }

    /// A folder set up as a project, with moving forward put on another key and not saved.
    fn project_with_a_change() -> (tempfile::TempDir, Project) {
        let folder = tempfile::tempdir().unwrap();
        let mut project = Project::of_folder(Some(folder.path().into()));
        project.set_up("Game", false).unwrap();
        project.ask_key(crate::project::KeyAction::MoveForward);
        assert!(project.press_key(KeyCode::KeyT));
        (folder, project)
    }

    #[test]
    fn only_closing_the_editor_or_leaving_the_project_leaves_its_settings_and_only_changed_ones() {
        let (folder, mut project) = project_with_a_change();
        assert!(project_left(&Leave::Close, &project));
        assert!(project_left(&Leave::Project, &project));
        assert!(!project_left(&Leave::Open("a".into()), &project));
        assert!(!project_left(&Leave::New("a".into()), &project));
        // Given up, or saved, there is nothing of them to ask about.
        project.discard_settings();
        assert!(!project_left(&Leave::Close, &project));
        let untouched = Project::of_folder(Some(folder.path().into()));
        assert!(!project_left(&Leave::Close, &untouched));
        assert!(!project_left(&Leave::Close, &Project::of_folder(None)));
    }

    #[test]
    fn saving_first_saves_the_project_settings_and_closing_goes_on() {
        let (_folder, mut project) = project_with_a_change();
        let mut dialog = ProjectDialog::default();
        // Opening another level leaves them alone: nothing is written.
        let before = std::fs::read(project.manifest_path().unwrap()).unwrap();
        assert!(save_project_first(
            &Leave::Open("a".into()),
            &mut project,
            &mut dialog
        ));
        assert_eq!(
            std::fs::read(project.manifest_path().unwrap()).unwrap(),
            before
        );
        assert!(project.settings_unsaved());

        assert!(save_project_first(&Leave::Close, &mut project, &mut dialog));
        assert!(!dialog.open && !project.settings_unsaved());
        assert_eq!(project.config().unwrap().controls.move_forward, "T");
        // With nothing of them to save, closing goes on and nothing is written.
        let saved = std::fs::read(project.manifest_path().unwrap()).unwrap();
        assert!(save_project_first(&Leave::Close, &mut project, &mut dialog));
        assert_eq!(
            std::fs::read(project.manifest_path().unwrap()).unwrap(),
            saved
        );
    }

    #[test]
    fn settings_that_cannot_be_saved_keep_the_editor_open_on_the_project_settings() {
        // Another program changed the project file: it is not written over.
        let (_folder, mut project) = project_with_a_change();
        let mut theirs = project.config().unwrap().clone();
        theirs.project.name = "Renamed".into();
        let their_bytes = editor_services::project_spec::canonical_bytes(&theirs).unwrap();
        std::fs::write(project.manifest_path().unwrap(), &their_bytes).unwrap();
        let mut dialog = ProjectDialog::default();
        assert!(!save_project_first(
            &Leave::Close,
            &mut project,
            &mut dialog
        ));
        assert!(dialog.open);
        assert_eq!(
            project.problem().unwrap().code,
            "project.changed_since_read"
        );
        assert_eq!(
            std::fs::read(project.manifest_path().unwrap()).unwrap(),
            their_bytes
        );
        assert!(project.settings_unsaved());

        // Two actions on one key: nothing is written, and the dialog is opened to say so.
        let (_folder, mut project) = project_with_a_change();
        let before = std::fs::read(project.manifest_path().unwrap()).unwrap();
        project.ask_key(crate::project::KeyAction::Jump);
        project.press_key(KeyCode::KeyT);
        let mut dialog = ProjectDialog::default();
        assert!(!save_project_first(
            &Leave::Close,
            &mut project,
            &mut dialog
        ));
        assert!(dialog.open);
        assert_eq!(
            std::fs::read(project.manifest_path().unwrap()).unwrap(),
            before
        );
        assert!(project_left(&Leave::Close, &project));
    }

    #[test]
    fn only_closing_the_editor_or_leaving_the_project_leaves_the_emitters() {
        let game = tempfile::tempdir().unwrap();
        let mut particles = Particles::of_game(game.path().into());
        particles.new_emitter().unwrap();
        assert_eq!(emitters_left(&Leave::Close, &particles), 1);
        assert_eq!(emitters_left(&Leave::Project, &particles), 1);
        assert_eq!(emitters_left(&Leave::Open("a".into()), &particles), 0);
        assert_eq!(emitters_left(&Leave::New("a".into()), &particles), 0);
    }

    #[test]
    fn the_line_about_a_running_game_says_what_is_left() {
        assert!(Leave::Close
            .game_runs_line()
            .ends_with("before the editor closes."));
        assert!(Leave::Project
            .game_runs_line()
            .ends_with("before the project is left."));
        assert!(!Leave::Open("a".into()).leaves_project());
        assert!(!Leave::New("a".into()).leaves_project());
    }

    #[test]
    fn the_button_that_saves_first_says_what_follows() {
        assert_eq!(Leave::Close.save_caption(), "Save and close");
        assert_eq!(Leave::Project.save_caption(), "Save and leave");
        assert_eq!(Leave::Open("a".into()).save_caption(), "Save and open");
        assert_eq!(Leave::New("a".into()).save_caption(), "Save and begin");
    }
}
