# Frozen terrain viewport seam

This seam is stable through initial terrain viewport integration. Enum/field changes
will be announced in the shared coordination log before landing. Additive grass
renderer APIs do not change this ground/geometry contract.

In `editor-services`, request `handle.job(JobOperation::Geometry)` and retain its
`JobTicket` if cancellation is needed. Geometry cooks the active preview or committed
source on the worker; it does not publish files or encode/hash packages. A newly
accepted Geometry request cancels the previous unfinished Geometry request. Every
accepted ticket emits exactly one JobFinished, including supersession. Treat
Geometry job.cancelled / job.stale as expected supersession, preserve the newest
request binding, and do not turn them into a UI success that clears other problems.
`queue.full` is recoverable and leaves the last accepted request usable. The bounded
dirty-height cache and rule/fallback limits are documented in
editor-cook/docs/geometry-preview.md. Geometry's encoding_us is zero; Cook still
performs full deterministic publication.

The result is exactly:

```rust
JobOutput::Geometry {
    binding: editor_core::GeometryBinding,
    simulation: Arc<scene_data::SimulationPayload>,
    visual: Arc<scene_data::VisualPayload>,
    timings: editor_cook::CookTimings,
}
```

`GeometryBinding {session: u64, revision: u64, preview: Option<(u64,u64)>}`
fences document/session/transaction/sequence. `from_snapshot(&ViewSnapshot)` and
`matches(&ViewSnapshot)` are public. Selection/camera changes do not invalidate it.
Drain a bounded event batch first, then obtain `handle.snapshot()` and check
`binding.matches(&view)` immediately before applying. A later Snapshot event in
that batch can invalidate an earlier Geometry event. Discard stale output and
request the newest binding; retain previous accepted mesh while work is pending.

The compiling `examples/viewport_contract.rs` shows an exact Bevy system call:

```rust
renderer.sync_with_visual(
    &mut commands, &mut meshes, &mut materials,
    &simulation.ground, Some(&visual),
    view.preview.as_ref().map(|p| p.dirty.as_ref()).unwrap_or(&[]),
)?;
```

`GroundRenderer` is a default Resource; initialize it along with the app's existing
Assets<Mesh>/Assets<StandardMaterial>. `sync(..., ground, dirty)` remains the L1
compatibility entry; `set_full_detail(bool)` is explicit and currently one L1 detail
level. GroundTile contains stable object/tile/key/cells_per_tile for picking.
Positions, normals and triangles are world-space, exactly cooked; entity transforms
are identity. Do not apply the authored terrain transform twice. The renderer owns
its entities, mesh updates and shared materials; UI owns cameras, view layers,
picking, gestures, brush overlays and wording. Default spawn visibility is the
ordinary world layer; an app with isolated render layers tags GroundTile entities
for its viewport. Box/spawn visuals remain app/game integration responsibilities.

Dirty regions have tile-local sample coordinates and exclusive max bounds, with
normal/interpolation halo supplied by core. They describe the changed area; whole
changed tiles are currently rebuilt. UpdateFacts measures CPU prepared bytes/time,
not observed GPU traffic or a promised frame budget. Equal cooked tiles reuse meshes.

Run the headless compiling seam example (no GPU/window/performance claim):

```sh
cargo run --offline -p landscape-render --features bevy-render --example viewport_contract
```

For Open/Save, `JobOutput::Opened { disk_hash, source_hash, timings }` already has
public hashes. On successful Open, associate the pending path with
`DiskVersion::Hash(disk_hash)`; only successful Open changes that association.
On successful Saved use `receipt.disk_hash`. `source_hash` tracks semantic document
identity, never the expected disk version. A new Save As path uses Absent unless
an explicit overwrite flow has read its actual disk version. Open and path browsing
create no directories. Explicit Save creates parents and preserves atomic publish
and conflict checks. `save.parent_create` includes `path` and `detail`; UI owns
remedy wording. Error leaves saved identity and unsaved edits unchanged.

For Claude's current `Editor::pump` job-result match, the exact successful-Open arm is:

```rust
Ok(JobOutput::Opened { disk_hash, .. }) => {
    self.on_disk = DiskVersion::Hash(disk_hash);
    Ok(())
}
```

Also adopt the pending Open path only on success. A file-version token belongs to
that path; Save As to another path must not reuse the prior path's disk hash.
