use super::{handle::Shared, model::Model, *};
use scene_data::Diagnostic;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError},
        Arc,
    },
    time::{Duration, Instant},
};
struct Preparation {
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<(), Diagnostic>>,
}
struct Owned {
    role: PlayRole,
    child: Box<dyn PlayChild>,
    exit: Option<PlayExit>,
    close: bool,
    stop_sent: bool,
    killed: bool,
}
pub(crate) fn run(
    config: PlayConfig,
    backend: Arc<dyn PlayBackend>,
    requests: Receiver<PlayRequest>,
    events: SyncSender<PlayEvent>,
    shared: Arc<Shared>,
) {
    let mut model = Model::new();
    let mut children: Vec<Owned> = vec![];
    let mut preparation: Option<Preparation> = None;
    let verifier = super::preflight::Worker::new();
    let mut startup = Instant::now();
    let mut stopping: Option<Instant> = None;
    let mut diagnostics = vec![];
    let mut disconnected = false;
    loop {
        // Requests and results are bounded. Successful submission always reserves an answer.
        for _ in 0..32 {
            match requests.try_recv() {
                Ok(request) => {
                    let (run, result) = match request.control {
                        PlayControl::Start { mode, receipt } => match model.start(mode, receipt) {
                            Ok(id) => {
                                startup = Instant::now();
                                stopping = None;
                                diagnostics.clear();
                                children.clear();
                                shared.publish(&mut model.status);
                                let cancel = Arc::new(AtomicBool::new(false));
                                let receipt = model.status.run.as_ref().unwrap().receipt.clone();
                                let queued =
                                    verifier.as_ref().map_err(Clone::clone).and_then(|worker| {
                                        worker.submit(
                                            receipt,
                                            config.asset_root.clone(),
                                            cancel.clone(),
                                        )
                                    });
                                match queued {
                                    Ok(result) => {
                                        preparation = Some(Preparation { cancel, result })
                                    }
                                    Err(error) => model.fail("play.start_failed", Some(error)),
                                }
                                (Some(id), Ok(()))
                            }
                            Err(error) => (None, Err(error)),
                        },
                        PlayControl::Stop { run } => (Some(run), model.stop(run)),
                    };
                    if events
                        .try_send(PlayEvent::Answer(PlayAnswer {
                            request: request.id,
                            run,
                            result,
                        }))
                        .is_err()
                    {
                        disconnected = true;
                    }
                    shared.publish(&mut model.status);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if disconnected && model.active() {
            model.stop(model.status.run.as_ref().unwrap().id).ok();
        }
        if model.active() {
            if matches!(
                model.status.run.as_ref().unwrap().state,
                PlayState::Stopping
            ) && let Some(preparation) = preparation.take()
            {
                preparation.cancel.store(true, Ordering::Release);
            }
            let prepared = preparation
                .as_ref()
                .and_then(|p| match p.result.try_recv() {
                    Ok(result) => Some(result),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => {
                        Some(Err(Diagnostic::new("play.preflight_closed")))
                    }
                });
            if let Some(result) = prepared {
                preparation = None;
                match result {
                    Err(error) => model.fail("play.start_failed", Some(error)),
                    Ok(()) => {
                        let run = model.status.run.as_ref().unwrap();
                        let launch = PlayLaunch {
                            run: run.id,
                            role: PlayRole::Server,
                            receipt: run.receipt.clone(),
                            server_address: None,
                            config: config.clone(),
                        };
                        match backend.launch(launch) {
                            Ok(child) => attach(&mut model, &mut children, PlayRole::Server, child),
                            Err(error) => model.fail("play.start_failed", Some(error)),
                        }
                    }
                }
            }
            for owned in &mut children {
                if owned.exit.is_some() {
                    continue;
                }
                match owned.child.poll() {
                    Ok(reports) => {
                        for report in reports {
                            let result = match report {
                                PlayRuntimeEvent::Stage(stage) => {
                                    model.stage(owned.role, stage).map(|changed| {
                                        if changed {
                                            shared.publish(&mut model.status);
                                        }
                                    })
                                }
                                PlayRuntimeEvent::Listening(address) => {
                                    if owned.role != PlayRole::Server
                                        || !address.ip().is_loopback()
                                        || address.port() == 0
                                        || !model
                                            .process(owned.role)
                                            .stages
                                            .contains(&PlayStage::CollisionReady)
                                    {
                                        Err(Diagnostic::new("play.invalid_listening"))
                                    } else if !matches!(
                                        model.status.run.as_ref().unwrap().state,
                                        PlayState::Stopping
                                    ) {
                                        let previous = model.process(owned.role).listening;
                                        if previous.is_some_and(|old| old != address) {
                                            Err(Diagnostic::new("play.listening_changed"))
                                        } else {
                                            model.process(owned.role).listening = Some(address);
                                            if previous.is_none() {
                                                shared.publish(&mut model.status);
                                            }
                                            Ok(())
                                        }
                                    } else {
                                        Ok(())
                                    }
                                }
                                PlayRuntimeEvent::ExpectedWindowClose => {
                                    if owned.role == PlayRole::Client {
                                        owned.close = true;
                                        Ok(())
                                    } else {
                                        Err(Diagnostic::new("play.invalid_close"))
                                    }
                                }
                            };
                            if let Err(error) = result {
                                let code = if matches!(
                                    model.status.run.as_ref().unwrap().state,
                                    PlayState::Running
                                ) {
                                    "play.crashed"
                                } else {
                                    "play.start_failed"
                                };
                                model.fail(code, Some(error));
                            }
                        }
                    }
                    Err(error) => model.fail(
                        if matches!(model.status.run.as_ref().unwrap().state, PlayState::Running) {
                            "play.crashed"
                        } else {
                            "play.start_failed"
                        },
                        Some(error),
                    ),
                }
                match owned.child.try_wait() {
                    Ok(Some(exit)) => {
                        if !owned.stop_sent {
                            if owned.close && exit.success() {
                                model.fail("play.window_closed", None);
                            } else {
                                model.fail(
                                    if matches!(
                                        model.status.run.as_ref().unwrap().state,
                                        PlayState::Starting
                                    ) {
                                        "play.start_failed"
                                    } else {
                                        "play.crashed"
                                    },
                                    Some(
                                        Diagnostic::new("play.unexpected_exit")
                                            .with("role", format!("{:?}", owned.role)),
                                    ),
                                );
                            }
                        } else if !exit.success() && !owned.killed {
                            diagnostics.push(
                                Diagnostic::new(if owned.role == PlayRole::Server {
                                    "play.server_cleanup_failed"
                                } else {
                                    "play.client_cleanup_failed"
                                })
                                .with("reason", "unclean_exit"),
                            );

                            model.fail(
                                "play.crashed",
                                Some(Diagnostic::new("play.exit_during_stop")),
                            );
                        }
                        model.process(owned.role).exit = Some(exit.clone());
                        owned.exit = Some(exit);
                    }
                    Ok(None) => (),
                    Err(error) => {
                        record(&mut diagnostics, error);
                        model.fail("play.crashed", Some(Diagnostic::new("play.wait_failed")));
                    }
                }
            }
            let stopping_now = matches!(
                model.status.run.as_ref().unwrap().state,
                PlayState::Stopping
            );
            if !stopping_now
                && !children.iter().any(|p| p.role == PlayRole::Client)
                && let Some(address) = model.process(PlayRole::Server).listening
            {
                let launch = PlayLaunch {
                    run: model.status.run.as_ref().unwrap().id,
                    role: PlayRole::Client,
                    receipt: model.status.run.as_ref().unwrap().receipt.clone(),
                    server_address: Some(address),
                    config: config.clone(),
                };
                match backend.launch(launch) {
                    Ok(child) => attach(&mut model, &mut children, PlayRole::Client, child),
                    Err(error) => model.fail("play.start_failed", Some(error)),
                }
            }
            if matches!(
                model.status.run.as_ref().unwrap().state,
                PlayState::Starting
            ) && startup.elapsed() >= config.startup_timeout
            {
                model.fail(
                    "play.start_failed",
                    Some(Diagnostic::new("play.readiness_timeout")),
                );
            }
            if matches!(
                model.status.run.as_ref().unwrap().state,
                PlayState::Stopping
            ) {
                let since = *stopping.get_or_insert_with(Instant::now);
                for owned in &mut children {
                    if owned.exit.is_some() {
                        continue;
                    }
                    if !owned.stop_sent {
                        owned.stop_sent = true;
                        if let Err(error) = owned.child.stop() {
                            record(&mut diagnostics, error);
                        }
                    }
                    if since.elapsed() >= config.stop_timeout && !owned.killed {
                        owned.killed = true;
                        diagnostics.push(
                            Diagnostic::new(if owned.role == PlayRole::Server {
                                "play.server_cleanup_failed"
                            } else {
                                "play.client_cleanup_failed"
                            })
                            .with("reason", "grace_deadline"),
                        );
                        if let Err(error) = owned.child.kill() {
                            record(&mut diagnostics, error);
                        }
                    }
                }
                let done = children.iter().all(|p| p.exit.is_some());
                if done
                    || since.elapsed() >= config.stop_timeout.saturating_add(config.reap_timeout)
                {
                    for owned in children.iter().filter(|child| child.exit.is_none()) {
                        diagnostics.push(
                            Diagnostic::new(if owned.role == PlayRole::Server {
                                "play.server_cleanup_failed"
                            } else {
                                "play.client_cleanup_failed"
                            })
                            .with("reason", "reap_deadline"),
                        );
                    }
                    let cleanup = PlayCleanup {
                        server: children
                            .iter()
                            .find(|p| p.role == PlayRole::Server)
                            .and_then(|p| p.exit.clone()),
                        client: children
                            .iter()
                            .find(|p| p.role == PlayRole::Client)
                            .and_then(|p| p.exit.clone()),
                        diagnostics: std::mem::take(&mut diagnostics),
                    };
                    if let Some((run, end)) = model.finish(cleanup) {
                        let _ = events.try_send(PlayEvent::Ended { run, end });
                        shared.publish(&mut model.status);
                    }
                    children.clear();
                    if let Some(preparation) = preparation.take() {
                        preparation.cancel.store(true, Ordering::Release);
                    }
                }
            }
            // Publish only when state/process facts changed, not on every tick.
            let published = shared.status.read().unwrap().clone();
            if *published != model.status {
                shared.publish(&mut model.status);
            }
        }
        if disconnected && !model.active() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn attach(model: &mut Model, children: &mut Vec<Owned>, role: PlayRole, child: Box<dyn PlayChild>) {
    let process = model.process(role);
    process.pid = Some(child.pid());
    process.log_path = child.log_path();
    children.push(Owned {
        role,
        child,
        exit: None,
        close: false,
        stop_sent: false,
        killed: false,
    });
}

fn record(diagnostics: &mut Vec<Diagnostic>, diagnostic: Diagnostic) {
    if diagnostics.len() < 32 && !diagnostics.contains(&diagnostic) {
        diagnostics.push(diagnostic);
    }
}
