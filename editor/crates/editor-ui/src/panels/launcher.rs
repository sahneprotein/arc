//! The project browser's screen: the whole window, before the editor is entered.
//!
//! It is the editor's own window and nothing beside it: the browser covers the window while
//! it is shown and is put away when a project is entered, and no second window or program is
//! started for either. What it shows is the list of projects that were opened before, each
//! with what its folder is now, and two buttons that lead to another project through the
//! system's folder chooser: "Open Project" and "New Project". Without a list the same two
//! buttons are there, under words that say what each is for.
//!
//! A folder that needs the person's word first, to be set up or to have its project file
//! converted, is shown in the list's place with what will be done and the button that does
//! it. The state is [`Launcher`]; this module shows it and hands it what the person does.

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
use editor_services::project::ProjectInspectionStatus;

use crate::{
    launcher::{opened_words, FolderFor, Launcher, Pending, Refusal, Row},
    panels::{
        inspector::group_title,
        project::{rejected_words, set_up_caption, warning, CONVERT_HINT, SETUP_HINT},
    },
    particles::Particles,
    project::Project,
    session::Editor,
    theme::{
        feathers::{BORDER, SELECTION, SIGNAL_WARN},
        radius, spacing, text,
    },
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
    "New Project makes a project in a folder you choose: a project file and a first level. \
     Open Project opens the folder of a project you already have.",
);

/// What it says where the system has no place to keep the list in.
pub const NO_LIST: &str = "This system has no place to keep the list of projects in, so none \
    is shown. Projects can still be opened and made.";

/// What the buttons of a row say.
pub const OPEN_ROW: &str = "Open";
pub const FORGET_ROW: &str = "Remove from list";

/// What a new project's field is called, and what leaves a setup or a conversion.
pub const PROJECT_NAME: &str = "Project name";
pub const BACK: &str = "Back to projects";
pub const CONVERT: &str = "Convert and open";

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
            "Not a project any more: opening it offers to set the folder up.".into(),
            false,
        )),
        ProjectInspectionStatus::ConversionRequired { .. } => Some((
            "Its project file is of an older version: opening it offers to convert it.".into(),
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

/// What a setup's panel is called: a new project that was asked for, or a folder that was
/// opened and is no project yet.
pub fn set_up_title(asked_new: bool) -> &'static str {
    if asked_new {
        "New project"
    } else {
        "This folder is not a project yet"
    }
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
        Pending::SetUp {
            folder: root,
            name,
            plan,
            asked_new,
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
            rows.push(Box::new(bsn_list![
                label(set_up_title(*asked_new)),
                path_line(folder),
                label_dim(SETUP_HINT),
                label_dim(level),
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
        Pending::Convert { detail, .. } => {
            let detail = if detail.is_empty() {
                String::new()
            } else {
                format!("What the reader said: {detail}.")
            };
            rows.push(Box::new(bsn_list![
                label("This project file is of an older version"),
                path_line(folder),
                label_dim(CONVERT_HINT),
                label_dim(detail),
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
                                @caption: bsn! { Text(CONVERT) ThemedText },
                                @variant: ButtonVariant::Primary,
                            }
                            Node { flex_shrink: 0.0 }
                            on(|_: On<Activate>, mut launcher: ResMut<Launcher>| {
                                launcher.convert();
                            })
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
        return rows;
    }
    rows.push(Box::new(bsn_list![group_title("Recent projects")]));
    for (index, row) in launcher.rows().iter().enumerate() {
        rows.push(project_row(
            index,
            row,
            launcher.selected() == Some(index),
            now_unix_ms,
        ));
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

/// Enters the editor on the project that was chosen in the browser: the project of that
/// folder is the editor's, its emitters are the Particles tool's, and the level it starts
/// with is opened in a session of its own, whose cooked levels go into the project's
/// folder. The game is the one in that folder; where it is not built there, the editor has
/// no game to start and says what is missing. A project that still needs the person's word
/// in the Project settings has them opened.
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
    *particles = Particles::of_game(context.game_root.clone());
    editor.open_project(&context);
    dialog.open = project.wants_attention();
    let name = project.config().map_or_else(
        || project.default_name(),
        |config| config.project.name.clone(),
    );
    launcher.entered(&context.project_root, &name, now_unix_ms());
}

#[cfg(test)]
mod tests {
    use super::*;
    use scene_data::Diagnostic;
    use std::path::PathBuf;

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
        let older = ProjectInspectionStatus::ConversionRequired {
            problem: Diagnostic::new("project.convert_required"),
        };
        let (words, warned) = row_words(&older).unwrap();
        assert!(!warned && words.contains("older version") && words.contains("convert"));
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
        assert!(NO_PROJECTS.1.contains("a project file and a first level"));
        assert!(NO_LIST.contains("can still be opened and made"));
        let (title, remedy) = list_problem_words("project.recents_unreadable");
        assert!(title.contains("cannot be used"));
        assert!(remedy.contains("left as it is") && remedy.contains("project.recents_unreadable"));
        assert_eq!(set_up_title(true), "New project");
        assert_eq!(set_up_title(false), "This folder is not a project yet");
    }
}
