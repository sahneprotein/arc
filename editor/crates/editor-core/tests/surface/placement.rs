use super::*;

#[test]
fn rotated_shared_edge_distances_are_independent_of_incident_triangle_area() {
    let mut checked = 0;
    for angle in [0.1, 0.3, 0.6, 0.9, 1.1] {
        let mut source = scene(1);
        for s in &mut terrain(&mut source).samples {
            s.height_m = 0.0;
        }
        source.objects[0].local = RigidTransform {
            translation_m: [8.25, -2.3, 4.125],
            rotation_xyzw: (Quat::from_rotation_y(0.4) * Quat::from_rotation_x(angle)).to_array(),
        };
        let cooked = editor_cook::cook(&source).unwrap();
        let tile = cooked
            .simulation
            .ground
            .iter()
            .find(|t| t.key == TileKey { x: -1, z: -1 })
            .unwrap();
        let a = tile.positions_m[1].map(f64::from);
        let b = tile.positions_m[3].map(f64::from);
        let midpoint = std::array::from_fn::<_, 3, _>(|i| (a[i] + b[i]) * 0.5);
        // Only exact f32-representable midpoint rays are mathematical edge ties.
        if [0, 2]
            .iter()
            .any(|i| f64::from(midpoint[*i] as f32) != midpoint[*i])
        {
            continue;
        }
        let r = ray(
            [
                midpoint[0] as f32,
                midpoint[1] as f32 + 9.7,
                midpoint[2] as f32,
            ],
            [0.0, -1.0, 0.0],
        );
        let found = hit(&source, &r).unwrap();
        assert_eq!(
            (found.key, found.triangle_index),
            (TileKey { x: -1, z: -1 }, 1),
            "angle={angle}"
        );
        checked += 1;
    }
    assert!(checked > 0);
}
#[test]
fn sculpt_preview_query_matches_cook_without_mutating_session_and_cancel_restores_hit() {
    let mut session = EditorSession::new(scene(2)).unwrap();
    let r = ray([0.75, 20.0, 1.25], [0.0, -1.0, 0.0]);
    let original = hit(&session.snapshot().document, &r).unwrap();
    let mut send = |id, command| {
        let view = session.snapshot();
        session
            .submit(CommandRequest {
                request: id,
                session: view.session,
                expected_revision: view.document_revision,
                expected_inspector_revision: None,
                command,
            })
            .outcome
            .unwrap()
    };
    let CommandOutcome::TransactionStarted(txn) = send(
        1,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    ) else {
        panic!()
    };
    send(
        2,
        Command::PreviewTerrainSculpt {
            transaction: txn,
            sequence: 1,
            request: TerrainSculptRequest {
                object: id(99),
                path_world_m: vec![[0.75, 0.0, 1.25]],
                radius_m: 0.5,
                strength: 1.0,
                falloff: 0.0,
                mode: TerrainSculptMode::Raise { amount_m: 2.0 },
            },
        },
    );
    let snapshot = session.snapshot();
    let preview = &snapshot.preview.as_ref().unwrap().document;
    let found = hit(preview, &r).unwrap();
    assert_eq!(
        found.position_world_m[1],
        original.position_world_m[1] + 2.0
    );
    let cook = editor_cook::cook(preview).unwrap();
    let tile = cook
        .simulation
        .ground
        .iter()
        .find(|t| t.tile == found.tile)
        .unwrap();
    assert!(tile
        .positions_m
        .iter()
        .any(|p| p == &found.position_world_m));
    assert_eq!(hit(&snapshot.document, &r), Some(original.clone()));
    let before = session.snapshot();
    let _ = hit(preview, &r);
    assert_eq!(session.snapshot().view_revision, before.view_revision);
    assert_eq!(
        session.snapshot().document_revision,
        before.document_revision
    );
    assert_eq!(session.snapshot().document, before.document);
    assert_eq!(
        session.snapshot().preview.as_ref().unwrap().document,
        before.preview.as_ref().unwrap().document
    );
    let view = session.snapshot();
    session
        .submit(CommandRequest {
            request: 3,
            session: view.session,
            expected_revision: view.document_revision,
            expected_inspector_revision: None,
            command: Command::CancelViewportEdit { transaction: txn },
        })
        .outcome
        .unwrap();
    assert_eq!(hit(&session.snapshot().document, &r), Some(original));
}
