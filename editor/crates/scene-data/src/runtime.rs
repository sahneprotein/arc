//! Versioned CPU payloads: simulation decoders never need visual layers or rules.
use crate::*;
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Compose parent/local rigid transforms once for cooker and world-space tools.
/// Root values are copied exactly so the legacy map's float bits are preserved.
pub fn world_transforms(
    scene: &SceneDocument,
) -> Result<BTreeMap<ObjectId, RigidTransform>, Diagnostic> {
    let objects: BTreeMap<_, _> = scene.objects.iter().map(|o| (&o.id, o)).collect();
    let mut cache = BTreeMap::new();
    let mut active = std::collections::BTreeSet::new();
    for object in &scene.objects {
        compose_world(object, &objects, &mut cache, &mut active, 0)?;
    }
    Ok(cache)
}
fn compose_world(
    object: &ObjectRecord,
    objects: &BTreeMap<&ObjectId, &ObjectRecord>,
    cache: &mut BTreeMap<ObjectId, RigidTransform>,
    active: &mut std::collections::BTreeSet<ObjectId>,
    depth: usize,
) -> Result<RigidTransform, Diagnostic> {
    if let Some(transform) = cache.get(&object.id) {
        return Ok(transform.clone());
    }
    if depth >= 256 || !active.insert(object.id.clone()) {
        return Err(Diagnostic::new("hierarchy.transform_cycle").at(&object.id));
    }
    if !object
        .local
        .translation_m
        .iter()
        .chain(&object.local.rotation_xyzw)
        .all(|v| v.is_finite())
    {
        return Err(Diagnostic::new("transform.nonfinite").at(&object.id));
    }
    let norm = object
        .local
        .rotation_xyzw
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>();
    if (norm - 1.0).abs() > 0.0001 {
        return Err(Diagnostic::new("transform.rotation_not_unit").at(&object.id));
    }
    let world = if let Some(parent) = &object.parent {
        let parent = objects
            .get(parent)
            .ok_or_else(|| Diagnostic::new("reference.missing_parent").at(&object.id))?;
        let parent = compose_world(parent, objects, cache, active, depth + 1)?;
        let rotation = Quat::from_array(parent.rotation_xyzw);
        RigidTransform {
            translation_m: (Vec3::from_array(parent.translation_m)
                + rotation * Vec3::from_array(object.local.translation_m))
            .to_array(),
            rotation_xyzw: (rotation * Quat::from_array(object.local.rotation_xyzw)).to_array(),
        }
    } else {
        object.local.clone()
    };
    active.remove(&object.id);
    if !world
        .translation_m
        .iter()
        .chain(&world.rotation_xyzw)
        .all(|v| v.is_finite())
    {
        return Err(Diagnostic::new("transform.nonfinite").at(&object.id));
    }
    cache.insert(object.id.clone(), world.clone());
    Ok(world)
}

/// The schema of a cooked package's manifest and of both its payloads: the format's one
/// version. A package of any other is refused.
pub const COOK_SCHEMA: u32 = 1;
/// The most emitters a level may place.
pub const MAX_EMITTERS: usize = 4096;
pub const COLLISION_PROFILE: &str = "arc-game-rigid-box-trimesh-v1";
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CookedBox {
    pub id: ObjectId,
    pub transform: RigidTransform,
    pub half_extents_m: [f32; 3],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CookedSpawn {
    pub id: ObjectId,
    pub order: u32,
    pub position_m: [f32; 3],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CookedGround {
    pub object: ObjectId,
    pub tile: TileId,
    pub key: terrain::TileKey,
    pub cells_per_tile: u32,
    pub positions_m: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationPayload {
    pub schema: u32,
    pub profile: String,
    pub boxes: Vec<CookedBox>,
    pub spawns: Vec<CookedSpawn>,
    pub ground: Vec<CookedGround>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecorationInstance {
    pub id: StableId,
    pub rule: RuleId,
    pub tile: TileId,
    pub position_m: [f32; 3],
    pub yaw_turns: u16,
    pub width_m: f32,
    pub height_m: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedTerrainLayer {
    pub rule: RuleId,
    pub layer: LayerId,
    pub tile: TileId,
    pub side_samples: u32,
    pub weights: Vec<f32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualTerrain {
    pub rules: Vec<terrain::RuleDefinition>,
    pub decoration: Vec<DecorationInstance>,
    pub derived_layers: Vec<DerivedTerrainLayer>,
    #[serde(default)]
    pub surface_group: Option<StableId>,
    pub object: ObjectId,
    pub layers: Vec<terrain::LayerDefinition>,
    pub tiles: Vec<terrain::TerrainTile>,
}
/// One particle emitter as a level places it, for the client to draw. Nothing of it is in
/// the simulation payload: the server does not know it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CookedEmitter {
    /// The stable ID of the placement, which is the level object's. The seed of the
    /// placement's particles is taken from it.
    pub id: ObjectId,
    /// The stable ID of the emitter asset: `assets/particles/<emitter_id>.emitter-v1.json`.
    pub emitter_id: StableId,
    /// The SHA-256 of that file's exact bytes as they were when the level was cooked.
    pub emitter_sha256: ContentHash,
    /// Where the emitter is in the world, through its parents. It has no scale.
    pub transform: RigidTransform,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualPayload {
    pub schema: u32,
    pub terrain: Vec<VisualTerrain>,
    /// The placed emitters, by placement ID. Always written, empty in a level that places
    /// none.
    pub emitters: Vec<CookedEmitter>,
}
impl VisualPayload {
    pub fn validate(&self, ground: &[CookedGround]) -> Result<(), Diagnostic> {
        if self.schema != COOK_SCHEMA || self.terrain.len() > 4096 {
            return Err(Diagnostic::new("payload.visual_invalid"));
        }
        if self.emitters.len() > MAX_EMITTERS {
            return Err(Diagnostic::new("payload.emitter_limit"));
        }
        let mut placements = std::collections::BTreeSet::new();
        for emitter in &self.emitters {
            let transform = &emitter.transform;
            let norm: f64 = transform
                .rotation_xyzw
                .iter()
                .map(|v| (*v as f64).powi(2))
                .sum();
            if !emitter.id.valid()
                || !placements.insert(&emitter.id)
                || !emitter.emitter_id.valid()
                || !transform
                    .translation_m
                    .iter()
                    .chain(&transform.rotation_xyzw)
                    .all(|v| v.is_finite())
                || (norm - 1.0).abs() > 0.0001
            {
                return Err(Diagnostic::new("payload.emitter_invalid").at(&emitter.id));
            }
        }
        // By placement ID, as the cook writes them: the order is part of the bytes.
        if !self.emitters.windows(2).all(|pair| pair[0].id < pair[1].id) {
            return Err(Diagnostic::new("payload.emitter_invalid"));
        }
        let mut objects = std::collections::BTreeSet::new();
        let mut referenced = std::collections::BTreeSet::new();
        let mut instance_count = 0;
        let mut derived_weight_count = 0u64;
        for t in &self.terrain {
            if !t.object.valid() || !objects.insert(&t.object) {
                return Err(Diagnostic::new("payload.visual_invalid"));
            }
            crate::layers::validate_layers(&t.layers, &t.tiles, t.surface_group.as_ref())?;
            let cells: u64 = ground
                .iter()
                .filter(|g| g.object == t.object)
                .map(|g| g.triangles.len() as u64 / 2)
                .sum();
            let ordered = crate::rules::validate_rules(&t.rules, &t.layers, cells)?;
            instance_count += t.decoration.len();
            if instance_count > crate::rules::MAX_DECORATION {
                return Err(Diagnostic::new("rule.output_limit"));
            }
            let mut instances = std::collections::BTreeSet::new();
            for instance in &t.decoration {
                if !instance.id.valid()
                    || !instances.insert(&instance.id)
                    || !t.rules.iter().any(|r| {
                        r.id == instance.rule && r.algorithm == crate::rules::GRASS_SCATTER
                    })
                    || !t.tiles.iter().any(|tile| tile.id == instance.tile)
                    || !instance.position_m.iter().all(|v| v.is_finite())
                    || !instance.width_m.is_finite()
                    || !(0.0..=1.0).contains(&instance.width_m)
                    || instance.width_m == 0.0
                    || !instance.height_m.is_finite()
                    || !(0.0..=3.0).contains(&instance.height_m)
                    || instance.height_m == 0.0
                {
                    return Err(Diagnostic::new("payload.decoration_invalid"));
                }
            }
            for tile in &t.tiles {
                if !referenced.insert((&t.object, &tile.id))
                    || !ground
                        .iter()
                        .any(|g| g.object == t.object && g.tile == tile.id && g.key == tile.key)
                {
                    return Err(Diagnostic::new("payload.visual_tile"));
                }
            }
            let outputs: std::collections::BTreeMap<_, _> = t
                .rules
                .iter()
                .filter(|r| r.algorithm == crate::rules::AUTO_LAYER_SLOPE_HEIGHT)
                .filter_map(|r| r.output_layer.as_ref().map(|layer| (&r.id, layer)))
                .collect();
            let expected_count = outputs.len() * t.tiles.len();
            if t.derived_layers.len() != expected_count {
                return Err(Diagnostic::new("payload.derived_layer_missing"));
            }
            let mut derived_keys = std::collections::BTreeSet::new();
            let mut derived_seams = std::collections::BTreeMap::new();
            for derived in &t.derived_layers {
                let tile = t
                    .tiles
                    .iter()
                    .find(|tile| tile.id == derived.tile)
                    .ok_or_else(|| Diagnostic::new("payload.derived_layer_tile"))?;
                let ground_tile = ground
                    .iter()
                    .find(|g| g.object == t.object && g.tile == tile.id)
                    .ok_or_else(|| Diagnostic::new("payload.derived_layer_tile"))?;
                if !(1..=1024).contains(&ground_tile.cells_per_tile) {
                    return Err(Diagnostic::new("payload.derived_layer_invalid"));
                }
                let expected_side = ground_tile.cells_per_tile + 1;
                if outputs.get(&derived.rule).copied() != Some(&derived.layer)
                    || !derived_keys.insert((&derived.rule, &derived.tile))
                    || derived.side_samples != expected_side
                    || derived.weights.len() != (derived.side_samples as usize).pow(2)
                    || derived
                        .weights
                        .iter()
                        .any(|w| !w.is_finite() || !(0.0..=1.0).contains(w))
                {
                    return Err(Diagnostic::new("payload.derived_layer_invalid"));
                }
                derived_weight_count += derived.weights.len() as u64;
                if derived_weight_count > crate::rules::MAX_DERIVED_LAYER_WEIGHTS {
                    return Err(Diagnostic::new("payload.derived_layer_limit"));
                }
                let n = ground_tile.cells_per_tile as usize;
                for (i, weight) in derived.weights.iter().enumerate() {
                    let (x, z) = (i % (n + 1), i / (n + 1));
                    if x == 0 || x == n || z == 0 || z == n {
                        let key = (
                            &derived.layer,
                            i64::from(tile.key.x) * n as i64 + x as i64,
                            i64::from(tile.key.z) * n as i64 + z as i64,
                        );
                        if derived_seams
                            .insert(key, weight.to_bits())
                            .is_some_and(|previous| previous != weight.to_bits())
                        {
                            return Err(Diagnostic::new("payload.derived_layer_seam"));
                        }
                    }
                }
            }
            // Any derived layer read by an automatic rule must come from an earlier
            // dependency in the deterministic topological order.
            let mut produced = std::collections::BTreeSet::new();
            for rule in ordered {
                if rule.algorithm == crate::rules::AUTO_LAYER_SLOPE_HEIGHT {
                    for input in &rule.input_layers {
                        if outputs.values().any(|layer| *layer == input)
                            && !produced.contains(input)
                        {
                            return Err(Diagnostic::new("payload.derived_layer_order"));
                        }
                    }
                    produced.insert(rule.output_layer.as_ref().unwrap());
                }
            }
        }
        if ground
            .iter()
            .any(|g| !referenced.contains(&(&g.object, &g.tile)))
        {
            return Err(Diagnostic::new("payload.visual_tile"));
        }
        Ok(())
    }
}

pub fn payload_hash(bytes: &[u8]) -> ContentHash {
    ContentHash(Sha256::digest(bytes).into())
}
pub fn decode_simulation(
    bytes: &[u8],
    expected: ContentHash,
) -> Result<SimulationPayload, Diagnostic> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(Diagnostic::new("payload.limit"));
    }
    if payload_hash(bytes) != expected {
        return Err(Diagnostic::new("payload.hash_mismatch"));
    }
    let p: SimulationPayload = serde_json::from_slice(bytes)
        .map_err(|e| Diagnostic::new("payload.decode").with("detail", e))?;
    p.validate()?;
    Ok(p)
}
impl SimulationPayload {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        if self.schema != COOK_SCHEMA || self.profile != COLLISION_PROFILE {
            return Err(Diagnostic::new("payload.schema_or_profile"));
        }
        if self.boxes.len() + self.spawns.len() > MAX_OBJECTS || self.ground.len() > 4096 {
            return Err(Diagnostic::new("payload.limit"));
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut orders = std::collections::BTreeSet::new();
        for b in &self.boxes {
            if !b.id.valid()
                || !ids.insert(&b.id)
                || !b.half_extents_m.iter().all(|x| x.is_finite() && *x > 0.0)
                || !b
                    .transform
                    .translation_m
                    .iter()
                    .chain(&b.transform.rotation_xyzw)
                    .all(|x| x.is_finite())
                || (b
                    .transform
                    .rotation_xyzw
                    .iter()
                    .map(|x| (*x as f64).powi(2))
                    .sum::<f64>()
                    - 1.0)
                    .abs()
                    > 0.0001
            {
                return Err(Diagnostic::new("payload.box_invalid"));
            }
        }
        for s in &self.spawns {
            if !s.id.valid()
                || !ids.insert(&s.id)
                || !orders.insert(s.order)
                || !s.position_m.iter().all(|x| x.is_finite())
            {
                return Err(Diagnostic::new("payload.spawn_invalid"));
            }
        }
        let mut vertices = 0usize;
        let mut tiles = std::collections::BTreeSet::new();
        let mut keys = std::collections::BTreeSet::new();
        let mut seam_samples = std::collections::BTreeMap::new();
        let mut sizes = std::collections::BTreeMap::new();
        for g in &self.ground {
            let n = g.cells_per_tile as usize;
            if !(1..=1024).contains(&n)
                || !g.object.valid()
                || !g.tile.valid()
                || !tiles.insert((&g.object, &g.tile))
                || !keys.insert((&g.object, g.key))
                || g.positions_m.len() != (n + 1) * (n + 1)
                || g.normals.len() != g.positions_m.len()
                || g.triangles.len() != n * n * 2
            {
                return Err(Diagnostic::new("payload.ground_invalid"));
            }
            if sizes
                .insert(&g.object, n)
                .is_some_and(|previous| previous != n)
            {
                return Err(Diagnostic::new("payload.ground_invalid"));
            }
            vertices += g.positions_m.len();
            if vertices > 2_000_000
                || !g
                    .positions_m
                    .iter()
                    .chain(&g.normals)
                    .flatten()
                    .all(|x| x.is_finite())
                || g.triangles.iter().any(|t| {
                    t.iter().any(|i| *i as usize >= g.positions_m.len())
                        || t[0] == t[1]
                        || t[1] == t[2]
                        || t[0] == t[2]
                })
            {
                return Err(Diagnostic::new("payload.ground_invalid"));
            }
            for z in 0..n {
                for x in 0..n {
                    let expected = crate::terrain::cell_triangles(n as u32, x as u32, z as u32)
                        .expect("validated cooked terrain cell");
                    let i = (z * n + x) * 2;
                    if g.triangles[i..i + 2] != expected {
                        return Err(Diagnostic::new("payload.topology"));
                    }
                }
            }
            for (i, (position, normal)) in g.positions_m.iter().zip(&g.normals).enumerate() {
                if (normal.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() - 1.0).abs() > 0.0001 {
                    return Err(Diagnostic::new("payload.normal"));
                }
                let key = (
                    &g.object,
                    g.key.x as i64 * n as i64 + (i % (n + 1)) as i64,
                    g.key.z as i64 * n as i64 + (i / (n + 1)) as i64,
                );
                let bits = (position.map(f32::to_bits), normal.map(f32::to_bits));
                if seam_samples
                    .insert(key, bits)
                    .is_some_and(|old| old != bits)
                {
                    return Err(Diagnostic::new("payload.seam"));
                }
            }
        }
        Ok(())
    }
}
