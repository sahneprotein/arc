use editor_core::*;
use glam::{Quat, Vec3};
use scene_data::{terrain::*, *};

fn terrain_scene(cells: u32) -> (SceneDocument, ObjectId) {
    let object = StableId::from_u128(1001);
    let mut scene = current_map();
    scene.objects.push(ObjectRecord {
        id: object.clone(),
        parent: None,
        sibling_order: 1001,
        name: "Terrain".into(),
        local: Default::default(),
        data: ObjectData::Terrain(
            TerrainDocument::flat(
                cells,
                [1.0, 1.0],
                vec![(StableId::from_u128(1002), TileKey { x: 0, z: 0 })],
            )
            .unwrap(),
        ),
    });
    (scene, object)
}
fn request(object: ObjectId, mode: TerrainSculptMode) -> TerrainSculptRequest {
    TerrainSculptRequest {
        object,
        path_world_m: vec![[2.0, 0.0, 2.0]],
        radius_m: 2.0,
        strength: 0.5,
        falloff: 1.0,
        mode,
    }
}
fn after(result: &TerrainSculptResult, key: SampleKey) -> f32 {
    result
        .sample_deltas
        .iter()
        .find(|d| d.key == key)
        .unwrap()
        .after_m
}

#[test]
fn modes_and_soft_edge_have_numeric_semantics_without_overlap_stacking() {
    let (mut scene, object) = terrain_scene(4);
    let center = SampleKey { x: 2, z: 2 };
    let neighbour = SampleKey { x: 3, z: 2 };
    let mut stroke = request(object.clone(), TerrainSculptMode::Raise { amount_m: 2.0 });
    let raised = terrain_sculpt(&scene, &stroke).unwrap();
    assert_eq!(after(&raised, center), 1.0);
    assert_eq!(after(&raised, neighbour), 0.75);
    stroke.path_world_m.extend([[2.0, 0.0, 2.0]; 3]);
    assert_eq!(terrain_sculpt(&scene, &stroke).unwrap(), raised);
    stroke.mode = TerrainSculptMode::Lower { amount_m: 2.0 };
    assert_eq!(
        after(&terrain_sculpt(&scene, &stroke).unwrap(), center),
        -1.0
    );
    stroke.falloff = 0.0;
    assert_eq!(
        after(&terrain_sculpt(&scene, &stroke).unwrap(), neighbour),
        -1.0
    );
    stroke.falloff = f32::MIN_POSITIVE;
    assert_eq!(
        after(&terrain_sculpt(&scene, &stroke).unwrap(), neighbour),
        -1.0
    );

    let ObjectData::Terrain(t) = &mut scene.object_mut(&object).unwrap().data else {
        panic!()
    };
    t.samples
        .iter_mut()
        .find(|s| s.key == center)
        .unwrap()
        .height_m = 4.0;
    stroke.mode = TerrainSculptMode::Smooth;
    stroke.radius_m = 0.5;
    assert_eq!(
        after(&terrain_sculpt(&scene, &stroke).unwrap(), center),
        2.0
    );
    stroke.mode = TerrainSculptMode::Flatten { height_m: 8.0 };
    assert_eq!(
        after(&terrain_sculpt(&scene, &stroke).unwrap(), center),
        6.0
    );
}

#[test]
fn hit_height_and_sculpt_share_parented_rotated_terrain_coordinates() {
    let (mut scene, object) = terrain_scene(4);
    let parent = StableId::from_u128(1003);
    scene.objects.push(ObjectRecord {
        id: parent.clone(),
        parent: None,
        sibling_order: 1003,
        name: "Parent".into(),
        local: RigidTransform {
            translation_m: [10.0, 20.0, 30.0],
            rotation_xyzw: [
                0.0,
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
            ],
        },
        data: ObjectData::Group,
    });
    let terrain = scene.object_mut(&object).unwrap();
    terrain.parent = Some(parent);
    terrain.local.translation_m = [2.0, 3.0, 4.0];
    let ObjectData::Terrain(t) = &mut terrain.data else {
        panic!()
    };
    let key = SampleKey { x: 2, z: 2 };
    t.samples
        .iter_mut()
        .find(|s| s.key == key)
        .unwrap()
        .height_m = 5.0;
    let transforms = scene_data::world_transforms(&scene).unwrap();
    let world = &transforms[&object];
    let hit = Quat::from_array(world.rotation_xyzw) * Vec3::new(2.0, 5.0, 2.0)
        + Vec3::from_array(world.translation_m);
    let height = terrain_hit_height_m(&scene, &object, hit.to_array()).unwrap();
    assert!((height - 5.0).abs() < 0.00001);
    assert!((hit.y - height).abs() > 10.0);
    let result = terrain_sculpt(
        &scene,
        &TerrainSculptRequest {
            object,
            path_world_m: vec![hit.to_array()],
            radius_m: 0.5,
            strength: 1.0,
            falloff: 0.0,
            mode: TerrainSculptMode::Flatten {
                height_m: height + 1.0,
            },
        },
    )
    .unwrap();
    assert_eq!(result.sample_deltas.len(), 1);
    assert!((after(&result, key) - 6.0).abs() < 0.00001);
}

#[test]
fn invalid_and_over_budget_strokes_return_errors_without_source_changes() {
    let (scene, object) = terrain_scene(256);
    let before = source_hash(&scene).unwrap();
    let mut stroke = request(object, TerrainSculptMode::Raise { amount_m: 1.0 });
    stroke.path_world_m = vec![[128.0, 0.0, 128.0]; MAX_SCULPT_POINTS + 1];
    assert_eq!(
        terrain_sculpt(&scene, &stroke).unwrap_err().code,
        "terrain.brush_invalid"
    );
    stroke.path_world_m.truncate(1);
    stroke.falloff = 0.0;
    stroke.radius_m = 1000.0;
    assert_eq!(
        terrain_sculpt(&scene, &stroke).unwrap_err().code,
        "terrain.brush_limit"
    );
    stroke.radius_m = MAX_SCULPT_RADIUS_M + 1.0;
    assert_eq!(
        terrain_sculpt(&scene, &stroke).unwrap_err().code,
        "terrain.brush_invalid"
    );
    stroke.radius_m = 2.0;
    stroke.strength = f32::NAN;
    assert_eq!(
        terrain_sculpt(&scene, &stroke).unwrap_err().code,
        "terrain.brush_invalid"
    );
    assert_eq!(source_hash(&scene).unwrap(), before);
}
