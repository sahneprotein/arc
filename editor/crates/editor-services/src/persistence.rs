use scene_data::*;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Clone, Copy, Debug)]
pub enum DiskVersion {
    Absent,
    Hash(ContentHash),
}
#[derive(Clone, Debug)]
pub struct LoadedDocument {
    pub document: SceneDocument,
    pub disk_hash: ContentHash,
    pub source_hash: ContentHash,
    pub timings: OpenTimings,
}
#[derive(Clone, Copy, Debug)]
pub struct OpenTimings {
    pub read_us: u64,
    pub decode_us: u64,
    pub validation_us: u64,
    pub total_us: u64,
}
#[derive(Clone, Debug)]
pub struct SaveReceipt {
    pub disk_hash: ContentHash,
    pub source_hash: ContentHash,
    pub total_us: u64,
    pub durability_warning: Option<Diagnostic>,
}
pub fn load_document(path: &Path, cancel: &AtomicBool) -> Result<LoadedDocument, Diagnostic> {
    let start = Instant::now();
    let bytes = read_bounded(path, MAX_SOURCE_BYTES, cancel)?;
    let read_us = elapsed(start);
    let decode = Instant::now();
    // The format has one schema; a file of another is refused by validation and never rewritten.
    let document: SceneDocument = serde_json::from_slice(&bytes)
        .map_err(|e| Diagnostic::new("source.decode").with("detail", e))?;
    let decode_us = elapsed(decode);
    let validation = Instant::now();
    let source_hash = source_hash(&document)?;
    let validation_us = elapsed(validation);
    check_cancel(cancel)?;
    Ok(LoadedDocument {
        document,
        source_hash,
        disk_hash: payload_hash(&bytes),
        timings: OpenTimings {
            read_us,
            decode_us,
            validation_us,
            total_us: elapsed(start),
        },
    })
}
/// Saves the level to `path`, which must be what `expected` says it is: absent, or the file
/// of that hash. The file is held with a lock beside it meanwhile, written beside itself
/// and put in its place in one step; a file that is not what was expected is not replaced
/// (`save.conflict`). Whenever the save does not go through, the level's file is as it was.
pub fn save_document(
    path: &Path,
    document: &SceneDocument,
    expected: DiskVersion,
    cancel: &AtomicBool,
) -> Result<SaveReceipt, Diagnostic> {
    save_with_hook(path, document, expected, cancel, || Ok(()))
}
fn save_with_hook(
    path: &Path,
    document: &SceneDocument,
    expected: DiskVersion,
    cancel: &AtomicBool,
    before_publish: impl FnOnce() -> Result<(), Diagnostic>,
) -> Result<SaveReceipt, Diagnostic> {
    let start = Instant::now();
    check_cancel(cancel)?;
    let hash = source_hash(document)?;
    let bytes = serde_json::to_vec_pretty(document)
        .map_err(|e| Diagnostic::new("source.encode").with("detail", e))?;
    if bytes.len() as u64 > MAX_SOURCE_BYTES {
        return Err(Diagnostic::new("source.limit"));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| Diagnostic::new("path.invalid"))?;
    check_cancel(cancel)?;
    // Only an explicit Save creates parents; loading and path inspection stay read-only.
    fs::create_dir_all(parent).map_err(|e| {
        Diagnostic::new("save.parent_create")
            .with("path", parent.display())
            .with("detail", e)
    })?;
    let mut lock_name = name.to_os_string();
    lock_name.push(".lock");
    let _lock = PathLock::acquire(parent.join(lock_name))?;
    verify_disk(path, expected, cancel)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
    if let Ok(metadata) = fs::metadata(path) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())
            .map_err(io_error)?;
    }
    temporary.write_all(&bytes).map_err(io_error)?;
    temporary.flush().map_err(io_error)?;
    temporary.as_file().sync_all().map_err(io_error)?;
    before_publish()?;
    check_cancel(cancel)?;
    verify_disk(path, expected, cancel)?;
    temporary.persist(path).map_err(|e| io_error(e.error))?;
    // Publication has succeeded; a directory durability failure is a warning, not a false failed save.
    let durability_warning = sync_directory(parent).err();
    Ok(SaveReceipt {
        disk_hash: payload_hash(&bytes),
        source_hash: hash,
        total_us: elapsed(start),
        durability_warning,
    })
}
fn verify_disk(path: &Path, expected: DiskVersion, cancel: &AtomicBool) -> Result<(), Diagnostic> {
    read_expected(path, expected, cancel).map(|_| ())
}
/// The bytes of the file, which must be what is expected: `None` for a file that is to be
/// absent and is. Anything else is a conflict.
fn read_expected(
    path: &Path,
    expected: DiskVersion,
    cancel: &AtomicBool,
) -> Result<Option<Vec<u8>>, Diagnostic> {
    match expected {
        DiskVersion::Absent => match fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Ok(_) => Err(Diagnostic::new("save.conflict")),
            Err(e) => Err(io_error(e)),
        },
        DiskVersion::Hash(hash) => {
            let bytes = read_bounded(path, MAX_SOURCE_BYTES, cancel)?;
            if payload_hash(&bytes) != hash {
                return Err(Diagnostic::new("save.conflict"));
            }
            Ok(Some(bytes))
        }
    }
}
/// A file that says its neighbour is being written: made as a new file, which only one
/// writer can do at a time, and removed when the writer is done. It holds off other editors,
/// which make it too; a program that does not is not held off. A writer that stops without
/// finishing leaves the file behind, and it must then be removed by hand.
pub(crate) struct PathLock {
    path: PathBuf,
    _file: File,
}
impl PathLock {
    pub(crate) fn acquire(path: PathBuf) -> Result<Self, Diagnostic> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    Diagnostic::new("save.locked")
                } else {
                    io_error(e)
                }
            })?;
        Ok(Self { path, _file: file })
    }
}
impl Drop for PathLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
pub(crate) fn read_bounded(
    path: &Path,
    limit: u64,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, Diagnostic> {
    check_cancel(cancel)?;
    let mut file = File::open(path).map_err(io_error)?.take(limit + 1);
    let mut bytes = Vec::new();
    let mut chunk = [0; 65536];
    loop {
        check_cancel(cancel)?;
        let n = file.read(&mut chunk).map_err(io_error)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
    if bytes.len() as u64 > limit {
        return Err(Diagnostic::new("source.limit"));
    }
    Ok(bytes)
}
pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<(), Diagnostic> {
    if cancel.load(Ordering::Acquire) {
        Err(Diagnostic::new("job.cancelled"))
    } else {
        Ok(())
    }
}
pub(crate) fn io_error(error: std::io::Error) -> Diagnostic {
    Diagnostic::new("io.failed").with("detail", error)
}
pub(crate) fn elapsed(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u64::MAX as u128) as u64
}
#[cfg(unix)]
pub(crate) fn sync_directory(path: &Path) -> Result<(), Diagnostic> {
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(io_error)
}
#[cfg(not(unix))]
pub(crate) fn sync_directory(_path: &Path) -> Result<(), Diagnostic> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_and_cancelled_saves_preserve_original_and_remove_temporary_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("map.arcscene");
        let cancel = AtomicBool::new(false);
        let scene = current_map();
        let receipt = save_document(&path, &scene, DiskVersion::Absent, &cancel).unwrap();
        let original = fs::read(&path).unwrap();
        let mut changed = scene.clone();
        changed.objects[0].name = "Changed".into();
        let fail = save_with_hook(
            &path,
            &changed,
            DiskVersion::Hash(receipt.disk_hash),
            &cancel,
            || Err(Diagnostic::new("test.io_failure")),
        );
        assert_eq!(fail.unwrap_err().code, "test.io_failure");
        assert_eq!(fs::read(&path).unwrap(), original);
        let cancelled = save_with_hook(
            &path,
            &changed,
            DiskVersion::Hash(receipt.disk_hash),
            &cancel,
            || {
                cancel.store(true, Ordering::Release);
                Ok(())
            },
        );
        assert_eq!(cancelled.unwrap_err().code, "job.cancelled");
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
    /// A level saved to a file, and the same level with a change not yet saved.
    fn saved_on_disk(directory: &Path) -> (PathBuf, SceneDocument, SaveReceipt, Vec<u8>) {
        let path = directory.join("map.level.json");
        let cancel = AtomicBool::new(false);
        let scene = current_map();
        let receipt = save_document(&path, &scene, DiskVersion::Absent, &cancel).unwrap();
        let original = fs::read(&path).unwrap();
        let mut changed = scene;
        changed.objects[0].name = "Changed".into();
        (path, changed, receipt, original)
    }
    fn names(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }
    #[test]
    fn a_level_of_another_schema_is_refused_and_its_file_is_never_rewritten() {
        let directory = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let (path, mut other, receipt, original) = saved_on_disk(directory.path());
        other.schema = SCHEMA_VERSION + 1;
        let refused = save_document(&path, &other, DiskVersion::Hash(receipt.disk_hash), &cancel)
            .unwrap_err();
        assert_eq!(refused.code, "schema.unsupported");
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(names(directory.path()), ["map.level.json"]);

        // A file of another schema is not opened as a level either.
        let text = String::from_utf8(original).unwrap().replacen(
            &format!("\"schema\": {SCHEMA_VERSION}"),
            &format!("\"schema\": {}", SCHEMA_VERSION + 1),
            1,
        );
        fs::write(&path, &text).unwrap();
        assert_eq!(
            load_document(&path, &cancel).unwrap_err().code,
            "schema.unsupported"
        );
        assert_eq!(fs::read(&path).unwrap(), text.as_bytes());
        assert_eq!(names(directory.path()), ["map.level.json"]);
    }
    #[test]
    fn a_save_that_does_not_go_through_leaves_the_level_as_it_was() {
        let directory = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let (path, changed, receipt, original) = saved_on_disk(directory.path());
        let expected = DiskVersion::Hash(receipt.disk_hash);

        // The write fails before the file is put in place.
        let failed = save_with_hook(&path, &changed, expected, &cancel, || {
            Err(Diagnostic::new("test.io_failure"))
        });
        assert_eq!(failed.unwrap_err().code, "test.io_failure");
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(names(directory.path()), ["map.level.json"]);

        // Another editor holds the file: nothing is read or written.
        {
            let _theirs = PathLock::acquire(directory.path().join("map.level.json.lock")).unwrap();
            let locked = save_document(&path, &changed, expected, &cancel).unwrap_err();
            assert_eq!(locked.code, "save.locked");
            assert_eq!(fs::read(&path).unwrap(), original);
        }

        // Another program writes the file before the publication: the file is theirs.
        let conflict = save_with_hook(&path, &changed, expected, &cancel, || {
            fs::write(&path, b"another program's level").unwrap();
            Ok(())
        });
        assert_eq!(conflict.unwrap_err().code, "save.conflict");
        assert_eq!(fs::read(&path).unwrap(), b"another program's level");
        assert_eq!(names(directory.path()), ["map.level.json"]);

        // A file that is no longer what was opened is not written over.
        let stale = save_document(&path, &changed, expected, &cancel).unwrap_err();
        assert_eq!(stale.code, "save.conflict");
        assert_eq!(names(directory.path()), ["map.level.json"]);

        // A save that is cancelled.
        fs::write(&path, &original).unwrap();
        let cancelled = save_with_hook(&path, &changed, expected, &cancel, || {
            cancel.store(true, Ordering::Release);
            Ok(())
        });
        assert_eq!(cancelled.unwrap_err().code, "job.cancelled");
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(names(directory.path()), ["map.level.json"]);
    }
    #[test]
    fn detects_external_changes_and_round_trips() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("map.arcscene");
        let cancel = AtomicBool::new(false);
        let scene = current_map();
        let receipt = save_document(&path, &scene, DiskVersion::Absent, &cancel).unwrap();
        let loaded = load_document(&path, &cancel).unwrap();
        assert_eq!(loaded.document, scene);
        assert_eq!(loaded.disk_hash, receipt.disk_hash);
        fs::write(&path, b"external edit").unwrap();
        assert_eq!(
            save_document(&path, &scene, DiskVersion::Hash(receipt.disk_hash), &cancel)
                .unwrap_err()
                .code,
            "save.conflict"
        );
        assert_eq!(fs::read(&path).unwrap(), b"external edit");
    }
    #[test]
    fn only_explicit_save_creates_missing_parents_and_preserves_conflicts() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("new/project/levels");
        let path = parent.join("map.arcscene");
        let cancel = AtomicBool::new(false);
        assert_eq!(load_document(&path, &cancel).unwrap_err().code, "io.failed");
        assert!(!directory.path().join("new").exists());
        let scene = current_map();
        let receipt = save_document(&path, &scene, DiskVersion::Absent, &cancel).unwrap();
        assert_eq!(load_document(&path, &cancel).unwrap().document, scene);
        assert_eq!(fs::read_dir(&parent).unwrap().count(), 1);
        assert_eq!(
            save_document(&path, &scene, DiskVersion::Absent, &cancel)
                .unwrap_err()
                .code,
            "save.conflict"
        );
        assert_eq!(payload_hash(&fs::read(&path).unwrap()), receipt.disk_hash);
    }
}
