# Terrain paint request/result

UI supplies ray hits, the complete ordered world-space stroke path, layer selection,
brush controls and overlays. Core enumerates texels and computes falloff. Use
`Command::PreviewTerrainPaint { transaction, sequence, request }` with
`TerrainPaintRequest { object, layer, path_world_m, radius_m, strength, falloff, mode }`;
`mode` is `TerrainPaintMode::{Paint, Erase}`. Existing Begin/Commit/CancelViewportEdit
commands surround the stroke. Commit label `paint` is valid.

`CommandOutcome::TerrainPainted { sequence, result }` returns
`TerrainPaintResult { object, layer, texel_deltas, dirty_regions }`. Both arrays are
`Arc<[...]>`. Each `TerrainPaintTexel` has tile, layer, index:u32, before:f32,
after:f32 and weight:f32. Deltas include changed blend peers; index is row-major
(z then x) at that tile/layer's authored resolution. Results sort by tile ID,
layer ID, index; dirty regions sort by object/tile ID. UI can check whether deltas
are empty; drawing uses the existing Geometry job and preview dirty regions.
Geometry, GeometryBinding, GroundRenderer and GrassRenderer signatures are stable.
The low-level TerrainLayer mutation is not the UI brush API.

Every sequence evaluates against the transaction's original document. Composed
rigid transforms map the world path to terrain-local XZ. The sculpt and paint
kernels share capsule distance and quadratic falloff. For radius r, soft fraction
q, inner radius r*(1-q), the influence f is 1 inside the inner radius, 0 at/outside
r, and 1-t² across the edge. Take the maximum influence over the entire path;
overlap, repeated points and repeated preview events do not accumulate paint.

For canonical base selected weight b and strength s in [0,1]:

- Paint: selected = clamp((1-f)*b + f*s, 0, 1); at f=1 return s directly.
- Erase: selected = clamp(b*(1-s*f), 0, 1).

Paint Strength is the target weight at the brush centre. Erase Strength is the
fraction removed at the centre. A Mask is just this clamped scalar. A BlendWeight
keeps selected fixed and apportions remainder 1-selected to its same-group peers
proportionally to their canonical BASE weights. Zero base peer sum falls back to
registered peer-default ratios; zero peer-default sum gives the remainder to the
lowest LayerId peer. Compute ratios in f64 in sorted LayerId order, round each
allocation to f32, then let the largest allocated peer (tie: lowest LayerId) absorb
the residual computed in f64 after the other rounded values, clamped to [0,1].
Result group sums satisfy the validated 1e-5 tolerance. If selected bits are
unchanged, retain canonical base group bits. A singleton group rejects any affected
selected weight below 1 with `layer.single_weight`.

Coincident border/corner texels have one base owner: lowest TileKey, independent
of storage order. Mixed-resolution shared coordinates compare exact reduced
rational tile coordinates, including negative keys. All coincident copies use the
owner's selected/peer base values and receive the same result; nonshared texels
remain at their own resolution. Authored paint may reconcile previously unequal
border masks. Missing payloads read as registered defaults and are materialized
only where values change, using existing group resolution or cells_per_tile+1.

Selected rule outputs and any actually changed rule-output peer reject with
`layer.derived_output`. No authored derived output is silently written. Invalid
or stale previews retain the last valid preview and sequence. Refused Preview or
Commit keeps the transaction open until Cancel; the UI cancels after refusals.
Cancel changes no authoritative source. Commit adopts the last valid preview as
one cross-tile undo. If source semantics are unchanged, Commit succeeds with
`Applied`, closes the transaction, and adds no undo, revision, dirty change or redo
clearing. A refused history.capacity Commit retains the preview for cancellation.

Limits: 1..256 finite points; positive radius <=10,000 terrain meters;
strength/falloff [0,1]; <=1M candidate rectangle visits; <=65,536 canonical
influenced sites AND <=65,536 changed stored texel weights including peers/copies.
Invalid request diagnostics are terrain.brush_invalid; work caps terrain.brush_limit.
Scene and payload budgets apply before missing-payload allocation. Commands stay
on the existing bounded worker FIFO; no new per-stroke thread or background queue.

# Registry and rendering

Creation defaults: a new blend surface group gets a fresh StableId; first member
has default 1, later members default 0. A Mask has default 0 and no color. Group
defaults must sum exactly 1 in stable-ID f32 order (and within 1e-5 in f64).
Layers <=256, side 2..1025, authored weights <=8M per terrain (`layer.limit`) and
across the scene (`scene.resource_limit`). Tiles*layers*side² can exhaust the
budget before layer-count/resolution limits. Registry materializes all registered
payloads, preserving every existing size/weight bit during ordinary rename/color
or default edits. New group members use that tile's existing group resolution.
Changed membership renormalizes surviving group values or defaults at zero sum.
Incompatible group resolutions reject atomically with layer.group_resolution.
`Mutation::TerrainLayerResolution { object, side_samples }` is the explicit
bilinear resampling operation over all registered tile/layer payloads. Same-size
resampling preserves bits. Initial side cells_per_tile+1 is valid.

Removing a layer referenced by a rule fails with rule.layer_binding unless the
same atomic Edit also updates the rule references. Readout is **average weight**:
mean authored samples per tile; missing payload contributes default_weight. It is
not physical area coverage. Snapshots contain authored payloads; label rule
outputs **by rule** rather than treating authored means as derived output.

The shared renderer blends color_linear_rgba using normalized weights only in
surface_group. Masks and other groups do not recolor. Cooked derived weights
supersede authored values for rule outputs. Paint/material-only changes reuse
exact cooked collision geometry; visuals/derived rules refresh. Grass remains
conservatively derived across the edited terrain. Changed renderer tiles currently
prepare complete CPU mesh buffers and replace their Bevy mesh contents while
retaining entity/asset handles. No GPU subrange, live-frame latency or product
budget claim follows from headless/native asset lifecycle tests.
