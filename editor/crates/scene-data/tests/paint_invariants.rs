use scene_data::{terrain::*, *};
use std::collections::BTreeMap;
fn id(n: u128) -> StableId {
    StableId::from_u128(n)
}
fn blend(n: u128, group: u128, default_weight: f32) -> LayerDefinition {
    LayerDefinition {
        id: id(n),
        name: format!("layer{n}"),
        kind: LayerKind::BlendWeight {
            group: id(group),
            default_weight,
        },
        color_linear_rgba: Some([1.0, 0.0, 0.0, 1.0]),
    }
}
fn mask(n: u128) -> LayerDefinition {
    LayerDefinition {
        id: id(n),
        name: format!("mask{n}"),
        kind: LayerKind::Mask,
        color_linear_rgba: None,
    }
}
fn terrain() -> TerrainDocument {
    TerrainDocument::flat(2, [1.0, 1.0], vec![(id(1), TileKey { x: 0, z: 0 })]).unwrap()
}
#[test]
fn peer_ratios_default_and_lowest_id_fallback_keep_selected_exact_and_normalized() {
    let layers = vec![
        blend(10, 100, 0.0),
        blend(11, 100, 0.25),
        blend(12, 100, 0.75),
        mask(13),
        blend(14, 101, 1.0),
    ];
    let cases = [
        ([0.2, 0.2, 0.6], 0.6, [0.6, 0.1, 0.3]),
        ([1.0, 0.0, 0.0], 0.6, [0.6, 0.1, 0.3]),
        ([0.0, 0.25, 0.75], 1.0, [1.0, 0.0, 0.0]),
    ];
    for (base, target, expected) in cases {
        let before: BTreeMap<_, _> = (10..13).zip(base).map(|(n, w)| (id(n), w)).collect();
        let actual = layers::redistribute(&layers, &id(10), &before, target).unwrap();
        assert_eq!(actual[0].1.to_bits(), target.to_bits());
        for (a, e) in actual.iter().map(|(_, w)| w).zip(expected) {
            assert!((*a - e).abs() < 1e-7);
        }
        assert!((actual.iter().map(|(_, w)| f64::from(*w)).sum::<f64>() - 1.0).abs() < 1e-7);
        let mut reversed = layers.clone();
        reversed.reverse();
        assert_eq!(
            layers::redistribute(&reversed, &id(10), &before, target).unwrap(),
            actual
        );
    }
    let defaults = vec![
        blend(10, 100, 1.0),
        blend(12, 100, 0.0),
        blend(11, 100, 0.0),
    ];
    let before = [(id(10), 1.0), (id(11), 0.0), (id(12), 0.0)].into();
    assert_eq!(
        layers::redistribute(&defaults, &id(10), &before, 0.25).unwrap(),
        vec![(id(10), 0.25), (id(11), 0.75), (id(12), 0.0)]
    );
    let many: Vec<_> = (10..110)
        .map(|n| blend(n, 100, if n == 10 { 1.0 } else { 0.0 }))
        .collect();
    let before: BTreeMap<_, _> = (10..110).map(|n| (id(n), 0.01)).collect();
    for target in [0.0, f32::MIN_POSITIVE, 0.1234567, 0.99999994, 1.0] {
        let out = layers::redistribute(&many, &id(10), &before, target).unwrap();
        assert_eq!(out[0].1.to_bits(), target.to_bits());
        assert!(out
            .iter()
            .all(|(_, w)| w.is_finite() && (0.0..=1.0).contains(w)));
        assert!((out.iter().map(|(_, w)| f64::from(*w)).sum::<f64>() - 1.0).abs() < 1e-7);
        // Tied largest peers: lowest ID absorbs the residual; every other peer rounds equally.
        assert!(out[2..]
            .windows(2)
            .all(|w| w[0].1.to_bits() == w[1].1.to_bits()));
    }
}
#[test]
fn ordinary_registry_preserves_bits_and_group_resolution_new_payloads_use_defaults() {
    let mut t = terrain();
    layers::set_registry(
        &mut t,
        vec![blend(10, 100, 1.0), blend(11, 100, 0.0), mask(12)],
        Some(id(100)),
        3,
    )
    .unwrap();
    layers::paint(&mut t, &id(1), &id(11), &[(0, 0.25), (4, 0.5)]).unwrap();
    t.tiles[0].layers[2].side_samples = 2;
    t.tiles[0].layers[2].weights = vec![-0.0, 0.2, 0.4, 0.6];
    let payloads = t.tiles[0].layers.clone();
    let mut defs = t.layers.clone();
    defs[1].name = "Renamed".into();
    defs[1].color_linear_rgba = Some([0.0, 1.0, 0.0, 1.0]);
    defs.reverse();
    layers::set_registry(&mut t, defs, Some(id(100)), 7).unwrap();
    for (a, b) in t.tiles[0].layers.iter().zip(&payloads) {
        assert_eq!(a.side_samples, b.side_samples);
        assert_eq!(
            a.weights.iter().map(|w| w.to_bits()).collect::<Vec<_>>(),
            b.weights.iter().map(|w| w.to_bits()).collect::<Vec<_>>()
        );
    }
    let mut defs = t.layers.clone();
    defs.push(blend(13, 100, 0.0));
    defs.push(mask(14));
    layers::set_registry(&mut t, defs, Some(id(100)), 7).unwrap();
    let new = &t.tiles[0].layers[3];
    assert_eq!(new.side_samples, 3);
    assert!(new.weights.iter().all(|w| *w == 0.0));
    let new = &t.tiles[0].layers[4];
    assert_eq!(new.side_samples, 7);
    assert_eq!(new.weights, vec![0.0; 49]);
    layers::resample_layers(&mut t, 5).unwrap();
    assert!(t.tiles[0]
        .layers
        .iter()
        .all(|p| p.side_samples == 5 && p.weights.len() == 25));
    scene_data::terrain::validate(&t).unwrap();
    let before = t.clone();
    layers::resample_layers(&mut t, 5).unwrap();
    assert_eq!(t, before);
}
#[test]
fn merging_incompatible_group_resolutions_is_atomic_and_does_not_index_wrong_payload() {
    let mut t = terrain();
    layers::set_registry(
        &mut t,
        vec![blend(10, 100, 1.0), blend(11, 101, 1.0)],
        Some(id(100)),
        2,
    )
    .unwrap();
    t.tiles[0].layers[1].side_samples = 3;
    t.tiles[0].layers[1].weights = vec![1.0; 9];
    let before = t.clone();
    let error = layers::set_registry(
        &mut t,
        vec![blend(10, 100, 1.0), blend(11, 100, 0.0)],
        Some(id(100)),
        2,
    )
    .unwrap_err();
    assert_eq!(error.code, "layer.group_resolution");
    assert_eq!(t, before);
    layers::resample_layers(&mut t, 3).unwrap();
    layers::set_registry(
        &mut t,
        vec![blend(10, 100, 1.0), blend(11, 100, 0.0)],
        Some(id(100)),
        2,
    )
    .unwrap();
    assert!(t.tiles[0].layers.iter().all(|p| p.weights == vec![0.5; 9]));
}
#[test]
fn invalid_defaults_colors_sizes_counts_and_payload_budget_refuse_atomically() {
    let base = terrain();
    for side in [1, 1026] {
        let mut t = base.clone();
        layers::set_registry(&mut t, vec![mask(10)], None, 2).unwrap();
        t.tiles[0].layers[0].side_samples = side;
        assert_eq!(
            scene_data::terrain::validate(&t).unwrap_err().code,
            "layer.limit"
        );
    }
    for side in [1, 1026] {
        let mut t = base.clone();
        assert_eq!(
            layers::set_registry(&mut t, vec![mask(10)], None, side)
                .unwrap_err()
                .code,
            "layer.limit"
        );
        assert_eq!(t, base);
    }
    for (defs, code) in [
        ((10..267).map(mask).collect::<Vec<_>>(), "layer.limit"),
        (
            vec![blend(10, 100, 0.9999999)],
            "layer.defaults_not_normalized",
        ),
        (
            vec![LayerDefinition {
                color_linear_rgba: Some([1.0; 4]),
                ..mask(10)
            }],
            "layer.color",
        ),
    ] {
        let mut t = base.clone();
        let surface = if defs
            .iter()
            .any(|l| matches!(l.kind, LayerKind::BlendWeight { .. }))
        {
            Some(id(100))
        } else {
            None
        };
        assert_eq!(
            layers::set_registry(&mut t, defs, surface, 2)
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(t, base);
    }
    let mut t = base.clone();
    assert_eq!(
        layers::set_registry(&mut t, (10..18).map(mask).collect(), None, 1025)
            .unwrap_err()
            .code,
        "layer.limit"
    );
    assert_eq!(t, base);
    layers::set_registry(&mut t, (10..18).map(mask).collect(), None, 2).unwrap();
    let before = t.clone();
    assert_eq!(
        layers::resample_layers(&mut t, 1025).unwrap_err().code,
        "layer.limit"
    );
    assert_eq!(t, before);
}
#[test]
fn scene_weight_budget_aggregates_valid_terrains_and_primitive_paint_is_atomic() {
    let mut t = terrain();
    layers::set_registry(
        &mut t,
        vec![blend(10, 100, 1.0), blend(11, 100, 0.0)],
        Some(id(100)),
        2,
    )
    .unwrap();
    let before = t.clone();
    assert_eq!(
        layers::paint(&mut t, &id(1), &id(11), &[(0, 0.25), (99, 0.5)])
            .unwrap_err()
            .code,
        "layer.index_or_value"
    );
    assert_eq!(t, before);
    let mut large = terrain();
    layers::set_registry(&mut large, (10..15).map(mask).collect(), None, 1000).unwrap();
    scene_data::terrain::validate(&large).unwrap();
    let mut scene = SceneDocument {
        schema: SCHEMA_VERSION,
        id: id(900),
        objects: vec![],
    };
    for n in 0..2 {
        scene.objects.push(ObjectRecord {
            id: id(901 + n),
            parent: None,
            sibling_order: n as u32,
            name: "terrain".into(),
            local: Default::default(),
            data: ObjectData::Terrain(large.clone()),
        });
    }
    assert_eq!(
        scene_data::validate(&scene).unwrap_err().code,
        "scene.resource_limit"
    );
    scene.objects.pop();
    scene_data::validate(&scene).unwrap();
}
