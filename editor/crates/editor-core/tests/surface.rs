use editor_core::*;
use glam::{Quat, Vec3};
use scene_data::{terrain::*, *};
fn id(n: u128) -> StableId {
    StableId::from_u128(n)
}
fn scene(cells: u32) -> SceneDocument {
    let mut t = TerrainDocument::flat(
        cells,
        [0.75, 1.25],
        vec![
            (id(105), TileKey { x: -1, z: -1 }),
            (id(104), TileKey { x: 0, z: -1 }),
            (id(103), TileKey { x: -1, z: 0 }),
            (id(102), TileKey { x: 0, z: 0 }),
            (id(101), TileKey { x: 2, z: 0 }),
        ],
    )
    .unwrap();
    for s in &mut t.samples {
        s.height_m = ((s.key.x * 17 + s.key.z * 31).rem_euclid(13) as f32 - 6.0) * 0.125;
    }
    SceneDocument {
        schema: SCHEMA_VERSION,
        id: id(1),
        objects: vec![ObjectRecord {
            id: id(99),
            parent: None,
            sibling_order: 0,
            name: "Ground".into(),
            local: Default::default(),
            data: ObjectData::Terrain(t),
        }],
    }
}
fn terrain(s: &mut SceneDocument) -> &mut TerrainDocument {
    let ObjectData::Terrain(t) = &mut s.objects[0].data else {
        panic!()
    };
    t
}
fn ray(origin: [f32; 3], direction: [f32; 3]) -> TerrainSurfaceRay {
    TerrainSurfaceRay {
        origin_world_m: origin,
        direction_world: direction,
        max_distance_m: 10000.0,
    }
}
fn hit(s: &SceneDocument, r: &TerrainSurfaceRay) -> Option<TerrainSurfaceHit> {
    terrain_surface_hit(s, &id(99), r).unwrap()
}
#[test]
fn every_nonplanar_cell_half_hits_exact_cooker_triangle_and_face_normal() {
    for cells in [1, 2, 4] {
        let source = scene(cells);
        let before = source_hash(&source).unwrap();
        let cooked = editor_cook::cook(&source).unwrap();
        for tile in &cooked.simulation.ground {
            for (index, triangle) in tile.triangles.iter().enumerate() {
                let [a, b, c] = triangle.map(|i| Vec3::from_array(tile.positions_m[i as usize]));
                let target = (a + b + c) / 3.0;
                let r = ray([target.x, 20.0, target.z], [0.0, -7.0, 0.0]);
                let found = hit(&source, &r).unwrap();
                assert_eq!(
                    (found.tile, found.key, found.triangle_index),
                    (tile.tile.clone(), tile.key, index as u32)
                );
                assert_eq!(
                    found.cell_xz,
                    [(index as u32 / 2) % cells, (index as u32 / 2) / cells]
                );
                assert!((Vec3::from_array(found.position_world_m) - target).length() < 2e-6);
                assert!(
                    (Vec3::from_array(found.normal_world) - (b - a).cross(c - a).normalize())
                        .length()
                        < 2e-6
                );
                assert!((found.distance_m - (20.0 - f64::from(target.y))).abs() < 2e-6);
            }
        }
        assert_eq!(source_hash(&source).unwrap(), before);
    }
}
#[test]
fn parented_rotated_world_triangles_and_two_sided_rays_are_authoritative() {
    let mut source = scene(2);
    source.objects[0].parent = Some(id(98));
    source.objects[0].local = RigidTransform {
        translation_m: [1.0, 2.0, 3.0],
        rotation_xyzw: Quat::from_rotation_z(0.7).to_array(),
    };
    source.objects.push(ObjectRecord {
        id: id(98),
        parent: None,
        sibling_order: 0,
        name: "Parent".into(),
        local: RigidTransform {
            translation_m: [8.0, -2.0, 4.0],
            rotation_xyzw: Quat::from_rotation_y(0.4).to_array(),
        },
        data: ObjectData::Group,
    });
    let cooked = editor_cook::cook(&source).unwrap();
    let q = Quat::from_array(world_transforms(&source).unwrap()[&id(99)].rotation_xyzw);
    for tile in &cooked.simulation.ground {
        for (index, triangle) in tile.triangles.iter().enumerate() {
            let [a, b, c] = triangle.map(|i| Vec3::from_array(tile.positions_m[i as usize]));
            let target = (a + b + c) / 3.0;
            let up = q * Vec3::Y;
            for sign in [1.0, -1.0] {
                let r = ray(
                    (target + up * 10.0 * sign).to_array(),
                    (-up * sign).to_array(),
                );
                let found = hit(&source, &r).unwrap();
                assert_eq!(found.tile, tile.tile);
                assert_eq!(found.triangle_index, index as u32);
                assert!((Vec3::from_array(found.position_world_m) - target).length() < 5e-6);
                assert!(
                    Vec3::from_array(found.normal_world).dot((b - a).cross(c - a).normalize())
                        > 0.99999
                );
            }
        }
    }
}
#[test]
fn seam_corner_and_diagonal_ties_use_tile_keys_not_tile_ids_or_storage_order() {
    let mut source = scene(1);
    terrain(&mut source).spacing_xz_m = [1.0, 1.0];
    for s in &mut terrain(&mut source).samples {
        s.height_m = 0.0;
    }
    for (point, key, index) in [
        ([0.0, 10.0, 0.0], TileKey { x: -1, z: -1 }, 1),
        ([0.0, 10.0, 0.25], TileKey { x: -1, z: 0 }, 1),
        ([0.25, 10.0, 0.0], TileKey { x: 0, z: -1 }, 1),
        ([0.5, 10.0, 0.5], TileKey { x: 0, z: 0 }, 0),
    ] {
        let r = ray(point, [0.0, -1.0, 0.0]);
        let found = hit(&source, &r).unwrap();
        assert_eq!((found.key, found.triangle_index), (key, index));
        let mut reordered = source.clone();
        let t = terrain(&mut reordered);
        t.tiles.reverse();
        t.samples.reverse();
        assert_eq!(hit(&reordered, &r), Some(found));
    }
    // Interior grid vertex: lower row-major triangle index wins all incident cell ties.
    let mut source = scene(2);
    for s in &mut terrain(&mut source).samples {
        s.height_m = 0.0;
    }
    let found = hit(&source, &ray([0.75, 10.0, 1.25], [0.0, -1.0, 0.0])).unwrap();
    assert_eq!(found.key, TileKey { x: 0, z: 0 });
    assert_eq!(found.triangle_index, 1);
}
#[test]
fn misses_gaps_parallel_coplanar_behind_origin_and_max_distance_do_not_extrapolate() {
    let mut source = scene(1);
    for s in &mut terrain(&mut source).samples {
        s.height_m = 0.0;
    }
    for r in [
        ray([1.0, 10.0, 0.5], [0.0, -1.0, 0.0]),
        ray([100.0, 10.0, 100.0], [0.0, -1.0, 0.0]),
        ray([0.1, 10.0, 0.1], [1.0, 0.0, 0.0]),
        ray([0.1, -10.0, 0.1], [0.0, -1.0, 0.0]),
        ray([0.1, 0.0, 0.1], [0.0, -1.0, 0.0]),
        ray([0.1, 0.0, 0.1], [1.0, 0.0, 0.0]),
    ] {
        assert!(hit(&source, &r).is_none());
    }
    let mut r = ray([0.1, 10.0, 0.1], [0.0, -1.0, 0.0]);
    r.max_distance_m = 9.999;
    assert!(hit(&source, &r).is_none());
    r.max_distance_m = 10.0;
    assert_eq!(hit(&source, &r).unwrap().distance_m, 10.0);
    terrain(&mut source).tiles.clear();
    assert!(hit(&source, &r).is_none());
}
#[test]
fn bad_ray_object_kind_source_and_collapsed_world_geometry_fail_with_stable_diagnostics() {
    let source = scene(1);
    let valid = ray([0.1, 10.0, 0.1], [0.0, -1.0, 0.0]);
    for r in [
        ray([f32::NAN, 0.0, 0.0], [0.0, -1.0, 0.0]),
        ray([0.0; 3], [0.0; 3]),
        ray([0.0; 3], [f32::INFINITY, 0.0, 0.0]),
        TerrainSurfaceRay {
            max_distance_m: 0.0,
            ..valid.clone()
        },
        TerrainSurfaceRay {
            max_distance_m: f64::INFINITY,
            ..valid.clone()
        },
    ] {
        assert_eq!(
            terrain_surface_hit(&source, &id(99), &r).unwrap_err().code,
            "terrain.ray_invalid"
        );
    }
    assert_eq!(
        terrain_surface_hit(&source, &id(999), &valid)
            .unwrap_err()
            .code,
        "object.missing"
    );
    let mut bad = source.clone();
    bad.objects[0].data = ObjectData::Group;
    assert_eq!(
        terrain_surface_hit(&bad, &id(99), &valid).unwrap_err().code,
        "field.kind"
    );
    let mut bad = source.clone();
    terrain(&mut bad).samples.pop();
    assert_eq!(
        terrain_surface_hit(&bad, &id(99), &valid).unwrap_err().code,
        "terrain.sample_missing"
    );
    let mut bad = source.clone();
    bad.objects[0].local.translation_m = [1e20; 3];
    assert_eq!(
        terrain_surface_hit(&bad, &id(99), &valid).unwrap_err().code,
        "cook.degenerate_triangle"
    );
    assert_eq!(
        editor_cook::cook(&bad).unwrap_err().code,
        "cook.degenerate_triangle"
    );
}
#[test]
fn direction_scale_extremes_and_mixed_layer_resolutions_do_not_change_surface_hits() {
    let mut source = scene(2);
    let r = ray([0.3, 10.0, 0.4], [0.0, -1.0, 0.0]);
    let baseline = hit(&source, &r).unwrap();
    for scale in [f32::MIN_POSITIVE, f32::from_bits(1), f32::MAX, 7.0] {
        assert_eq!(
            hit(&source, &ray(r.origin_world_m, [0.0, -scale, 0.0])),
            Some(baseline.clone())
        );
    }
    let t = terrain(&mut source);
    scene_data::layers::set_registry(
        t,
        vec![LayerDefinition {
            id: id(300),
            name: "mask".into(),
            kind: LayerKind::Mask,
            color_linear_rgba: None,
        }],
        None,
        2,
    )
    .unwrap();
    for (i, tile) in t.tiles.iter_mut().enumerate() {
        tile.layers[0].side_samples = 2 + i as u32;
        tile.layers[0].weights = vec![0.5; tile.layers[0].side_samples.pow(2) as usize];
    }
    assert_eq!(hit(&source, &r), Some(baseline));
}
#[test]
fn nearest_positive_hit_wins_on_a_folded_heightfield_seen_along_world_x() {
    let mut source = scene(2);
    let t = terrain(&mut source);
    t.tiles.retain(|tile| tile.key == TileKey { x: 0, z: 0 });
    t.spacing_xz_m = [1.0, 1.0];
    for s in &mut t.samples {
        s.height_m = if s.key.x == 1 { 2.0 } else { 0.0 };
    }
    let found = hit(&source, &ray([-1.0, 1.0, 0.25], [1.0, 0.0, 0.0])).unwrap();
    assert!((found.distance_m - 1.5).abs() < 1e-12);
    assert_eq!(found.cell_xz, [0, 0]);
    let found = hit(&source, &ray([3.0, 1.0, 0.25], [-1.0, 0.0, 0.0])).unwrap();
    assert!((found.distance_m - 1.5).abs() < 1e-12);
    assert_eq!(found.cell_xz, [1, 0]);
}

#[test]
fn parent_rotated_shared_world_vertices_have_stable_incident_triangle_ties() {
    for angle in [0.1, 0.3, 0.6, 0.9, 1.1] {
        let mut source = scene(1);
        for s in &mut terrain(&mut source).samples {
            s.height_m = 0.0;
        }
        source.objects[0].parent = Some(id(98));
        source.objects.push(ObjectRecord {
            id: id(98),
            parent: None,
            sibling_order: 0,
            name: "Parent".into(),
            local: RigidTransform {
                translation_m: [8.25, -2.3, 4.125],
                rotation_xyzw: (Quat::from_rotation_y(0.4) * Quat::from_rotation_x(angle))
                    .to_array(),
            },
            data: ObjectData::Group,
        });
        let cooked = editor_cook::cook(&source).unwrap();
        let tile = cooked
            .simulation
            .ground
            .iter()
            .find(|t| t.key == TileKey { x: -1, z: -1 })
            .unwrap();
        let point = tile.positions_m[3];
        let r = ray([point[0], point[1] + 9.7, point[2]], [0.0, -1.0, 0.0]);
        let found = hit(&source, &r).unwrap();
        assert_eq!(
            (found.key, found.triangle_index),
            (TileKey { x: -1, z: -1 }, 1),
            "angle={angle}"
        );
        assert_eq!(
            found.position_world_m.map(f32::to_bits),
            point.map(f32::to_bits)
        );
    }
}

#[path = "surface/placement.rs"]
mod placement;
