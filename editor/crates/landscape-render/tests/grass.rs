use landscape_render::*;
use scene_data::{terrain::*, *};
fn package() -> editor_cook::CookedPackage {
    let mut terrain = TerrainDocument::flat(
        1,
        [1.0, 1.0],
        vec![
            (StableId::from_u128(100), TileKey { x: -1, z: 0 }),
            (StableId::from_u128(101), TileKey { x: 0, z: 0 }),
        ],
    )
    .unwrap();
    terrain.rules.push(RuleDefinition {
        id: StableId::from_u128(200),
        algorithm: scene_data::rules::GRASS_SCATTER.into(),
        version: 1,
        seed: 17,
        input_layers: vec![],
        input_rules: vec![],
        output_layer: None,
        density_layer: None,
        exclusion_layers: vec![],
        parameters: Default::default(),
    });
    let scene = SceneDocument {
        schema: SCHEMA_VERSION,
        id: StableId::from_u128(1),
        objects: vec![ObjectRecord {
            id: StableId::from_u128(2),
            parent: None,
            sibling_order: 0,
            name: "Terrain".into(),
            local: RigidTransform::default(),
            data: ObjectData::Terrain(terrain),
        }],
    };
    editor_cook::cook(&scene).unwrap()
}
#[test]
fn grass_blades_use_derived_world_placement_and_reuse_reordered_outputs() {
    let mut package = package();
    let simulation_before = package.simulation.clone();
    let mut cache = GrassCache::default();
    let facts = cache
        .update(&package.visual, &package.simulation.ground)
        .unwrap();
    assert_eq!(facts.cpu_allocated_instances, 8);
    assert_eq!(facts.tiles_rebuilt, 2);
    assert_eq!(facts.prepared_bytes, 8 * (8 * 24 + 12 * 4));
    let before = cache.tiles().clone();
    for mesh in cache.tiles().values() {
        let instances: Vec<_> = package.visual.terrain[0]
            .decoration
            .iter()
            .filter(|d| d.tile == mesh.tile)
            .collect();
        assert_eq!(mesh.positions.len(), instances.len() * 8);
        assert_eq!(mesh.indices.len(), instances.len() * 12);
        assert!(mesh
            .indices
            .iter()
            .all(|i| (*i as usize) < mesh.positions.len()));
        for (instance, vertices) in instances.iter().zip(mesh.positions.as_chunks::<8>().0) {
            for quad in vertices.as_chunks::<4>().0 {
                for (axis, expected) in instance.position_m.iter().enumerate() {
                    let center = (quad[0][axis] + quad[1][axis]) * 0.5;
                    assert!((center - expected).abs() < 1e-6);
                }
                assert!((quad[2][1] - instance.position_m[1] - instance.height_m).abs() < 1e-6);
            }
        }
        for normal in &mesh.normals {
            let length_squared: f32 = normal.iter().map(|n| n * n).sum();
            assert!((length_squared - 1.0).abs() < 1e-5);
        }
    }
    package.visual.terrain[0].decoration.reverse();
    let repeated = cache
        .update(&package.visual, &package.simulation.ground)
        .unwrap();
    assert_eq!(repeated.tiles_rebuilt, 0);
    assert_eq!(repeated.prepared_bytes, 0);
    for (key, data) in cache.tiles() {
        assert!(std::sync::Arc::ptr_eq(data, &before[key]));
    }
    assert_eq!(package.simulation, simulation_before);
    // Change one stable instance; only its tile is rebuilt.
    package.visual.terrain[0].decoration[0].height_m += 0.1;
    assert_eq!(
        cache
            .update(&package.visual, &package.simulation.ground)
            .unwrap()
            .tiles_rebuilt,
        1
    );
    package.visual.terrain[0].decoration.clear();
    let removed = cache
        .update(&package.visual, &package.simulation.ground)
        .unwrap();
    assert_eq!(removed.tiles_removed, 2);
    assert_eq!(removed.cpu_allocated_instances, 0);
    assert!(cache.tiles().is_empty());
}
#[test]
fn invalid_decoration_leaves_accepted_meshes_and_counts_unchanged() {
    let mut package = package();
    let mut cache = GrassCache::default();
    cache
        .update(&package.visual, &package.simulation.ground)
        .unwrap();
    let before = cache.tiles().clone();
    package.visual.terrain[0].decoration[0].position_m[0] = f32::NAN;
    assert_eq!(
        cache
            .update(&package.visual, &package.simulation.ground)
            .unwrap_err()
            .code,
        "payload.decoration_invalid"
    );
    for (key, data) in cache.tiles() {
        assert!(std::sync::Arc::ptr_eq(data, &before[key]));
    }
}
