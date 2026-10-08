# Viewport Geometry

`GeometryCache::cook(Arc<SceneDocument>, check)` produces validated CPU simulation
and visual payloads, with no package serialization or source/payload hashing.
`cook` / `cook_with_cancel` remain the publication path and enforce encoded size
and content identities. A viewport result is not evidence that publication passes.

The worker owns one cache containing only its latest successful source, world
transforms and CPU payload pair. Cancellation/failure leaves the previous cache
intact; Open clears it. Session/revision gates still precede work. The output's
existing GeometryBinding carries actual session, revision, transaction and
sequence, and consumers must accept it only when `binding.matches(snapshot)`.

For identical geometry layout and exact composed transform bits, compare sample
keys/heights against the last successful geometry source (not the last dirty
rectangle). Up to65,536 changed samples use an incremental path. A conservative
one-sample halo contains every vertex whose triangle normal may change. Every
tile containing those samples is replaced. Adjacent context tiles contribute
normals in the original global TileKey order; their output must not replace
unchanged cached tiles, because context-edge normals can omit outside triangles.
Negative coordinates use Euclidean division. Corner samples retain one global
authority. Bulk changes, changed geometry layout or world transforms fall back to
complete geometry for the affected terrain. Material/paint/rule metadata changes
retain exact collision geometry and refresh all visual derived outputs for that
terrain; partial derived-layer reuse requires unchanged visual metadata. Unrelated terrain reuses
its previous cooked geometry and all rules. Original global ordering and float
bits are checked against publication bytes in the parity tests.

Slope/height layers and their dependency DAG are pointwise at canonical SampleKey.
Only halo sample values are evaluated again; other weights are copied from the
previous matching rule/tile output. TileKey-first ownership of authored masks at
borders is preserved. Output arrays/maps still require full traversal. Grass
scatter is evaluated across the edited terrain: rule dependencies and exclusions
can propagate beyond dirty tiles, especially under rotated transforms. No local
grass shortcut or debounce latency guarantee is claimed.

After a new Geometry request successfully enters the64-request FIFO, its token
supersedes the previous Geometry token. The single worker skips cancelled queued
jobs and checks active work at geometry/rule rows and validation boundaries.
Every accepted ticket still emits exactly one JobFinished, normally job.cancelled
for supersession. A rejected queue.full does not cancel the last accepted request.
Commands remain FIFO and fully evaluated; previews that are invalid/stale cannot
replace the last valid source. Save/Open/Cook are not superseded. Snapshot/events
remain bounded to128 entries and job history to32. There are no per-sample
threads, additional background jobs or an accumulating cache by revision.

Incremental scratch holds at most589,824 halo keys before deduplication, and at
most the schema-bounded4096 tiles /1M authoritative samples /2M tile vertices.
Existing source, visual-layer, rule-candidate and decoration schema budgets also
apply. Plain Vec payloads retain copying costs; full scene/payload validation and
renderer validation/comparison still traverse the level. Bounded event queues can
retain multiple source/payload Arcs if a consumer stops polling; these existing
limits bound counts, not a small process-memory promise. New preview commands
alone do not interrupt a job; supersession starts when a newer Geometry ticket
successfully queues. The measured latest-only burst leaves command evaluation as
the dominant remaining cost. Live UI input-to-frame/GPU timing remains separate.

CookTimings.encoding_us is zero for viewport Geometry. Exported Geometry stages
are geometry_validate, geometry_prepare, geometry_rules, geometry_total and
job_total; they do not claim hash/publication work. GeometryWork is additive cooker
instrumentation for affected/rebuilt/context/reused tiles and evaluated samples;
JobOutput::Geometry and GeometryBinding fields remain unchanged.

Controlled baseline/comparison harness:
`cargo build --offline -p landscape-render --example preview_profile`, then run
the binary directly with `<output.jsonl> <label>` (serial) or an additional
`burst`; additional `work` writes functional scope/parity evidence. Run timings
only in a separately coordinated quiet interval after all builds end. Raw rows,
binary/source fingerprints, methodology and descriptive distributions accompany
the batch evidence, without a product frame/latency acceptance claim.
