use crate::*;
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_OBJECTS: usize = 100_000;

pub fn validate(scene: &SceneDocument) -> Result<(), Diagnostic> {
    validation_diagnostics(scene)
        .into_iter()
        .next()
        .map_or(Ok(()), Err)
}

/// Independent scene/object errors; each terrain validator contributes its first
/// diagnostic. Bounds and invalid terrain stop dependent work, not other objects.
pub fn validation_diagnostics(scene: &SceneDocument) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    if scene.schema != SCHEMA_VERSION {
        errors.push(Diagnostic::new("schema.unsupported").with("version", scene.schema));
    }
    if !scene.id.valid() {
        errors.push(Diagnostic::new("id.invalid"));
    }
    if scene.objects.len() > MAX_OBJECTS {
        errors.push(Diagnostic::new("scene.limit"));
        return errors;
    }
    let mut ids = BTreeMap::new();
    let mut spawn_orders = BTreeSet::new();
    let mut sibling_orders = BTreeSet::new();
    let mut resources = [0u64; 5];
    for obj in &scene.objects {
        if !obj.id.valid() || ids.insert(obj.id.clone(), obj).is_some() {
            errors.push(Diagnostic::new("id.invalid_or_duplicate").at(&obj.id));
        }
        if obj.name.len() > 256 || obj.name.chars().any(char::is_control) {
            errors.push(Diagnostic::new("name.invalid").at(&obj.id));
        }
        if !sibling_orders.insert((obj.parent.clone(), obj.sibling_order)) {
            errors.push(Diagnostic::new("hierarchy.duplicate_order").at(&obj.id));
        }
        let t = &obj.local;
        if !t
            .translation_m
            .iter()
            .chain(&t.rotation_xyzw)
            .all(|v| v.is_finite())
        {
            errors.push(Diagnostic::new("transform.nonfinite").at(&obj.id));
        }
        let norm: f64 = t.rotation_xyzw.iter().map(|v| (*v as f64).powi(2)).sum();
        if (norm - 1.0).abs() > 0.0001 {
            errors.push(Diagnostic::new("transform.rotation_not_unit").at(&obj.id));
        }
        match &obj.data {
            ObjectData::Box { half_extents_m }
                if !half_extents_m.iter().all(|v| v.is_finite() && *v > 0.0) =>
            {
                errors.push(Diagnostic::new("box.extents").at(&obj.id))
            }
            ObjectData::Spawn { order } if !spawn_orders.insert(*order) => {
                errors.push(Diagnostic::new("spawn.duplicate_order").at(&obj.id))
            }
            ObjectData::Emitter { emitter_id } => {
                // An emitter's asset ID has the shape of every stable ID.
                if !StableId(emitter_id.clone()).valid() {
                    errors.push(Diagnostic::new("emitter.id_invalid").at(&obj.id));
                }
            }
            ObjectData::Terrain(t) => {
                if let Err(error) = crate::terrain::validate(t) {
                    errors.push(error.at(&obj.id));
                    continue;
                }
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
                    && !errors.iter().any(|e| e.code == "scene.resource_limit")
                {
                    errors.push(Diagnostic::new("scene.resource_limit"));
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
                errors.push(Diagnostic::new("hierarchy.cycle").at(&obj.id));
                break;
            }
            if seen.len() > 256 {
                errors.push(Diagnostic::new("hierarchy.depth_limit").at(&obj.id));
                break;
            }
            let Some(parent) = ids.get(id) else {
                errors.push(Diagnostic::new("reference.missing_parent").at(&obj.id));
                break;
            };
            current = parent.parent.as_ref();
        }
    }
    errors
}
