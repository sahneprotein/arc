# Grass rendering and measurements

The separate GrassRenderer API is additive. Existing Geometry/GeometryBinding and
GroundRenderer fields/signatures are frozen through terrain viewport integration.
The Geometry result already supplies both visual decoration and exact cooked ground.

With `landscape-render` feature `bevy-render`, initialize `GrassRenderer` as a Resource
and call it only for the same accepted Geometry binding used for ground:

```rust
let facts = grass.sync(
    &mut commands, &mut meshes, &mut materials,
    &visual, &simulation.ground,
)?;
```

All derived instances share one unit crossed-blade mesh and StandardMaterial.
Transforms provide the cooker position, seeded yaw and width/height. This follows
pinned Bevy's built-in automatic instancing path described in
`examples/shader_advanced/custom_shader_instancing.rs`; actual batch count/GPU
submission is not inferred from CPU entities. No custom render pipeline or game
physics dependency is added. Material is opaque, two-sided and shared across views.
GrassInstance contains stable object/instance/tile/rule identities; picking/viewport
render-layer policy is UI-owned. Instances are visual output, never source entities.

`grass.set_enabled(value, &mut commands)` changes per-view Visibility only and retains
CPU instances/shared mesh. This does not alter source/visual/simulation hashes. An
empty accepted visual result despawns prior instances and removes the mesh asset.
Subsequent nonempty data creates a fresh shared mesh. Identity-preserving updates
reuse entities and mesh/material handles. Malformed input is rejected before state
or Commands changes. Asset handles/entity lifetime are renderer-owned; consumers
must not despawn renderer entities independently.

GrassRenderFacts reports:

- update.cpu_allocated_instances: actual retained derived instance records;
- instances_spawned/updated/removed and prepared_transform_bytes/preparation_us:
  CPU preparation, not observed GPU traffic;
- scene_instances: CPU instances represented by visible scene entities after the
  deferred Commands apply, zero while disabled;
- gpu_submitted_instances and gpu_visible_instances: None until measured on the
  render/GPU path, never aliases of loaded or scene counts.

GrassCache is an optional bounded CPU reference of transformed blade geometry used
for parity/cache tests. Production GrassRenderer does not allocate those duplicated
per-tile blades. Per-instance Bevy entities have measurable CPU/memory overhead; no
frame/memory budget is promised before representative live measurements. Rule
outputs have the existing global 100,000-instance cap. Slope/height-derived masks,
wind/material assets, streaming/LOD, game integration and owner visual acceptance
are separate remaining roadmap work.
