//! Filesystem discovery shared by the editor and game. No files are written.
use std::{
    fs,
    path::{Path, PathBuf},
};

/// A stable discovery code, the affected path, and optional filesystem detail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveryError {
    pub code: &'static str,
    pub path: PathBuf,
    pub detail: Option<String>,
}
impl DiscoveryError {
    fn new(code: &'static str, path: &Path) -> Self {
        Self {
            code,
            path: path.into(),
            detail: None,
        }
    }
}
impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.path.display())?;
        if let Some(detail) = &self.detail {
            write!(f, ": {detail}")?;
        }
        Ok(())
    }
}
impl std::error::Error for DiscoveryError {}
fn io_problem(code: &'static str, path: &Path, error: std::io::Error) -> DiscoveryError {
    DiscoveryError {
        code,
        path: path.into(),
        detail: Some(error.to_string()),
    }
}

/// Finds the unique case-insensitive `.aproj` entry directly in `root`.
/// Counts directories and links too, so an invalid entry cannot hide ambiguity.
/// Without an `.aproj`, returns the default setup destination (even when the root
/// does not exist yet). Callers loading a project must refuse that missing file.
/// Links resolving outside the root, including dangling links, are refused.
/// Callers must check that the returned entry is a regular file before reading it.
pub fn resolve_project_path(root: &Path) -> Result<PathBuf, DiscoveryError> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => Some(entries),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(io_problem("project.read", root, error)),
    };
    let mut found = None;
    if let Some(entries) = entries {
        for entry in entries {
            let entry = entry.map_err(|error| io_problem("project.read", root, error))?;
            let path = entry.path();
            if crate::is_project_file(&path) {
                if found.is_some() {
                    return Err(DiscoveryError::new("project.multiple_files", root));
                }
                found = Some(path);
            }
        }
    }
    let path = found.unwrap_or_else(|| root.join(crate::FILE_NAME));
    inside(root, &path)?;
    Ok(path)
}

/// Validates a selected `.aproj` as the folder's unique, existing regular project
/// file and returns its canonical parent. Does not read or decode its contents.
/// Links within the project are accepted; links outside it are refused.
pub fn project_root_from_file(path: &Path) -> Result<PathBuf, DiscoveryError> {
    if !crate::is_project_file(path) {
        return Err(DiscoveryError::new("project.path_invalid", path));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let root =
        fs::canonicalize(parent).map_err(|error| io_problem("project.read", parent, error))?;
    let selected = root.join(
        path.file_name()
            .ok_or_else(|| DiscoveryError::new("project.path_invalid", path))?,
    );
    let resolved = resolve_project_path(&root)?;
    if resolved != selected {
        return Err(DiscoveryError::new("project.path_invalid", path));
    }
    let metadata =
        fs::metadata(&selected).map_err(|error| io_problem("project.read", &selected, error))?;
    if !metadata.is_file() {
        return Err(DiscoveryError::new("project.not_a_file", &selected));
    }
    Ok(root)
}

fn inside(root: &Path, path: &Path) -> Result<(), DiscoveryError> {
    let outside = || DiscoveryError::new("project.outside_folder", path);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FILE_NAME;

    #[test]
    fn no_project_returns_the_setup_destination_even_for_a_missing_root() {
        let dir = tempfile::tempdir().unwrap();
        for root in [dir.path().to_path_buf(), dir.path().join("new")] {
            assert_eq!(resolve_project_path(&root).unwrap(), root.join(FILE_NAME));
        }
        assert!(!dir.path().join(FILE_NAME).exists());
        assert_eq!(
            project_root_from_file(&dir.path().join(FILE_NAME))
                .unwrap_err()
                .code,
            "project.read"
        );
    }

    #[test]
    fn one_renamed_mixed_case_project_has_its_parent_as_root() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Renamed.ApRoJ");
        // Discovery does not parse contents.
        fs::write(&path, b"{").unwrap();
        assert_eq!(resolve_project_path(dir.path()).unwrap(), path);
        assert_eq!(
            project_root_from_file(&path).unwrap(),
            fs::canonicalize(dir.path()).unwrap()
        );
        assert_eq!(
            project_root_from_file(&dir.path().join("other.aproj"))
                .unwrap_err()
                .code,
            "project.path_invalid"
        );
        assert_eq!(fs::read(&path).unwrap(), b"{");
    }

    #[test]
    fn two_entries_are_ambiguous_even_when_one_is_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), b"{}").unwrap();
        fs::write(dir.path().join("other.APROJ"), b"{}").unwrap();
        assert_eq!(
            resolve_project_path(dir.path()).unwrap_err().code,
            "project.multiple_files"
        );
        fs::remove_file(dir.path().join("other.APROJ")).unwrap();
        fs::create_dir(dir.path().join("other.APROJ")).unwrap();
        assert_eq!(
            resolve_project_path(dir.path()).unwrap_err().code,
            "project.multiple_files"
        );
        assert_eq!(
            project_root_from_file(&dir.path().join(FILE_NAME))
                .unwrap_err()
                .code,
            "project.multiple_files"
        );
    }

    #[test]
    fn a_directory_is_discovered_but_cannot_be_opened_as_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        fs::create_dir(&path).unwrap();
        assert_eq!(resolve_project_path(dir.path()).unwrap(), path);
        assert_eq!(
            project_root_from_file(&path).unwrap_err().code,
            "project.not_a_file"
        );
    }

    #[cfg(unix)]
    #[test]
    fn links_inside_are_accepted_but_outside_or_dangling_links_are_refused() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let target = dir.path().join("contents.json");
        fs::write(&target, b"{}").unwrap();
        symlink(&target, &path).unwrap();
        assert_eq!(resolve_project_path(dir.path()).unwrap(), path);
        assert_eq!(
            project_root_from_file(&path).unwrap(),
            fs::canonicalize(dir.path()).unwrap()
        );
        symlink(&target, dir.path().join("second.aproj")).unwrap();
        assert_eq!(
            resolve_project_path(dir.path()).unwrap_err().code,
            "project.multiple_files"
        );
        fs::remove_file(dir.path().join("second.aproj")).unwrap();
        fs::remove_file(&path).unwrap();
        let external = outside.path().join("contents.json");
        fs::write(&external, b"{}").unwrap();
        symlink(&external, &path).unwrap();
        for _ in 0..2 {
            assert_eq!(
                resolve_project_path(dir.path()).unwrap_err().code,
                "project.outside_folder"
            );
            assert_eq!(
                project_root_from_file(&path).unwrap_err().code,
                "project.outside_folder"
            );
            fs::remove_file(&external).ok();
        }
    }
}
