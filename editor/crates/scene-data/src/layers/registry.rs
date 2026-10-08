use super::*;

/// Ordinary registry changes retain every existing payload's resolution and bits.
/// `side` initializes missing payloads only, using existing group resolution first.
pub fn set_registry(
    t: &mut TerrainDocument,
    layers: Vec<LayerDefinition>,
    surface_group: Option<StableId>,
    side: u32,
) -> Result<(), Diagnostic> {
    validate_layers(&layers, &[], surface_group.as_ref())?;
    if !(2..=1025).contains(&side) {
        return Err(Diagnostic::new("layer.limit"));
    }
    validate_layers(&t.layers, &t.tiles, t.surface_group.as_ref())?;
    let mut definitions = layers;
    definitions.sort_by(|a, b| a.id.cmp(&b.id));
    let mut total = 0u64;
    // Count BEFORE allocating default payloads; preserve different per-tile sizes.
    for tile in &t.tiles {
        for layer in &definitions {
            let side = side_for(tile, layer, &definitions, side)?;
            total += u64::from(side).pow(2);
            if total > 8_000_000 {
                return Err(Diagnostic::new("layer.limit"));
            }
        }
    }
    let mut tiles = t.tiles.clone();
    for tile in &mut tiles {
        let old = tile.clone();
        let mut payloads = vec![];
        for layer in &definitions {
            let side = side_for(&old, layer, &definitions, side)?;
            let weights = old
                .layers
                .iter()
                .find(|p| p.layer == layer.id)
                .map(|p| p.weights.clone())
                .unwrap_or_else(|| vec![default_weight(layer); (side * side) as usize]);
            payloads.push(TileLayer {
                layer: layer.id.clone(),
                side_samples: side,
                weights,
            });
        }
        let mut groups: BTreeMap<_, Vec<usize>> = BTreeMap::new();
        for (i, l) in definitions.iter().enumerate() {
            if let LayerKind::BlendWeight { group, .. } = &l.kind {
                groups.entry(group).or_default().push(i);
            }
        }
        for (group, members) in groups {
            if members
                .iter()
                .map(|i| payloads[*i].side_samples)
                .collect::<BTreeSet<_>>()
                .len()
                > 1
            {
                return Err(Diagnostic::new("layer.group_resolution"));
            }
            let before: BTreeSet<_> = t
                .layers
                .iter()
                .filter(|l| matches!(&l.kind,LayerKind::BlendWeight{group:g,..}if g==group))
                .map(|l| &l.id)
                .collect();
            let after: BTreeSet<_> = members.iter().map(|i| &definitions[*i].id).collect();
            if before == after {
                continue;
            }
            // Registry membership edits renormalize surviving group weights, in ID order.
            for index in 0..payloads[members[0]].weights.len() {
                let sum: f64 = members
                    .iter()
                    .map(|i| f64::from(payloads[*i].weights[index]))
                    .sum();
                if sum == 0.0 {
                    for i in &members {
                        payloads[*i].weights[index] = default_weight(&definitions[*i]);
                    }
                } else {
                    for i in &members {
                        payloads[*i].weights[index] =
                            (f64::from(payloads[*i].weights[index]) / sum) as f32;
                    }
                }
            }
        }
        tile.layers = payloads;
    }
    validate_layers(&definitions, &tiles, surface_group.as_ref())?;
    t.tiles = tiles;
    t.layers = definitions;
    t.surface_group = surface_group;
    Ok(())
}
fn side_for(
    tile: &TerrainTile,
    layer: &LayerDefinition,
    definitions: &[LayerDefinition],
    initial: u32,
) -> Result<u32, Diagnostic> {
    if let Some(p) = tile.layers.iter().find(|p| p.layer == layer.id) {
        return Ok(p.side_samples);
    }
    if let LayerKind::BlendWeight { group, .. } = &layer.kind {
        let sizes: BTreeSet<_> = definitions
            .iter()
            .filter(|l| matches!(&l.kind,LayerKind::BlendWeight{group:g,..}if g==group))
            .filter_map(|l| {
                tile.layers
                    .iter()
                    .find(|p| p.layer == l.id)
                    .map(|p| p.side_samples)
            })
            .collect();
        if sizes.len() > 1 {
            return Err(Diagnostic::new("layer.group_resolution"));
        }
        if let Some(side) = sizes.first() {
            return Ok(*side);
        }
    }
    Ok(initial)
}
/// Explicit, atomic bilinear resample of all currently authored layers.
pub fn resample_layers(t: &mut TerrainDocument, side: u32) -> Result<(), Diagnostic> {
    if !(2..=1025).contains(&side)
        || u64::from(side).pow(2) * t.layers.len() as u64 * t.tiles.len() as u64 > 8_000_000
    {
        return Err(Diagnostic::new("layer.limit"));
    }
    validate_layers(&t.layers, &t.tiles, t.surface_group.as_ref())?;
    let mut tiles = t.tiles.clone();
    for tile in &mut tiles {
        let old = tile.clone();
        let mut payloads = vec![];
        for layer in &t.layers {
            let previous = old.layers.iter().find(|p| p.layer == layer.id);
            let weights = match previous {
                Some(p) if p.side_samples == side => p.weights.clone(),
                Some(p) => (0..side * side)
                    .map(|i| {
                        sample_weight(
                            p,
                            (i % side) as f32 / (side - 1) as f32,
                            (i / side) as f32 / (side - 1) as f32,
                        )
                    })
                    .collect(),
                None => vec![default_weight(layer); (side * side) as usize],
            };
            payloads.push(TileLayer {
                layer: layer.id.clone(),
                side_samples: side,
                weights,
            });
        }
        tile.layers = payloads;
    }
    validate_layers(&t.layers, &tiles, t.surface_group.as_ref())?;
    t.tiles = tiles;
    Ok(())
}
