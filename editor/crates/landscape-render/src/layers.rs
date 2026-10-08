use crate::*;
use scene_data::layers::{default_weight, sample_weight};
use scene_data::terrain::LayerKind;

pub(crate) fn colors(g: &CookedGround, visual: Option<&VisualPayload>) -> Vec<[f32; 4]> {
    let Some(t) = visual.and_then(|v| v.terrain.iter().find(|t| t.object == g.object)) else {
        return vec![DEFAULT_GROUND_LINEAR_RGBA; g.positions_m.len()];
    };
    let Some(group) = &t.surface_group else {
        return vec![DEFAULT_GROUND_LINEAR_RGBA; g.positions_m.len()];
    };
    let tile = t
        .tiles
        .iter()
        .find(|tile| tile.id == g.tile)
        .expect("validated visual tile reference");
    let mut members: Vec<_> = t
        .layers
        .iter()
        .filter(|l| matches!(&l.kind,LayerKind::BlendWeight {group:g,..} if g==group))
        .collect();
    members.sort_by(|a, b| a.id.cmp(&b.id));
    let n = g.cells_per_tile as usize;
    (0..g.positions_m.len())
        .map(|i| {
            let u = (i % (n + 1)) as f32 / n as f32;
            let v = (i / (n + 1)) as f32 / n as f32;
            let mut weights: Vec<_> = members
                .iter()
                .map(|l| {
                    t.derived_layers
                        .iter()
                        .find(|d| d.tile == g.tile && d.layer == l.id)
                        .map(|d| d.weights[i])
                        .or_else(|| {
                            tile.layers
                                .iter()
                                .find(|p| p.layer == l.id)
                                .map(|p| sample_weight(p, u, v))
                        })
                        .unwrap_or_else(|| default_weight(l))
                })
                .collect();
            let sum: f32 = weights.iter().sum();
            if sum > 0.0 {
                weights.iter_mut().for_each(|weight| *weight /= sum);
            } else {
                weights = members.iter().map(|l| default_weight(l)).collect();
            }
            let mut color = [0.0, 0.0, 0.0, 1.0];
            for (l, weight) in members.iter().zip(weights) {
                let base = l.color_linear_rgba.unwrap_or(DEFAULT_GROUND_LINEAR_RGBA);
                for c in 0..3 {
                    color[c] += base[c] * weight;
                }
            }
            color
        })
        .collect()
}
