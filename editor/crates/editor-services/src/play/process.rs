//! Explicit executable paths + loopback channel. Never scrape stdout for readiness.
use super::{wire::*, *};
use scene_data::Diagnostic;
use std::{
    fs::File,
    io::{Read, Write},
    net::{TcpListener, TcpStream, UdpSocket},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
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
