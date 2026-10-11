use scene_data::Diagnostic;
use std::path::Path;

/// Injectable launch seam. Implementations must spawn/dispatch without waiting
/// for the viewer to close; scripted checks can record requests without opening UI.
pub trait PathLauncher {
    fn launch(&self, path: &Path) -> Result<(), Diagnostic>;
}
pub struct SystemPathLauncher;
impl PathLauncher for SystemPathLauncher {
    fn launch(&self, path: &Path) -> Result<(), Diagnostic> {
        #[cfg(target_os = "macos")]
        let executable = "/usr/bin/open";
        #[cfg(target_os = "linux")]
        let executable = "xdg-open";
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        return Err(Diagnostic::new("path.platform_unsupported"));

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            // Prepare a reaper before spawning, so thread-start failure cannot
            // leave an unowned child. Spawn errors are returned synchronously;
            // eventual viewer exit is not a synchronous launch outcome.
            let (sender, receiver) = std::sync::mpsc::channel::<std::process::Child>();
            std::thread::Builder::new()
                .name("open-system-path".into())
                .spawn(move || {
                    if let Ok(mut child) = receiver.recv() {
                        let _ = child.wait();
                    }
                })
                .map_err(|error| Diagnostic::new("path.launch_failed").with("error", error))?;
            let mut child = std::process::Command::new(executable)
                .arg(path)
                .spawn()
                .map_err(|error| Diagnostic::new("path.launch_failed").with("error", error))?;
            if let Err(error) = sender.send(child) {
                child = error.0;
                let _ = child.kill();
                let _ = child.wait();
                return Err(Diagnostic::new("path.launch_failed"));
            }
            Ok(())
        }
    }
}
/// Dispatch a native file path. No shell interpolation and no process wait.
/// Success means dispatch accepted, not proof a window became visible.
pub fn open_path(path: &Path) -> Result<(), Diagnostic> {
    open_path_with(path, &SystemPathLauncher)
}
pub fn open_path_with(path: &Path, launcher: &impl PathLauncher) -> Result<(), Diagnostic> {
    if !path.is_file() {
        return Err(Diagnostic::new("path.missing").with("path", path.display()));
    }
    launcher.launch(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    struct Fake {
        seen: RefCell<Vec<std::path::PathBuf>>,
        fail: bool,
    }
    impl PathLauncher for Fake {
        fn launch(&self, path: &Path) -> Result<(), Diagnostic> {
            self.seen.borrow_mut().push(path.to_owned());
            if self.fail {
                Err(Diagnostic::new("path.launch_failed"))
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn missing_file_is_refused_before_dispatch() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake {
            seen: RefCell::default(),
            fail: false,
        };
        assert_eq!(
            open_path_with(&dir.path().join("missing.log"), &fake)
                .unwrap_err()
                .code,
            "path.missing"
        );
        assert!(fake.seen.borrow().is_empty());
    }
    #[test]
    fn native_path_and_launcher_failure_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log spaces ; $(literal).log");
        std::fs::write(&path, "log").unwrap();
        let fake = Fake {
            seen: RefCell::default(),
            fail: true,
        };
        assert_eq!(
            open_path_with(&path, &fake).unwrap_err().code,
            "path.launch_failed"
        );
        assert_eq!(&*fake.seen.borrow(), &[path]);
    }
}
