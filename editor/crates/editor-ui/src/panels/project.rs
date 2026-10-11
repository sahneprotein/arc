//! Project settings: a dialog that is always to be had from the Level menu, and that opens
//! by itself when the folder the editor was started with needs the person's word.
//!
//! What it shows follows what the editor found of the project (`crate::project`):
//!
//! - No project folder: it says how the editor is given one.
//! - A folder without a project file: the first-run setup. A name, and one button. The
//!   setup writes the project file and takes the first level of the folder that can be
//!   read, or makes a starter level; no file that is there is changed. The level the
//!   project starts with is opened afterwards.
//! - A project file of an older version: what converting does, and one button. The editor
//!   never converts by itself; the file as it was is kept beside the new one.
//! - A project file that cannot be used: why, and where it is. One of a newer version is
//!   such a file: there is nothing to convert it with, and no button is offered.
//!
//! The first-run setup says what it would do before it does it: the level the project would
//! start with, and each level file of the folder that cannot be read, with its reason. If
//! none can be read, the button says that it makes a new starter level, and pressing it is
//! the person's agreement to that; the files that could not be read are left as they are.
//! - A project: its name and folder, the level it starts with, the game's look profile and
//!   whether the editor has it, and the keys the game uses now. These are shown and not
//!   changed here. Under them are the settings the game does not read yet: each is a
//!   control that is switched off, and the group says that it is planned.

use bevy::{
    feathers::{
        containers::pane_body,
        controls::{ButtonVariant, FeathersButton, FeathersTextInput, FeathersTextInputContainer},
        display::{label, label_dim},
        theme::{ThemeBackgroundColor, ThemeBorderColor, ThemedText},
    },
    prelude::*,
    text::{EditableText, TextEdit},
    ui::InteractionDisabled,
    ui_widgets::{Activate, ScrollArea},
};

use crate::{
    panels::{inspector::group_title, unsaved::Leave},
    project::{control_rows, planned_rows, Project, ProjectState},
    session::Editor,
    theme::{
        feathers::{BORDER, SCRIM, SIGNAL_WARN},
        radius, size, spacing,
    },
    tool::LookStatus,
    wording,
};

/// Whether the dialog is open.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct ProjectDialog {
    pub open: bool,
}

/// The dialog and the layer that covers the window behind it.
#[derive(Component, Default, Clone)]
pub struct ProjectDialogRoot;

/// The node the dialog's content is rebuilt in.
#[derive(Component, Default, Clone)]
pub(crate) struct ProjectBody;

/// The text field of the name a new project gets.
#[derive(Component, Default, Clone)]
pub struct ProjectNameInput(pub String);

/// The longest name the field takes: what the project format lets a name be.
const NAME_LENGTH: usize = 96;

/// The width a setting's name takes, so that the values line up.
const LABEL_WIDTH: f32 = 168.0;

/// What the dialog says without a project folder.
pub const NO_FOLDER: &str = "The editor was started without a project folder. Start it with \
    --game and the game's folder to see its project, or to set one up there.";

/// What the first-run setup says it will do, before the person agrees.
pub const SETUP_HINT: &str = "Setting up writes arc-project.json into this folder. The \
    project starts with the first level in the folder's levels that can be read; if there is \
    none, a starter level is made there. No file that is there is changed.";

/// What the dialog says of the folder's level files that cannot be read, as a title and what
/// follows from it: with another level to start with they are only left alone; without one
/// a setup makes a new starter level, which is the person's to agree to.
pub fn rejected_words(another_level: bool, count: usize) -> (&'static str, &'static str) {
    match (count == 1, another_level) {
        (true, true) => (
            "A level file in this folder cannot be read",
            "It is left as it is. The project starts with the level that can be read.",
        ),
        (false, true) => (
            "Level files in this folder cannot be read",
            "They are left as they are. The project starts with the level that can be read.",
        ),
        (true, false) => (
            "A level file in this folder cannot be read",
            "It is left as it is. Setting up makes a new starter level beside it and starts \
             the project with that. If it is the level you mean, put it right first.",
        ),
        (false, false) => (
            "Level files in this folder cannot be read",
            "They are left as they are. Setting up makes a new starter level beside them \
             and starts the project with that. If one of them is the level you mean, put \
             it right first.",
        ),
    }
}

/// What the dialog says of a project file of another version, before the person agrees.
pub const CONVERT_HINT: &str = "It is of an older version, which this editor can convert. \
    The editor does not change the file by itself. Converting first keeps the file as it is \
    beside it, under its name with .schema-N.bak after it, and then writes the version this \
    editor reads. Its settings are kept.";

/// What the group of the current keys says.
pub const KEYS_HINT: &str = "The keys the game uses now. They are shown here and not changed \
    here.";

/// What a group of planned settings says.
pub const PLANNED_HINT: &str = "Planned. The game does not read these yet, so they cannot be \
    set.";

/// What the dialog says of the game's look profile: whether the editor holds it.
pub fn look_words(look: &LookStatus) -> String {
    match look {
        LookStatus::NoGame => "The editor has not read it.".into(),
        LookStatus::Loaded { identity } => match identity {
            Some(identity) => format!("Read: look profile {}.", wording::short_hash(identity)),
            None => "Read.".into(),
        },
        LookStatus::Refused { code, .. } => {
            let problem = wording::problem(code, |_| None);
            format!("Not used. {}. {}", problem.title, problem.remedy)
        }
    }
}

/// What the dialog says after a setup in this session.
pub fn set_up_words(start_level: &str, made_level: bool) -> String {
    if made_level {
        format!(
            "The project is set up. It starts with {start_level}, a starter level that was made."
        )
    } else {
        format!("The project is set up. It starts with {start_level}, which was in the folder.")
    }
}

pub fn project_dialog() -> impl Scene {
    bsn! {
        ProjectDialogRoot
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
        // In front of the window, and it takes every click: nothing behind can be used.
        GlobalZIndex(100)
        ThemeBackgroundColor(SCRIM)
        Children [(
            pane_body()
            Node {
                width: px(520),
                max_width: percent(92),
                max_height: percent(92),
                row_gap: px(spacing::SPACE_4),
                padding: UiRect::all(px(spacing::SPACE_6)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(radius::RADIUS_FIELD)),
            }
            ThemeBorderColor(BORDER)
            Children [
                label("Project settings"),
                (
                    ProjectBody
                    ScrollArea
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(spacing::SPACE_2),
                        min_height: px(0),
                        overflow: Overflow::scroll_y(),
                    }
                ),
                (
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::End,
                        flex_shrink: 0.0,
                    }
                    Children [(
                        @FeathersButton { @caption: bsn! { Text("Close settings") ThemedText } }
                        on(|_: On<Activate>, mut dialog: ResMut<ProjectDialog>| {
                            dialog.open = false;
                        })
                    )]
                ),
            ]
        )]
    }
}

fn row(children: impl SceneList) -> impl Scene {
    bsn! {
        Node {
            min_height: px(size::ROW_HEIGHT),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(spacing::SPACE_4),
            flex_shrink: 0.0,
        }
        Children [ {children} ]
    }
}

fn name_of(text: &str) -> impl Scene {
    bsn! {
        Node { width: px(LABEL_WIDTH), flex_shrink: 0.0 }
        Children [ label(text.to_owned()) ]
    }
}

/// A setting that is shown: its name and what it is.
fn shown(name: &str, value: String) -> Box<dyn SceneList> {
    Box::new(bsn_list![row(bsn_list![name_of(name), label_dim(value)])])
}

/// A setting the game does not read yet: its name and a control that is switched off.
fn planned(name: &str, value: String) -> Box<dyn SceneList> {
    Box::new(bsn_list![row(bsn_list![
        name_of(name),
        (
            @FeathersButton { @caption: bsn! { Text({value}) ThemedText } }
            PlannedSetting
            InteractionDisabled
            Node { flex_grow: 1.0, justify_content: JustifyContent::Start }
        )
    ])])
}

/// The control of a setting the game does not read yet. It is always switched off.
#[derive(Component, Default, Clone)]
pub struct PlannedSetting;

/// A refusal as the dialog shows it, edged in the warning colour, which always comes with
/// these words.
fn warning(title: String, remedy: String, more: Option<String>) -> Box<dyn SceneList> {
    let more: Box<dyn SceneList> = match more {
        Some(more) => Box::new(bsn_list![label_dim(more)]),
        None => Box::new(bsn_list![]),
    };
    Box::new(bsn_list![(
        ProjectWarning
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(spacing::SPACE_1),
            padding: UiRect::left(px(spacing::SPACE_4)),
            border: UiRect::left(px(2)),
            flex_shrink: 0.0,
        }
        ThemeBorderColor(SIGNAL_WARN)
        Children [ label(title), label_dim(remedy), {more} ]
    )])
}

/// A block that says something is wrong with the project.
#[derive(Component, Default, Clone)]
pub struct ProjectWarning;

/// The button that sets the folder up. `fresh_starter` says that it is the button of a
/// folder whose level files cannot be read: pressing it is the person's agreement to a new
/// starter level beside them.
#[derive(Component, Default, Clone)]
pub struct SetUpButton {
    pub fresh_starter: bool,
}

/// What the button that sets the folder up says: with level files that cannot be read, it
/// says what pressing it makes.
pub fn set_up_caption(fresh_starter: bool) -> &'static str {
    if fresh_starter {
        "Set up with a new starter level"
    } else {
        "Set up project"
    }
}

fn set_up(
    activate: On<Activate>,
    mut commands: Commands,
    (names, buttons): (
        Query<&EditableText, With<ProjectNameInput>>,
        Query<&SetUpButton>,
    ),
    mut project: ResMut<Project>,
    mut editor: NonSendMut<Editor>,
) {
    let name = names
        .iter()
        .next()
        .map(|text| text.value().to_string())
        .unwrap_or_else(|| project.default_name());
    let fresh_starter = buttons
        .get(activate.entity)
        .is_ok_and(|button| button.fresh_starter);
    let Some(level) = project.set_up(&name, fresh_starter) else {
        return;
    };
    // The level the project starts with is what the editor shows next. A level that was
    // not touched, the one the editor began with, is left without a question; anything
    // else is asked about first, as every leaving of a level is.
    let untouched = !editor.has_file() && editor.snapshot().undo_label.is_none();
    if untouched {
        editor.open(level);
    } else {
        commands.trigger(Leave::Open(level));
    }
}

fn convert(_: On<Activate>, mut project: ResMut<Project>) {
    project.convert();
}

fn open_start_level(
    _: On<Activate>,
    mut commands: Commands,
    project: Res<Project>,
    mut dialog: ResMut<ProjectDialog>,
) {
    if let Some(level) = project.start_level() {
        dialog.open = false;
        commands.trigger(Leave::Open(level));
    }
}

/// The dialog's content for what was found of the project.
fn content(project: &Project, look: &LookStatus) -> Vec<Box<dyn SceneList>> {
    let mut rows: Vec<Box<dyn SceneList>> = Vec::new();
    if let Some(refusal) = project.problem() {
        let words = wording::project(&refusal.code, |key| {
            refusal.parameters.get(key).map(String::as_str)
        });
        rows.push(warning(words.title, words.remedy, None));
    }
    let folder = project
        .root()
        .map(|root| root.display().to_string())
        .unwrap_or_default();
    match project.state() {
        ProjectState::NoFolder => rows.push(Box::new(bsn_list![label_dim(NO_FOLDER)])),
        ProjectState::NotSetUp => {
            let name = project.default_name();
            rows.push(Box::new(bsn_list![
                label("This folder is not a project yet"),
                label_dim(SETUP_HINT),
            ]));
            rows.push(shown("Folder", folder));
            // What the setup would do in this folder, and what it cannot use.
            let plan = project.setup_plan();
            let fresh_starter = plan.is_some_and(|plan| plan.needs_agreement());
            if let Some(plan) = plan {
                let relative = |path: &std::path::Path| {
                    project
                        .root()
                        .and_then(|root| path.strip_prefix(root).ok())
                        .unwrap_or(path)
                        .display()
                        .to_string()
                };
                match &plan.existing {
                    Some(level) => rows.push(shown("Start level", relative(level))),
                    None => rows.push(shown(
                        "Start level",
                        format!("{}, a starter level that is made", relative(&plan.starter)),
                    )),
                }
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
            }
            let caption = set_up_caption(fresh_starter);
            rows.push(Box::new(bsn_list![
                row(bsn_list![
                    name_of("Project name"),
                    (
                        @FeathersTextInputContainer
                        Node { flex_grow: 1.0, min_width: px(0), border: UiRect::all(px(1)) }
                        ThemeBorderColor(BORDER)
                        Children [(
                            @FeathersTextInput { @max_characters: NAME_LENGTH }
                            ProjectNameInput({name})
                        )]
                    )
                ]),
                (
                    @FeathersButton {
                        @caption: bsn! { Text(caption) ThemedText },
                        @variant: ButtonVariant::Primary,
                    }
                    SetUpButton { fresh_starter: {fresh_starter} }
                    Node { align_self: AlignSelf::FlexStart, flex_shrink: 0.0 }
                    on(set_up)
                ),
            ]));
        }
        ProjectState::NeedsConversion { detail } => {
            let detail = if detail.is_empty() {
                String::new()
            } else {
                format!("What the reader said: {detail}.")
            };
            rows.push(Box::new(bsn_list![
                label("This project file is of an older version"),
                label_dim(CONVERT_HINT),
                label_dim(detail),
            ]));
            rows.push(shown("Folder", folder));
            rows.push(Box::new(bsn_list![(
                @FeathersButton {
                    @caption: bsn! { Text("Convert project file") ThemedText },
                    @variant: ButtonVariant::Primary,
                }
                Node { align_self: AlignSelf::FlexStart, flex_shrink: 0.0 }
                on(convert)
            )]));
        }
        ProjectState::Refused(refusal) => {
            let words = wording::project(&refusal.code, |key| {
                refusal.parameters.get(key).map(String::as_str)
            });
            let file = project
                .manifest_path()
                .map(|path| format!("File: {}", path.display()));
            rows.push(warning(words.title, words.remedy, file));
        }
        ProjectState::Ready {
            config,
            start_level_there,
        } => {
            if let Some(made) = project.last_set_up() {
                rows.push(Box::new(bsn_list![label(set_up_words(
                    &made.start_level,
                    made.made_level
                ))]));
            }
            rows.push(Box::new(bsn_list![group_title("Project")]));
            rows.push(shown("Name", config.project.name.clone()));
            rows.push(shown("Folder", folder));

            rows.push(Box::new(bsn_list![group_title("Start")]));
            rows.push(shown("Start level", config.startup.start_level.clone()));
            if *start_level_there {
                rows.push(Box::new(bsn_list![(
                    @FeathersButton { @caption: bsn! { Text("Open start level") ThemedText } }
                    Node { align_self: AlignSelf::FlexStart, flex_shrink: 0.0 }
                    on(open_start_level)
                )]));
            } else {
                rows.push(warning(
                    "The level the project starts with is not in the folder".into(),
                    "Save a level under that name, or put the file back.".into(),
                    None,
                ));
            }

            rows.push(Box::new(bsn_list![group_title("Rendering")]));
            rows.push(shown("Look profile", config.rendering.profile.clone()));
            rows.push(shown("In this editor", look_words(look)));

            rows.push(Box::new(bsn_list![
                group_title("Controls"),
                label_dim(KEYS_HINT)
            ]));
            for (what, key) in control_rows(config) {
                rows.push(shown(what, key));
            }

            for (group, settings) in planned_rows(config) {
                rows.push(Box::new(bsn_list![
                    group_title(group),
                    label_dim(PLANNED_HINT)
                ]));
                for (what, value) in settings {
                    rows.push(planned(what, value));
                }
            }
        }
    }
    rows
}

/// What the content was last built from.
type Built = (
    ProjectState,
    Option<String>,
    Option<(String, bool)>,
    LookStatus,
);

/// Shows the dialog while it is open, and rebuilds its content when what was found of the
/// project, or what the editor knows of the look profile, changed. Opens the dialog once by
/// itself when the folder needs the person's word: it has no project file yet, or one that
/// waits to be converted.
pub(crate) fn show_project(
    mut commands: Commands,
    (project, look): (Res<Project>, Res<LookStatus>),
    mut dialog: ResMut<ProjectDialog>,
    mut roots: Query<&mut Node, With<ProjectDialogRoot>>,
    body: Query<Entity, With<ProjectBody>>,
    mut state: Local<(bool, Option<Built>)>,
) {
    let (offered, built) = &mut *state;
    if !*offered {
        *offered = true;
        if project.wants_attention() {
            dialog.open = true;
        }
    }
    let wanted = if dialog.open {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut roots {
        if node.display != wanted {
            node.display = wanted;
        }
    }
    let Ok(body) = body.single() else {
        return;
    };
    let now: Built = (
        project.state().clone(),
        project.problem().map(|refusal| refusal.to_string()),
        project
            .last_set_up()
            .map(|made| (made.start_level.clone(), made.made_level)),
        look.clone(),
    );
    if built.as_ref() == Some(&now) {
        return;
    }
    *built = Some(now);
    commands
        .entity(body)
        .despawn_children()
        .queue_spawn_related_scenes::<Children>(content(&project, &look));
}

/// Writes the offered name into the setup's field when the field is new.
pub(crate) fn fill_project_name(
    mut names: Query<(&ProjectNameInput, &mut EditableText), Added<ProjectNameInput>>,
) {
    for (name, mut text) in &mut names {
        text.queue_edit(TextEdit::SelectAll);
        text.queue_edit(TextEdit::Insert(name.0.as_str().into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_look_profile_is_said_to_be_read_or_why_it_is_not_used() {
        assert_eq!(
            look_words(&LookStatus::NoGame),
            "The editor has not read it."
        );
        let read = look_words(&LookStatus::Loaded {
            identity: Some("3abf9a2fbcb48b74".into()),
        });
        assert_eq!(read, "Read: look profile 3abf9a2f.");
        let missing = look_words(&LookStatus::Refused {
            code: "look.missing".into(),
            path: "/game/assets/render/look-v1.json".into(),
            field: None,
        });
        assert!(missing.starts_with("Not used. "), "{missing}");
        assert!(!missing.contains("look.missing"), "{missing}");
    }

    #[test]
    fn what_setting_up_and_converting_do_is_said_before_the_person_agrees() {
        assert!(SETUP_HINT.contains("arc-project.json"));
        assert!(SETUP_HINT.contains("No file that is there is changed"));
        assert!(CONVERT_HINT.contains("does not change the file by itself"));
        assert!(CONVERT_HINT.contains("older version"));
        // The button of a folder whose levels cannot be read says what pressing it makes.
        assert_eq!(set_up_caption(false), "Set up project");
        assert_eq!(set_up_caption(true), "Set up with a new starter level");
        let (title, follows) = rejected_words(false, 1);
        assert_eq!(title, "A level file in this folder cannot be read");
        assert!(follows.contains("new starter level") && follows.contains("left as it is"));
        assert!(rejected_words(false, 3).1.contains("left as they are"));
        let (title, follows) = rejected_words(true, 2);
        assert_eq!(title, "Level files in this folder cannot be read");
        assert!(follows.contains("starts with the level that can be read"));
        assert!(CONVERT_HINT.contains(".schema-N.bak"));
        assert!(PLANNED_HINT.contains("does not read these yet"));
        assert!(NO_FOLDER.contains("--game"));
        assert!(set_up_words("levels/a.level.json", true).contains("starter level that was made"));
        assert!(set_up_words("levels/a.level.json", false).contains("was in the folder"));
    }
}
