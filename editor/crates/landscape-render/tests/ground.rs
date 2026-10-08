use landscape_render::*;
use scene_data::{terrain::*, *};
fn tile() -> CookedGround {
    CookedGround {
        object: StableId::from_u128(1),
        tile: StableId::from_u128(2),
        key: TileKey { x: 0, z: 0 },
        cells_per_tile: 1,
        positions_m: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
        ],
        normals: vec![[0.0, 1.0, 0.0]; 4],
        triangles: vec![[0, 2, 1], [2, 3, 1]],
    }
}
#[test]
fn renderer_preserves_collision_geometry_and_reuses_unchanged_meshes() {
    let tile = tile();
    let mut cache = GroundCache::default();
    let first = cache.update(std::slice::from_ref(&tile), &[]).unwrap();
    assert_eq!(first.tiles_rebuilt, 1);
    assert_eq!(first.prepared_bytes, 4 * 48 + 6 * 4);
    let key = (tile.object.clone(), tile.tile.clone());
    let mesh = cache.tiles()[&key].clone();
    assert_eq!(mesh.positions, tile.positions_m);
    assert_eq!(mesh.indices, vec![0, 2, 1, 2, 3, 1]);
    let repeated = cache.update(std::slice::from_ref(&tile), &[]).unwrap();
    assert_eq!(repeated.tiles_rebuilt, 0);
    assert_eq!(repeated.prepared_bytes, 0);
    assert!(std::sync::Arc::ptr_eq(&mesh, &cache.tiles()[&key]));
    cache.set_full_detail(true);
    cache.update(std::slice::from_ref(&tile), &[]).unwrap();
    assert!(std::sync::Arc::ptr_eq(&mesh, &cache.tiles()[&key]));
    let removed = cache.update(&[], &[]).unwrap();
    assert_eq!(removed.tiles_removed, 1);
    assert!(cache.tiles().is_empty());
}
#[test]
fn malformed_preview_inputs_leave_previous_render_state_intact() {
    let tile = tile();
    let mut cache = GroundCache::default();
    cache.update(std::slice::from_ref(&tile), &[]).unwrap();
    let key = (tile.object.clone(), tile.tile.clone());
    let before = cache.tiles()[&key].clone();
    let dirty = DirtyRegion {
        object: tile.object.clone(),
        tile: tile.tile.clone(),
        min_xz: [0, 0],
        max_xz_exclusive: [3, 2],
    };
    assert_eq!(
        cache
            .update(std::slice::from_ref(&tile), &[dirty])
            .unwrap_err()
            .code,
        "render.dirty_bounds"
    );
    assert!(std::sync::Arc::ptr_eq(&before, &cache.tiles()[&key]));
}

#[test]
fn both_consumers_blend_the_same_authored_layer_colors_without_changing_triangles() {
    let tile = tile();
    let group = StableId::from_u128(100);
    let base = StableId::from_u128(10);
    let paint = StableId::from_u128(11);
    let visual = VisualPayload {
        schema: COOK_SCHEMA,
        terrain: vec![VisualTerrain {
            rules: vec![RuleDefinition {
                id: StableId::from_u128(101),
                algorithm: scene_data::rules::AUTO_LAYER_SLOPE_HEIGHT.into(),
                version: scene_data::rules::AUTO_LAYER_VERSION,
                seed: 0,
                input_layers: Vec::new(),
                input_rules: Vec::new(),
                output_layer: Some(paint.clone()),
                density_layer: None,
                exclusion_layers: Vec::new(),
                parameters: Default::default(),
            }],
            decoration: Vec::new(),
            derived_layers: vec![DerivedTerrainLayer {
                rule: StableId::from_u128(101),
                layer: paint.clone(),
                tile: tile.tile.clone(),
                side_samples: 2,
                weights: vec![0.75; 4],
            }],
            object: tile.object.clone(),
            surface_group: Some(group.clone()),
            layers: vec![
                LayerDefinition {
                    id: base.clone(),
                    name: "Red".into(),
                    kind: LayerKind::BlendWeight {
                        group: group.clone(),
                        default_weight: 1.0,
                    },
                    color_linear_rgba: Some([1.0, 0.0, 0.0, 1.0]),
                },
                LayerDefinition {
                    id: paint.clone(),
                    name: "Blue".into(),
                    kind: LayerKind::BlendWeight {
                        group,
                        default_weight: 0.0,
                    },
                    color_linear_rgba: Some([0.0, 0.0, 1.0, 1.0]),
                },
            ],
            tiles: vec![TerrainTile {
                id: tile.tile.clone(),
                key: tile.key,
                layers: vec![
                    TileLayer {
                        layer: base,
                        side_samples: 2,
                        weights: vec![0.75; 4],
                    },
                    TileLayer {
                        layer: paint,
                        side_samples: 2,
                        weights: vec![0.25; 4],
                    },
                ],
            }],
        }],
    };
    let mut game = GroundCache::default();
    let mut editor = GroundCache::default();
    game.update_with_visual(std::slice::from_ref(&tile), Some(&visual), &[])
        .unwrap();
    editor
        .update_with_visual(std::slice::from_ref(&tile), Some(&visual), &[])
        .unwrap();
    assert_eq!(game.tiles(), editor.tiles());
    let mesh = game.tiles().values().next().unwrap();
    assert_eq!(mesh.colors, vec![[0.5, 0.0, 0.5, 1.0]; 4]);
    assert_eq!(mesh.indices, vec![0, 2, 1, 2, 3, 1]);
}
