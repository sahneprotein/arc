use glam::Vec3;
use scene_data::{rules::*, terrain::*, *};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn derive(
    object: &ObjectId,
    t: &TerrainDocument,
    ground: &[CookedGround],
    derived_layers: &[DerivedTerrainLayer],
    check: &dyn Fn() -> Result<(), Diagnostic>,
) -> Result<Vec<DecorationInstance>, Diagnostic> {
    let ordered = validate_rules(
        &t.rules,
        &t.layers,
        t.cells_per_tile as u64 * t.cells_per_tile as u64 * t.tiles.len() as u64,
    )?;
    let mut results: Vec<DecorationInstance> = Vec::new();
    let mut tiles = t.tiles.iter().collect::<Vec<_>>();
    tiles.sort_by_key(|tile| tile.key);
    for rule in ordered {
        if rule.algorithm != GRASS_SCATTER {
            continue;
        }
        check()?;
        let radius = setting(rule, "exclusion_distance_m", 1.0);
        let mut exclusions: BTreeMap<(i64, i64), Vec<[f32; 3]>> = BTreeMap::new();
        if radius > 0.0 {
            for instance in &results {
                if rule.input_rules.contains(&instance.rule) {
                    exclusions
                        .entry(bucket(instance.position_m, radius))
                        .or_default()
                        .push(instance.position_m);
                }
            }
        }
        for tile in &tiles {
            let mesh = ground
                .iter()
                .find(|g| g.tile == tile.id)
                .expect("cooked source tile exists");
            let n = t.cells_per_tile;
            for z in 0..n {
                check()?;
                for x in 0..n {
                    for candidate in 0..setting(rule, "candidates_per_cell", 4.0) as u32 {
                        let global = t.sample_key(tile.key, x, z);
                        let random = random_key(object, rule, global, candidate);
                        let u = fraction(&random[16..20]);
                        let v = fraction(&random[20..24]);
                        let tile_u = (x as f32 + u) / n as f32;
                        let tile_v = (z as f32 + v) / n as f32;
                        let density = rule.density_layer.as_ref().map_or(1.0, |id| {
                            weight(t, tile, id, tile_u, tile_v, derived_layers)
                        }) * setting(rule, "density", 1.0);
                        let exclusion = rule.exclusion_layers.iter().any(|id| {
                            weight(t, tile, id, tile_u, tile_v, derived_layers)
                                > fraction(&random[28..32])
                        });
                        if fraction(&random[24..28]) >= density || exclusion {
                            continue;
                        }
                        let i = (z * (n + 1) + x) as usize;
                        let [p00, p01, p10, p11] = [
                            mesh.positions_m[i],
                            mesh.positions_m[i + 1],
                            mesh.positions_m[i + n as usize + 1],
                            mesh.positions_m[i + n as usize + 2],
                        ]
                        .map(Vec3::from_array);
                        let point = if u + v <= 1.0 {
                            p00 + (p01 - p00) * u + (p10 - p00) * v
                        } else {
                            p11 + (p10 - p11) * (1.0 - u) + (p01 - p11) * (1.0 - v)
                        };
                        let key = bucket(point.to_array(), radius.max(f32::MIN_POSITIVE));
                        let mut excluded = false;
                        for dx in -1..=1 {
                            for dz in -1..=1 {
                                let Some(bx) = key.0.checked_add(dx) else {
                                    continue;
                                };
                                let Some(bz) = key.1.checked_add(dz) else {
                                    continue;
                                };
                                if exclusions.get(&(bx, bz)).is_some_and(|points| {
                                    points.iter().any(|p| {
                                        let delta = Vec3::from_array(*p) - point;
                                        delta.x * delta.x + delta.z * delta.z < radius * radius
                                    })
                                }) {
                                    excluded = true;
                                }
                            }
                        }
                        if excluded {
                            continue;
                        }
                        if results.len() >= MAX_DECORATION {
                            return Err(Diagnostic::new("rule.output_limit"));
                        }
                        results.push(DecorationInstance {
                            id: StableId(random[..16].iter().map(|b| format!("{b:02x}")).collect()),
                            rule: rule.id.clone(),
                            tile: tile.id.clone(),
                            position_m: point.to_array(),
                            yaw_turns: u16::from_le_bytes([random[30], random[31]]),
                            width_m: setting(rule, "width_m", 0.08),
                            height_m: setting(rule, "height_m", 0.4),
                        });
                    }
                }
            }
        }
    }
    results.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(results)
}

/// Derive versioned, render-only slope/height masks in stable rule/tile/sample order.
pub(crate) fn derive_layers(
    t: &TerrainDocument,
    ground: &[CookedGround],
    world_up: [f32; 3],
    check: &dyn Fn() -> Result<(), Diagnostic>,
) -> Result<Vec<DerivedTerrainLayer>, Diagnostic> {
    derive_layers_cached(t, ground, world_up, None, check).map(|(layers, _)| layers)
}

// Only used after the cache has verified identical tile/layer/rule metadata.
// Slope/height and their layer DAG are pointwise at the canonical global key.
pub(crate) fn derive_layers_cached(
    t: &TerrainDocument,
    ground: &[CookedGround],
    world_up: [f32; 3],
    previous: Option<(&[DerivedTerrainLayer], &BTreeSet<SampleKey>)>,
    check: &dyn Fn() -> Result<(), Diagnostic>,
) -> Result<(Vec<DerivedTerrainLayer>, usize), Diagnostic> {
    let mut evaluated = 0;
    let cached: BTreeMap<_, _> = previous
        .map(|(layers, _)| layers.iter().map(|p| ((&p.rule, &p.tile), p)).collect())
        .unwrap_or_default();
    let ordered = validate_rules(&t.rules, &t.layers, 0)?;
    let up = Vec3::from_array(world_up).normalize();
    let heights = t.heights();
    let mut derived = Vec::<DerivedTerrainLayer>::new();
    let mut by_layer = BTreeMap::<LayerId, BTreeMap<SampleKey, f32>>::new();
    let mut tiles = t.tiles.iter().collect::<Vec<_>>();
    tiles.sort_by_key(|tile| tile.key);
    for rule in ordered
        .into_iter()
        .filter(|r| r.algorithm == AUTO_LAYER_SLOPE_HEIGHT)
    {
        check()?;
        let layer = rule.output_layer.as_ref().expect("validated output layer");
        let mut rule_weights = BTreeMap::new();
        for tile in &tiles {
            check()?;
            let mesh = ground
                .iter()
                .find(|g| g.tile == tile.id)
                .ok_or_else(|| Diagnostic::new("cook.rule_tile"))?;
            let n = t.cells_per_tile as usize;
            let side = n + 1;
            let mut weights = Vec::with_capacity(side * side);
            for z in 0..side {
                check()?;
                for x in 0..side {
                    let i = z * side + x;
                    let key = t.sample_key(tile.key, x as u32, z as u32);
                    // Shared corners/borders are evaluated once. Authored masks may differ
                    // across tiles: the first tile in TileKey order owns their rule input.
                    if let Some(weight) = rule_weights.get(&key) {
                        weights.push(*weight);
                        continue;
                    }
                    if previous.is_some_and(|(_, dirty)| !dirty.contains(&key))
                        && let Some(old) = cached.get(&(&rule.id, &tile.id))
                    {
                        let weight = old.weights[i];
                        rule_weights.insert(key, weight);
                        weights.push(weight);
                        continue;
                    }
                    evaluated += 1;
                    let height = heights[&key];
                    let slope = (1.0 - Vec3::from_array(mesh.normals[i]).dot(up)).clamp(0.0, 1.0);
                    let mut weight = band(
                        slope,
                        setting(rule, "slope_min", 0.0),
                        setting(rule, "slope_max", 1.0),
                        setting(rule, "slope_falloff", 0.1),
                    ) * band(
                        height,
                        setting(rule, "height_min_m", -100_000.0),
                        setting(rule, "height_max_m", 100_000.0),
                        setting(rule, "height_falloff_m", 1.0),
                    );
                    for input in &rule.input_layers {
                        let source = if let Some(samples) = by_layer.get(input) {
                            samples[&key]
                        } else {
                            let authored = tile.layers.iter().find(|p| &p.layer == input);
                            authored.map_or_else(
                                || {
                                    t.layers
                                        .iter()
                                        .find(|l| &l.id == input)
                                        .map_or(0.0, scene_data::layers::default_weight)
                                },
                                |p| {
                                    scene_data::layers::sample_weight(
                                        p,
                                        x as f32 / n as f32,
                                        z as f32 / n as f32,
                                    )
                                },
                            )
                        };
                        weight *= source;
                    }
                    let weight = weight.clamp(0.0, 1.0);
                    rule_weights.insert(key, weight);
                    weights.push(weight);
                }
            }
            derived.push(DerivedTerrainLayer {
                rule: rule.id.clone(),
                layer: layer.clone(),
                tile: tile.id.clone(),
                side_samples: side as u32,
                weights,
            });
        }
        by_layer.insert(layer.clone(), rule_weights);
    }
    Ok((derived, evaluated))
}

fn band(value: f32, min: f32, max: f32, falloff: f32) -> f32 {
    if value < min || value > max {
        return 0.0;
    }
    if falloff == 0.0 {
        return 1.0;
    }
    ((value - min) / falloff)
        .clamp(0.0, 1.0)
        .min(((max - value) / falloff).clamp(0.0, 1.0))
}
fn weight(
    t: &TerrainDocument,
    tile: &TerrainTile,
    id: &LayerId,
    u: f32,
    v: f32,
    derived: &[DerivedTerrainLayer],
) -> f32 {
    if let Some(payload) = derived.iter().find(|p| p.tile == tile.id && &p.layer == id) {
        return sample_grid(&payload.weights, payload.side_samples as usize, u, v);
    }
    let layer = t
        .layers
        .iter()
        .find(|l| &l.id == id)
        .expect("validated layer binding");
    tile.layers
        .iter()
        .find(|p| &p.layer == id)
        .map_or(scene_data::layers::default_weight(layer), |p| {
            scene_data::layers::sample_weight(p, u, v)
        })
}
fn sample_grid(weights: &[f32], n: usize, u: f32, v: f32) -> f32 {
    let x = u.clamp(0.0, 1.0) * (n - 1) as f32;
    let z = v.clamp(0.0, 1.0) * (n - 1) as f32;
    let (x0, z0) = (x as usize, z as usize);
    let (x1, z1) = ((x0 + 1).min(n - 1), (z0 + 1).min(n - 1));
    let (dx, dz) = (x - x0 as f32, z - z0 as f32);
    let a = weights[z0 * n + x0] * (1.0 - dx) + weights[z0 * n + x1] * dx;
    let b = weights[z1 * n + x0] * (1.0 - dx) + weights[z1 * n + x1] * dx;
    a * (1.0 - dz) + b * dz
}
fn random_key(
    object: &ObjectId,
    rule: &RuleDefinition,
    key: SampleKey,
    candidate: u32,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"arc-grass-scatter-v1\0");
    hash.update(object.0.as_bytes());
    hash.update(rule.id.0.as_bytes());
    hash.update(rule.seed.to_le_bytes());
    hash.update(key.x.to_le_bytes());
    hash.update(key.z.to_le_bytes());
    hash.update(candidate.to_le_bytes());
    hash.finalize().into()
}
fn fraction(bytes: &[u8]) -> f32 {
    (u32::from_le_bytes(bytes.try_into().unwrap()) >> 8) as f32 / 16777216.0
}
fn bucket(position: [f32; 3], radius: f32) -> (i64, i64) {
    (
        (position[0] / radius).floor() as i64,
        (position[2] / radius).floor() as i64,
    )
}
