use glam::Vec3;
use scene_data::{terrain::*, *};
use std::collections::BTreeMap;

pub(crate) fn cook_ground(
    object: &ObjectId,
    t: &TerrainDocument,
    transform: &RigidTransform,
    check: &dyn Fn() -> Result<(), Diagnostic>,
) -> Result<Vec<CookedGround>, Diagnostic> {
    let heights = t.heights();
    let n = t.cells_per_tile;
    // Compute every seam position once, from the GLOBAL key, then share exact bits.
    let positions: BTreeMap<_, _> = heights
        .iter()
        .map(|(key, y)| {
            (
                *key,
                Vec3::from_array(sample_world_position(*key, *y, t.spacing_xz_m, transform)),
            )
        })
        .collect();
    if positions.values().any(|p| !p.is_finite()) {
        return Err(Diagnostic::new("cook.geometry_nonfinite").at(object));
    }
    let mut tiles: Vec<_> = t.tiles.iter().collect();
    tiles.sort_by_key(|tile| tile.key);
    let mut normal_sums: BTreeMap<SampleKey, Vec3> = BTreeMap::new();
    let mut cooked = Vec::new();
    for tile in tiles {
        check()?;
        let mut keys = Vec::new();
        for z in 0..=n {
            for x in 0..=n {
                keys.push(t.sample_key(tile.key, x, z));
            }
        }
        let mut triangles = Vec::new();
        for z in 0..n {
            check()?;
            for x in 0..n {
                for tri in cell_triangles(n, x, z).expect("validated terrain cell") {
                    let [a, b, c] = tri.map(|i| positions[&keys[i as usize]]);
                    let normal = (b - a).cross(c - a);
                    if !normal.is_finite() || normal.length_squared() <= f32::MIN_POSITIVE {
                        return Err(Diagnostic::new("cook.degenerate_triangle").at(object));
                    }
                    for i in tri {
                        *normal_sums.entry(keys[i as usize]).or_insert(Vec3::ZERO) += normal;
                    }
                    triangles.push(tri);
                }
            }
        }
        cooked.push((tile, keys, triangles));
    }
    let mut result = Vec::new();
    for (tile, keys, triangles) in cooked {
        let normals: Vec<_> = keys
            .iter()
            .map(|key| normal_sums[key].normalize().to_array())
            .collect();
        if !normals.iter().flatten().all(|v| v.is_finite()) {
            return Err(Diagnostic::new("cook.normal_nonfinite").at(object));
        }
        result.push(CookedGround {
            object: object.clone(),
            tile: tile.id.clone(),
            key: tile.key,
            cells_per_tile: n,
            positions_m: keys.iter().map(|key| positions[key].to_array()).collect(),
            normals,
            triangles,
        });
    }
    result.sort_by(|a, b| a.tile.cmp(&b.tile));
    Ok(result)
}
