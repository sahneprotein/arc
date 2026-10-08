use editor_core::*;
use scene_data::{terrain::*, *};
#[path = "support/paint.rs"]
mod support;
use support::*;
#[test]
fn implicit_defaults_materialize_only_affected_payloads_and_repeated_previews_are_base_relative() {
    let mut base = source(2);
    for tile in &mut t(&mut base).tiles {
        tile.layers.clear();
    }
    let mut s = EditorSession::new(base.clone()).unwrap();
    let txn = begin(&mut s, 1);
    let mut r = request(202);
    r.path_world_m = vec![[-1.0, 0.0, -1.0]];
    r.radius_m = 0.25;
    let first = preview(&mut s, 2, txn, 1, r.clone());
    assert_eq!(first.texel_deltas.len(), 2);
    let saved = s.snapshot().preview.as_ref().unwrap().document.clone();
    assert_eq!(preview(&mut s, 3, txn, 2, r.clone()), first);
    assert_eq!(s.snapshot().preview.as_ref().unwrap().document, saved);
    let ObjectData::Terrain(tt) = &saved.objects[0].data else {
        panic!()
    };
    assert_eq!(tt.tiles.iter().filter(|t| !t.layers.is_empty()).count(), 1);
    assert_eq!(tt.tiles[0].layers.len(), 2); // unaffected mask remains implicit default zero
    r.strength = 0.0;
    assert!(preview(&mut s, 4, txn, 3, r).texel_deltas.is_empty());
    assert_eq!(
        s.snapshot().preview.as_ref().unwrap().document.as_ref(),
        &base
    );
    send(
        &mut s,
        5,
        Command::CommitViewportEdit {
            transaction: txn,
            label_key: "paint".into(),
        },
    )
    .unwrap();
    assert!(s.snapshot().undo_label.is_none());
    assert_eq!(s.snapshot().document_revision, 0);
    let txn = begin(&mut s, 6);
    let mut r = request(201);
    r.strength = 1.0;
    assert!(preview(&mut s, 7, txn, 1, r).texel_deltas.is_empty());
    send(&mut s, 8, Command::CancelViewportEdit { transaction: txn }).unwrap();
}
#[test]
fn rule_referenced_removal_requires_atomic_reference_update_and_explicit_resolution_is_undoable() {
    let mut base = source(2);
    let object = base.objects[0].id.clone();
    t(&mut base).rules.push(RuleDefinition {
        id: StableId::from_u128(300),
        algorithm: scene_data::rules::AUTO_LAYER_SLOPE_HEIGHT.into(),
        version: 1,
        seed: 0,
        input_layers: vec![StableId::from_u128(203)],
        input_rules: vec![],
        output_layer: Some(StableId::from_u128(202)),
        density_layer: None,
        exclusion_layers: vec![],
        parameters: Default::default(),
    });
    let mut defs = t(&mut base).layers.clone();
    defs.retain(|l| l.id != StableId::from_u128(203));
    let registry = Mutation::TerrainRegistry {
        object: object.clone(),
        layers: defs,
        surface_group: Some(StableId::from_u128(200)),
        side_samples: 7,
    };
    let mut s = EditorSession::new(base.clone()).unwrap();
    assert_eq!(
        send(
            &mut s,
            1,
            Command::Edit {
                label_key: "remove".into(),
                mutations: vec![registry.clone()]
            }
        )
        .unwrap_err()
        .code,
        "rule.layer_binding"
    );
    assert_eq!(s.snapshot().document.as_ref(), &base);
    send(
        &mut s,
        2,
        Command::Edit {
            label_key: "remove".into(),
            mutations: vec![
                registry,
                Mutation::TerrainRules {
                    object: object.clone(),
                    rules: vec![],
                },
            ],
        },
    )
    .unwrap();
    let ObjectData::Terrain(tt) = &s.snapshot().document.objects[0].data else {
        panic!()
    };
    assert!(tt
        .tiles
        .iter()
        .flat_map(|t| &t.layers)
        .all(|p| p.side_samples == 3));
    send(&mut s, 3, Command::Undo).unwrap();
    assert_eq!(s.snapshot().document.as_ref(), &base);
    send(
        &mut s,
        4,
        Command::Edit {
            label_key: "resample".into(),
            mutations: vec![Mutation::TerrainLayerResolution {
                object,
                side_samples: 5,
            }],
        },
    )
    .unwrap();
    let snapshot = s.snapshot();
    let ObjectData::Terrain(tt) = &snapshot.document.objects[0].data else {
        panic!()
    };
    assert!(tt
        .tiles
        .iter()
        .flat_map(|t| &t.layers)
        .all(|p| p.side_samples == 5));
    send(&mut s, 5, Command::Undo).unwrap();
    assert_eq!(s.snapshot().document.as_ref(), &base);
}
#[test]
fn refused_history_capacity_commit_keeps_preview_open_until_cancel() {
    let mut tt = TerrainDocument::flat(
        256,
        [1.0, 1.0],
        vec![(StableId::from_u128(100), TileKey { x: 0, z: 0 })],
    )
    .unwrap();
    let defs = (0..128)
        .map(|i| LayerDefinition {
            id: StableId::from_u128(1000 + i),
            name: format!("mask{i}"),
            kind: LayerKind::Mask,
            color_linear_rgba: None,
        })
        .collect();
    scene_data::layers::set_registry(&mut tt, defs, None, 250).unwrap();
    let mut base = source(2);
    base.objects[0].data = ObjectData::Terrain(tt);
    let mut s = EditorSession::new(base.clone()).unwrap();
    s.mark_saved(s.source_hash());
    let txn = begin(&mut s, 1);
    let mut r = request(1000);
    r.path_world_m = vec![[0.0; 3]];
    r.radius_m = 0.1;
    assert!(!preview(&mut s, 2, txn, 1, r).texel_deltas.is_empty());
    let valid = s.snapshot().preview.as_ref().unwrap().document.clone();
    assert_eq!(
        send(
            &mut s,
            3,
            Command::CommitViewportEdit {
                transaction: txn,
                label_key: "paint".into()
            }
        )
        .unwrap_err()
        .code,
        "history.capacity"
    );
    assert_eq!(s.snapshot().document.as_ref(), &base);
    assert_eq!(s.snapshot().preview.as_ref().unwrap().document, valid);
    assert!(!s.snapshot().status.dirty);
    assert!(s.snapshot().undo_label.is_none());
    send(&mut s, 4, Command::CancelViewportEdit { transaction: txn }).unwrap();
    assert!(s.snapshot().preview.is_none());
    assert_eq!(s.snapshot().document_revision, 0);
    assert!(send(
        &mut s,
        5,
        Command::Edit {
            label_key: "no-op".into(),
            mutations: vec![]
        }
    )
    .is_ok());
}
