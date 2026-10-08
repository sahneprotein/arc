# Derived landscape rules (v1)

`layer.slope_height` is a deterministic visual-data rule. It writes one row-major sample grid per output layer and tile into `VisualTerrain::derived_layers`; generated values never enter the simulation payload or simulation hash. Output layer IDs are authored, stable registry IDs. A rule output may be a mask or blend-weight layer. A derived output consumed by another rule must be named in `input_layers` and its producer must be in that rule's transitive `input_rules` dependency set.

At every authoritative global height sample, the cooker evaluates normalized slope as `clamp(1 - dot(world_normal, terrain_world_up), 0, 1)` and height in terrain-local meters. Each configured min/max band contributes a bounded linear edge ramp: values outside the range are zero; with zero falloff, values inside are one; otherwise each edge rises linearly over its configured falloff distance. Slope and height bands multiply together, as do input-mask weights. Authored masks are bilinearly sampled at their own resolution. Where authored masks disagree at a seam, the first tile in `TileKey` order owns the generated rule input; the source masks are retained. Each global output sample is evaluated once and copied to adjacent tiles, so shared derived borders have identical weight bits. Payload validation rejects contradictory derived seams. Rules, tiles, and samples use deterministic topological, tile-key, and row-major ordering.

The payload is capped at 2,000,000 derived weights for a scene. Each rule remains versioned and unsupported versions fail closed. `grass.scatter` can sample authored masks or dependency-ordered derived masks for density/exclusion. Grass instances and all generated masks remain visual-only; cooked ground triangles and collision data are unchanged.

# Existing rule panel API (documentation, no new source scope)

Use the snapshot's `ObjectData::Terrain(t).rules` and registry IDs. The entire rule
list is replaced atomically; there is no separate AddRule job, name, enabled flag,
rule preview command or mutable derived-layer payload in this API.

```rust,ignore
handle.submit(CommandRequest {
    request,                         // unique request ID
    session: snapshot.session,
    expected_revision: snapshot.document_revision,
    expected_inspector_revision: None,
    command: Command::Edit {
        label_key: "rule".into(),     // ordinary unrestricted undo label
        mutations: vec![Mutation::TerrainRules { object, rules }],
    },
})?;
```

`RuleDefinition` fields, unchanged:

```rust,ignore
RuleDefinition {
    id: RuleId,                      // StableId, fresh on creation
    algorithm: String,               // registered string below
    version: u32,                    // 1
    seed: u64,
    input_layers: Vec<LayerId>,
    input_rules: Vec<RuleId>,
    output_layer: Option<LayerId>,
    density_layer: Option<LayerId>,
    exclusion_layers: Vec<LayerId>,
    parameters: BTreeMap<String, f32>,
}
```

Keep unchanged rule IDs, registry IDs, seeds and parameter values on ordinary
edits. For a new slope/height output, send TerrainRegistry and TerrainRules in the
same Edit so the new registry layer exists when final scene validation runs.
For removal, update dependent rules and layer references in the same Edit. Rules
validate only after all mutations, enabling either order. Layer registry validates
its own group/payload state during its mutation. Unknown object/kind rejects with
object.missing / field.kind. An invalid Edit preserves source/revision/history.
Success is `CommandOutcome::Applied` via `ServiceEvent::Command(CommandResult)`;
use the worker's new snapshot, not an optimistic local revision. Changed semantics
produce one undo entry, dirty status and cook_needed. Semantic no-op changes none
of those. There is no special rule outcome. An ordinary Edit during a viewport
transaction refuses transaction.busy.

Generic BeginViewportEdit + Preview {transaction, sequence, mutations} also accepts
TerrainRules for an explicitly transactional panel preview. Each Preview is based
on the original transaction document; use increasing sequences and send the whole
rule list. Commit/Cancel has the same no-op/refusal contract as paint. Rules alone
currently produce no sample dirty rectangles; that does not indicate unchanged
visuals. Always request existing JobOperation::Geometry for the updated snapshot,
accept only a matching GeometryBinding and pass its visual payload to the shared
Ground/Grass renderers. A changed rule list refreshes visual derivation, retaining
unchanged exact collision. Save/Cook are blocked during a viewport transaction;
after Commit, ordinary Save persists authored rules and Cook publishes derived
visual data. Cancel/Undo restores rules and requires Geometry for restored source.

`layer.slope_height` v1 requires exactly one existing output layer (Mask or blend
member), unique among rule outputs. density_layer=None, exclusion_layers empty.
input_layers are existing Mask IDs only and multiply together. A derived mask's
producer must occur in the consumer's transitive input_rules. Registered settings:

| Parameter | Default | Accepted range |
| --- | --- | --- |
| slope_min | 0 | 0..1, strictly below slope_max |
| slope_max | 1 | 0..1 |
| slope_falloff | 0.1 | 0..1 |
| height_min_m | -100000 | -100000..100000, strictly below height_max_m |
| height_max_m | 100000 | -100000..100000 |
| height_falloff_m | 1 | 0..100000 |

The default positive slope falloff makes a weight zero at slope_min, including
perfectly flat ground at slope=0. An explicit slope_falloff=0 admits flat ground
inside the slope band. These are band semantics, not an implicit grass preset.
Height is terrain-local; slope uses cooked world normals relative to terrain up.

`grass.scatter` v1 requires output_layer=None. Optional density_layer and each
exclusion_layers ID can be any existing layer; input_layers must equal their
exact unique union, without extra declarations. Default density when no layer is
selected is1. A derived input again requires its producer in transitive input_rules.
Grass rules in direct input_rules also supply placed instances for distance
exclusion (world XZ distance); there is no implicit self-spacing rule.

| Parameter | Default | Accepted range |
| --- | --- | --- |
| candidates_per_cell | 4 | integer 1..16 |
| density | 1 | 0..1 |
| height_m | 0.4 | greater than0, at most3 |
| width_m | 0.08 | greater than0, at most1 |
| exclusion_distance_m | 1 | 0..10 |

Both registered algorithms/version1 are visual-only. Unsupported algorithm/version
fails closed. Seed is u64; scatter identities derive deterministically from object,
rule ID, seed, global cell and candidate. Derived arrays are in
JobOutput::Geometry.visual.terrain[*].derived_layers, decoration in .decoration;
authored source snapshots do not contain generated weights. Rule outputs are shown
as 'by rule' and reject authored paint, including changed blend peers.

Validation diagnostics for wording:

| Code | Cause/remedy |
| --- | --- |
| rule.limit | More than64 rules on a terrain |
| rule.id | Invalid/duplicate stable rule ID |
| rule.unsupported | Unregistered algorithm or version |
| rule.output_layer | Missing/duplicate output, output on grass, or invalid output-specific bindings |
| rule.layer_binding | Missing input layer, non-Mask slope input, duplicate/mismatched declared inputs; also referenced layer removal |
| rule.duplicate_binding | Repeated slope input, repeated exclusion or density also excluded |
| rule.parameter | Unknown/nonfinite/out-of-range setting; diagnostic parameter field names the setting |
| rule.parameter_range | Min>=max for slope or height |
| rule.duplicate_dependency | Repeated input_rules entry |
| rule.dependency_missing | Missing rule ID or missing transitive producer dependency for a derived input |
| rule.cycle | Cyclic rule dependency graph |
| rule.candidate_limit | More than1M total grass candidates on one terrain |
| scene.resource_limit | Scene-wide candidates>1M or derived weights>2M (and existing scene budgets) |
| rule.output_limit | Generated decoration>100000; can arise on Geometry/Cook after a valid command |
| payload.derived_layer_limit | Cooked derived weights exceed2M |

Existing command/service errors also apply: session.stale, revision.stale,
inspector.stale, request.reused, transaction.busy/stale, preview.stale,
history.capacity, queue.full and worker.stopped. Refused Preview/Commit leaves the
transaction open until Cancel. Geometry supersession job.cancelled/job.stale still
has exactly one terminal event per accepted ticket and is silent in UI. Cooked
payload validation errors remain job errors; never treat a failed Geometry as
success or clear an unrelated notice on supersession.
