//! Pure placement query over the exact cooked world triangles; no render/cache state.
mod intersection;
use crate::*;
use glam::{DVec3, Vec3};
use scene_data::{terrain::*, ObjectData, TileId};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSurfaceRay {
    pub origin_world_m: [f32; 3],
    /// Need not be unit length. Normalized in f64, including tiny/large finite inputs.
    pub direction_world: [f32; 3],
    pub max_distance_m: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSurfaceHit {
    pub object: ObjectId,
    pub tile: TileId,
    pub key: TileKey,
    pub cell_xz: [u32; 2],
    /// Index into CookedGround.triangles: 2*(z*cells_per_tile+x)+half.
    pub triangle_index: u32,
    pub position_world_m: [f32; 3],
    /// Geometric face normal in authored winding, not the smoothed render normal.
    pub normal_world: [f32; 3],
    pub distance_m: f64,
}

/// Two-sided, nearest strictly positive hit within the maximum (inclusive).
/// Equal distances break by TileKey, then cooked triangle index. Miss/gap is None.
/// Source limits bound traversal to <4M triangles and at most1M cached positions.
pub fn terrain_surface_hit(
    scene: &SceneDocument,
    object: &ObjectId,
    ray: &TerrainSurfaceRay,
) -> Result<Option<TerrainSurfaceHit>, Diagnostic> {
    let origin = DVec3::from(Vec3::from_array(ray.origin_world_m));
    let direction = DVec3::from(Vec3::from_array(ray.direction_world));
    if !origin.is_finite()
        || !direction.is_finite()
        || direction.length_squared() == 0.0
        || !ray.max_distance_m.is_finite()
        || ray.max_distance_m <= 0.0
    {
        return Err(Diagnostic::new("terrain.ray_invalid").at(object));
    }
    let direction = direction.normalize();
    scene_data::validate(scene)?;
    let source = scene
        .object(object)
        .ok_or_else(|| Diagnostic::new("object.missing").at(object))?;
    let ObjectData::Terrain(t) = &source.data else {
        return Err(Diagnostic::new("field.kind").at(object));
    };
    let world = &scene_data::world_transforms(scene)?[object];
    let positions: BTreeMap<_, _> = t
        .samples
        .iter()
        .map(|s| {
            (
                s.key,
                Vec3::from_array(sample_world_position(
                    s.key,
                    s.height_m,
                    t.spacing_xz_m,
                    world,
                )),
            )
        })
        .collect();
    if positions.values().any(|p| !p.is_finite()) {
        return Err(Diagnostic::new("cook.geometry_nonfinite").at(object));
    }
    let mut tiles: Vec<_> = t.tiles.iter().collect();
    tiles.sort_by_key(|tile| tile.key);
    let mut best: Option<TerrainSurfaceHit> = None;
    for tile in tiles {
        for z in 0..t.cells_per_tile {
            for x in 0..t.cells_per_tile {
                for (half, indices) in cell_triangles(t.cells_per_tile, x, z)
                    .expect("validated terrain cell")
                    .into_iter()
                    .enumerate()
                {
                    let vertices = indices.map(|i| {
                        positions[&t.sample_key(
                            tile.key,
                            i % (t.cells_per_tile + 1),
                            i / (t.cells_per_tile + 1),
                        )]
                    });
                    let [a, b, c] = vertices;
                    let normal = (b - a).cross(c - a);
                    // Match the cooker's degenerate/overflow checks even on missed cells.
                    if !normal.is_finite() || normal.length_squared() <= f32::MIN_POSITIVE {
                        return Err(Diagnostic::new("cook.degenerate_triangle").at(object));
                    }
                    let vertices = vertices.map(DVec3::from);
                    let limit = best
                        .as_ref()
                        .map_or(ray.max_distance_m, |hit| hit.distance_m);
                    let Some(distance) = intersection::hit(origin, direction, vertices, limit)
                    else {
                        continue;
                    };
                    let triangle_index = 2 * (z * t.cells_per_tile + x) + half as u32;
                    if best.as_ref().is_some_and(|hit| {
                        distance > hit.distance_m
                            || (distance == hit.distance_m
                                && (tile.key, triangle_index) >= (hit.key, hit.triangle_index))
                    }) {
                        continue;
                    }
                    let [a, b, c] = vertices;
                    best = Some(TerrainSurfaceHit {
                        object: object.clone(),
                        tile: tile.id.clone(),
                        key: tile.key,
                        cell_xz: [x, z],
                        triangle_index,
                        position_world_m: (origin + direction * distance).as_vec3().to_array(),
                        normal_world: (b - a).cross(c - a).normalize().as_vec3().to_array(),
                        distance_m: distance,
                    });
                }
            }
        }
    }
    Ok(best)
}
