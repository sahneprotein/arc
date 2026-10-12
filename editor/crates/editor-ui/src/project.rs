//! The project as the editor holds it: the folder's `arc-project.json`, and what can be done
//! about it when it is not there or is of an older version.
//!
//! A project is the game's folder with a manifest at its root. The manifest names the level
//! the game starts with, the game's look profile and the keys the game uses now; it also has
//! places for settings the game does not read yet, which are shown and cannot be set.
//!
//! Nothing here writes by itself. A folder without a manifest is set up only when the person
//! asks: the manifest is written, and it names the first level of the folder that can be
//! read, or a starter level that is made beside the others. No file that is there is
//! changed. A manifest of an older version is converted only when the person asks, and the
//! file as it was is kept beside it first. Reading and writing are the services'
//! (`editor_services::project`); the format is `project_spec`'s.
//!
//! The keys and the look profile of a project that is read can be changed. What is changed
//! is a draft the editor holds beside the manifest: looking at the settings and changing
//! them writes nothing. Only a save writes, on the person's word, and the services refuse
//! it rather than write over a manifest that is no longer the one that was read. A draft
//! in which two actions share a key is kept, so that two keys can be exchanged, and is not
//! saved while it is so.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use editor_services::{
    project::{self, FreshStarter, ProjectSetup},
    project_spec,
};

pub use editor_services::project::{LookChoice, RejectedLevel, SettingsEdit, SetupPlan};
use scene_data::Diagnostic;

pub use editor_services::project_spec::ProjectConfig;

/// What the editor found of the project.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum ProjectState {
    /// The editor was started without a project folder.
    #[default]
    NoFolder,
    /// The folder has no manifest: it can be set up.
    NotSetUp,
    /// The manifest is of another version than this editor reads. It is not changed until
    /// the person asks for the conversion. `detail` is what the reader said.
    NeedsConversion { detail: String },
    /// The manifest is there and cannot be used, with the reader's refusal.
    Refused(Diagnostic),
    /// The manifest is read. `start_level_there` says whether the level it starts with is a
    /// file of the folder.
    Ready {
        config: Box<ProjectConfig>,
        start_level_there: bool,
    },
}

impl ProjectState {
    /// One word for the state, for the log and the checks.
    pub fn word(&self) -> &'static str {
        match self {
            Self::NoFolder => "NoFolder",
            Self::NotSetUp => "NotSetUp",
            Self::NeedsConversion { .. } => "NeedsConversion",
            Self::Refused(_) => "Refused",
            Self::Ready { .. } => "Ready",
        }
    }
}

/// What the last setup did, to say it once: the level the project starts with, relative to
/// the folder, and whether that level was made or was there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetUp {
    pub start_level: String,
    pub made_level: bool,
}

/// The project of the folder the editor was started with.
#[derive(Resource, Default)]
pub struct Project {
    root: Option<PathBuf>,
    state: ProjectState,
    /// What a setup or a conversion was last refused for.
    problem: Option<Diagnostic>,
    /// What the setup of this session made.
    set_up: Option<SetUp>,
    /// What a setup of the folder would do, while it is not set up: found by reading.
    plan: Option<SetupPlan>,
    /// The settings as they are being changed, while they are not the manifest's.
    draft: Option<SettingsEdit>,
    /// The look profiles of the folder, as they were when the manifest was read.
    looks: Vec<LookChoice>,
    /// The action whose key is being asked for.
    asking: Option<KeyAction>,
    /// The key that was pressed for it and is none a binding can name.
    refused_key: bool,
    /// Whether the last thing done to the settings was a save that went through.
    saved: bool,
    /// How many saves of the settings went through since the editor started.
    saves: u64,
    /// How many look profiles were made here since the editor started.
    looks_made: u64,
    /// What of the project's own files stops the game from being started with it, as the
    /// last check found it ([`Project::check_play`]).
    play_blocker: Option<Diagnostic>,
}

/// What a key is bound to, in the manifest's order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum KeyAction {
    #[default]
    MoveForward,
    MoveBackward,
    MoveLeft,
    MoveRight,
    Jump,
    ToggleView,
    ReleaseCursor,
}

impl KeyAction {
    pub const ALL: [Self; 7] = [
        Self::MoveForward,
        Self::MoveBackward,
        Self::MoveLeft,
        Self::MoveRight,
        Self::Jump,
        Self::ToggleView,
        Self::ReleaseCursor,
    ];

    /// What the action is called in the settings.
    pub fn words(self) -> &'static str {
        match self {
            Self::MoveForward => "Move forward",
            Self::MoveBackward => "Move backward",
            Self::MoveLeft => "Move left",
            Self::MoveRight => "Move right",
            Self::Jump => "Jump",
            Self::ToggleView => "Change view",
            Self::ReleaseCursor => "Release cursor",
        }
    }

    /// The manifest's field of the action, as a refusal names it.
    pub fn field(self) -> &'static str {
        match self {
            Self::MoveForward => "controls.move_forward",
            Self::MoveBackward => "controls.move_backward",
            Self::MoveLeft => "controls.move_left",
            Self::MoveRight => "controls.move_right",
            Self::Jump => "controls.jump",
            Self::ToggleView => "controls.toggle_view",
            Self::ReleaseCursor => "controls.release_cursor",
        }
    }

    /// The action a field of the manifest is, if it is a key's.
    pub fn of_field(field: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.field() == field)
    }

    /// The key the action is on in these controls.
    pub fn key(self, controls: &project_spec::ControlSettings) -> &str {
        match self {
            Self::MoveForward => &controls.move_forward,
            Self::MoveBackward => &controls.move_backward,
            Self::MoveLeft => &controls.move_left,
            Self::MoveRight => &controls.move_right,
            Self::Jump => &controls.jump,
            Self::ToggleView => &controls.toggle_view,
            Self::ReleaseCursor => &controls.release_cursor,
        }
    }

    fn key_mut(self, controls: &mut project_spec::ControlSettings) -> &mut String {
        match self {
            Self::MoveForward => &mut controls.move_forward,
            Self::MoveBackward => &mut controls.move_backward,
            Self::MoveLeft => &mut controls.move_left,
            Self::MoveRight => &mut controls.move_right,
            Self::Jump => &mut controls.jump,
            Self::ToggleView => &mut controls.toggle_view,
            Self::ReleaseCursor => &mut controls.release_cursor,
        }
    }
}

/// The keys of the keyboard a binding can name, each with the name the manifest has for it:
/// `project_spec::KEY_NAMES`, in that order.
const KEYS: [(KeyCode, &str); 46] = [
    (KeyCode::KeyA, "A"),
    (KeyCode::KeyB, "B"),
    (KeyCode::KeyC, "C"),
    (KeyCode::KeyD, "D"),
    (KeyCode::KeyE, "E"),
    (KeyCode::KeyF, "F"),
    (KeyCode::KeyG, "G"),
    (KeyCode::KeyH, "H"),
    (KeyCode::KeyI, "I"),
    (KeyCode::KeyJ, "J"),
    (KeyCode::KeyK, "K"),
    (KeyCode::KeyL, "L"),
    (KeyCode::KeyM, "M"),
    (KeyCode::KeyN, "N"),
    (KeyCode::KeyO, "O"),
    (KeyCode::KeyP, "P"),
    (KeyCode::KeyQ, "Q"),
    (KeyCode::KeyR, "R"),
    (KeyCode::KeyS, "S"),
    (KeyCode::KeyT, "T"),
    (KeyCode::KeyU, "U"),
    (KeyCode::KeyV, "V"),
    (KeyCode::KeyW, "W"),
    (KeyCode::KeyX, "X"),
    (KeyCode::KeyY, "Y"),
    (KeyCode::KeyZ, "Z"),
    (KeyCode::Digit0, "0"),
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit2, "2"),
    (KeyCode::Digit3, "3"),
    (KeyCode::Digit4, "4"),
    (KeyCode::Digit5, "5"),
    (KeyCode::Digit6, "6"),
    (KeyCode::Digit7, "7"),
    (KeyCode::Digit8, "8"),
    (KeyCode::Digit9, "9"),
    (KeyCode::Space, "Space"),
    (KeyCode::Escape, "Escape"),
    (KeyCode::Enter, "Enter"),
    (KeyCode::Tab, "Tab"),
    (KeyCode::Backspace, "Backspace"),
    (KeyCode::Delete, "Delete"),
    (KeyCode::ArrowUp, "ArrowUp"),
    (KeyCode::ArrowDown, "ArrowDown"),
    (KeyCode::ArrowLeft, "ArrowLeft"),
    (KeyCode::ArrowRight, "ArrowRight"),
];

/// The name the manifest has for this key of the keyboard, if a binding can name it.
pub fn key_name(key: KeyCode) -> Option<&'static str> {
    KEYS.iter()
        .find(|(code, _)| *code == key)
        .map(|(_, name)| *name)
}

/// Whether the settings as they are being changed can be saved, and if not, why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftState {
    /// They are the manifest's: there is nothing to save.
    Unchanged,
    /// They differ from the manifest's and are a manifest the game can play with.
    Changed,
    /// Two actions share a key: `first` and `second` in the manifest's order.
    SharedKey { first: KeyAction, second: KeyAction },
    /// They are refused for another reason, in the reader's words.
    Refused(String),
}

impl Project {
    /// The project of this folder as it is on disk now. Nothing is written.
    pub fn of_folder(root: Option<PathBuf>) -> Self {
        let mut project = Self { root, ..default() };
        project.read();
        project
    }

    /// Reads the manifest again. Nothing is written, whatever is found. Settings that are
    /// being changed stay as they are while the manifest is one that can be read.
    pub fn read(&mut self) {
        self.read_manifest();
        self.asking = None;
        self.refused_key = false;
        self.looks = match (&self.root, &self.state) {
            (Some(root), ProjectState::Ready { config, .. }) => {
                let mut looks = project::look_profiles(root);
                // The profile the manifest names is always among them, wherever it is, so
                // that what is chosen now is shown with what the look reader makes of it.
                let named = &config.rendering.profile;
                if !looks.iter().any(|choice| &choice.path == named) {
                    looks.insert(
                        0,
                        LookChoice {
                            path: named.clone(),
                            read: project::look_profile(root, named).map(|look| look.sha256_hex),
                        },
                    );
                }
                looks
            }
            _ => Vec::new(),
        };
        if self.config().is_none() {
            self.draft = None;
        }
        self.settle();
        self.check_play();
    }

    /// Asks the services whether the game could be started with this project as its files
    /// are now: the project file, the level it starts with and its look profile, which the
    /// game opens in the project's folder. The same check is made before every start, and
    /// that one decides; this one is what the Play menu says in the meantime. It reads
    /// files, so it is asked for when something changed and not while drawing: when the
    /// manifest is read, which is also after a setup, a conversion, a save of the settings
    /// and a look profile that was made, and by whoever knows that a level was saved.
    pub fn check_play(&mut self) {
        self.play_blocker = match &self.root {
            Some(root) => editor_services::play::check_project_play_inputs(root).err(),
            None => None,
        };
    }

    /// What stops the game from being started with this project, as the last check found
    /// it. `None` without a project folder: such an editor has no project to answer for.
    pub fn play_blocker(&self) -> Option<&Diagnostic> {
        self.play_blocker.as_ref()
    }

    fn read_manifest(&mut self) {
        self.plan = None;
        let Some(root) = &self.root else {
            self.state = ProjectState::NoFolder;
            return;
        };
        // Anything of the manifest's name counts as a manifest, a link that leads nowhere
        // too: it is then refused, not taken for a folder to set up.
        self.state = if std::fs::symlink_metadata(project::project_path(root)).is_err() {
            match project::plan_setup(root) {
                Ok(plan) => {
                    self.plan = Some(plan);
                    ProjectState::NotSetUp
                }
                Err(refusal) => ProjectState::Refused(refusal),
            }
        } else {
            match project::load_project(root) {
                // A start level that leads out of the folder makes the project one that
                // cannot be used: the editor does not follow it there.
                Ok(config) => match project::start_level_path(root, &config) {
                    Ok(level) => ProjectState::Ready {
                        start_level_there: level.is_file(),
                        config: Box::new(config),
                    },
                    Err(refusal) => ProjectState::Refused(refusal),
                },
                Err(refusal) if refusal.code == "project.convert_required" => {
                    ProjectState::NeedsConversion {
                        detail: refusal
                            .parameters
                            .get("detail")
                            .cloned()
                            .unwrap_or_default(),
                    }
                }
                Err(refusal) => ProjectState::Refused(refusal),
            }
        };
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn state(&self) -> &ProjectState {
        &self.state
    }

    /// The manifest, if it is read.
    pub fn config(&self) -> Option<&ProjectConfig> {
        match &self.state {
            ProjectState::Ready { config, .. } => Some(config),
            _ => None,
        }
    }

    /// The manifest's file, whether it is there or not.
    pub fn manifest_path(&self) -> Option<PathBuf> {
        self.root.as_deref().map(project::project_path)
    }

    /// What a setup or a conversion was last refused for.
    pub fn problem(&self) -> Option<&Diagnostic> {
        self.problem.as_ref()
    }

    /// What the setup of this session made.
    pub fn last_set_up(&self) -> Option<&SetUp> {
        self.set_up.as_ref()
    }

    /// What a setup of the folder would do, while the folder is not set up: the level it
    /// would start with, the level files it cannot read, and the starter it would make.
    pub fn setup_plan(&self) -> Option<&SetupPlan> {
        self.plan.as_ref()
    }

    /// Whether the person is to be shown the project's panel without asking for it: the
    /// folder has no manifest yet, one that waits to be converted, or one that cannot be
    /// used; or the project is read and the level it starts with is not in the folder,
    /// which the panel says with what to do about it.
    pub fn wants_attention(&self) -> bool {
        matches!(
            self.state,
            ProjectState::NotSetUp
                | ProjectState::NeedsConversion { .. }
                | ProjectState::Refused(_)
                | ProjectState::Ready {
                    start_level_there: false,
                    ..
                }
        )
    }

    /// The name a new project is offered: the folder's.
    pub fn default_name(&self) -> String {
        self.root
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Arc project".to_owned())
    }

    /// The level the project starts with, if the manifest is read and the level is a file.
    pub fn start_level(&self) -> Option<PathBuf> {
        match (&self.root, &self.state) {
            (
                Some(root),
                ProjectState::Ready {
                    config,
                    start_level_there: true,
                },
            ) => Some(root.join(&config.startup.start_level)),
            _ => None,
        }
    }

    /// Sets the folder up as a project called `name`: writes the manifest, naming the first
    /// level of the folder that can be read, or a starter level made beside the others. No
    /// file that is there is changed. Returns the level the project starts with; `None`
    /// when the folder is not one to set up or the setup was refused, which the problem
    /// says.
    ///
    /// A folder whose level files all cannot be read is set up only with `fresh_starter`:
    /// the person saw which files and why, and agreed to a new starter level beside them.
    /// Without it such a setup is refused and nothing is made.
    pub fn set_up(&mut self, name: &str, fresh_starter: bool) -> Option<PathBuf> {
        let root = self.root.clone()?;
        if self.state != ProjectState::NotSetUp {
            return None;
        }
        // The name is looked at before anything is made: a setup that is refused for its
        // name must leave nothing behind, not even a starter level.
        let name = name.trim();
        if let Err(refusal) = project_spec::validate(&ProjectConfig::new(name)) {
            self.problem = Some(
                Diagnostic::new("project.value")
                    .with("path", project::project_path(&root).display())
                    .with("detail", refusal),
            );
            return None;
        }
        let starter = if fresh_starter {
            FreshStarter::Agreed
        } else {
            FreshStarter::OnlyWithoutLevels
        };
        match project::create_project_with(&root, name, starter) {
            Ok(ProjectSetup {
                config,
                start_level_path,
                created_level,
                ..
            }) => {
                self.problem = None;
                self.set_up = Some(SetUp {
                    start_level: config.startup.start_level,
                    made_level: created_level,
                });
                self.read();
                Some(start_level_path)
            }
            Err(refusal) => {
                self.problem = Some(refusal);
                self.read();
                None
            }
        }
    }

    /// Converts the manifest to the version this editor reads, keeping the file as it was
    /// beside it first. Only on the person's word. Whether it is read now; if not, the
    /// problem says why, and the manifest is as it was.
    pub fn convert(&mut self) -> bool {
        let Some(root) = self.root.clone() else {
            return false;
        };
        if !matches!(self.state, ProjectState::NeedsConversion { .. }) {
            return false;
        }
        match project::convert_project(&root) {
            Ok(_) => {
                self.problem = None;
                self.read();
                matches!(self.state, ProjectState::Ready { .. })
            }
            Err(refusal) => {
                self.problem = Some(refusal);
                self.read();
                false
            }
        }
    }
}

/// The project's settings as they are changed and saved.
impl Project {
    /// The look profile the manifest names, as a file and as the look reader takes it: what
    /// Game look in the editor is drawn from. `None` without a manifest that is read. A
    /// profile that is a link out of the folder is refused, not followed.
    pub fn look_profile(
        &self,
    ) -> Option<(PathBuf, Result<editor_services::LoadedLook, Diagnostic>)> {
        let (root, config) = (self.root.as_deref()?, self.config()?);
        let named = &config.rendering.profile;
        Some((root.join(named), project::look_profile(root, named)))
    }

    /// The settings as the dialog shows them: as they are being changed, or the manifest's.
    pub fn settings(&self) -> Option<SettingsEdit> {
        let config = self.config()?;
        Some(
            self.draft
                .clone()
                .unwrap_or_else(|| SettingsEdit::of(config)),
        )
    }

    /// Whether the settings can be saved, and if not, why.
    pub fn draft_state(&self) -> DraftState {
        let (Some(config), Some(draft)) = (self.config(), &self.draft) else {
            return DraftState::Unchanged;
        };
        match project_spec::validate(&draft.applied_to(config)) {
            Ok(()) => DraftState::Changed,
            Err(project_spec::ProjectError::SharedKey { first, second }) => {
                match (KeyAction::of_field(first), KeyAction::of_field(second)) {
                    (Some(first), Some(second)) => DraftState::SharedKey { first, second },
                    _ => DraftState::Refused(format!("{first} and {second} name the same key")),
                }
            }
            Err(refusal) => DraftState::Refused(refusal.to_string()),
        }
    }

    /// Whether there are settings that were changed and not saved.
    pub fn settings_unsaved(&self) -> bool {
        self.draft.is_some()
    }

    /// Whether the last thing done to the settings was a save that went through.
    pub fn settings_saved(&self) -> bool {
        self.saved
    }

    /// How many saves of the settings went through since the editor started. What the
    /// editor draws from the manifest, as Game look from its look profile, is read again
    /// when this grows.
    pub fn settings_saves(&self) -> u64 {
        self.saves
    }

    /// The look profiles of the folder to choose from.
    pub fn look_choices(&self) -> &[LookChoice] {
        &self.looks
    }

    /// Whether the look profile the manifest names is not in the folder: nothing has that
    /// name. A profile that is there and cannot be read is another matter, and is the
    /// person's file to put right.
    pub fn look_missing(&self) -> bool {
        self.config().is_some_and(|config| {
            self.looks.iter().any(|choice| {
                choice.path == config.rendering.profile
                    && matches!(&choice.read, Err(refusal) if refusal.code == "look.missing")
            })
        })
    }

    /// Makes the default look profile where the manifest names one that is not in the
    /// folder, on the person's word. Nothing that is there is written over, and the
    /// manifest is not written. Whether it was made; if not, the problem says why.
    pub fn install_look(&mut self) -> bool {
        let (Some(root), Some(named)) = (
            self.root.clone(),
            self.config().map(|config| config.rendering.profile.clone()),
        ) else {
            return false;
        };
        if !self.look_missing() {
            return false;
        }
        match project::install_default_look(&root, &named) {
            Ok(_) => {
                self.problem = None;
                self.read();
                self.looks_made += 1;
                true
            }
            Err(refusal) => {
                self.problem = Some(refusal);
                self.read();
                false
            }
        }
    }

    /// How many look profiles were made here since the editor started. What the editor
    /// draws from the profile is read again when this grows.
    pub fn looks_made(&self) -> u64 {
        self.looks_made
    }

    /// The action whose key is being asked for.
    pub fn asking(&self) -> Option<KeyAction> {
        self.asking
    }

    /// Whether the key last pressed for it is none a binding can name.
    pub fn refused_key(&self) -> bool {
        self.refused_key
    }

    /// Asks for the key of this action, or stops asking when it is the one asked for.
    pub fn ask_key(&mut self, action: KeyAction) {
        self.refused_key = false;
        self.asking = match self.asking {
            Some(asked) if asked == action => None,
            _ if self.config().is_some() => Some(action),
            _ => None,
        };
    }

    /// Stops asking for a key, and leaves every key as it is.
    pub fn stop_asking(&mut self) {
        self.asking = None;
        self.refused_key = false;
    }

    /// A key of the keyboard was pressed while one was asked for: the action is on that key
    /// in the draft, and nothing is asked for any more. A key no binding can name leaves
    /// the question open and is said to be refused. Whether the key was taken.
    pub fn press_key(&mut self, key: KeyCode) -> bool {
        let Some(action) = self.asking else {
            return false;
        };
        let Some(name) = key_name(key) else {
            self.refused_key = true;
            return false;
        };
        self.asking = None;
        self.refused_key = false;
        self.change(|edit| *action.key_mut(&mut edit.controls) = name.to_owned());
        true
    }

    /// Whether the mouse, and whether the gamepad's right stick, is looked with.
    pub fn set_look(&mut self, mouse: Option<bool>, gamepad_right_stick: Option<bool>) {
        self.change(|edit| {
            let look = &mut edit.controls.look;
            look.mouse = mouse.unwrap_or(look.mouse);
            look.gamepad_right_stick = gamepad_right_stick.unwrap_or(look.gamepad_right_stick);
        });
    }

    /// Chooses the look profile at this path in the folder. Only one of the folder's
    /// profiles that the look reader takes is chosen; whether it was.
    pub fn choose_look(&mut self, path: &str) -> bool {
        let usable = self
            .looks
            .iter()
            .any(|choice| choice.path == path && choice.read.is_ok());
        if usable {
            self.change(|edit| edit.rendering_profile = path.to_owned());
        }
        usable
    }

    /// How many clients Play starts on this machine: what the manifest names, and one when
    /// it names none. It is the file's number, not one that is being changed and is not
    /// saved yet. Without a manifest that is read it is one.
    pub fn local_clients(&self) -> u8 {
        self.config()
            .and_then(|config| config.planned.multiplayer.clients)
            .unwrap_or(1)
    }

    /// Sets how many clients Play starts on this machine, one to four; whether it was taken.
    /// A manifest that names no number is left naming none when the number is the one that
    /// means: looking at the setting, or choosing what it is already, changes nothing.
    pub fn set_local_clients(&mut self, clients: u8) -> bool {
        let range = project_spec::MIN_LOCAL_CLIENTS..=project_spec::MAX_LOCAL_CLIENTS;
        let Some(named) = self
            .config()
            .map(|config| config.planned.multiplayer.clients)
            .filter(|_| range.contains(&clients))
        else {
            return false;
        };
        let unnamed = named.is_none() && clients == 1;
        self.change(|edit| edit.local_clients = (!unnamed).then_some(clients));
        true
    }

    /// Puts the settings back to the manifest's. Nothing is written.
    pub fn discard_settings(&mut self) {
        self.draft = None;
        self.asking = None;
        self.refused_key = false;
        self.saved = false;
        self.problem = None;
    }

    /// Saves the settings as they were changed, on the person's word. Whether they are the
    /// manifest's now; if not, the problem says why, the manifest is as it was and the
    /// changes are still held.
    pub fn save_settings(&mut self) -> bool {
        // Only settings the game can play with are handed on: what is wrong with any other
        // is said where they are changed.
        if self.draft_state() != DraftState::Changed {
            return false;
        }
        let (Some(root), Some(config), Some(draft)) = (
            self.root.clone(),
            self.config().cloned(),
            self.draft.clone(),
        ) else {
            return false;
        };
        self.asking = None;
        self.refused_key = false;
        match project::save_settings(&root, &config, &draft) {
            Ok(_) => {
                self.problem = None;
                self.draft = None;
                self.read();
                self.saved = true;
                self.saves += 1;
                true
            }
            Err(refusal) => {
                self.problem = Some(refusal);
                self.saved = false;
                false
            }
        }
    }

    /// Reads the manifest again after a save was refused because it had changed: what is
    /// shown is the file as it is now, with the changes made here still on it and unsaved.
    pub fn read_again(&mut self) {
        self.problem = None;
        self.read();
    }

    /// Changes the draft, which begins as the manifest's settings.
    fn change(&mut self, change: impl FnOnce(&mut SettingsEdit)) {
        let Some(mut edit) = self.settings() else {
            return;
        };
        change(&mut edit);
        self.draft = Some(edit);
        self.saved = false;
        self.settle();
    }

    /// A draft that is the manifest's settings again is no draft.
    fn settle(&mut self) {
        if let (Some(config), Some(draft)) = (self.config(), &self.draft)
            && *draft == SettingsEdit::of(config)
        {
            self.draft = None;
        }
    }
}

/// The keys the game uses now, each as what it does and the key, in the manifest's order.
pub fn control_rows(config: &ProjectConfig) -> Vec<(&'static str, String)> {
    let controls = &config.controls;
    let look = match (controls.look.mouse, controls.look.gamepad_right_stick) {
        (true, true) => "Mouse, gamepad right stick",
        (true, false) => "Mouse",
        (false, true) => "Gamepad right stick",
        (false, false) => "Nothing",
    };
    vec![
        ("Move forward", controls.move_forward.clone()),
        ("Move backward", controls.move_backward.clone()),
        ("Move left", controls.move_left.clone()),
        ("Move right", controls.move_right.clone()),
        ("Jump", controls.jump.clone()),
        ("Change view", controls.toggle_view.clone()),
        ("Release cursor", controls.release_cursor.clone()),
        ("Look", look.to_owned()),
    ]
}

/// What a planned setting shows while it has no value.
pub const NOT_SET: &str = "Not set";

/// The name of the group of settings the number of local test clients is in.
pub const MULTIPLAYER: &str = "Multiplayer";

/// The settings the manifest has a place for and the game does not read yet: two groups,
/// each row what it is and what the manifest holds for it. The number of local test clients
/// is not among them: it is read, and set in the dialog.
pub fn planned_rows(config: &ProjectConfig) -> [(&'static str, Vec<(&'static str, String)>); 2] {
    fn shown<T: ToString>(value: &Option<T>) -> String {
        value
            .as_ref()
            .map_or_else(|| NOT_SET.to_owned(), ToString::to_string)
    }
    let (multiplayer, gameplay) = (&config.planned.multiplayer, &config.planned.gameplay);
    [
        (
            MULTIPLAYER,
            vec![
                ("Bots", shown(&multiplayer.bots)),
                ("Network conditions", shown(&multiplayer.network_conditions)),
                ("Start at", shown(&multiplayer.start_at)),
                ("Zone", shown(&multiplayer.zone)),
            ],
        ),
        (
            "Gameplay",
            vec![
                ("Mode", shown(&gameplay.mode)),
                ("Teams", shown(&gameplay.team_count)),
                ("Respawn (s)", shown(&gameplay.respawn_seconds)),
                ("Weapon set", shown(&gameplay.weapon_set)),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A manifest of the first version, as an older editor wrote it.
    fn old_manifest(root: &Path, start_level: &str) -> Vec<u8> {
        let config = ProjectConfig::new("Old Project");
        let old = project_spec::ProjectConfigV1 {
            schema: 1,
            project: config.project,
            startup: project_spec::StartupSettings {
                start_level: start_level.into(),
            },
            rendering: config.rendering,
            controls: config.controls,
        };
        let bytes = serde_json::to_vec_pretty(&old).unwrap();
        std::fs::write(project::project_path(root), &bytes).unwrap();
        bytes
    }

    #[test]
    fn without_a_folder_there_is_no_project_and_nothing_to_set_up() {
        let mut project = Project::of_folder(None);
        assert_eq!(*project.state(), ProjectState::NoFolder);
        assert!(!project.wants_attention());
        assert_eq!(project.set_up("Anything", false), None);
        assert!(!project.convert());
        assert_eq!(project.manifest_path(), None);
        assert_eq!(project.default_name(), "Arc project");
    }

    #[test]
    fn a_folder_without_a_manifest_is_set_up_on_the_persons_word_with_a_starter_level() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("my-game");
        std::fs::create_dir_all(&root).unwrap();
        let mut project = Project::of_folder(Some(root.clone()));
        assert_eq!(*project.state(), ProjectState::NotSetUp);
        assert!(project.wants_attention());
        assert_eq!(project.default_name(), "my-game");
        // Looking wrote nothing.
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);

        let level = project.set_up("  My Game  ", false).unwrap();
        assert_eq!(level, root.join("levels/first-level.level.json"));
        assert!(level.is_file());
        let config = project.config().unwrap();
        assert_eq!(config.project.name, "My Game");
        assert_eq!(config.startup.start_level, "levels/first-level.level.json");
        assert_eq!(project.start_level(), Some(level));
        assert_eq!(
            project.last_set_up(),
            Some(&SetUp {
                start_level: "levels/first-level.level.json".into(),
                made_level: true
            })
        );
        assert!(!project.wants_attention() && project.problem().is_none());
        // The look profile the project file names was made with it.
        assert!(root.join("assets/render/look-v1.json").is_file());
        assert!(!project.look_missing());
        assert!(project.look_profile().unwrap().1.is_ok());
        // Another editor on the folder finds the project, and there is nothing to set up.
        let mut again = Project::of_folder(Some(root));
        assert_eq!(again.state().word(), "Ready");
        assert_eq!(again.set_up("Another", false), None);
        assert_eq!(again.config().unwrap().project.name, "My Game");
    }

    #[test]
    fn what_stops_play_is_found_when_the_project_is_read_and_goes_when_it_is_mended() {
        let code = |project: &Project| project.play_blocker().map(|blocker| blocker.code.clone());
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("my-game");
        std::fs::create_dir_all(&root).unwrap();
        // Without a folder there is no project to answer for.
        assert_eq!(code(&Project::of_folder(None)), None);
        // A folder that is no project yet is not played; set up, it has all the game opens.
        let mut project = Project::of_folder(Some(root.clone()));
        assert_eq!(
            code(&project).as_deref(),
            Some("play.project_manifest_unavailable")
        );
        let level = project.set_up("My Game", false).unwrap();
        assert_eq!(code(&project), None);

        // The look profile goes. Nothing is read while nothing is said to have changed;
        // read again, the project says so, and making the profile mends it at once.
        let look = root.join("assets/render/look-v1.json");
        std::fs::remove_file(&look).unwrap();
        assert_eq!(code(&project), None);
        project.read_again();
        let blocker = project.play_blocker().unwrap();
        assert_eq!(blocker.code, "play.project_profile_unavailable");
        assert_eq!(
            blocker.parameters.get("cause").map(String::as_str),
            Some("look.missing")
        );
        assert_eq!(
            blocker.parameters.get("profile").map(String::as_str),
            Some("assets/render/look-v1.json")
        );
        assert!(project.install_look());
        assert_eq!(code(&project), None);

        // A file under the profile's name that is no profile.
        let profile = std::fs::read(&look).unwrap();
        std::fs::write(&look, b"not a look profile").unwrap();
        project.check_play();
        assert_eq!(
            project
                .play_blocker()
                .and_then(|blocker| blocker.parameters.get("cause"))
                .map(String::as_str),
            Some("look.decode")
        );
        std::fs::write(&look, profile).unwrap();
        project.check_play();
        assert_eq!(code(&project), None);

        // The level the project starts with goes, is no level, and is put back.
        let saved = std::fs::read(&level).unwrap();
        std::fs::remove_file(&level).unwrap();
        project.check_play();
        assert_eq!(
            code(&project).as_deref(),
            Some("play.project_start_level_missing")
        );
        std::fs::write(&level, b"not a level").unwrap();
        project.check_play();
        assert_eq!(code(&project).as_deref(), Some("source.decode"));
        std::fs::write(&level, saved).unwrap();
        project.check_play();
        assert_eq!(code(&project), None);
    }

    #[test]
    fn setup_takes_a_level_that_is_there_and_leaves_it_as_it_is() {
        let folder = tempfile::tempdir().unwrap();
        let levels = folder.path().join("levels");
        std::fs::create_dir_all(&levels).unwrap();
        let owner = levels.join("owner.level.json");
        let bytes = serde_json::to_vec_pretty(&scene_data::current_map()).unwrap();
        std::fs::write(&owner, &bytes).unwrap();

        let mut project = Project::of_folder(Some(folder.path().into()));
        assert_eq!(project.set_up("Game", false), Some(owner.clone()));
        assert_eq!(std::fs::read(&owner).unwrap(), bytes);
        assert_eq!(
            project.last_set_up(),
            Some(&SetUp {
                start_level: "levels/owner.level.json".into(),
                made_level: false
            })
        );
        // A name the format refuses is the problem, and nothing is set up.
        let other = tempfile::tempdir().unwrap();
        let mut nameless = Project::of_folder(Some(other.path().into()));
        assert_eq!(nameless.set_up("   ", false), None);
        assert_eq!(nameless.problem().unwrap().code, "project.value");
        assert_eq!(*nameless.state(), ProjectState::NotSetUp);
        assert_eq!(
            std::fs::read_dir(other.path()).unwrap().count(),
            0,
            "and nothing was made"
        );
    }

    #[test]
    fn an_older_manifest_is_never_changed_by_looking_and_is_converted_with_a_backup_on_request() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        let old = old_manifest(root, "levels/legacy.level.json");
        let manifest = project::project_path(root);

        let mut project = Project::of_folder(Some(root.into()));
        assert_eq!(project.state().word(), "NeedsConversion");
        assert!(project.wants_attention());
        assert!(project.config().is_none() && project.start_level().is_none());
        // Looking, and looking again, changed nothing and made no backup.
        project.read();
        assert_eq!(std::fs::read(&manifest).unwrap(), old);
        assert_eq!(std::fs::read_dir(root).unwrap().count(), 1);
        // Nor is it "set up" over: that is for a folder without a manifest.
        assert_eq!(project.set_up("New", false), None);
        assert_eq!(std::fs::read(&manifest).unwrap(), old);

        assert!(project.convert());
        let backup = root.join("arc-project.json.schema-1.bak");
        assert_eq!(std::fs::read(&backup).unwrap(), old);
        let config = project.config().unwrap();
        assert_eq!(config.schema, project_spec::SCHEMA);
        assert_eq!(config.project.name, "Old Project");
        assert_eq!(config.startup.start_level, "levels/legacy.level.json");
        // The level it names is not there: the project is read, and says so.
        assert_eq!(
            *project.state(),
            ProjectState::Ready {
                config: Box::new(config.clone()),
                start_level_there: false
            }
        );
        assert_eq!(project.start_level(), None);
        // That is the person's to put right, so the project's panel is shown unasked. The
        // level is not made by looking, by converting or by reading again.
        assert!(project.wants_attention());
        project.read();
        assert!(!root.join("levels/legacy.level.json").exists());
        // With the level in the folder the project needs nothing more.
        std::fs::create_dir_all(root.join("levels")).unwrap();
        std::fs::write(root.join("levels/legacy.level.json"), b"{}").unwrap();
        project.read();
        assert!(project.start_level().is_some() && !project.wants_attention());
        assert!(!project.convert(), "there is nothing left to convert");
    }

    #[test]
    fn a_conversion_that_cannot_keep_a_backup_changes_nothing_and_says_why() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        let old = old_manifest(root, "levels/legacy.level.json");
        // A backup of that name is there already: it is not written over.
        std::fs::write(root.join("arc-project.json.schema-1.bak"), b"earlier").unwrap();
        let mut project = Project::of_folder(Some(root.into()));
        assert!(!project.convert());
        assert_eq!(project.problem().unwrap().code, "project.conflict");
        assert_eq!(project.state().word(), "NeedsConversion");
        assert_eq!(std::fs::read(project::project_path(root)).unwrap(), old);
        assert_eq!(
            std::fs::read(root.join("arc-project.json.schema-1.bak")).unwrap(),
            b"earlier"
        );
    }

    #[test]
    fn a_manifest_that_is_no_project_file_is_refused_and_not_offered_for_conversion() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(project::project_path(folder.path()), b"{").unwrap();
        let mut project = Project::of_folder(Some(folder.path().into()));
        let ProjectState::Refused(refusal) = project.state().clone() else {
            panic!("refused, not {:?}", project.state());
        };
        assert_eq!(refusal.code, "project.decode");
        assert!(
            project.wants_attention(),
            "it is shown, and not for conversion"
        );
        assert!(!project.convert());
        assert_eq!(project.set_up("New", false), None);
    }

    #[test]
    fn a_newer_manifest_is_refused_as_that_and_no_conversion_is_offered() {
        let folder = tempfile::tempdir().unwrap();
        let mut newer = serde_json::to_value(ProjectConfig::new("Newer")).unwrap();
        newer["schema"] = (project_spec::SCHEMA + 1).into();
        let bytes = serde_json::to_vec_pretty(&newer).unwrap();
        std::fs::write(project::project_path(folder.path()), &bytes).unwrap();

        let mut project = Project::of_folder(Some(folder.path().into()));
        let ProjectState::Refused(refusal) = project.state().clone() else {
            panic!("refused, not {:?}", project.state());
        };
        assert_eq!(refusal.code, "project.schema_unsupported");
        assert!(project.wants_attention());
        // It is not one to convert: asking changes nothing and makes no backup.
        assert!(!project.convert());
        assert_eq!(
            std::fs::read(project::project_path(folder.path())).unwrap(),
            bytes
        );
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
    }

    #[test]
    fn level_files_that_cannot_be_read_are_shown_and_a_fresh_starter_is_the_persons_choice() {
        let folder = tempfile::tempdir().unwrap();
        let levels = folder.path().join("levels");
        std::fs::create_dir_all(&levels).unwrap();
        let old = levels.join("current-map.level.json");
        std::fs::write(&old, b"not a level").unwrap();

        let mut project = Project::of_folder(Some(folder.path().into()));
        assert_eq!(*project.state(), ProjectState::NotSetUp);
        let plan = project.setup_plan().unwrap().clone();
        assert!(plan.needs_agreement());
        assert_eq!(plan.rejected.len(), 1);
        assert_eq!(plan.rejected[0].path, old);
        assert_eq!(plan.rejected[0].code, "source.decode");

        // Not agreed to: refused, said, and nothing made.
        assert_eq!(project.set_up("Game", false), None);
        assert_eq!(project.problem().unwrap().code, "project.levels_rejected");
        assert_eq!(*project.state(), ProjectState::NotSetUp);
        assert_eq!(std::fs::read_dir(&levels).unwrap().count(), 1);

        // Agreed to: the starter beside it, and the old file as it was.
        let level = project.set_up("Game", true).unwrap();
        assert_eq!(level, plan.starter);
        assert_eq!(std::fs::read(&old).unwrap(), b"not a level");
        assert_eq!(project.state().word(), "Ready");
        assert!(project.setup_plan().is_none() && project.problem().is_none());
    }

    /// A look profile the look reader takes.
    const LOOK: &str = include_str!("../../../apps/arc-editor/checks/look-v1.json");

    /// A folder set up as a project, with the look profile its manifest names.
    fn ready() -> (tempfile::TempDir, Project) {
        let folder = tempfile::tempdir().unwrap();
        let render = folder.path().join("assets/render");
        std::fs::create_dir_all(&render).unwrap();
        std::fs::write(render.join("look-v1.json"), LOOK).unwrap();
        let mut project = Project::of_folder(Some(folder.path().into()));
        project.set_up("Game", false).unwrap();
        (folder, project)
    }

    fn manifest(project: &Project) -> Vec<u8> {
        std::fs::read(project.manifest_path().unwrap()).unwrap()
    }

    #[test]
    fn every_key_a_binding_can_name_is_a_key_of_the_keyboard_and_no_other_is() {
        let names: Vec<&str> = KEYS.iter().map(|(_, name)| *name).collect();
        assert_eq!(names, project_spec::KEY_NAMES);
        for (key, name) in KEYS {
            assert_eq!(key_name(key), Some(name));
        }
        for other in [
            KeyCode::ShiftLeft,
            KeyCode::F5,
            KeyCode::Numpad1,
            KeyCode::Minus,
        ] {
            assert_eq!(key_name(other), None);
        }
        // Each action is a field of the manifest, and the other way round.
        let fields = ProjectConfig::new("Game")
            .controls
            .keys()
            .map(|(field, _)| field);
        assert_eq!(KeyAction::ALL.map(KeyAction::field), fields);
        for action in KeyAction::ALL {
            assert_eq!(KeyAction::of_field(action.field()), Some(action));
        }
        assert_eq!(KeyAction::of_field("controls.look"), None);
    }

    #[test]
    fn changing_a_key_writes_nothing_until_it_is_saved() {
        let (_folder, mut project) = ready();
        let before = manifest(&project);
        assert_eq!(project.draft_state(), DraftState::Unchanged);
        assert!(!project.settings_unsaved() && !project.settings_saved());
        assert_eq!(project.settings_saves(), 0);

        // A key the game does not read is not taken, and the question stays open.
        project.ask_key(KeyAction::Jump);
        assert_eq!(project.asking(), Some(KeyAction::Jump));
        assert!(!project.press_key(KeyCode::ShiftLeft));
        assert!(project.refused_key());
        assert_eq!(project.asking(), Some(KeyAction::Jump));
        assert_eq!(project.draft_state(), DraftState::Unchanged);

        assert!(project.press_key(KeyCode::Enter));
        assert_eq!(project.asking(), None);
        assert!(!project.refused_key());
        assert_eq!(project.settings().unwrap().controls.jump, "Enter");
        assert_eq!(project.draft_state(), DraftState::Changed);
        project.set_look(None, Some(false));
        // The manifest the editor read, and the file, are as they were.
        assert_eq!(project.config().unwrap().controls.jump, "Space");
        assert_eq!(manifest(&project), before);
        assert_eq!(project.settings_saves(), 0);

        assert!(project.save_settings());
        assert!(project.settings_saved() && !project.settings_unsaved());
        assert_eq!(project.settings_saves(), 1);
        assert_eq!(project.draft_state(), DraftState::Unchanged);
        assert_eq!(project.problem(), None);
        let saved = project.config().unwrap();
        assert_eq!(saved.controls.jump, "Enter");
        assert!(saved.controls.look.mouse && !saved.controls.look.gamepad_right_stick);
        // Another editor on the folder reads the same.
        let root = project.root().unwrap().to_path_buf();
        let again = Project::of_folder(Some(root));
        assert_eq!(again.config(), project.config());
        // A key that is pressed while none is asked for is nobody's.
        assert!(!project.press_key(KeyCode::KeyQ));
        assert_eq!(project.draft_state(), DraftState::Unchanged);
    }

    #[test]
    fn two_actions_on_one_key_are_held_and_not_saved_until_one_has_another() {
        let (_folder, mut project) = ready();
        let before = manifest(&project);
        // Jump goes onto the key of moving forward: on the way to exchanging the two.
        project.ask_key(KeyAction::Jump);
        project.press_key(KeyCode::KeyW);
        assert_eq!(
            project.draft_state(),
            DraftState::SharedKey {
                first: KeyAction::MoveForward,
                second: KeyAction::Jump
            }
        );
        assert!(project.settings_unsaved());
        assert!(!project.save_settings());
        assert_eq!(manifest(&project), before);
        assert_eq!(project.problem(), None);

        project.ask_key(KeyAction::MoveForward);
        project.press_key(KeyCode::Space);
        assert_eq!(project.draft_state(), DraftState::Changed);
        assert!(project.save_settings());
        let controls = &project.config().unwrap().controls;
        assert_eq!(
            (controls.move_forward.as_str(), controls.jump.as_str()),
            ("Space", "W")
        );
    }

    #[test]
    fn settings_put_back_or_discarded_are_the_files_again() {
        let (_folder, mut project) = ready();
        let before = manifest(&project);
        project.ask_key(KeyAction::Jump);
        project.press_key(KeyCode::Enter);
        // Asking for the key again and pressing the one it had: nothing is changed.
        project.ask_key(KeyAction::Jump);
        project.press_key(KeyCode::Space);
        assert_eq!(project.draft_state(), DraftState::Unchanged);
        assert!(!project.settings_unsaved());

        project.set_look(Some(false), Some(false));
        assert_eq!(project.draft_state(), DraftState::Changed);
        project.ask_key(KeyAction::ToggleView);
        project.discard_settings();
        assert_eq!(project.draft_state(), DraftState::Unchanged);
        assert_eq!(project.asking(), None);
        assert!(project.settings().unwrap().controls.look.mouse);
        // Pressing the button of the key that is asked for stops asking.
        project.ask_key(KeyAction::Jump);
        project.ask_key(KeyAction::Jump);
        assert_eq!(project.asking(), None);
        assert_eq!(manifest(&project), before);
    }

    #[test]
    fn a_look_profile_is_chosen_among_the_folders_that_can_be_read() {
        let (folder, mut project) = ready();
        let render = folder.path().join("assets/render");
        std::fs::write(render.join("dusk.json"), LOOK).unwrap();
        std::fs::write(render.join("broken.json"), "{").unwrap();
        project.read();
        let paths: Vec<&str> = project
            .look_choices()
            .iter()
            .map(|choice| choice.path.as_str())
            .collect();
        assert_eq!(
            paths,
            [
                "assets/render/broken.json",
                "assets/render/dusk.json",
                "assets/render/look-v1.json"
            ]
        );
        // What Game look is drawn from is the profile the manifest names.
        let (path, read) = project.look_profile().unwrap();
        assert_eq!(path, render.join("look-v1.json"));
        assert!(read.is_ok());

        assert!(!project.choose_look("assets/render/broken.json"));
        assert!(!project.choose_look("assets/render/none.json"));
        assert_eq!(project.draft_state(), DraftState::Unchanged);
        assert!(project.choose_look("assets/render/dusk.json"));
        assert_eq!(project.draft_state(), DraftState::Changed);
        assert!(project.save_settings());
        assert_eq!(
            project.config().unwrap().rendering.profile,
            "assets/render/dusk.json"
        );
        assert_eq!(project.look_profile().unwrap().0, render.join("dusk.json"));

        // A profile the manifest names and the folder does not have is shown as that.
        std::fs::remove_file(render.join("dusk.json")).unwrap();
        project.read();
        let named = &project.look_choices()[0];
        assert_eq!(named.path, "assets/render/dusk.json");
        assert_eq!(named.read.as_ref().unwrap_err().code, "look.missing");
        assert!(project.look_profile().unwrap().1.is_err());

        // It is made on the person's word, from the default, where the manifest names it;
        // the manifest is not written for it.
        assert!(project.look_missing());
        let manifest = std::fs::read(folder.path().join("arc-project.json")).unwrap();
        assert_eq!(project.looks_made(), 0);
        assert!(project.install_look());
        assert!(!project.look_missing() && project.problem().is_none());
        assert_eq!(project.looks_made(), 1);
        assert_eq!(
            std::fs::read(render.join("dusk.json")).unwrap(),
            project::DEFAULT_LOOK_PROFILE
        );
        assert!(project.look_profile().unwrap().1.is_ok());
        assert_eq!(
            std::fs::read(folder.path().join("arc-project.json")).unwrap(),
            manifest
        );
        // Once it is there nothing is made, and a file that cannot be read is the
        // person's: it is not missing, and is not replaced.
        assert!(!project.install_look());
        std::fs::write(render.join("dusk.json"), "{").unwrap();
        project.read();
        assert!(!project.look_missing() && !project.install_look());
        assert_eq!(std::fs::read(render.join("dusk.json")).unwrap(), b"{");
        assert_eq!(project.looks_made(), 1);
        // Without a manifest there is no profile of a project.
        assert!(Project::of_folder(None).look_profile().is_none());
    }

    #[test]
    fn a_save_over_a_manifest_another_program_changed_is_refused_and_the_changes_are_kept() {
        let (_folder, mut project) = ready();
        project.ask_key(KeyAction::Jump);
        project.press_key(KeyCode::Enter);
        // Another program renames the project meanwhile.
        let mut theirs = project.config().unwrap().clone();
        theirs.project.name = "Renamed".into();
        let their_bytes = project_spec::canonical_bytes(&theirs).unwrap();
        std::fs::write(project.manifest_path().unwrap(), &their_bytes).unwrap();

        assert!(!project.save_settings());
        assert_eq!(
            project.problem().unwrap().code,
            "project.changed_since_read"
        );
        assert_eq!(manifest(&project), their_bytes);
        assert_eq!(project.settings().unwrap().controls.jump, "Enter");
        // A save that was refused is none: nothing reads the manifest again for it.
        assert!(!project.settings_saved());
        assert_eq!(project.settings_saves(), 0);

        // Read again: their name is shown, the change made here is still unsaved.
        project.read_again();
        assert_eq!(project.problem(), None);
        assert_eq!(project.config().unwrap().project.name, "Renamed");
        assert_eq!(project.draft_state(), DraftState::Changed);
        assert!(project.save_settings());
        let saved = project.config().unwrap();
        assert_eq!(
            (saved.project.name.as_str(), saved.controls.jump.as_str()),
            ("Renamed", "Enter")
        );
    }

    #[test]
    fn the_keys_are_shown_as_the_manifest_has_them_and_planned_settings_as_not_set() {
        let config = ProjectConfig::new("Game");
        let keys = control_rows(&config);
        assert_eq!(keys[0], ("Move forward", "W".to_owned()));
        assert_eq!(keys[4], ("Jump", "Space".to_owned()));
        assert_eq!(keys[7], ("Look", "Mouse, gamepad right stick".to_owned()));
        assert_eq!(keys.len(), 8);

        let [(multiplayer, play), (gameplay, rules)] = planned_rows(&config);
        assert_eq!((multiplayer, gameplay), ("Multiplayer", "Gameplay"));
        assert_eq!(play.len() + rules.len(), 8);
        assert!(play.iter().chain(&rules).all(|(_, value)| value == NOT_SET));
        // The number of local test clients is read, and so is none of the planned.
        assert!(play.iter().all(|(name, _)| !name.contains("lients")));
        // A value the manifest holds is shown, and is still not one the game reads.
        let mut with_bots = config;
        with_bots.planned.multiplayer.bots = Some("idle".into());
        assert_eq!(
            planned_rows(&with_bots)[0].1[0],
            ("Bots", "idle".to_owned())
        );
    }

    #[test]
    fn the_local_test_clients_are_set_saved_and_read_again_and_none_stays_none() {
        let clients = |project: &Project| project.config().unwrap().planned.multiplayer.clients;
        let shown = |project: &Project| project.settings().unwrap().local_clients;
        let (folder, mut project) = ready();
        let before = manifest(&project);
        // A project that names no number starts one client.
        assert_eq!((clients(&project), project.local_clients()), (None, 1));

        // Choosing the one it has is no change; neither is a number out of range.
        assert!(project.set_local_clients(1));
        assert!(!project.set_local_clients(0) && !project.set_local_clients(5));
        assert_eq!(project.draft_state(), DraftState::Unchanged);
        assert_eq!(shown(&project), None);

        // A number is held and not written; Play is still started with the file's.
        assert!(project.set_local_clients(3));
        assert_eq!(project.draft_state(), DraftState::Changed);
        assert_eq!((shown(&project), project.local_clients()), (Some(3), 1));
        assert_eq!(manifest(&project), before);
        // Back to one, the manifest names none again and there is nothing to save.
        assert!(project.set_local_clients(1));
        assert_eq!((shown(&project), project.settings_unsaved()), (None, false));
        // Discarded, it is the file's.
        project.set_local_clients(4);
        project.discard_settings();
        assert_eq!((shown(&project), project.local_clients()), (None, 1));
        assert_eq!(manifest(&project), before);

        // Saved, it is the file's number and what Play is started with.
        project.set_local_clients(3);
        assert!(project.save_settings());
        assert_eq!((clients(&project), project.local_clients()), (Some(3), 3));
        assert!(project.settings_saved() && !project.settings_unsaved());
        // A save of something else keeps it, and so does reading the folder anew.
        project.ask_key(KeyAction::Jump);
        project.press_key(KeyCode::Enter);
        assert!(project.save_settings());
        let again = Project::of_folder(Some(folder.path().to_path_buf()));
        assert_eq!((clients(&again), again.local_clients()), (Some(3), 3));
        assert_eq!(again.config().unwrap().controls.jump, "Enter");

        // A number that is named stays named: one is written as one.
        assert!(project.set_local_clients(1));
        assert_eq!(shown(&project), Some(1));
        assert!(project.save_settings());
        assert_eq!((clients(&project), project.local_clients()), (Some(1), 1));

        // Without a project there is one client, and nothing to set.
        let mut none = Project::of_folder(None);
        assert_eq!(none.local_clients(), 1);
        assert!(!none.set_local_clients(2));
    }
}
