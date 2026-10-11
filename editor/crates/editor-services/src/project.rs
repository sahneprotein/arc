//! Project manifest I/O, first-project bootstrap and explicit, backed-up conversion.
//!
//! Project setup is an explicit operation. Merely opening an older project never changes
//! its files; conversion keeps the original bytes in a schema-named backup first.
//!
//! What is read and written stays inside the project's folder, as the file system resolves
//! it: a manifest, a level or a levels folder that is a link to somewhere else is refused,
//! not followed. Nothing is made before what is asked for has been found valid, so a setup
//! that is refused leaves the folder as it was.

use crate::persistence::PathLock;
use project_spec::{ProjectConfig, ProjectError};
use scene_data::{Diagnostic, SceneDocument};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

mod settings;
pub use settings::*;

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
    let mut plan = SetupPlan {
        existing: None,
        rejected: Vec::new(),
        starter: next_starter_path(root),
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

/// Creates the project manifest and first playable starter level without replacing anything.
/// If a level already exists, it becomes the selected start level instead of being overwritten.
/// A folder whose level files are all rejected is not set up: see [`create_project_with`].
pub fn create_project(root: &Path, name: &str) -> Result<ProjectSetup, Diagnostic> {
    create_project_with(root, name, FreshStarter::OnlyWithoutLevels)
}

/// [`create_project`], saying whether a fresh starter level may be made beside level files
/// that were rejected.
///
/// Nothing is made before the name and the whole manifest have been found valid. A starter
/// level that was made is taken away again if the manifest cannot be written after it.
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
    config.startup.start_level = relative_string(root, &level_path)?;
    // The whole manifest, before anything is written.
    let bytes = project_spec::canonical_bytes(&config)
        .map_err(|error| project_error(&config_path, error))?;
    if created_level {
        create_starter_level(&level_path)?;
    }
    if let Err(refusal) = create_exclusive(&config_path, &bytes) {
        // The starter was made for this project, and there is no project.
        if created_level {
            let _ = fs::remove_file(&level_path);
        }
        return Err(refusal);
    }
    Ok(ProjectSetup {
        config,
        config_path,
        start_level_path: level_path,
        created_project: true,
        created_level,
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

fn create_starter_level(path: &Path) -> Result<(), Diagnostic> {
    let parent = path
        .parent()
        .ok_or_else(|| Diagnostic::new("project.path_invalid"))?;
    fs::create_dir_all(parent)
        .map_err(|error| io_problem("project.create_folder", parent, error))?;
    let document: SceneDocument = scene_data::current_map();
    scene_data::validate(&document)
        .map_err(|error| Diagnostic::new("project.starter_invalid").with("detail", error))?;
    let mut bytes = serde_json::to_vec_pretty(&document)
        .map_err(|error| Diagnostic::new("project.encode").with("detail", error))?;
    bytes.push(b'\n');
    create_exclusive(path, &bytes)
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
        assert!(project.created_project && project.created_level);
        assert!(project.start_level_path.is_file());
        assert_eq!(load_project(folder.path()).unwrap(), project.config);

        let level_bytes = fs::read(&project.start_level_path).unwrap();
        let again = create_project(folder.path(), "A Different Name").unwrap();
        assert!(!again.created_project && !again.created_level);
        assert_eq!(fs::read(&project.start_level_path).unwrap(), level_bytes);
        assert_eq!(again.config.project.name, "Arc Test");
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
