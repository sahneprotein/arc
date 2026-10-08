//! Visual layer semantics. Blend groups normalize; masks remain independent.
use crate::{terrain::*, *};
use std::collections::{BTreeMap, BTreeSet};
mod registry;
pub use registry::{resample_layers, set_registry};
mod redistribution;
pub use redistribution::redistribute;

pub fn validate_layers(
    layers: &[LayerDefinition],
    tiles: &[TerrainTile],
    surface_group: Option<&StableId>,
) -> Result<(), Diagnostic> {
    if layers.len() > 256 {
        return Err(Diagnostic::new("layer.limit"));
    }
    let mut ids = BTreeMap::new();
    let mut groups: BTreeMap<&StableId, Vec<&LayerDefinition>> = BTreeMap::new();
    for l in layers {
        if !l.id.valid() || ids.insert(&l.id, l).is_some() {
            return Err(Diagnostic::new("layer.duplicate"));
        }
        if l.name.len() > 256 || l.name.chars().any(char::is_control) {
            return Err(Diagnostic::new("name.invalid"));
        }
        if let Some(color) = l.color_linear_rgba
            && (!color
                .iter()
                .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
                || color[3] != 1.0
                || matches!(l.kind, LayerKind::Mask))
        {
            return Err(Diagnostic::new("layer.color"));
        }
        if let LayerKind::BlendWeight {
            group,
            default_weight,
        } = &l.kind
        {
            if !group.valid()
                || !default_weight.is_finite()
                || !(0.0..=1.0).contains(default_weight)
            {
                return Err(Diagnostic::new("layer.default"));
            }
            groups.entry(group).or_default().push(l);
        }
    }
    match surface_group {
        Some(group) if !groups.contains_key(group) => {
            return Err(Diagnostic::new("layer.surface_group_missing"))
        }
        None if !groups.is_empty() => return Err(Diagnostic::new("layer.surface_group_required")),
        _ => {}
    }
    for group in groups.values_mut() {
        group.sort_by(|a, b| a.id.cmp(&b.id));
        if group.iter().map(|l| default_weight(l)).sum::<f32>() != 1.0
            || (group.iter().map(|l| default_weight(l) as f64).sum::<f64>() - 1.0).abs() > 0.00001
        {
            return Err(Diagnostic::new("layer.defaults_not_normalized"));
        }
    }
    let mut total = 0usize;
    for tile in tiles {
        let mut payload = BTreeMap::new();
        for p in &tile.layers {
            total = total.saturating_add(p.weights.len());
            if total > 8_000_000 || !(2..=1025).contains(&p.side_samples) {
                return Err(Diagnostic::new("layer.limit"));
            }
            if !ids.contains_key(&p.layer)
                || payload.insert(&p.layer, p).is_some()
                || p.weights.len() != p.side_samples as usize * p.side_samples as usize
                || !p
                    .weights
                    .iter()
                    .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
            {
                return Err(Diagnostic::new("layer.invalid_payload"));
            }
        }
        for members in groups.values() {
            let sizes: BTreeSet<_> = members
                .iter()
                .filter_map(|l| payload.get(&l.id).map(|p| p.side_samples))
                .collect();
            if sizes.len() > 1 {
                return Err(Diagnostic::new("layer.group_resolution"));
            }
            let side = sizes.first().copied().unwrap_or(2) as usize;
            for index in 0..side * side {
                let sum: f64 = members
                    .iter()
                    .map(|l| {
                        payload
                            .get(&l.id)
                            .map_or(default_weight(l), |p| p.weights[index])
                            as f64
                    })
                    .sum();
                if (sum - 1.0).abs() > 0.00001 {
                    return Err(Diagnostic::new("layer.weights_not_normalized"));
                }
            }
        }
    }
    Ok(())
}
pub fn default_weight(layer: &LayerDefinition) -> f32 {
    match layer.kind {
        LayerKind::BlendWeight { default_weight, .. } => default_weight,
        LayerKind::Mask => 0.0,
    }
}
/// Bilinear sample in normalized tile coordinates; fixed row-major (z then x).
pub fn sample_weight(payload: &TileLayer, u: f32, v: f32) -> f32 {
    let n = payload.side_samples as usize;
    let x = u.clamp(0.0, 1.0) * (n - 1) as f32;
    let z = v.clamp(0.0, 1.0) * (n - 1) as f32;
    let x0 = x as usize;
    let z0 = z as usize;
    let x1 = (x0 + 1).min(n - 1);
    let z1 = (z0 + 1).min(n - 1);
    let dx = x - x0 as f32;
    let dz = z - z0 as f32;
    let a = payload.weights[z0 * n + x0] * (1.0 - dx) + payload.weights[z0 * n + x1] * dx;
    let b = payload.weights[z1 * n + x0] * (1.0 - dx) + payload.weights[z1 * n + x1] * dx;
    a * (1.0 - dz) + b * dz
}
/// Paint one layer, redistributing only its blend-group peers, never masks/other groups.
pub fn paint(
    t: &mut TerrainDocument,
    tile: &TileId,
    layer: &LayerId,
    values: &[(usize, f32)],
) -> Result<(), Diagnostic> {
    validate_layers(&t.layers, &t.tiles, t.surface_group.as_ref())?;
    let definition = t
        .layers
        .iter()
        .find(|l| &l.id == layer)
        .ok_or_else(|| Diagnostic::new("layer.missing"))?;
    let derived: BTreeSet<_> = t
        .rules
        .iter()
        .filter_map(|r| r.output_layer.as_ref())
        .collect();
    if derived.contains(layer) {
        return Err(Diagnostic::new("layer.derived_output").at(layer));
    }
    let tile_index = t
        .tiles
        .iter()
        .position(|p| &p.id == tile)
        .ok_or_else(|| Diagnostic::new("terrain.tile_missing"))?;
    let old = &t.tiles[tile_index];
    let target = old
        .layers
        .iter()
        .find(|p| &p.layer == layer)
        .ok_or_else(|| Diagnostic::new("layer.missing_payload"))?;
    for (index, value) in values {
        if *index >= target.weights.len() || !value.is_finite() || !(0.0..=1.0).contains(value) {
            return Err(Diagnostic::new("layer.index_or_value"));
        }
    }
    let members:Vec<_>=t.layers.iter().filter(|l|&l.id==layer || matches!((&definition.kind,&l.kind),(LayerKind::BlendWeight{group:a,..},LayerKind::BlendWeight{group:b,..})if a==b)).collect();
    if members
        .iter()
        .any(|l| !old.layers.iter().any(|p| p.layer == l.id))
    {
        return Err(Diagnostic::new("layer.missing_payload"));
    }
    let mut next = old.clone();
    for (index, value) in values {
        let before: BTreeMap<_, _> = members
            .iter()
            .map(|l| {
                (
                    l.id.clone(),
                    next.layers
                        .iter()
                        .find(|p| p.layer == l.id)
                        .unwrap()
                        .weights[*index],
                )
            })
            .collect();
        for (id, weight) in redistribute(&t.layers, layer, &before, *value)? {
            if derived.contains(&id) && before[&id].to_bits() != weight.to_bits() {
                return Err(Diagnostic::new("layer.derived_output").at(&id));
            }
            next.layers
                .iter_mut()
                .find(|p| p.layer == id)
                .unwrap()
                .weights[*index] = weight;
        }
    }
    t.tiles[tile_index] = next;
    Ok(())
}
