use editor_cook::*;
use scene_data::{rules::*, terrain::*, *};
use std::{cell::Cell, sync::Arc};

fn scene(cells: u32, rules: bool) -> SceneDocument {
    let mut scene = current_map();
    let tiles = (-3..3)
        .flat_map(|z| {
            (-3..3).map(move |x| {
                (
                    StableId::from_u128(10_000 + ((z + 3) * 6 + x + 3) as u128),
                    TileKey { x, z },
                )
            })
        })
        .collect();
    let mut t = TerrainDocument::flat(cells, [0.75, 1.25], tiles).unwrap();
    for s in &mut t.samples {
        s.height_m = ((s.key.x * 17 + s.key.z * 31).rem_euclid(9) as f32 - 4.0) * 0.03;
    }
    if rules {
        let input = StableId::from_u128(20_000);
        let layer = StableId::from_u128(20_001);
        let second = StableId::from_u128(20_002);
        for id in [&input, &layer, &second] {
            t.layers.push(LayerDefinition {
                id: id.clone(),
                name: "mask".into(),
                kind: LayerKind::Mask,
                color_linear_rgba: None,
            });
        }
        for (i, tile) in t.tiles.iter_mut().enumerate() {
            let side = if i % 2 == 0 { 2 } else { 3 };
            tile.layers.push(TileLayer {
                layer: input.clone(),
                side_samples: side,
                weights: (0..side * side)
                    .map(|x| 0.3 + x as f32 / (side * side) as f32 * 0.7)
                    .collect(),
            });
        }
        let first_rule = StableId::from_u128(30_000);
        t.rules.push(RuleDefinition {
            id: first_rule.clone(),
            algorithm: AUTO_LAYER_SLOPE_HEIGHT.into(),
            version: 1,
            seed: 0,
            input_layers: vec![input],
            input_rules: vec![],
            output_layer: Some(layer.clone()),
            density_layer: None,
            exclusion_layers: vec![],
            parameters: [
                ("slope_falloff".into(), 0.0),
                ("height_min_m".into(), -1.0),
                ("height_max_m".into(), 2.0),
            ]
            .into_iter()
            .collect(),
        });
        t.rules.push(RuleDefinition {
            id: StableId::from_u128(30_001),
            algorithm: AUTO_LAYER_SLOPE_HEIGHT.into(),
            version: 1,
            seed: 0,
            input_layers: vec![layer.clone()],
            input_rules: vec![first_rule],
            output_layer: Some(second.clone()),
            density_layer: None,
            exclusion_layers: vec![],
            parameters: Default::default(),
        });
        let scatter = StableId::from_u128(30_002);
        t.rules.push(RuleDefinition {
            id: scatter.clone(),
            algorithm: GRASS_SCATTER.into(),
            version: 1,
            seed: 77,
            input_layers: vec![second.clone()],
            input_rules: vec![StableId::from_u128(30_001)],
            output_layer: None,
            density_layer: Some(second),
            exclusion_layers: vec![],
            parameters: [
                ("density".into(), 0.35),
                ("candidates_per_cell".into(), 1.0),
            ]
            .into_iter()
            .collect(),
        });
        // Exclusions can propagate across tiles; they must keep full deterministic evaluation.
        t.rules.push(RuleDefinition {
            id: StableId::from_u128(30_003),
            algorithm: GRASS_SCATTER.into(),
            version: 1,
            seed: 78,
            input_layers: vec![layer.clone()],
            input_rules: vec![scatter, StableId::from_u128(30_000)],
            output_layer: None,
            density_layer: Some(layer),
            exclusion_layers: vec![],
            parameters: [
                ("density".into(), 0.5),
                ("candidates_per_cell".into(), 1.0),
                ("exclusion_distance_m".into(), 1.75),
            ]
            .into_iter()
            .collect(),
        });
    }
    scene.objects.push(ObjectRecord {
        id: StableId::from_u128(9000),
        parent: None,
        sibling_order: 9000,
        name: "terrain".into(),
        local: RigidTransform {
            translation_m: [2.0, 5.0, -7.0],
            rotation_xyzw: glam::Quat::from_rotation_z(0.2).to_array(),
        },
        data: ObjectData::Terrain(t),
    });
    scene
}
fn terrain(s: &mut SceneDocument) -> &mut TerrainDocument {
    let ObjectData::Terrain(t) = &mut s.objects.last_mut().unwrap().data else {
        panic!()
    };
    t
}
fn assert_parity(cache: &mut GeometryCache, source: &SceneDocument) -> GeometryPackage {
    let actual = cache.cook(Arc::new(source.clone()), &|| Ok(())).unwrap();
    let full = cook(source).unwrap();
    // JSON bytes also catch signed-zero and ordering mistakes that float PartialEq misses.
    assert_eq!(
        serde_json::to_vec(actual.simulation.as_ref()).unwrap(),
        full.simulation_bytes
    );
    assert_eq!(
        serde_json::to_vec(actual.visual.as_ref()).unwrap(),
        full.visual_bytes
    );
    assert_eq!(actual.timings.encoding_us, 0);
    actual
}
#[test]
fn incremental_triangles_normals_layers_and_exclusion_rules_equal_full_cook_at_borders() {
    for cells in [1, 4, 8] {
        let mut source = scene(cells, true);
        let mut cache = GeometryCache::default();
        assert_parity(&mut cache, &source);
        for key in [
            SampleKey { x: 0, z: 0 },
            SampleKey { x: -1, z: 1 },
            SampleKey {
                x: cells as i64,
                z: cells as i64,
            },
            SampleKey {
                x: -3 * cells as i64,
                z: 0,
            },
            SampleKey { x: 2, z: 2 },
        ] {
            terrain(&mut source)
                .samples
                .iter_mut()
                .find(|s| s.key == key)
                .unwrap()
                .height_m += 0.625;
            let actual = assert_parity(&mut cache, &source);
            assert_eq!(actual.work.changed_samples, 1);
            assert!(actual.work.rebuilt_tiles <= 16);
            assert!(actual.work.derived_samples_evaluated <= 18);
            assert_eq!(
                actual.work.scatter_cells_visited,
                2 * 36 * cells as usize * cells as usize
            );
        }
        let repeated = assert_parity(&mut cache, &source);
        assert_eq!(repeated.work.rebuilt_tiles, 0);
        assert_eq!(repeated.work.reused_tiles, 36);
        assert_eq!(repeated.work.derived_samples_evaluated, 0);
        assert_eq!(repeated.work.scatter_cells_visited, 0);
    }
}
#[test]
fn cancelled_or_invalid_update_does_not_poison_cache_and_clear_discards_it() {
    let source = scene(4, true);
    let mut cache = GeometryCache::default();
    assert_parity(&mut cache, &source);
    let mut edited = source.clone();
    terrain(&mut edited).samples[20].height_m += 1.0;
    for stop in [12, 120, 350, 600] {
        let steps = Cell::new(0);
        let error = cache
            .cook(Arc::new(edited.clone()), &|| {
                steps.set(steps.get() + 1);
                if steps.get() == stop {
                    Err(Diagnostic::new("job.cancelled"))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert_eq!(error.code, "job.cancelled");
    }
    let mut bad = edited.clone();
    terrain(&mut bad).samples[0].height_m = f32::NAN;
    assert!(cache.cook(Arc::new(bad), &|| Ok(())).is_err());
    assert_eq!(assert_parity(&mut cache, &edited).work.changed_samples, 1);
    cache.clear();
    assert_eq!(assert_parity(&mut cache, &edited).work.rebuilt_tiles, 36);
}
#[test]
fn parent_layout_changes_fall_back_but_material_rules_reuse_exact_collision() {
    let mut source = scene(4, true);
    let mut cache = GeometryCache::default();
    assert_parity(&mut cache, &source);
    let parent = StableId::from_u128(9500);
    source.objects.last_mut().unwrap().parent = Some(parent.clone());
    source.objects.push(ObjectRecord {
        id: parent,
        parent: None,
        sibling_order: 9500,
        name: "parent".into(),
        local: RigidTransform {
            translation_m: [-3.0, 0.25, 4.0],
            rotation_xyzw: glam::Quat::from_rotation_y(0.4).to_array(),
        },
        data: ObjectData::Group,
    });
    assert_eq!(assert_parity(&mut cache, &source).work.rebuilt_tiles, 36);
    source.objects.pop();
    source.objects.last_mut().unwrap().parent = None;
    assert_parity(&mut cache, &source);
    terrain(&mut source).tiles[0].layers[0].weights[0] = 0.125;
    assert_eq!(assert_parity(&mut cache, &source).work.rebuilt_tiles, 0);
    terrain(&mut source).rules[0]
        .parameters
        .insert("height_max_m".into(), 1.0);
    assert_eq!(assert_parity(&mut cache, &source).work.rebuilt_tiles, 0);
    terrain(&mut source).tiles.pop();
    assert_eq!(assert_parity(&mut cache, &source).work.rebuilt_tiles, 35);
    let t = terrain(&mut source);
    t.tiles.reverse();
    t.samples.reverse();
    t.layers.reverse();
    t.rules.reverse();
    assert_eq!(assert_parity(&mut cache, &source).work.rebuilt_tiles, 35);
}
#[test]
fn signed_zero_and_extreme_unused_sample_keys_do_not_reuse_wrong_bits_or_overflow() {
    let mut source = scene(4, false);
    let mut cache = GeometryCache::default();
    assert_parity(&mut cache, &source);
    let t = terrain(&mut source);
    t.samples.push(HeightSample {
        key: SampleKey {
            x: i64::MAX,
            z: i64::MIN,
        },
        height_m: 0.0,
    });
    assert_parity(&mut cache, &source);
    terrain(&mut source).samples.last_mut().unwrap().height_m = -0.0;
    assert_eq!(assert_parity(&mut cache, &source).work.changed_samples, 1);
    source.objects[0].local.translation_m[2] = -0.0;
    assert_parity(&mut cache, &source);
}

#[test]
fn unrelated_terrain_is_reused_and_bulk_height_changes_use_bounded_full_fallback() {
    let mut source = scene(4, true);
    let mut other = source.objects.last().unwrap().clone();
    other.id = StableId::from_u128(9001);
    other.sibling_order = 9001;
    source.objects.push(other);
    let mut cache = GeometryCache::default();
    assert_parity(&mut cache, &source);
    let index = source.objects.len() - 2;
    let ObjectData::Terrain(t) = &mut source.objects[index].data else {
        panic!()
    };
    t.samples[20].height_m += 1.0;
    let actual = assert_parity(&mut cache, &source);
    assert!(actual.work.reused_tiles >= 36);
    assert!(actual.work.rebuilt_tiles < 36);
    let mut bulk = current_map();
    let t = TerrainDocument::flat(
        256,
        [1.0, 1.0],
        vec![(StableId::from_u128(10001), TileKey { x: 0, z: 0 })],
    )
    .unwrap();
    bulk.objects.push(ObjectRecord {
        id: StableId::from_u128(9000),
        parent: None,
        sibling_order: 9000,
        name: "bulk".into(),
        local: Default::default(),
        data: ObjectData::Terrain(t),
    });
    let mut cache = GeometryCache::default();
    assert_parity(&mut cache, &bulk);
    for s in &mut terrain(&mut bulk).samples {
        s.height_m = 1.0;
    }
    let actual = assert_parity(&mut cache, &bulk);
    assert_eq!(actual.work.rebuilt_tiles, 1);
    assert_eq!(actual.work.reused_tiles, 0);
    assert_eq!(actual.work.changed_samples, 0);
}
