//! The project browser: what the editor shows first when it is started without a project.
//!
//! A person who starts the editor names no folder on a command line. The window opens on
//! the projects that were opened before, most recent first, with two ways to another one:
//! opening the folder of a project, and making a new project. Folders are chosen in the
//! system's own folder chooser, which the application shows when this asks for it
//! ([`Launcher::wants_folder`]) and answers ([`Launcher::folder_chosen`]).
//!
//! A new project is made in a folder of its own. The folder that is chosen for it is where
//! that folder goes: the project is named here, its folder is named after it, and it is
//! made only where nothing has that name. The chosen folder is given nothing else, so a new
//! project can be made in a folder that holds other things.
//!
//! The editor is entered only on a project whose file is read: a folder that was opened and
//! is no project yet is set up here first, on the person's word and under a name, and a
//! project file of an older version is converted here first, with the file as it was kept
//! beside it. A folder that is not there, or whose project file is refused, stays in the
//! browser and says why. Nothing is written by looking at a folder.
//!
//! A project is content: its file, its levels, its look profile. The game that plays it is
//! not part of it. Where the game is, is one setting of the editor for this person and
//! every project ([`Launcher::game`]), chosen in the same folder chooser and kept beside
//! the list of projects; a project that is itself the game's folder needs none.
//!
//! A project whose start level is not in its folder is opened all the same, and says so
//! before and after: its row says that the level is missing and that opening uses an
//! unsaved map, and the Project settings, opened with it, say how to put it right. The
//! level's file is not made by opening; saving is the person's to do.
//!
//! What is to be entered is handed on as the services' account of the project
//! ([`Launcher::take_opening`]): its folder, the level it starts with, where its cooked
//! levels go and whether the game is built there. The browser is then put away, in the same
//! window, and the project is remembered in the list.
//!
//! With a folder, a level or a package named on the command line, or in a scripted check, the
//! browser is off ([`Launcher::off`]) and the editor starts as it always did.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use editor_services::{
    project::{self, ProjectInspectionStatus, ProjectStartupContext, RecentProject, SetupPlan},
    runtime::{self, RuntimeLocation, RuntimeLocationStatus},
};
use scene_data::Diagnostic;

use crate::project::{Project, ProjectState};

/// What a folder is chosen for in the system's folder chooser.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FolderFor {
    /// The folder of a project that is to be opened.
    Open,
    /// The folder a new project's own folder is made in.
    New,
    /// The folder of the game that plays every project: the editor's one setting for it.
    Game,
}

/// The name a new project is offered.
pub const NEW_PROJECT_NAME: &str = "New Arc Project";

/// A project of the list: what was remembered of it, and what its folder is now.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub recent: RecentProject,
    pub status: ProjectInspectionStatus,
}

impl Row {
    /// Whether choosing the row leads anywhere: to the editor, or to the setup or the
    /// conversion that comes before it. A folder that is gone and a project file that is
    /// refused lead nowhere, and say so.
    pub fn can_open(&self) -> bool {
        !matches!(
            self.status,
            ProjectInspectionStatus::Missing | ProjectInspectionStatus::Refused { .. }
        )
    }
}

/// A folder that needs the person's word before the editor is entered on it.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    /// A new project is made in a folder of its own inside this one, under a name.
    New {
        /// The folder that was chosen: where the project's folder goes.
        parent: PathBuf,
        /// The name that is offered.
        name: String,
    },
    /// The folder was opened and is no project yet: it is set up under a name.
    SetUp {
        folder: PathBuf,
        /// The name that is offered: the folder's.
        name: String,
        /// What a setup would do in the folder.
        plan: SetupPlan,
    },
    /// The folder's project file is of an older version: it is converted.
    Convert {
        folder: PathBuf,
        /// What the reader said of the file.
        detail: String,
    },
}

impl Pending {
    /// The folder it is about: the one that was chosen.
    pub fn folder(&self) -> &Path {
        match self {
            Self::New { parent: folder, .. }
            | Self::SetUp { folder, .. }
            | Self::Convert { folder, .. } => folder,
        }
    }
}

/// Why the folder that was last chosen was not entered.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// The folder is not there.
    Missing(PathBuf),
    /// A new project was asked for inside a folder that is a project already.
    AlreadyProject(PathBuf),
    /// The services refused the folder, its project file, a setup or a conversion.
    Refused(PathBuf, Diagnostic),
}

/// The file the list of projects is kept in for this person, in the place the system has
/// for an application's own settings. `None` where it has none.
pub fn default_list() -> Option<PathBuf> {
    project::default_recents_path()
}

/// The file the game's folder is kept in for this person, beside the list of projects.
/// `None` where the system has no place for it.
pub fn default_game_file() -> Option<PathBuf> {
    runtime::default_runtime_preference_path()
}

/// The project browser's state.
#[derive(Resource, Default)]
pub struct Launcher {
    /// Whether the browser is what the window shows.
    shown: bool,
    /// The file the list is kept in. `None` where the system has no place for it: the
    /// browser then has no list, and remembers nothing.
    recents: Option<PathBuf>,
    rows: Vec<Row>,
    /// Why the list could not be read, or a project not be remembered.
    list_problem: Option<Diagnostic>,
    selected: Option<usize>,
    /// What the system's folder chooser is asked for and has not answered.
    asking: Option<FolderFor>,
    pending: Option<Pending>,
    refusal: Option<Refusal>,
    /// The project that was chosen and waits to be entered.
    opening: Option<ProjectStartupContext>,
    /// How many projects were entered from the browser since the editor started.
    entered: u64,
    /// The file the game's folder is kept in for this person. `None` where the system has
    /// no place for it, and in an editor without the browser.
    game_file: Option<PathBuf>,
    /// The game's folder as that file names it, and what the folder is now. `None` until it
    /// is read: in an editor without the browser it never is.
    game: Option<RuntimeLocation>,
    /// The folder that was last chosen for the game and refused, and why. The setting is
    /// then as it was.
    game_refusal: Option<(PathBuf, Diagnostic)>,
    /// How many times a folder was taken as the game's since the editor started.
    game_changes: u64,
}

impl Launcher {
    /// No browser: the editor was told on the command line what to open, or is checked by a
    /// script, and starts as it always did.
    pub fn off() -> Self {
        Self::default()
    }

    /// The browser as the editor's first screen, with the list that is kept in this file.
    pub fn at_start(recents: Option<PathBuf>) -> Self {
        let mut launcher = Self {
            shown: true,
            recents,
            ..default()
        };
        launcher.read();
        launcher.read_game();
        launcher
    }

    /// The same browser, with the game's folder kept in this file: the one setting of the
    /// editor for where the game is. `None` where the system has no place for it; the
    /// folder can then not be chosen, and only a project that is the game's folder is
    /// played.
    pub fn with_game_file(mut self, file: Option<PathBuf>) -> Self {
        self.game_file = file;
        self.read_game();
        self
    }

    /// Reads the game's folder again, and looks at it. Nothing is written.
    pub fn read_game(&mut self) {
        self.game = Some(match &self.game_file {
            Some(file) => runtime::runtime_location_at(file),
            None => RuntimeLocation {
                preference_path: None,
                status: RuntimeLocationStatus::PreferenceUnavailable,
            },
        });
    }

    /// The game's folder as the editor's setting names it, and what that folder is now.
    /// `None` in an editor without the browser.
    pub fn game(&self) -> Option<&RuntimeLocation> {
        self.game.as_ref()
    }

    /// The file the game's folder is kept in, for whoever asks the services about a
    /// project's game.
    pub fn game_file(&self) -> Option<&Path> {
        self.game_file.as_deref()
    }

    /// One word for what the editor's setting says of the game's folder, for the log and
    /// the checks: `off` in an editor without the browser, `nowhere` without a place to keep
    /// it, `unset`, `unreadable` for a setting that cannot be read, and for a folder that
    /// is set `ready`, `gone`, `incomplete` or `refused`.
    pub fn game_word(&self) -> &'static str {
        use runtime::RuntimeFolderStatus as Folder;
        match self.game.as_ref().map(|game| &game.status) {
            None => "off",
            Some(RuntimeLocationStatus::PreferenceUnavailable) => "nowhere",
            Some(RuntimeLocationStatus::Unconfigured) => "unset",
            Some(RuntimeLocationStatus::Refused { .. }) => "unreadable",
            Some(RuntimeLocationStatus::Configured { folder, .. }) => match folder.status {
                Folder::Ready { .. } => "ready",
                Folder::Missing => "gone",
                Folder::Incomplete { .. } => "incomplete",
                Folder::Refused { .. } => "refused",
            },
        }
    }

    /// The folder that was last chosen for the game and refused, and why.
    pub fn game_refusal(&self) -> Option<&(PathBuf, Diagnostic)> {
        self.game_refusal.as_ref()
    }

    /// How many times a folder was taken as the game's since the editor started. What an
    /// open project plays with is looked for again when this grows.
    pub fn game_changes(&self) -> u64 {
        self.game_changes
    }

    /// Takes this folder as the game's, for this person and every project, if it is one:
    /// the services look at it first and write nothing for a folder they refuse, which
    /// leaves the setting as it was and is said ([`Launcher::game_refusal`]). Whether it
    /// was taken.
    pub fn choose_game(&mut self, folder: &Path) -> bool {
        let Some(file) = self.game_file.clone() else {
            self.game_refusal = Some((
                folder.to_path_buf(),
                Diagnostic::new("runtime.preference_unavailable"),
            ));
            return false;
        };
        match runtime::save_runtime_preference(&file, folder) {
            Ok(_) => {
                self.game_refusal = None;
                self.game_changes += 1;
                self.read_game();
                true
            }
            Err(refusal) => {
                self.game_refusal = Some((folder.to_path_buf(), refusal));
                self.read_game();
                false
            }
        }
    }

    /// Reads the list again and looks at each of its folders. Nothing is written.
    pub fn read(&mut self) {
        self.rows.clear();
        self.list_problem = None;
        if let Some(path) = &self.recents {
            match project::load_recents(path) {
                Ok(recents) => self.take_list(recents),
                Err(refusal) => self.list_problem = Some(refusal),
            }
        }
        self.settle_selection();
    }

    fn take_list(&mut self, recents: Vec<RecentProject>) {
        self.rows = recents
            .into_iter()
            .map(|recent| {
                let status = project::inspect(&recent.root).status;
                Row { recent, status }
            })
            .collect();
    }

    /// The row that is chosen is one of the list, the first without another.
    fn settle_selection(&mut self) {
        self.selected = match (self.selected, self.rows.len()) {
            (_, 0) => None,
            (Some(index), rows) if index < rows => Some(index),
            _ => Some(0),
        };
    }

    /// Whether the browser is what the window shows.
    pub fn shown(&self) -> bool {
        self.shown
    }

    /// Whether the system has a place for the list. Without one there is none to show.
    pub fn has_list(&self) -> bool {
        self.recents.is_some()
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn list_problem(&self) -> Option<&Diagnostic> {
        self.list_problem.as_ref()
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn pending(&self) -> Option<&Pending> {
        self.pending.as_ref()
    }

    pub fn refusal(&self) -> Option<&Refusal> {
        self.refusal.as_ref()
    }

    /// How many projects were entered from the browser since the editor started. What is
    /// set up once for a project, as the wake-up of the editor for the game, is set up again
    /// when this grows.
    pub fn times_entered(&self) -> u64 {
        self.entered
    }

    /// Chooses a row of the list.
    pub fn select(&mut self, index: usize) {
        if index < self.rows.len() {
            self.selected = Some(index);
        }
    }

    /// Chooses the row above or below the chosen one, and stays at the list's ends.
    pub fn step_selection(&mut self, down: bool) {
        let Some(index) = self.selected else {
            return self.settle_selection();
        };
        let last = self.rows.len().saturating_sub(1);
        self.selected = Some(if down {
            (index + 1).min(last)
        } else {
            index.saturating_sub(1)
        });
    }

    /// Opens the project of a row. Whether it led anywhere.
    pub fn open_row(&mut self, index: usize) -> bool {
        let Some(row) = self.rows.get(index) else {
            return false;
        };
        let folder = row.recent.root.clone();
        self.selected = Some(index);
        self.open(&folder, FolderFor::Open)
    }

    /// Opens the project of the chosen row.
    pub fn open_selected(&mut self) -> bool {
        self.selected.is_some_and(|index| self.open_row(index))
    }

    /// Takes a row out of the list. The project's folder is not touched.
    pub fn forget_row(&mut self, index: usize) {
        let (Some(path), Some(row)) = (self.recents.clone(), self.rows.get(index)) else {
            return;
        };
        match project::forget_recent(&path, &row.recent.root) {
            Ok(recents) => {
                self.list_problem = None;
                self.take_list(recents);
            }
            Err(refusal) => self.list_problem = Some(refusal),
        }
        self.settle_selection();
    }

    /// Asks for a folder in the system's folder chooser. One question at a time. A project
    /// is asked for only in the browser; the game's folder can be asked for in the editor
    /// too, where the browser has one to keep it in.
    pub fn ask_folder(&mut self, why: FolderFor) {
        let asked = match why {
            FolderFor::Open | FolderFor::New => self.shown,
            FolderFor::Game => self.game.is_some(),
        };
        if asked && self.asking.is_none() && self.opening.is_none() {
            self.asking = Some(why);
        }
    }

    /// What the system's folder chooser is to be shown for, if it is asked for.
    pub fn wants_folder(&self) -> Option<FolderFor> {
        self.asking
    }

    /// The folder chooser's answer: the folder, or `None` when the person chose none. A
    /// folder that is chosen is looked at, and leads to the editor, to a setup or a
    /// conversion first, or to the reason why it leads nowhere.
    pub fn folder_chosen(&mut self, folder: Option<PathBuf>) {
        let Some(why) = self.asking.take() else {
            return;
        };
        match (folder, why) {
            (None, _) => {}
            (Some(folder), FolderFor::Game) => {
                self.choose_game(&folder);
            }
            (Some(folder), FolderFor::Open | FolderFor::New) => {
                self.open(&folder, why);
            }
        }
    }

    /// Looks at a folder and goes where it leads. Whether it led anywhere.
    fn open(&mut self, folder: &Path, why: FolderFor) -> bool {
        self.refusal = None;
        self.pending = None;
        let found = project::inspect(folder);
        let root = found.root;
        match found.status {
            ProjectInspectionStatus::Missing => self.refusal = Some(Refusal::Missing(root)),
            ProjectInspectionStatus::Refused { problem } => {
                self.refusal = Some(Refusal::Refused(root, problem));
            }
            // A new project gets a folder of its own inside the chosen one, which is not
            // looked into: whatever it holds is left alone.
            ProjectInspectionStatus::SetupRequired { .. } if why == FolderFor::New => {
                self.pending = Some(Pending::New {
                    parent: root,
                    name: NEW_PROJECT_NAME.to_owned(),
                });
            }
            ProjectInspectionStatus::SetupRequired { plan } => {
                let name = Project::of_folder(Some(root.clone())).default_name();
                self.pending = Some(Pending::SetUp {
                    folder: root,
                    name,
                    plan,
                });
            }
            // A new project is not made inside one that is there, and one that is there is
            // not opened in its place unasked.
            ProjectInspectionStatus::Ready { .. }
            | ProjectInspectionStatus::MissingStartLevel { .. }
            | ProjectInspectionStatus::ConversionRequired { .. }
                if why == FolderFor::New =>
            {
                self.refusal = Some(Refusal::AlreadyProject(root));
            }
            ProjectInspectionStatus::ConversionRequired { problem } => {
                self.pending = Some(Pending::Convert {
                    folder: root,
                    detail: problem
                        .parameters
                        .get("detail")
                        .cloned()
                        .unwrap_or_default(),
                });
            }
            // A project whose start level is not in the folder is entered too: on an
            // unsaved map, with the Project settings saying what is missing.
            ProjectInspectionStatus::Ready { .. }
            | ProjectInspectionStatus::MissingStartLevel { .. } => self.begin(&root),
        }
        self.refusal.is_none()
    }

    /// Asks the services for a ready project's account, which is what is entered: with
    /// the game that plays it, as the editor's setting or the project's own folder has it.
    fn begin(&mut self, root: &Path) {
        match project::startup_context_using(root, self.game_file.as_deref()) {
            Ok(context) => self.opening = Some(context),
            Err(refusal) => self.refusal = Some(Refusal::Refused(root.to_path_buf(), refusal)),
        }
    }

    /// Sets the pending folder up as a project of this name, on the person's word, and
    /// opens it. `fresh_starter` is the person's agreement to a new starter level beside
    /// level files that cannot be read. Whether the project was made; if not, the refusal
    /// says why and the folder is still pending.
    pub fn set_up(&mut self, name: &str, fresh_starter: bool) -> bool {
        let Some(Pending::SetUp { folder, .. }) = &self.pending else {
            return false;
        };
        let folder = folder.clone();
        let mut made = Project::of_folder(Some(folder.clone()));
        made.set_up(name, fresh_starter);
        self.after_the_persons_word(&folder, &made)
    }

    /// Where the new project that is pending would be made under this name: the folder
    /// named after it inside the chosen one, or why it cannot be made there. Reads only.
    /// `None` while no new project is pending.
    pub fn new_folder(&self, name: &str) -> Option<Result<PathBuf, Diagnostic>> {
        match &self.pending {
            Some(Pending::New { parent, .. }) => Some(project::new_project_folder(parent, name)),
            _ => None,
        }
    }

    /// Makes the pending new project under this name, on the person's word, and opens it:
    /// a folder of its own inside the chosen one, with its project file, its first level
    /// and its look profile. Whether it was made; if not, the refusal says why, nothing was
    /// made and the new project is still pending.
    pub fn create(&mut self, name: &str) -> bool {
        let Some(Pending::New { parent, .. }) = &self.pending else {
            return false;
        };
        let parent = parent.clone();
        self.refusal = None;
        match project::create_project_in(&parent, name) {
            Ok(made) => {
                self.pending = None;
                let root = made.config_path.parent().map_or(parent, Path::to_path_buf);
                self.begin(&root);
                self.opening.is_some()
            }
            Err(refusal) => {
                // A folder that is taken is named; anything else is about the chosen one.
                let folder = refusal
                    .parameters
                    .get("path")
                    .filter(|_| refusal.code == "project.folder_taken")
                    .map_or(parent, PathBuf::from);
                self.refusal = Some(Refusal::Refused(folder, refusal));
                // The name that was tried is the one the field has when it is shown again.
                if let Some(Pending::New { name: offered, .. }) = &mut self.pending {
                    *offered = name.trim().to_owned();
                }
                false
            }
        }
    }

    /// Converts the pending folder's project file, on the person's word, and opens it.
    /// Whether it was converted.
    pub fn convert(&mut self) -> bool {
        let Some(Pending::Convert { folder, .. }) = &self.pending else {
            return false;
        };
        let folder = folder.clone();
        let mut converted = Project::of_folder(Some(folder.clone()));
        converted.convert();
        self.after_the_persons_word(&folder, &converted)
    }

    /// A project that is ready after a setup or a conversion is opened; one that is not
    /// says why, and the folder stays pending.
    fn after_the_persons_word(&mut self, folder: &Path, project: &Project) -> bool {
        self.refusal = None;
        if matches!(project.state(), ProjectState::Ready { .. }) {
            self.pending = None;
            self.begin(folder);
            return self.opening.is_some();
        }
        let refusal = match (project.problem(), project.state()) {
            (Some(refusal), _) | (None, ProjectState::Refused(refusal)) => refusal.clone(),
            _ => Diagnostic::new("project.not_ready"),
        };
        self.refusal = Some(Refusal::Refused(folder.to_path_buf(), refusal));
        false
    }

    /// Leaves the new project, the setup or the conversion that is pending. Nothing was
    /// written.
    pub fn back(&mut self) {
        self.pending = None;
        self.refusal = None;
    }

    /// The project that was chosen and waits to be entered.
    pub fn opening(&self) -> Option<&ProjectStartupContext> {
        self.opening.as_ref()
    }

    /// Takes the project that waits to be entered: the one who takes it enters it, and says
    /// so with [`Launcher::entered`].
    pub fn take_opening(&mut self) -> Option<ProjectStartupContext> {
        self.opening.take()
    }

    /// The editor was entered on the project in this folder: the browser is put away and
    /// the project remembered at the top of the list, at this time in milliseconds since
    /// 1970. A list that cannot be written does not keep the project from being entered;
    /// why it could not is kept ([`Launcher::list_problem`]).
    pub fn entered(&mut self, root: &Path, name: &str, now_unix_ms: u64) {
        self.shown = false;
        self.asking = None;
        self.pending = None;
        self.refusal = None;
        self.entered += 1;
        if let Some(path) = self.recents.clone() {
            match project::remember_recent(&path, root, name, now_unix_ms) {
                Ok(recents) => {
                    self.list_problem = None;
                    self.take_list(recents);
                }
                Err(refusal) => self.list_problem = Some(refusal),
            }
        }
        self.selected = None;
        self.settle_selection();
    }
}

/// How long ago a project was opened, in words: "today", "yesterday", "5 days ago", and
/// from thirty days on in whole months and years. Both times are milliseconds since 1970.
pub fn opened_words(last_opened_unix_ms: u64, now_unix_ms: u64) -> String {
    const DAY_MS: u64 = 24 * 60 * 60 * 1000;
    let days = now_unix_ms.saturating_sub(last_opened_unix_ms) / DAY_MS;
    match days {
        0 => "today".to_owned(),
        1 => "yesterday".to_owned(),
        2..=29 => format!("{days} days ago"),
        30..=59 => "a month ago".to_owned(),
        60..=364 => format!("{} months ago", days / 30),
        365..=729 => "a year ago".to_owned(),
        _ => format!("{} years ago", days / 365),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_services::project::GameRuntimeAvailability;

    const NOW: u64 = 1_800_000_000_000;

    /// A folder that is set up as a project of this name, and its canonical path.
    fn project_in(parent: &Path, name: &str) -> PathBuf {
        let folder = parent.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        let mut project = Project::of_folder(Some(folder.clone()));
        assert!(project.set_up(name, false).is_some());
        folder.canonicalize().unwrap()
    }

    /// A browser whose list is a file in this folder.
    fn launcher_in(home: &Path) -> Launcher {
        Launcher::at_start(Some(home.join("recent-projects.json")))
    }

    fn names(launcher: &Launcher) -> Vec<&str> {
        launcher
            .rows()
            .iter()
            .map(|row| row.recent.name.as_str())
            .collect()
    }

    #[test]
    fn without_projects_the_browser_has_no_list_and_is_off_when_it_is_not_asked_for() {
        let home = tempfile::tempdir().unwrap();
        let launcher = launcher_in(home.path());
        assert!(launcher.shown() && launcher.has_list());
        assert!(launcher.rows().is_empty());
        assert_eq!(launcher.selected(), None);
        assert_eq!(launcher.list_problem(), None);
        // Looking made no file.
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);

        // Where the system has no place for a list there is none, and the browser is still
        // the first screen.
        let nowhere = Launcher::at_start(None);
        assert!(nowhere.shown() && !nowhere.has_list());
        // Told on the command line what to open, the editor has no browser.
        let off = Launcher::off();
        assert!(!off.shown() && off.wants_folder().is_none());
    }

    #[test]
    fn a_folder_is_asked_for_once_and_the_answer_none_changes_nothing() {
        let home = tempfile::tempdir().unwrap();
        let mut launcher = launcher_in(home.path());
        assert_eq!(launcher.wants_folder(), None);
        launcher.ask_folder(FolderFor::Open);
        // A second question while the first is open is not asked.
        launcher.ask_folder(FolderFor::New);
        assert_eq!(launcher.wants_folder(), Some(FolderFor::Open));
        launcher.folder_chosen(None);
        assert_eq!(launcher.wants_folder(), None);
        assert!(launcher.pending().is_none() && launcher.refusal().is_none());
        assert!(launcher.opening().is_none());
        // An answer nobody asked for is not taken.
        launcher.folder_chosen(Some(home.path().to_path_buf()));
        assert!(launcher.pending().is_none() && launcher.opening().is_none());
        // A browser that is off asks for nothing.
        let mut off = Launcher::off();
        off.ask_folder(FolderFor::Open);
        assert_eq!(off.wants_folder(), None);
    }

    #[test]
    fn a_new_project_is_made_in_a_folder_of_its_own_inside_the_chosen_one_and_entered() {
        let home = tempfile::tempdir().unwrap();
        // A folder that holds other things, as a person's documents do.
        let chosen = home.path().join("Documents");
        std::fs::create_dir_all(&chosen).unwrap();
        std::fs::write(chosen.join("letter.txt"), b"the person's").unwrap();
        let chosen = chosen.canonicalize().unwrap();
        let inside = |folder: &Path| {
            let mut names: Vec<String> = std::fs::read_dir(folder)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect();
            names.sort();
            names
        };
        let mut launcher = launcher_in(home.path());

        launcher.ask_folder(FolderFor::New);
        launcher.folder_chosen(Some(chosen.clone()));
        let Some(Pending::New { parent, name }) = launcher.pending() else {
            panic!("a new project waits for its name");
        };
        assert_eq!((parent, name.as_str()), (&chosen, NEW_PROJECT_NAME));
        // Where it would be made is said before anything is, by reading only.
        assert_eq!(
            launcher.new_folder(" Harbour Town "),
            Some(Ok(chosen.join("Harbour Town")))
        );
        let refused = launcher.new_folder("  ").unwrap().unwrap_err();
        assert_eq!(refused.code, "project.value");
        assert_eq!(inside(&chosen), ["letter.txt"]);
        assert!(launcher.opening().is_none());

        // Leaving it writes nothing either.
        launcher.back();
        assert!(launcher.pending().is_none() && launcher.new_folder("Harbour").is_none());
        assert!(!launcher.create("Harbour"));
        assert_eq!(inside(&chosen), ["letter.txt"]);

        launcher.ask_folder(FolderFor::New);
        launcher.folder_chosen(Some(chosen.clone()));
        assert!(launcher.create("Harbour Town"));
        assert!(launcher.pending().is_none() && launcher.refusal().is_none());
        let context = launcher.take_opening().expect("the new project is entered");
        let root = chosen.join("Harbour Town");
        assert_eq!(context.project_root, root);
        assert_eq!(context.package_root, root.join("cooked"));
        assert!(context.start_level_exists && context.start_level.starts_with(&root));
        // The chosen folder was given the project's folder and nothing else, and the
        // project has its file, its level and its look profile.
        assert_eq!(inside(&chosen), ["Harbour Town", "letter.txt"]);
        assert_eq!(inside(&root), ["arc-project.json", "assets", "levels"]);
        assert!(root.join("assets/render/look-v1.json").is_file());
        assert_eq!(
            std::fs::read(chosen.join("letter.txt")).unwrap(),
            b"the person's"
        );
        // It is content only: no game is in it, and none is set for this editor.
        assert_eq!(context.game_root, None);
        assert_eq!(context.runtime, GameRuntimeAvailability::Unconfigured);
        assert!(launcher.take_opening().is_none());
        // The browser stays until the editor says that it was entered.
        assert!(launcher.shown());
        assert_eq!(launcher.times_entered(), 0);
        launcher.entered(&root, "Harbour Town", NOW);
        assert!(!launcher.shown());
        assert_eq!(launcher.times_entered(), 1);
        assert_eq!(names(&launcher), ["Harbour Town"]);
        assert_eq!(launcher.rows()[0].recent.last_opened_unix_ms, NOW);

        // A second project of that name is not made over the first: the folder that is
        // taken is named, nothing is written, and another name can be given.
        let mut again = launcher_in(home.path());
        again.ask_folder(FolderFor::New);
        again.folder_chosen(Some(chosen.clone()));
        assert!(!again.create("Harbour Town"));
        let Some(Refusal::Refused(folder, refusal)) = again.refusal() else {
            panic!("a folder that is taken is refused");
        };
        assert_eq!(
            (folder, refusal.code.as_str()),
            (&root, "project.folder_taken")
        );
        // The name that was tried is still the one in the field.
        let Some(Pending::New { name, .. }) = again.pending() else {
            panic!("the new project is still pending");
        };
        assert_eq!(name, "Harbour Town");
        assert!(again.opening().is_none());
        assert_eq!(inside(&chosen), ["Harbour Town", "letter.txt"]);
        assert!(again.create("Harbour Quay"));
        assert!(again.refusal().is_none());
        assert_eq!(
            again.take_opening().unwrap().project_root,
            chosen.join("Harbour Quay")
        );

        // A new project is not made inside a folder that is a project; the folder is named
        // as it was chosen.
        again.ask_folder(FolderFor::New);
        again.folder_chosen(Some(root.clone()));
        assert_eq!(
            again.refusal(),
            Some(&Refusal::AlreadyProject(root.clone()))
        );
        assert!(again.opening().is_none() && again.pending().is_none());
        assert_eq!(inside(&root), ["arc-project.json", "assets", "levels"]);
    }

    /// A folder that has what the services take a game's folder to have.
    fn game_in(parent: &Path, name: &str) -> PathBuf {
        let folder = parent.join(name);
        let programs = folder.join("target/debug");
        std::fs::create_dir_all(&programs).unwrap();
        std::fs::create_dir_all(folder.join("assets")).unwrap();
        for program in ["server", "client"] {
            let path = programs.join(format!("{program}{}", std::env::consts::EXE_SUFFIX));
            std::fs::write(&path, b"").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        folder.canonicalize().unwrap()
    }

    #[test]
    fn the_games_folder_is_one_setting_of_the_editor_and_a_folder_that_is_no_game_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let file = home.path().join("runtime-install.json");
        let project = project_in(home.path(), "Quay");
        let game = game_in(home.path(), "game");
        let mut launcher = launcher_in(home.path()).with_game_file(Some(file.clone()));
        // Nothing is set, and looking wrote nothing.
        assert!(matches!(
            launcher.game().unwrap().status,
            RuntimeLocationStatus::Unconfigured
        ));
        assert!(!file.exists());
        launcher.ask_folder(FolderFor::Open);
        launcher.folder_chosen(Some(project.clone()));
        let context = launcher.take_opening().unwrap();
        assert_eq!(context.game_root, None);
        assert_eq!(context.runtime, GameRuntimeAvailability::Unconfigured);

        // A folder that is no game is refused, with the folder, and nothing is kept.
        let plain = home.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        launcher.ask_folder(FolderFor::Game);
        assert_eq!(launcher.wants_folder(), Some(FolderFor::Game));
        launcher.folder_chosen(Some(plain.clone()));
        let (folder, refusal) = launcher.game_refusal().expect("no game is there");
        assert_eq!(folder, &plain);
        assert!(refusal.code.starts_with("runtime."), "{}", refusal.code);
        assert_eq!(launcher.game_changes(), 0);
        assert!(!file.exists());
        // Choosing none leaves the refusal and the setting as they are.
        launcher.ask_folder(FolderFor::Game);
        launcher.folder_chosen(None);
        assert!(launcher.game_refusal().is_some() && launcher.wants_folder().is_none());

        // The game's folder is taken, kept in the file, and is what a project is played with.
        launcher.ask_folder(FolderFor::Game);
        launcher.folder_chosen(Some(game.clone()));
        assert_eq!(launcher.game_refusal(), None);
        assert_eq!(launcher.game_changes(), 1);
        let RuntimeLocationStatus::Configured { root, .. } = &launcher.game().unwrap().status
        else {
            panic!("the game's folder is set");
        };
        assert_eq!(root, &game);
        assert!(file.is_file());
        launcher.ask_folder(FolderFor::Open);
        launcher.folder_chosen(Some(project.clone()));
        let context = launcher.take_opening().unwrap();
        assert_eq!(context.game_root, Some(game.clone()));
        assert_eq!(context.runtime, GameRuntimeAvailability::Ready);
        assert_eq!(context.project_root, project);

        // Started again, the setting is read from the file; and it can be asked for in the
        // editor too, when the browser is put away.
        let mut later = launcher_in(home.path()).with_game_file(Some(file.clone()));
        assert!(matches!(
            later.game().unwrap().status,
            RuntimeLocationStatus::Configured { .. }
        ));
        later.entered(&project, "Quay", NOW);
        later.ask_folder(FolderFor::Open);
        assert_eq!(later.wants_folder(), None);
        later.ask_folder(FolderFor::Game);
        assert_eq!(later.wants_folder(), Some(FolderFor::Game));
        later.folder_chosen(Some(plain));
        assert!(later.game_refusal().is_some());
        assert!(matches!(
            later.game().unwrap().status,
            RuntimeLocationStatus::Configured { .. }
        ));

        // Without a place to keep it the folder cannot be chosen, which is said; and an
        // editor without the browser has no such setting to ask for.
        let mut nowhere = launcher_in(home.path()).with_game_file(None);
        assert!(matches!(
            nowhere.game().unwrap().status,
            RuntimeLocationStatus::PreferenceUnavailable
        ));
        assert!(!nowhere.choose_game(&game));
        assert_eq!(
            nowhere.game_refusal().unwrap().1.code,
            "runtime.preference_unavailable"
        );
        let mut off = Launcher::off();
        assert!(off.game().is_none());
        off.ask_folder(FolderFor::Game);
        assert_eq!(off.wants_folder(), None);
    }

    #[test]
    fn opening_leads_to_the_editor_or_to_what_must_come_first_or_says_why_not() {
        let home = tempfile::tempdir().unwrap();
        let ready = project_in(home.path(), "Ready");
        let mut launcher = launcher_in(home.path());

        // A project that is ready is entered.
        launcher.ask_folder(FolderFor::Open);
        launcher.folder_chosen(Some(ready.clone()));
        assert_eq!(launcher.opening().unwrap().project_root, ready);
        launcher.take_opening();

        // A folder that is no project yet is offered for a setup, and said to be opened.
        let plain = home.path().join("Plain");
        std::fs::create_dir_all(&plain).unwrap();
        launcher.ask_folder(FolderFor::Open);
        launcher.folder_chosen(Some(plain));
        assert!(matches!(launcher.pending(), Some(Pending::SetUp { .. })));
        launcher.back();

        // A folder that is not there, and a project file that is none, lead nowhere.
        let gone = home.path().join("Gone");
        launcher.ask_folder(FolderFor::Open);
        launcher.folder_chosen(Some(gone));
        assert!(matches!(launcher.refusal(), Some(Refusal::Missing(_))));
        let broken = home.path().join("Broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("arc-project.json"), b"{").unwrap();
        launcher.ask_folder(FolderFor::Open);
        launcher.folder_chosen(Some(broken));
        let Some(Refusal::Refused(_, refusal)) = launcher.refusal() else {
            panic!("a project file that is none is refused");
        };
        assert!(refusal.code.starts_with("project."), "{}", refusal.code);
        assert!(launcher.opening().is_none() && launcher.pending().is_none());

        // The next folder that is chosen takes the refusal away.
        launcher.ask_folder(FolderFor::Open);
        launcher.folder_chosen(Some(ready));
        assert!(launcher.refusal().is_none() && launcher.opening().is_some());
    }

    #[test]
    fn a_project_whose_start_level_is_missing_is_listed_as_that_and_opened_without_making_it() {
        let home = tempfile::tempdir().unwrap();
        let root = project_in(home.path(), "Quay");
        let mut launcher = launcher_in(home.path());
        launcher.entered(&root, "Quay", NOW);
        // The level the project starts with is taken away.
        let level = project::startup_context_using(&root, None)
            .unwrap()
            .start_level;
        std::fs::remove_file(&level).unwrap();

        // The row is not a ready project's: it says what is missing, and can be opened.
        let mut launcher = launcher_in(home.path());
        let ProjectInspectionStatus::MissingStartLevel { name, start_level } =
            &launcher.rows()[0].status
        else {
            panic!("a project without its start level is listed as that");
        };
        assert_eq!((name.as_str(), start_level), ("Quay", &level));
        assert!(launcher.rows()[0].can_open());

        // Opened, it is entered on that level's place, which is said not to be there; the
        // file is not made by looking or by opening.
        assert!(launcher.open_selected());
        assert!(launcher.refusal().is_none() && launcher.pending().is_none());
        let context = launcher.take_opening().expect("the project is entered");
        assert_eq!(context.start_level, level);
        assert!(!context.start_level_exists);
        assert_eq!(context.package_root, root.join("cooked"));
        assert!(!level.exists());

        // The same through the folder chooser; and a new project is not made inside it.
        launcher.ask_folder(FolderFor::Open);
        launcher.folder_chosen(Some(root.clone()));
        assert!(!launcher.take_opening().unwrap().start_level_exists);
        launcher.ask_folder(FolderFor::New);
        launcher.folder_chosen(Some(root.clone()));
        assert_eq!(launcher.refusal(), Some(&Refusal::AlreadyProject(root)));
        assert!(launcher.opening().is_none() && !level.exists());
    }

    #[test]
    fn an_older_project_file_is_converted_on_the_persons_word_before_it_is_entered() {
        let home = tempfile::tempdir().unwrap();
        let folder = home.path().join("Old");
        std::fs::create_dir_all(folder.join("levels")).unwrap();
        let old = br#"{"schema":1,"project":{"name":"Old"},"startup":{"start_level":"levels/old.level.json"},"rendering":{"profile":"assets/render/look-v1.json"},"controls":{"move_forward":"W","move_backward":"S","move_left":"A","move_right":"D","jump":"Space","toggle_view":"V","release_cursor":"Escape","look":{"mouse":true,"gamepad_right_stick":true}}}"#;
        let manifest = folder.join("arc-project.json");
        std::fs::write(&manifest, old).unwrap();
        // The level the old file starts with is in the folder, and is a level.
        let level = serde_json::to_vec_pretty(&scene_data::current_map()).unwrap();
        std::fs::write(folder.join("levels/old.level.json"), level).unwrap();
        let mut launcher = launcher_in(home.path());

        launcher.ask_folder(FolderFor::Open);
        launcher.folder_chosen(Some(folder.clone()));
        assert!(matches!(launcher.pending(), Some(Pending::Convert { .. })));
        // Looking converted nothing.
        assert_eq!(std::fs::read(&manifest).unwrap(), old);
        assert!(launcher.opening().is_none());

        assert!(launcher.convert());
        assert!(launcher.pending().is_none());
        assert_eq!(
            launcher.opening().unwrap().project_root,
            folder.canonicalize().unwrap()
        );
        // The file as it was is kept beside the new one.
        assert_eq!(
            std::fs::read(folder.join("arc-project.json.schema-1.bak")).unwrap(),
            old
        );
    }

    #[test]
    fn the_list_is_most_recent_first_says_what_each_folder_is_now_and_forgets_on_request() {
        let home = tempfile::tempdir().unwrap();
        let first = project_in(home.path(), "First");
        let second = project_in(home.path(), "Second");
        let mut launcher = launcher_in(home.path());
        launcher.entered(&first, "First", NOW);
        launcher.entered(&second, "Second", NOW + 1000);

        // Started again, the list is read from the file, the latest first and chosen.
        let mut launcher = launcher_in(home.path());
        assert_eq!(names(&launcher), ["Second", "First"]);
        assert_eq!(launcher.selected(), Some(0));
        assert!(launcher.rows().iter().all(Row::can_open));
        assert!(matches!(
            launcher.rows()[0].status,
            ProjectInspectionStatus::Ready { .. }
        ));

        // The chosen row moves with the keys and stays at the list's ends.
        launcher.step_selection(false);
        assert_eq!(launcher.selected(), Some(0));
        launcher.step_selection(true);
        launcher.step_selection(true);
        assert_eq!(launcher.selected(), Some(1));
        assert!(launcher.open_selected());
        assert_eq!(launcher.take_opening().unwrap().project_root, first);

        // A project whose folder was taken away is still listed, as that, and opens nothing.
        std::fs::remove_dir_all(&second).unwrap();
        launcher.read();
        assert_eq!(names(&launcher), ["Second", "First"]);
        assert_eq!(launcher.rows()[0].status, ProjectInspectionStatus::Missing);
        assert!(!launcher.rows()[0].can_open());
        assert!(!launcher.open_row(0));
        assert!(matches!(launcher.refusal(), Some(Refusal::Missing(_))));
        assert!(launcher.opening().is_none());

        // Forgotten, it is out of the list and of the file; the other project is untouched.
        launcher.forget_row(0);
        assert_eq!(names(&launcher), ["First"]);
        assert_eq!(names(&launcher_in(home.path())), ["First"]);
        assert!(first.join("arc-project.json").is_file());
        assert!(!launcher.open_row(5));
    }

    #[test]
    fn a_list_that_cannot_be_read_is_said_and_kept_and_a_project_is_entered_all_the_same() {
        let home = tempfile::tempdir().unwrap();
        let ready = project_in(home.path(), "Ready");
        let file = home.path().join("recent-projects.json");
        std::fs::write(&file, b"not a list").unwrap();

        let mut launcher = Launcher::at_start(Some(file.clone()));
        assert!(launcher.rows().is_empty());
        assert!(launcher.list_problem().is_some());
        launcher.ask_folder(FolderFor::Open);
        launcher.folder_chosen(Some(ready.clone()));
        let context = launcher.take_opening().expect("the project is entered");
        launcher.entered(&context.project_root, "Ready", NOW);
        assert!(!launcher.shown());
        // The file nobody could read is as it was, and why is still said.
        assert_eq!(std::fs::read(&file).unwrap(), b"not a list");
        assert!(launcher.list_problem().is_some());
    }

    #[test]
    fn how_long_ago_a_project_was_opened_is_said_in_days_months_and_years() {
        const DAY: u64 = 24 * 60 * 60 * 1000;
        let ago = |days: u64| opened_words(NOW - days * DAY, NOW);
        assert_eq!(ago(0), "today");
        assert_eq!(opened_words(NOW - DAY + 1, NOW), "today");
        assert_eq!(ago(1), "yesterday");
        assert_eq!(ago(2), "2 days ago");
        assert_eq!(ago(29), "29 days ago");
        assert_eq!(ago(30), "a month ago");
        assert_eq!(ago(75), "2 months ago");
        assert_eq!(ago(364), "12 months ago");
        assert_eq!(ago(365), "a year ago");
        assert_eq!(ago(800), "2 years ago");
        // A clock that was set back does not make a project one of the future.
        assert_eq!(opened_words(NOW + DAY, NOW), "today");
    }
}
