//! Cook -> verify -> external server/client readiness -> Stop -> reaped terminal.
//! Run from editor/: cargo run -p editor-services --example play_smoke -- <game-worktree>
use editor_services::*;
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args_os()
        .nth(1)
        .ok_or("usage: play_smoke <game-worktree>")?;
    let root = std::fs::canonicalize(root)?;
    let logs = std::env::current_dir()?.join("target/play-smoke");
    std::fs::create_dir_all(&logs)?;
    let packages = tempfile::tempdir_in(&logs)?;
    let cooked =
        editor_cook::cook(&scene_data::current_map()).map_err(|e| format!("cook: {e:?}"))?;
    let cancel = AtomicBool::new(false);
    let path = publish_package(packages.path(), &cooked, &cancel)
        .map_err(|e| format!("publish: {e:?}"))?;
    let receipt = verify_package(&path, None, &cancel).map_err(|e| format!("verify: {e:?}"))?;
    println!("package {}", receipt.identity().manifest_hash.hex());
    let mut config = PlayConfig::from_game_worktree(root, logs);
    config.startup_timeout = Duration::from_secs(90);
    let deadline = Instant::now()
        + config.startup_timeout
        + config.stop_timeout
        + config.reap_timeout
        + Duration::from_secs(2);
    let mut handle = PlayHandle::start(config).map_err(|e| format!("handle: {e:?}"))?;
    handle
        .submit(PlayRequest {
            id: 1,
            control: PlayControl::Start {
                mode: PlayMode::ExternalGame,
                receipt,
            },
        })
        .map_err(|e| format!("start: {e:?}"))?;
    let mut stopped = false;
    let mut revision = 0;
    loop {
        for event in handle.poll(32) {
            if let PlayEvent::Answer(answer) = event {
                answer.result.map_err(|e| format!("control: {e:?}"))?;
            }
        }
        let snapshot = handle.snapshot();
        if let Some(run) = &snapshot.run {
            if snapshot.revision != revision {
                println!("run {} {:?} {:?}", run.id.0, run.state, run.processes);
                revision = snapshot.revision;
            }
            match &run.state {
                PlayState::Running if !stopped => {
                    stopped = true;
                    handle
                        .submit(PlayRequest {
                            id: 2,
                            control: PlayControl::Stop { run: run.id },
                        })
                        .map_err(|e| format!("stop: {e:?}"))?;
                }
                PlayState::Ended(end) => {
                    if end.primary.code != "play.stopped"
                        || !end.cleanup.diagnostics.is_empty()
                        || !end.cleanup.server.as_ref().is_some_and(PlayExit::success)
                        || !end.cleanup.client.as_ref().is_some_and(PlayExit::success)
                    {
                        return Err(format!("Play smoke failed: {end:?}").into());
                    }
                    println!("PASS: cooked identity, measured Running, graceful Stop, both children reaped");
                    return Ok(());
                }
                _ => (),
            }
        }
        if Instant::now() >= deadline {
            return Err(format!("Play smoke deadline: {snapshot:?}").into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
