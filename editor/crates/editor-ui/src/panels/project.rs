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
//! - A project: its name and folder and the level it starts with, which are shown; the
//!   game's look profile and the keys the game uses, which are changed here. Under them are
//!   the settings the game does not read yet: each is a control that is switched off, and
//!   the group says that it is planned.
//!
//! Changing a key or the look profile changes what the editor holds, not the file. The
//! dialog's foot says that there are unsaved changes and has the two buttons for them:
//! "Save project settings" writes them, and "Discard changes" puts back what the file has.
//! A key is changed by pressing its button and then the key; a key the game does not read is
//! not taken, and two actions on one key are marked and not saved. A look profile is chosen
//! among the folder's that the look reader takes. A save that is refused says why at the
//! top, and the changes stay.

use bevy::{
    feathers::{
        containers::pane_body,
        controls::{
            ButtonVariant, FeathersButton, FeathersCheckbox, FeathersTextInput,
            FeathersTextInputContainer,
        },
        display::{label, label_dim},
        theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeTextColor, ThemedText},
        tokens,
    },
    input::{keyboard::KeyboardInput, ButtonState},
    input_focus::InputFocus,
    prelude::*,
    text::{EditableText, TextEdit},
    ui::{Checked, InteractionDisabled},
    ui_widgets::{Activate, ScrollArea, ValueChange},
};

use crate::{
    panels::{inspector::group_title, unsaved::Leave},
    project::{
        planned_rows, DraftState, KeyAction, LookChoice, Project, ProjectState, SettingsEdit,
        MULTIPLAYER,
    },
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

/// The node the dialog's content is rebuilt in. It scrolls when the content is taller than
/// the dialog.
#[derive(Component, Default, Clone)]
pub struct ProjectBody;

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

/// What the dialog says of the level the project starts with when the game cannot be
/// started with it: it is not in the folder, or it is there and the check before a start
/// (`Project::play_blocker`) found it to be no level the game can open. `None` while
/// nothing is known to be wrong with it. The words of a level that is there are the Play
/// menu's, which sends the person here.
pub fn start_level_words(
    start_level_there: bool,
    blocker: Option<&scene_data::Diagnostic>,
) -> Option<(String, String)> {
    if !start_level_there {
        return Some((
            "The level the project starts with is not in the folder".into(),
            "Save a level under that name, or put the file back.".into(),
        ));
    }
    let blocker = blocker?;
    let words = wording::play_start_level(&blocker.code, |key| {
        blocker.parameters.get(key).map(String::as_str)
    })?;
    Some((words.title, words.remedy))
}

/// What the first-run setup says it will do, before the person agrees.
pub const SETUP_HINT: &str = "Setting up writes arc-project.json into this folder. The \
    project starts with the first level in the folder's levels that can be read; if there is \
    none, a starter level is made there. The look profile the game is drawn with is made \
    too if the folder has none. No file that is there is changed.";

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

/// What the group of the keys says.
pub const KEYS_HINT: &str = "The keys the game uses. To change one, press its button and then \
    the key. Letters, digits, Space, Escape, Enter, Tab, Backspace, Delete and the arrow keys \
    can be used.";

/// What a key's button says while its key is asked for.
pub const PRESS_A_KEY: &str = "Press a key";

/// What the keys say when the key that was pressed is none the game reads.
pub const KEY_REFUSED: &str = "The game does not read that key. Press another, or press the \
    button again to leave the key as it is.";

/// What a key's row says when another action is on the same key.
pub const KEY_SHARED: &str = "Also on another action";

/// What the look says when neither the mouse nor the stick is looked with.
pub const NO_LOOK: &str = "With neither, the player cannot look around.";

/// What the group of the look profile says.
pub const LOOK_HINT: &str = "The profile the game is drawn with. Game look in this editor is \
    drawn from the project's profile; one that is saved here is used at once.";

/// What the foot says of the settings: nothing while they are the file's, that they are
/// saved after a save, that there are changes, and why changes cannot be saved. The second
/// part says whether it is said in the warning colour.
pub fn settings_line(state: &DraftState, saved: bool) -> (String, bool) {
    match state {
        DraftState::Unchanged if saved => ("Saved".into(), false),
        DraftState::Unchanged => (String::new(), false),
        DraftState::Changed => ("Unsaved changes".into(), true),
        DraftState::SharedKey { first, second } => (
            format!(
                "{} and {} are on the same key. Give one another key to save.",
                first.words(),
                second.words()
            ),
            true,
        ),
        DraftState::Refused(reason) => (format!("Cannot be saved: {reason}."), true),
    }
}

/// What a look profile's row says of it: that the look reader took it, or why not.
pub fn look_choice_words(choice: &LookChoice) -> String {
    match &choice.read {
        Ok(identity) => format!("look profile {}", wording::short_hash(identity)),
        Err(refusal) if refusal.code == "project.outside_folder" => {
            "a link to a file outside the project's folder".into()
        }
        Err(refusal) => wording::problem(&refusal.code, |_| None).title,
    }
}

/// The actions that are on a key another action is on too, in these settings.
pub fn shared_keys(settings: &SettingsEdit) -> Vec<KeyAction> {
    let key = |action: KeyAction| action.key(&settings.controls);
    KeyAction::ALL
        .into_iter()
        .filter(|&action| {
            KeyAction::ALL
                .into_iter()
                .any(|other| other != action && key(other) == key(action))
        })
        .collect()
}

/// What a group of planned settings says.
pub const PLANNED_HINT: &str = "Planned. The game does not read these yet, so they cannot be \
    set.";

/// What the setting of the local test clients is called.
pub const LOCAL_CLIENTS: &str = "Local test clients";

/// What the dialog says of the local test clients.
pub const LOCAL_CLIENTS_HINT: &str = "How many game windows Play opens on this machine, each a \
    client of the one server it starts. Play uses the number that is saved.";

/// What the dialog says of the number of local test clients while the project file names
/// none: that it is one, and the file's to leave unnamed.
pub const LOCAL_CLIENTS_UNNAMED: &str = "The project file names no number, which is 1.";

/// The button that makes the default look profile where the project's is not in its folder.
pub const MAKE_LOOK: &str = "Make the default look profile";

/// What the dialog says of a project whose look profile is not in its folder, as a title and
/// what to do: the file is named, and what the button makes.
pub fn look_missing_words(profile: &str) -> (String, String) {
    (
        "The project's look profile is not in its folder".into(),
        format!(
            "The game does not start without it, and Game look in this editor is not drawn \
             from the project. \"{MAKE_LOOK}\" writes the profile the game has been drawn \
             with so far to {profile}. Nothing that is there is written over, and the \
             project file is not changed."
        ),
    )
}

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
                        align_items: AlignItems::Center,
                        column_gap: px(spacing::SPACE_4),
                        flex_shrink: 0.0,
                    }
                    Children [
                        (
                            Node { flex_grow: 1.0, min_width: px(0) }
                            Children [ (label_dim("") SettingsLine) ]
                        ),
                        (
                            @FeathersButton {
                                @caption: bsn! { Text("Discard changes") ThemedText }
                            }
                            DiscardSettingsButton
                            InteractionDisabled
                            Node { flex_shrink: 0.0 }
                            on(|_: On<Activate>, mut project: ResMut<Project>| {
                                project.discard_settings();
                            })
                        ),
                        (
                            @FeathersButton {
                                @caption: bsn! { Text("Save project settings") ThemedText },
                                @variant: ButtonVariant::Primary,
                            }
                            SaveSettingsButton
                            InteractionDisabled
                            Node { flex_shrink: 0.0 }
                            on(|_: On<Activate>, mut project: ResMut<Project>| {
                                project.save_settings();
                            })
                        ),
                        (
                            @FeathersButton {
                                @caption: bsn! { Text("Close settings") ThemedText }
                            }
                            Node { flex_shrink: 0.0 }
                            on(|_: On<Activate>, mut dialog: ResMut<ProjectDialog>| {
                                dialog.open = false;
                            })
                        ),
                    ]
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

/// The line of the dialog's foot that says whether the settings are saved.
#[derive(Component, Default, Clone)]
pub struct SettingsLine;

/// The button that saves the settings as they were changed.
#[derive(Component, Default, Clone)]
pub struct SaveSettingsButton;

/// The button that puts the settings back to the file's.
#[derive(Component, Default, Clone)]
pub struct DiscardSettingsButton;

/// The button of an action's key.
#[derive(Component, Default, Clone, Copy)]
pub struct KeyButton(pub KeyAction);

/// What is looked with.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LookDevice {
    #[default]
    Mouse,
    GamepadRightStick,
}

/// The checkbox of something that is looked with.
#[derive(Component, Default, Clone, Copy)]
pub struct LookWith(pub LookDevice);

/// The button that chooses a look profile, with the profile's path in the folder.
#[derive(Component, Default, Clone)]
pub struct UseLookButton(pub String);

/// The button that reads the project file again after another program changed it.
#[derive(Component, Default, Clone)]
pub struct ReadAgainButton;

/// The width of a key's button, so that the keys line up.
const KEY_WIDTH: f32 = 120.0;

fn ask_key(
    activate: On<Activate>,
    buttons: Query<&KeyButton>,
    mut project: ResMut<Project>,
    mut focus: ResMut<InputFocus>,
) {
    if let Ok(button) = buttons.get(activate.entity) {
        project.ask_key(button.0);
        // The key that is pressed next is the action's. It must not also be a press of
        // this button, as Space and Enter are of a button that has the focus.
        focus.clear();
    }
}

/// An action and the button of its key. While its key is asked for the button says so; an
/// action that shares its key with another says that beside it, in the warning colour.
fn key_row(action: KeyAction, key: &str, asking: bool, shared: bool) -> Box<dyn SceneList> {
    let caption = if asking {
        PRESS_A_KEY.to_owned()
    } else {
        key.to_owned()
    };
    let variant = if asking {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Normal
    };
    let note: Box<dyn SceneList> = if shared && !asking {
        Box::new(bsn_list![(label(KEY_SHARED) ThemeTextColor(SIGNAL_WARN))])
    } else {
        Box::new(bsn_list![])
    };
    Box::new(bsn_list![row(bsn_list![
        name_of(action.words()),
        (
            @FeathersButton { @caption: bsn! { Text({caption}) ThemedText }, @variant: {variant} }
            KeyButton({action})
            Node { min_width: px(KEY_WIDTH), flex_shrink: 0.0 }
            on(ask_key)
        ),
        {note}
    ])])
}

fn set_look(change: On<ValueChange<bool>>, boxes: Query<&LookWith>, mut project: ResMut<Project>) {
    match boxes.get(change.source).map(|look| look.0) {
        Ok(LookDevice::Mouse) => project.set_look(Some(change.value), None),
        Ok(LookDevice::GamepadRightStick) => project.set_look(None, Some(change.value)),
        Err(_) => {}
    }
}

/// The checkbox of something that is looked with, ticked or not.
fn look_box(device: LookDevice, caption: &'static str, ticked: bool) -> Box<dyn SceneList> {
    if ticked {
        Box::new(bsn_list![(
            @FeathersCheckbox { @caption: bsn! { Text(caption) ThemedText } }
            LookWith({device})
            Checked
            on(set_look)
        )])
    } else {
        Box::new(bsn_list![(
            @FeathersCheckbox { @caption: bsn! { Text(caption) ThemedText } }
            LookWith({device})
            on(set_look)
        )])
    }
}

fn use_look(activate: On<Activate>, buttons: Query<&UseLookButton>, mut project: ResMut<Project>) {
    if let Ok(button) = buttons.get(activate.entity) {
        project.choose_look(&button.0);
    }
}

/// A look profile of the folder: its file, what the look reader made of it, and the button
/// that chooses it. The chosen one says that it is; one the reader refuses cannot be chosen
/// and says why, in the warning colour.
fn look_row(choice: &LookChoice, chosen: bool) -> Box<dyn SceneList> {
    let path = choice.path.clone();
    let words = look_choice_words(choice);
    let usable = choice.read.is_ok();
    let said: Box<dyn SceneList> = if usable {
        Box::new(bsn_list![label_dim(words)])
    } else {
        Box::new(bsn_list![(label(words) ThemeTextColor(SIGNAL_WARN))])
    };
    let button: Box<dyn SceneList> = if chosen {
        Box::new(bsn_list![(
            @FeathersButton {
                @caption: bsn! { Text("Selected") ThemedText },
                @variant: ButtonVariant::Primary,
            }
            InteractionDisabled
            Node { flex_shrink: 0.0 }
        )])
    } else if usable {
        Box::new(bsn_list![(
            @FeathersButton { @caption: bsn! { Text("Use") ThemedText } }
            UseLookButton({path.clone()})
            Node { flex_shrink: 0.0 }
            on(use_look)
        )])
    } else {
        Box::new(bsn_list![(
            @FeathersButton { @caption: bsn! { Text("Use") ThemedText } }
            InteractionDisabled
            Node { flex_shrink: 0.0 }
        )])
    };
    Box::new(bsn_list![row(bsn_list![
        (
            Node { flex_grow: 1.0, min_width: px(0), flex_direction: FlexDirection::Column }
            Children [ label(path), {said} ]
        ),
        {button}
    ])])
}

/// The button of a number of local test clients.
#[derive(Component, Default, Clone, Copy)]
pub struct LocalClientsButton(pub u8);

/// The width of the button of a number of clients.
const COUNT_WIDTH: f32 = 36.0;

fn set_local_clients(
    activate: On<Activate>,
    buttons: Query<&LocalClientsButton>,
    mut project: ResMut<Project>,
) {
    if let Ok(button) = buttons.get(activate.entity) {
        project.set_local_clients(button.0);
    }
}

/// How many clients Play starts on this machine: a button for each number, the one that is
/// set marked as the chosen one.
fn local_clients_row(chosen: u8) -> Box<dyn SceneList> {
    let button = |clients: u8| {
        let variant = if clients == chosen {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Normal
        };
        let caption = clients.to_string();
        bsn! {
            @FeathersButton { @caption: bsn! { Text({caption}) ThemedText }, @variant: {variant} }
            LocalClientsButton({clients})
            Node { min_width: px(COUNT_WIDTH), flex_shrink: 0.0 }
            on(set_local_clients)
        }
    };
    Box::new(bsn_list![row(bsn_list![
        name_of(LOCAL_CLIENTS),
        button(1),
        button(2),
        button(3),
        button(4),
    ])])
}

/// Hands the number of local test clients the project file names to the part of the editor
/// that starts the game, whenever it is another: what Play starts is the saved number.
pub(crate) fn hand_local_clients_to_play(project: Res<Project>, mut editor: NonSendMut<Editor>) {
    let clients = project.local_clients();
    if editor.local_clients() != clients {
        editor.set_local_clients(clients);
    }
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
pub(crate) fn warning(title: String, remedy: String, more: Option<String>) -> Box<dyn SceneList> {
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

/// The button that makes the default look profile.
#[derive(Component, Default, Clone)]
pub struct MakeLookButton;

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
        if refusal.code == "project.changed_since_read" {
            rows.push(Box::new(bsn_list![(
                @FeathersButton { @caption: bsn! { Text("Read project file again") ThemedText } }
                ReadAgainButton
                Node { align_self: AlignSelf::FlexStart, flex_shrink: 0.0 }
                on(|_: On<Activate>, mut project: ResMut<Project>| project.read_again())
            )]));
        }
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
                rows.push(shown(
                    "Look profile",
                    match &plan.profile {
                        Some(profile) => format!("{}, which is made", relative(profile)),
                        None => "the one in the folder, which is kept".to_owned(),
                    },
                ));
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
            }
            if let Some((title, remedy)) =
                start_level_words(*start_level_there, project.play_blocker())
            {
                rows.push(warning(title, remedy, None));
            }

            // The settings as they are being changed, which are the file's until they are.
            let settings = project
                .settings()
                .unwrap_or_else(|| SettingsEdit::of(config));

            rows.push(Box::new(bsn_list![
                group_title("Rendering"),
                label_dim(LOOK_HINT)
            ]));
            rows.push(shown("Look profile", settings.rendering_profile.clone()));
            for choice in project.look_choices() {
                rows.push(look_row(choice, choice.path == settings.rendering_profile));
            }
            let chosen_usable = project
                .look_choices()
                .iter()
                .any(|choice| choice.path == settings.rendering_profile && choice.read.is_ok());
            // The profile the project file names is not there: it can be made, on the
            // person's word. Not while another one is being chosen in its place.
            let missing = project.look_missing()
                && project
                    .config()
                    .is_some_and(|config| config.rendering.profile == settings.rendering_profile);
            if missing {
                let (title, remedy) = look_missing_words(&settings.rendering_profile);
                rows.push(warning(title, remedy, None));
                rows.push(Box::new(bsn_list![(
                    @FeathersButton {
                        @caption: bsn! { Text(MAKE_LOOK) ThemedText },
                        @variant: ButtonVariant::Primary,
                    }
                    MakeLookButton
                    Node { align_self: AlignSelf::FlexStart, flex_shrink: 0.0 }
                    on(|_: On<Activate>, mut project: ResMut<Project>| {
                        project.install_look();
                    })
                )]));
            } else if !chosen_usable {
                rows.push(warning(
                    "The project's look profile cannot be used".into(),
                    "The game does not start with it. Choose another here, or put the file \
                     right."
                        .into(),
                    None,
                ));
            }
            rows.push(shown("In this editor", look_words(look)));

            rows.push(Box::new(bsn_list![
                group_title("Controls"),
                label_dim(KEYS_HINT)
            ]));
            let shared = shared_keys(&settings);
            for action in KeyAction::ALL {
                rows.push(key_row(
                    action,
                    action.key(&settings.controls),
                    project.asking() == Some(action),
                    shared.contains(&action),
                ));
            }
            if project.refused_key() {
                rows.push(Box::new(bsn_list![(
                    label(KEY_REFUSED) ThemeTextColor(SIGNAL_WARN)
                )]));
            }
            let look_with = &settings.controls.look;
            let mouse = look_box(LookDevice::Mouse, "Mouse", look_with.mouse);
            let stick = look_box(
                LookDevice::GamepadRightStick,
                "Gamepad right stick",
                look_with.gamepad_right_stick,
            );
            rows.push(Box::new(bsn_list![row(bsn_list![
                name_of("Look with"),
                { mouse },
                { stick },
            ])]));
            if !look_with.mouse && !look_with.gamepad_right_stick {
                rows.push(Box::new(bsn_list![label_dim(NO_LOOK)]));
            }

            for (group, planned_settings) in planned_rows(config) {
                rows.push(Box::new(bsn_list![group_title(group)]));
                // The one setting of the group that is read comes first, and is set here.
                if group == MULTIPLAYER {
                    rows.push(Box::new(bsn_list![label_dim(LOCAL_CLIENTS_HINT)]));
                    rows.push(local_clients_row(settings.local_clients.unwrap_or(1)));
                    if settings.local_clients.is_none() {
                        rows.push(Box::new(bsn_list![label_dim(LOCAL_CLIENTS_UNNAMED)]));
                    }
                }
                rows.push(Box::new(bsn_list![label_dim(PLANNED_HINT)]));
                for (what, value) in planned_settings {
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
    // The settings as they are being changed, the profiles to choose from, and the key
    // that is asked for.
    (
        Option<SettingsEdit>,
        Vec<LookChoice>,
        Option<KeyAction>,
        bool,
    ),
    // What of the project's own files stops the game, as it was last checked.
    Option<scene_data::Diagnostic>,
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
        (
            project.settings(),
            project.look_choices().to_vec(),
            project.asking(),
            project.refused_key(),
        ),
        project.play_blocker().cloned(),
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

/// Gives the key that is pressed to the action whose key is asked for. Only while the
/// dialog is open; the first key that goes down in a frame is the one.
pub(crate) fn press_project_key(
    mut pressed: MessageReader<KeyboardInput>,
    dialog: Res<ProjectDialog>,
    mut project: ResMut<Project>,
) {
    let keys: Vec<KeyCode> = pressed
        .read()
        .filter(|input| input.state == ButtonState::Pressed && !input.repeat)
        .map(|input| input.key_code)
        .collect();
    if !dialog.open {
        // A question that was open when the dialog was closed is not asked any more.
        if project.asking().is_some() {
            project.stop_asking();
        }
        return;
    }
    if project.asking().is_none() {
        return;
    }
    if let Some(&key) = keys.first() {
        project.press_key(key);
    }
}

/// What the dialog's foot says and offers: whether there is anything to save, and the two
/// buttons while there is.
pub(crate) fn fit_project_foot(
    mut commands: Commands,
    project: Res<Project>,
    mut lines: Query<(Entity, &mut Text), With<SettingsLine>>,
    save: Query<(Entity, Has<InteractionDisabled>), With<SaveSettingsButton>>,
    discard: Query<(Entity, Has<InteractionDisabled>), With<DiscardSettingsButton>>,
    mut warned: Local<Option<bool>>,
) {
    let state = project.draft_state();
    let (words, warn) = settings_line(&state, project.settings_saved());
    for (entity, mut text) in &mut lines {
        if text.0 != words {
            words.clone_into(&mut text.0);
        }
        // The colour is a token that is set anew when it is to be another.
        if *warned != Some(warn) {
            *warned = Some(warn);
            let color = if warn { SIGNAL_WARN } else { tokens::TEXT_DIM };
            commands.entity(entity).insert(ThemeTextColor(color));
        }
    }
    let offered = [
        (state == DraftState::Changed, save.iter().next()),
        (project.settings_unsaved(), discard.iter().next()),
    ];
    for (offered, button) in offered {
        let Some((entity, off)) = button else {
            continue;
        };
        if offered && off {
            commands.entity(entity).remove::<InteractionDisabled>();
        } else if !offered && !off {
            commands.entity(entity).insert(InteractionDisabled);
        }
    }
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
    fn a_start_level_the_game_cannot_open_is_said_where_play_sends_the_person() {
        use scene_data::Diagnostic;
        const LEVEL: &str = "/p/levels/first-level.level.json";
        // Nothing is said of a level that is there and that nothing is known against.
        assert_eq!(start_level_words(true, None), None);
        // One that is not there is said as before, whatever else the check found.
        let missing = Diagnostic::new("play.project_start_level_missing").with("path", LEVEL);
        for blocker in [None, Some(&missing)] {
            let (title, remedy) = start_level_words(false, blocker).unwrap();
            assert_eq!(
                title,
                "The level the project starts with is not in the folder"
            );
            assert!(remedy.contains("Save a level under that name"));
        }
        // One that is there and is no level: what it is, the file, and what mends it, and
        // neither the code nor what the reader said.
        let unreadable = Diagnostic::new("source.decode")
            .with("path", LEVEL)
            .with("detail", "expected value at line 1 column 1");
        let (title, remedy) = start_level_words(true, Some(&unreadable)).unwrap();
        assert_eq!(title, "The level the project starts with cannot be used");
        assert!(remedy.contains(LEVEL) && remedy.contains("put a file there that can be read"));
        assert!(!remedy.contains("source.decode") && !remedy.contains("expected value"));
        // What stops the game for another reason is said in its own place, not here.
        let profile = Diagnostic::new("play.project_profile_unavailable")
            .with("profile", "assets/render/look-v1.json")
            .with("cause", "look.decode");
        assert_eq!(start_level_words(true, Some(&profile)), None);
        let manifest = Diagnostic::new("project.read").with("path", "/p/arc-project.json");
        assert_eq!(start_level_words(true, Some(&manifest)), None);
    }

    #[test]
    fn what_setting_up_and_converting_do_is_said_before_the_person_agrees() {
        assert!(SETUP_HINT.contains("arc-project.json"));
        assert!(SETUP_HINT.contains("No file that is there is changed"));
        assert!(SETUP_HINT.contains("look profile") && SETUP_HINT.contains("has none"));
        let (title, remedy) = look_missing_words("assets/render/look-v1.json");
        assert_eq!(title, "The project's look profile is not in its folder");
        assert!(remedy.contains(MAKE_LOOK) && remedy.contains("assets/render/look-v1.json"));
        assert!(remedy.contains("Nothing that is there is written over"));
        assert!(remedy.contains("project file is not changed"));
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
        // The local test clients are named as that, and said to be windows on this machine
        // and the saved number.
        assert_eq!(LOCAL_CLIENTS, "Local test clients");
        assert!(LOCAL_CLIENTS_HINT.contains("on this machine"));
        assert!(LOCAL_CLIENTS_HINT.contains("the number that is saved"));
        assert!(LOCAL_CLIENTS_UNNAMED.contains("names no number, which is 1"));
        assert!(NO_FOLDER.contains("--game"));
        assert!(set_up_words("levels/a.level.json", true).contains("starter level that was made"));
        assert!(set_up_words("levels/a.level.json", false).contains("was in the folder"));
    }

    #[test]
    fn the_foot_says_whether_there_is_anything_to_save_and_why_it_cannot_be() {
        assert_eq!(
            settings_line(&DraftState::Unchanged, false),
            (String::new(), false)
        );
        assert_eq!(
            settings_line(&DraftState::Unchanged, true),
            ("Saved".into(), false)
        );
        assert_eq!(
            settings_line(&DraftState::Changed, false),
            ("Unsaved changes".into(), true)
        );
        // Changes that are made after a save are unsaved, whatever was saved before.
        assert_eq!(
            settings_line(&DraftState::Changed, true).0,
            "Unsaved changes"
        );
        let (words, warn) = settings_line(
            &DraftState::SharedKey {
                first: KeyAction::MoveForward,
                second: KeyAction::Jump,
            },
            false,
        );
        assert!(warn);
        assert!(words.starts_with("Move forward and Jump are on the same key"));
        let (words, warn) = settings_line(&DraftState::Refused("too long".into()), false);
        assert!(warn && words == "Cannot be saved: too long.");
    }

    #[test]
    fn the_actions_that_share_a_key_are_each_marked() {
        let mut settings = SettingsEdit::of(&crate::project::ProjectConfig::new("Game"));
        assert!(shared_keys(&settings).is_empty());
        settings.controls.jump = settings.controls.move_left.clone();
        assert_eq!(
            shared_keys(&settings),
            [KeyAction::MoveLeft, KeyAction::Jump]
        );
        settings.controls.toggle_view = settings.controls.move_left.clone();
        assert_eq!(shared_keys(&settings).len(), 3);
    }

    #[test]
    fn a_look_profile_says_that_it_was_read_or_why_it_cannot_be_chosen() {
        let read = LookChoice {
            path: "assets/render/look-v1.json".into(),
            read: Ok("3abf9a2fbcb48b74".into()),
        };
        assert_eq!(look_choice_words(&read), "look profile 3abf9a2f");
        let missing = LookChoice {
            path: "assets/render/none.json".into(),
            read: Err(scene_data::Diagnostic::new("look.missing")),
        };
        let words = look_choice_words(&missing);
        assert!(
            !words.is_empty() && !words.contains("look.missing"),
            "{words}"
        );
        let linked = LookChoice {
            path: "assets/render/linked.json".into(),
            read: Err(scene_data::Diagnostic::new("project.outside_folder")),
        };
        assert!(look_choice_words(&linked).contains("outside the project's folder"));
        // What the dialog says of changing a key and of the profile is said before it is done.
        assert!(KEYS_HINT.contains("press its button and then the key"));
        assert!(LOOK_HINT.contains("one that is saved here is used at once"));
        assert!(KEY_REFUSED.contains("does not read that key"));
    }
}
