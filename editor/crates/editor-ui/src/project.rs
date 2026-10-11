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

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use editor_services::{
    project::{self, FreshStarter, ProjectSetup},
    project_spec,
};

pub use editor_services::project::{RejectedLevel, SetupPlan};
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
}

impl Project {
    /// The project of this folder as it is on disk now. Nothing is written.
    pub fn of_folder(root: Option<PathBuf>) -> Self {
        let mut project = Self { root, ..default() };
        project.read();
        project
    }

    /// Reads the manifest again. Nothing is written, whatever is found.
    pub fn read(&mut self) {
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
    /// used.
    pub fn wants_attention(&self) -> bool {
        matches!(
            self.state,
            ProjectState::NotSetUp
                | ProjectState::NeedsConversion { .. }
                | ProjectState::Refused(_)
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

/// The settings the manifest has a place for and the game does not read yet: two groups,
/// each row what it is and what the manifest holds for it.
pub fn planned_rows(config: &ProjectConfig) -> [(&'static str, Vec<(&'static str, String)>); 2] {
    fn shown<T: ToString>(value: &Option<T>) -> String {
        value
            .as_ref()
            .map_or_else(|| NOT_SET.to_owned(), ToString::to_string)
    }
    let (multiplayer, gameplay) = (&config.planned.multiplayer, &config.planned.gameplay);
    [
        (
            "Multiplayer",
            vec![
                ("Clients", shown(&multiplayer.clients)),
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
        // Another editor on the folder finds the project, and there is nothing to set up.
        let mut again = Project::of_folder(Some(root));
        assert_eq!(again.state().word(), "Ready");
        assert_eq!(again.set_up("Another", false), None);
        assert_eq!(again.config().unwrap().project.name, "My Game");
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
        assert_eq!(play.len() + rules.len(), 9);
        assert!(play.iter().chain(&rules).all(|(_, value)| value == NOT_SET));
        // A value the manifest holds is shown, and is still not one the game reads.
        let mut with_clients = config;
        with_clients.planned.multiplayer.clients = Some(2);
        assert_eq!(
            planned_rows(&with_clients)[0].1[0],
            ("Clients", "2".to_owned())
        );
    }
}
