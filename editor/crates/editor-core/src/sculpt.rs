//! Deterministic terrain brush geometry and height kernel, shared by preview and commit.
use crate::*;
use glam::{Quat, Vec3};
use scene_data::{
    terrain::{SampleKey, TerrainDocument},
    ObjectData,
};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const MAX_SCULPT_POINTS: usize = 256;
const MAX_SCULPT_RECT_VISITS: u64 = 1_000_000;
pub const MAX_SCULPT_CHANGED_SAMPLES: usize = 65_536;
pub const MAX_SCULPT_RADIUS_M: f32 = 10_000.0;

/// Terrain-local height of an actual world-space ground hit, for flatten targets/readouts.
/// This converts an existing hit; it does not intersect or sample terrain geometry.
pub fn terrain_hit_height_m(
    scene: &SceneDocument,
    object: &ObjectId,
    hit_world_m: [f32; 3],
) -> Result<f32, Diagnostic> {
    scene_data::validate(scene)?;
    if !matches!(
        scene.object(object).map(|o| &o.data),
        Some(ObjectData::Terrain(_))
    ) {
        return Err(Diagnostic::new("field.kind").at(object));
    }
    let transforms = scene_data::world_transforms(scene)?;
    let transform = &transforms[object];
    let point = Quat::from_array(transform.rotation_xyzw).conjugate()
        * (Vec3::from_array(hit_world_m) - Vec3::from_array(transform.translation_m));
    if !point.is_finite() {
        return Err(Diagnostic::new("terrain.brush_nonfinite").at(object));
    }
    Ok(point.y)
}

/// Compute canonical sample deltas from a complete ordered world-space brush path.
/// Output order is always ascending global SampleKey and never depends on tile storage.
pub fn terrain_sculpt(
    scene: &SceneDocument,
    request: &TerrainSculptRequest,
) -> Result<TerrainSculptResult, Diagnostic> {
    evaluate_terrain_sculpt(scene, request).map(|(result, _)| result)
}

pub(crate) fn evaluate_terrain_sculpt(
    scene: &SceneDocument,
    request: &TerrainSculptRequest,
) -> Result<(TerrainSculptResult, Option<SceneDocument>), Diagnostic> {
    scene_data::validate(scene)?;
    let terrain = scene
        .object(&request.object)
        .ok_or_else(|| Diagnostic::new("object.missing").at(&request.object))?;
    let ObjectData::Terrain(terrain) = &terrain.data else {
        return Err(Diagnostic::new("field.kind").at(&request.object));
    };
    let samples = evaluate_samples(scene, terrain, request)?;
    let mut result = TerrainSculptResult {
        object: request.object.clone(),
        sample_deltas: samples.into(),
        dirty_regions: Arc::from([]),
    };
    if result.sample_deltas.is_empty() {
        return Ok((result, None));
    }
    let mut next = scene.clone();
    mutate(&mut next, &[result.mutation()])?;
    result.dirty_regions = crate::session::dirty::terrain_dirty(scene, &next);
    Ok((result, Some(next)))
}

fn evaluate_samples(
    scene: &SceneDocument,
    terrain: &TerrainDocument,
    request: &TerrainSculptRequest,
) -> Result<Vec<TerrainSculptSample>, Diagnostic> {
    let diagnostic = || Diagnostic::new("terrain.brush_invalid").at(&request.object);
    crate::brush::validate_path(
        &request.object,
        &request.path_world_m,
        request.radius_m,
        request.strength,
        request.falloff,
    )?;
    match request.mode {
        TerrainSculptMode::Raise { amount_m } | TerrainSculptMode::Lower { amount_m }
            if !amount_m.is_finite() || !(0.0..=1_000.0).contains(&amount_m) =>
        {
            return Err(diagnostic())
        }
        TerrainSculptMode::Flatten { height_m } if !height_m.is_finite() => {
            return Err(diagnostic())
        }
        _ => {}
    }
    let points = crate::brush::local_path(scene, &request.object, &request.path_world_m)?;

    // Only lattice keys referenced by actual tiles can be edited. Extra source keys
    // cannot affect cooking and therefore are intentionally excluded.
    let heights = terrain.heights();
    let mut samples = BTreeMap::new();
    for tile in &terrain.tiles {
        for z in 0..=terrain.cells_per_tile {
            for x in 0..=terrain.cells_per_tile {
                let key = terrain.sample_key(tile.key, x, z);
                samples.insert(key, heights[&key]);
            }
        }
    }
    let Some((&first_key, _)) = samples.first_key_value() else {
        return Ok(Vec::new());
    };
    let (mut min_sample_x, mut max_sample_x, mut min_sample_z, mut max_sample_z) =
        (first_key.x, first_key.x, first_key.z, first_key.z);
    for key in samples.keys() {
        min_sample_x = min_sample_x.min(key.x);
        max_sample_x = max_sample_x.max(key.x);
        min_sample_z = min_sample_z.min(key.z);
        max_sample_z = max_sample_z.max(key.z);
    }
    let radius = f64::from(request.radius_m);
    let spacing_x = f64::from(terrain.spacing_xz_m[0]);
    let spacing_z = f64::from(terrain.spacing_xz_m[1]);
    let mut weights = BTreeMap::<SampleKey, f32>::new();
    let mut visits = 0u64;

    // A one-hit path is a disk; each successive pair otherwise forms a capsule.
    let segments: Vec<_> = if points.len() == 1 {
        vec![(points[0], points[0])]
    } else {
        points.windows(2).map(|p| (p[0], p[1])).collect()
    };
    for (start, end) in segments {
        let min_x = (f64::from(start[0]).min(f64::from(end[0])) - radius) / spacing_x;
        let max_x = (f64::from(start[0]).max(f64::from(end[0])) + radius) / spacing_x;
        let min_z = (f64::from(start[2]).min(f64::from(end[2])) - radius) / spacing_z;
        let max_z = (f64::from(start[2]).max(f64::from(end[2])) + radius) / spacing_z;
        let x0 = (min_x.floor() as i64).max(min_sample_x);
        let x1 = (max_x.ceil() as i64).min(max_sample_x);
        let z0 = (min_z.floor() as i64).max(min_sample_z);
        let z1 = (max_z.ceil() as i64).min(max_sample_z);
        if x0 > x1 || z0 > z1 {
            continue;
        }
        let area = (i128::from(x1) - i128::from(x0) + 1) as u128
            * (i128::from(z1) - i128::from(z0) + 1) as u128;
        visits = visits.saturating_add(area.min(u128::from(u64::MAX)) as u64);
        if visits > MAX_SCULPT_RECT_VISITS {
            return Err(Diagnostic::new("terrain.brush_limit").at(&request.object));
        }

        for x in x0..=x1 {
            for z in z0..=z1 {
                let key = SampleKey { x, z };
                if !samples.contains_key(&key) {
                    continue;
                }
                // Use the same f32 lattice multiplication as the cooker.
                let sx = f64::from(x as f32 * terrain.spacing_xz_m[0]);
                let sz = f64::from(z as f32 * terrain.spacing_xz_m[1]);
                let influence = crate::brush::influence(
                    start,
                    end,
                    [sx, sz],
                    radius,
                    request.falloff,
                    request.strength,
                );
                if influence > 0.0 {
                    weights
                        .entry(key)
                        .and_modify(|current| *current = current.max(influence))
                        .or_insert(influence);
                }
            }
        }
    }

    let mut result = Vec::with_capacity(weights.len().min(MAX_SCULPT_CHANGED_SAMPLES));
    for (key, weight) in weights {
        let before = samples[&key];
        let target = match request.mode {
            TerrainSculptMode::Raise { amount_m } => before + amount_m * weight,
            TerrainSculptMode::Lower { amount_m } => before - amount_m * weight,
            TerrainSculptMode::Flatten { height_m } => before + (height_m - before) * weight,
            TerrainSculptMode::Smooth => {
                let neighbours = [
                    SampleKey {
                        x: key.x - 1,
                        z: key.z,
                    },
                    SampleKey {
                        x: key.x + 1,
                        z: key.z,
                    },
                    SampleKey {
                        x: key.x,
                        z: key.z - 1,
                    },
                    SampleKey {
                        x: key.x,
                        z: key.z + 1,
                    },
                ];
                let mut sum = 0.0f64;
                let mut count = 0u32;
                for neighbour in neighbours {
                    if let Some(height) = samples.get(&neighbour) {
                        sum += f64::from(*height);
                        count += 1;
                    }
                }
                if count == 0 {
                    before
                } else {
                    (f64::from(before)
                        + (sum / f64::from(count) - f64::from(before)) * f64::from(weight))
                        as f32
                }
            }
        };
        if !target.is_finite() {
            return Err(Diagnostic::new("terrain.brush_nonfinite").at(&request.object));
        }
        if target != before {
            result.push(TerrainSculptSample {
                key,
                before_m: before,
                after_m: target,
                weight,
            });
            if result.len() > MAX_SCULPT_CHANGED_SAMPLES {
                return Err(Diagnostic::new("terrain.brush_limit").at(&request.object));
            }
        }
    }
    Ok(result)
}
