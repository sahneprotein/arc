use editor_cook::GeometryCache;
use landscape_render::GroundCache;
use scene_data::{terrain::*, *};
use std::sync::Arc;

#[test]
fn seam_preview_rebuilds_only_changed_meshes_and_cancel_restores_exact_collision() {
    let terrain = TerrainDocument::flat(
        8,
        [1.0, 1.0],
        (0..4)
            .flat_map(|z| {
                (0..4).map(move |x| {
                    (
                        StableId::from_u128(100 + (z * 4 + x) as u128),
                        TileKey { x, z },
                    )
                })
            })
            .collect(),
    )
    .unwrap();
    let base = SceneDocument {
        schema: SCHEMA_VERSION,
        id: StableId::from_u128(1),
        objects: vec![ObjectRecord {
            id: StableId::from_u128(2),
            parent: None,
            sibling_order: 0,
            name: "ground".into(),
            local: Default::default(),
            data: ObjectData::Terrain(terrain),
        }],
    };
    let mut cook = GeometryCache::default();
    let original = cook.cook(Arc::new(base.clone()), &|| Ok(())).unwrap();
    let mut renderer = GroundCache::default();
    renderer
        .update_with_visual(&original.simulation.ground, Some(&original.visual), &[])
        .unwrap();
    let old_meshes = renderer.tiles().clone();
    let mut preview = base.clone();
    let ObjectData::Terrain(t) = &mut preview.objects[0].data else {
        panic!()
    };
    t.samples
        .iter_mut()
        .find(|s| s.key == SampleKey { x: 8, z: 8 })
        .unwrap()
        .height_m = 0.75;
    let updated = cook.cook(Arc::new(preview), &|| Ok(())).unwrap();
    assert_eq!(updated.work.rebuilt_tiles, 4);
    let facts = renderer
        .update_with_visual(&updated.simulation.ground, Some(&updated.visual), &[])
        .unwrap();
    assert_eq!(facts.tiles_rebuilt, 4);
    for tile in &updated.simulation.ground {
        let key = (tile.object.clone(), tile.tile.clone());
        let mesh = &renderer.tiles()[&key];
        assert_eq!(mesh.positions, tile.positions_m);
        assert_eq!(mesh.normals, tile.normals);
        assert_eq!(
            mesh.indices,
            tile.triangles.iter().flatten().copied().collect::<Vec<_>>()
        );
        if tile.key.x > 1 || tile.key.z > 1 {
            assert!(Arc::ptr_eq(mesh, &old_meshes[&key]));
        }
    }
    let restored = cook.cook(Arc::new(base), &|| Ok(())).unwrap();
    assert_eq!(restored.simulation, original.simulation);
    let facts = renderer
        .update_with_visual(&restored.simulation.ground, Some(&restored.visual), &[])
        .unwrap();
    assert_eq!(facts.tiles_rebuilt, 4);
    for (key, mesh) in renderer.tiles() {
        assert_eq!(mesh.as_ref(), old_meshes[key].as_ref());
    }
}
