use crate::{PackageIdentity, VerifiedPackageReceipt};
use scene_data::Diagnostic;
use std::{path::PathBuf, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RunId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayMode {
    InEditor,
    ExternalGame,
}
impl PlayMode {
    pub fn key(self) -> &'static str {
        match self {
            Self::InEditor => "in_editor",
            Self::ExternalGame => "external_game",
        }
    }
}
#[allow(
    clippy::large_enum_variant,
    reason = "The agreed UI control seam carries a concrete verified receipt; requests are bounded."
)]
#[derive(Clone, Debug)]
pub enum PlayControl {
    Start {
        mode: PlayMode,
        receipt: VerifiedPackageReceipt,
        /// Number of local test client processes. The caller resolves the project setting
        /// (`None` means one); the coordinator accepts only 1 through 4.
        local_clients: u8,
    },
    Stop {
        run: RunId,
    },
}
#[derive(Clone, Debug)]
pub struct PlayRequest {
    pub id: u64,
    pub control: PlayControl,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayAnswer {
    pub request: u64,
    pub run: Option<RunId>,
    /// Success acknowledges the control; runtime readiness is reported separately.
    pub result: Result<(), Diagnostic>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayRole {
    Server,
    Client,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayStage {
    PackageVerified,
    CollisionReady,
    HandshakeAccepted,
    Simulating,
    LevelDrawn,
    FirstControllableFrame,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayExit {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub forced: bool,
}
impl PlayExit {
    pub fn success(&self) -> bool {
        self.code == Some(0) && self.signal.is_none() && !self.forced
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayProcess {
    pub role: PlayRole,
    /// `None` for the server; 1 through N for local client processes.
    pub client_id: Option<u8>,
    pub pid: Option<u32>,
    pub stages: Vec<PlayStage>,
    pub listening: Option<std::net::SocketAddr>,
    pub log_path: Option<PathBuf>,
    pub exit: Option<PlayExit>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayCleanup {
    pub server: Option<PlayExit>,
    /// Compatibility view of local client 1. Use `clients` for all client outcomes.
    pub client: Option<PlayExit>,
    /// Every local client, in ascending client ID order, including a missing exit when it
    /// could not be reaped before the cleanup deadline.
    pub clients: Vec<PlayClientExit>,
    /// Includes play.server_cleanup_failed without replacing the primary outcome.
    pub diagnostics: Vec<Diagnostic>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayClientExit {
    pub client_id: u8,
    pub exit: Option<PlayExit>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayEnd {
    pub primary: Diagnostic,
    pub cause: Option<Diagnostic>,
    pub cleanup: PlayCleanup,
}
#[allow(
    clippy::large_enum_variant,
    reason = "Preserve the agreed Ended(PlayEnd) UI seam; cached status retains one run."
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayState {
    Starting,
    Running,
    Stopping,
    Ended(PlayEnd),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayRun {
    pub id: RunId,
    pub mode: PlayMode,
    pub receipt: VerifiedPackageReceipt,
    pub identity: PackageIdentity,
    /// Number of local client processes requested for this run.
    pub local_clients: u8,
    pub state: PlayState,
    pub processes: Vec<PlayProcess>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayStatus {
    pub revision: u64,
    pub run: Option<PlayRun>,
}
#[derive(Clone, Debug)]
pub enum PlayEvent {
    Answer(PlayAnswer),
    Ended { run: RunId, end: PlayEnd },
}

/// Explicit native runtime paths. No PATH search, implicit build or game crate dependency.
#[derive(Clone, Debug)]
pub struct PlayConfig {
    pub server: PathBuf,
    pub client: PathBuf,
    /// Directory containing assets/, passed as BEVY_ASSET_ROOT.
    pub asset_root: PathBuf,
    pub working_directory: PathBuf,
    /// The project's folder, if the game is started on a project: the folder with
    /// a unique `.aproj`. It is passed to the client as `--project-root`, and only while
    /// that manifest is one this editor reads, which is looked at each time a client is
    /// started. Without it, or with a folder that is not a ready project, the client is
    /// started exactly as it always was.
    pub project_root: Option<PathBuf>,
    pub log_directory: PathBuf,
    pub startup_timeout: Duration,
    pub stop_timeout: Duration,
    pub reap_timeout: Duration,
}
impl PlayConfig {
    /// Known development layout; paths are checked on the supervisor, never the UI.
    pub fn from_game_worktree(root: PathBuf, logs: PathBuf) -> Self {
        let binary = |name: &str| {
            root.join("target/debug")
                .join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
        };
        Self {
            server: binary("server"),
            client: binary("client"),
            asset_root: root.clone(),
            working_directory: root,
            project_root: None,
            log_directory: logs,
            startup_timeout: Duration::from_secs(30),
            stop_timeout: Duration::from_secs(3),
            reap_timeout: Duration::from_secs(2),
        }
    }
    /// The same, with this folder as the project the game is started on
    /// ([`PlayConfig::project_root`]).
    pub fn with_project_root(mut self, root: PathBuf) -> Self {
        self.project_root = Some(root);
        self
    }
}
