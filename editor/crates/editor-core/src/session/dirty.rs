use super::*;

pub(crate) fn terrain_dirty(before: &SceneDocument, after: &SceneDocument) -> Arc<[TerrainDirty]> {
    let mut dirty = Vec::new();
    for o in &after.objects {
        let ObjectData::Terrain(t) = &o.data else {
            continue;
        };
        let old = before.object(&o.id).and_then(|o| match &o.data {
            ObjectData::Terrain(t) => Some(t),
            _ => None,
        });
        let heights = old.map(|t| t.heights()).unwrap_or_default();
        let changed: BTreeSet<SampleKey> = t
            .samples
            .iter()
            .filter(|s| heights.get(&s.key) != Some(&s.height_m))
            .map(|s| s.key)
            .collect();
        for tile in &t.tiles {
            let mut min = [u32::MAX; 2];
            let mut max = [0; 2];
            let old_tile = old.and_then(|t| t.tiles.iter().find(|old| old.id == tile.id));
            if old.is_none_or(|old| old.layers != t.layers || old.surface_group != t.surface_group)
            {
                min = [0, 0];
                max = [t.cells_per_tile + 1; 2];
            }
            for payload in &tile.layers {
                let previous = old_tile
                    .and_then(|old| old.layers.iter().find(|old| old.layer == payload.layer));
                if previous.is_none_or(|p| p.side_samples != payload.side_samples) {
                    min = [0, 0];
                    max = [t.cells_per_tile + 1; 2];
                    continue;
                }
                let previous = previous.unwrap();
                let k = payload.side_samples;
                for (i, value) in payload.weights.iter().enumerate() {
                    if previous.weights[i] == *value {
                        continue;
                    }
                    let coords = [i as u32 % k, i as u32 / k];
                    for axis in 0..2 {
                        let lo = coords[axis].saturating_sub(1) * t.cells_per_tile / (k - 1);
                        let hi = (coords[axis] + 1).min(k - 1) * t.cells_per_tile;
                        let hi = hi.div_ceil(k - 1) + 1;
                        min[axis] = min[axis].min(lo);
                        max[axis] = max[axis].max(hi.min(t.cells_per_tile + 1));
                    }
                }
            }
            for z in 0..=t.cells_per_tile {
                for x in 0..=t.cells_per_tile {
                    let key = t.sample_key(tile.key, x, z);
                    // Adjacent triangle normals also change; include their shared-border halo.
                    if (-1..=1).any(|dx| {
                        (-1..=1).any(|dz| {
                            changed.contains(&SampleKey {
                                x: key.x + dx,
                                z: key.z + dz,
                            })
                        })
                    }) {
                        min[0] = min[0].min(x);
                        min[1] = min[1].min(z);
                        max[0] = max[0].max(x + 1);
                        max[1] = max[1].max(z + 1);
                    }
                }
            }
            if min[0] != u32::MAX {
                dirty.push(TerrainDirty {
                    object: o.id.clone(),
                    tile: tile.id.clone(),
                    min_xz: min,
                    max_xz_exclusive: max,
                });
            }
        }
    }
    dirty.into()
}
