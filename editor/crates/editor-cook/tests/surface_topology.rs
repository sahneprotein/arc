use scene_data::*;
#[test]
fn shared_surface_helpers_keep_pre_refactor_cooked_bytes_exact() {
    // Captured with the previous paint-complete arc-scene executable, BEFORE refactoring;
    // the visual payloads since end with the list of placed emitters, which is empty here.
    // Noncoplanar negative tiles, four-tile corner, a gap, identity and parent rotations.
    for (source, simulation, visual) in [
        (
            include_bytes!("fixtures/surface/identity.source.json").as_slice(),
            include_bytes!("fixtures/surface/identity.simulation.json").as_slice(),
            include_bytes!("fixtures/surface/identity.visual.json").as_slice(),
        ),
        (
            include_bytes!("fixtures/surface/transformed.source.json").as_slice(),
            include_bytes!("fixtures/surface/transformed.simulation.json").as_slice(),
            include_bytes!("fixtures/surface/transformed.visual.json").as_slice(),
        ),
    ] {
        let scene: SceneDocument = serde_json::from_slice(source).unwrap();
        let cooked = editor_cook::cook(&scene).unwrap();
        assert_eq!(cooked.simulation_bytes, simulation);
        assert_eq!(cooked.visual_bytes, visual);
    }
}
#[test]
fn cell_topology_is_bounded_and_preserves_the_collision_diagonal() {
    use scene_data::terrain::cell_triangles;
    assert_eq!(cell_triangles(1, 0, 0), Some([[0, 2, 1], [2, 3, 1]]));
    assert_eq!(cell_triangles(3, 1, 2), Some([[9, 13, 10], [13, 14, 10]]));
    for (n, x, z) in [
        (0, 0, 0),
        (1025, 0, 0),
        (u32::MAX, 0, 0),
        (2, 2, 0),
        (2, 0, 2),
    ] {
        assert!(cell_triangles(n, x, z).is_none());
    }
}
