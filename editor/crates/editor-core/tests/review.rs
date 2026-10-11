use editor_core::*;
use scene_data::{ObjectData, ObjectRecord, RigidTransform, SCHEMA_VERSION};
fn id(n: u128) -> ObjectId {
    StableId::from_u128(n)
}
fn scene() -> SceneDocument {
    SceneDocument {
        schema: SCHEMA_VERSION,
        id: id(99),
        objects: vec![
            ObjectRecord {
                id: id(1),
                parent: None,
                sibling_order: 0,
                name: "Box".into(),
                local: RigidTransform::default(),
                data: ObjectData::Box {
                    half_extents_m: [1.; 3],
                },
            },
            ObjectRecord {
                id: id(2),
                parent: None,
                sibling_order: 1,
                name: "Spawn".into(),
                local: RigidTransform::default(),
                data: ObjectData::Spawn { order: 0 },
            },
        ],
    }
}
#[test]
fn independent_errors_are_aggregated_stable_and_real_validator_codes() {
    let mut scene = scene();
    scene.objects[0].name = "bad\nname".into();
    scene.objects[0].local.translation_m[0] = f32::INFINITY;
    scene.objects[0].data = ObjectData::Box {
        half_extents_m: [-1.; 3],
    };
    scene.objects[1].parent = Some(id(500));
    let diagnostics = scene_data::validation_diagnostics(&scene);
    for code in [
        "name.invalid",
        "transform.nonfinite",
        "box.extents",
        "reference.missing_parent",
    ] {
        assert!(diagnostics.iter().any(|d| d.code == code), "{code}");
    }
    assert_eq!(scene_data::validate(&scene).unwrap_err(), diagnostics[0]);
    let review = review_document(&scene);
    scene.objects.reverse();
    assert_eq!(review_document(&scene), review);
    assert!(review
        .iter()
        .filter(|p| p.scope == ReviewScope::Scene)
        .all(|p| p.diagnostic.object.is_some()));
}
#[test]
fn play_producer_lists_every_reason_and_missing_spawn_without_guessing_outputs() {
    let facts = PlayCapabilityFacts {
        spawns: 0,
        boxes: 100001,
        terrain: vec![
            ProceduralTerrainFacts {
                object: id(2),
                rules: 2,
                decoration: 3,
                derived_layers: 4,
            },
            ProceduralTerrainFacts {
                object: id(1),
                rules: 1,
                decoration: 0,
                derived_layers: 0,
            },
        ],
    };
    let problems = play_problems(&facts);
    assert_eq!(problems.len(), 6);
    assert_eq!(problems[0].diagnostic.object, Some(id(1)));
    assert_eq!(problems[0].diagnostic.parameters["reason"], "rules");
    assert_eq!(problems[1].diagnostic.parameters["count"], "2");
    assert_eq!(problems[2].diagnostic.parameters["reason"], "decoration");
    assert_eq!(
        problems[3].diagnostic.parameters["reason"],
        "derived_layers"
    );
    assert_eq!(problems[4].diagnostic.code, "play.spawn_required");
    assert_eq!(problems[5].diagnostic.code, "play.runtime_limit");
    assert!(problems.iter().all(|p| p.scope == ReviewScope::Play));
    assert!(play_problems(&PlayCapabilityFacts {
        spawns: 1,
        ..Default::default()
    })
    .is_empty());
}
#[test]
fn snapshot_review_refreshes_after_edit_undo_redo_but_not_selection() {
    let mut session = EditorSession::new(scene()).unwrap();
    let before = session.snapshot();
    assert!(before.review.is_empty());
    let send = |session: &mut EditorSession, request, command| {
        let view = session.snapshot();
        session
            .submit(CommandRequest {
                request,
                session: view.session,
                expected_revision: view.document_revision,
                expected_inspector_revision: None,
                command,
            })
            .outcome
            .unwrap()
    };
    send(&mut session, 1, Command::Select(vec![id(1)]));
    assert_eq!(session.snapshot().regions.review, before.regions.review);
    send(
        &mut session,
        2,
        Command::Edit {
            label_key: "delete".into(),
            mutations: vec![Mutation::Delete(id(2))],
        },
    );
    let review = session.snapshot();
    assert!(review.regions.review > before.regions.review);
    assert_eq!(review.review[0].diagnostic.code, "play.spawn_required");
    send(&mut session, 3, Command::Undo);
    assert!(session.snapshot().review.is_empty());
    send(&mut session, 4, Command::Redo);
    assert_eq!(session.snapshot().review, review.review);
}
