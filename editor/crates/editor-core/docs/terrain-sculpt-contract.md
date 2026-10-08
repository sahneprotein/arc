# Terrain sculpt request/result

UI owns pointer ray hits, an ordered world-space path, brush settings and overlays.
Core owns affected global samples, falloff, mode semantics and transaction history.
Send the complete path on each monotonically increasing preview sequence:

```rust,ignore
Command::PreviewTerrainSculpt {
    transaction,
    sequence,
    request: TerrainSculptRequest {
        object,
        path_world_m,
        radius_m,
        strength,
        falloff,
        mode,
    },
}
```

`CommandOutcome::TerrainSculpted { sequence, result }` returns canonical sorted
`sample_deltas` (`key`, `before_m`, `after_m`, `weight`) and tile `dirty_regions`.
The preview snapshot also exposes those dirty regions. Every preview uses the
transaction's base document; commit adopts the last successful preview as one undo
step, and cancel removes it without changing source. Invalid/stale previews leave
the last successful preview intact. Existing Geometry jobs and GeometryBinding
fences apply during strokes.

Radius is in terrain meters; paths are inverse-transformed through the same composed
rigid hierarchy transform used by cook. A single hit makes a disk; successive hits
make a connected capsule path. The maximum influence across all segments is used
per authoritative sample, so repeated/overlapping hits do not compound strength.
Falloff 0 is full strength inside the radius; falloff 1 is a quadratic edge across
the whole radius. Intermediate values set the fraction of the radius used by the
soft edge. Given normalized edge position t, weight is `strength * (1 - t*t)`.

Raise/Lower apply `amount_m * weight` once across the complete stroke. Smooth moves
toward the average of available four-neighbor base heights. Flatten moves toward
the explicit **terrain-local** `height_m` target. To flatten at a stroke's first
world hit, call `terrain_hit_height_m(base_document, &object, first_hit_array)` and
use its result. This helper converts a real hit; it does not sample arbitrary XZ
or intersect geometry. Use world hit Y only for an explicitly world-space readout.

Limits: strength/falloff 0..1, positive radius <= `MAX_SCULPT_RADIUS_M` (10,000 m),
1..`MAX_SCULPT_POINTS` (256) finite hits, Raise/Lower amount 0..1,000 m,
<=1,000,000 candidate rectangle visits, <=`MAX_SCULPT_CHANGED_SAMPLES` (65,536)
changed samples. Core does not require a pointer minimum spacing or timestamps.
Path decimation/point-cap feedback belongs to UI. Empty results remain no-op
previews. A no-op Commit succeeds and closes the transaction without adding undo,
document revision, dirty change or clearing redo; cancelling an empty gesture is
also valid. Refused Preview/Commit retains the transaction until Cancel.

Dirty regions include border and normal-neighbor effects. Renderer currently
rebuilds changed tiles completely; GPU subranges, preview latency/frame acceptance,
and hardware-visible instance counts remain unmeasured.
