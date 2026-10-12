//! A game that is not a game: a stand-in for the game's processes that behaves as scripted.
//!
//! The editor's Play is checked without the game's binaries. The core's Play coordinator
//! takes any launcher; this one launches nothing and answers as a server and a client would,
//! in one of a few ways: it runs until stopped, never becomes ready, crashes, has its window
//! closed, or leaves a server that will not go. It is used by this crate's tests and by the
//! application's scripted checks, and by nothing else.

use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use editor_core::Diagnostic;
use editor_services::{
    PlayBackend, PlayChild, PlayConfig, PlayExit, PlayHandle, PlayLaunch, PlayRole,
    PlayRuntimeEvent, PlayStage,
};

/// How the scripted game behaves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Script {
    /// Server and client become ready and run until they are stopped.
    Runs,
    /// The client never becomes ready: the run stays "starting" until it is stopped.
    Hangs,
    /// The client becomes ready and then ends by itself with exit code 101.
    Crashes,
    /// The client becomes ready and the person closes its window.
    WindowClosed,
    /// As `Runs`, but the server ignores being stopped and being killed.
    ServerStays,
    /// As `Runs`, but the server ignores being asked to stop and goes only when it is killed.
    NeedsKill,
    /// Nothing can be launched: the launcher says its adapter is not there, as the core's
    /// own launcher does until the game can be started for real.
    Unavailable,
}

impl Script {
    /// The script named in a scripted check.
    pub fn named(name: &str) -> Option<Self> {
        Some(match name {
            "runs" => Self::Runs,
            "hangs" => Self::Hangs,
            "crashes" => Self::Crashes,
            "window-closed" => Self::WindowClosed,
            "server-stays" => Self::ServerStays,
            "needs-kill" => Self::NeedsKill,
            "unavailable" => Self::Unavailable,
            _ => return None,
        })
    }

    /// A Play handle of the core whose game is this script. The waits are short, so that a
    /// server that will not go is given up within half a second.
    pub fn handle(self) -> Result<PlayHandle, Diagnostic> {
        let folder = std::env::temp_dir();
        let mut config = PlayConfig::from_game_worktree(folder.clone(), folder.join("play-logs"));
        config.stop_timeout = Duration::from_millis(250);
        config.reap_timeout = Duration::from_millis(150);
        PlayHandle::with_backend(config, Arc::new(self))
    }
}

/// The process numbers the scripted game reports. `CLIENT_PID` is the first client's; each
/// further client of a run has the next number.
pub const SERVER_PID: u32 = 4312;
pub const CLIENT_PID: u32 = 4313;

/// How often the client is asked before it ends by itself, in the scripts where it does:
/// long enough for the run to have been seen running.
const POLLS_BEFORE_THE_END: u32 = 40;

impl PlayBackend for Script {
    fn launch(&self, launch: PlayLaunch) -> Result<Box<dyn PlayChild>, Diagnostic> {
        if *self == Self::Unavailable {
            return Err(Diagnostic::new("play.runtime_adapter_unavailable"));
        }
        // The first client is the one every run has, with the number and the log it always
        // had; a further local test client has its own of each.
        let (pid, name) = match (launch.role, launch.client_id) {
            (PlayRole::Server, _) => (SERVER_PID, "server".to_owned()),
            (PlayRole::Client, None | Some(1)) => (CLIENT_PID, "client".to_owned()),
            (PlayRole::Client, Some(id)) => {
                (CLIENT_PID + u32::from(id) - 1, format!("client-{id}"))
            }
        };
        Ok(Box::new(Scripted {
            script: *self,
            role: launch.role,
            pid,
            log: launch.config.log_directory.join(format!("{name}.log")),
            polls: 0,
            reports: 0,
            stopped: false,
            killed: false,
            closed: false,
        }))
    }
}

struct Scripted {
    script: Script,
    role: PlayRole,
    pid: u32,
    log: PathBuf,
    /// How often it was asked for news, and how many reports it has given.
    polls: u32,
    reports: usize,
    stopped: bool,
    killed: bool,
    /// Whether it reported that the person closed its window.
    closed: bool,
}

impl Scripted {
    /// What the process reports while it starts, one report each time it is asked.
    fn start_up(&self) -> Vec<PlayRuntimeEvent> {
        use PlayRuntimeEvent::{Listening, Stage};
        match self.role {
            PlayRole::Server => vec![
                Stage(PlayStage::PackageVerified),
                Stage(PlayStage::CollisionReady),
                Listening(SocketAddr::from((Ipv4Addr::LOCALHOST, 47_777))),
            ],
            PlayRole::Client => {
                let mut stages = vec![
                    Stage(PlayStage::PackageVerified),
                    Stage(PlayStage::CollisionReady),
                    Stage(PlayStage::HandshakeAccepted),
                ];
                if self.script != Script::Hangs {
                    stages.extend([
                        Stage(PlayStage::Simulating),
                        Stage(PlayStage::LevelDrawn),
                        Stage(PlayStage::FirstControllableFrame),
                    ]);
                }
                stages
            }
        }
    }

    /// Whether the client's own end has come, in the scripts where it ends by itself.
    fn ends_by_itself(&self) -> bool {
        self.role == PlayRole::Client
            && matches!(self.script, Script::Crashes | Script::WindowClosed)
            && self.polls > self.start_up().len() as u32 + POLLS_BEFORE_THE_END
    }

    fn stays(&self) -> bool {
        self.script == Script::ServerStays && self.role == PlayRole::Server
    }
}

impl PlayChild for Scripted {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn log_path(&self) -> Option<PathBuf> {
        Some(self.log.clone())
    }

    fn poll(&mut self) -> Result<Vec<PlayRuntimeEvent>, Diagnostic> {
        self.polls += 1;
        let start_up = self.start_up();
        if let Some(report) = start_up.get(self.reports) {
            self.reports += 1;
            return Ok(vec![report.clone()]);
        }
        if self.script == Script::WindowClosed && self.ends_by_itself() && !self.closed {
            self.closed = true;
            return Ok(vec![PlayRuntimeEvent::ExpectedWindowClose]);
        }
        Ok(Vec::new())
    }

    fn stop(&mut self) -> Result<(), Diagnostic> {
        self.stopped = true;
        Ok(())
    }

    fn kill(&mut self) -> Result<(), Diagnostic> {
        self.killed = true;
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<PlayExit>, Diagnostic> {
        if self.stays() {
            return Ok(None);
        }
        let exit = |code: Option<i32>, signal: Option<i32>, forced: bool| PlayExit {
            code,
            signal,
            forced,
        };
        let deaf = self.script == Script::NeedsKill && self.role == PlayRole::Server;
        Ok(if self.killed {
            Some(exit(None, Some(9), true))
        } else if (self.stopped && !deaf) || self.closed {
            Some(exit(Some(0), None, false))
        } else if self.script == Script::Crashes && self.ends_by_itself() {
            Some(exit(Some(101), None, false))
        } else {
            None
        })
    }
}
