//! Read-only project discovery, portable recent-project storage and editor startup facts.
//!
//! Recent projects are stored in a small versioned file in the user's local config folder.
//! A path is only remembered after it can be represented as UTF-8; opening a project whose
//! path is not UTF-8 is still allowed by the editor.

use super::{
    inside, io_problem, level_problem, load_project, plan_setup, read_limited,
    resolve_project_path, start_level_path, SetupPlan,
};
use crate::persistence::PathLock;
use directories::ProjectDirs;
use scene_data::Diagnostic;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

/// Maximum size of the local recent-project store.
pub const MAX_RECENTS_BYTES: usize = 64 * 1024;
/// Maximum number of projects retained in the local recent-project store.
pub const MAX_RECENT_PROJECTS: usize = 32;
/// Current schema of the local recent-project store.
pub const RECENTS_SCHEMA: u32 = 1;

/// A read-only inspection of a folder from the project browser.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectInspection {
    pub root: PathBuf,
    pub status: ProjectInspectionStatus,
}

/// What the browser can do with a project folder without changing it.
#[derive(Clone, Debug, PartialEq)]
pub enum ProjectInspectionStatus {
    /// The selected recent folder was moved or removed.
    Missing,
    /// A current manifest and its configured start-level file were read and validated.
    Ready { name: String, start_level: PathBuf },
    /// The manifest is current and safe, but its configured start level is absent. The
    /// project can still be entered with an unsaved working map so the missing file can be
    /// restored or saved from Project settings.
    MissingStartLevel { name: String, start_level: PathBuf },
    /// No manifest exists; the normal setup flow can inspect/create one.
    SetupRequired { plan: SetupPlan },
    /// The folder or manifest cannot be used, with the same diagnostic as project services.
    Refused { problem: Diagnostic },
}

/// Inspect a project folder without writing files. Filesystem and manifest failures are
/// retained as a visible status so a project browser can show broken recent entries.
pub fn inspect(root: &Path) -> ProjectInspection {
    let status = match fs::metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ProjectInspectionStatus::Missing
        }
        Err(error) => ProjectInspectionStatus::Refused {
            problem: io_problem("project.read", root, error),
        },
        Ok(metadata) if !metadata.is_dir() => ProjectInspectionStatus::Refused {
            problem: Diagnostic::new("project.not_a_folder").with("path", root.display()),
        },
        Ok(_) => inspect_existing_folder(root),
    };
    ProjectInspection {
        root: root.to_path_buf(),
        status,
    }
}

fn inspect_existing_folder(root: &Path) -> ProjectInspectionStatus {
    let canonical = match fs::canonicalize(root) {
        Ok(path) => path,
        Err(error) => {
            return ProjectInspectionStatus::Refused {
                problem: io_problem("project.read", root, error),
            };
        }
    };
    let manifest = match resolve_project_path(&canonical) {
        Ok(path) => path,
        Err(problem) => return ProjectInspectionStatus::Refused { problem },
    };
    if let Err(problem) = inside(&canonical, &manifest) {
        return ProjectInspectionStatus::Refused { problem };
    }
    match fs::symlink_metadata(&manifest) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return match plan_setup(&canonical) {
                Ok(plan) => ProjectInspectionStatus::SetupRequired { plan },
                Err(problem) => ProjectInspectionStatus::Refused { problem },
            };
        }
        Err(error) => {
            return ProjectInspectionStatus::Refused {
                problem: io_problem("project.read", &manifest, error),
            };
        }
        Ok(_) => {}
    }
    match load_project(&canonical) {
        Ok(config) => match start_level_path(&canonical, &config) {
            Ok(start_level) => match fs::metadata(&start_level) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    ProjectInspectionStatus::MissingStartLevel {
                        name: config.project.name,
                        start_level,
                    }
                }
                Err(error) => ProjectInspectionStatus::Refused {
                    problem: io_problem("project.read", &start_level, error),
                },
                Ok(metadata) if !metadata.is_file() => ProjectInspectionStatus::Refused {
                    problem: Diagnostic::new("project.not_a_file")
                        .with("path", start_level.display()),
                },
                Ok(_) => match level_problem(&canonical, &start_level) {
                    None => ProjectInspectionStatus::Ready {
                        name: config.project.name,
                        start_level,
                    },
                    Some((code, detail)) => ProjectInspectionStatus::Refused {
                        problem: Diagnostic::new(&code)
                            .with("path", start_level.display())
                            .with("detail", detail),
                    },
                },
            },
            Err(problem) => ProjectInspectionStatus::Refused { problem },
        },
        Err(problem) => ProjectInspectionStatus::Refused { problem },
    }
}

/// The kind of runtime file or directory needed for editor Play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameRuntimeRequirementKind {
    ServerExecutable,
    ClientExecutable,
    AssetsDirectory,
    PipCharacterModel,
}

/// A runtime requirement which is absent or points outside the selected game folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameRuntimeRequirement {
    pub kind: GameRuntimeRequirementKind,
    pub path: PathBuf,
}

/// Whether the selected project folder has the local game runtime Play needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameRuntimeAvailability {
    Ready,
    Missing {
        requirements: Vec<GameRuntimeRequirement>,
    },
    Unconfigured,
    PreferenceRefused {
        problem: Diagnostic,
    },
    FolderRefused {
        problem: Diagnostic,
    },
}

/// The source that selected the game runtime for this project session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeLocationSource {
    EditorPreference,
    ProjectFolder,
    Unconfigured,
}

/// Facts passed to the editor after a project has been selected and validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectStartupContext {
    pub project_root: PathBuf,
    pub start_level: PathBuf,
    pub start_level_exists: bool,
    /// Runtime root, separate from `project_root`; absent when no runtime can be selected.
    pub game_root: Option<PathBuf>,
    pub runtime_source: RuntimeLocationSource,
    pub package_root: PathBuf,
    pub runtime: GameRuntimeAvailability,
}

/// Runtime selection facts that can be refreshed after the editor-wide preference changes,
/// without reopening a project's document or changing its source files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectRuntimeContext {
    pub game_root: Option<PathBuf>,
    pub runtime_source: RuntimeLocationSource,
    pub runtime: GameRuntimeAvailability,
}

/// Resolve the selected runtime for an already-open project using the OS preference path.
pub fn resolve_project_runtime(project_root: &Path) -> ProjectRuntimeContext {
    resolve_project_runtime_using(
        project_root,
        crate::runtime::default_runtime_preference_path().as_deref(),
    )
}

/// Injected-path form of [`resolve_project_runtime`] for scripted use.
pub fn resolve_project_runtime_using(
    project_root: &Path,
    runtime_preference_path: Option<&Path>,
) -> ProjectRuntimeContext {
    let (game_root, runtime_source, runtime) =
        resolve_runtime_location(project_root, runtime_preference_path);
    ProjectRuntimeContext {
        game_root,
        runtime_source,
        runtime,
    }
}

/// Resolve paths and runtime availability for a valid current-schema project.
///
/// Call [`inspect`] first: missing, setup, conversion and refused folders have distinct
/// browser behavior and are not accepted here. This returns paths only; the game still
/// starts from the verified cooked package rather than loading the source start level.
pub fn startup_context(root: &Path) -> Result<ProjectStartupContext, Diagnostic> {
    startup_context_using(
        root,
        crate::runtime::default_runtime_preference_path().as_deref(),
    )
}

/// As [`startup_context`], with an injected user-preference path for scripted checks.
/// Passing `None` means the platform has no standard preference location.
pub fn startup_context_using(
    root: &Path,
    runtime_preference_path: Option<&Path>,
) -> Result<ProjectStartupContext, Diagnostic> {
    let inspection = inspect(root);
    let (inspected_start_level, start_level_exists) = match inspection.status {
        ProjectInspectionStatus::Ready { start_level, .. } => (start_level, true),
        ProjectInspectionStatus::MissingStartLevel { start_level, .. } => (start_level, false),
        ProjectInspectionStatus::Missing => {
            return Err(Diagnostic::new("project.folder_missing").with("path", root.display()));
        }
        ProjectInspectionStatus::SetupRequired { .. } => {
            return Err(Diagnostic::new("project.setup_required").with("path", root.display()));
        }
        ProjectInspectionStatus::Refused { problem } => return Err(problem),
    };
    let project_root =
        fs::canonicalize(root).map_err(|error| io_problem("project.read", root, error))?;
    // The inspection produces an absolute path based on the canonical folder. Check it once
    // more against the canonical root in case the filesystem changed between calls.
    inside(&project_root, &inspected_start_level)?;
    let start_level = inspected_start_level;
    let start_level_exists = start_level_exists && start_level.is_file();
    let package_root = project_root.join("cooked");
    let runtime_context = resolve_project_runtime_using(&project_root, runtime_preference_path);
    Ok(ProjectStartupContext {
        game_root: runtime_context.game_root,
        runtime_source: runtime_context.runtime_source,
        project_root,
        start_level,
        start_level_exists,
        package_root,
        runtime: runtime_context.runtime,
    })
}

fn resolve_runtime_location(
    project_root: &Path,
    preference_path: Option<&Path>,
) -> (
    Option<PathBuf>,
    RuntimeLocationSource,
    GameRuntimeAvailability,
) {
    if let Some(path) = preference_path {
        match crate::runtime::load_runtime_preference(path) {
            Err(problem) => {
                return (
                    None,
                    RuntimeLocationSource::EditorPreference,
                    GameRuntimeAvailability::PreferenceRefused { problem },
                );
            }
            Ok(Some(root)) => {
                return runtime_result(&root, RuntimeLocationSource::EditorPreference);
            }
            Ok(None) => {}
        }
    }

    // Existing projects opened directly from a game worktree remain usable without a
    // preference. A content-only project with no game executables stays unconfigured,
    // even if project setup has created its own assets/ directory.
    let (root, source, availability) =
        runtime_result(project_root, RuntimeLocationSource::ProjectFolder);
    if matches!(availability, GameRuntimeAvailability::Ready)
        || matches!(availability, GameRuntimeAvailability::Missing { .. })
            && project_has_runtime_program(project_root)
    {
        (root, source, availability)
    } else {
        (
            None,
            RuntimeLocationSource::Unconfigured,
            GameRuntimeAvailability::Unconfigured,
        )
    }
}

fn runtime_result(
    root: &Path,
    source: RuntimeLocationSource,
) -> (
    Option<PathBuf>,
    RuntimeLocationSource,
    GameRuntimeAvailability,
) {
    let inspection = crate::runtime::inspect_runtime_folder(root);
    match inspection.status {
        crate::runtime::RuntimeFolderStatus::Missing => (
            Some(root.to_path_buf()),
            source,
            GameRuntimeAvailability::Missing {
                requirements: runtime_requirements(root),
            },
        ),
        crate::runtime::RuntimeFolderStatus::Ready { installation } => (
            Some(installation.root),
            source,
            GameRuntimeAvailability::Ready,
        ),
        crate::runtime::RuntimeFolderStatus::Incomplete { requirements } => (
            Some(
                inspection
                    .canonical_root
                    .unwrap_or(inspection.selected_root),
            ),
            source,
            GameRuntimeAvailability::Missing { requirements },
        ),
        crate::runtime::RuntimeFolderStatus::Refused { problem } => (
            Some(
                inspection
                    .canonical_root
                    .unwrap_or(inspection.selected_root),
            ),
            source,
            GameRuntimeAvailability::FolderRefused { problem },
        ),
    }
}

fn runtime_requirements(root: &Path) -> Vec<GameRuntimeRequirement> {
    let suffix = std::env::consts::EXE_SUFFIX;
    vec![
        GameRuntimeRequirement {
            kind: GameRuntimeRequirementKind::ServerExecutable,
            path: root.join("target/debug").join(format!("server{suffix}")),
        },
        GameRuntimeRequirement {
            kind: GameRuntimeRequirementKind::ClientExecutable,
            path: root.join("target/debug").join(format!("client{suffix}")),
        },
        GameRuntimeRequirement {
            kind: GameRuntimeRequirementKind::AssetsDirectory,
            path: root.join("assets"),
        },
        GameRuntimeRequirement {
            kind: GameRuntimeRequirementKind::PipCharacterModel,
            path: root.join("assets/characters/pip/pip_robe_lod0.glb"),
        },
    ]
}

fn project_has_runtime_program(root: &Path) -> bool {
    runtime_requirements(root)
        .into_iter()
        .filter(|requirement| {
            matches!(
                requirement.kind,
                GameRuntimeRequirementKind::ServerExecutable
                    | GameRuntimeRequirementKind::ClientExecutable
            )
        })
        .any(|requirement| requirement.path.is_file())
}

#[cfg(test)]
fn game_runtime(root: &Path) -> GameRuntimeAvailability {
    runtime_result(root, RuntimeLocationSource::ProjectFolder).2
}

/// A valid recent project, including its caller-supplied last-opened timestamp.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentProject {
    pub root: PathBuf,
    pub name: String,
    pub last_opened_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecentStore {
    schema: u32,
    projects: Vec<RecentEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecentEntry {
    root: String,
    name: String,
    last_opened_unix_ms: u64,
}

/// Per-user local configuration file for recent projects, or `None` when the OS provides
/// no local configuration directory.
pub fn default_recents_path() -> Option<PathBuf> {
    ProjectDirs::from("com", "Arc", "Arc Editor")
        .map(|dirs| dirs.config_local_dir().join("recent-projects.json"))
}

/// Load the bounded recent-project store. A missing file is an empty list. A malformed,
/// oversized or newer-version file is refused and left untouched.
pub fn load_recents(path: &Path) -> Result<Vec<RecentProject>, Diagnostic> {
    let Some(bytes) = read_optional_store(path)? else {
        return Ok(Vec::new());
    };
    let store = decode_recents(path, &bytes)?;
    Ok(normalize_recents(store.projects))
}

/// Add or refresh a project in the most-recent-first list. The caller supplies the time so
/// saved state and tests do not depend on a hidden clock. The root is stored canonically.
pub fn remember_recent(
    path: &Path,
    root: &Path,
    name: &str,
    last_opened_unix_ms: u64,
) -> Result<Vec<RecentProject>, Diagnostic> {
    validate_recent_name(path, name)?;
    // Give this format limitation a stable code instead of a platform-specific canonicalize
    // error when the OS permits a non-UTF-8 path but the JSON store cannot represent it.
    root.to_str().ok_or_else(|| {
        Diagnostic::new("project.recent_path_invalid").with("path", root.display())
    })?;
    let canonical_root = fs::canonicalize(root).map_err(|error| {
        Diagnostic::new("project.recent_root_invalid")
            .with("path", root.display())
            .with("detail", error)
    })?;
    if !canonical_root.is_dir() {
        return Err(
            Diagnostic::new("project.recent_root_invalid").with("path", canonical_root.display())
        );
    }
    let root_text = canonical_root.to_str().ok_or_else(|| {
        Diagnostic::new("project.recent_path_invalid").with("path", canonical_root.display())
    })?;
    let _lock = lock_recents(path)?;
    let mut entries = read_store_for_update(path)?;
    entries.retain(|entry| stored_root_key(&entry.root) != root_text);
    entries.push(RecentEntry {
        root: root_text.to_owned(),
        name: name.to_owned(),
        last_opened_unix_ms,
    });
    entries.sort_by(|left, right| right.last_opened_unix_ms.cmp(&left.last_opened_unix_ms));
    entries.truncate(MAX_RECENT_PROJECTS);
    write_recents(path, entries.clone())?;
    Ok(normalize_recents(entries))
}

/// Remove a project from recents. Missing project folders can still be removed: their
/// absolute stored path is compared lexically when it no longer canonicalizes.
pub fn forget_recent(path: &Path, root: &Path) -> Result<Vec<RecentProject>, Diagnostic> {
    let key = recent_root_key(root)?;
    let _lock = lock_recents(path)?;
    let mut entries = read_store_for_update(path)?;
    entries.retain(|entry| stored_root_key(&entry.root) != key);
    write_recents(path, entries.clone())?;
    Ok(normalize_recents(entries))
}

fn validate_recent_name(path: &Path, name: &str) -> Result<(), Diagnostic> {
    if name.trim().is_empty() || name.len() > 512 {
        Err(Diagnostic::new("project.recent_name_invalid").with("path", path.display()))
    } else {
        Ok(())
    }
}

fn recent_root_key(root: &Path) -> Result<String, Diagnostic> {
    root.to_str().ok_or_else(|| {
        Diagnostic::new("project.recent_path_invalid").with("path", root.display())
    })?;
    let canonical = match fs::canonicalize(root) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && root.is_absolute() => {
            root.to_path_buf()
        }
        Err(error) => {
            return Err(Diagnostic::new("project.recent_root_invalid")
                .with("path", root.display())
                .with("detail", error));
        }
    };
    canonical.to_str().map(str::to_owned).ok_or_else(|| {
        Diagnostic::new("project.recent_path_invalid").with("path", canonical.display())
    })
}

fn lock_recents(path: &Path) -> Result<PathLock, Diagnostic> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| io_problem("project.recents_write", parent, error))?;
    let mut name = path
        .file_name()
        .ok_or_else(|| {
            Diagnostic::new("project.recents_path_invalid").with("path", path.display())
        })?
        .to_os_string();
    name.push(".lock");
    let lock = path.with_file_name(name);
    PathLock::acquire(lock.clone()).map_err(|error| {
        if error.code == "save.locked" {
            Diagnostic::new("project.recents_locked")
                .with("path", path.display())
                .with("lock", lock.display())
        } else {
            Diagnostic::new("project.recents_write")
                .with("path", path.display())
                .with("detail", error)
        }
    })
}

fn read_store_for_update(path: &Path) -> Result<Vec<RecentEntry>, Diagnostic> {
    let Some(bytes) = read_optional_store(path)? else {
        return Ok(Vec::new());
    };
    Ok(decode_recents(path, &bytes)?.projects)
}

fn read_optional_store(path: &Path) -> Result<Option<Vec<u8>>, Diagnostic> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_problem("project.recents_read", path, error)),
        Ok(_) => read_limited(path, MAX_RECENTS_BYTES)
            .map(Some)
            .map_err(|problem| recents_read_error(path, problem)),
    }
}

fn stored_root_key(root: &str) -> String {
    fs::canonicalize(root)
        .ok()
        .and_then(|path| path.to_str().map(str::to_owned))
        .unwrap_or_else(|| root.to_owned())
}

fn decode_recents(path: &Path, bytes: &[u8]) -> Result<RecentStore, Diagnostic> {
    let store: RecentStore = serde_json::from_slice(bytes).map_err(|error| {
        Diagnostic::new("project.recents_decode")
            .with("path", path.display())
            .with("detail", error)
    })?;
    if store.schema != RECENTS_SCHEMA {
        return Err(Diagnostic::new("project.recents_schema")
            .with("path", path.display())
            .with("expected", RECENTS_SCHEMA)
            .with("found", store.schema));
    }
    if store.projects.len() > MAX_RECENT_PROJECTS {
        return Err(Diagnostic::new("project.recents_limit")
            .with("path", path.display())
            .with("max", MAX_RECENT_PROJECTS));
    }
    for entry in &store.projects {
        validate_recent_name(path, &entry.name)?;
        let root = Path::new(&entry.root);
        if !root.is_absolute() {
            return Err(Diagnostic::new("project.recents_decode")
                .with("path", path.display())
                .with("detail", "recent project root must be absolute"));
        }
    }
    Ok(store)
}

fn normalize_recents(entries: Vec<RecentEntry>) -> Vec<RecentProject> {
    let mut newest: HashMap<String, RecentEntry> = HashMap::new();
    for entry in entries {
        let key = stored_root_key(&entry.root);
        match newest.get(&key) {
            Some(existing) if existing.last_opened_unix_ms >= entry.last_opened_unix_ms => {}
            _ => {
                newest.insert(key, entry);
            }
        }
    }
    let mut entries: Vec<_> = newest.into_values().collect();
    entries.sort_by(|left, right| {
        right
            .last_opened_unix_ms
            .cmp(&left.last_opened_unix_ms)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.root.cmp(&right.root))
    });
    entries.truncate(MAX_RECENT_PROJECTS);
    entries
        .into_iter()
        .filter_map(|entry| {
            let root = PathBuf::from(&entry.root);
            // Preserve a malformed legacy/non-UTF8-looking stored path as a visible entry;
            // it will inspect as Missing or Refused and can still be forgotten.
            if root.as_os_str().is_empty() {
                return None;
            }
            Some(RecentProject {
                root,
                name: entry.name,
                last_opened_unix_ms: entry.last_opened_unix_ms,
            })
        })
        .collect()
}

fn recents_read_error(path: &Path, problem: Diagnostic) -> Diagnostic {
    Diagnostic::new("project.recents_read")
        .with("path", path.display())
        .with("reason", problem.code)
        .with(
            "detail",
            problem.parameters.get("detail").map_or("", String::as_str),
        )
}

fn write_recents(path: &Path, entries: Vec<RecentEntry>) -> Result<(), Diagnostic> {
    let store = RecentStore {
        schema: RECENTS_SCHEMA,
        projects: entries,
    };
    let mut bytes = serde_json::to_vec_pretty(&store).map_err(|error| {
        Diagnostic::new("project.recents_write")
            .with("path", path.display())
            .with("detail", error)
    })?;
    bytes.push(b'\n');
    if bytes.len() > MAX_RECENTS_BYTES {
        return Err(Diagnostic::new("project.recents_limit")
            .with("path", path.display())
            .with("max_bytes", MAX_RECENTS_BYTES));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| io_problem("project.recents_write", parent, error))?;
    temp.write_all(&bytes)
        .and_then(|()| temp.as_file().sync_all())
        .map_err(|error| io_problem("project.recents_write", path, error))?;
    temp.persist(path)
        .map_err(|error| io_problem("project.recents_write", path, error.error))?;
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::project_path;
    use super::*;
    use std::{fs, path::PathBuf};

    fn setup_project(root: &Path, name: &str) {
        super::super::create_project(root, name).unwrap();
    }

    #[test]
    fn inspect_reports_missing_setup_ready_conversion_and_refused_without_writing() {
        let folder = tempfile::tempdir().unwrap();
        let absent = folder.path().join("gone");
        assert_eq!(inspect(&absent).status, ProjectInspectionStatus::Missing);

        let setup = folder.path().join("setup");
        fs::create_dir_all(&setup).unwrap();
        let before = fs::read_dir(&setup).unwrap().count();
        assert!(matches!(
            inspect(&setup).status,
            ProjectInspectionStatus::SetupRequired { .. }
        ));
        assert_eq!(fs::read_dir(&setup).unwrap().count(), before);

        let ready = folder.path().join("ready");
        setup_project(&ready, "Ready");
        let inspected = inspect(&ready);
        let ProjectInspectionStatus::Ready { name, start_level } = inspected.status else {
            panic!("unexpected inspection: {inspected:?}");
        };
        assert_eq!(name, "Ready");
        assert!(start_level.is_file());

        let refused = folder.path().join("refused");
        fs::create_dir_all(&refused).unwrap();
        fs::write(project_path(&refused), br#"{"schema":999}"#).unwrap();
        assert!(matches!(
            inspect(&refused).status,
            ProjectInspectionStatus::Refused { ref problem }
                if problem.code == "project.schema_unsupported"
        ));
    }

    #[test]
    fn startup_context_allows_missing_start_level_for_repair_and_reports_canonical_paths() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("project");
        setup_project(&root, "Context");
        let config = super::super::load_project(&root).unwrap();
        let canonical_root = fs::canonicalize(&root).unwrap();
        let missing_start_level = super::super::start_level_path(&canonical_root, &config).unwrap();
        fs::remove_file(&missing_start_level).unwrap();
        let inspected = inspect(&root);
        assert!(matches!(
            inspected.status,
            ProjectInspectionStatus::MissingStartLevel {
                ref name,
                ref start_level,
            } if name == "Context" && start_level == &missing_start_level
        ));
        let context = startup_context_using(&root, None).unwrap();
        assert!(context.project_root.is_absolute());
        assert_eq!(context.project_root, fs::canonicalize(&root).unwrap());
        assert_eq!(context.game_root, None);
        assert_eq!(context.runtime_source, RuntimeLocationSource::Unconfigured);
        assert_eq!(context.package_root, context.project_root.join("cooked"));
        assert_eq!(context.start_level, missing_start_level);
        assert!(!context.start_level_exists);
        assert!(!context.start_level.exists());
        assert!(matches!(
            context.runtime,
            GameRuntimeAvailability::Unconfigured
        ));

        fs::write(
            &context.start_level,
            serde_json::to_vec(&scene_data::current_map()).unwrap(),
        )
        .unwrap();
        assert!(
            startup_context_using(&root, None)
                .unwrap()
                .start_level_exists
        );

        let absent = folder.path().join("absent");
        assert_eq!(
            startup_context(&absent).unwrap_err().code,
            "project.folder_missing"
        );
        let not_ready = folder.path().join("not-ready");
        fs::create_dir_all(&not_ready).unwrap();
        assert_eq!(
            startup_context(&not_ready).unwrap_err().code,
            "project.setup_required"
        );
    }

    #[cfg(unix)]
    #[test]
    fn inspect_follows_a_selected_symlink_to_a_project_folder() {
        use std::os::unix::fs::symlink;
        let folder = tempfile::tempdir().unwrap();
        let actual = folder.path().join("actual");
        setup_project(&actual, "Target Project");
        let link = folder.path().join("selected");
        symlink(&actual, &link).unwrap();

        let inspection = inspect(&link);
        let canonical = fs::canonicalize(&actual).unwrap();
        assert_eq!(
            inspection,
            ProjectInspection {
                root: link,
                status: ProjectInspectionStatus::Ready {
                    name: "Target Project".into(),
                    start_level: super::super::load_project(&canonical)
                        .and_then(|config| super::super::start_level_path(&canonical, &config))
                        .unwrap(),
                },
            }
        );
    }

    #[test]
    fn a_directory_at_the_configured_start_level_path_is_refused_not_treated_as_missing() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("project");
        setup_project(&root, "Not a File");
        let config = super::super::load_project(&root).unwrap();
        let canonical_root = fs::canonicalize(&root).unwrap();
        let start_level = super::super::start_level_path(&canonical_root, &config).unwrap();
        fs::remove_file(&start_level).unwrap();
        fs::create_dir(&start_level).unwrap();

        let inspection = inspect(&root);
        assert!(matches!(
            inspection.status,
            ProjectInspectionStatus::Refused { ref problem }
                if problem.code == "project.not_a_file"
                    && problem.parameters.get("path") == Some(&start_level.display().to_string())
        ));
        assert_eq!(
            startup_context(&root).unwrap_err().code,
            "project.not_a_file"
        );
    }

    #[test]
    fn runtime_availability_lists_each_missing_requirement_and_accepts_complete_runtime() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        let GameRuntimeAvailability::Missing { requirements } = game_runtime(root) else {
            panic!("empty folder should not have a runtime");
        };
        assert_eq!(requirements.len(), 4);
        assert_eq!(
            requirements[0].kind,
            GameRuntimeRequirementKind::ServerExecutable
        );
        assert!(requirements[0]
            .path
            .ends_with(format!("server{}", std::env::consts::EXE_SUFFIX)));
        assert_eq!(
            requirements[1].kind,
            GameRuntimeRequirementKind::ClientExecutable
        );
        assert_eq!(
            requirements[2].kind,
            GameRuntimeRequirementKind::AssetsDirectory
        );
        assert_eq!(
            requirements[3].kind,
            GameRuntimeRequirementKind::PipCharacterModel
        );

        let target = root.join("target/debug");
        fs::create_dir_all(&target).unwrap();
        fs::write(
            target.join(format!("server{}", std::env::consts::EXE_SUFFIX)),
            b"server",
        )
        .unwrap();
        fs::write(
            target.join(format!("client{}", std::env::consts::EXE_SUFFIX)),
            b"client",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for name in [
                format!("server{}", std::env::consts::EXE_SUFFIX),
                format!("client{}", std::env::consts::EXE_SUFFIX),
            ] {
                fs::set_permissions(target.join(name), fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        fs::create_dir_all(root.join("assets")).unwrap();
        let model = root.join("assets/characters/pip/pip_robe_lod0.glb");
        fs::create_dir_all(model.parent().unwrap()).unwrap();
        fs::write(
            model,
            [b"glTF".as_slice(), &[2, 0, 0, 0], &[12, 0, 0, 0]].concat(),
        )
        .unwrap();
        assert_eq!(game_runtime(root), GameRuntimeAvailability::Ready);
    }

    #[test]
    fn startup_context_uses_injected_editor_runtime_without_changing_project_content_root() {
        let folder = tempfile::tempdir().unwrap();
        let project = folder.path().join("content");
        setup_project(&project, "Content");
        let runtime = folder.path().join("shared-runtime");
        make_runtime(&runtime);
        let preference = folder.path().join("user-config/runtime-install.json");
        crate::runtime::save_runtime_preference(&preference, &runtime).unwrap();

        let context = startup_context_using(&project, Some(&preference)).unwrap();
        assert_eq!(context.project_root, fs::canonicalize(&project).unwrap());
        assert_eq!(context.game_root, Some(fs::canonicalize(&runtime).unwrap()));
        assert_eq!(
            context.runtime_source,
            RuntimeLocationSource::EditorPreference
        );
        assert_eq!(context.runtime, GameRuntimeAvailability::Ready);
        assert_eq!(context.package_root, context.project_root.join("cooked"));

        let second_runtime = folder.path().join("second-runtime");
        make_runtime(&second_runtime);
        crate::runtime::save_runtime_preference(&preference, &second_runtime).unwrap();
        let refreshed = resolve_project_runtime_using(&context.project_root, Some(&preference));
        assert_eq!(
            refreshed.game_root,
            Some(fs::canonicalize(second_runtime).unwrap())
        );
        assert_eq!(
            refreshed.runtime_source,
            RuntimeLocationSource::EditorPreference
        );
        assert_eq!(refreshed.runtime, GameRuntimeAvailability::Ready);
    }

    fn make_runtime(root: &Path) {
        let target = root.join("target/debug");
        fs::create_dir_all(&target).unwrap();
        for name in [
            format!("server{}", std::env::consts::EXE_SUFFIX),
            format!("client{}", std::env::consts::EXE_SUFFIX),
        ] {
            let path = target.join(name);
            fs::write(&path, b"runtime test executable").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        fs::create_dir_all(root.join("assets")).unwrap();
        let model = root.join("assets/characters/pip/pip_robe_lod0.glb");
        fs::create_dir_all(model.parent().unwrap()).unwrap();
        fs::write(
            model,
            [b"glTF".as_slice(), &[2, 0, 0, 0], &[12, 0, 0, 0]].concat(),
        )
        .unwrap();
    }

    #[test]
    fn recents_are_bounded_deduplicated_newest_first_and_removable() {
        let folder = tempfile::tempdir().unwrap();
        let store = folder.path().join("config/recents.json");
        let first = folder.path().join("first");
        let second = folder.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        remember_recent(&store, &first, "First", 100).unwrap();
        remember_recent(&store, &second, "Second", 200).unwrap();
        let rows = remember_recent(&store, &first, "First renamed", 300).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].root, fs::canonicalize(&first).unwrap());
        assert_eq!(rows[0].name, "First renamed");
        assert_eq!(rows[0].last_opened_unix_ms, 300);
        assert_eq!(load_recents(&store).unwrap(), rows);
        assert_eq!(forget_recent(&store, &first).unwrap().len(), 1);

        for index in 0..MAX_RECENT_PROJECTS + 2 {
            let project = folder.path().join(format!("p{index}"));
            fs::create_dir(&project).unwrap();
            remember_recent(&store, &project, &format!("P{index}"), 400 + index as u64).unwrap();
        }
        let bounded = load_recents(&store).unwrap();
        assert_eq!(bounded.len(), MAX_RECENT_PROJECTS);
        assert!(bounded
            .windows(2)
            .all(|pair| { pair[0].last_opened_unix_ms >= pair[1].last_opened_unix_ms }));
    }

    #[test]
    fn unreadable_or_newer_recents_are_never_overwritten() {
        let folder = tempfile::tempdir().unwrap();
        let store = folder.path().join("recents.json");
        let project = folder.path().join("project");
        fs::create_dir(&project).unwrap();
        for bytes in [
            br#"{"schema":1,"projects":["bad"]}"#.as_slice(),
            br#"{"schema":999,"projects":[]}"#.as_slice(),
        ] {
            fs::write(&store, bytes).unwrap();
            let before = fs::read(&store).unwrap();
            assert!(load_recents(&store).is_err());
            assert!(remember_recent(&store, &project, "Project", 1).is_err());
            assert!(forget_recent(&store, &project).is_err());
            assert_eq!(fs::read(&store).unwrap(), before);
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_project_paths_can_be_opened_but_not_remembered() {
        use std::os::unix::ffi::OsStringExt;
        let root = PathBuf::from(std::ffi::OsString::from_vec(vec![b'p', 0xff]));
        let error = recent_root_key(&root).unwrap_err();
        assert_eq!(error.code, "project.recent_path_invalid");
    }
}
