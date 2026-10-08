use scene_data::{terrain::*, *};
fn setup() -> TerrainDocument {
    let mut t = TerrainDocument::flat(
        2,
        [1.0, 1.0],
        vec![(StableId::from_u128(1), TileKey { x: 0, z: 0 })],
    )
    .unwrap();
    let group = StableId::from_u128(100);
    let definitions = vec![
        LayerDefinition {
            id: StableId::from_u128(10),
            name: "Base".into(),
            kind: LayerKind::BlendWeight {
                group: group.clone(),
                default_weight: 1.0,
            },
            color_linear_rgba: Some([1.0, 0.0, 0.0, 1.0]),
        },
        LayerDefinition {
            id: StableId::from_u128(11),
            name: "Paint".into(),
            kind: LayerKind::BlendWeight {
                group: group.clone(),
                default_weight: 0.0,
            },
            color_linear_rgba: Some([0.0, 0.0, 1.0, 1.0]),
        },
        LayerDefinition {
            id: StableId::from_u128(12),
            name: "Exclusion".into(),
            kind: LayerKind::Mask,
            color_linear_rgba: None,
        },
    ];
    layers::set_registry(&mut t, definitions, Some(group), 2).unwrap();
    t
}
#[test]
fn paint_normalizes_only_group_peers_and_masks_keep_independent_values() {
    let mut t = setup();
    layers::paint(
        &mut t,
        &StableId::from_u128(1),
        &StableId::from_u128(11),
        &[(0, 0.25)],
    )
    .unwrap();
    layers::paint(
        &mut t,
        &StableId::from_u128(1),
        &StableId::from_u128(12),
        &[(0, 1.0)],
    )
    .unwrap();
    validate_terrain(&t);
    assert_eq!(t.tiles[0].layers[0].weights[0], 0.75);
    assert_eq!(t.tiles[0].layers[1].weights[0], 0.25);
    assert_eq!(t.tiles[0].layers[2].weights[0], 1.0);
    assert_eq!(t.tiles[0].layers[0].weights[1], 1.0);
    assert!(t.layers.iter().all(|l| !l.affects_simulation()));
}
fn validate_terrain(t: &TerrainDocument) {
    scene_data::terrain::validate(t).unwrap();
}
#[test]
fn rename_reorder_and_bilinear_resample_keep_ids_and_authored_paint() {
    let mut t = setup();
    layers::paint(
        &mut t,
        &StableId::from_u128(1),
        &StableId::from_u128(11),
        &[(0, 1.0)],
    )
    .unwrap();
    let mut registry = t.layers.clone();
    registry.reverse();
    registry[1].name = "Renamed".into();
    let group = t.surface_group.clone();
    layers::set_registry(&mut t, registry, group, 3).unwrap();
    assert!(t.tiles[0].layers.iter().all(|p| p.side_samples == 2));
    layers::resample_layers(&mut t, 3).unwrap();
    validate_terrain(&t);
    let paint = t.tiles[0]
        .layers
        .iter()
        .find(|p| p.layer == StableId::from_u128(11))
        .unwrap();
    assert_eq!(
        paint.weights,
        vec![1.0, 0.5, 0.0, 0.5, 0.25, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(t.layers[1].name, "Renamed");
}
#[test]
fn unnormalized_groups_bad_resolution_and_unknown_surface_selection_reject() {
    let mut t = setup();
    t.tiles[0].layers[0].weights[0] = 0.5;
    assert_eq!(
        scene_data::terrain::validate(&t).unwrap_err().code,
        "layer.weights_not_normalized"
    );
    let mut t = setup();
    t.tiles[0].layers[1].side_samples = 3;
    t.tiles[0].layers[1].weights = vec![0.0; 9];
    assert_eq!(
        scene_data::terrain::validate(&t).unwrap_err().code,
        "layer.group_resolution"
    );
    let mut t = setup();
    t.surface_group = Some(StableId::from_u128(999));
    assert_eq!(
        scene_data::terrain::validate(&t).unwrap_err().code,
        "layer.surface_group_missing"
    );
}
