//! Placed particle emitters in the session: the level's schema, the inspector, and what the
//! review says of them from what it is told of the game's emitter assets.
use editor_core::*;
use scene_data::{ObjectData, ObjectRecord, RigidTransform, SCHEMA_VERSION};

fn id(n: u128) -> ObjectId {
    StableId::from_u128(n)
}
fn placement(object: u128, emitter: u128) -> ObjectRecord {
    ObjectRecord {
        id: id(object),
        parent: None,
        sibling_order: object as u32,
        name: "Sparks".into(),
        local: RigidTransform::default(),
        data: ObjectData::Emitter {
            emitter_id: id(emitter).0,
        },
    }
}
fn level() -> SceneDocument {
    SceneDocument {
        schema: SCHEMA_VERSION,
        id: id(999),
        objects: vec![ObjectRecord {
            id: id(1),
            parent: None,
            sibling_order: 0,
            name: "Spawn".into(),
            local: RigidTransform::default(),
            data: ObjectData::Spawn { order: 0 },
        }],
    }
}
fn send(session: &mut EditorSession, request: u64, command: Command) -> CommandResult {
    let view = session.snapshot();
    session.submit(CommandRequest {
        request,
        session: view.session,
        expected_revision: view.document_revision,
        expected_inspector_revision: Some(view.regions.inspector),
        command,
    })
}
fn place(session: &mut EditorSession, request: u64, object: u128, emitter: u128) {
    send(
        session,
        request,
        Command::Edit {
            label_key: "create".into(),
            mutations: vec![Mutation::Create(placement(object, emitter))],
        },
    )
    .outcome
    .unwrap();
}
fn known(assets: &[(u128, EmitterAssetState)]) -> EmitterAssets {
    EmitterAssets {
        game: true,
        assets: assets
            .iter()
            .map(|(emitter, state)| EmitterAsset {
                id: id(*emitter).0,
                state: state.clone(),
            })
            .collect(),
    }
}
/// Each problem of the review as its code, the object it is about, and one parameter.
fn problems(session: &EditorSession, parameter: &str) -> Vec<(String, Option<ObjectId>, String)> {
    session
        .snapshot()
        .review
        .iter()
        .map(|problem| {
            assert_eq!(problem.scope, ReviewScope::Play);
            (
                problem.diagnostic.code.clone(),
                problem.diagnostic.object.clone(),
                problem
                    .diagnostic
                    .parameters
                    .get(parameter)
                    .cloned()
                    .unwrap_or_default(),
            )
        })
        .collect()
}

#[test]
fn a_placement_and_its_removal_leave_the_level_of_its_one_schema_and_undo_takes_both_back() {
    let mut session = EditorSession::new(level()).unwrap();
    assert_eq!(session.snapshot().document.schema, SCHEMA_VERSION);
    place(&mut session, 1, 20, 5);
    let placed = session.snapshot();
    assert_eq!(placed.document.schema, SCHEMA_VERSION);
    assert!(placed.document.places_emitters());
    assert_eq!(placed.undo_label.as_deref(), Some("create"));

    send(
        &mut session,
        2,
        Command::Edit {
            label_key: "delete".into(),
            mutations: vec![Mutation::Delete(id(20))],
        },
    )
    .outcome
    .unwrap();
    assert_eq!(session.snapshot().document.schema, SCHEMA_VERSION);
    // Undo of the placement itself is the level as it was before it.
    send(&mut session, 3, Command::Undo).outcome.unwrap();
    send(&mut session, 4, Command::Undo).outcome.unwrap();
    let before = session.snapshot();
    assert_eq!(before.document.schema, SCHEMA_VERSION);
    assert!(!before.document.places_emitters());
}

#[test]
fn a_placed_emitter_says_which_asset_it_is_and_that_is_not_changed_in_the_inspector() {
    let mut session = EditorSession::new(level()).unwrap();
    place(&mut session, 1, 20, 5);
    place(&mut session, 2, 21, 6);
    send(&mut session, 3, Command::Select(vec![id(20)]))
        .outcome
        .unwrap();
    let one = session.snapshot();
    let fields: Vec<_> = one.inspector.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(fields, ["name", "translation", "rotation", "emitter"]);
    let emitter = one.inspector.last().unwrap();
    assert_eq!(emitter.value, FieldValue::Text(id(5).0));
    assert!(emitter.read_only && !emitter.mixed_selection);

    // Two placements of different emitters: the field is there and says they differ.
    send(&mut session, 4, Command::Select(vec![id(20), id(21)]))
        .outcome
        .unwrap();
    assert!(session.snapshot().inspector.last().unwrap().mixed_selection);
    // With another kind of object selected too it is no field of them all.
    send(&mut session, 5, Command::Select(vec![id(1), id(20)]))
        .outcome
        .unwrap();
    let mixed = session.snapshot();
    assert!(mixed.inspector.iter().all(|f| f.id != "emitter"));
}

#[test]
fn the_review_says_of_each_placement_what_stops_the_game_from_showing_it() {
    let mut session = EditorSession::new(level()).unwrap();
    assert_eq!(problems(&session, "emitter_id"), []);
    place(&mut session, 1, 22, 7);
    place(&mut session, 2, 20, 5);
    place(&mut session, 3, 21, 6);
    place(&mut session, 4, 23, 8);

    // Before the session is told anything there is no game to read an emitter from.
    assert_eq!(
        problems(&session, "emitter_id")
            .iter()
            .map(|(code, object, _)| (code.as_str(), object.clone()))
            .collect::<Vec<_>>(),
        [
            ("emitter.no_game", Some(id(20))),
            ("emitter.no_game", Some(id(21))),
            ("emitter.no_game", Some(id(22))),
            ("emitter.no_game", Some(id(23))),
        ]
    );

    // Told what the game's folder holds: one usable, one unreadable, one the runtime does
    // not draw, and one that is not there. By placement, each with its emitter.
    let before = session.snapshot();
    let hash_before = session.source_hash();
    let told = known(&[
        (5, EmitterAssetState::Usable),
        (
            6,
            EmitterAssetState::Refused {
                code: "particle.decode".into(),
            },
        ),
        (
            7,
            EmitterAssetState::Unsupported {
                field: "shape".into(),
            },
        ),
    ]);
    send(&mut session, 5, Command::SetEmitterAssets(told.clone()))
        .outcome
        .unwrap();
    assert_eq!(
        problems(&session, "emitter_id"),
        [
            ("emitter.asset_refused".into(), Some(id(21)), id(6).0),
            ("emitter.runtime_unsupported".into(), Some(id(22)), id(7).0),
            ("emitter.asset_missing".into(), Some(id(23)), id(8).0),
        ]
    );
    assert_eq!(problems(&session, "code")[0].2, "particle.decode");
    assert_eq!(problems(&session, "field")[1].2, "shape");

    // What it was told is no part of the level: nothing to save, nothing to undo.
    let after = session.snapshot();
    assert_eq!(after.document_revision, before.document_revision);
    assert_eq!(after.undo_label, before.undo_label);
    assert_eq!(session.source_hash(), hash_before);
    assert_ne!(after.regions.review, before.regions.review);
    // Told the same again, nothing changes.
    send(&mut session, 6, Command::SetEmitterAssets(told))
        .outcome
        .unwrap();
    assert_eq!(session.snapshot().regions.review, after.regions.review);

    // An edit of the level keeps what was told: the missing one is removed, the rest stay.
    send(
        &mut session,
        7,
        Command::Edit {
            label_key: "delete".into(),
            mutations: vec![Mutation::Delete(id(23))],
        },
    )
    .outcome
    .unwrap();
    assert_eq!(
        problems(&session, "emitter_id")
            .iter()
            .map(|(code, ..)| code.as_str())
            .collect::<Vec<_>>(),
        ["emitter.asset_refused", "emitter.runtime_unsupported"]
    );
}

#[test]
fn a_placement_is_duplicated_and_moved_as_any_object() {
    let mut session = EditorSession::new(level()).unwrap();
    place(&mut session, 1, 20, 5);
    send(&mut session, 2, Command::Select(vec![id(20)]))
        .outcome
        .unwrap();
    send(
        &mut session,
        3,
        Command::DuplicateObjects {
            copies: vec![DuplicateObject {
                source: id(20),
                id: id(30),
                name: "Sparks 2".into(),
                sibling_order: 40,
                local: RigidTransform {
                    translation_m: [1.0, 0.0, 0.0],
                    ..RigidTransform::default()
                },
                spawn_order: None,
            }],
            label_key: "duplicate".into(),
        },
    )
    .outcome
    .unwrap();
    let view = session.snapshot();
    let copy = view.document.object(&id(30)).unwrap();
    assert_eq!(
        copy.data,
        ObjectData::Emitter {
            emitter_id: id(5).0
        }
    );
    assert_eq!(copy.local.translation_m, [1.0, 0.0, 0.0]);
    assert_eq!(view.selection.as_ref(), [id(30)]);
}
