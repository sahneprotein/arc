//! The editor-wide game runtime location and its per-user preference.
//!
//! Projects keep their own source, cooked packages and emitter assets. A runtime folder
//! supplies the matching server/client programs and the base directory for Bevy assets.
//! This small preference is deliberately stored outside project metadata.

use crate::{persistence::PathLock, project::GameRuntimeRequirementKind};
use scene_data::Diagnostic;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const RUNTIME_PREFERENCE_SCHEMA: u32 = 1;
pub const MAX_RUNTIME_PREFERENCE_BYTES: usize = 4096;
const RUNTIME_PREFERENCE_FILE: &str = "runtime-install.json";

/// A validated installation whose children are all inside its canonical root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeInstallation {
    pub root: PathBuf,
    pub server: PathBuf,
    pub client: PathBuf,
    /// Base directory passed as `BEVY_ASSET_ROOT`; Bevy reads its `assets/` child.
    pub asset_root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeFolderStatus {
    Missing,
    Ready {
        installation: RuntimeInstallation,
    },
    Incomplete {
        requirements: Vec<crate::project::GameRuntimeRequirement>,
    },
    Refused {
        problem: Diagnostic,
    },
}

/// Result of checking one user-selected runtime folder. This never scans other folders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeFolderInspection {
    pub selected_root: PathBuf,
    pub canonical_root: Option<PathBuf>,
    pub status: RuntimeFolderStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeLocationStatus {
    /// The preference file is absent.
    Unconfigured,
    Configured {
        root: PathBuf,
        folder: RuntimeFolderInspection,
    },
    Refused {
        problem: Diagnostic,
    },
    /// No platform config location is available, so a preference cannot be persisted.
    PreferenceUnavailable,
}

/// Editor-wide runtime selection and the preference file which supplied it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeLocation {
    pub preference_path: Option<PathBuf>,
    pub status: RuntimeLocationStatus,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RuntimePreferenceFile {
    schema: u32,
    root: String,
}

/// Standard per-user local configuration path, or `None` if the OS has no such location.
pub fn default_runtime_preference_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("com", "Arc", "Arc Editor")
        .map(|dirs| dirs.config_local_dir().join(RUNTIME_PREFERENCE_FILE))
}

/// Per-user log directory, outside the selected runtime folder. `None` lets the caller use
/// its documented project-local fallback when the platform exposes no data directory.
pub fn default_play_logs_directory() -> Option<PathBuf> {
    directories::ProjectDirs::from("com", "Arc", "Arc Editor")
        .map(|dirs| dirs.data_local_dir().join("play-logs"))
}

/// Inspect the current per-user preference. The injected-path form keeps scripted checks
/// independent of the actual user's files.
pub fn runtime_location() -> RuntimeLocation {
    match default_runtime_preference_path() {
        Some(path) => runtime_location_at(&path),
        None => RuntimeLocation {
            preference_path: None,
            status: RuntimeLocationStatus::PreferenceUnavailable,
        },
    }
}

pub fn runtime_location_at(path: &Path) -> RuntimeLocation {
    let status = match load_runtime_preference(path) {
        Ok(None) => RuntimeLocationStatus::Unconfigured,
        Ok(Some(root)) => RuntimeLocationStatus::Configured {
            folder: inspect_runtime_folder(&root),
            root,
        },
        Err(problem) => RuntimeLocationStatus::Refused { problem },
    };
    RuntimeLocation {
        preference_path: Some(path.to_path_buf()),
        status,
    }
}

/// Read a bounded preference. A missing file is unconfigured; malformed, oversized and
/// newer-schema files are returned as errors and left untouched.
pub fn load_runtime_preference(path: &Path) -> Result<Option<PathBuf>, Diagnostic> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(runtime_io("runtime.preference_read", path, error)),
    };
    let mut bytes = Vec::new();
    file.take((MAX_RUNTIME_PREFERENCE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| runtime_io("runtime.preference_read", path, error))?;
    if bytes.len() > MAX_RUNTIME_PREFERENCE_BYTES {
        return Err(Diagnostic::new("runtime.preference_limit")
            .with("path", path.display())
            .with("max", MAX_RUNTIME_PREFERENCE_BYTES));
    }
    let preference: RuntimePreferenceFile = serde_json::from_slice(&bytes).map_err(|error| {
        Diagnostic::new("runtime.preference_decode")
            .with("path", path.display())
            .with("detail", error)
    })?;
    if preference.schema != RUNTIME_PREFERENCE_SCHEMA {
        return Err(Diagnostic::new("runtime.preference_schema")
            .with("path", path.display())
            .with("expected", RUNTIME_PREFERENCE_SCHEMA)
            .with("found", preference.schema));
    }
    let root = PathBuf::from(preference.root);
    if !root.is_absolute() {
        return Err(Diagnostic::new("runtime.preference_path").with("path", path.display()));
    }
    Ok(Some(root))
}

/// Validate a selected folder without searching the filesystem. Symlinks within an
/// installation are accepted only when they resolve inside its canonical root.
pub fn inspect_runtime_folder(root: &Path) -> RuntimeFolderInspection {
    let (canonical_root, status) = match fs::canonicalize(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (None, RuntimeFolderStatus::Missing)
        }
        Err(error) => (
            None,
            RuntimeFolderStatus::Refused {
                problem: runtime_io("runtime.folder_read", root, error),
            },
        ),
        Ok(canonical) if !canonical.is_dir() => (
            Some(canonical.clone()),
            RuntimeFolderStatus::Refused {
                problem: Diagnostic::new("runtime.not_a_folder").with("path", canonical.display()),
            },
        ),
        Ok(canonical) => (
            Some(canonical.clone()),
            inspect_canonical_runtime(&canonical),
        ),
    };
    RuntimeFolderInspection {
        selected_root: root.to_path_buf(),
        canonical_root,
        status,
    }
}

/// Validate and return a complete runtime installation, or a stable diagnostic naming every
/// missing required file/directory. This is used before persisting a new preference.
pub fn validate_runtime_folder(root: &Path) -> Result<RuntimeInstallation, Diagnostic> {
    match inspect_runtime_folder(root).status {
        RuntimeFolderStatus::Ready { installation } => Ok(installation),
        RuntimeFolderStatus::Missing => {
            Err(Diagnostic::new("runtime.folder_missing").with("path", root.display()))
        }
        RuntimeFolderStatus::Incomplete { requirements } => {
            Err(Diagnostic::new("runtime.requirements_missing")
                .with("requirements", requirement_text(&requirements)))
        }
        RuntimeFolderStatus::Refused { problem } => Err(problem),
    }
}

/// Validate first, then atomically publish a canonical selected runtime path in the injected
/// per-user preference file. Invalid selections never touch the previous preference.
pub fn save_runtime_preference(
    path: &Path,
    root: &Path,
) -> Result<RuntimeInstallation, Diagnostic> {
    let installation = validate_runtime_folder(root)?;
    let root_text = installation.root.to_str().ok_or_else(|| {
        Diagnostic::new("runtime.preference_path").with("path", installation.root.display())
    })?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| runtime_io("runtime.preference_write", parent, error))?;
    let lock_path = path.with_extension("json.lock");
    let _lock = PathLock::acquire(lock_path.clone()).map_err(|problem| {
        if problem.code == "save.locked" {
            Diagnostic::new("runtime.preference_locked")
                .with("path", path.display())
                .with("lock", lock_path.display())
        } else {
            Diagnostic::new("runtime.preference_write").with("detail", problem)
        }
    })?;
    // Do not replace a broken or newer file; it may contain settings from a newer editor.
    let _ = load_runtime_preference(path)?;
    let mut bytes = serde_json::to_vec_pretty(&RuntimePreferenceFile {
        schema: RUNTIME_PREFERENCE_SCHEMA,
        root: root_text.to_owned(),
    })
    .map_err(|error| Diagnostic::new("runtime.preference_encode").with("detail", error))?;
    bytes.push(b'\n');
    if bytes.len() > MAX_RUNTIME_PREFERENCE_BYTES {
        return Err(Diagnostic::new("runtime.preference_limit")
            .with("path", path.display())
            .with("max", MAX_RUNTIME_PREFERENCE_BYTES));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| runtime_io("runtime.preference_write", parent, error))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| runtime_io("runtime.preference_write", path, error))?;
    temporary
        .persist(path)
        .map_err(|error| runtime_io("runtime.preference_write", path, error.error))?;
    Ok(installation)
}

/// Persist a runtime to the standard per-user location. The injected-path function above is
/// the preferred seam for scripted checks.
pub fn set_runtime_preference(root: &Path) -> Result<RuntimeInstallation, Diagnostic> {
    let path = default_runtime_preference_path()
        .ok_or_else(|| Diagnostic::new("runtime.preference_unavailable"))?;
    save_runtime_preference(&path, root)
}

fn inspect_canonical_runtime(root: &Path) -> RuntimeFolderStatus {
    let suffix = std::env::consts::EXE_SUFFIX;
    let candidates = [
        (
            GameRuntimeRequirementKind::ServerExecutable,
            root.join("target/debug").join(format!("server{suffix}")),
            RequirementType::Executable,
        ),
        (
            GameRuntimeRequirementKind::ClientExecutable,
            root.join("target/debug").join(format!("client{suffix}")),
            RequirementType::Executable,
        ),
        (
            GameRuntimeRequirementKind::AssetsDirectory,
            root.join("assets"),
            RequirementType::Directory,
        ),
        (
            GameRuntimeRequirementKind::PipCharacterModel,
            root.join("assets/characters/pip/pip_robe_lod0.glb"),
            RequirementType::PipGlb,
        ),
    ];
    let mut missing = Vec::new();
    let mut resolved = Vec::with_capacity(3);
    for (kind, candidate, requirement_type) in candidates {
        let path = match candidate.canonicalize() {
            Ok(path) if !path.starts_with(root) => {
                return RuntimeFolderStatus::Refused {
                    problem: Diagnostic::new("runtime.outside_folder")
                        .with("path", candidate.display()),
                };
            }
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(crate::project::GameRuntimeRequirement {
                    kind,
                    path: candidate,
                });
                continue;
            }
            Err(error) => {
                return RuntimeFolderStatus::Refused {
                    problem: runtime_io("runtime.file_read", &candidate, error),
                };
            }
        };
        let present = match requirement_type {
            RequirementType::Directory => path.is_dir(),
            RequirementType::Executable => is_program_file(&path),
            RequirementType::PipGlb => match validate_pip_model(&path) {
                Ok(()) => true,
                Err(problem) => return RuntimeFolderStatus::Refused { problem },
            },
        };
        if present {
            if !matches!(requirement_type, RequirementType::PipGlb) {
                resolved.push(path);
            }
        } else {
            missing.push(crate::project::GameRuntimeRequirement {
                kind,
                path: candidate,
            });
        }
    }
    if !missing.is_empty() {
        return RuntimeFolderStatus::Incomplete {
            requirements: missing,
        };
    }
    RuntimeFolderStatus::Ready {
        installation: RuntimeInstallation {
            root: root.to_path_buf(),
            server: resolved[0].clone(),
            client: resolved[1].clone(),
            asset_root: root.to_path_buf(),
        },
    }
}

#[derive(Clone, Copy)]
enum RequirementType {
    Executable,
    Directory,
    PipGlb,
}

const LFS_POINTER_START: &[u8] = b"version https://git-lfs.github.com/spec/";

/// Match the client's early model check: reject LFS pointer bytes and require a complete
/// GLB header whose declared total length is exactly the file length.
fn validate_pip_model(path: &Path) -> Result<(), Diagnostic> {
    let metadata =
        fs::metadata(path).map_err(|error| runtime_io("runtime.pip_model_read", path, error))?;
    if !metadata.is_file() {
        return Err(Diagnostic::new("runtime.pip_model_invalid")
            .with("path", path.display())
            .with("reason", "not_a_file"));
    }
    let mut file =
        File::open(path).map_err(|error| runtime_io("runtime.pip_model_read", path, error))?;
    let mut prefix = [0; 64];
    let read = file
        .read(&mut prefix)
        .map_err(|error| runtime_io("runtime.pip_model_read", path, error))?;
    if prefix[..read].starts_with(LFS_POINTER_START) {
        return Err(Diagnostic::new("runtime.pip_model_lfs_pointer").with("path", path.display()));
    }
    if metadata.len() < 12
        || read < 12
        || &prefix[..4] != b"glTF"
        || u32::from_le_bytes(prefix[8..12].try_into().expect("four-byte GLB length")) as u64
            != metadata.len()
    {
        return Err(Diagnostic::new("runtime.pip_model_invalid")
            .with("path", path.display())
            .with("reason", "not_a_whole_glb"));
    }
    Ok(())
}

fn is_program_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn requirement_text(requirements: &[crate::project::GameRuntimeRequirement]) -> String {
    requirements
        .iter()
        .map(|requirement| format!("{:?}: {}", requirement.kind, requirement.path.display()))
        .collect::<Vec<_>>()
        .join("; ")
}

fn runtime_io(code: &str, path: &Path, error: std::io::Error) -> Diagnostic {
    Diagnostic::new(code)
        .with("path", path.display())
        .with("detail", error)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // The client's startup check verifies the GLB header and declared file length.
        fs::write(
            model,
            [b"glTF".as_slice(), &[2, 0, 0, 0], &[12, 0, 0, 0]].concat(),
        )
        .unwrap();
    }

    #[test]
    fn preference_is_injected_bounded_and_round_trips_a_canonical_valid_runtime() {
        let folder = tempfile::tempdir().unwrap();
        let runtime = folder.path().join("runtime");
        make_runtime(&runtime);
        let preference = folder.path().join("config/runtime-install.json");

        let saved = save_runtime_preference(&preference, &runtime).unwrap();
        assert_eq!(saved.root, fs::canonicalize(runtime).unwrap());
        assert_eq!(
            load_runtime_preference(&preference).unwrap(),
            Some(saved.root.clone())
        );
        let state = runtime_location_at(&preference);
        assert!(matches!(
            state.status,
            RuntimeLocationStatus::Configured {
                folder: RuntimeFolderInspection {
                    status: RuntimeFolderStatus::Ready { .. },
                    ..
                },
                ..
            }
        ));
        assert!(fs::metadata(&preference).unwrap().len() as usize <= MAX_RUNTIME_PREFERENCE_BYTES);
    }

    #[test]
    fn invalid_selection_does_not_write_or_replace_the_preference() {
        let folder = tempfile::tempdir().unwrap();
        let preference = folder.path().join("config/runtime-install.json");
        let missing = folder.path().join("missing-runtime");
        assert_eq!(
            save_runtime_preference(&preference, &missing)
                .unwrap_err()
                .code,
            "runtime.folder_missing"
        );
        assert!(!preference.exists());

        let original = br#"{"schema":98,"root":"/preserve-me"}"#;
        fs::create_dir_all(preference.parent().unwrap()).unwrap();
        fs::write(&preference, original).unwrap();
        let runtime = folder.path().join("runtime");
        make_runtime(&runtime);
        assert_eq!(
            save_runtime_preference(&preference, &runtime)
                .unwrap_err()
                .code,
            "runtime.preference_schema"
        );
        assert_eq!(fs::read(&preference).unwrap(), original);
    }

    #[test]
    fn selected_runtime_reports_each_missing_requirement_and_rejects_escaping_links() {
        let folder = tempfile::tempdir().unwrap();
        let empty = folder.path().join("empty");
        fs::create_dir(&empty).unwrap();
        let inspection = inspect_runtime_folder(&empty);
        let RuntimeFolderStatus::Incomplete { requirements } = inspection.status else {
            panic!("expected missing runtime requirements: {inspection:?}");
        };
        assert_eq!(requirements.len(), 4);
        assert_eq!(
            requirements[3].kind,
            GameRuntimeRequirementKind::PipCharacterModel
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let outside = folder.path().join("outside");
            make_runtime(&outside);
            let selected_link = folder.path().join("runtime-link");
            symlink(&outside, &selected_link).unwrap();
            assert!(matches!(
                inspect_runtime_folder(&selected_link).status,
                RuntimeFolderStatus::Ready { .. }
            ));

            let runtime = folder.path().join("contained-runtime");
            make_runtime(&runtime);
            let external_assets = folder.path().join("assets-outside");
            fs::create_dir(&external_assets).unwrap();
            fs::remove_dir_all(runtime.join("assets")).unwrap();
            symlink(&external_assets, runtime.join("assets")).unwrap();
            assert!(matches!(
                inspect_runtime_folder(&runtime).status,
                RuntimeFolderStatus::Refused { ref problem }
                    if problem.code == "runtime.outside_folder"
            ));
        }
    }

    #[test]
    fn pip_model_is_required_and_lfs_or_incomplete_glb_bytes_refuse_the_runtime() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("runtime");
        make_runtime(&root);
        let root = fs::canonicalize(root).unwrap();
        let model = root.join("assets/characters/pip/pip_robe_lod0.glb");
        assert!(matches!(
            inspect_runtime_folder(&root).status,
            RuntimeFolderStatus::Ready { .. }
        ));

        fs::remove_file(&model).unwrap();
        let RuntimeFolderStatus::Incomplete { requirements } = inspect_runtime_folder(&root).status
        else {
            panic!("a missing model must be a missing runtime requirement");
        };
        assert_eq!(requirements.len(), 1);
        assert_eq!(
            requirements[0].kind,
            GameRuntimeRequirementKind::PipCharacterModel
        );
        assert_eq!(requirements[0].path, model);

        fs::write(
            &model,
            b"version https://git-lfs.github.com/spec/v1\noid sha256:abc\n",
        )
        .unwrap();
        assert_eq!(
            inspect_runtime_folder(&root).status,
            RuntimeFolderStatus::Refused {
                problem: Diagnostic::new("runtime.pip_model_lfs_pointer")
                    .with("path", model.display()),
            }
        );

        fs::write(&model, b"glTF\x02\0\0\0\x40\0\0\0short").unwrap();
        assert_eq!(
            inspect_runtime_folder(&root).status,
            RuntimeFolderStatus::Refused {
                problem: Diagnostic::new("runtime.pip_model_invalid")
                    .with("path", model.display())
                    .with("reason", "not_a_whole_glb"),
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn pip_model_symlink_cannot_escape_the_runtime_root() {
        use std::os::unix::fs::symlink;

        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("runtime");
        make_runtime(&root);
        let model = root.join("assets/characters/pip/pip_robe_lod0.glb");
        fs::remove_file(&model).unwrap();
        let external = folder.path().join("outside.glb");
        fs::write(
            &external,
            [b"glTF".as_slice(), &[2, 0, 0, 0], &[12, 0, 0, 0]].concat(),
        )
        .unwrap();
        symlink(&external, &model).unwrap();

        assert!(matches!(
            inspect_runtime_folder(&root).status,
            RuntimeFolderStatus::Refused { ref problem }
                if problem.code == "runtime.outside_folder"
        ));
    }

    #[test]
    fn malformed_and_oversized_preferences_are_visible_and_never_overwritten() {
        let folder = tempfile::tempdir().unwrap();
        let preference = folder.path().join("runtime-install.json");
        fs::write(&preference, b"not json").unwrap();
        assert_eq!(
            runtime_location_at(&preference).status,
            RuntimeLocationStatus::Refused {
                problem: Diagnostic::new("runtime.preference_decode")
                    .with("path", preference.display())
                    .with(
                        "detail",
                        serde_json::from_slice::<RuntimePreferenceFile>(b"not json").unwrap_err()
                    ),
            }
        );
        let original = fs::read(&preference).unwrap();
        let runtime = folder.path().join("runtime");
        make_runtime(&runtime);
        assert_eq!(
            save_runtime_preference(&preference, &runtime)
                .unwrap_err()
                .code,
            "runtime.preference_decode"
        );
        assert_eq!(fs::read(preference).unwrap(), original);
    }
}
