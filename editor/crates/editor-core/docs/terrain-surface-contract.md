# Terrain placement surface query

Public imports: `editor_core::{terrain_surface_hit, TerrainSurfaceRay, TerrainSurfaceHit}`.
This pure synchronous query reads one immutable scene and one selected terrain;
it creates no objects, jobs, caches or background work and has no rendering/game
runtime dependency. It is intended for occasional editor placement, not a measured
pointer/frame budget.

```rust,ignore
let view = handle.snapshot();
let ray = TerrainSurfaceRay {
    origin_world_m: camera_origin.to_array(),
    direction_world: camera_direction.to_array(),
    max_distance_m: 10_000.0,
};
if let Some(hit) = terrain_surface_hit(view.document.as_ref(), &terrain_object, &ray)? {
    let position = hit.position_world_m;
    // UI chooses box/spawn offset and orientation, then issues its ordinary Create
    // Edit using the captured session/document revision. Core never mutates here.
}
```

`TerrainSurfaceRay` fields are origin_world_m:[f32;3], direction_world:[f32;3],
max_distance_m:f64. Origin/direction must be finite, direction nonzero and maximum
finite/positive. Nonunit directions, including finite subnormal/large f32 values,
are normalized internally in f64. Distance is world meters, independent of input
direction length. No arbitrary near epsilon is imposed: accept strictly positive
distance <=maximum (inclusive). Rays starting on the surface at distance0 miss;
coplanar/parallel rays do not invent an isolated hit. The query is two-sided.

Result is `Result<Option<TerrainSurfaceHit>, Diagnostic>`, with fields:

- object:ObjectId, tile:TileId, key:TileKey
- cell_xz:[u32;2], tile-local height cell
- triangle_index:u32, index into that tile's CookedGround.triangles
- position_world_m:[f32;3], normal_world:[f32;3], distance_m:f64

Triangle index is2*(z*cells_per_tile+x)+half (half0 or1). The nearest positive hit
wins; equal distance ties use TileKey then triangle_index, independent of tile IDs
or authored storage order. Normal is the geometric triangle face normal in the
terrain's authored winding. It is neither smooth vertex-normal interpolation nor
flipped toward the ray; a hit from underneath has the same authored normal.

`scene_data::terrain::{cell_triangles, sample_world_position}` supplies the single
collision topology/global-sample placement convention to query, cooker and cooked
payload topology validation. At a row-major cell p00, p10 (next row), p01 (next
column), p11, the triangles are [p00,p10,p01] and [p10,p11,p01]. Diagonal p10--p01
is fixed. Every seam/corner world position uses its authoritative global SampleKey
and exact existing f32 quaternion/translation expression. Parent transforms use
the existing world_transforms composer. Mixed layer resolutions never affect the
height cell grid. Pre-refactor golden bytes cover identity and parent rotations
on noncoplanar negative tiles with shared corner and a gap.

Intersect these exact cooked f32 world vertices with f64 dominant-axis shearing
and closed edges. Boundary distances use only their shared vertex or canonical
ordered edge endpoints; this prevents different incident triangle areas rounding
an identical hit into different distances. No tolerance expands the terrain or
permits extrapolation. Float rounding of caller ray coordinates can place an
intended seam ray slightly on one side; the actual supplied ray is authoritative.

No hit, missing tile/gap, out-of-bounds, behind-origin or beyond-maximum returns
None. No height plane, bilinear height substitution or fallback is invented in
core. Claude's placement policy uses the viewport camera centre ray, current
committed source, nearest hit across terrain objects, upright box centre half its
height above the hit in worldY or spawn2m above. None falls back to its existing
view target at y=0; a Diagnostic prevents creation and is shown as a notice. Those
are UI choices. A caller needing active sculpt preview can explicitly supply
snapshot.preview.document; query changes no session/revision. Ordinary Create
while a viewport transaction is open still refuses transaction.busy.

Diagnostics: terrain.ray_invalid for invalid ray; object.missing for unknown
object; field.kind for an existing non-terrain object. Existing scene/transform
validation diagnostics propagate. Nonfinite sample world positions return
cook.geometry_nonfinite; collapsed/nonfinite cross products return
cook.degenerate_triangle, matching cooker checks. Scene validation happens before
traversal and all selected-terrain triangles are checked, even those outside the
ray; no partially valid hit is returned if later selected geometry is invalid.

Work is explicitly bounded by current schema: <=100000 scene objects, hierarchy
depth<=256, <=4096 tiles per terrain, <=1M authoritative samples, <=2M scene tile
vertices and fewer than4M candidate triangles. Query scratch contains one selected
terrain map of sample world positions and a bounded sorted tile list; no full
CookedGround/visual payload or all-hit list is allocated. Each triangle's closed
world AABB prunes intersection arithmetic, not source validation or traversal.
There is no BVH/spatial index or repeated-click cache in this slice. Calling once
per terrain multiplies these costs; no small-memory or interactive timing claim
follows merely from schema bounds. Runtime core dependencies are unchanged;
editor-cook is a dev dependency only, to test cooked geometry parity.
