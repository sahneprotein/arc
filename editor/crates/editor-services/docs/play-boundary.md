# Verified packages and the Play lifecycle boundary

The receipt/preflight API and P1 external Play coordinator are implemented in
`editor-services`. Game readiness/control hooks live in arc-game; no game/editor
crate dependency is introduced. Embedded Play and multi-client modes remain later
work. Claude owns the editor UI integration.

## Implemented package receipt API

Import from `editor_services`:

```rust,ignore
use editor_services::{
    verify_package, PackageIdentity, VerifiedPackageReceipt,
    JobOperation, JobOutput, JobKind,
};

// Worker-side synchronous primitive; never call this on the UI thread.
let receipt = verify_package(&path, None, &cancel)?;
let native_path: &std::path::Path = receipt.path();
let identity: &PackageIdentity = receipt.identity();

// UI-side asynchronous preflight of the exact package retained from Cooked.
let ticket = handle.job(JobOperation::VerifyPackage {
    path: receipt.path().to_path_buf(),
    expected: Some(receipt.identity().clone()),
})?;
// Exactly one normal JobFinished per accepted ticket; success contains:
// JobOutput::PackageVerified { receipt, verification_us }.
```

`VerifiedPackageReceipt` has private fields, read-only `path()`/`identity()`
accessors, and `Clone + Debug + PartialEq + Eq`. Its path is a canonical absolute
native `PathBuf`, never converted to UTF-8. Symlink aliases resolve to the physical
published directory; the resolved directory's final component must be its exact
lowercase SHA-256 manifest digest. Non-UTF-8 parents remain valid on Unix.

`PackageIdentity` has public fields and `Clone + Debug + PartialEq + Eq`:

| Field | Type and meaning |
|---|---|
| `manifest_hash` | `ContentHash`, digest of the exact original manifest bytes |
| `source_hash` | `ContentHash`, authored source identity |
| `simulation_hash` | `ContentHash`, exact simulation payload bytes |
| `visual_hash` | `ContentHash`, exact visual payload bytes |
| `schema` | `u32`, current cooked schema |
| `profile` | `String`, collision profile |
| `engine_revision` | `String`, exact pinned engine revision |

All hashes provide `.hex()`. The package directory key hashes the manifest
bytes, not source or simulation bytes. Manifest formatting therefore affects
package identity. `expected: Some(identity)` compares every field, not just the
simulation hash; `None` discovers identity while still verifying everything.

Verification reads the manifest once (at most 8192 bytes), validates provenance,
checks its directory key and expected identity, then reads each payload at most
64 MiB plus one detection byte. Both payload hashes, decoding, schema/resource
limits, exact ground topology/seams and visual references/derived outputs use the
existing validators. Cancellation is checked during chunked reads and around
decoding/validation. A synchronous filesystem operation or bounded serde decode
is not interrupted mid-call; there is no promised wall-clock timeout.

`JobOutput::Cooked` adds `receipt` and preserves `path`, `source_hash`,
`simulation_hash`, `timings` and `publication_us`. The path/hash compatibility
fields equal the receipt values. Cook verifies against its own expected identity,
not merely whichever valid package a filesystem alias resolves to. Publication
timing now includes receipt verification. `JobKind::VerifyPackage` identifies
preflight independently; `PackageVerified.verification_us` measures it.

The job uses the existing bounded nonblocking request/event queues, cancellation
and session/document-revision fences. It does not mutate the document, saved
state or undo history. Like other non-Geometry jobs, it refuses an active viewport
transaction with `transaction.busy`; queued outdated requests fail `job.stale`.
An old cooked package may still be deliberately reverified after authoring edits
by submitting a fresh request at the current session/revision. It remains pinned
to the supplied package identity, independently of current document contents.

New diagnostics:

| Code | Parameters/meaning |
|---|---|
| `package.directory_identity` | `expected` manifest digest; `found` resolved directory key |
| `package.identity_mismatch` | first differing `field`, `expected`, `found` |
| `package.manifest_limit` | manifest exceeds 8192 bytes |

Identity differences use deterministic field order: manifest, source, simulation,
visual hashes, schema, profile, engine revision. Diagnostic display may lossily
render an invalid directory name; the receipt path itself remains native.
Existing `package.decode`, `package.provenance`, `payload.hash_mismatch`,
`payload.decode`, `payload.limit`, payload-validation codes, `io.failed` and
`job.cancelled` remain actionable failures. No directories are created by verify.

A receipt attests to bytes observed during a completed verification; it does
not lock the filesystem or authenticate the publisher. A later consumer must
reverify the expected identity and retain/use the verified bytes or open handles
through consumption. Reopening by path after verification can race an external
writer. This prerequisite returns identity only, not a runtime-ready payload
lease. No process-supervision loop belongs in the serial editing/job worker.

## Implemented P1 Play lifecycle

The agreed seam is exported from `editor_services`:

```rust,ignore
let mut play = PlayHandle::start(config)?;
play.set_waker(Box::new(move || wake_event_loop()));
play.submit(PlayRequest {
    id: request_id,
    control: PlayControl::Start {
        mode: PlayMode::ExternalGame,
        receipt,
        local_clients: 1, // caller resolves planned.multiplayer.clients.unwrap_or(1)
    },
})?;
for event in play.poll(32) { /* PlayEvent::Answer or PlayEvent::Ended */ }
let status: std::sync::Arc<PlayStatus> = play.snapshot();
play.submit(PlayRequest { id: stop_id, control: PlayControl::Stop { run } })?;
```

`submit` never waits. Caller IDs are echoed in `PlayAnswer.request`; its success
acknowledges admission, not readiness. Queue rejection (`play.queue_full` or
`play.coordinator_closed`) means the request was not accepted. Each accepted
control has one answer; each admitted run has a separate terminal event.
`local_clients` is explicit and must be in `1..=4`; the caller resolves an unset
project value to one. The server receives `--max-clients N`, and the coordinator
starts client IDs `1..=N`. `PlayRun.local_clients` and its process rows expose the
requested count and each process identity; Running waits for the first
controllable frame from every client. End cleanup reports every client exit by
ID, including a missing exit when a child could not be reaped by the deadline.
`PlayHandle::with_backend(config, Arc<dyn PlayBackend>)` supports scripted UI and
coordinator checks without game binaries. Injected backends still pass the common
receipt revalidation/capability check. `PlayChild::poll/stop/kill/try_wait` must be
nonblocking, and a returned exit means the child has been reaped.

`PlayStatus { revision, run: Option<PlayRun> }` retains the latest terminal run.
`PlayRun { id, mode, receipt, identity, state, processes }` pins native receipt and
identity; each `PlayProcess` exposes role, optional PID/native log path, measured
stages, listening endpoint and native exit facts. `PlayEnd { primary, cause,
cleanup }` preserves the underlying startup/runtime cause and independent
server/client cleanup exit facts/diagnostics. The handle is a UI-thread handle:
`snapshot()` uses a try-read of the cached Arc and returns its last observed
revision on contention. It never waits for the supervisor or a state lock.
`poll(&mut self, max)` drains at most that many events. Installing a waker wakes
once; each published revision wakes outside locks. Callback panics are contained.

Only `ExternalGame` launches in P1; `InEditor` returns `play.mode_unsupported`.
The typed Start requires a receipt; a caller without one reports
`play.package_required` before submission. No authoring lock is held for a run.

* Verification is an async job. After successful verification, `StartPlay` and
  `StopPlay` are control commands on a separate Play coordinator/handle, not
  authoring `Command`, undo mutations or long-lived jobs. Each accepted control
  request has one correlated result, including refusals. A terminal run event is
  distinct from command completion.
* `PlayMode` is an enum: `InEditor` (stable key `in_editor`) and `ExternalGame`
  (`external_game`). `StartPlay` names the mode and frozen receipt. Never start
  directly from preview geometry or a mutable source path.
* `play_snapshot()` returns cached `Arc<PlayStatus>` without blocking, separate
  from authoring `snapshot()` and job counts. It has its own monotonically
  increasing revision, unique `run_id`, mode, immutable package receipt/identity,
  and `Starting`, `Running`, `Stopping`, `Ended` state. Before any run it has no
  active run. Keep one terminal status per run ID; duplicate/late events cannot
  revive an ended run or change the new run's status.
* The coordinator exposes
  `set_waker(Box<dyn Fn() + Send + Sync + 'static>)`. After publishing **every**
  status revision, it invokes the registered callback, including start, ready,
  stop, external exit/crash and cleanup completion. Invoke outside state locks;
  the UI callback sends an event-loop wake/redraw message and does not touch UI
  state from the supervisor thread. The UI then polls results/status. Installing
  a waker also wakes once for an existing status so registration cannot miss a
  prior change. The callback wakes; it does not require continuous frames during
  an external run. Embedded gameplay requests frames while it renders.
* `Running` follows actual runtime readiness and verified collision/content
  identity, not successful process creation. `Ended` is published exactly once
  per run ID, after owned server cleanup finishes or its bounded deadline fails.
  Each stage wakes the UI even when the editor is otherwise asleep.
* End outcome codes are `play.stopped` (requested stop), `play.window_closed`
  (expected external client/window exit), `play.crashed` (unexpected runtime
  failure) and `play.server_cleanup_failed`. Startup failure additionally uses
  `play.start_failed`. Native exit facts retain exit code and Unix signal when
  available; an embedded outcome need not have a process exit status. Keep
  `log_path: Option<PathBuf>` native; expose display parameters separately for
  notices. A crash includes its log location when one exists.
* Preserve the primary stop/close/crash/start-failure outcome and server cleanup
  as separate facts. A cleanup failure exposes `play.server_cleanup_failed`
  without erasing the primary crash or close, with owned-child exit facts and
  cleanup detail. Do not infer a user close from exit code alone: classify using
  the runtime's expected-close event/intent and exit status. Crashes and readiness
  failures always remain visible.
* Refusals are `play.already_active`, `play.package_required`, and
  `play.no_active_run`. Expected-identity/provenance verification failures retain
  their package diagnostics. A stale run ID must not stop a replacement run
  (`play.stale_run`). Starting/Stopping count as active for refusing a second
  start. Controls/results and diagnostic queues remain bounded.
* Closing the editor requests `StopPlay` and waits for terminal cleanup through
  the normal unsaved-change dialog flow. Dropping a handle is not the normal
  shutdown protocol and cannot substitute for the terminal acknowledgement.
  An eventual supervisor should have bounded emergency child cleanup on host
  teardown, but that is not a promise that dropping a handle is graceful.
* Authoring edits continue while playing. They never mutate runtime state or its
  receipt. Compare current source hash against the pinned source hash to show
  "playing cooked <hash>, older than the level". Start preparation may briefly
  need an editing transaction committed/cancelled; the running lifetime does not
  hold that transaction or the serial worker.

## External launch and runtime protocol

`PlayConfig` requires explicit native server/client executable, working-directory,
asset-root and log-directory paths. `from_game_worktree(root, logs)` selects
`root/target/debug/{server,client}` and `root/assets`. No PATH discovery or implicit
build occurs. Both children receive the same `--cooked-package` receipt path and
`--cooked-package-hash` manifest digest. `BEVY_ASSET_ROOT` is the directory
containing `assets/`. Local launch selects an available UDP port and passes
`--bind 127.0.0.1:<port>`; the client uses the actual authenticated listening
endpoint. Reservation is released before child bind; a port race fails startup.
Metrics also bind loopback port 0. The development key is explicitly local and
inherited `ARC_PRIVATE_KEY` is removed for this pair.

Revalidation compares every pinned identity field and consumes decoded payloads
from those same reads. Cooked ground and authored terrain paint (blend weights and
masks) are inside the runtime subset. Procedural terrain is not: a package whose
visual terrain carries rules, decoration or derived layers fails with
`play.terrain_unsupported { reason, count }` naming the first such kind. No spawns
(`play.spawn_required`) and runtime limits (`play.runtime_limit`) fail likewise, all
before spawning. Children independently verify the expected manifest/payload
hashes at consumption, then their existing gameplay handshake compares package
identity and game fingerprint. The receipt remains an attestation, not a lock.

Each child gets an independent loopback TCP listener and OS-random 256-bit token
(current process adapter supports Unix). `--play-control`, `--play-run-id` and
`--play-token` require each other and the cooked path/hash. Without these flags,
standalone test-level/cooked startup remains supported.

Frames are newline JSON, at most 4096 bytes, with bounded reads/writes each tick.
Unknown fields, bad version/run/token/role/hash, sequence gaps/replays and unequal
game fingerprints fail closed. Report fields are `version: 1`, `run`, `token`,
`role`, `package_hash`, `protocol_version`, `sequence` and tagged `event`:
`stage { stage }`, `listening { address }`, or `expected_window_close`. Stop is
`{ version: 1, run, token, command: "stop" }`. Tokens are not logged. DTOs are
mirrored in game `protocol::play::wire` and editor `play::wire`, with matching exact
JSON tests; neither repository depends on the other.

Server stages are `package_verified`, then `collision_ready` (all authored level
colliders observed in the actual broadphase), then a nonzero loopback listening
endpoint. Client stages are `package_verified`, `collision_ready`,
`handshake_accepted`, `simulating` (owning synced prediction has executed a tick),
`level_drawn`, then `first_controllable_frame`. The last two require the owning
Pip rig/dependencies, required GPU mesh/image assets, ready pipelines and render
world confirmation after render submission/presentation. Startup does not infer
readiness from process creation, logs, clock alignment alone or a bare window.

Client primary-window close sends and bounded-flushes explicit close intent before
engine exit. Only that intent plus a clean client exit yields `play.window_closed`;
zero exit without intent is unexpected. Stop uses the same server graceful
shutdown/watchdog as OS signals and client `AppExit::Success`. Parent channel loss
or malformed commands stop the runtime. Default deadlines are startup 30s,
graceful stop 3s, then kill/reap 2s. Forced/unreaped/unclean cleanup is reported
separately, including `play.server_cleanup_failed`. Terminal publication waits
for reaping or the deadline. Dropping the handle triggers emergency cleanup; it
is not the normal editor shutdown acknowledgement. Logs remain for diagnostics.
A cancelled verifier cannot later launch a child. One verifier thread and one
queued preparation bound preflight work (`play.preflight_busy` fails a start if
that slot is occupied); synchronous filesystem calls
remain subject to the verification limitations documented above.

## Smoke path and validation boundary

From `editor/`, after building the game worktree's production server/client:

```sh
cargo run -p editor-services --example play_smoke -- /private/tmp/arc-game-play-readiness
```

The example cooks the current sample map, verifies its receipt, starts one pair,
waits for measured Running, requests Stop, and requires both clean/reaped exits.
It records native logs under `editor/target/play-smoke`. This requires a working
window/render environment and the required game assets. Scripted real-child
adapter tests exercise cook-to-stop, close/crash and protocol failure without a
renderer. The results below do not represent full CI or human visual inspection.

### Validation recorded 2026-10-11

All commands below ran in the isolated `codex/play-backend` and
`codex/play-readiness` worktrees. Editor UI changes and UI validation belong to
Claude; the services handle API was kept compatible.

| Check | Actual outcome |
| --- | --- |
| Editor `cargo nextest run -p editor-services --offline --target-dir target/play-services` | **45/45 passed**, no skipped tests. Includes fake-child deadlines/failed kill/unreaped cleanup, startup exit classification, receipt mutation, correlated controls and real subprocess/channel cook-to-stop, close/crash/fingerprint rejection. |
| Game filtered nextest command below | **78/78 passed**, 178 skipped. Includes all **54 protocol library tests**, client/server startup config checks, cooked scene construction, measured collision/listening, authenticated Stop/EOF, close intent, owning simulation/render acknowledgement gate and the actual Pip rig/dependency readiness check. |
| Editor `cargo check -p editor-services --all-targets --offline --target-dir target/play-check` | Passed, including the smoke example. |
| Game `cargo build -p server -p client --offline` | Passed with production features (test clock disabled). |
| Game `cargo check -p protocol --no-default-features --offline` | Passed without warnings; collision helpers are gated under client/server. |
| Game `cargo fmt --all -- --check`; editor `cargo fmt -p editor-services -- --check`; scoped `git diff --check` in both worktrees | Passed. |
| Editor `cargo clippy -p editor-services --all-targets --offline --target-dir target/play-check -- -D warnings` | **Fails on the existing `src/actor.rs:48` `clippy::large_enum_variant`**. New Play findings were fixed; the agreed concrete receipt/terminal enum shapes have local, explained allowances. The older authoring API was not changed. |
| Actual production cook-to-stop smoke (`play_smoke` example, real server and client) | **Passed on a later run.** The run reached measured `Running`; the client reported `LevelDrawn` and `FirstControllableFrame`; Stop was graceful and both children exited 0 and were reaped. This is the smoke example driving the coordinator, not a person clicking through the editor UI. An earlier attempt did not pass: package, actual broadphase, authenticated loopback server endpoint, accepted handshake and owning simulation were observed, but no prepared 3D `ViewTarget`/acquired swapchain view appeared, the two render stages were withheld and the run ended `play.start_failed` with `play.readiness_timeout`. |

Final game check:

```sh
cargo nextest run -p protocol -p server -p client --offline \
  -E 'test(play::) or test(play_runtime::) or test(play_tests::) or test(config::tests::) or test(scene::tests::verified_cooked) or test(pip::tests::readiness::) or (package(protocol) and kind(lib))'
```

The passing smoke stopped and reaped both children with exit 0 and no cleanup
diagnostics. The failed attempt before it reaped the server with exit 0; its
client exceeded the 3-second grace deadline and was killed/reaped with signal 9,
retaining `play.client_cleanup_failed { reason: grace_deadline }` independently
of the startup failure. Temporary gate diagnostics were removed. Native logs
remain under `editor/target/play-smoke`; the coordinator trace in
`editor/target/play-smoke-result.log` of this worktree is the failed attempt's.

The editor UI's lifecycle and unsaved-close checks ran against a scripted
stand-in for the game: the full scripted suite passed 210/210.

Remaining validation: starting and stopping the real game from the editor's own
window, rather than through the smoke example; actual game primary-window close;
full CI and resolution of the existing authoring clippy warning. The process
adapter is Unix-only; other platforms are not validated. The implementation is
committed locally and not pushed.
