# Arc editor workspace

`editor/` on `arc/0.19` holds the engine-owned editor as its own cargo workspace.
It leaves upstream engine sources unchanged, is not part of the engine crates the
game patches, and is carried to the next `arc/<version>` branch on an upgrade.
The editor's dedicated CI workflow lives in `.github/workflows/editor-ci.yml`.

Development checkout: `arc-editor-work`, branch `editor/0.19`, based on engine
revision `bd0883094533ae6789af1f99de69da16a78f160b` (0.19.1). Root dependency
patches keep editor/toolkit packages on that one local engine. Local paths are
development provenance; an exact published repository revision must be recorded
before game distribution. No game checkout, upstream main, staging or publication
is changed by this workspace.

Core owns workspace configuration and scene-data/editor-core/editor-services/
editor-cook/landscape-render. UI owns editor-ui, apps and design. Engine/toolkit
builds share `editor/target`; engine-free core checks can use `editor/target/core`
to avoid waiting behind an engine build without compiling a second engine.

From this directory:

```sh
cargo test --offline -p scene-data -p editor-core -p editor-cook -p editor-services -p landscape-render --target-dir target/core
cargo fmt -p scene-data -p editor-core -p editor-cook -p editor-services -p landscape-render
cargo check --offline -p landscape-render --features bevy-render
cargo run --offline -p editor-services --bin arc-scene --target-dir target/core -- convert-current-map target/current-map.arcscene
cargo run --offline -p editor-services --bin arc-scene --target-dir target/core -- cook target/current-map.arcscene target/packages
```

Use explicit packages for formatting: `cargo fmt --all` also formats local path
dependencies and can rewrite pinned engine files outside this editor workspace.

## Desktop CI

[Editor CI](../.github/workflows/editor-ci.yml) runs on Linux, macOS and Windows
for pull requests and pushes that change this workspace, its local engine inputs,
shared Cargo/lint configuration, workflows or the Linux dependency action. A
change to the workflow itself triggers it. Existing engine CI and dependency
checks remain separate and unchanged.

Every Cargo step runs from `editor/` using this directory's `rust-toolchain.toml`
and committed lockfile. The matrix checks formatting with an explicit list of
all eight editor packages, then runs `cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` and
`cargo clippy --locked --workspace --all-targets -- -D warnings`.
Update the formatting package list whenever workspace membership changes; never
replace it with `cargo fmt --all`. The repository's pinned checkout/toolchain
actions and shared Linux dependency action are reused. The preserved `ui-spike`
fallback package is included, and the editor app enables the native renderer.

Workspace-wide test harnesses link the full Bevy graph. The default local
debug/incremental profile exhausted RAM and disk during verification on this
machine. For full-workspace runs on constrained machines, use the verified local
CI-profile settings with serial build/link jobs and an isolated target (from
`editor/`, in Bash):

```sh
export CARGO_BUILD_JOBS=1
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_TERM_COLOR=always
export CARGO_TARGET_DIR=target/ci
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
```

These jobs run Cargo test harnesses without launching the editor window. The
event-injected GUI suite in `apps/arc-editor/checks/run.sh` is separate desktop
evidence and is not run by this headless workflow. A CI version of that suite
requires an explicit virtual-display setup before it can be included. Cargo CI
does not verify live rendering, physical input or startup appearance. Local Linux
checks do not establish macOS/Windows results; those require native matrix runs.

`convert-current-map` refuses to overwrite an existing file. The fixture preserves
the game's fourteen boxes and eight spawn slots; tests compare float bits against
an independently frozen extract of `arc-game/crates/shared/src/level.rs`.

Scene schema 1 validates IDs, references, hierarchy, rigid transforms, terrain and
bounded payloads. Unknown fields/versions fail closed. There is no earlier editor
schema to migrate; future migrations must be explicit pure conversions and never
modify the original on failure. Explicit Save creates missing parent directories;
Open and path viewing do not. Parent creation failures carry save.parent_create
with path/detail and leave unsaved state intact. Saves write and sync a sibling temporary file,
check the expected disk hash under a cooperating-writer lock and atomically replace
the destination. External writers that ignore the lock can race the final hash
check; this is optimistic conflict detection, not a filesystem compare-and-swap.
After a process crash, a stale `.lock` requires explicit recovery rather than
silently deleting a potentially live writer's lock. Unix parent directory sync
is attempted; Windows directory crash durability has not been exercised.

`EditorHandle` lives on the UI thread. `snapshot()` clones a cached immutable Arc;
`submit()` uses a bounded nonblocking queue; `poll(max_events)` updates the cache
and reports command/job results. One worker serializes editing and source jobs.
Cancellation is a shared token, including while file work is running. Events use
bounded backpressure; a paused UI can pause the worker, never make the UI wait.
Opening a document creates a new session, so queued old commands cannot affect it.

Viewport previews are absolute edits against the transaction's starting document.
Only commit changes source and produces one undo entry. Cancellation removes the
preview and exposes the unchanged committed document. History and result caches
are bounded. Document, selection, inspector, hierarchy, jobs and status carry
independent revisions; cached regions preserve Arc identity when unaffected.

Terrain sculpt previews accept the complete ordered world-space hit path and
brush controls through `Command::PreviewTerrainSculpt`. Core enumerates canonical
global samples, computes quadratic soft-edge falloff and Raise/Lower/Smooth/Flatten
semantics, and returns sample deltas/dirty regions. Raise/Lower amounts apply once
per stroke; overlapping segments use maximum influence. Smooth reads the four
neighbors from the transaction base. Flatten targets are terrain-local meters;
`terrain_hit_height_m` converts an actual world-space hit through parent transforms.
See [the sculpt contract](crates/editor-core/docs/terrain-sculpt-contract.md).

The first cooked codec is versioned canonical JSON with SHA-256 full-file hashes.
Each immutable package directory contains `manifest.json`, `level.sim.json`, and
`level.visual.json`. Simulation includes boxes/spawns and exact CPU triangles;
visual layers are separate. The headless simulation loader never reads the visual
file. Cook bytes are deterministic under source storage reordering; authoring names
do not affect simulation identity. Game handshake/readiness integration is pending
the owner-controlled foundation commit boundary; this CLI does not launch Play.

Cook jobs now return an immutable `VerifiedPackageReceipt` with a canonical native
absolute path and exact manifest/source/simulation/visual identity. An async
`VerifyPackage` job rechecks bounds, provenance, directory identity and both
payloads against a retained identity before delayed use. See
[the receipt API and agreed future Play lifecycle](crates/editor-services/docs/play-boundary.md).
The lifecycle includes separate versioned status and redraw wakes; gameplay,
process launch and the Play coordinator are not implemented by this prerequisite.

L1 terrain uses a uniform lattice with globally authoritative border/corner samples,
stable tile identities and integer coordinates. Shared cooked positions, triangle
indices and accumulated normals drive both collision and the ground renderer.
`GroundRenderer` owns meshes/materials; editor owns cameras, picking and overlays.
Changed meshes are currently rebuilt as whole tiles. Reported prepared bytes are
CPU buffer bytes, not observed GPU traffic. No streaming/LOD or in-frame GPU update
claim is made. Runtime game collision installation and controller regression tests
are pending integration; peer payload equality alone does not prove correct walking.

Read/decode/validation/save/cook stages report measured durations from the first
slice. Compilation times and functional smoke timings are not idle-machine
performance acceptance. Product budgets and platform execution remain unmeasured.

L2 adds explicit surface-group selection, normalized blend weights, independent
masks, stable-ID registry edits and bilinear weight resampling. Shared renderer
vertex colors blend authored linear colors; roughness remains the shared0.9
constant. Source layer roles come from registered semantics and cannot be changed
by a UI boolean. Color/mask painting preserves simulation identity.

L3 cooker foundations implement `grass.scatter` version1 with explicit density
and exclusion bindings, stable seeded global cell/candidate IDs, acyclic derived
exclusion dependencies and bounded outputs. Placement interpolates the exact cooked
triangles; tile ownership is half-open. Rule output is in visual files only.
`DecorationView` provides loaded CPU counts/visibility state. The additive
`GrassRenderer` uses one shared unit mesh/material and stable world-space instance
transforms through pinned Bevy automatic instancing. Per-view visibility changes
no source or collision. It reports CPU instance/transform preparation and scene
counts separately from unobserved GPU submitted/visible counts (`None`). GrassCache
is optional bounded CPU reference geometry; the production renderer shares one
mesh rather than duplicating every blade. Versioned `layer.slope_height` rules now
derive bounded visual masks/blend weights on authoritative samples, with
dependency-ordered masks driving grass density/exclusion and the shared ground
renderer consuming derived blend colors. See [derived rule semantics](crates/editor-cook/docs/derived-landscape-rules.md).
Live rendering/frame cost and GPU observations remain pending.

`MeasurementRecord`/`MeasurementWriter` export bounded JSONL stage records with
engine/lock provenance, run/platform/profile and declared cache/workload conditions.
Unknown hardware/GPU/readiness facts remain absent. Geometry preview jobs include
revision/transaction binding; consumers discard results that no longer match.

The ground/geometry seam is frozen through initial terrain viewport integration: see
[the call contract](crates/landscape-render/docs/viewport-contract.md) and compiling
`landscape-render/examples/viewport_contract.rs`. Opened already exposes public
disk_hash/source_hash; expected DiskVersion uses disk_hash and successful Save
refreshes it from SaveReceipt. Geometry consumers check binding against the latest
cached snapshot immediately before applying, after draining their event batch.
