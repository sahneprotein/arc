use super::*;
use project_spec::{MAX_LOCAL_CLIENTS, MIN_LOCAL_CLIENTS};
use scene_data::Diagnostic;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_RUN: AtomicU64 = AtomicU64::new(1);

pub(crate) struct Model {
    pub status: PlayStatus,
    pub primary: Option<Diagnostic>,
    pub cause: Option<Diagnostic>,
}
impl Model {
    pub fn new() -> Self {
        Self {
            status: PlayStatus::default(),
            primary: None,
            cause: None,
        }
    }
    pub fn active(&self) -> bool {
        self.status
            .run
            .as_ref()
            .is_some_and(|r| !matches!(r.state, PlayState::Ended(_)))
    }
    pub fn start(
        &mut self,
        mode: PlayMode,
        receipt: crate::VerifiedPackageReceipt,
        local_clients: u8,
    ) -> Result<RunId, Diagnostic> {
        if self.active() {
            return Err(Diagnostic::new("play.already_active"));
        }
        if mode != PlayMode::ExternalGame {
            return Err(Diagnostic::new("play.mode_unsupported"));
        }
        if !(MIN_LOCAL_CLIENTS..=MAX_LOCAL_CLIENTS).contains(&local_clients) {
            return Err(
                Diagnostic::new("play.local_clients_invalid").with("clients", local_clients)
            );
        }
        let id = RunId(NEXT_RUN.fetch_add(1, Ordering::Relaxed));
        self.primary = None;
        self.cause = None;
        self.status.run = Some(PlayRun {
            id,
            mode,
            identity: receipt.identity().clone(),
            receipt,
            local_clients,
            state: PlayState::Starting,
            processes: std::iter::once(PlayProcess {
                role: PlayRole::Server,
                client_id: None,
                pid: None,
                stages: vec![],
                listening: None,
                log_path: None,
                exit: None,
            })
            .chain((1..=local_clients).map(|client_id| PlayProcess {
                role: PlayRole::Client,
                client_id: Some(client_id),
                pid: None,
                stages: vec![],
                listening: None,
                log_path: None,
                exit: None,
            }))
            .collect(),
        });
        Ok(id)
    }
    pub fn stop(&mut self, id: RunId) -> Result<(), Diagnostic> {
        let Some(run) = self.status.run.as_mut() else {
            return Err(Diagnostic::new("play.no_active_run"));
        };
        if run.id != id {
            return Err(Diagnostic::new("play.stale_run"));
        }
        if matches!(run.state, PlayState::Ended(_)) {
            return Err(Diagnostic::new("play.no_active_run"));
        }
        if self.primary.is_none() {
            self.primary = Some(Diagnostic::new("play.stopped"));
        }
        run.state = PlayState::Stopping;
        Ok(())
    }
    pub fn process(&mut self, role: PlayRole, client_id: Option<u8>) -> &mut PlayProcess {
        self.status
            .run
            .as_mut()
            .unwrap()
            .processes
            .iter_mut()
            .find(|p| p.role == role && p.client_id == client_id)
            .unwrap()
    }
    pub fn stage(
        &mut self,
        role: PlayRole,
        client_id: Option<u8>,
        stage: PlayStage,
    ) -> Result<bool, Diagnostic> {
        if !self.active() || matches!(self.status.run.as_ref().unwrap().state, PlayState::Stopping)
        {
            return Ok(false);
        }
        let known_process = self
            .status
            .run
            .as_ref()
            .unwrap()
            .processes
            .iter()
            .any(|process| process.role == role && process.client_id == client_id);
        if !known_process {
            return Err(Diagnostic::new("play.process_unknown"));
        }
        let expected: &[PlayStage] = match role {
            PlayRole::Server => &[PlayStage::PackageVerified, PlayStage::CollisionReady],
            PlayRole::Client => &[
                PlayStage::PackageVerified,
                PlayStage::CollisionReady,
                PlayStage::HandshakeAccepted,
                PlayStage::Simulating,
                PlayStage::LevelDrawn,
                PlayStage::FirstControllableFrame,
            ],
        };
        if role == PlayRole::Client && stage == PlayStage::FirstControllableFrame {
            let run = self.status.run.as_ref().unwrap();
            if !run.processes[0].stages.contains(&PlayStage::CollisionReady)
                || run.processes[0].listening.is_none()
            {
                return Err(Diagnostic::new("play.server_not_ready"));
            }
        }
        let process = self.process(role, client_id);
        if process.stages.contains(&stage) {
            return Ok(false);
        }
        if expected.get(process.stages.len()) != Some(&stage) {
            return Err(Diagnostic::new("play.readiness_order"));
        }
        process.stages.push(stage);
        if role == PlayRole::Client && stage == PlayStage::FirstControllableFrame {
            let run = self.status.run.as_mut().unwrap();
            let all_clients_ready = run
                .processes
                .iter()
                .filter(|p| p.role == PlayRole::Client)
                .all(|p| p.stages.contains(&PlayStage::FirstControllableFrame));
            if all_clients_ready {
                run.state = PlayState::Running;
            }
        }
        Ok(true)
    }
    pub fn fail(&mut self, code: &str, cause: Option<Diagnostic>) {
        if !self.active() {
            return;
        }
        // Crash facts remain visible even if a stop was already requested.
        if self.primary.is_none() || code == "play.crashed" {
            self.primary = Some(Diagnostic::new(code));
            self.cause = cause;
        }
        self.status.run.as_mut().unwrap().state = PlayState::Stopping;
    }
    pub fn finish(&mut self, cleanup: PlayCleanup) -> Option<(RunId, PlayEnd)> {
        if !self.active() {
            return None;
        }
        let run = self.status.run.as_mut().unwrap();
        let end = PlayEnd {
            primary: self
                .primary
                .take()
                .unwrap_or_else(|| Diagnostic::new("play.crashed")),
            cause: self.cause.take(),
            cleanup,
        };
        run.state = PlayState::Ended(end.clone());
        Some((run.id, end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn receipt() -> (tempfile::TempDir, crate::VerifiedPackageReceipt) {
        let root = tempfile::tempdir().unwrap();
        let cooked = editor_cook::cook(&scene_data::current_map()).unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let path = crate::publish_package(root.path(), &cooked, &cancel).unwrap();
        let receipt = crate::verify_package(&path, None, &cancel).unwrap();
        (root, receipt)
    }
    #[test]
    fn readiness_is_measured_and_first_frame_requires_all_stages() {
        let (_root, receipt) = receipt();
        let mut model = Model::new();
        model.start(PlayMode::ExternalGame, receipt, 2).unwrap();
        assert!(model
            .stage(PlayRole::Client, Some(1), PlayStage::FirstControllableFrame)
            .is_err());
        assert_eq!(
            model.status.run.as_ref().unwrap().state,
            PlayState::Starting
        );
        for stage in [PlayStage::PackageVerified, PlayStage::CollisionReady] {
            model.stage(PlayRole::Server, None, stage).unwrap();
        }
        model.process(PlayRole::Server, None).listening = Some("127.0.0.1:7777".parse().unwrap());
        for stage in [
            PlayStage::PackageVerified,
            PlayStage::CollisionReady,
            PlayStage::HandshakeAccepted,
            PlayStage::Simulating,
            PlayStage::LevelDrawn,
            PlayStage::FirstControllableFrame,
        ] {
            model.stage(PlayRole::Client, Some(1), stage).unwrap();
        }
        assert_eq!(
            model.status.run.as_ref().unwrap().state,
            PlayState::Starting
        );
        for stage in [
            PlayStage::PackageVerified,
            PlayStage::CollisionReady,
            PlayStage::HandshakeAccepted,
            PlayStage::Simulating,
            PlayStage::LevelDrawn,
            PlayStage::FirstControllableFrame,
        ] {
            model.stage(PlayRole::Client, Some(2), stage).unwrap();
        }
        assert_eq!(model.status.run.as_ref().unwrap().state, PlayState::Running);
    }

    #[test]
    fn client_count_is_explicit_bounded_and_expanded_into_processes() {
        let (_root, receipt) = receipt();
        for count in [0, 5] {
            assert_eq!(
                Model::new()
                    .start(PlayMode::ExternalGame, receipt.clone(), count)
                    .unwrap_err()
                    .code,
                "play.local_clients_invalid"
            );
        }
        let mut model = Model::new();
        model.start(PlayMode::ExternalGame, receipt, 4).unwrap();
        let processes = &model.status.run.as_ref().unwrap().processes;
        assert_eq!(model.status.run.as_ref().unwrap().local_clients, 4);
        assert_eq!(processes.len(), 5);
        assert_eq!(processes[0].role, PlayRole::Server);
        assert_eq!(processes[0].client_id, None);
        assert_eq!(
            processes[1..]
                .iter()
                .map(|process| process.client_id.unwrap())
                .collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
    }
    #[test]
    fn stop_start_and_stale_run_are_fenced_and_terminal_is_once() {
        let (_root, receipt) = receipt();
        let mut m = Model::new();
        let id = m.start(PlayMode::ExternalGame, receipt.clone(), 1).unwrap();
        assert_eq!(
            m.start(PlayMode::ExternalGame, receipt.clone(), 1)
                .unwrap_err()
                .code,
            "play.already_active"
        );
        m.stop(id).unwrap();
        assert_eq!(
            m.start(PlayMode::ExternalGame, receipt.clone(), 1)
                .unwrap_err()
                .code,
            "play.already_active"
        );
        let (_, end) = m.finish(PlayCleanup::default()).unwrap();
        assert_eq!(end.primary.code, "play.stopped");
        assert!(m.finish(PlayCleanup::default()).is_none());
        let next = m.start(PlayMode::ExternalGame, receipt, 1).unwrap();
        assert_ne!(id, next);
        assert_eq!(m.stop(id).unwrap_err().code, "play.stale_run");
        assert_eq!(m.status.run.as_ref().unwrap().state, PlayState::Starting);
    }
    #[test]
    fn cleanup_failure_preserves_crash_and_exit_facts() {
        let (_root, receipt) = receipt();
        let mut m = Model::new();
        m.start(PlayMode::ExternalGame, receipt, 1).unwrap();
        m.fail("play.crashed", Some(Diagnostic::new("runtime.panic")));
        let cleanup = PlayCleanup {
            server: Some(PlayExit {
                code: None,
                signal: Some(9),
                forced: true,
            }),
            client: None,
            clients: vec![PlayClientExit {
                client_id: 1,
                exit: None,
            }],
            diagnostics: vec![Diagnostic::new("play.server_cleanup_failed")],
        };
        let (_, end) = m.finish(cleanup.clone()).unwrap();
        assert_eq!(end.primary.code, "play.crashed");
        assert_eq!(end.cause.unwrap().code, "runtime.panic");
        assert_eq!(end.cleanup, cleanup);
    }
}
