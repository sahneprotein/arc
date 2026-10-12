//! Project manifest I/O, first-project bootstrap and explicit, backed-up conversion.
//!
//! Project setup is an explicit operation. Merely opening an older project never changes
//! its files; conversion keeps the original bytes in a schema-named backup first.
//!
//! What is read and written stays inside the project's folder, as the file system resolves
//! it: a manifest, a level or a levels folder that is a link to somewhere else is refused,
//! not followed. Nothing is made before what is asked for has been found valid, so a setup
//! that is refused leaves the folder as it was.
//!
//! A project that is set up has what the game needs of a project's folder: the manifest, a
//! level to start with and the look profile the manifest names. A level and a profile that
//! are there are kept; one that is missing is made, the level as the starter arena
//! (`scene_data::starter_level`) under an ID of its own and the profile as the default one
//! ([`DEFAULT_LOOK_PROFILE`]). The game's programs and its own assets are not part of a
//! project and are not made here.
//!
//! A new project is made in a folder of its own ([`create_project_in`]): a folder named
//! after the project, inside the one the person chose, and only where nothing has that
//! name. So a new project never fills a folder that holds other things.

use crate::persistence::PathLock;
use project_spec::{ProjectConfig, ProjectError};
use scene_data::{Diagnostic, SceneDocument, StableId};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    hash::{BuildHasher, Hasher},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

mod settings;
pub use settings::*;
mod catalog;
pub use catalog::*;

pub const PROJECT_FILE: &str = project_spec::FILE_NAME;

/// The largest level file a setup looks into.
const MAX_LEVEL_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectSetup {
    pub config: ProjectConfig,
    pub config_path: PathBuf,
    pub start_level_path: PathBuf,
    pub created_project: bool,
    pub created_level: bool,
    /// Whether the look profile the manifest names was made: it was not in the folder.
    pub created_profile: bool,
}

/// A level file of the folder that a project cannot start with, and why: the refusal's code
/// and what the reader said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RejectedLevel {
    pub path: PathBuf,
    pub code: String,
    pub detail: String,
}

/// What a setup of a folder would do, found by reading only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupPlan {
    /// The level the project would start with: the first of the folder's levels, by name,
    /// that can be read.
    pub existing: Option<PathBuf>,
    /// The folder's level files that cannot be read as levels, by name. None of them is
    /// changed by a setup.
    pub rejected: Vec<RejectedLevel>,
    /// The starter level a setup would make if it made one: a name no file has.
    pub starter: PathBuf,
    /// The look profile a setup would make: the one a new manifest names, when nothing in
    /// the folder has that name. `None` when something has, which is then kept as it is.
    pub profile: Option<PathBuf>,
}

impl SetupPlan {
    /// Whether a setup would make a fresh starter level although the folder has level
    /// files: all of them were rejected. That is the person's to agree to
    /// ([`FreshStarter::Agreed`]); a setup does not pass over a level by itself.
    pub fn needs_agreement(&self) -> bool {
        self.existing.is_none() && !self.rejected.is_empty()
    }
}

/// Whether a setup may make a fresh starter level beside level files it rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreshStarter {
    /// Only in a folder without level files. With rejected ones the setup is refused
    /// (`project.levels_rejected`) and nothing is made.
    OnlyWithoutLevels,
    /// The person was shown the rejected files and agreed to a fresh starter beside them.
    Agreed,
}

pub fn project_path(root: &Path) -> PathBuf {
    root.join(PROJECT_FILE)
}

pub fn load_project(root: &Path) -> Result<ProjectConfig, Diagnostic> {
    let path = project_path(root);
    inside(root, &path)?;
    let bytes = read_limited(&path, project_spec::MAX_FILE_BYTES)?;
    project_spec::decode(&bytes).map_err(|error| project_error(&path, error))
}

/// The level a project starts with, as a path in the project's folder. Refused
/// (`project.outside_folder`) when the path leads out of the folder through a link. Whether
/// the file is there is the caller's to look at.
pub fn start_level_path(root: &Path, config: &ProjectConfig) -> Result<PathBuf, Diagnostic> {
    let path = root.join(&config.startup.start_level);
    inside(root, &path)?;
    Ok(path)
}

/// What a setup of this folder would do. Reads only.
pub fn plan_setup(root: &Path) -> Result<SetupPlan, Diagnostic> {
    let folder = root.join("levels");
    let mut files: Vec<PathBuf> = match fs::read_dir(&folder) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.ends_with(".level.json"))
            })
            .map(|entry| entry.path())
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(io_problem("project.read", &folder, error)),
    };
    files.sort();
    let profile = root.join(ProjectConfig::new("").rendering.profile);
    let mut plan = SetupPlan {
        existing: None,
        rejected: Vec::new(),
        starter: next_starter_path(root),
        // Anything of the name is kept, a link that leads nowhere too.
        profile: fs::symlink_metadata(&profile).is_err().then_some(profile),
    };
    for path in files {
        match level_problem(root, &path) {
            None if plan.existing.is_none() => plan.existing = Some(path),
            None => {}
            Some((code, detail)) => plan.rejected.push(RejectedLevel { path, code, detail }),
        }
    }
    Ok(plan)
}

/// Creates the project manifest, a first playable starter level and the look profile the
/// manifest names, without replacing anything. If a level already exists, it becomes the
/// selected start level instead of being overwritten; a look profile that is there is kept.
/// A folder whose level files are all rejected is not set up: see [`create_project_with`].
pub fn create_project(root: &Path, name: &str) -> Result<ProjectSetup, Diagnostic> {
    create_project_with(root, name, FreshStarter::OnlyWithoutLevels)
}

/// [`create_project`], saying whether a fresh starter level may be made beside level files
/// that were rejected.
///
/// Nothing is made before the name and the whole manifest have been found valid. A starter
/// level and a look profile that were made are taken away again if the manifest cannot be
/// written after them.
pub fn create_project_with(
    root: &Path,
    name: &str,
    starter: FreshStarter,
) -> Result<ProjectSetup, Diagnostic> {
    let config_path = project_path(root);
    // The name first, before the folder is so much as made.
    let mut config = ProjectConfig::new(name);
    project_spec::validate(&config).map_err(|error| project_error(&config_path, error))?;
    fs::create_dir_all(root).map_err(|error| io_problem("project.create_folder", root, error))?;
    if config_path.exists() {
        let config = load_project(root)?;
        let level = start_level_path(root, &config)?;
        if !level.is_file() {
            return Err(
                Diagnostic::new("project.start_level_missing").with("path", level.display())
            );
        }
        return Ok(ProjectSetup {
            config,
            config_path,
            start_level_path: level,
            created_project: false,
            created_level: false,
            created_profile: false,
        });
    }

    let plan = plan_setup(root)?;
    if plan.needs_agreement() && starter != FreshStarter::Agreed {
        let first = &plan.rejected[0];
        return Err(Diagnostic::new("project.levels_rejected")
            .with("count", plan.rejected.len())
            .with("path", first.path.display())
            .with("code", &first.code)
            .with("detail", &first.detail));
    }
    let created_level = plan.existing.is_none();
    let level_path = plan.existing.unwrap_or(plan.starter);
    inside(root, &level_path)?;
    inside(root, &config_path)?;
    if let Some(profile) = &plan.profile {
        inside(root, profile)?;
    }
    config.startup.start_level = relative_string(root, &level_path)?;
    // The whole manifest, before anything is written.
    let bytes = project_spec::canonical_bytes(&config)
        .map_err(|error| project_error(&config_path, error))?;
    // What was made for this project is taken away again when there is no project.
    let take_back = |level: bool, profile: bool| {
        if level {
            let _ = fs::remove_file(&level_path);
        }
        if let Some(path) = plan.profile.as_ref().filter(|_| profile) {
            let _ = fs::remove_file(path);
        }
    };
    if created_level {
        create_starter_level(&level_path)?;
    }
    let created_profile = plan.profile.is_some();
    if created_profile && let Err(refusal) = install_default_look(root, &config.rendering.profile) {
        take_back(created_level, false);
        return Err(refusal);
    }
    if let Err(refusal) = create_exclusive(&config_path, &bytes) {
        take_back(created_level, created_profile);
        return Err(refusal);
    }
    Ok(ProjectSetup {
        config,
        config_path,
        start_level_path: level_path,
        created_project: true,
        created_level,
        created_profile,
    })
}

/// The folders a setup makes in a folder that had none of them, deepest first: what is
/// taken away again, when it is empty, after a new project could not be made.
const MADE_FOLDERS: [&str; 3] = ["levels", "assets/render", "assets"];

/// The name of the folder a new project called `name` is made in: the name as it is, with
/// what a folder's name cannot have on some system (`/ \ : * ? " < > |` and control
/// characters) written as `-`, and without spaces and dots at its ends. Refused
/// (`project.folder_name`) when nothing is left, and for a name a system keeps for itself.
pub fn project_folder_name(name: &str) -> Result<String, Diagnostic> {
    const KEPT: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let written: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let folder = written.trim_matches(|c: char| c.is_whitespace() || c == '.');
    let stem = folder.split('.').next().unwrap_or_default().trim_end();
    if folder.is_empty() || KEPT.iter().any(|kept| kept.eq_ignore_ascii_case(stem)) {
        return Err(Diagnostic::new("project.folder_name").with("name", name.trim()));
    }
    Ok(folder.to_owned())
}

/// Where a new project called `name` would be made inside `parent`. Reads only.
///
/// Refused when the name is none a project can have (`project.value`) or leaves no folder
/// name (`project.folder_name`), when `parent` is not a folder that is there
/// (`project.parent_missing`), and when anything inside it has the folder's name already,
/// a file or a link that leads nowhere too (`project.folder_taken`).
pub fn new_project_folder(parent: &Path, name: &str) -> Result<PathBuf, Diagnostic> {
    let name = name.trim();
    let config = ProjectConfig::new(name);
    project_spec::validate(&config)
        .map_err(|error| project_error(&parent.join(PROJECT_FILE), error))?;
    let folder_name = project_folder_name(name)?;
    let parent = fs::canonicalize(parent)
        .ok()
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| Diagnostic::new("project.parent_missing").with("path", parent.display()))?;
    let folder = parent.join(folder_name);
    if fs::symlink_metadata(&folder).is_ok() {
        return Err(Diagnostic::new("project.folder_taken").with("path", folder.display()));
    }
    Ok(folder)
}

/// Makes a new project called `name` in a folder of its own inside `parent`
/// ([`new_project_folder`]): the folder, and in it the manifest, the starter level and the
/// default look profile. `parent` itself is given nothing but that one folder.
///
/// Nothing that is there is written over or entered: a folder of that name that another
/// program makes meanwhile refuses it too (`project.folder_taken`). If the project cannot
/// be made in the new folder, what was made is taken away again, the folder too.
pub fn create_project_in(parent: &Path, name: &str) -> Result<ProjectSetup, Diagnostic> {
    let name = name.trim();
    let folder = new_project_folder(parent, name)?;
    // Made as a new folder or not at all: one that is there is not entered.
    fs::create_dir(&folder).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Diagnostic::new("project.folder_taken").with("path", folder.display())
        } else {
            io_problem("project.create_folder", &folder, error)
        }
    })?;
    create_project_with(&folder, name, FreshStarter::OnlyWithoutLevels).inspect_err(|_| {
        // Only folders that are empty go: the files were taken away where they were made,
        // and anything another program put there stays.
        for made in MADE_FOLDERS {
            let _ = fs::remove_dir(folder.join(made));
        }
        let _ = fs::remove_dir(&folder);
    })
}

/// Converts an older project manifest only when explicitly requested. The source bytes are
/// copied to `<manifest>.schema-<old>.bak` with create-new semantics before replacement.
///
/// The manifest is held for the conversion with a lock file beside it, which another editor
/// that converts it cannot make meanwhile (`project.locked`), and it is read once more just
/// before it is replaced: if it is no longer the bytes that were converted, nothing is
/// replaced, the backup that was just made is taken away again, and the refusal is
/// `project.changed`. A manifest of a schema that has no converter is refused
/// (`project.schema_unsupported`) and nothing is written.
pub fn convert_project(root: &Path) -> Result<ProjectConfig, Diagnostic> {
    convert_with(root, || {})
}

/// [`convert_project`], with something done between the backup and the replacement: for a
/// test to stand in for another program that writes the manifest just then.
fn convert_with(root: &Path, before_replace: impl FnOnce()) -> Result<ProjectConfig, Diagnostic> {
    let path = project_path(root);
    inside(root, &path)?;
    let _held = hold(&path)?;
    let bytes = read_limited(&path, project_spec::MAX_FILE_BYTES)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        Diagnostic::new("project.decode")
            .with("path", path.display())
            .with("detail", error)
    })?;
    let old_schema = value
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .and_then(|schema| u32::try_from(schema).ok())
        .ok_or_else(|| Diagnostic::new("project.schema_missing").with("path", path.display()))?;
    let config = project_spec::convert(value).map_err(|error| project_error(&path, error))?;
    if old_schema == config.schema {
        return Ok(config);
    }
    let converted =
        project_spec::canonical_bytes(&config).map_err(|error| project_error(&path, error))?;
    let mut backup_name = path
        .file_name()
        .ok_or_else(|| Diagnostic::new("project.path_invalid"))?
        .to_os_string();
    backup_name.push(format!(".schema-{old_schema}.bak"));
    let backup = path.with_file_name(backup_name);
    inside(root, &backup)?;
    create_exclusive(&backup, &bytes)?;
    before_replace();
    // Still the bytes that were converted? A program that does not make the lock file may
    // have written the manifest since it was read.
    let unchanged = read_limited(&path, project_spec::MAX_FILE_BYTES).is_ok_and(|now| now == bytes);
    if !unchanged {
        // The backup is of what was read, which is no longer the manifest.
        let _ = fs::remove_file(&backup);
        return Err(Diagnostic::new("project.changed").with("path", path.display()));
    }
    replace_atomic(&path, &converted)?;
    Ok(config)
}

/// Holds the manifest for one conversion: a file of the same name with `.lock` after it,
/// which is gone again when what is returned is dropped. An editor that stops in the middle
/// leaves it behind, and it is then removed by hand.
fn hold(path: &Path) -> Result<PathLock, Diagnostic> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    let lock = path.with_file_name(name);
    PathLock::acquire(lock.clone()).map_err(|refusal| {
        if refusal.code == "save.locked" {
            Diagnostic::new("project.locked")
                .with("path", path.display())
                .with("lock", lock.display())
        } else {
            let detail = refusal.parameters.get("detail").unwrap_or(&refusal.code);
            Diagnostic::new("project.write")
                .with("path", path.display())
                .with("detail", detail)
        }
    })
}

/// Refuses a path that is not in the project's folder as the file system resolves it: the
/// folder and the path are followed through every link, and what does not exist yet is
/// taken as it is named under the last part that does. `path` is one that was made from
/// `root` and names nothing above it; this is about links.
fn inside(root: &Path, path: &Path) -> Result<(), Diagnostic> {
    let outside = || Diagnostic::new("project.outside_folder").with("path", path.display());
    let folder = match fs::canonicalize(root) {
        Ok(folder) => folder,
        // A folder that is not there yet has no links in it.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_problem("project.read", root, error)),
    };
    let mut there = path;
    let mut missing = Vec::new();
    while fs::symlink_metadata(there).is_err() {
        let (Some(parent), Some(name)) = (there.parent(), there.file_name()) else {
            return Err(outside());
        };
        missing.push(name);
        there = parent;
    }
    // A link that leads nowhere is not followed either.
    let mut resolved = fs::canonicalize(there).map_err(|_| outside())?;
    resolved.extend(missing.iter().rev());
    if resolved.starts_with(&folder) {
        Ok(())
    } else {
        Err(outside())
    }
}

fn next_starter_path(root: &Path) -> PathBuf {
    let folder = root.join("levels");
    for suffix in 1..=u32::MAX {
        let name = if suffix == 1 {
            "first-level.level.json".to_owned()
        } else {
            format!("first-level-{suffix}.level.json")
        };
        let candidate = folder.join(name);
        // A name that anything has, a link that leads nowhere too, is taken.
        if fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
    }
    folder.join("first-level.level.json")
}

/// Why this level file cannot be what a project starts with: the refusal's code and what
/// the reader said. `None` for a level that can be read.
fn level_problem(root: &Path, path: &Path) -> Option<(String, String)> {
    let said = |refusal: &Diagnostic| {
        (
            refusal.code.clone(),
            refusal
                .parameters
                .get("detail")
                .cloned()
                .unwrap_or_default(),
        )
    };
    if let Err(refusal) = inside(root, path) {
        return Some(said(&refusal));
    }
    if !path.is_file() {
        return Some(("project.not_a_file".into(), String::new()));
    }
    let bytes = match read_limited(path, MAX_LEVEL_BYTES) {
        Ok(bytes) => bytes,
        Err(refusal) => return Some(said(&refusal)),
    };
    let document = match serde_json::from_slice::<SceneDocument>(&bytes) {
        Ok(document) => document,
        Err(error) => return Some(("source.decode".into(), error.to_string())),
    };
    scene_data::validate(&document)
        .err()
        .map(|refusal| (refusal.code.clone(), refusal.to_string()))
}

/// The level a new project starts with, under an ID of its own: the starter arena, as a
/// setup writes it. For whoever shows a project a level before it has one of its own.
pub fn starter_level() -> SceneDocument {
    scene_data::starter_level(fresh_level_id())
}

fn create_starter_level(path: &Path) -> Result<(), Diagnostic> {
    let parent = path
        .parent()
        .ok_or_else(|| Diagnostic::new("project.path_invalid"))?;
    fs::create_dir_all(parent)
        .map_err(|error| io_problem("project.create_folder", parent, error))?;
    let document = starter_level();
    scene_data::validate(&document)
        .map_err(|error| Diagnostic::new("project.starter_invalid").with("detail", error))?;
    let mut bytes = serde_json::to_vec_pretty(&document)
        .map_err(|error| Diagnostic::new("project.encode").with("detail", error))?;
    bytes.push(b'\n');
    create_exclusive(path, &bytes)
}

/// An ID for a level that is made here, which no other level is likely to have: from the
/// time, this process, a count of the IDs it made and what the system gives each program
/// to keep its tables apart, which is different every time a program is started.
fn fresh_level_id() -> StableId {
    static MADE: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let chance = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    let mut hash = Sha256::new();
    hash.update(b"arc level id");
    hash.update(nanos.to_le_bytes());
    hash.update(std::process::id().to_le_bytes());
    hash.update(MADE.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    hash.update(chance.to_le_bytes());
    let digest = hash.finalize();
    let mut bits = [0u8; 16];
    bits.copy_from_slice(&digest[..16]);
    // Never the one ID no level may have.
    StableId::from_u128(u128::from_le_bytes(bits) | 1)
}

fn read_limited(path: &Path, limit: usize) -> Result<Vec<u8>, Diagnostic> {
    let mut file = File::open(path).map_err(|error| io_problem("project.read", path, error))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_problem("project.read", path, error))?;
    if bytes.len() > limit {
        return Err(Diagnostic::new("project.limit")
            .with("path", path.display())
            .with("max_bytes", limit));
    }
    Ok(bytes)
}

fn create_exclusive(path: &Path, bytes: &[u8]) -> Result<(), Diagnostic> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| io_problem("project.create_folder", parent, error))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                Diagnostic::new("project.conflict").with("path", path.display())
            } else {
                io_problem("project.write", path, error)
            }
        })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| io_problem("project.write", path, error))
}

fn replace_atomic(path: &Path, bytes: &[u8]) -> Result<(), Diagnostic> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| io_problem("project.write", parent, error))?;
    temp.write_all(bytes)
        .and_then(|()| temp.as_file().sync_all())
        .map_err(|error| io_problem("project.write", path, error))?;
    temp.persist(path)
        .map_err(|error| io_problem("project.write", path, error.error))?;
    Ok(())
}

fn relative_string(root: &Path, path: &Path) -> Result<String, Diagnostic> {
    path.strip_prefix(root)
        .map_err(|_| Diagnostic::new("project.path_invalid").with("path", path.display()))
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

/// A refusal of the manifest's format as a diagnostic. A manifest of another schema says
/// which (`schema`), and whether this editor can convert it: an older schema it has a
/// converter for is `project.convert_required`; any other, a newer one above all, is
/// `project.schema_unsupported`, for which no conversion is to be offered.
fn project_error(path: &Path, error: ProjectError) -> Diagnostic {
    let (code, schema) = match &error {
        ProjectError::TooLarge => ("project.limit", None),
        ProjectError::Decode(_) => ("project.decode", None),
        ProjectError::Schema { found, .. } if project_spec::can_convert(*found) => {
            ("project.convert_required", Some(*found))
        }
        ProjectError::Schema { found, .. } | ProjectError::NoConverter(found) => {
            ("project.schema_unsupported", Some(*found))
        }
        ProjectError::Value(_) | ProjectError::SharedKey { .. } => ("project.value", None),
    };
    let refusal = Diagnostic::new(code)
        .with("path", path.display())
        .with("detail", &error);
    // Which setting it is about, and for a key two actions share, the other one.
    let refusal = match &error {
        ProjectError::Value(field) => refusal.with("field", field),
        ProjectError::SharedKey { first, second } => {
            refusal.with("field", second).with("shares_with", first)
        }
        _ => refusal,
    };
    match schema {
        Some(schema) => refusal
            .with("schema", schema)
            .with("expected", project_spec::SCHEMA),
        None => refusal,
    }
}

fn io_problem(code: &str, path: &Path, error: std::io::Error) -> Diagnostic {
    Diagnostic::new(code)
        .with("path", path.display())
        .with("detail", error)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything under `root`, by path, for saying that nothing was made or changed.
    fn files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(folder: &Path, found: &mut Vec<(PathBuf, Vec<u8>)>) {
            let Ok(entries) = fs::read_dir(folder) else {
                return;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, found);
                } else {
                    found.push((path.clone(), fs::read(&path).unwrap_or_default()));
                }
            }
        }
        let mut found = Vec::new();
        walk(root, &mut found);
        found.sort();
        found
    }

    /// A manifest of this schema with the current fields, written as an older or newer
    /// editor might have written it.
    fn manifest_of_schema(root: &Path, schema: u32) -> Vec<u8> {
        let config = ProjectConfig::new("Arc Test");
        let bytes = if schema == 1 {
            serde_json::to_vec_pretty(&project_spec::ProjectConfigV1 {
                schema: 1,
                project: config.project,
                startup: config.startup,
                rendering: config.rendering,
                controls: config.controls,
            })
            .unwrap()
        } else {
            let mut value = serde_json::to_value(&config).unwrap();
            value["schema"] = schema.into();
            serde_json::to_vec_pretty(&value).unwrap()
        };
        fs::write(project_path(root), &bytes).unwrap();
        bytes
    }

    #[test]
    fn first_setup_creates_manifest_and_playable_starter_without_replacing_existing_files() {
        let folder = tempfile::tempdir().unwrap();
        let project = create_project(folder.path(), "Arc Test").unwrap();
        assert!(project.created_project && project.created_level && project.created_profile);
        assert!(project.start_level_path.is_file());
        assert_eq!(load_project(folder.path()).unwrap(), project.config);
        // The look profile the manifest names is in the folder, and is the default one.
        let profile = folder.path().join(&project.config.rendering.profile);
        assert_eq!(fs::read(&profile).unwrap(), DEFAULT_LOOK_PROFILE);
        assert!(look_profile(folder.path(), &project.config.rendering.profile).is_ok());

        let level_bytes = fs::read(&project.start_level_path).unwrap();
        let again = create_project(folder.path(), "A Different Name").unwrap();
        assert!(!again.created_project && !again.created_level && !again.created_profile);
        assert_eq!(fs::read(&project.start_level_path).unwrap(), level_bytes);
        assert_eq!(again.config.project.name, "Arc Test");
    }

    fn level_at(path: &Path) -> SceneDocument {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn the_starter_is_the_arena_under_an_id_of_its_own_and_can_be_cooked_and_played() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let one = level_at(
            &create_project(first.path(), "One")
                .unwrap()
                .start_level_path,
        );
        let two = level_at(
            &create_project(second.path(), "Two")
                .unwrap()
                .start_level_path,
        );
        // The same arena, and each project's level is a level of its own.
        assert_eq!(one.objects, two.objects);
        assert!(one.id.valid() && two.id.valid());
        assert_ne!(one.id, two.id);
        assert_eq!(one, scene_data::starter_level(one.id.clone()));
        // Not the game's old map.
        let sample = scene_data::current_map();
        assert_ne!(one.id, sample.id);
        assert_ne!(one.objects, sample.objects);
        // Nothing stops it from being cooked and played as it is.
        let problems = editor_core::review_document(&one);
        assert!(problems.is_empty(), "{}", problems.len());
        editor_cook::cook(&one).unwrap();
    }

    #[test]
    fn a_setup_keeps_a_look_profile_that_is_there_and_says_what_it_would_make() {
        let folder = tempfile::tempdir().unwrap();
        let named = folder.path().join("assets/render/look-v1.json");
        let plan = plan_setup(folder.path()).unwrap();
        assert_eq!(plan.profile, Some(named.clone()));
        // One of the folder's own, which the look reader would not even take.
        fs::create_dir_all(named.parent().unwrap()).unwrap();
        fs::write(&named, b"the folder's own").unwrap();
        assert_eq!(plan_setup(folder.path()).unwrap().profile, None);
        let project = create_project(folder.path(), "Arc Test").unwrap();
        assert!(project.created_project && !project.created_profile);
        assert_eq!(fs::read(&named).unwrap(), b"the folder's own");
    }

    #[cfg(unix)]
    #[test]
    fn a_setup_whose_manifest_cannot_be_written_takes_back_the_level_and_the_profile() {
        use std::os::unix::fs::PermissionsExt;
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        // The level and the profile can be made, in folders that are there, and the
        // manifest cannot: nothing may be made in the folder itself.
        fs::create_dir_all(root.join("levels")).unwrap();
        fs::create_dir_all(root.join("assets/render")).unwrap();
        let allow = |mode: u32| fs::set_permissions(root, fs::Permissions::from_mode(mode));
        allow(0o555).unwrap();
        let refused = create_project(root, "Arc Test");
        allow(0o755).unwrap();
        // Whoever may write anywhere is not refused, and then there is nothing to see.
        if let Err(refused) = refused {
            assert_eq!(refused.code, "project.write");
            assert!(files(root).is_empty());
        }
    }

    #[test]
    fn a_new_project_is_made_in_a_folder_of_its_own_inside_the_chosen_one() {
        let chosen = tempfile::tempdir().unwrap();
        let parent = chosen.path().canonicalize().unwrap();
        fs::write(parent.join("notes.txt"), b"the person's").unwrap();
        assert_eq!(
            new_project_folder(&parent, " Harbour Duel ").unwrap(),
            parent.join("Harbour Duel")
        );
        assert_eq!(files(&parent).len(), 1);

        let project = create_project_in(&parent, " Harbour Duel ").unwrap();
        let root = parent.join("Harbour Duel");
        assert_eq!(project.config.project.name, "Harbour Duel");
        assert_eq!(project.config_path, project_path(&root));
        assert!(project.created_project && project.created_level && project.created_profile);
        // The chosen folder has the new one and what it had, nothing more.
        let mut inside: Vec<_> = fs::read_dir(&parent)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        inside.sort();
        assert_eq!(inside, ["Harbour Duel", "notes.txt"]);
        assert_eq!(fs::read(parent.join("notes.txt")).unwrap(), b"the person's");
        let made: Vec<_> = files(&root)
            .into_iter()
            .map(|(path, _)| relative_string(&root, &path).unwrap())
            .collect();
        assert_eq!(
            made,
            [
                "arc-project.json",
                "assets/render/look-v1.json",
                "levels/first-level.level.json"
            ]
        );
        assert_eq!(load_project(&root).unwrap(), project.config);
    }

    #[test]
    fn a_new_project_is_not_made_where_anything_has_its_folders_name() {
        let chosen = tempfile::tempdir().unwrap();
        let parent = chosen.path().canonicalize().unwrap();
        // A folder with something in it, a file, and a project that is there already.
        fs::create_dir(parent.join("Taken")).unwrap();
        fs::write(parent.join("Taken/keep.txt"), b"kept").unwrap();
        fs::write(parent.join("A file"), b"kept").unwrap();
        create_project_in(&parent, "Made").unwrap();
        let before = files(&parent);
        for name in ["Taken", "A file", "Made"] {
            let refused = create_project_in(&parent, name).unwrap_err();
            assert_eq!(refused.code, "project.folder_taken", "{name}");
            assert_eq!(
                refused.parameters["path"],
                parent.join(name).display().to_string()
            );
            assert_eq!(
                new_project_folder(&parent, name).unwrap_err().code,
                "project.folder_taken"
            );
        }
        // An empty folder of the name is not entered either: it is not this project's.
        fs::create_dir(parent.join("Empty")).unwrap();
        assert_eq!(
            create_project_in(&parent, "Empty").unwrap_err().code,
            "project.folder_taken"
        );
        assert!(parent.join("Empty").is_dir());
        assert_eq!(files(&parent), before);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(parent.join("nowhere"), parent.join("Linked")).unwrap();
            assert_eq!(
                create_project_in(&parent, "Linked").unwrap_err().code,
                "project.folder_taken"
            );
            assert!(!parent.join("nowhere").exists());
        }
    }

    #[test]
    fn a_new_projects_folder_is_named_after_it_as_far_as_a_folder_can_be() {
        assert_eq!(project_folder_name("Harbour Duel").unwrap(), "Harbour Duel");
        assert_eq!(project_folder_name("  a/b\\c: d?  ").unwrap(), "a-b-c- d-");
        assert_eq!(project_folder_name("..hidden.").unwrap(), "hidden");
        assert_eq!(project_folder_name("tab\there").unwrap(), "tab-here");
        for name in [
            "", "  ", ".", "..", " . . ", "con", "NUL", "com1.txt", "Lpt9",
        ] {
            let refused = project_folder_name(name).unwrap_err();
            assert_eq!(refused.code, "project.folder_name", "{name:?}");
        }
        assert_eq!(project_folder_name("console").unwrap(), "console");

        let chosen = tempfile::tempdir().unwrap();
        let before = files(chosen.path());
        // A name no project can have, a name that leaves no folder name, and a chosen
        // folder that is not there: nothing is made for any of them.
        for (name, code) in [("  ", "project.value"), ("..", "project.folder_name")] {
            let refused = create_project_in(chosen.path(), name).unwrap_err();
            assert_eq!(refused.code, code, "{name:?}");
        }
        let gone = chosen.path().join("gone");
        let refused = create_project_in(&gone, "Game").unwrap_err();
        assert_eq!(refused.code, "project.parent_missing");
        assert!(!gone.exists());
        // Nor inside a file.
        fs::write(chosen.path().join("file"), b"").unwrap();
        let refused = create_project_in(&chosen.path().join("file"), "Game").unwrap_err();
        assert_eq!(refused.code, "project.parent_missing");
        fs::remove_file(chosen.path().join("file")).unwrap();
        assert_eq!(files(chosen.path()), before);
    }

    #[test]
    fn setup_selects_an_existing_level_instead_of_overwriting_it() {
        let folder = tempfile::tempdir().unwrap();
        let levels = folder.path().join("levels");
        fs::create_dir_all(&levels).unwrap();
        let path = levels.join("owner.level.json");
        let document = scene_data::current_map();
        let original = serde_json::to_vec_pretty(&document).unwrap();
        fs::write(&path, &original).unwrap();
        let project = create_project(folder.path(), "Arc Test").unwrap();
        assert_eq!(project.start_level_path, path);
        assert_eq!(fs::read(path).unwrap(), original);
        assert!(!project.created_level);
    }

    #[test]
    fn a_name_the_format_refuses_is_refused_before_anything_is_made() {
        let folder = tempfile::tempdir().unwrap();
        // Not even the folder is made for it.
        let absent = folder.path().join("not-yet");
        for name in ["", "   ", &"n".repeat(97)] {
            let refused = create_project(&absent, name).unwrap_err();
            assert_eq!(refused.code, "project.value", "{name:?}");
            assert!(!absent.exists(), "{name:?}");
        }
        // In a folder that is there, with a level of its own: nothing is added.
        let levels = folder.path().join("levels");
        fs::create_dir_all(&levels).unwrap();
        fs::write(levels.join("old.level.json"), b"not a level").unwrap();
        let before = files(folder.path());
        for starter in [FreshStarter::OnlyWithoutLevels, FreshStarter::Agreed] {
            let refused = create_project_with(folder.path(), "  ", starter).unwrap_err();
            assert_eq!(refused.code, "project.value");
            assert_eq!(files(folder.path()), before);
        }
    }

    #[test]
    fn rejected_levels_are_said_with_their_reason_and_a_fresh_starter_needs_agreement() {
        let folder = tempfile::tempdir().unwrap();
        let levels = folder.path().join("levels");
        fs::create_dir_all(&levels).unwrap();
        let old = levels.join("current-map.level.json");
        fs::write(&old, b"not a level").unwrap();
        // A level of a schema this editor does not read: decoded, and then refused.
        let mut future = scene_data::current_map();
        future.schema = 9;
        let newer = levels.join("newer.level.json");
        fs::write(&newer, serde_json::to_vec(&future).unwrap()).unwrap();
        let before = files(folder.path());

        // What a setup would do, by reading only.
        let plan = plan_setup(folder.path()).unwrap();
        assert_eq!(plan.existing, None);
        assert!(plan.needs_agreement());
        assert_eq!(plan.starter, levels.join("first-level.level.json"));
        let said: Vec<_> = plan
            .rejected
            .iter()
            .map(|level| (level.path.clone(), level.code.as_str()))
            .collect();
        assert_eq!(
            said,
            [
                (old.clone(), "source.decode"),
                (newer.clone(), "schema.unsupported")
            ]
        );
        assert!(!plan.rejected[0].detail.is_empty());
        assert_eq!(files(folder.path()), before);

        // Not agreed to: refused, with the first rejected file and its reason, nothing made.
        let refused = create_project(folder.path(), "Arc Test").unwrap_err();
        assert_eq!(refused.code, "project.levels_rejected");
        assert_eq!(refused.parameters["count"], "2");
        assert_eq!(refused.parameters["path"], old.display().to_string());
        assert_eq!(refused.parameters["code"], "source.decode");
        assert_eq!(files(folder.path()), before);

        // Agreed to: a starter beside them, and they are as they were.
        let project = create_project_with(folder.path(), "Arc Test", FreshStarter::Agreed).unwrap();
        assert!(project.created_level);
        assert_eq!(project.start_level_path, plan.starter);
        assert_eq!(fs::read(&old).unwrap(), b"not a level");
        assert_eq!(
            level_problem(folder.path(), &project.start_level_path),
            None
        );
    }

    #[test]
    fn a_level_that_can_be_read_is_taken_whatever_else_is_rejected() {
        let folder = tempfile::tempdir().unwrap();
        let levels = folder.path().join("levels");
        fs::create_dir_all(&levels).unwrap();
        fs::write(levels.join("a-broken.level.json"), b"{").unwrap();
        let good = levels.join("b-good.level.json");
        fs::write(
            &good,
            serde_json::to_vec(&scene_data::current_map()).unwrap(),
        )
        .unwrap();
        let plan = plan_setup(folder.path()).unwrap();
        assert_eq!(plan.existing, Some(good.clone()));
        assert_eq!(plan.rejected.len(), 1);
        assert!(!plan.needs_agreement());
        let project = create_project(folder.path(), "Arc Test").unwrap();
        assert_eq!(project.start_level_path, good);
        assert!(!project.created_level);
    }

    #[test]
    fn an_older_schema_is_to_be_converted_and_a_newer_one_is_not_offered_for_it() {
        let older = tempfile::tempdir().unwrap();
        manifest_of_schema(older.path(), 1);
        let refused = load_project(older.path()).unwrap_err();
        assert_eq!(refused.code, "project.convert_required");
        assert_eq!(refused.parameters["schema"], "1");
        assert_eq!(
            refused.parameters["expected"],
            project_spec::SCHEMA.to_string()
        );

        let newer = tempfile::tempdir().unwrap();
        let bytes = manifest_of_schema(newer.path(), project_spec::SCHEMA + 1);
        let refused = load_project(newer.path()).unwrap_err();
        assert_eq!(refused.code, "project.schema_unsupported");
        assert_eq!(
            refused.parameters["schema"],
            (project_spec::SCHEMA + 1).to_string()
        );
        // Asked to convert it all the same: refused as that, and nothing is written.
        let before = files(newer.path());
        let refused = convert_project(newer.path()).unwrap_err();
        assert_eq!(refused.code, "project.schema_unsupported");
        assert_eq!(files(newer.path()), before);
        assert_eq!(fs::read(project_path(newer.path())).unwrap(), bytes);
    }

    #[test]
    fn explicit_conversion_keeps_a_non_replacing_schema_backup() {
        let folder = tempfile::tempdir().unwrap();
        let bytes = manifest_of_schema(folder.path(), 1);
        let path = project_path(folder.path());
        let converted = convert_project(folder.path()).unwrap();
        assert_eq!(converted.schema, project_spec::SCHEMA);
        assert_eq!(
            fs::read(path.with_file_name("arc-project.json.schema-1.bak")).unwrap(),
            bytes
        );
        assert_eq!(load_project(folder.path()).unwrap(), converted);
        // The conversion let go of the manifest: no lock is left beside it.
        assert!(!path.with_file_name("arc-project.json.lock").exists());
        // Converted, there is nothing more to convert, and no second backup is made.
        assert_eq!(convert_project(folder.path()).unwrap(), converted);
    }

    #[test]
    fn conversion_does_not_replace_an_existing_backup_or_manifest() {
        let folder = tempfile::tempdir().unwrap();
        manifest_of_schema(folder.path(), 1);
        let path = project_path(folder.path());
        let backup = path.with_file_name("arc-project.json.schema-1.bak");
        fs::write(&backup, b"do not replace").unwrap();
        assert_eq!(
            convert_project(folder.path()).unwrap_err().code,
            "project.conflict"
        );
        assert!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&path).unwrap()).unwrap()
                ["schema"]
                == 1
        );
        assert_eq!(fs::read(backup).unwrap(), b"do not replace");
    }

    #[test]
    fn a_manifest_another_editor_is_converting_is_left_alone() {
        let folder = tempfile::tempdir().unwrap();
        manifest_of_schema(folder.path(), 1);
        let path = project_path(folder.path());
        let lock = path.with_file_name("arc-project.json.lock");
        fs::write(&lock, "").unwrap();
        let before = files(folder.path());
        let held = convert_project(folder.path()).unwrap_err();
        assert_eq!(held.code, "project.locked");
        assert_eq!(held.parameters["lock"], lock.display().to_string());
        // Nothing was written, no backup was made, and the other's lock is still there.
        assert_eq!(files(folder.path()), before);
        fs::remove_file(&lock).unwrap();
        assert!(convert_project(folder.path()).is_ok());
    }

    #[test]
    fn a_manifest_written_during_the_conversion_is_not_replaced() {
        let folder = tempfile::tempdir().unwrap();
        manifest_of_schema(folder.path(), 1);
        let path = project_path(folder.path());
        // Another program, which makes no lock file, writes the manifest after it was read.
        let other = br#"{"schema":1,"written":"elsewhere"}"#;
        let changed = convert_with(folder.path(), || fs::write(&path, other).unwrap()).unwrap_err();
        assert_eq!(changed.code, "project.changed");
        assert_eq!(fs::read(&path).unwrap(), other);
        // The backup was of what is no longer the manifest: it is not left to be taken for
        // one. And the manifest is let go.
        assert!(!path
            .with_file_name("arc-project.json.schema-1.bak")
            .exists());
        assert!(!path.with_file_name("arc-project.json.lock").exists());
    }

    #[cfg(unix)]
    #[test]
    fn what_leads_out_of_the_folder_through_a_link_is_refused_not_followed() {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        let level = serde_json::to_vec(&scene_data::current_map()).unwrap();
        let elsewhere = outside.path().join("elsewhere.level.json");
        fs::write(&elsewhere, &level).unwrap();

        // A level that is a link to a file outside: rejected as that, though it is a level.
        let folder = tempfile::tempdir().unwrap();
        let levels = folder.path().join("levels");
        fs::create_dir_all(&levels).unwrap();
        let linked = levels.join("linked.level.json");
        symlink(&elsewhere, &linked).unwrap();
        let plan = plan_setup(folder.path()).unwrap();
        assert_eq!(plan.existing, None);
        assert_eq!(plan.rejected[0].code, "project.outside_folder");
        assert_eq!(
            create_project(folder.path(), "Arc Test").unwrap_err().code,
            "project.levels_rejected"
        );

        // A manifest whose start level is reached through a linked folder: its path is
        // refused, and a setup on that project is refused.
        let project = tempfile::tempdir().unwrap();
        symlink(outside.path(), project.path().join("levels")).unwrap();
        let mut config = ProjectConfig::new("Arc Test");
        config.startup.start_level = "levels/elsewhere.level.json".into();
        fs::write(
            project_path(project.path()),
            project_spec::canonical_bytes(&config).unwrap(),
        )
        .unwrap();
        assert_eq!(
            start_level_path(project.path(), &config).unwrap_err().code,
            "project.outside_folder"
        );
        assert_eq!(
            create_project(project.path(), "Arc Test").unwrap_err().code,
            "project.outside_folder"
        );

        // A levels folder that is a link: no starter is written through it.
        let fresh = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        symlink(target.path(), fresh.path().join("levels")).unwrap();
        let refused = create_project(fresh.path(), "Arc Test").unwrap_err();
        assert_eq!(refused.code, "project.outside_folder");
        assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
        assert!(!project_path(fresh.path()).exists());

        // A manifest that is itself a link out of the folder is not read or converted.
        let linked_manifest = tempfile::tempdir().unwrap();
        let real = outside.path().join("arc-project.json");
        fs::write(&real, project_spec::canonical_bytes(&config).unwrap()).unwrap();
        symlink(&real, project_path(linked_manifest.path())).unwrap();
        assert_eq!(
            load_project(linked_manifest.path()).unwrap_err().code,
            "project.outside_folder"
        );
        assert_eq!(
            convert_project(linked_manifest.path()).unwrap_err().code,
            "project.outside_folder"
        );
        // A link inside the folder to a file inside the folder is the folder's own.
        let own = tempfile::tempdir().unwrap();
        let own_levels = own.path().join("levels");
        fs::create_dir_all(&own_levels).unwrap();
        fs::write(own.path().join("real.level.json"), &level).unwrap();
        symlink(
            own.path().join("real.level.json"),
            own_levels.join("alias.level.json"),
        )
        .unwrap();
        assert_eq!(
            plan_setup(own.path()).unwrap().existing,
            Some(own_levels.join("alias.level.json"))
        );
    }
}
