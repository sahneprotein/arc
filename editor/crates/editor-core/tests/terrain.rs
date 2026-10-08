use editor_core::*;
use scene_data::{terrain::*, *};
fn send(s: &mut EditorSession, id: u64, command: Command) -> CommandResult {
    let v = s.snapshot();
    s.submit(CommandRequest {
        request: id,
        session: v.session,
        expected_revision: v.document_revision,
        expected_inspector_revision: None,
        command,
    })
}

#[test]
fn sculpt_preview_uses_ordered_global_samples_and_commits_as_one_undo() {
    let t = TerrainDocument::flat(
        2,
        [1.0, 1.0],
        vec![
            (StableId::from_u128(150), TileKey { x: 0, z: 0 }),
            (StableId::from_u128(151), TileKey { x: 1, z: 0 }),
        ],
    )
    .unwrap();
    let object = StableId::from_u128(149);
    let mut scene = current_map();
    scene.objects.push(ObjectRecord {
        id: object.clone(),
        parent: None,
        sibling_order: 149,
        name: "Sculpt".into(),
        local: Default::default(),
        data: ObjectData::Terrain(t),
    });
    let base = scene.clone();
    let request = TerrainSculptRequest {
        object: object.clone(),
        path_world_m: vec![[1.5, 0.0, 0.5], [0.5, 0.0, 0.5]],
        radius_m: 0.8,
        strength: 1.0,
        falloff: 1.0,
        mode: TerrainSculptMode::Raise { amount_m: 2.0 },
    };
    let expected = terrain_sculpt(&base, &request).unwrap();
    assert!(expected
        .sample_deltas
        .windows(2)
        .all(|w| w[0].key < w[1].key));
    assert!(expected.sample_deltas.iter().any(|d| d.key.x == 2));
    let mut reversed = request.clone();
    reversed.path_world_m.reverse();
    assert_eq!(terrain_sculpt(&base, &reversed).unwrap(), expected);

    let mut session = EditorSession::new(base.clone()).unwrap();
    let CommandOutcome::TransactionStarted(transaction) = send(
        &mut session,
        1,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    )
    .outcome
    .unwrap() else {
        panic!()
    };
    let outcome = send(
        &mut session,
        2,
        Command::PreviewTerrainSculpt {
            transaction,
            sequence: 1,
            request,
        },
    )
    .outcome
    .unwrap();
    let CommandOutcome::TerrainSculpted { sequence, result } = outcome else {
        panic!()
    };
    assert_eq!(sequence, 1);
    assert_eq!(result.sample_deltas, expected.sample_deltas);
    assert_eq!(session.snapshot().document.as_ref(), &base);
    assert!(session.snapshot().preview.is_some());
    send(
        &mut session,
        3,
        Command::CommitViewportEdit {
            transaction,
            label_key: "terrain.sculpt".into(),
        },
    );
    assert_eq!(session.snapshot().document_revision, 1);
    send(&mut session, 4, Command::Undo);
    assert_eq!(session.snapshot().document.as_ref(), &base);

    let CommandOutcome::TransactionStarted(transaction) = send(
        &mut session,
        5,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    )
    .outcome
    .unwrap() else {
        panic!()
    };
    send(
        &mut session,
        6,
        Command::PreviewTerrainSculpt {
            transaction,
            sequence: 1,
            request: TerrainSculptRequest {
                object: object.clone(),
                path_world_m: vec![[1.0, 0.0, 0.5]],
                radius_m: 0.8,
                strength: 1.0,
                falloff: 0.5,
                mode: TerrainSculptMode::Lower { amount_m: 3.0 },
            },
        },
    );
    send(&mut session, 7, Command::CancelViewportEdit { transaction });
    assert_eq!(session.snapshot().document.as_ref(), &base);
}

#[test]
fn sculpt_bounds_track_each_global_axis_across_diagonally_separated_tiles() {
    let t = TerrainDocument::flat(
        1,
        [1.0, 1.0],
        vec![
            (StableId::from_u128(160), TileKey { x: -20, z: 100 }),
            (StableId::from_u128(161), TileKey { x: 30, z: -100 }),
        ],
    )
    .unwrap();
    let object = StableId::from_u128(159);
    let mut scene = current_map();
    scene.objects.push(ObjectRecord {
        id: object.clone(),
        parent: None,
        sibling_order: 159,
        name: "Diagonal tiles".into(),
        local: Default::default(),
        data: ObjectData::Terrain(t),
    });
    let result = terrain_sculpt(
        &scene,
        &TerrainSculptRequest {
            object,
            path_world_m: vec![[-20.0, 0.0, 100.0]],
            radius_m: 0.6,
            strength: 1.0,
            falloff: 0.0,
            mode: TerrainSculptMode::Raise { amount_m: 1.0 },
        },
    )
    .unwrap();
    assert!(result
        .sample_deltas
        .iter()
        .any(|sample| sample.key == SampleKey { x: -20, z: 100 }));
}
#[test]
fn border_stroke_is_one_atomic_undo_and_cancel_never_changes_authority() {
    let t = TerrainDocument::flat(
        2,
        [1.0, 1.0],
        vec![
            (StableId::from_u128(50), TileKey { x: 0, z: 0 }),
            (StableId::from_u128(51), TileKey { x: 1, z: 0 }),
        ],
    )
    .unwrap();
    let object = StableId::from_u128(49);
    let mut scene = current_map();
    scene.objects.push(ObjectRecord {
        id: object.clone(),
        parent: None,
        sibling_order: 49,
        name: "Ground tiles".into(),
        local: Default::default(),
        data: ObjectData::Terrain(t),
    });
    let mut session = EditorSession::new(scene).unwrap();
    let base = session.snapshot();
    let CommandOutcome::TransactionStarted(transaction) = send(
        &mut session,
        1,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    )
    .outcome
    .unwrap() else {
        panic!()
    };
    let mutations = vec![Mutation::TerrainSamples {
        object: object.clone(),
        values: vec![
            (SampleKey { x: 2, z: 1 }, 2.0),
            (SampleKey { x: 3, z: 1 }, 1.0),
        ],
    }];
    send(
        &mut session,
        2,
        Command::Preview {
            transaction,
            sequence: 1,
            mutations: mutations.clone(),
        },
    );
    assert_eq!(session.snapshot().preview.as_ref().unwrap().dirty.len(), 2);
    assert_eq!(session.snapshot().document, base.document);
    assert!(send(
        &mut session,
        3,
        Command::Preview {
            transaction,
            sequence: 2,
            mutations: vec![Mutation::TerrainSamples {
                object: object.clone(),
                values: vec![(SampleKey { x: 999, z: 1 }, 0.0)]
            }]
        }
    )
    .outcome
    .is_err());
    assert_eq!(session.snapshot().preview.as_ref().unwrap().sequence, 1);
    send(&mut session, 4, Command::CancelViewportEdit { transaction });
    assert_eq!(session.snapshot().document, base.document);
    let CommandOutcome::TransactionStarted(transaction) = send(
        &mut session,
        5,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    )
    .outcome
    .unwrap() else {
        panic!()
    };
    send(
        &mut session,
        6,
        Command::Preview {
            transaction,
            sequence: 1,
            mutations,
        },
    );
    send(
        &mut session,
        7,
        Command::CommitViewportEdit {
            transaction,
            label_key: "brush".into(),
        },
    );
    assert_eq!(session.snapshot().document_revision, 1);
    send(&mut session, 8, Command::Undo);
    assert_eq!(session.snapshot().document, base.document);
    assert!(session.snapshot().undo_label.is_none());
}

#[test]
fn layer_stroke_across_tiles_updates_dirty_regions_and_is_atomic() {
    let mut t = TerrainDocument::flat(
        2,
        [1.0, 1.0],
        vec![
            (StableId::from_u128(50), TileKey { x: 0, z: 0 }),
            (StableId::from_u128(51), TileKey { x: 1, z: 0 }),
        ],
    )
    .unwrap();
    let group = StableId::from_u128(70);
    let base = StableId::from_u128(71);
    let paint = StableId::from_u128(72);
    scene_data::layers::set_registry(
        &mut t,
        vec![
            LayerDefinition {
                id: base,
                name: "Base".into(),
                kind: LayerKind::BlendWeight {
                    group: group.clone(),
                    default_weight: 1.0,
                },
                color_linear_rgba: None,
            },
            LayerDefinition {
                id: paint.clone(),
                name: "Paint".into(),
                kind: LayerKind::BlendWeight {
                    group: group.clone(),
                    default_weight: 0.0,
                },
                color_linear_rgba: None,
            },
        ],
        Some(group),
        2,
    )
    .unwrap();
    let object = StableId::from_u128(49);
    let mut scene = current_map();
    scene.objects.push(ObjectRecord {
        id: object.clone(),
        parent: None,
        sibling_order: 49,
        name: "Terrain".into(),
        local: Default::default(),
        data: ObjectData::Terrain(t),
    });
    let mut session = EditorSession::new(scene).unwrap();
    let before = session.snapshot();
    let mutations = vec![
        Mutation::TerrainLayer {
            object: object.clone(),
            tile: StableId::from_u128(50),
            layer: paint.clone(),
            values: vec![(1, 0.5)],
        },
        Mutation::TerrainLayer {
            object: object.clone(),
            tile: StableId::from_u128(51),
            layer: paint.clone(),
            values: vec![(0, 0.5)],
        },
    ];
    let CommandOutcome::TransactionStarted(transaction) = send(
        &mut session,
        1,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    )
    .outcome
    .unwrap() else {
        panic!()
    };
    send(
        &mut session,
        2,
        Command::Preview {
            transaction,
            sequence: 1,
            mutations: mutations.clone(),
        },
    );
    assert_eq!(session.snapshot().preview.as_ref().unwrap().dirty.len(), 2);
    assert_eq!(session.snapshot().document, before.document);
    let mut invalid = mutations.clone();
    invalid.push(Mutation::TerrainLayer {
        object,
        tile: StableId::from_u128(51),
        layer: paint,
        values: vec![(0, -1.0)],
    });
    assert!(send(
        &mut session,
        3,
        Command::Preview {
            transaction,
            sequence: 2,
            mutations: invalid
        }
    )
    .outcome
    .is_err());
    assert_eq!(session.snapshot().preview.as_ref().unwrap().sequence, 1);
    send(
        &mut session,
        4,
        Command::CommitViewportEdit {
            transaction,
            label_key: "paint".into(),
        },
    );
    send(&mut session, 5, Command::Undo);
    assert_eq!(session.snapshot().document, before.document);
    assert!(session.snapshot().undo_label.is_none());
}
