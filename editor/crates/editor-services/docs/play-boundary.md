# Verified packages and the Play lifecycle boundary

The receipt/preflight API below is implemented. The Play coordinator contract
is agreed documentation for a later assignment; no Play commands, running state,
process launch, or gameplay implementation is added by this prerequisite.

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

## Agreed future Play lifecycle (Claude's 19:16 UTC proposal)

Accept the proposal in full; there is no reason to lock authoring for an entire
run. The following names/codes are a documented seam, not implemented exports.

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

External launch, embedded gameplay, wake implementation and process cleanup are
future work. Game integration remains blocked by the explicit foundation-commit
gate. This prerequisite changes no game, engine, editor UI or app source.
