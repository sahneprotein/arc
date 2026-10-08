use scene_data::{rules::*, terrain::*, *};

fn mask(id: u128) -> LayerDefinition {
    LayerDefinition {
        id: StableId::from_u128(id),
        name: "Mask".into(),
        kind: LayerKind::Mask,
        color_linear_rgba: None,
    }
}
fn rule(id: u128, output: LayerId) -> RuleDefinition {
    RuleDefinition {
        id: StableId::from_u128(id),
        algorithm: AUTO_LAYER_SLOPE_HEIGHT.into(),
        version: AUTO_LAYER_VERSION,
        seed: 0,
        input_layers: vec![],
        input_rules: vec![],
        output_layer: Some(output),
        density_layer: None,
        exclusion_layers: vec![],
        parameters: [
            ("slope_falloff".into(), 0.0),
            ("height_falloff_m".into(), 0.0),
        ]
        .into_iter()
        .collect(),
    }
}
fn scene(cells: u32) -> SceneDocument {
    let mut scene = current_map();
    scene.objects.push(ObjectRecord {
        id: StableId::from_u128(1001),
        parent: None,
        sibling_order: 1001,
        name: "Terrain".into(),
        local: Default::default(),
        data: ObjectData::Terrain(
            TerrainDocument::flat(
                cells,
                [1.0, 1.0],
                vec![
                    (StableId::from_u128(1002), TileKey { x: 0, z: 0 }),
                    (StableId::from_u128(1003), TileKey { x: 1, z: 0 }),
                ],
            )
            .unwrap(),
        ),
    });
    scene
}
fn terrain(scene: &mut SceneDocument) -> &mut TerrainDocument {
    let ObjectData::Terrain(t) = &mut scene.objects.last_mut().unwrap().data else {
        panic!()
    };
    t
}

#[test]
fn mixed_resolution_inputs_and_chained_rules_share_canonical_border_values() {
    let mut source = scene(2);
    let input = mask(2001);
    let output = mask(2002);
    let chained = mask(2003);
    let t = terrain(&mut source);
    t.layers = vec![input.clone(), output.clone(), chained.clone()];
    t.tiles[0].layers.push(TileLayer {
        layer: input.id.clone(),
        side_samples: 2,
        weights: vec![0.0, 1.0, 0.0, 1.0],
    });
    t.tiles[1].layers.push(TileLayer {
        layer: input.id.clone(),
        side_samples: 3,
        weights: vec![0.0; 9],
    });
    let baseline = editor_cook::cook(&source).unwrap();
    let mut producer = rule(3002, output.id.clone());
    producer.input_layers.push(input.id);
    let mut consumer = rule(3001, chained.id.clone());
    consumer.input_rules.push(producer.id.clone());
    consumer.input_layers.push(output.id.clone());
    terrain(&mut source).rules = vec![consumer.clone(), producer.clone()];
    let unchanged = source.clone();
    let cooked = editor_cook::cook(&source).unwrap();
    assert_eq!(source, unchanged);
    assert_eq!(baseline.simulation_hash, cooked.simulation_hash);
    let derived = &cooked.visual.terrain[0].derived_layers;
    let lookup = |layer: &LayerId, tile: u128| {
        &derived
            .iter()
            .find(|d| &d.layer == layer && d.tile == StableId::from_u128(tile))
            .unwrap()
            .weights
    };
    assert_eq!(
        lookup(&output.id, 1002),
        &vec![0.0, 0.5, 1.0, 0.0, 0.5, 1.0, 0.0, 0.5, 1.0]
    );
    assert_eq!(
        lookup(&output.id, 1003),
        &vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
    );
    assert_eq!(lookup(&output.id, 1002), lookup(&chained.id, 1002));
    assert_eq!(lookup(&output.id, 1003), lookup(&chained.id, 1003));
    let t = terrain(&mut source);
    t.tiles.reverse();
    t.samples.reverse();
    t.rules.reverse();
    t.layers.reverse();
    assert_eq!(
        cooked.visual_bytes,
        editor_cook::cook(&source).unwrap().visual_bytes
    );

    let mut corrupt = cooked.visual.clone();
    let right = corrupt.terrain[0]
        .derived_layers
        .iter_mut()
        .find(|d| d.layer == output.id && d.tile == StableId::from_u128(1003))
        .unwrap();
    right.weights[0] = 0.25;
    assert_eq!(
        corrupt
            .validate(&cooked.simulation.ground)
            .unwrap_err()
            .code,
        "payload.derived_layer_seam"
    );
    let t = terrain(&mut source);
    t.rules
        .iter_mut()
        .find(|r| r.id == consumer.id)
        .unwrap()
        .input_rules
        .clear();
    assert_eq!(
        editor_cook::cook(&source).unwrap_err().code,
        "rule.dependency_missing"
    );
}

#[test]
fn slope_and_height_bands_multiply_with_numeric_expected_weights() {
    let mut source = scene(2);
    let t = terrain(&mut source);
    for sample in &mut t.samples {
        sample.height_m = sample.key.x as f32 * 0.75;
    }
    let output = mask(2002);
    let mut definition = rule(3002, output.id.clone());
    definition.parameters.extend([
        ("slope_min".into(), 0.1),
        ("slope_max".into(), 0.5),
        ("slope_falloff".into(), 0.2),
        ("height_min_m".into(), 0.0),
        ("height_max_m".into(), 10.0),
        ("height_falloff_m".into(), 1.0),
    ]);
    t.layers.push(output);
    t.rules.push(definition);
    let cooked = editor_cook::cook(&source).unwrap();
    let values = &cooked.visual.terrain[0]
        .derived_layers
        .iter()
        .find(|d| d.tile == StableId::from_u128(1002))
        .unwrap()
        .weights;
    assert_eq!(values[0], 0.0);
    assert!((values[1] - 0.375).abs() < 0.000001);
    assert!((values[2] - 0.5).abs() < 0.000001);
    let t = terrain(&mut source);
    t.rules[0].parameters.insert("slope_min".into(), 0.9);
    assert_eq!(
        editor_cook::cook(&source).unwrap_err().code,
        "rule.parameter_range"
    );
}

#[test]
fn scene_wide_derived_weight_budget_rejects_before_generation() {
    let mut source = scene(128);
    let t = terrain(&mut source);
    for index in 0..64u128 {
        let layer = mask(2000 + index);
        t.rules.push(rule(3000 + index, layer.id.clone()));
        t.layers.push(layer);
    }
    assert_eq!(
        editor_cook::cook(&source).unwrap_err().code,
        "scene.resource_limit"
    );
}
