//! Real process/channel checks using this test executable as the scripted runtime.
//! No Python, game binaries, renderer, or UI dependency.
#![cfg(unix)]
use super::wire::{Command, Event, Report, Role, Stage, VERSION};
use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpStream, UdpSocket},
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
fn fixture(mode: &str) -> (tempfile::TempDir, crate::VerifiedPackageReceipt, PlayConfig) {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    let cooked = editor_cook::cook(&scene_data::current_map()).unwrap();
    let path = crate::publish_package(root.path(), &cooked, &AtomicBool::new(false)).unwrap();
    let receipt = crate::verify_package(&path, None, &AtomicBool::new(false)).unwrap();
    let binary = std::env::current_exe().unwrap();
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    for role in ["server", "client"] {
        let script = format!(
            r#"#!/bin/sh
export PLAY_FIXTURE_MODE={mode}
export PLAY_FIXTURE_ROLE={role}
while [ "$#" -gt 0 ]; do
  case "$1" in
    --play-control) export PLAY_FIXTURE_CONTROL="$2" ;;
    --play-run-id) export PLAY_FIXTURE_RUN="$2" ;;
    --play-token) export PLAY_FIXTURE_TOKEN="$2" ;;
    --cooked-package) export PLAY_FIXTURE_PACKAGE="$2" ;;
    --cooked-package-hash) export PLAY_FIXTURE_HASH="$2" ;;
    --bind) export PLAY_FIXTURE_BIND="$2" ;;
    --allow-dev-key) shift; continue ;;
  esac
  shift 2
done
exec {} --exact play::process_tests::runtime_child_fixture --nocapture
"#,
            quote(binary.to_str().unwrap())
        );
        let path = root.path().join(role);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut config = PlayConfig::from_game_worktree(root.path().into(), root.path().join("logs"));
    config.server = root.path().join("server");
    config.client = root.path().join("client");
    // Allow cold executable inspection on macOS; deadline escalation is tested
    // separately with fake children and millisecond deadlines.
    config.startup_timeout = Duration::from_secs(10);
    config.stop_timeout = Duration::from_secs(1);
    config.reap_timeout = Duration::from_secs(1);
    (root, receipt, config)
}
fn wait(handle: &mut PlayHandle, predicate: impl Fn(&PlayStatus) -> bool) -> Vec<PlayEvent> {
    let mut events = vec![];
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        events.extend(handle.poll(100));
        if predicate(&handle.snapshot()) {
            events.extend(handle.poll(100));
            return events;
        }
        assert!(Instant::now() < deadline, "{:?}", handle.snapshot());
        std::thread::sleep(Duration::from_millis(3));
    }
}
fn ended(status: &PlayStatus) -> bool {
    status
        .run
        .as_ref()
        .is_some_and(|r| matches!(r.state, PlayState::Ended(_)))
}
#[test]
fn real_adapter_cook_to_running_to_stop_reaps_both_children() {
    let (_root, receipt, config) = fixture("stop");
    let mut handle = PlayHandle::start(config).unwrap();
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
    let status = handle.snapshot();
    let run = status.run.as_ref().unwrap();
    assert!(run
        .processes
        .iter()
        .all(|p| p.pid.is_some() && p.log_path.as_ref().is_some_and(|p| p.is_file())));
    assert!(run.processes[0].listening.unwrap().ip().is_loopback());
    handle
        .submit(PlayRequest {
            id: 2,
            control: PlayControl::Stop { run: run.id },
        })
        .unwrap();
    let events = wait(&mut handle, ended);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, PlayEvent::Ended { .. }))
            .count(),
        1
    );
    let status = handle.snapshot();
    let PlayState::Ended(end) = &status.run.as_ref().unwrap().state else {
        panic!()
    };
    assert_eq!(end.primary.code, "play.stopped");
    assert!(end.cleanup.diagnostics.is_empty(), "{end:?}");
    assert!(end.cleanup.server.as_ref().unwrap().success());
    assert!(end.cleanup.client.as_ref().unwrap().success());
}
#[test]
fn real_adapter_distinguishes_expected_close_crash_and_protocol_failure() {
    for (mode, expected) in [
        ("close", "play.window_closed"),
        ("crash", "play.crashed"),
        ("fingerprint", "play.start_failed"),
    ] {
        let (_root, receipt, config) = fixture(mode);
        let mut handle = PlayHandle::start(config).unwrap();
        handle
            .submit(PlayRequest {
                id: 1,
                control: PlayControl::Start {
                    mode: PlayMode::ExternalGame,
                    receipt,
                },
            })
            .unwrap();
        wait(&mut handle, ended);
        let status = handle.snapshot();
        let PlayState::Ended(end) = &status.run.as_ref().unwrap().state else {
            panic!()
        };
        assert_eq!(end.primary.code, expected, "{mode}: {end:?}");
        assert!(end.cleanup.server.as_ref().unwrap().success());
        if mode == "fingerprint" {
            assert_eq!(end.cause.as_ref().unwrap().code, "play.protocol_mismatch");
        }
        if mode == "crash" {
            assert_eq!(end.cleanup.client.as_ref().unwrap().code, Some(17));
        }
    }
}
/// Invoked in a child test executable by fixture scripts; ordinary test run is a no-op.
#[test]
fn runtime_child_fixture() {
    let Ok(role) = std::env::var("PLAY_FIXTURE_ROLE") else {
        return;
    };
    let env = |name| std::env::var(name).unwrap();
    let role = if role == "server" {
        Role::Server
    } else {
        Role::Client
    };
    let mode = env("PLAY_FIXTURE_MODE");
    let path = env("PLAY_FIXTURE_PACKAGE");
    let receipt = crate::verify_package(Path::new(&path), None, &AtomicBool::new(false)).unwrap();
    let hash = env("PLAY_FIXTURE_HASH");
    assert_eq!(receipt.identity().manifest_hash.hex(), hash);
    let run = env("PLAY_FIXTURE_RUN").parse().unwrap();
    let token = env("PLAY_FIXTURE_TOKEN");
    let mut stream = TcpStream::connect(env("PLAY_FIXTURE_CONTROL")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let mut sequence = 0;
    let mut report = |event: Event| {
        sequence += 1;
        let version = if mode == "fingerprint" && role == Role::Client {
            "arc-other+ffffffff"
        } else {
            "arc-test+12345678"
        };
        let report = Report {
            version: VERSION,
            run,
            token: token.clone(),
            role,
            package_hash: hash.clone(),
            protocol_version: version.into(),
            sequence,
            event,
        };
        let mut bytes = serde_json::to_vec(&report).unwrap();
        bytes.push(b'\n');
        stream.write_all(&bytes).unwrap();
    };
    report(Event::Stage {
        stage: Stage::PackageVerified,
    });
    let _socket;
    if role == Role::Server {
        report(Event::Stage {
            stage: Stage::CollisionReady,
        });
        _socket = Some(UdpSocket::bind(env("PLAY_FIXTURE_BIND")).unwrap());
        report(Event::Listening {
            address: _socket.as_ref().unwrap().local_addr().unwrap(),
        });
    } else {
        _socket = None;
        for stage in [
            Stage::CollisionReady,
            Stage::HandshakeAccepted,
            Stage::Simulating,
            Stage::LevelDrawn,
            Stage::FirstControllableFrame,
        ] {
            report(Event::Stage { stage });
        }
        if mode == "close" {
            report(Event::ExpectedWindowClose);
            std::process::exit(0);
        }
        if mode == "crash" {
            std::process::exit(17);
        }
    }
    let mut command = String::new();
    BufReader::new(stream).read_line(&mut command).unwrap();
    let command: Command = serde_json::from_str(&command).unwrap();
    assert!(command.is_stop_for(run, &token));
}
