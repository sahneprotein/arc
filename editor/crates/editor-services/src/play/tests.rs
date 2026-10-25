use super::*;
use scene_data::Diagnostic;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
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
    withhold_first_frame: Option<Arc<AtomicBool>>,
    unexpected_exit: Option<PlayExit>,
    allow_exit: Option<Arc<AtomicBool>>,
}
impl PlayChild for FakeChild {
    fn pid(&self) -> u32 {
        42
    }
    fn poll(&mut self) -> Result<Vec<PlayRuntimeEvent>, Diagnostic> {
        let reports = std::mem::take(&mut self.reports);
        if self
            .withhold_first_frame
            .as_ref()
            .is_some_and(|ready| !ready.load(Ordering::SeqCst))
            && let Some(index) = reports.iter().position(|event| {
                matches!(
                    event,
                    PlayRuntimeEvent::Stage(PlayStage::FirstControllableFrame)
                )
            })
        {
            let mut reports = reports;
            let first_frame = reports.remove(index);
            self.reports.push(first_frame);
            return Ok(reports);
        }
        Ok(reports)
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
        if self.unexpected_exit.is_some()
            && self
                .allow_exit
                .as_ref()
                .is_none_or(|ready| ready.load(Ordering::SeqCst))
        {
            return Ok(self.unexpected_exit.take());
        }
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
        withhold_first_frame: None,
        unexpected_exit: None,
        allow_exit: None,
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
        withhold_first_frame: None,
        unexpected_exit: None,
        allow_exit: None,
    }
}
fn gated_client(ready: Arc<AtomicBool>) -> FakeChild {
    FakeChild {
        withhold_first_frame: Some(ready),
        ..client()
    }
}
fn exited_client(code: i32) -> FakeChild {
    FakeChild {
        reports: vec![],
        unexpected_exit: Some(PlayExit {
            code: Some(code),
            signal: None,
            forced: false,
        }),
        ..client()
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
                local_clients: 1,
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
                local_clients: 1,
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
fn coordinator_runs_two_local_clients_gates_all_readiness_and_reports_every_exit() {
    let (_root, receipt, config) = fixture();
    let second_ready = Arc::new(AtomicBool::new(false));
    let backend = Arc::new(FakeBackend {
        children: Mutex::new(VecDeque::from([
            server(false),
            client(),
            gated_client(second_ready.clone()),
        ])),
        ..Default::default()
    });
    let mut handle = PlayHandle::with_backend(config, backend.clone()).unwrap();
    handle
        .submit(PlayRequest {
            id: 10,
            control: PlayControl::Start {
                mode: PlayMode::ExternalGame,
                receipt,
                local_clients: 2,
            },
        })
        .unwrap();

    wait(&mut handle, |status| {
        status.run.as_ref().is_some_and(|run| {
            run.processes.iter().any(|process| {
                process.client_id == Some(1)
                    && process.stages.contains(&PlayStage::FirstControllableFrame)
            }) && run.processes.iter().any(|process| {
                process.client_id == Some(2)
                    && !process.stages.contains(&PlayStage::FirstControllableFrame)
            })
        })
    });
    let status = handle.snapshot();
    let run = status.run.as_ref().unwrap();
    assert_eq!(run.local_clients, 2);
    assert_eq!(run.state, PlayState::Starting);

    let launches = backend.launches.lock().unwrap();
    assert_eq!(launches.len(), 3);
    assert_eq!(launches[0].role, PlayRole::Server);
    assert_eq!(launches[0].client_id, None);
    assert_eq!(launches[0].local_clients, 2);
    assert_eq!(launches[1].client_id, Some(1));
    assert_eq!(launches[2].client_id, Some(2));
    assert!(launches.iter().all(|launch| launch.local_clients == 2));
    drop(launches);

    second_ready.store(true, Ordering::SeqCst);
    wait(&mut handle, |status| {
        status
            .run
            .as_ref()
            .is_some_and(|run| run.state == PlayState::Running)
    });
    let run_id = handle.snapshot().run.as_ref().unwrap().id;
    handle
        .submit(PlayRequest {
            id: 11,
            control: PlayControl::Stop { run: run_id },
        })
        .unwrap();
    wait(&mut handle, |status| {
        status
            .run
            .as_ref()
            .is_some_and(|run| matches!(run.state, PlayState::Ended(_)))
    });

    let status = handle.snapshot();
    let run = status.run.as_ref().unwrap();
    let PlayState::Ended(end) = &run.state else {
        panic!()
    };
    assert!(end.cleanup.server.as_ref().unwrap().success());
    assert!(end.cleanup.client.as_ref().unwrap().success());
    assert_eq!(end.cleanup.clients.len(), 2);
    assert_eq!(
        end.cleanup
            .clients
            .iter()
            .map(|client| (client.client_id, client.exit.as_ref().unwrap().success()))
            .collect::<Vec<_>>(),
        [(1, true), (2, true)]
    );
    assert!(run.processes.iter().all(|process| process.exit.is_some()));
}

#[test]
fn a_failed_local_client_stops_and_reports_the_server_and_every_client() {
    let (_root, receipt, config) = fixture();
    // Exercise a crash during play, after every client has reported readiness.
    // An exited_client has no readiness reports and instead exercises startup failure.
    let allow_exit = Arc::new(AtomicBool::new(false));
    let failing_client = FakeChild {
        unexpected_exit: exited_client(17).unexpected_exit,
        allow_exit: Some(allow_exit.clone()),
        ..client()
    };
    let backend = Arc::new(FakeBackend {
        children: Mutex::new(VecDeque::from([server(false), client(), failing_client])),
        ..Default::default()
    });
    let mut handle = PlayHandle::with_backend(config, backend).unwrap();
    handle
        .submit(PlayRequest {
            id: 1,
            control: PlayControl::Start {
                mode: PlayMode::ExternalGame,
                receipt,
                local_clients: 2,
            },
        })
        .unwrap();
    wait(&mut handle, |status| {
        status
            .run
            .as_ref()
            .is_some_and(|run| run.state == PlayState::Running)
    });
    allow_exit.store(true, Ordering::SeqCst);
    wait(&mut handle, |status| {
        status
            .run
            .as_ref()
            .is_some_and(|run| matches!(run.state, PlayState::Ended(_)))
    });
    let run = handle.snapshot().run.clone().unwrap();
    let PlayState::Ended(end) = &run.state else {
        panic!()
    };
    assert_eq!(end.primary.code, "play.crashed");
    assert_eq!(end.cause.as_ref().unwrap().code, "play.unexpected_exit");
    assert!(end.cleanup.server.as_ref().unwrap().success());
    assert_eq!(end.cleanup.clients.len(), 2);
    assert!(end.cleanup.clients[0].exit.as_ref().unwrap().success());
    assert_eq!(end.cleanup.clients[1].exit.as_ref().unwrap().code, Some(17));
    assert!(run.processes.iter().all(|process| process.exit.is_some()));
}

#[test]
fn a_second_client_launch_failure_reaps_launched_children_and_reports_unlaunched_client() {
    let (_root, receipt, config) = fixture();
    // The backend has a server and client 1, but fails when the coordinator requests client 2.
    let backend = Arc::new(FakeBackend {
        children: Mutex::new(VecDeque::from([server(false), client()])),
        ..Default::default()
    });
    let mut handle = PlayHandle::with_backend(config, backend.clone()).unwrap();
    handle
        .submit(PlayRequest {
            id: 1,
            control: PlayControl::Start {
                mode: PlayMode::ExternalGame,
                receipt,
                local_clients: 2,
            },
        })
        .unwrap();

    wait(&mut handle, |status| {
        status
            .run
            .as_ref()
            .is_some_and(|run| matches!(run.state, PlayState::Ended(_)))
    });
    let run = handle.snapshot().run.clone().unwrap();
    let PlayState::Ended(end) = &run.state else {
        panic!()
    };
    assert_eq!(end.primary.code, "play.start_failed");
    assert_eq!(end.cause.as_ref().unwrap().code, "fake.launch_failed");
    assert!(end.cleanup.server.as_ref().unwrap().success());
    assert_eq!(end.cleanup.clients.len(), 2);
    assert!(end.cleanup.clients[0]
        .exit
        .as_ref()
        .is_some_and(PlayExit::success));
    assert_eq!(end.cleanup.clients[1].client_id, 2);
    assert!(end.cleanup.clients[1].exit.is_none());
    assert!(run.processes[0]
        .exit
        .as_ref()
        .is_some_and(PlayExit::success));
    assert!(run.processes[1]
        .exit
        .as_ref()
        .is_some_and(PlayExit::success));
    assert!(run.processes[2].exit.is_none());
    assert_eq!(backend.launches.lock().unwrap().len(), 3);
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
                local_clients: 1,
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
                local_clients: 1,
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
                local_clients: 1,
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
                local_clients: 1,
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
