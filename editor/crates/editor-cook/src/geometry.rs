//! Bounded viewport cache. Publication still goes through the full cooker.
mod layout;
use crate::{rules, terrain as cook_terrain, CookTimings};
use layout::{same_geometry_layout, same_layout, same_transform};
use scene_data::{terrain::*, *};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Instant,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct GeometryWork {
    /// Incremental-path changed samples (zero on a cold/metadata/bulk fallback).
    pub changed_samples: usize,
    pub rebuilt_tiles: usize,
    pub context_tiles: usize,
    pub reused_tiles: usize,
    pub derived_samples_evaluated: usize,
    /// Grass exclusions may depend on other tiles/rules: recompute the edited terrain.
    pub scatter_cells_visited: usize,
}
#[derive(Clone, Debug)]
pub struct GeometryPackage {
    pub simulation: Arc<SimulationPayload>,
    pub visual: Arc<VisualPayload>,
    pub timings: CookTimings,
    pub work: GeometryWork,
}
struct Cached {
    scene: Arc<SceneDocument>,
    transforms: BTreeMap<ObjectId, RigidTransform>,
    simulation: Arc<SimulationPayload>,
    visual: Arc<VisualPayload>,
}
/// Exactly one successful source/payload pair; failures/cancel never publish a partial cache.
#[derive(Default)]
pub struct GeometryCache {
    previous: Option<Cached>,
}
impl GeometryCache {
    pub fn clear(&mut self) {
        self.previous = None;
    }
    pub fn cook(
        &mut self,
        scene: Arc<SceneDocument>,
        check: &dyn Fn() -> Result<(), Diagnostic>,
    ) -> Result<GeometryPackage, Diagnostic> {
        check()?;
        let start = Instant::now();
        scene_data::validate(&scene)?;
        check()?;
        let validation_us = crate::elapsed(start);
        let geometry_start = Instant::now();
        let transforms = world_transforms(&scene)?;
        let objects: BTreeMap<_, _> = scene.objects.iter().map(|o| (&o.id, o)).collect();
        let old_objects: BTreeMap<_, _> = self
            .previous
            .as_ref()
            .map(|p| p.scene.objects.iter().map(|o| (&o.id, o)).collect())
            .unwrap_or_default();
        let mut simulation = SimulationPayload {
            schema: COOK_SCHEMA,
            profile: COLLISION_PROFILE.into(),
            boxes: vec![],
            spawns: vec![],
            ground: vec![],
        };
        let mut visual = VisualPayload {
            schema: COOK_SCHEMA,
            terrain: vec![],
        };
        let mut work = GeometryWork::default();
        let mut rule_us = 0;
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
                ObjectData::Group => {}
                ObjectData::Terrain(t) => {
                    let previous = self.previous.as_ref().and_then(|p| {
                        let old = old_objects.get(&o.id)?;
                        let ObjectData::Terrain(old) = &old.data else {
                            return None;
                        };
                        if !same_transform(p.transforms.get(&o.id)?, world)
                            || !same_geometry_layout(old, t)
                        {
                            return None;
                        }
                        let visual = p.visual.terrain.iter().find(|v| v.object == o.id)?;
                        Some((p, old, visual))
                    });
                    let previous = previous.filter(|(_, old, _)| {
                        old.samples
                            .iter()
                            .zip(&t.samples)
                            .filter(|(a, b)| a.height_m.to_bits() != b.height_m.to_bits())
                            .take(65_537)
                            .count()
                            <= 65_536
                    });
                    let previous_visual = previous.filter(|(_, old, _)| same_layout(old, t));
                    let changed: Option<BTreeSet<_>> = previous.map(|(_, old, _)| {
                        old.samples
                            .iter()
                            .zip(&t.samples)
                            .filter(|(a, b)| a.height_m.to_bits() != b.height_m.to_bits())
                            .map(|(_, b)| b.key)
                            .collect()
                    });
                    let ground;
                    let dirty;
                    if let (Some((p, _, old_visual)), Some(changed)) = (previous, changed.as_ref())
                    {
                        work.changed_samples += changed.len();
                        let old_ground: Vec<_> = p
                            .simulation
                            .ground
                            .iter()
                            .filter(|g| g.object == o.id)
                            .cloned()
                            .collect();
                        if changed.is_empty() {
                            work.reused_tiles += old_ground.len();
                            if previous_visual.is_some() {
                                simulation.ground.extend(old_ground);
                                visual.terrain.push(old_visual.clone());
                                continue;
                            }
                            ground = old_ground;
                            dirty = BTreeSet::new();
                        } else {
                            dirty = normal_halo(changed);
                            ground = patch_ground(
                                &o.id, t, world, old_ground, &dirty, check, &mut work,
                            )?;
                        }
                    } else {
                        dirty = BTreeSet::new();
                        ground = cook_terrain::cook_ground(&o.id, t, world, check)?;
                        work.rebuilt_tiles += ground.len();
                        work.context_tiles += ground.len();
                    }
                    let rule_start = Instant::now();
                    let up =
                        (glam::Quat::from_array(world.rotation_xyzw) * glam::Vec3::Y).to_array();
                    let (derived_layers, evaluated) = rules::derive_layers_cached(
                        t,
                        &ground,
                        up,
                        previous_visual.map(|(_, _, v)| (v.derived_layers.as_slice(), &dirty)),
                        check,
                    )?;
                    work.derived_samples_evaluated += evaluated;
                    let decoration = rules::derive(&o.id, t, &ground, &derived_layers, check)?;
                    work.scatter_cells_visited += t
                        .rules
                        .iter()
                        .filter(|r| r.algorithm == scene_data::rules::GRASS_SCATTER)
                        .count()
                        * t.tiles.len()
                        * t.cells_per_tile as usize
                        * t.cells_per_tile as usize;
                    rule_us += crate::elapsed(rule_start);
                    simulation.ground.extend(ground);
                    let mut layers = t.layers.clone();
                    layers.sort_by(|a, b| a.id.cmp(&b.id));
                    let mut tiles = t.tiles.clone();
                    tiles.sort_by(|a, b| a.id.cmp(&b.id));
                    for tile in &mut tiles {
                        tile.layers.sort_by(|a, b| a.layer.cmp(&b.layer));
                    }
                    let mut definitions = t.rules.clone();
                    definitions.sort_by(|a, b| a.id.cmp(&b.id));
                    for rule in &mut definitions {
                        rule.input_layers.sort();
                        rule.input_rules.sort();
                        rule.exclusion_layers.sort();
                    }
                    visual.terrain.push(VisualTerrain {
                        object: o.id.clone(),
                        surface_group: t.surface_group.clone(),
                        layers,
                        tiles,
                        rules: definitions,
                        derived_layers,
                        decoration,
                    });
                }
            }
        }
        simulation.spawns.sort_by_key(|s| s.order);
        check()?;
        simulation.validate()?;
        visual.validate(&simulation.ground)?;
        check()?;
        let package = GeometryPackage {
            simulation: Arc::new(simulation),
            visual: Arc::new(visual),
            work,
            timings: CookTimings {
                validation_us,
                geometry_us: crate::elapsed(geometry_start).saturating_sub(rule_us),
                rule_us,
                encoding_us: 0,
                total_us: crate::elapsed(start),
            },
        };
        self.previous = Some(Cached {
            scene,
            transforms,
            simulation: package.simulation.clone(),
            visual: package.visual.clone(),
        });
        Ok(package)
    }
}

fn normal_halo(changed: &BTreeSet<SampleKey>) -> BTreeSet<SampleKey> {
    changed
        .iter()
        .flat_map(|key| {
            (-1..=1).flat_map(move |dx| {
                (-1..=1).filter_map(move |dz| {
                    Some(SampleKey {
                        x: key.x.checked_add(dx)?,
                        z: key.z.checked_add(dz)?,
                    })
                })
            })
        })
        .collect()
}
fn patch_ground(
    object: &ObjectId,
    t: &TerrainDocument,
    world: &RigidTransform,
    mut old: Vec<CookedGround>,
    dirty: &BTreeSet<SampleKey>,
    check: &dyn Fn() -> Result<(), Diagnostic>,
    work: &mut GeometryWork,
) -> Result<Vec<CookedGround>, Diagnostic> {
    // Include every tile containing a changed position OR a changed vertex normal.
    let n = i64::from(t.cells_per_tile);
    let mut affected = BTreeSet::new();
    for key in dirty {
        for x in [
            key.x.checked_sub(1).map(|x| x.div_euclid(n)),
            Some(key.x.div_euclid(n)),
        ]
        .into_iter()
        .flatten()
        {
            for z in [
                key.z.checked_sub(1).map(|z| z.div_euclid(n)),
                Some(key.z.div_euclid(n)),
            ]
            .into_iter()
            .flatten()
            {
                if let (Ok(x), Ok(z)) = (i32::try_from(x), i32::try_from(z)) {
                    affected.insert(TileKey { x, z });
                }
            }
        }
    }
    let targets: BTreeSet<_> = t
        .tiles
        .iter()
        .filter(|tile| affected.contains(&tile.key))
        .map(|tile| tile.key)
        .collect();
    // A complete tile normal needs all triangles from adjacent tiles, in original
    // TileKey accumulation order. Context tile outputs must NEVER replace old tiles.
    let context: BTreeSet<_> = targets
        .iter()
        .flat_map(|tile| {
            (-1..=1).flat_map(move |dx| {
                (-1..=1).filter_map(move |dz| {
                    Some(TileKey {
                        x: tile.x.checked_add(dx)?,
                        z: tile.z.checked_add(dz)?,
                    })
                })
            })
        })
        .collect();
    let tiles: Vec<_> = t
        .tiles
        .iter()
        .filter(|tile| context.contains(&tile.key))
        .map(|tile| TerrainTile {
            id: tile.id.clone(),
            key: tile.key,
            layers: vec![],
        })
        .collect();
    let mut keys = BTreeSet::new();
    for tile in &tiles {
        for z in 0..=t.cells_per_tile {
            check()?;
            for x in 0..=t.cells_per_tile {
                keys.insert(t.sample_key(tile.key, x, z));
            }
        }
    }
    let subset = TerrainDocument {
        cells_per_tile: t.cells_per_tile,
        spacing_xz_m: t.spacing_xz_m,
        tiles,
        samples: t
            .samples
            .iter()
            .filter(|s| keys.contains(&s.key))
            .cloned()
            .collect(),
        layers: vec![],
        rules: vec![],
        surface_group: None,
    };
    work.context_tiles += subset.tiles.len();
    work.rebuilt_tiles += targets.len();
    work.reused_tiles += t.tiles.len() - targets.len();
    let updated = cook_terrain::cook_ground(object, &subset, world, check)?;
    let replacements: BTreeMap<_, _> = updated
        .into_iter()
        .filter(|g| targets.contains(&g.key))
        .map(|g| (g.tile.clone(), g))
        .collect();
    for tile in &mut old {
        if let Some(new) = replacements.get(&tile.tile) {
            *tile = new.clone();
        }
    }
    Ok(old)
}
