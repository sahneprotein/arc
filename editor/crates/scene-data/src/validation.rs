use crate::*;
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_OBJECTS: usize = 100_000;

pub fn validate(scene: &SceneDocument) -> Result<(), Diagnostic> {
    if scene.schema != SCHEMA_VERSION {
        return Err(Diagnostic::new("schema.unsupported").with("version", scene.schema));
    }
    if !scene.id.valid() {
        return Err(Diagnostic::new("id.invalid"));
    }
    if scene.objects.len() > MAX_OBJECTS {
        return Err(Diagnostic::new("scene.limit"));
    }
    let mut ids = BTreeMap::new();
    let mut spawn_orders = BTreeSet::new();
    let mut sibling_orders = BTreeSet::new();
    let mut resources = [0u64; 5];
    for obj in &scene.objects {
        if !obj.id.valid() || ids.insert(obj.id.clone(), obj).is_some() {
            return Err(Diagnostic::new("id.invalid_or_duplicate").at(&obj.id));
        }
        if obj.name.len() > 256 || obj.name.chars().any(char::is_control) {
            return Err(Diagnostic::new("name.invalid").at(&obj.id));
        }
        if !sibling_orders.insert((obj.parent.clone(), obj.sibling_order)) {
            return Err(Diagnostic::new("hierarchy.duplicate_order").at(&obj.id));
        }
        let t = &obj.local;
        if !t
            .translation_m
            .iter()
            .chain(&t.rotation_xyzw)
            .all(|v| v.is_finite())
        {
            return Err(Diagnostic::new("transform.nonfinite").at(&obj.id));
        }
        let norm: f64 = t.rotation_xyzw.iter().map(|v| (*v as f64).powi(2)).sum();
        if (norm - 1.0).abs() > 0.0001 {
            return Err(Diagnostic::new("transform.rotation_not_unit").at(&obj.id));
        }
        match &obj.data {
            ObjectData::Box { half_extents_m }
                if !half_extents_m.iter().all(|v| v.is_finite() && *v > 0.0) =>
            {
                return Err(Diagnostic::new("box.extents").at(&obj.id))
            }
            ObjectData::Spawn { order } if !spawn_orders.insert(*order) => {
                return Err(Diagnostic::new("spawn.duplicate_order").at(&obj.id))
            }
            ObjectData::Terrain(t) => {
                crate::terrain::validate(t).map_err(|e| e.at(&obj.id))?;
                resources[0] += t.samples.len() as u64;
                resources[1] += (t.cells_per_tile as u64 + 1).pow(2) * t.tiles.len() as u64;
                resources[2] += t
                    .tiles
                    .iter()
                    .flat_map(|tile| &tile.layers)
                    .map(|l| l.weights.len() as u64)
                    .sum::<u64>();
                resources[3] += t
                    .rules
                    .iter()
                    .filter(|r| r.algorithm == crate::rules::GRASS_SCATTER)
                    .map(|r| {
                        crate::rules::setting(r, "candidates_per_cell", 4.0) as u64
                            * t.cells_per_tile as u64
                            * t.cells_per_tile as u64
                            * t.tiles.len() as u64
                    })
                    .sum::<u64>();
                resources[4] += t
                    .rules
                    .iter()
                    .filter(|r| r.algorithm == crate::rules::AUTO_LAYER_SLOPE_HEIGHT)
                    .count() as u64
                    * (t.cells_per_tile as u64 + 1).pow(2)
                    * t.tiles.len() as u64;
                if resources
                    .iter()
                    .zip([
                        1_000_000,
                        2_000_000,
                        8_000_000,
                        1_000_000,
                        crate::rules::MAX_DERIVED_LAYER_WEIGHTS,
                    ])
                    .any(|(value, limit)| *value > limit)
                {
                    return Err(Diagnostic::new("scene.resource_limit"));
                }
            }
            _ => {}
        }
    }
    for obj in &scene.objects {
        let mut seen = BTreeSet::new();
        let mut current = Some(&obj.id);
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(Diagnostic::new("hierarchy.cycle").at(&obj.id));
            }
            if seen.len() > 256 {
                return Err(Diagnostic::new("hierarchy.depth_limit").at(&obj.id));
            }
            current = ids
                .get(id)
                .ok_or_else(|| Diagnostic::new("reference.missing_parent").at(&obj.id))?
                .parent
                .as_ref();
        }
    }
    Ok(())
}
