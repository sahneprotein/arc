//! Deterministic headless cooker. Payload JSON is schema-versioned and hashed as bytes.
mod geometry;
pub use geometry::*;
mod rules;
mod terrain;
use scene_data::*;
use std::{collections::BTreeMap, time::Instant};

#[derive(Clone, Debug)]
pub struct CookedPackage {
    pub simulation: SimulationPayload,
    pub visual: VisualPayload,
    pub simulation_bytes: Vec<u8>,
    pub visual_bytes: Vec<u8>,
    pub source_hash: ContentHash,
    pub simulation_hash: ContentHash,
    pub visual_hash: ContentHash,
    pub timings: CookTimings,
}
#[derive(Clone, Copy, Debug)]
pub struct CookTimings {
    pub validation_us: u64,
    pub geometry_us: u64,
    pub encoding_us: u64,
    pub rule_us: u64,
    pub total_us: u64,
}
pub fn cook(scene: &SceneDocument) -> Result<CookedPackage, Diagnostic> {
    cook_with_cancel(scene, &|| Ok(()))
}
/// Cooperative cancellation at validation/encoding boundaries and geometry rows.
pub fn cook_with_cancel(
    scene: &SceneDocument,
    check: &dyn Fn() -> Result<(), Diagnostic>,
) -> Result<CookedPackage, Diagnostic> {
    check()?;
    let start = Instant::now();
    let source_hash = source_hash(scene)?;
    let validation_us = elapsed(start);
    let geometry_start = Instant::now();
    let objects: BTreeMap<_, _> = scene.objects.iter().map(|o| (&o.id, o)).collect();
    let transforms = world_transforms(scene)?;
    let mut rule_us = 0;
    let mut simulation = SimulationPayload {
        schema: COOK_SCHEMA,
        profile: COLLISION_PROFILE.into(),
        boxes: Vec::new(),
        spawns: Vec::new(),
        ground: Vec::new(),
    };
    let mut visual = VisualPayload {
        schema: COOK_SCHEMA,
        terrain: Vec::new(),
    };
    for o in objects.values() {
        check()?;
        let world = &transforms[&o.id];
        match &o.data {
            ObjectData::Box { half_extents_m } => simulation.boxes.push(CookedBox {
                id: o.id.clone(),
                transform: world.clone(),
                half_extents_m: *half_extents_m,
            }),
            ObjectData::Spawn { order } => simulation.spawns.push(CookedSpawn {
                id: o.id.clone(),
                order: *order,
                position_m: world.translation_m,
            }),
            ObjectData::Terrain(t) => {
                let ground = terrain::cook_ground(&o.id, t, world, check)?;
                let rule_start = Instant::now();
                let world_up = glam::Quat::from_array(world.rotation_xyzw) * glam::Vec3::Y;
                let derived_layers = rules::derive_layers(t, &ground, world_up.to_array(), check)?;
                let decoration = rules::derive(&o.id, t, &ground, &derived_layers, check)?;
                rule_us += elapsed(rule_start);
                simulation.ground.extend(ground);
                let mut layers = t.layers.clone();
                layers.sort_by(|a, b| a.id.cmp(&b.id));
                let mut tiles = t.tiles.clone();
                tiles.sort_by(|a, b| a.id.cmp(&b.id));
                for tile in &mut tiles {
                    tile.layers.sort_by(|a, b| a.layer.cmp(&b.layer));
                }
                visual.terrain.push(VisualTerrain {
                    object: o.id.clone(),
                    surface_group: t.surface_group.clone(),
                    rules: {
                        let mut rules = t.rules.clone();
                        rules.sort_by(|a, b| a.id.cmp(&b.id));
                        for rule in &mut rules {
                            rule.input_layers.sort();
                            rule.input_rules.sort();
                            rule.exclusion_layers.sort();
                        }
                        rules
                    },
                    decoration,
                    derived_layers,
                    layers,
                    tiles,
                });
            }
            ObjectData::Group => {}
        }
    }
    simulation.spawns.sort_by_key(|s| s.order);
    check()?;
    simulation.validate()?;
    visual.validate(&simulation.ground)?;
    let geometry_us = elapsed(geometry_start).saturating_sub(rule_us);
    let encoding_start = Instant::now();
    let simulation_bytes = encode(&simulation)?;
    check()?;
    let visual_bytes = encode(&visual)?;
    let simulation_hash = payload_hash(&simulation_bytes);
    let visual_hash = payload_hash(&visual_bytes);
    let encoding_us = elapsed(encoding_start);
    Ok(CookedPackage {
        simulation,
        visual,
        simulation_bytes,
        visual_bytes,
        source_hash,
        simulation_hash,
        visual_hash,
        timings: CookTimings {
            validation_us,
            geometry_us,
            encoding_us,
            rule_us,
            total_us: elapsed(start),
        },
    })
}
fn encode(value: &impl serde::Serialize) -> Result<Vec<u8>, Diagnostic> {
    let bytes =
        serde_json::to_vec(value).map_err(|e| Diagnostic::new("cook.encode").with("detail", e))?;
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(Diagnostic::new("payload.limit"));
    }
    Ok(bytes)
}
fn elapsed(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u64::MAX as u128) as u64
}
