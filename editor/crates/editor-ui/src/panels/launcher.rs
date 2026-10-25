//! The project browser's screen: the whole window, before the editor is entered.
//!
//! It is the editor's own window and nothing beside it: the browser covers the window while
//! it is shown and is put away when a project is entered, and no second window or program is
//! started for either. What it shows is the list of projects that were opened before, each
//! with what its folder is now, and two buttons that lead to another project through the
//! system's folder chooser: "Open Project" and "New Project". Without a list the same two
//! buttons are there, under words that say what each is for.
//!
//! What needs the person's word first is shown in the list's place, with what will be done
//! and the button that does it: a new project, with its name and the folder it will be made
//! as, which is said again whenever the name changes; a folder that was opened and is to be
//! set up.
//!
//! Under the list the browser says where the game is that plays every project, or that
//! none is set, with the button that chooses its folder. A project is content and can be
//! made, opened and edited without it. The state is [`Launcher`]; this module shows it and
//! hands it what the person does.

use bevy::{
    app::PropagateOver,
    feathers::{
        constants::fonts,
        controls::{ButtonVariant, FeathersButton, FeathersTextInput, FeathersTextInputContainer},
        display::{label, label_dim},
        theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeTextColor, ThemedText},
        tokens,
    },
    input::{keyboard::KeyboardInput, ButtonState},
    prelude::*,
    text::{EditableText, FontSourceTemplate, TextEdit},
    ui_widgets::{Activate, ScrollArea},
};
use editor_services::{
    project::{self as project_services, ProjectInspectionStatus},
    runtime::{RuntimeFolderStatus, RuntimeLocation, RuntimeLocationStatus},
};
use scene_data::Diagnostic;
use std::path::{Path, PathBuf};

use crate::{
    launcher::{opened_words, FolderFor, Launcher, Pending, Refusal, Row},
    panels::levels::LevelDialog,
    panels::{
        inspector::group_title,
        play::{requirement_words, CHANGE_GAME, CHOOSE_GAME},
        project::{rejected_words, set_up_caption, warning, SETUP_HINT},
    },
    particles::Particles,
    project::Project,
    session::Editor,
    theme::{
        feathers::{BORDER, SELECTION, SIGNAL_WARN},
        radius, spacing, text,
    },
    tool::Tool,
    wording, ProjectDialog,
};

/// The widest the browser's column is.
const COLUMN_WIDTH: f32 = 720.0;

/// The longest name the field of a new project's name takes: what the format lets it be.
const NAME_LENGTH: usize = 96;

/// What the browser is called, and what it says under that.
pub const TITLE: &str = "Arc Editor";
pub const LEAD: &str = "Open a project, or make a new one.";

/// The two buttons that lead to another project.
pub const OPEN_PROJECT: &str = "Open Project";
pub const NEW_PROJECT: &str = "New Project";

/// What the browser says in the list's place while there is no project to list.
pub const NO_PROJECTS: (&str, &str) = (
    "No projects yet",
    "New Project makes a project in a folder of its own, inside a folder you choose: a \
     project file, a first level and a look profile. Open Project opens the folder of a \
     project you already have.",
);

/// What a new project's panel is called, what it says will be made, and its button.
pub const NEW_TITLE: &str = "New project";
pub const NEW_HINT: &str = "The project gets a folder of its own, named after it, inside \
    the folder you chose; nothing else is put into that folder, and nothing in it is \
    changed. In the project's folder are made: project.aproj, a first level (a small \
    arena for two teams) and the look profile the game is drawn with. The game itself is \
    not part of a project.";
pub const CREATE: &str = "Create project";
/// What a folder that was opened and is no project is called.
pub const SET_UP_TITLE: &str = "This folder is not a project yet";

/// What the browser says of the game, above where it is.
pub const GAME_TITLE: &str = "Game";
pub const GAME_HINT: &str = "A project is content: its levels and its look. The game that \
    plays it is set here once, for this editor and every project.";

/// What it says where the system has no place to keep the list in.
pub const NO_LIST: &str = "This system has no place to keep the list of projects in, so none \
    is shown. Projects can still be opened and made.";

/// What the buttons of a row say.
pub const OPEN_ROW: &str = "Open";
pub const FORGET_ROW: &str = "Remove from list";

/// What a new project's field is called, and what leaves a setup or a conversion.
pub const PROJECT_NAME: &str = "Project name";
pub const BACK: &str = "Back to projects";

/// What a row says of its folder besides its path, and whether it is said as a warning: a
/// project that is ready says nothing.
pub fn row_words(status: &ProjectInspectionStatus) -> Option<(String, bool)> {
    match status {
        ProjectInspectionStatus::Ready { .. } => None,
        ProjectInspectionStatus::MissingStartLevel { start_level, .. } => {
            let level = start_level
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            Some((
                format!(
                    "Its start level, {level}, is not in the folder. Opening it uses an \
                     unsaved map; the Project settings say how to put that right."
                ),
                true,
            ))
        }
        ProjectInspectionStatus::SetupRequired { .. } => Some((
            "Not a project yet: opening it offers to set the folder up.".into(),
            false,
        )),
        ProjectInspectionStatus::Missing => Some(("The folder is not there.".into(), true)),
        ProjectInspectionStatus::Refused { problem } => {
            let words = wording::project(&problem.code, |key| {
                problem.parameters.get(key).map(String::as_str)
            });
            Some((format!("{}.", words.title), true))
        }
    }
}

/// Why the folder that was last chosen was not entered: a title, what to do, and the folder.
pub fn refusal_words(refusal: &Refusal) -> (String, String, String) {
    match refusal {
        Refusal::Missing(folder) => (
            "The folder is not there".into(),
            "It was moved, renamed or removed, or it is on a disk that is not connected. \
             Nothing was changed."
                .into(),
            folder.display().to_string(),
        ),
        Refusal::AlreadyProject(folder) => (
            "This folder is a project already".into(),
            "A new project is not made over it. Open it with Open Project, or choose another \
             folder for the new project."
                .into(),
            folder.display().to_string(),
        ),
        Refusal::Refused(folder, problem) => {
            let words = wording::project(&problem.code, |key| {
                problem.parameters.get(key).map(String::as_str)
            });
            (words.title, words.remedy, folder.display().to_string())
        }
    }
}

/// What the browser says when the list of projects could not be read or written.
pub fn list_problem_words(code: &str) -> (String, String) {
    (
        "The list of projects cannot be used".into(),
        format!(
            "It is left as it is, and no project is added to it or taken from it until it \
             can be read ({code}). Projects can still be opened and made."
        ),
    )
}

/// Where a new project would be made under the name in its field, in words, and whether
/// that is a warning: the folder it would be made as, or why it cannot be made there.
pub fn new_folder_words(found: &Result<PathBuf, Diagnostic>) -> (String, bool) {
    match found {
        Ok(folder) => (format!("Will be made as {}", folder.display()), false),
        Err(refusal) => {
            let words = wording::project(&refusal.code, |key| {
                refusal.parameters.get(key).map(String::as_str)
            });
            let named = match (refusal.code.as_str(), refusal.parameters.get("path")) {
                ("project.folder_taken", Some(path)) => format!(" ({path})"),
                _ => String::new(),
            };
            let remedy = match refusal.code.as_str() {
                // Before anything is asked for, nothing was refused yet: only what to do.
                "project.folder_taken" => {
                    "Give the project another name, or choose another folder to make it in."
                }
                "project.value" => "Give the project a name of at most 96 characters.",
                _ => words.remedy.as_str(),
            };
            (format!("{}{named}. {remedy}", words.title), true)
        }
    }
}

/// What the browser says of the game's folder: what it is now, the folder if one is set,
/// whether that is a warning, and what its button says. Without a place to keep the
/// setting in there is no button.
pub fn game_folder_words(
    location: &RuntimeLocation,
) -> (String, Option<String>, bool, Option<&'static str>) {
    let refused = |problem: &Diagnostic| {
        let words = wording::game_folder(&problem.code, |key| {
            problem.parameters.get(key).map(String::as_str)
        });
        format!("{}. {}", words.title, words.remedy)
    };
    match &location.status {
        RuntimeLocationStatus::Unconfigured => (
            "No game folder is set. Projects can be made, opened, edited, cooked and \
             checked without one; playing a level needs the game. Choose the folder the \
             game is built in."
                .into(),
            None,
            false,
            Some(CHOOSE_GAME),
        ),
        RuntimeLocationStatus::Configured { root, folder } => {
            let path = Some(root.display().to_string());
            match &folder.status {
                RuntimeFolderStatus::Ready { .. } => (
                    "Every project is played with the game in this folder.".into(),
                    path,
                    false,
                    Some(CHANGE_GAME),
                ),
                RuntimeFolderStatus::Missing => (
                    refused(&Diagnostic::new("runtime.folder_missing")),
                    path,
                    true,
                    Some(CHANGE_GAME),
                ),
                RuntimeFolderStatus::Incomplete { requirements } => (
                    format!(
                        "This folder does not hold the whole game, so nothing can be played. \
                         Not there: {}. Build the game there, or choose another folder.",
                        requirement_words(requirements)
                    ),
                    path,
                    true,
                    Some(CHANGE_GAME),
                ),
                RuntimeFolderStatus::Refused { problem } => {
                    (refused(problem), path, true, Some(CHANGE_GAME))
                }
            }
        }
        RuntimeLocationStatus::Refused { problem } => {
            (refused(problem), None, true, Some(CHOOSE_GAME))
        }
        RuntimeLocationStatus::PreferenceUnavailable => (
            refused(&Diagnostic::new("runtime.preference_unavailable")),
            None,
            true,
            None,
        ),
    }
}

/// Why the folder that was last chosen for the game was not taken: a title, what to do, and
/// the folder. The setting is as it was.
pub fn game_refusal_words(folder: &Path, refusal: &Diagnostic) -> (String, String, String) {
    let words = wording::game_folder(&refusal.code, |key| {
        refusal.parameters.get(key).map(String::as_str)
    });
    (
        words.title,
        format!("{} The game folder is as it was.", words.remedy),
        folder.display().to_string(),
    )
}

/// The browser, over the whole window.
#[derive(Component, Default, Clone)]
pub struct LauncherRoot;

/// What the browser shows between its title and its foot.
#[derive(Component, Default, Clone)]
pub struct LauncherBody;

/// The foot with the two buttons that lead to another project.
#[derive(Component, Default, Clone)]
pub struct LauncherFoot;

/// A button of a row of the list, with the row's place in it.
#[derive(Component, Default, Clone, Copy)]
pub struct LauncherRow(pub usize);

/// The button that takes a row out of the list.
#[derive(Component, Default, Clone, Copy)]
pub struct ForgetRow(pub usize);

/// The field of a new project's name, with the name it is filled with.
#[derive(Component, Default, Clone)]
pub struct LauncherNameInput(pub String);

/// The line that says where a new project will be made under the name in the field.
#[derive(Component, Default, Clone)]
pub struct LauncherNewFolder;

/// The button that makes the new project.
#[derive(Component, Default, Clone)]
pub struct LauncherCreate;

/// The button that asks for the game's folder.
#[derive(Component, Default, Clone)]
pub struct LauncherChooseGame;

/// The button that sets the pending folder up. `fresh_starter` is as in the Project
/// settings: pressing it agrees to a new starter level beside files that cannot be read.
#[derive(Component, Default, Clone, Copy)]
pub struct LauncherSetUp {
    pub fresh_starter: bool,
}

fn title_line() -> impl Scene {
    bsn! {
        Text(TITLE)
        TextFont {
            font: FontSourceTemplate::Handle(fonts::BOLD),
            font_size: {FontSize::Px(text::TITLE.size)},
        }
        PropagateOver<TextFont>
        ThemeTextColor(tokens::TEXT_MAIN)
    }
}

/// A path, in the type style paths and hashes are written in.
fn path_line(path: String) -> impl Scene {
    bsn! {
        Text({path})
        TextFont {
            font: FontSourceTemplate::Handle(fonts::MONO),
            font_size: {FontSize::Px(text::NUMBER.size)},
        }
        PropagateOver<TextFont>
        ThemeTextColor(tokens::TEXT_DIM)
    }
}

pub fn launcher() -> impl Scene {
    bsn! {
        LauncherRoot
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            top: px(0),
            bottom: px(0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            padding: UiRect::all(px(spacing::SPACE_6)),
            display: Display::None,
        }
        // In front of everything the editor has, and it takes every click: while the
        // browser is shown there is no editor to use behind it.
        GlobalZIndex(200)
        ThemeBackgroundColor(tokens::WINDOW_BG)
        Children [(
            Node {
                width: px(COLUMN_WIDTH),
                max_width: percent(100),
                max_height: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(spacing::SPACE_6),
            }
            Children [
                (
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(spacing::SPACE_2),
                        flex_shrink: 0.0,
                    }
                    Children [ title_line(), label_dim(LEAD) ]
                ),
                (
                    LauncherBody
                    ScrollArea
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(spacing::SPACE_4),
                        min_height: px(0),
                        overflow: Overflow::scroll_y(),
                    }
                ),
                (
                    LauncherFoot
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::End,
                        column_gap: px(spacing::SPACE_4),
                        flex_shrink: 0.0,
                    }
                    Children [
                        (
                            @FeathersButton { @caption: bsn! { Text(NEW_PROJECT) ThemedText } }
                            Node { flex_shrink: 0.0 }
                            on(|_: On<Activate>, mut launcher: ResMut<Launcher>| {
                                launcher.ask_folder(FolderFor::New);
                            })
                        ),
                        (
                            @FeathersButton {
                                @caption: bsn! { Text(OPEN_PROJECT) ThemedText },
                                @variant: ButtonVariant::Primary,
                            }
                            Node { flex_shrink: 0.0 }
                            on(|_: On<Activate>, mut launcher: ResMut<Launcher>| {
                                launcher.ask_folder(FolderFor::Open);
                            })
                        ),
                    ]
                ),
            ]
        )]
    }
}

fn open_row(activate: On<Activate>, rows: Query<&LauncherRow>, mut launcher: ResMut<Launcher>) {
    if let Ok(row) = rows.get(activate.entity) {
        launcher.open_row(row.0);
    }
}

fn forget_row(activate: On<Activate>, rows: Query<&ForgetRow>, mut launcher: ResMut<Launcher>) {
    if let Ok(row) = rows.get(activate.entity) {
        launcher.forget_row(row.0);
    }
}

/// A project of the list: its name, its folder, what the folder is now where that is not a
/// ready project, when it was last opened, and the button that opens it. The chosen row has
/// the selection's background and the primary button. A row that leads nowhere has no
/// button to open it; every row can be taken out of the list.
fn project_row(index: usize, row: &Row, chosen: bool, now_unix_ms: u64) -> Box<dyn SceneList> {
    let name = row.recent.name.clone();
    let path = row.recent.root.display().to_string();
    let opened = opened_words(row.recent.last_opened_unix_ms, now_unix_ms);
    let said: Box<dyn SceneList> = match row_words(&row.status) {
        Some((words, true)) => Box::new(bsn_list![(label(words) ThemeTextColor(SIGNAL_WARN))]),
        Some((words, false)) => Box::new(bsn_list![label_dim(words)]),
        None => Box::new(bsn_list![]),
    };
    let variant = if chosen {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Normal
    };
    let open: Box<dyn SceneList> = if row.can_open() {
        Box::new(bsn_list![(
            @FeathersButton { @caption: bsn! { Text(OPEN_ROW) ThemedText }, @variant: {variant} }
            LauncherRow({index})
            Node { flex_shrink: 0.0 }
            on(open_row)
        )])
    } else {
        Box::new(bsn_list![])
    };
    let content = bsn_list![
        (
            Node {
                flex_grow: 1.0,
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(spacing::SPACE_1),
            }
            Children [ label(name), path_line(path), {said} ]
        ),
        (Node { flex_shrink: 0.0 } Children [ label_dim(opened) ]),
        {open},
        (
            @FeathersButton { @caption: bsn! { Text(FORGET_ROW) ThemedText } }
            ForgetRow({index})
            Node { flex_shrink: 0.0 }
            on(forget_row)
        ),
    ];
    if chosen {
        Box::new(bsn_list![(
            row_frame()
            ThemeBackgroundColor(SELECTION)
            Children [ {content} ]
        )])
    } else {
        Box::new(bsn_list![(
            row_frame()
            Children [ {content} ]
        )])
    }
}

/// The frame of a row of the list.
fn row_frame() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(spacing::SPACE_4),
            padding: UiRect::all(px(spacing::SPACE_4)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(radius::RADIUS_CONTROL)),
            flex_shrink: 0.0,
        }
        ThemeBorderColor(BORDER)
    }
}

fn set_up(
    activate: On<Activate>,
    names: Query<&EditableText, With<LauncherNameInput>>,
    buttons: Query<&LauncherSetUp>,
    mut launcher: ResMut<Launcher>,
) {
    let offered = match launcher.pending() {
        Some(Pending::SetUp { name, .. }) => name.clone(),
        _ => return,
    };
    let name = names
        .iter()
        .next()
        .map_or(offered, |text| text.value().to_string());
    let fresh_starter = buttons
        .get(activate.entity)
        .is_ok_and(|button| button.fresh_starter);
    launcher.set_up(&name, fresh_starter);
}

/// Makes the new project under the name in its field.
fn create(
    _: On<Activate>,
    names: Query<&EditableText, With<LauncherNameInput>>,
    mut launcher: ResMut<Launcher>,
) {
    let offered = match launcher.pending() {
        Some(Pending::New { name, .. }) => name.clone(),
        _ => return,
    };
    let name = names
        .iter()
        .next()
        .map_or(offered, |text| text.value().to_string());
    launcher.create(&name);
}

fn back_button() -> impl Scene {
    bsn! {
        @FeathersButton { @caption: bsn! { Text(BACK) ThemedText } }
        Node { flex_shrink: 0.0 }
        on(|_: On<Activate>, mut launcher: ResMut<Launcher>| launcher.back())
    }
}

/// The folder that waits for the person's word: what it is, what will be done in it, and
/// the button that does it.
fn pending_rows(pending: &Pending) -> Vec<Box<dyn SceneList>> {
    let mut rows: Vec<Box<dyn SceneList>> = Vec::new();
    let folder = pending.folder().display().to_string();
    match pending {
        Pending::New { parent, name } => {
            let (will_be, warns) =
                new_folder_words(&project_services::new_project_folder(parent, name));
            let colour = if warns { SIGNAL_WARN } else { tokens::TEXT_DIM };
            let name = name.clone();
            rows.push(Box::new(bsn_list![
                label(NEW_TITLE),
                label_dim(NEW_HINT),
                group_title("Made inside"),
                path_line(folder),
                group_title(PROJECT_NAME),
                (
                    @FeathersTextInputContainer
                    Node { border: UiRect::all(px(1)), flex_shrink: 0.0 }
                    ThemeBorderColor(BORDER)
                    Children [(
                        @FeathersTextInput { @max_characters: NAME_LENGTH }
                        LauncherNameInput({name})
                    )]
                ),
                (
                    label(will_be)
                    ThemeTextColor({colour})
                    LauncherNewFolder
                ),
                (
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::End,
                        column_gap: px(spacing::SPACE_4),
                        flex_shrink: 0.0,
                    }
                    Children [
                        back_button(),
                        (
                            @FeathersButton {
                                @caption: bsn! { Text(CREATE) ThemedText },
                                @variant: ButtonVariant::Primary,
                            }
                            LauncherCreate
                            Node { flex_shrink: 0.0 }
                            on(create)
                        ),
                    ]
                ),
            ]));
        }
        Pending::SetUp {
            folder: root,
            name,
            plan,
        } => {
            let relative = |path: &std::path::Path| {
                path.strip_prefix(root)
                    .unwrap_or(path)
                    .display()
                    .to_string()
            };
            let level = match &plan.existing {
                Some(level) => format!("Starts with {}, which is in the folder.", relative(level)),
                None => format!(
                    "Starts with {}, a starter level that is made.",
                    relative(&plan.starter)
                ),
            };
            let profile = match &plan.profile {
                Some(profile) => format!(
                    "{}, the look profile the game is drawn with, is made.",
                    relative(profile)
                ),
                None => "The look profile that is in the folder is kept.".to_owned(),
            };
            rows.push(Box::new(bsn_list![
                label(SET_UP_TITLE),
                path_line(folder),
                label_dim(SETUP_HINT),
                label_dim(level),
                label_dim(profile),
            ]));
            if !plan.rejected.is_empty() {
                let said = rejected_words(plan.existing.is_some(), plan.rejected.len());
                let files = plan
                    .rejected
                    .iter()
                    .map(|level| {
                        format!(
                            "{}: {}",
                            relative(&level.path),
                            wording::level_rejection(&level.code, &level.detail)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                rows.push(warning(said.0.into(), said.1.into(), Some(files)));
            }
            let fresh_starter = plan.needs_agreement();
            let caption = set_up_caption(fresh_starter);
            let name = name.clone();
            rows.push(Box::new(bsn_list![
                group_title(PROJECT_NAME),
                (
                    @FeathersTextInputContainer
                    Node { border: UiRect::all(px(1)), flex_shrink: 0.0 }
                    ThemeBorderColor(BORDER)
                    Children [(
                        @FeathersTextInput { @max_characters: NAME_LENGTH }
                        LauncherNameInput({name})
                    )]
                ),
                (
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::End,
                        column_gap: px(spacing::SPACE_4),
                        flex_shrink: 0.0,
                    }
                    Children [
                        back_button(),
                        (
                            @FeathersButton {
                                @caption: bsn! { Text(caption) ThemedText },
                                @variant: ButtonVariant::Primary,
                            }
                            LauncherSetUp { fresh_starter: {fresh_starter} }
                            Node { flex_shrink: 0.0 }
                            on(set_up)
                        ),
                    ]
                ),
            ]));
        }
    }
    rows
}

/// What the browser shows between its title and its foot.
fn content(launcher: &Launcher, now_unix_ms: u64) -> Vec<Box<dyn SceneList>> {
    let mut rows: Vec<Box<dyn SceneList>> = Vec::new();
    if let Some(refusal) = launcher.refusal() {
        let (title, remedy, folder) = refusal_words(refusal);
        rows.push(warning(title, remedy, Some(folder)));
    }
    if let Some(pending) = launcher.pending() {
        rows.extend(pending_rows(pending));
        return rows;
    }
    if let Some(problem) = launcher.list_problem() {
        let (title, remedy) = list_problem_words(&problem.code);
        rows.push(warning(title, remedy, None));
    }
    if launcher.rows().is_empty() {
        let words = if launcher.has_list() {
            NO_PROJECTS.1
        } else {
            NO_LIST
        };
        rows.push(Box::new(bsn_list![label(NO_PROJECTS.0), label_dim(words)]));
    } else {
        rows.push(Box::new(bsn_list![group_title("Recent projects")]));
    }
    for (index, row) in launcher.rows().iter().enumerate() {
        rows.push(project_row(
            index,
            row,
            launcher.selected() == Some(index),
            now_unix_ms,
        ));
    }
    if let Some(location) = launcher.game() {
        rows.extend(game_rows(location, launcher.game_refusal()));
    }
    rows
}

/// Where the game is that plays every project, or that none is set, with the button that
/// chooses its folder; and why the folder that was last chosen for it was not taken.
fn game_rows(
    location: &RuntimeLocation,
    refusal: Option<&(PathBuf, Diagnostic)>,
) -> Vec<Box<dyn SceneList>> {
    let (words, folder, warns, button) = game_folder_words(location);
    let colour = if warns { SIGNAL_WARN } else { tokens::TEXT_DIM };
    let folder: Box<dyn SceneList> = match folder {
        Some(folder) => Box::new(bsn_list![path_line(folder)]),
        None => Box::new(bsn_list![]),
    };
    let button: Box<dyn SceneList> = match button {
        Some(caption) => Box::new(bsn_list![(
            @FeathersButton { @caption: bsn! { Text(caption) ThemedText } }
            LauncherChooseGame
            Node { align_self: AlignSelf::FlexStart, flex_shrink: 0.0 }
            on(|_: On<Activate>, mut launcher: ResMut<Launcher>| {
                launcher.ask_folder(FolderFor::Game);
            })
        )]),
        None => Box::new(bsn_list![]),
    };
    let mut rows: Vec<Box<dyn SceneList>> = vec![Box::new(bsn_list![
        group_title(GAME_TITLE),
        label_dim(GAME_HINT),
        {folder},
        (label(words) ThemeTextColor({colour})),
        {button},
    ])];
    if let Some((folder, refusal)) = refusal {
        let (title, remedy, folder) = game_refusal_words(folder, refusal);
        rows.push(warning(title, remedy, Some(folder)));
    }
    rows
}

/// The time now, in milliseconds since 1970: what a project is remembered with.
pub fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

/// What the content was last built from.
type Built = (
    Vec<Row>,
    Option<usize>,
    Option<Pending>,
    Option<Refusal>,
    Option<String>,
    Option<RuntimeLocation>,
    Option<(PathBuf, Diagnostic)>,
);

/// Shows the browser while it is the window's screen, and rebuilds what it shows when the
/// list, the chosen row, the folder that is pending or the refusal changed. The foot's two
/// buttons are not offered while a folder waits for the person's word: that panel has its
/// own.
pub(crate) fn show_launcher(
    mut commands: Commands,
    launcher: Res<Launcher>,
    mut roots: Query<&mut Node, (With<LauncherRoot>, Without<LauncherFoot>)>,
    mut feet: Query<&mut Node, (With<LauncherFoot>, Without<LauncherRoot>)>,
    body: Query<Entity, With<LauncherBody>>,
    mut built: Local<Option<Built>>,
) {
    let shown = |on: bool| if on { Display::Flex } else { Display::None };
    for mut node in &mut roots {
        if node.display != shown(launcher.shown()) {
            node.display = shown(launcher.shown());
        }
    }
    for mut node in &mut feet {
        let offered = shown(launcher.pending().is_none());
        if node.display != offered {
            node.display = offered;
        }
    }
    let Ok(body) = body.single() else {
        return;
    };
    if !launcher.shown() {
        // Put away, it holds nothing: a browser that is shown again is built anew.
        if built.take().is_some() {
            commands.entity(body).despawn_children();
        }
        return;
    }
    let now: Built = (
        launcher.rows().to_vec(),
        launcher.selected(),
        launcher.pending().cloned(),
        launcher.refusal().cloned(),
        launcher.list_problem().map(|problem| problem.code.clone()),
        launcher.game().cloned(),
        launcher.game_refusal().cloned(),
    );
    if built.as_ref() == Some(&now) {
        return;
    }
    *built = Some(now);
    commands
        .entity(body)
        .despawn_children()
        .queue_spawn_related_scenes::<Children>(content(&launcher, now_unix_ms()));
}

/// Writes the offered name into the field of a new project's name when the field is new.
pub(crate) fn fill_launcher_name(
    mut names: Query<(&LauncherNameInput, &mut EditableText), Added<LauncherNameInput>>,
) {
    for (name, mut text) in &mut names {
        text.queue_edit(TextEdit::SelectAll);
        text.queue_edit(TextEdit::Insert(name.0.as_str().into()));
    }
}

/// Says where the new project will be made whenever the name in its field changes: the
/// folder named after it inside the chosen one, or why it cannot be made there. The folder
/// is looked for only when the name is another one.
pub(crate) fn show_new_folder(
    mut commands: Commands,
    launcher: Res<Launcher>,
    names: Query<&EditableText, With<LauncherNameInput>>,
    mut lines: Query<(Entity, &mut Text), With<LauncherNewFolder>>,
    mut said_for: Local<Option<String>>,
) {
    let (Some(Pending::New { .. }), Ok((line, mut text))) =
        (launcher.pending(), lines.single_mut())
    else {
        *said_for = None;
        return;
    };
    let Some(name) = names.iter().next().map(|field| field.value().to_string()) else {
        return;
    };
    // The field of a panel that was just built is empty until its name is written into
    // it: the line says the offered name's folder until then.
    if said_for.as_deref() == Some(name.as_str()) || (said_for.is_none() && name.is_empty()) {
        return;
    }
    let Some(found) = launcher.new_folder(&name) else {
        return;
    };
    let (words, warns) = new_folder_words(&found);
    let colour = if warns { SIGNAL_WARN } else { tokens::TEXT_DIM };
    if text.0 != words {
        text.0 = words;
    }
    commands.entity(line).insert(ThemeTextColor(colour));
    *said_for = Some(name);
}

/// What an open project is played with is looked for again when the game's folder was
/// chosen anew: the project's level and session stay as they are. A game that is running
/// keeps its own until it has ended, and the editor is asked again then.
pub(crate) fn follow_game_folder(
    launcher: Res<Launcher>,
    project: Res<Project>,
    mut editor: NonSendMut<Editor>,
    mut followed: Local<u64>,
) {
    if *followed == launcher.game_changes() {
        return;
    }
    // Only an editor that was entered on a project has a game of this setting.
    let (Some(root), false, true) = (project.root(), launcher.shown(), editor.runtime().is_some())
    else {
        *followed = launcher.game_changes();
        return;
    };
    let runtime = project_services::resolve_project_runtime_using(root, launcher.game_file());
    if editor.use_runtime(root, runtime) {
        *followed = launcher.game_changes();
    }
}

/// The keys of the browser while it shows the list: the arrows choose a row and Enter opens
/// the chosen one. While a folder waits for the person's word the keys are the name
/// field's, and Escape leaves it.
pub(crate) fn launcher_keys(
    mut pressed: MessageReader<KeyboardInput>,
    mut launcher: ResMut<Launcher>,
) {
    let keys: Vec<KeyCode> = pressed
        .read()
        .filter(|input| input.state == ButtonState::Pressed)
        .map(|input| input.key_code)
        .collect();
    if !launcher.shown() || launcher.wants_folder().is_some() {
        return;
    }
    for key in keys {
        match (key, launcher.pending().is_some()) {
            (KeyCode::Escape, true) => launcher.back(),
            (KeyCode::ArrowDown, false) => launcher.step_selection(true),
            (KeyCode::ArrowUp, false) => launcher.step_selection(false),
            (KeyCode::Enter | KeyCode::NumpadEnter, false) => {
                launcher.open_selected();
            }
            _ => {}
        }
    }
}

/// The open project is left for the project browser: what was unsaved has been asked about,
/// and a game the project played has stopped (`unsaved.rs`, `closing.rs`).
#[derive(Event, Clone, Copy, PartialEq, Eq, Debug)]
pub struct ToBrowser;

/// Shows the project browser in front of an editor that holds no project: the project, its
/// emitters and its level are let go, as they are behind the browser at the start, and the
/// dialogs of the project are closed. The browser reads its list and the game's folder anew;
/// the project that was left is the first row and is chosen, so Enter goes back to it. A
/// game that runs keeps the editor where it is, which only a race can bring about: the
/// game was stopped first.
pub(crate) fn return_to_browser(
    _: On<ToBrowser>,
    mut launcher: ResMut<Launcher>,
    (mut project, mut particles, mut dialog): (
        ResMut<Project>,
        ResMut<Particles>,
        ResMut<ProjectDialog>,
    ),
    (mut levels, mut tool): (ResMut<LevelDialog>, ResMut<Tool>),
    mut editor: NonSendMut<Editor>,
) {
    if !editor.leave_project() {
        return;
    }
    *project = Project::of_folder(None);
    *particles = Particles::default();
    dialog.open = false;
    *levels = LevelDialog::Closed;
    // The Particles tool shows a project's emitters; the next project starts on objects.
    if *tool == Tool::Particles {
        *tool = Tool::default();
    }
    launcher.back_to_browser();
}

/// Enters the editor on the project that was chosen in the browser: the project of that
/// folder is the editor's, its emitters are the Particles tool's, and the level it starts
/// with is opened in a session of its own, whose cooked levels go into the project's
/// folder. The emitters are the project's own, never the game's: a project is content, and
/// the game that plays it is the one the editor's setting names, or the project's folder
/// when that is a game's. Without one the editor has no game to start and says so. A
/// project that still needs the person's word in the Project settings has them opened: one
/// whose look profile is not in its folder too, which is made there on that word.
pub(crate) fn enter_project(
    mut launcher: ResMut<Launcher>,
    mut project: ResMut<Project>,
    mut particles: ResMut<Particles>,
    mut dialog: ResMut<ProjectDialog>,
    mut editor: NonSendMut<Editor>,
) {
    if launcher.opening().is_none() {
        return;
    }
    let Some(context) = launcher.take_opening() else {
        return;
    };
    *project = Project::of_folder(Some(context.project_root.clone()));
    *particles = Particles::of_game(context.project_root.clone());
    editor.open_project(&context);
    dialog.open = project.wants_attention() || project.look_missing();
    let name = project.config().map_or_else(
        || project.default_name(),
        |config| config.project.name.clone(),
    );
    launcher.entered(&context.project_root, &name, now_unix_ms());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_says_what_its_folder_is_now_unless_it_is_a_ready_project() {
        let ready = ProjectInspectionStatus::Ready {
            name: "Game".into(),
            start_level: PathBuf::from("levels/first.level.json"),
        };
        assert_eq!(row_words(&ready), None);
        let (gone, warned) = row_words(&ProjectInspectionStatus::Missing).unwrap();
        assert!(warned && gone.contains("not there"));
        // A project without its start level is not shown as a ready one: the row warns,
        // names the level and says what opening does.
        let no_level = ProjectInspectionStatus::MissingStartLevel {
            name: "Game".into(),
            start_level: PathBuf::from("/projects/game/levels/first.level.json"),
        };
        let (words, warned) = row_words(&no_level).unwrap();
        assert!(warned, "{words}");
        assert!(words.contains("first.level.json") && words.contains("not in the folder"));
        assert!(words.contains("unsaved map") && words.contains("Project settings"));
        let refused = ProjectInspectionStatus::Refused {
            problem: Diagnostic::new("project.schema_unsupported"),
        };
        let (words, warned) = row_words(&refused).unwrap();
        assert!(warned && words.contains("does not read"), "{words}");
    }

    #[test]
    fn a_folder_that_was_not_entered_says_why_and_what_to_do_with_its_path() {
        let folder = PathBuf::from("/projects/harbour");
        let (title, remedy, path) = refusal_words(&Refusal::Missing(folder.clone()));
        assert_eq!(title, "The folder is not there");
        assert!(remedy.contains("Nothing was changed"));
        assert_eq!(path, "/projects/harbour");
        let (title, remedy, _) = refusal_words(&Refusal::AlreadyProject(folder.clone()));
        assert_eq!(title, "This folder is a project already");
        assert!(remedy.contains(OPEN_PROJECT) && remedy.contains("not made over it"));
        let refused = Refusal::Refused(folder, Diagnostic::new("project.schema_unsupported"));
        let (title, _, _) = refusal_words(&refused);
        assert!(title.contains("does not read"), "{title}");
    }

    #[test]
    fn the_empty_browser_says_what_each_of_its_two_buttons_is_for() {
        assert_eq!(NO_PROJECTS.0, "No projects yet");
        assert!(NO_PROJECTS.1.contains(NEW_PROJECT) && NO_PROJECTS.1.contains(OPEN_PROJECT));
        assert!(NO_PROJECTS.1.contains("a folder of its own"));
        assert!(NO_PROJECTS
            .1
            .contains("a project file, a first level and a look profile"));
        assert!(NO_LIST.contains("can still be opened and made"));
        let (title, remedy) = list_problem_words("project.recents_unreadable");
        assert!(title.contains("cannot be used"));
        assert!(remedy.contains("left as it is") && remedy.contains("project.recents_unreadable"));
        assert_eq!(NEW_TITLE, "New project");
        assert_eq!(SET_UP_TITLE, "This folder is not a project yet");
    }

    #[test]
    fn a_new_project_says_what_is_made_and_where_before_it_is_made() {
        assert!(NEW_HINT.contains("a folder of its own, named after it"));
        assert!(NEW_HINT.contains("nothing in it is changed"));
        assert!(NEW_HINT.contains("project.aproj"));
        assert!(NEW_HINT.contains("a first level") && NEW_HINT.contains("look profile"));
        assert!(NEW_HINT.contains("The game itself is not part of a project"));
        let (words, warns) = new_folder_words(&Ok(PathBuf::from("/docs/Harbour Duel")));
        assert_eq!(words, "Will be made as /docs/Harbour Duel");
        assert!(!warns);
        // A name that is taken names the folder and says what to do, not that anything
        // was refused: nothing was asked for yet.
        let taken = Diagnostic::new("project.folder_taken").with("path", "/docs/Harbour");
        let (words, warns) = new_folder_words(&Err(taken));
        assert!(warns && words.contains("is there already") && words.contains("(/docs/Harbour)"));
        assert!(words.contains("another name") && !words.contains("Nothing was made"));
        for code in [
            "project.value",
            "project.folder_name",
            "project.parent_missing",
        ] {
            let (words, warns) = new_folder_words(&Err(Diagnostic::new(code)));
            assert!(warns && !words.contains("project."), "{code}: {words}");
        }
    }

    #[test]
    fn the_browser_says_where_the_game_is_or_that_none_is_set_and_never_a_code() {
        use editor_services::{
            project::{GameRuntimeRequirement, GameRuntimeRequirementKind},
            runtime::{RuntimeFolderInspection, RuntimeInstallation},
        };
        let of = |status| RuntimeLocation {
            preference_path: Some(PathBuf::from("/settings/runtime-install.json")),
            status,
        };
        let folder = |status| RuntimeLocationStatus::Configured {
            root: PathBuf::from("/games/arc"),
            folder: RuntimeFolderInspection {
                selected_root: PathBuf::from("/games/arc"),
                canonical_root: Some(PathBuf::from("/games/arc")),
                status,
            },
        };
        assert!(GAME_HINT.contains("content") && GAME_HINT.contains("every project"));

        let (words, path, warns, button) =
            game_folder_words(&of(RuntimeLocationStatus::Unconfigured));
        assert!(words.contains("No game folder is set") && words.contains("without one"));
        assert_eq!((path, warns, button), (None, false, Some(CHOOSE_GAME)));

        let ready = folder(RuntimeFolderStatus::Ready {
            installation: RuntimeInstallation {
                root: PathBuf::from("/games/arc"),
                server: PathBuf::from("/games/arc/target/debug/server"),
                client: PathBuf::from("/games/arc/target/debug/client"),
                asset_root: PathBuf::from("/games/arc"),
            },
        });
        let (words, path, warns, button) = game_folder_words(&of(ready));
        assert!(words.contains("Every project is played"));
        assert_eq!(path.as_deref(), Some("/games/arc"));
        assert_eq!((warns, button), (false, Some(CHANGE_GAME)));

        let incomplete = folder(RuntimeFolderStatus::Incomplete {
            requirements: vec![GameRuntimeRequirement {
                kind: GameRuntimeRequirementKind::ClientExecutable,
                path: PathBuf::from("/games/arc/target/debug/client"),
            }],
        });
        let (words, _, warns, _) = game_folder_words(&of(incomplete));
        assert!(warns && words.contains("its client (/games/arc/target/debug/client)"));
        let (words, path, warns, _) = game_folder_words(&of(folder(RuntimeFolderStatus::Missing)));
        assert!(warns && words.contains("not there") && path.is_some());

        let unreadable = RuntimeLocationStatus::Refused {
            problem: Diagnostic::new("runtime.preference_decode"),
        };
        let (words, _, warns, button) = game_folder_words(&of(unreadable));
        assert!(warns && words.contains("cannot be read") && !words.contains("runtime."));
        assert_eq!(button, Some(CHOOSE_GAME));
        // Without a place to keep it there is nothing to press.
        let (words, _, warns, button) =
            game_folder_words(&of(RuntimeLocationStatus::PreferenceUnavailable));
        assert!(warns && words.contains("no place to keep") && button.is_none());

        let refusal = Diagnostic::new("runtime.requirements_missing");
        let (title, remedy, path) = game_refusal_words(Path::new("/docs"), &refusal);
        assert_eq!(title, "That folder does not hold the whole game");
        assert!(remedy.contains("target/debug/server") && remedy.contains("as it was"));
        assert_eq!(path, "/docs");
    }
}
