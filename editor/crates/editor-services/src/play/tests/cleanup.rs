use super::*;

#[test]
fn failed_kill_and_unreaped_server_end_at_the_cleanup_deadline() {
    struct NeverReaped {
        reports: Vec<PlayRuntimeEvent>,
    }
    impl PlayChild for NeverReaped {
        fn pid(&self) -> u32 {
            43
        }
        fn poll(&mut self) -> Result<Vec<PlayRuntimeEvent>, Diagnostic> {
            Ok(std::mem::take(&mut self.reports))
        }
        fn stop(&mut self) -> Result<(), Diagnostic> {
            Ok(())
        }
        fn kill(&mut self) -> Result<(), Diagnostic> {
            Err(Diagnostic::new("fake.kill_denied"))
        }
        fn try_wait(&mut self) -> Result<Option<PlayExit>, Diagnostic> {
            Ok(None)
        }
    }
    struct Backend;
    impl PlayBackend for Backend {
        fn launch(&self, launch: PlayLaunch) -> Result<Box<dyn PlayChild>, Diagnostic> {
            Ok(match launch.role {
                PlayRole::Server => Box::new(NeverReaped {
                    reports: server(false).reports,
                }),
                PlayRole::Client => Box::new(client()),
            })
        }
    }
    let (_root, receipt, config) = fixture();
    let mut handle = PlayHandle::with_backend(config, Arc::new(Backend)).unwrap();
    handle
        .submit(PlayRequest {
            id: 1,
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
    let start = Instant::now();
    handle
        .submit(PlayRequest {
            id: 2,
            control: PlayControl::Stop { run: id },
        })
        .unwrap();
    wait(&mut handle, |s| {
        s.run
            .as_ref()
            .is_some_and(|r| matches!(r.state, PlayState::Ended(_)))
    });
    assert!(start.elapsed() < Duration::from_secs(1));
    let snapshot = handle.snapshot();
    let PlayState::Ended(end) = &snapshot.run.as_ref().unwrap().state else {
        panic!()
    };
    assert_eq!(end.primary.code, "play.stopped");
    assert!(end.cleanup.server.is_none());
    assert!(end
        .cleanup
        .diagnostics
        .iter()
        .any(|d| d.code == "fake.kill_denied"));
    assert!(end
        .cleanup
        .diagnostics
        .iter()
        .any(|d| d.code == "play.server_cleanup_failed"
            && d.parameters
                .get("reason")
                .is_some_and(|r| r == "reap_deadline")));
}

#[test]
fn unexpected_exit_before_readiness_is_a_start_failure() {
    let (_root, receipt, config) = fixture();
    // A clean exit without readiness or close intent is still a startup failure.
    let mut exited = server(false);
    exited.reports.clear();
    exited.stopped = true;
    let backend = Arc::new(FakeBackend {
        children: Mutex::new(VecDeque::from([exited])),
        ..Default::default()
    });
    let mut handle = PlayHandle::with_backend(config, backend.clone()).unwrap();
    handle
        .submit(PlayRequest {
            id: 1,
            control: PlayControl::Start {
                mode: PlayMode::ExternalGame,
                receipt,
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
    assert_eq!(end.cause.as_ref().unwrap().code, "play.unexpected_exit");
    assert!(end.cleanup.server.as_ref().unwrap().success());
    assert!(end.cleanup.client.is_none());
    assert_eq!(backend.launches.lock().unwrap().len(), 1);
}
