use scene_data::{terrain::*, *};
fn tiles() -> SceneDocument {
    let mut t = TerrainDocument::flat(
        1,
        [1.0, 1.0],
        vec![
            (StableId::from_u128(100), TileKey { x: -1, z: -1 }),
            (StableId::from_u128(101), TileKey { x: 0, z: -1 }),
            (StableId::from_u128(102), TileKey { x: -1, z: 0 }),
            (StableId::from_u128(103), TileKey { x: 0, z: 0 }),
        ],
    )
    .unwrap();
    for s in &mut t.samples {
        s.height_m = s.key.x as f32 + 2.0 * s.key.z as f32;
    }
    let mut scene = current_map();
    scene.objects.push(ObjectRecord {
        id: StableId::from_u128(99),
        parent: None,
        sibling_order: 99,
        name: "Terrain".into(),
        local: RigidTransform::default(),
        data: ObjectData::Terrain(t),
    });
    scene
}
#[test]
fn deterministic_storage_order_and_simulation_hash_exclude_authoring_names() {
    let mut scene = tiles();
    let before = editor_cook::cook(&scene).unwrap();
    scene.objects.reverse();
    for o in &mut scene.objects {
        o.name.push('!');
        if let ObjectData::Terrain(t) = &mut o.data {
            t.samples.reverse();
            t.tiles.reverse();
        }
    }
    let after = editor_cook::cook(&scene).unwrap();
    assert_eq!(before.simulation_bytes, after.simulation_bytes);
    assert_eq!(before.visual_bytes, after.visual_bytes);
    assert_ne!(before.source_hash, after.source_hash);
    let ObjectData::Terrain(t) = &mut scene
        .objects
        .iter_mut()
        .find(|o| matches!(o.data, ObjectData::Terrain(_)))
        .unwrap()
        .data
    else {
        panic!()
    };
    t.samples[0].height_m += 0.25;
    assert_ne!(
        editor_cook::cook(&scene).unwrap().simulation_hash,
        before.simulation_hash
    );
}
#[test]
fn negative_coordinate_borders_and_four_tile_corner_have_identical_positions_normals_and_upward_triangles(
) {
    let package = editor_cook::cook(&tiles()).unwrap();
    let mut shared = std::collections::BTreeMap::new();
    for g in &package.simulation.ground {
        assert_eq!(g.triangles, vec![[0, 2, 1], [2, 3, 1]]);
        for (index, (p, n)) in g.positions_m.iter().zip(&g.normals).enumerate() {
            let key = SampleKey {
                x: g.key.x as i64 + (index % 2) as i64,
                z: g.key.z as i64 + (index / 2) as i64,
            };
            let value = (p.map(f32::to_bits), n.map(f32::to_bits));
            if let Some(old) = shared.insert(key, value) {
                assert_eq!(old, value);
            }
            assert!(n[1] > 0.0);
            assert_eq!(
                *p,
                [
                    key.x as f32,
                    key.x as f32 + 2.0 * key.z as f32,
                    key.z as f32
                ]
            );
        }
    }
    assert_eq!(shared.len(), 9);
    let decoded = decode_simulation(&package.simulation_bytes, package.simulation_hash).unwrap();
    assert_eq!(decoded, package.simulation);
}
#[test]
fn decorative_registry_changes_only_visual_hash_and_cannot_claim_a_simulation_toggle() {
    let mut scene = tiles();
    let before = editor_cook::cook(&scene).unwrap();
    let ObjectData::Terrain(t) = &mut scene.objects.last_mut().unwrap().data else {
        panic!()
    };
    t.layers.push(LayerDefinition {
        id: StableId::from_u128(500),
        name: "Exclusion".into(),
        kind: LayerKind::Mask,
        color_linear_rgba: None,
    });
    assert!(!t.layers[0].affects_simulation());
    let after = editor_cook::cook(&scene).unwrap();
    assert_eq!(before.simulation_hash, after.simulation_hash);
    assert_ne!(before.visual_hash, after.visual_hash);
    let mut value = serde_json::to_value(&scene).unwrap();
    value["objects"][22]["data"]["layers"][0]["affects_simulation"] = serde_json::json!(false);
    assert!(serde_json::from_value::<SceneDocument>(value).is_err());
}
#[test]
fn corrupt_truncated_and_unknown_runtime_schema_fail_closed() {
    let package = editor_cook::cook(&current_map()).unwrap();
    let mut corrupt = package.simulation_bytes.clone();
    corrupt[0] ^= 1;
    assert_eq!(
        decode_simulation(&corrupt, package.simulation_hash)
            .unwrap_err()
            .code,
        "payload.hash_mismatch"
    );
    let truncated = &package.simulation_bytes[..20];
    assert_eq!(
        decode_simulation(truncated, payload_hash(truncated))
            .unwrap_err()
            .code,
        "payload.decode"
    );
    let mut p = package.simulation.clone();
    p.schema += 1;
    let bytes = serde_json::to_vec(&p).unwrap();
    assert_eq!(
        decode_simulation(&bytes, payload_hash(&bytes))
            .unwrap_err()
            .code,
        "payload.schema_or_profile"
    );
}

#[test]
fn cancellation_stops_geometry_without_returning_a_partial_package() {
    let checks = std::cell::Cell::new(0);
    let error = editor_cook::cook_with_cancel(&tiles(), &|| {
        checks.set(checks.get() + 1);
        if checks.get() == 10 {
            Err(Diagnostic::new("job.cancelled"))
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(error.code, "job.cancelled");
    assert_eq!(checks.get(), 10);
}

#[test]
fn runtime_decoder_checks_triangle_order_and_border_bits_even_with_matching_file_hash() {
    let package = editor_cook::cook(&tiles()).unwrap();
    let mut bad = package.simulation.clone();
    bad.ground[0].triangles[0].swap(1, 2);
    let bytes = serde_json::to_vec(&bad).unwrap();
    assert_eq!(
        decode_simulation(&bytes, payload_hash(&bytes))
            .unwrap_err()
            .code,
        "payload.topology"
    );
    let mut bad = package.simulation.clone();
    bad.ground[0].positions_m[3][1] += 1.0;
    let bytes = serde_json::to_vec(&bad).unwrap();
    assert_eq!(
        decode_simulation(&bytes, payload_hash(&bytes))
            .unwrap_err()
            .code,
        "payload.seam"
    );
}

#[test]
fn painted_layers_affect_visual_payload_and_never_collision_hash() {
    let mut scene = tiles();
    let ObjectData::Terrain(t) = &mut scene.objects.last_mut().unwrap().data else {
        panic!()
    };
    let group = StableId::from_u128(500);
    scene_data::layers::set_registry(
        t,
        vec![
            LayerDefinition {
                id: StableId::from_u128(501),
                name: "Base".into(),
                kind: LayerKind::BlendWeight {
                    group: group.clone(),
                    default_weight: 1.0,
                },
                color_linear_rgba: Some([0.2, 0.4, 0.1, 1.0]),
            },
            LayerDefinition {
                id: StableId::from_u128(502),
                name: "Rock".into(),
                kind: LayerKind::BlendWeight {
                    group: group.clone(),
                    default_weight: 0.0,
                },
                color_linear_rgba: Some([0.4, 0.4, 0.4, 1.0]),
            },
        ],
        Some(group),
        2,
    )
    .unwrap();
    let before = editor_cook::cook(&scene).unwrap();
    let ObjectData::Terrain(t) = &mut scene.objects.last_mut().unwrap().data else {
        panic!()
    };
    let tile = t.tiles[0].id.clone();
    scene_data::layers::paint(t, &tile, &StableId::from_u128(502), &[(0, 0.5)]).unwrap();
    let after = editor_cook::cook(&scene).unwrap();
    assert_eq!(before.simulation_bytes, after.simulation_bytes);
    assert_ne!(before.visual_hash, after.visual_hash);
    after.visual.validate(&after.simulation.ground).unwrap();
}

fn grass(id: u128, seed: u64) -> RuleDefinition {
    RuleDefinition {
        id: StableId::from_u128(id),
        algorithm: scene_data::rules::GRASS_SCATTER.into(),
        version: 1,
        seed,
        input_layers: vec![],
        input_rules: vec![],
        output_layer: None,
        density_layer: None,
        exclusion_layers: vec![],
        parameters: Default::default(),
    }
}
#[test]
fn slope_height_layers_are_deterministic_visual_only_and_share_tile_edges() {
    let mut scene = tiles();
    let ObjectData::Terrain(t) = &mut scene.objects.last_mut().unwrap().data else {
        panic!()
    };
    let output = StableId::from_u128(720);
    let rule = StableId::from_u128(721);
    t.layers.push(LayerDefinition {
        id: output.clone(),
        name: "Auto grass mask".into(),
        kind: LayerKind::Mask,
        color_linear_rgba: None,
    });
    t.rules.push(RuleDefinition {
        id: rule.clone(),
        algorithm: scene_data::rules::AUTO_LAYER_SLOPE_HEIGHT.into(),
        version: scene_data::rules::AUTO_LAYER_VERSION,
        seed: 0,
        input_layers: vec![],
        input_rules: vec![],
        output_layer: Some(output.clone()),
        density_layer: None,
        exclusion_layers: vec![],
        parameters: [
            ("slope_min".into(), 0.0),
            ("slope_max".into(), 1.0),
            ("slope_falloff".into(), 0.0),
            ("height_min_m".into(), -100.0),
            ("height_max_m".into(), 100.0),
            ("height_falloff_m".into(), 0.0),
        ]
        .into_iter()
        .collect(),
    });
    let before = editor_cook::cook(&scene).unwrap();
    let values: std::collections::BTreeMap<_, _> = before.visual.terrain[0]
        .derived_layers
        .iter()
        .map(|d| (d.tile.clone(), (d.side_samples, d.weights.clone())))
        .collect();
    assert_eq!(values.len(), 4);
    assert!(values.values().all(|(_, weights)| weights == &vec![1.0; 4]));
    // Each geometric seam has one deterministic shared value in each adjacent payload.
    assert_eq!(
        values[&StableId::from_u128(100)].1[1],
        values[&StableId::from_u128(101)].1[0]
    );
    assert_eq!(
        values[&StableId::from_u128(100)].1[2],
        values[&StableId::from_u128(102)].1[0]
    );
    let mut reordered = scene.clone();
    let ObjectData::Terrain(t) = &mut reordered.objects.last_mut().unwrap().data else {
        panic!()
    };
    t.tiles.reverse();
    t.samples.reverse();
    assert_eq!(
        before.visual_bytes,
        editor_cook::cook(&reordered).unwrap().visual_bytes
    );
    assert_eq!(
        before.simulation_hash,
        editor_cook::cook(&reordered).unwrap().simulation_hash
    );
    let ObjectData::Terrain(t) = &mut scene.objects.last_mut().unwrap().data else {
        panic!()
    };
    t.rules[0].parameters.insert("slope_min".into(), 0.2);
    let changed = editor_cook::cook(&scene).unwrap();
    assert_ne!(before.visual_hash, changed.visual_hash);
    assert_eq!(before.simulation_hash, changed.simulation_hash);
    let ObjectData::Terrain(t) = &mut scene.objects.last_mut().unwrap().data else {
        panic!()
    };
    t.rules[0].parameters.insert("height_min_m".into(), 50.0);
    t.rules[0].parameters.insert("height_max_m".into(), 100.0);
    let mut grass_rule = grass(722, 91);
    grass_rule.input_layers.push(output.clone());
    grass_rule.input_rules.push(rule);
    grass_rule.density_layer = Some(output);
    t.rules.push(grass_rule);
    let masked = editor_cook::cook(&scene).unwrap();
    assert!(masked.visual.terrain[0].decoration.is_empty());
    assert_eq!(before.simulation_hash, masked.simulation_hash);
}
#[test]
fn derived_grass_is_deterministic_border_safe_and_on_exact_cooked_triangles() {
    let mut scene = tiles();
    let ObjectData::Terrain(t) = &mut scene.objects.last_mut().unwrap().data else {
        panic!()
    };
    t.rules.push(grass(700, 17));
    let before = editor_cook::cook(&scene).unwrap();
    let decoration = &before.visual.terrain[0].decoration;
    assert_eq!(decoration.len(), 16);
    let ids: std::collections::BTreeSet<_> = decoration.iter().map(|d| &d.id).collect();
    assert_eq!(ids.len(), decoration.len());
    for d in decoration {
        let tile = before
            .simulation
            .ground
            .iter()
            .find(|g| g.tile == d.tile)
            .unwrap();
        let [x, y, z] = d.position_m;
        assert!(x >= tile.key.x as f32 && x < (tile.key.x + 1) as f32);
        assert!(z >= tile.key.z as f32 && z < (tile.key.z + 1) as f32);
        assert!((y - x - 2.0 * z).abs() < 0.000002);
    }
    before.visual.validate(&before.simulation.ground).unwrap();
    let ObjectData::Terrain(t) = &mut scene.objects.last_mut().unwrap().data else {
        panic!()
    };
    t.tiles.reverse();
    t.samples.reverse();
    let reordered = editor_cook::cook(&scene).unwrap();
    assert_eq!(before.visual_bytes, reordered.visual_bytes);
    let ObjectData::Terrain(t) = &mut scene.objects.last_mut().unwrap().data else {
        panic!()
    };
    t.rules[0].seed += 1;
    let reseeded = editor_cook::cook(&scene).unwrap();
    assert_eq!(before.simulation_hash, reseeded.simulation_hash);
    assert_ne!(before.visual_hash, reseeded.visual_hash);
}
#[test]
fn masks_exclude_derived_output_and_rule_graphs_reject_cycles_or_unknown_semantics() {
    let mut scene = tiles();
    let ObjectData::Terrain(t) = &mut scene.objects.last_mut().unwrap().data else {
        panic!()
    };
    let layer = StableId::from_u128(710);
    scene_data::layers::set_registry(
        t,
        vec![LayerDefinition {
            id: layer.clone(),
            name: "Exclude".into(),
            kind: LayerKind::Mask,
            color_linear_rgba: None,
        }],
        None,
        2,
    )
    .unwrap();
    for tile in &mut t.tiles {
        tile.layers[0].weights.fill(1.0);
    }
    let mut rule = grass(700, 17);
    rule.exclusion_layers.push(layer.clone());
    rule.input_layers.push(layer);
    t.rules.push(rule);
    assert!(editor_cook::cook(&scene).unwrap().visual.terrain[0]
        .decoration
        .is_empty());
    let ObjectData::Terrain(t) = &mut scene.objects.last_mut().unwrap().data else {
        panic!()
    };
    let mut second = grass(701, 18);
    second.input_rules.push(t.rules[0].id.clone());
    t.rules[0].input_rules.push(second.id.clone());
    t.rules.push(second);
    assert_eq!(editor_cook::cook(&scene).unwrap_err().code, "rule.cycle");
    let ObjectData::Terrain(t) = &mut scene.objects.last_mut().unwrap().data else {
        panic!()
    };
    t.rules[0].input_rules.clear();
    t.rules[0].version = 999;
    assert_eq!(
        editor_cook::cook(&scene).unwrap_err().code,
        "rule.unsupported"
    );
}
