use super::*;
use scene_data::Diagnostic;
use std::{net::SocketAddr, path::PathBuf};

/// Events from an authenticated, run-fenced runtime channel, never inferred from logs.
#[derive(Clone, Debug)]
pub enum PlayRuntimeEvent {
    Stage(PlayStage),
    Listening(SocketAddr),
    ExpectedWindowClose,
}
#[derive(Clone, Debug)]
pub struct PlayLaunch {
    pub run: RunId,
    pub role: PlayRole,
    /// `None` for the server; `Some(1..=N)` for a local client process.
    pub client_id: Option<u8>,
    /// Local client count requested for this run, passed to the server as its capacity.
    pub local_clients: u8,
    pub receipt: crate::VerifiedPackageReceipt,
    pub server_address: Option<SocketAddr>,
    pub config: PlayConfig,
}
/// Implementations must keep poll/stop/kill/try_wait nonblocking. Exit means reaped.
pub trait PlayChild: Send {
    fn pid(&self) -> u32;
    fn log_path(&self) -> Option<PathBuf> {
        None
    }
    fn poll(&mut self) -> Result<Vec<PlayRuntimeEvent>, Diagnostic>;
    fn stop(&mut self) -> Result<(), Diagnostic>;
    fn kill(&mut self) -> Result<(), Diagnostic>;
    fn try_wait(&mut self) -> Result<Option<PlayExit>, Diagnostic>;
}
/// Injectable launcher for scripted coordinator/UI tests; invoked only on the supervisor.
pub trait PlayBackend: Send + Sync + 'static {
    fn launch(&self, launch: PlayLaunch) -> Result<Box<dyn PlayChild>, Diagnostic>;
}
