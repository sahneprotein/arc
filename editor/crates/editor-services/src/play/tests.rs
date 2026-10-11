use super::*;
use scene_data::Diagnostic;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
#[derive(Default)]
struct FakeBackend {
    launches: Mutex<Vec<PlayLaunch>>,
    children: Mutex<VecDeque<FakeChild>>,
}
impl PlayBackend for FakeBackend {
    fn launch(&self, launch: PlayLaunch) -> Result<Box<dyn PlayChild>, Diagnostic> {
        self.launches.lock().unwrap().push(launch);
        self.children
            .lock()
            .unwrap()
            .pop_front()
            .map(|c| Box::new(c) as Box<dyn PlayChild>)
            .ok_or_else(|| Diagnostic::new("fake.launch_failed"))
    }
}
struct FakeChild {
    reports: Vec<PlayRuntimeEvent>,
    stopped: bool,
    killed: bool,
    stubborn: bool,
}
impl PlayChild for FakeChild {
    fn pid(&self) -> u32 {
        42
    }
    fn poll(&mut self) -> Result<Vec<PlayRuntimeEvent>, Diagnostic> {
        Ok(std::mem::take(&mut self.reports))
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
        Ok(
            (self.killed || self.stopped && !self.stubborn).then_some(PlayExit {
                code: Some(if self.killed { 1 } else { 0 }),
                signal: None,
                forced: self.killed,
            }),
        )
    }
}
fn fixture() -> (tempfile::TempDir, crate::VerifiedPackageReceipt, PlayConfig) {
    let root = tempfile::tempdir().unwrap();
    let cooked = editor_cook::cook(&scene_data::current_map()).unwrap();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let path = crate::publish_package(root.path(), &cooked, &cancel).unwrap();
    let receipt = crate::verify_package(&path, None, &cancel).unwrap();
    let mut config = PlayConfig::from_game_worktree(root.path().into(), root.path().join("logs"));
    config.startup_timeout = Duration::from_secs(1);
    config.stop_timeout = Duration::from_millis(20);
    config.reap_timeout = Duration::from_millis(20);
    (root, receipt, config)
}
fn wait(handle: &mut PlayHandle, predicate: impl Fn(&PlayStatus) -> bool) -> Vec<PlayEvent> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut events = vec![];
    loop {
        events.extend(handle.poll(100));
        if predicate(&handle.snapshot()) {
            events.extend(handle.poll(100));
            return events;
        }
        assert!(
            Instant::now() < deadline,
            "timeout: {:?}",
            handle.snapshot()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn server(stubborn: bool) -> FakeChild {
    FakeChild {
        reports: vec![
            PlayRuntimeEvent::Stage(PlayStage::PackageVerified),
            PlayRuntimeEvent::Stage(PlayStage::CollisionReady),
            PlayRuntimeEvent::Listening("127.0.0.1:7777".parse().unwrap()),
        ],
        stopped: false,
        killed: false,
        stubborn,
    }
}
fn client() -> FakeChild {
    FakeChild {
        reports: [
            PlayStage::PackageVerified,
            PlayStage::CollisionReady,
            PlayStage::HandshakeAccepted,
            PlayStage::Simulating,
            PlayStage::LevelDrawn,
            PlayStage::FirstControllableFrame,
        ]
        .into_iter()
        .map(PlayRuntimeEvent::Stage)
        .collect(),
        stopped: false,
        killed: false,
        stubborn: false,
    }
}
#[test]
fn coordinator_runs_one_pair_pins_identity_and_stops_once() {
    let (_root, receipt, config) = fixture();
    let backend = Arc::new(FakeBackend {
        children: Mutex::new(VecDeque::from([server(false), client()])),
        ..Default::default()
    });
    let mut handle = PlayHandle::with_backend(config, backend.clone()).unwrap();
    let wakes = Arc::new(AtomicUsize::new(0));
    let wake = wakes.clone();
    handle.set_waker(Box::new(move || {
        wake.fetch_add(1, Ordering::SeqCst);
    }));
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    handle
        .submit(PlayRequest {
            id: 10,
            control: PlayControl::Start {
                mode: PlayMode::ExternalGame,
                receipt: receipt.clone(),
            },
        })
        .unwrap();
    let events = wait(&mut handle, |s| {
        s.run
            .as_ref()
            .is_some_and(|r| r.state == PlayState::Running)
    });
    assert!(events
        .iter()
        .any(|e| matches!(e,PlayEvent::Answer(a) if a.request==10 && a.result.is_ok())));
    let snapshot = handle.snapshot();
    let run = snapshot.run.as_ref().unwrap();
    let id = run.id;
    assert_eq!(run.identity, *receipt.identity());
    assert_eq!(run.processes.len(), 2);
    let launches = backend.launches.lock().unwrap();
    assert_eq!(launches.len(), 2);
    assert_eq!(launches[0].receipt, launches[1].receipt);
    assert_eq!(
        launches[1].server_address,
        Some("127.0.0.1:7777".parse().unwrap())
    );
    drop(launches);
    handle
        .submit(PlayRequest {
            id: 11,
            control: PlayControl::Start {
                mode: PlayMode::ExternalGame,
                receipt,
            },
        })
        .unwrap();
    handle
        .submit(PlayRequest {
            id: 12,
            control: PlayControl::Stop {
                run: RunId(id.0 + 100),
            },
        })
        .unwrap();
    handle
        .submit(PlayRequest {
            id: 13,
            control: PlayControl::Stop { run: id },
        })
        .unwrap();
    let events = wait(&mut handle, |s| {
        s.run
            .as_ref()
            .is_some_and(|r| matches!(r.state, PlayState::Ended(_)))
    });
    assert!(events.iter().any(|e|matches!(e,PlayEvent::Answer(a) if a.request==11 && a.result.as_ref().unwrap_err().code=="play.already_active")));
    assert!(events.iter().any(|e|matches!(e,PlayEvent::Answer(a) if a.request==12 && a.result.as_ref().unwrap_err().code=="play.stale_run")));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, PlayEvent::Ended { .. }))
            .count(),
        1
    );
    let snapshot = handle.snapshot();
    let PlayState::Ended(end) = &snapshot.run.as_ref().unwrap().state else {
        panic!()
    };
    assert_eq!(end.primary.code, "play.stopped");
    assert!(end.cleanup.server.as_ref().unwrap().success());
    assert!(end.cleanup.diagnostics.is_empty());
    assert!(wakes.load(Ordering::SeqCst) > 3);
}
#[test]
fn launch_failure_keeps_cause_and_stop_escalation_keeps_primary() {
    let (_root, receipt, config) = fixture();
    let mut handle =
        PlayHandle::with_backend(config.clone(), Arc::new(FakeBackend::default())).unwrap();
    handle
        .submit(PlayRequest {
            id: 1,
            control: PlayControl::Start {
                mode: PlayMode::ExternalGame,
                receipt: receipt.clone(),
            },
        })
        .unwrap();
    wait(&mut handle, |s| {
        s.run
            .as_ref()
            .is_some_and(|r| matches!(r.state, PlayState::Ended(_)))
    });
    let snapshot = handle.snapshot();
    let PlayState::Ended(end) = &snapshot.run.as_ref().unwrap().state else {
        panic!()
    };
    assert_eq!(end.primary.code, "play.start_failed");
    assert_eq!(end.cause.as_ref().unwrap().code, "fake.launch_failed");
    let backend = Arc::new(FakeBackend {
        children: Mutex::new(VecDeque::from([server(true), client()])),
        ..Default::default()
    });
    let mut handle = PlayHandle::with_backend(config, backend).unwrap();
    handle
        .submit(PlayRequest {
            id: 2,
            control: PlayControl::Start {
                mode: PlayMode::ExternalGame,
                receipt,
            },
        })
        .unwrap();
    wait(&mut handle, |s| {
        s.run
            .as_ref()
            .is_some_and(|r| r.state == PlayState::Running)
    });
    let id = handle.snapshot().run.as_ref().unwrap().id;
    handle
        .submit(PlayRequest {
            id: 3,
            control: PlayControl::Stop { run: id },
        })
        .unwrap();
    wait(&mut handle, |s| {
        s.run
            .as_ref()
            .is_some_and(|r| matches!(r.state, PlayState::Ended(_)))
    });
    let snapshot = handle.snapshot();
    let PlayState::Ended(end) = &snapshot.run.as_ref().unwrap().state else {
        panic!()
    };
    assert_eq!(end.primary.code, "play.stopped");
    assert!(end.cleanup.server.as_ref().unwrap().forced);
    assert!(end
        .cleanup
        .diagnostics
        .iter()
        .any(|d| d.code == "play.server_cleanup_failed"));
}
#[test]
fn snapshot_does_not_wait_for_publisher_lock() {
    let (_root, _receipt, config) = fixture();
    let handle = PlayHandle::with_backend(config, Arc::new(FakeBackend::default())).unwrap();
    let _publishing = handle.shared.status.write().unwrap();
    // Contention returns the cached revision immediately.
    assert_eq!(handle.snapshot().revision, 0);
}

#[test]
fn receipt_mutation_fails_before_any_process_launch() {
    let (_root, receipt, config) = fixture();
    let backend = Arc::new(FakeBackend::default());
    std::fs::write(receipt.path().join("level.sim.json"), b"{}").unwrap();
    let mut handle = PlayHandle::with_backend(config.clone(), backend.clone()).unwrap();
    handle
        .submit(PlayRequest {
            id: 1,
            control: PlayControl::Start {
                mode: PlayMode::ExternalGame,
                receipt: receipt.clone(),
            },
        })
        .unwrap();
    wait(&mut handle, |s| {
        s.run
            .as_ref()
            .is_some_and(|r| matches!(r.state, PlayState::Ended(_)))
    });
    let snapshot = handle.snapshot();
    let PlayState::Ended(end) = &snapshot.run.as_ref().unwrap().state else {
        panic!()
    };
    assert_eq!(end.primary.code, "play.start_failed");
    assert_eq!(end.cause.as_ref().unwrap().code, "payload.hash_mismatch");
    assert!(backend.launches.lock().unwrap().is_empty());
}

#[test]
fn stop_queued_during_starting_prevents_process_launch() {
    let (_root, receipt, config) = fixture();
    let backend = Arc::new(FakeBackend::default());
    let mut handle = PlayHandle::with_backend(config, backend.clone()).unwrap();
    let (started, starting) = std::sync::mpsc::sync_channel(1);
    let (resume, gate) = std::sync::mpsc::sync_channel(1);
    let gate = Mutex::new(gate);
    let shared = handle.shared.clone();
    let once = std::sync::atomic::AtomicBool::new(false);
    // A deterministic test gate pauses immediately after Starting publication.
    handle.set_waker(Box::new(move || {
        let status = shared.status.read().unwrap().clone();
        if let Some(run) = &status.run
            && run.state == PlayState::Starting
            && !once.swap(true, Ordering::SeqCst)
        {
            started.try_send(run.id).unwrap();
            gate.lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(3))
                .unwrap();
        }
    }));
    handle
        .submit(PlayRequest {
            id: 1,
            control: PlayControl::Start {
                mode: PlayMode::ExternalGame,
                receipt,
            },
        })
        .unwrap();
    let id = starting.recv_timeout(Duration::from_secs(3)).unwrap();
    handle
        .submit(PlayRequest {
            id: 2,
            control: PlayControl::Stop { run: id },
        })
        .unwrap();
    resume.send(()).unwrap();
    let events = wait(&mut handle, |s| {
        s.run
            .as_ref()
            .is_some_and(|r| matches!(r.state, PlayState::Ended(_)))
    });
    assert!(backend.launches.lock().unwrap().is_empty());
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, PlayEvent::Answer(_)))
            .count(),
        2
    );
    let status = handle.snapshot();
    let PlayState::Ended(end) = &status.run.as_ref().unwrap().state else {
        panic!()
    };
    assert_eq!(end.primary.code, "play.stopped");
}

mod cleanup;
