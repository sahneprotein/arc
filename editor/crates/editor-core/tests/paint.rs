use editor_core::*;
use scene_data::{terrain::*, *};
#[path = "support/paint.rs"]
mod support;
use support::*;

#[test]
fn scalar_target_erase_and_shared_height_falloff_do_not_stack() {
    let mut base = source(2);
    let mut r = request(202);
    let first = terrain_paint(&base, &r).unwrap();
    let corner: Vec<_> = first
        .texel_deltas
        .iter()
        .filter(|d| d.layer == r.layer && d.after == 0.75)
        .collect();
    assert_eq!(corner.len(), 4);
    let side: Vec<_> = first
        .texel_deltas
        .iter()
        .filter(|d| d.layer == r.layer && (d.weight - 5.0 / 9.0).abs() < 1e-6)
        .collect();
    assert!(!side.is_empty());
    assert!((side[0].after - 0.75 * 5.0 / 9.0).abs() < 1e-6);
    r.path_world_m = vec![[0.0; 3]; 32];
    assert_eq!(terrain_paint(&base, &r).unwrap(), first);
    let tt = t(&mut base);
    for tile in &mut tt.tiles {
        for p in &mut tile.layers {
            if p.layer == StableId::from_u128(203) {
                p.weights.fill(0.8);
            }
        }
    }
    r = request(203);
    r.mode = TerrainPaintMode::Erase;
    r.strength = 0.25;
    let erased = terrain_paint(&base, &r).unwrap();
    assert!(erased
        .texel_deltas
        .iter()
        .any(|d| d.weight == 1.0 && (d.after - 0.6).abs() < 1e-6));
    r.mode = TerrainPaintMode::Paint;
    r.strength = 0.4;
    let painted = terrain_paint(&base, &r).unwrap();
    assert!(painted
        .texel_deltas
        .iter()
        .any(|d| d.weight == 1.0 && d.after == 0.4));
    let sculpt = TerrainSculptRequest {
        object: r.object.clone(),
        path_world_m: r.path_world_m.clone(),
        radius_m: r.radius_m,
        strength: 1.0,
        falloff: r.falloff,
        mode: TerrainSculptMode::Raise { amount_m: 1.0 },
    };
    let height = terrain_sculpt(&base, &sculpt).unwrap();
    for delta in painted.texel_deltas.iter() {
        assert!(height
            .sample_deltas
            .iter()
            .any(|d| d.weight.to_bits() == delta.weight.to_bits()));
    }
}
#[test]
fn preview_cancel_stale_commit_noop_and_one_undo_keep_exact_authority() {
    let base = source(2);
    let mut s = EditorSession::new(base.clone()).unwrap();
    s.mark_saved(s.source_hash());
    let txn = begin(&mut s, 1);
    let r = request(202);
    let one = preview(&mut s, 2, txn, 1, r.clone());
    assert_eq!(one.dirty_regions.len(), 4);
    assert_eq!(s.snapshot().document.as_ref(), &base);
    let saved = s.snapshot().preview.as_ref().unwrap().document.clone();
    let mut bad = r.clone();
    bad.radius_m = f32::NAN;
    assert_eq!(
        send(
            &mut s,
            3,
            Command::PreviewTerrainPaint {
                transaction: txn,
                sequence: 2,
                request: bad
            }
        )
        .unwrap_err()
        .code,
        "terrain.brush_invalid"
    );
    assert_eq!(s.snapshot().preview.as_ref().unwrap().sequence, 1);
    assert_eq!(s.snapshot().preview.as_ref().unwrap().document, saved);
    assert_eq!(
        send(
            &mut s,
            4,
            Command::PreviewTerrainPaint {
                transaction: txn,
                sequence: 1,
                request: r.clone()
            }
        )
        .unwrap_err()
        .code,
        "preview.stale"
    );
    assert_eq!(
        send(&mut s, 5, Command::Undo).unwrap_err().code,
        "transaction.busy"
    );
    send(&mut s, 6, Command::CancelViewportEdit { transaction: txn }).unwrap();
    assert_eq!(s.snapshot().document.as_ref(), &base);
    assert!(!s.snapshot().status.dirty);
    let txn = begin(&mut s, 7);
    preview(&mut s, 8, txn, 1, r);
    send(
        &mut s,
        9,
        Command::CommitViewportEdit {
            transaction: txn,
            label_key: "paint".into(),
        },
    )
    .unwrap();
    let edited = s.snapshot().document.clone();
    assert_eq!(s.snapshot().undo_label.as_deref(), Some("paint"));
    send(&mut s, 10, Command::Undo).unwrap();
    assert_eq!(s.snapshot().document.as_ref(), &base);
    assert!(s.snapshot().undo_label.is_none());
    assert!(!s.snapshot().status.dirty);
    let txn = begin(&mut s, 11);
    let mut no = request(202);
    no.path_world_m = vec![[100.0, 0.0, 100.0]];
    assert!(preview(&mut s, 12, txn, 1, no).texel_deltas.is_empty());
    let revision = s.snapshot().document_revision;
    send(
        &mut s,
        13,
        Command::CommitViewportEdit {
            transaction: txn,
            label_key: "paint".into(),
        },
    )
    .unwrap();
    assert_eq!(s.snapshot().document_revision, revision);
    assert!(!s.snapshot().status.dirty);
    assert!(s.snapshot().undo_label.is_none());
    assert!(s.snapshot().redo_label.is_some());
    assert!(s.snapshot().preview.is_none());
    send(&mut s, 14, Command::Redo).unwrap();
    assert_eq!(s.snapshot().document, edited);
}
#[test]
fn canonical_border_owner_and_rational_mixed_resolution_are_storage_independent() {
    let mut base = source(2);
    let tt = t(&mut base);
    tt.tiles.retain(|tile| tile.key.z == 0);
    // Shared x=0, z=1 exists in side3 and side5, with intentionally different base masks.
    for tile in &mut tt.tiles {
        let mask = tile
            .layers
            .iter_mut()
            .find(|p| p.layer == StableId::from_u128(203))
            .unwrap();
        mask.side_samples = if tile.key.x == -1 { 3 } else { 5 };
        mask.weights = vec![
            if tile.key.x == -1 { 0.2 } else { 0.8 };
            (mask.side_samples * mask.side_samples) as usize
        ];
    }
    let mut r = request(203);
    r.path_world_m = vec![[0.0, 0.0, 1.0]];
    r.radius_m = 0.25;
    r.strength = 0.6;
    let result = terrain_paint(&base, &r).unwrap();
    assert_eq!(result.texel_deltas.len(), 2);
    assert!(result.texel_deltas.iter().all(|d| d.after == 0.6));
    r.mode = TerrainPaintMode::Erase;
    r.strength = 0.5;
    let result = terrain_paint(&base, &r).unwrap();
    assert_eq!(result.texel_deltas.len(), 2);
    assert!(result.texel_deltas.iter().all(|d| d.after == 0.1));
    let tt = t(&mut base);
    tt.tiles.reverse();
    tt.layers.reverse();
    for tile in &mut tt.tiles {
        tile.layers.reverse();
    }
    assert_eq!(terrain_paint(&base, &r).unwrap(), result);
}
#[test]
fn parented_world_path_equals_local_path_and_negative_tile_brush_is_exact() {
    let local = source(2);
    let mut world = local.clone();
    let rotation = glam::Quat::from_rotation_z(0.3);
    world.objects[0].parent = Some(StableId::from_u128(48));
    world.objects[0].local.translation_m = [1.0, 2.0, 3.0];
    world.objects.push(ObjectRecord {
        id: StableId::from_u128(48),
        parent: None,
        sibling_order: 1,
        name: "parent".into(),
        local: RigidTransform {
            translation_m: [8.0, -2.0, 4.0],
            rotation_xyzw: rotation.to_array(),
        },
        data: ObjectData::Group,
    });
    let mut a = request(202);
    a.path_world_m = vec![[-1.0, 0.0, -1.0]];
    a.falloff = 0.0;
    a.radius_m = 0.25;
    let expected = terrain_paint(&local, &a).unwrap();
    assert_eq!(expected.texel_deltas.len(), 2);
    let transform = scene_data::world_transforms(&world).unwrap()[&a.object].clone();
    let mut b = a;
    b.path_world_m = b
        .path_world_m
        .iter()
        .map(|p| {
            (glam::Quat::from_array(transform.rotation_xyzw) * glam::Vec3::from_array(*p)
                + glam::Vec3::from_array(transform.translation_m))
            .to_array()
        })
        .collect();
    assert_eq!(terrain_paint(&world, &b).unwrap(), expected);
}
#[test]
fn singleton_selected_and_peer_rule_outputs_refuse_without_mutation() {
    let mut base = source(2);
    let tt = t(&mut base);
    tt.layers.retain(|l| l.id != StableId::from_u128(202));
    for tile in &mut tt.tiles {
        tile.layers.retain(|p| p.layer != StableId::from_u128(202));
    }
    assert_eq!(
        terrain_paint(&base, &request(201)).unwrap_err().code,
        "layer.single_weight"
    );
    let mut r = request(201);
    r.mode = TerrainPaintMode::Erase;
    assert_eq!(
        terrain_paint(&base, &r).unwrap_err().code,
        "layer.single_weight"
    );
    r.strength = 0.0;
    assert!(terrain_paint(&base, &r).unwrap().texel_deltas.is_empty());
    let mut base = source(2);
    t(&mut base).rules.push(RuleDefinition {
        id: StableId::from_u128(300),
        algorithm: scene_data::rules::AUTO_LAYER_SLOPE_HEIGHT.into(),
        version: 1,
        seed: 0,
        input_layers: vec![],
        input_rules: vec![],
        output_layer: Some(StableId::from_u128(202)),
        density_layer: None,
        exclusion_layers: vec![],
        parameters: Default::default(),
    });
    assert_eq!(
        terrain_paint(&base, &request(202)).unwrap_err().code,
        "layer.derived_output"
    );
    assert_eq!(
        terrain_paint(&base, &request(201)).unwrap_err().code,
        "layer.derived_output"
    );
    let before = base.clone();
    let tile = t(&mut base).tiles[0].id.clone();
    assert_eq!(
        scene_data::layers::paint(t(&mut base), &tile, &StableId::from_u128(201), &[(0, 0.5)])
            .unwrap_err()
            .code,
        "layer.derived_output"
    );
    assert_eq!(base, before);
}
#[test]
fn brush_limits_bound_points_sites_peer_deltas_and_rectangle_visits() {
    let base = source(2);
    for invalid in [0, 1, 2, 3, 4] {
        let mut r = request(202);
        match invalid {
            0 => r.path_world_m = vec![[0.0; 3]; 257],
            1 => r.radius_m = 10_001.0,
            2 => r.strength = 1.1,
            3 => r.falloff = -0.1,
            _ => r.path_world_m[0][0] = f32::INFINITY,
        };
        assert_eq!(
            terrain_paint(&base, &r).unwrap_err().code,
            "terrain.brush_invalid"
        );
    }
    for (cells, layer, points) in [(256, 203, 1), (184, 202, 1), (64, 203, 256)] {
        let mut base = source(cells);
        t(&mut base).tiles.truncate(1);
        let mut r = request(layer);
        r.path_world_m = vec![[0.0; 3]; points];
        r.radius_m = 1000.0;
        r.falloff = 0.0;
        assert_eq!(
            terrain_paint(&base, &r).unwrap_err().code,
            "terrain.brush_limit"
        );
    }
}

#[test]
fn mixed_resolution_blend_border_reuses_canonical_base_and_renormalizes_every_copy() {
    let mut base = source(2);
    let tt = t(&mut base);
    tt.tiles.retain(|tile| tile.key.z == 0);
    for tile in &mut tt.tiles {
        for payload in &mut tile.layers {
            if payload.layer == StableId::from_u128(203) {
                continue;
            }
            payload.side_samples = if tile.key.x == -1 { 3 } else { 5 };
            let blue = if tile.key.x == -1 { 0.2 } else { 0.8 };
            payload.weights = vec![
                if payload.layer == StableId::from_u128(202) {
                    blue
                } else {
                    1.0 - blue
                };
                (payload.side_samples * payload.side_samples) as usize
            ];
        }
    }
    let mut r = request(202);
    r.path_world_m = vec![[0.0, 0.0, 1.0]];
    r.radius_m = 0.25;
    r.mode = TerrainPaintMode::Erase;
    r.strength = 0.5;
    let result = terrain_paint(&base, &r).unwrap();
    assert_eq!(result.texel_deltas.len(), 4);
    for d in result.texel_deltas.iter() {
        assert_eq!(d.after, if d.layer == r.layer { 0.1 } else { 0.9 });
    }
}
