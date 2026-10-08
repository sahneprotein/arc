use scene_data::{terrain::*, *};

pub(super) fn same_transform(a: &RigidTransform, b: &RigidTransform) -> bool {
    a.translation_m.map(f32::to_bits) == b.translation_m.map(f32::to_bits)
        && a.rotation_xyzw.map(f32::to_bits) == b.rotation_xyzw.map(f32::to_bits)
}
// Visual metadata changes invalidate derived outputs. Float bits include signed zero.
pub(super) fn same_layout(a: &TerrainDocument, b: &TerrainDocument) -> bool {
    a.cells_per_tile == b.cells_per_tile
        && a.spacing_xz_m.map(f32::to_bits) == b.spacing_xz_m.map(f32::to_bits)
        && a.surface_group == b.surface_group
        && a.tiles == b.tiles
        && a.layers == b.layers
        && a.rules == b.rules
        && a.samples.len() == b.samples.len()
        && a.samples
            .iter()
            .zip(&b.samples)
            .all(|(a, b)| a.key == b.key)
        && a.tiles.iter().zip(&b.tiles).all(|(a, b)| {
            a.layers.iter().zip(&b.layers).all(|(a, b)| {
                a.weights
                    .iter()
                    .zip(&b.weights)
                    .all(|(a, b)| a.to_bits() == b.to_bits())
            })
        })
        && a.layers.iter().zip(&b.layers).all(|(a, b)| {
            a.color_linear_rgba.map(|v| v.map(f32::to_bits))
                == b.color_linear_rgba.map(|v| v.map(f32::to_bits))
                && match (&a.kind, &b.kind) {
                    (
                        LayerKind::BlendWeight {
                            default_weight: a, ..
                        },
                        LayerKind::BlendWeight {
                            default_weight: b, ..
                        },
                    ) => a.to_bits() == b.to_bits(),
                    _ => true,
                }
        })
        && a.rules.iter().zip(&b.rules).all(|(a, b)| {
            a.parameters
                .values()
                .zip(b.parameters.values())
                .all(|(a, b)| a.to_bits() == b.to_bits())
        })
}
pub(super) fn same_geometry_layout(a: &TerrainDocument, b: &TerrainDocument) -> bool {
    a.cells_per_tile == b.cells_per_tile
        && a.spacing_xz_m.map(f32::to_bits) == b.spacing_xz_m.map(f32::to_bits)
        && a.tiles.len() == b.tiles.len()
        && a.tiles
            .iter()
            .zip(&b.tiles)
            .all(|(a, b)| a.id == b.id && a.key == b.key)
        && a.samples.len() == b.samples.len()
        && a.samples
            .iter()
            .zip(&b.samples)
            .all(|(a, b)| a.key == b.key)
}
