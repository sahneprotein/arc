use super::*;

/// Preserve the selected target; distribute remainder using canonical base peer ratios.
/// Default ratios, then lowest-ID peer, are the zero-base fallbacks. The largest
/// allocated peer (ID tie break) absorbs f32 rounding residual calculated in f64.
pub fn redistribute(
    layers: &[LayerDefinition],
    layer: &LayerId,
    before: &BTreeMap<LayerId, f32>,
    target: f32,
) -> Result<Vec<(LayerId, f32)>, Diagnostic> {
    if layers.len() > 256 {
        return Err(Diagnostic::new("layer.limit"));
    }
    if before
        .values()
        .any(|w| !w.is_finite() || !(0.0..=1.0).contains(w))
    {
        return Err(Diagnostic::new("layer.index_or_value"));
    }
    if !target.is_finite() || !(0.0..=1.0).contains(&target) {
        return Err(Diagnostic::new("layer.index_or_value"));
    }
    let selected = layers
        .iter()
        .find(|l| &l.id == layer)
        .ok_or_else(|| Diagnostic::new("layer.missing"))?;
    let LayerKind::BlendWeight { group, .. } = &selected.kind else {
        return Ok(vec![(layer.clone(), target)]);
    };
    let mut members: Vec<_> = layers
        .iter()
        .filter(|l| matches!(&l.kind,LayerKind::BlendWeight{group:g,..}if g==group))
        .collect();
    members.sort_by(|a, b| a.id.cmp(&b.id));
    if members.len() == 1 && target != 1.0 {
        return Err(Diagnostic::new("layer.single_weight"));
    }
    if before
        .get(layer)
        .copied()
        .unwrap_or_else(|| default_weight(selected))
        .to_bits()
        == target.to_bits()
    {
        return Ok(members
            .into_iter()
            .map(|l| {
                (
                    l.id.clone(),
                    before
                        .get(&l.id)
                        .copied()
                        .unwrap_or_else(|| default_weight(l)),
                )
            })
            .collect());
    }
    let peers: Vec<_> = members.iter().filter(|l| &l.id != layer).copied().collect();
    let mut ratios: Vec<f64> = peers
        .iter()
        .map(|l| {
            f64::from(
                before
                    .get(&l.id)
                    .copied()
                    .unwrap_or_else(|| default_weight(l)),
            )
        })
        .collect();
    let mut sum: f64 = ratios.iter().sum();
    if sum == 0.0 {
        ratios = peers.iter().map(|l| f64::from(default_weight(l))).collect();
        sum = ratios.iter().sum();
    }
    if sum == 0.0 && !ratios.is_empty() {
        ratios[0] = 1.0;
        sum = 1.0;
    }
    let remaining = 1.0 - f64::from(target);
    let mut weights: Vec<f32> = ratios
        .iter()
        .map(|r| (remaining * r / sum) as f32)
        .collect();
    if let Some(residual) =
        (0..weights.len()).max_by(|a, b| weights[*a].total_cmp(&weights[*b]).then_with(|| b.cmp(a)))
    {
        let others: f64 = weights
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != residual)
            .map(|(_, v)| f64::from(*v))
            .sum();
        weights[residual] = (remaining - others).clamp(0.0, 1.0) as f32;
    }
    let mut result = vec![(layer.clone(), target)];
    result.extend(
        peers
            .into_iter()
            .zip(weights)
            .map(|(l, w)| (l.id.clone(), w)),
    );
    result.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(result)
}
