//! Explicit executable paths + loopback channel. Never scrape stdout for readiness.
use super::{wire::*, *};
use scene_data::Diagnostic;
use std::{
    ffi::OsString,
    fs::File,
    io::{Read, Write},
    net::{TcpListener, TcpStream, UdpSocket},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
/// What a child of this role is told of the project: for the client, `--project-root` and
/// the project's folder, if the configuration names one and the folder is a ready project
/// now. That is: its manifest is one this editor reads, and the level it starts with does
/// not lead out of the folder. For the server, and for a folder that is no ready project
/// (no manifest, one of an older or newer version, one that cannot be read), nothing: the
/// child is started with the arguments it always had.
///
/// The folder is given as the file system resolves it: the child's working directory is
/// another than the editor's, so a path relative to the editor's would name another place.
pub(crate) fn project_arguments(config: &PlayConfig, role: PlayRole) -> Vec<OsString> {
    if role != PlayRole::Client {
        return Vec::new();
    }
    let Some(root) = &config.project_root else {
        return Vec::new();
    };
    let ready = crate::project::load_project(root)
        .and_then(|project| crate::project::start_level_path(root, &project))
        .is_ok();
    let Some(resolved) = std::fs::canonicalize(root).ok().filter(|_| ready) else {
        return Vec::new();
    };
    vec!["--project-root".into(), resolved.into_os_string()]
}

#[derive(Default)]
pub(crate) struct ExternalBackend {
    protocol: Arc<Mutex<Option<(RunId, String)>>>,
}
impl PlayBackend for ExternalBackend {
    fn launch(&self, launch: PlayLaunch) -> Result<Box<dyn PlayChild>, Diagnostic> {
        for executable in [&launch.config.server, &launch.config.client] {
            if !executable.is_absolute() || !executable.is_file() {
                return Err(Diagnostic::new("play.executable_missing")
                    .with("path", executable.to_string_lossy()));
            }
        }
        if !launch.config.working_directory.is_dir()
            || !launch.config.asset_root.join("assets").is_dir()
        {
            return Err(Diagnostic::new("play.asset_root_missing"));
        }
        let listener = TcpListener::bind("127.0.0.1:0").map_err(io_error)?;
        listener.set_nonblocking(true).map_err(io_error)?;
        let address = listener.local_addr().map_err(io_error)?;
        let token = random_token()?;
        std::fs::create_dir_all(&launch.config.log_directory).map_err(io_error)?;
        let log = tempfile::Builder::new()
            .prefix(&format!("play-{}-{:?}-", launch.run.0, launch.role))
            .suffix(".log")
            .tempfile_in(&launch.config.log_directory)
            .map_err(io_error)?;
        let (log, log_path) = log.keep().map_err(|e| io_error(e.error))?;
        let executable = if launch.role == PlayRole::Server {
            &launch.config.server
        } else {
            &launch.config.client
        };
        let mut cmd = Command::new(executable);
        cmd.current_dir(&launch.config.working_directory)
            .env("BEVY_ASSET_ROOT", &launch.config.asset_root)
            .env_remove("ARC_PRIVATE_KEY")
            .env_remove("ARC_PLAY_TOKEN")
            .arg("--cooked-package")
            .arg(launch.receipt.path())
            .arg("--cooked-package-hash")
            .arg(launch.receipt.identity().manifest_hash.hex())
            .arg("--play-control")
            .arg(address.to_string())
            .arg("--play-run-id")
            .arg(launch.run.0.to_string())
            .arg("--play-token")
            .arg(&token)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().map_err(io_error)?))
            .stderr(Stdio::from(log));
        if launch.role == PlayRole::Server {
            // Reserve/select locally; release before the child's bind. A bind race is a visible startup failure.
            let socket = UdpSocket::bind("127.0.0.1:0").map_err(io_error)?;
            let bind = socket.local_addr().map_err(io_error)?;
            drop(socket);
            *self.protocol.lock().unwrap() = None;
            cmd.arg("--bind")
                .arg(bind.to_string())
                .arg("--max-clients")
                .arg("1")
                .arg("--metrics-addr")
                .arg("127.0.0.1:0")
                .arg("--allow-dev-key");
        } else {
            let server = launch
                .server_address
                .ok_or_else(|| Diagnostic::new("play.server_not_ready"))?;
            if !server.ip().is_loopback() || server.port() == 0 {
                return Err(Diagnostic::new("play.invalid_listening"));
            }
            cmd.arg("--server")
                .arg(server.to_string())
                .arg("--client-id")
                .arg(launch.run.0.to_string());
        }
        // After everything a child always got, so that those arguments are as they were.
        cmd.args(project_arguments(&launch.config, launch.role));
        let child = cmd.spawn().map_err(io_error)?;
        let fence = Fence {
            run: launch.run,
            role: launch.role,
            token,
            hash: launch.receipt.identity().manifest_hash.hex(),
            sequence: 0,
            protocol: self.protocol.clone(),
        };
        Ok(Box::new(Process {
            child,
            listener,
            stream: None,
            frames: Frames::default(),
            fence,
            log_path,
            stop: false,
            command: vec![],
            written: 0,
            authenticated: false,
            forced: false,
            reaped: false,
            eof: None,
        }))
    }
}
fn io_error(error: std::io::Error) -> Diagnostic {
    Diagnostic::new("play.io_failed").with("error", error)
}
#[cfg(unix)]
fn random_token() -> Result<String, Diagnostic> {
    let mut bytes = [0; 32];
    File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(io_error)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
#[cfg(not(unix))]
fn random_token() -> Result<String, Diagnostic> {
    Err(Diagnostic::new("play.platform_unsupported"))
}
struct Fence {
    run: RunId,
    role: PlayRole,
    token: String,
    hash: String,
    sequence: u64,
    protocol: Arc<Mutex<Option<(RunId, String)>>>,
}
impl Fence {
    fn accept(&mut self, report: Report) -> Result<PlayRuntimeEvent, Diagnostic> {
        let role = match self.role {
            PlayRole::Server => Role::Server,
            PlayRole::Client => Role::Client,
        };
        if report.version != VERSION
            || report.run != self.run.0
            || report.token != self.token
            || report.role != role
            || report.package_hash != self.hash
            || report.sequence != self.sequence + 1
        {
            return Err(Diagnostic::new("play.event_fence_mismatch"));
        }
        let valid_version =
            report
                .protocol_version
                .split_once('+')
                .is_some_and(|(version, fingerprint)| {
                    version.starts_with("arc-")
                        && version.len() > 4
                        && fingerprint.len() == 8
                        && fingerprint
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                });
        if !valid_version || report.protocol_version.len() > 128 {
            return Err(Diagnostic::new("play.protocol_invalid"));
        }
        let mut protocol = self.protocol.lock().unwrap();
        match &*protocol {
            Some((run, expected)) if *run != self.run || expected != &report.protocol_version => {
                return Err(Diagnostic::new("play.protocol_mismatch"))
            }
            None if self.role == PlayRole::Server => {
                *protocol = Some((self.run, report.protocol_version))
            }
            None => return Err(Diagnostic::new("play.server_not_ready")),
            _ => (),
        }
        self.sequence = report.sequence;
        Ok(match report.event {
            Event::Stage { stage } => PlayRuntimeEvent::Stage(match stage {
                Stage::PackageVerified => PlayStage::PackageVerified,
                Stage::CollisionReady => PlayStage::CollisionReady,
                Stage::HandshakeAccepted => PlayStage::HandshakeAccepted,
                Stage::Simulating => PlayStage::Simulating,
                Stage::LevelDrawn => PlayStage::LevelDrawn,
                Stage::FirstControllableFrame => PlayStage::FirstControllableFrame,
            }),
            Event::Listening { address } => PlayRuntimeEvent::Listening(address),
            Event::ExpectedWindowClose => PlayRuntimeEvent::ExpectedWindowClose,
        })
    }
}
struct Process {
    child: Child,
    listener: TcpListener,
    stream: Option<TcpStream>,
    frames: Frames,
    fence: Fence,
    log_path: PathBuf,
    stop: bool,
    command: Vec<u8>,
    written: usize,
    authenticated: bool,
    forced: bool,
    reaped: bool,
    eof: Option<Instant>,
}
impl PlayChild for Process {
    fn pid(&self) -> u32 {
        self.child.id()
    }
    fn log_path(&self) -> Option<PathBuf> {
        Some(self.log_path.clone())
    }
    fn poll(&mut self) -> Result<Vec<PlayRuntimeEvent>, Diagnostic> {
        if self.stream.is_none() {
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    if !peer.ip().is_loopback() {
                        return Err(Diagnostic::new("play.nonlocal_peer"));
                    }
                    stream.set_nonblocking(true).map_err(io_error)?;
                    self.stream = Some(stream);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(vec![]),
                Err(e) => return Err(io_error(e)),
            }
        }
        let stream = self.stream.as_mut().unwrap();
        let mut reports = vec![];
        for _ in 0..8 {
            let mut bytes = [0; 512];
            match stream.read(&mut bytes) {
                Ok(0) => {
                    self.eof.get_or_insert_with(Instant::now);
                    break;
                }
                Ok(n) => {
                    for frame in self
                        .frames
                        .feed(&bytes[..n])
                        .map_err(|e| Diagnostic::new("play.frame_invalid").with("error", e))?
                    {
                        let report: Report = serde_json::from_slice(&frame)
                            .map_err(|_| Diagnostic::new("play.event_decode"))?;
                        reports.push(self.fence.accept(report)?);
                        self.authenticated = true;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(io_error(e)),
            }
        }
        if self.stop && self.authenticated {
            if self.command.is_empty() {
                self.command = serde_json::to_vec(&super::wire::Command {
                    version: VERSION,
                    run: self.fence.run.0,
                    token: self.fence.token.clone(),
                    command: "stop".into(),
                })
                .unwrap();
                self.command.push(b'\n');
            }
            if self.written < self.command.len() {
                match stream.write(&self.command[self.written..]) {
                    Ok(n) => self.written += n,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                    Err(e) => return Err(io_error(e)),
                }
            }
        }
        if self
            .eof
            .is_some_and(|t| t.elapsed() > Duration::from_millis(100))
        {
            return Err(Diagnostic::new("play.channel_closed"));
        }
        Ok(reports)
    }
    fn stop(&mut self) -> Result<(), Diagnostic> {
        self.stop = true;
        Ok(())
    }
    fn kill(&mut self) -> Result<(), Diagnostic> {
        self.forced = true;
        self.child.kill().map_err(io_error)
    }
    fn try_wait(&mut self) -> Result<Option<PlayExit>, Diagnostic> {
        self.child.try_wait().map_err(io_error).map(|exit| {
            exit.map(|exit| {
                self.reaped = true;
                #[cfg(unix)]
                let signal = {
                    use std::os::unix::process::ExitStatusExt;
                    exit.signal()
                };
                #[cfg(not(unix))]
                let signal = None;
                PlayExit {
                    code: exit.code(),
                    signal,
                    forced: self.forced,
                }
            })
        })
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.try_wait();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reports_reject_wrong_hash_run_role_sequence_and_fingerprint() {
        let protocol = Arc::new(Mutex::new(None));
        let mut fence = Fence {
            run: RunId(7),
            role: PlayRole::Server,
            token: "nonce".into(),
            hash: "ab".repeat(32),
            sequence: 0,
            protocol: protocol.clone(),
        };
        let report = Report {
            version: 1,
            run: 7,
            token: "nonce".into(),
            role: Role::Server,
            package_hash: "ab".repeat(32),
            protocol_version: "arc-test+12345678".into(),
            sequence: 1,
            event: Event::Stage {
                stage: Stage::PackageVerified,
            },
        };
        for mutation in 0..5 {
            let mut bad = report.clone();
            match mutation {
                0 => bad.run = 8,
                1 => bad.package_hash = "cd".repeat(32),
                2 => bad.role = Role::Client,
                3 => bad.sequence = 2,
                _ => bad.token = "wrong".into(),
            };
            assert!(fence.accept(bad).is_err());
            assert_eq!(fence.sequence, 0);
        }
        fence.accept(report.clone()).unwrap();
        assert!(fence.accept(report.clone()).is_err());
        let mut client = Fence {
            role: PlayRole::Client,
            sequence: 0,
            protocol,
            ..fence
        };
        let mut bad = report;
        bad.role = Role::Client;
        bad.protocol_version = "arc-other+12345678".into();
        assert_eq!(
            client.accept(bad).unwrap_err().code,
            "play.protocol_mismatch"
        );
        assert_eq!(client.sequence, 0);
    }
}

#[cfg(test)]
mod project_argument_tests {
    use super::*;
    use std::path::Path;

    fn config(root: &Path, project: bool) -> PlayConfig {
        let config = PlayConfig::from_game_worktree(root.into(), root.join("logs"));
        if project {
            config.with_project_root(root.into())
        } else {
            config
        }
    }

    fn arguments(config: &PlayConfig, role: PlayRole) -> Vec<String> {
        project_arguments(config, role)
            .into_iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn without_a_project_folder_a_child_is_told_nothing_of_one() {
        let folder = tempfile::tempdir().unwrap();
        // Even in a folder that is a project: the configuration names none.
        crate::project::create_project(folder.path(), "Game").unwrap();
        let config = config(folder.path(), false);
        assert_eq!(config.project_root, None);
        assert_eq!(arguments(&config, PlayRole::Client), [] as [&str; 0]);
        assert_eq!(arguments(&config, PlayRole::Server), [] as [&str; 0]);
    }

    #[test]
    fn the_client_of_a_ready_project_is_told_its_folder_and_the_server_is_not() {
        let folder = tempfile::tempdir().unwrap();
        crate::project::create_project(folder.path(), "Game").unwrap();
        let config = config(folder.path(), true);
        // As the file system resolves it: the child's working directory is not the editor's.
        let resolved = std::fs::canonicalize(folder.path())
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            arguments(&config, PlayRole::Client),
            ["--project-root", resolved.as_str()]
        );
        assert_eq!(arguments(&config, PlayRole::Server), [] as [&str; 0]);
    }

    #[test]
    fn a_folder_that_is_no_ready_project_is_not_passed() {
        // No manifest yet.
        let folder = tempfile::tempdir().unwrap();
        let named = config(folder.path(), true);
        assert_eq!(arguments(&named, PlayRole::Client), [] as [&str; 0]);

        // A manifest of an older version, which waits to be converted, and one of a newer.
        let project = project_spec::ProjectConfig::new("Game");
        let older = project_spec::ProjectConfigV1 {
            schema: 1,
            project: project.project.clone(),
            startup: project.startup.clone(),
            rendering: project.rendering.clone(),
            controls: project.controls.clone(),
        };
        let manifest = crate::project::project_path(folder.path());
        std::fs::write(&manifest, serde_json::to_vec(&older).unwrap()).unwrap();
        assert_eq!(arguments(&named, PlayRole::Client), [] as [&str; 0]);
        let mut newer = serde_json::to_value(&project).unwrap();
        newer["schema"] = (project_spec::SCHEMA + 1).into();
        std::fs::write(&manifest, serde_json::to_vec(&newer).unwrap()).unwrap();
        assert_eq!(arguments(&named, PlayRole::Client), [] as [&str; 0]);
        // One that is no project file at all.
        std::fs::write(&manifest, b"{").unwrap();
        assert_eq!(arguments(&named, PlayRole::Client), [] as [&str; 0]);

        // Set up in the same session: the next client is told.
        std::fs::remove_file(&manifest).unwrap();
        crate::project::create_project(folder.path(), "Game").unwrap();
        assert_eq!(arguments(&named, PlayRole::Client).len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn a_project_whose_start_level_leads_out_of_the_folder_is_not_passed() {
        let outside = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), folder.path().join("levels")).unwrap();
        let mut project = project_spec::ProjectConfig::new("Game");
        project.startup.start_level = "levels/elsewhere.level.json".into();
        std::fs::write(
            crate::project::project_path(folder.path()),
            project_spec::canonical_bytes(&project).unwrap(),
        )
        .unwrap();
        std::fs::write(outside.path().join("elsewhere.level.json"), b"{}").unwrap();
        let named = config(folder.path(), true);
        assert_eq!(arguments(&named, PlayRole::Client), [] as [&str; 0]);
    }
}
