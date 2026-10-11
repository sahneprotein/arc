use editor_core::*;
use scene_data::{ObjectData, ObjectRecord, RigidTransform, SCHEMA_VERSION};

fn id(n: u128) -> ObjectId {
    StableId::from_u128(n)
}
fn object(n: u128, parent: Option<u128>, order: u32, data: ObjectData) -> ObjectRecord {
    ObjectRecord {
        id: id(n),
        parent: parent.map(id),
        sibling_order: order,
        name: format!("Object {n}"),
        local: RigidTransform::default(),
        data,
    }
}
fn boxes() -> SceneDocument {
    SceneDocument {
        schema: SCHEMA_VERSION,
        id: id(999),
        objects: vec![
            object(
                1,
                None,
                0,
                ObjectData::Box {
                    half_extents_m: [1., 2., 3.],
                },
            ),
            object(
                2,
                None,
                1,
                ObjectData::Box {
                    half_extents_m: [4., 5., 6.],
                },
            ),
        ],
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

#[test]
fn active_snap_and_pivot_are_session_facts_and_last_pick_is_active() {
    let mut session = EditorSession::new(boxes()).unwrap();
    session.mark_saved(session.source_hash());
    let initial = session.snapshot();
    assert_eq!(initial.snap, SnapSettings::default());
    send(&mut session, 1, Command::Select(vec![id(2), id(1)]))
        .outcome
        .unwrap();
    assert_eq!(&*session.snapshot().selection, &[id(1), id(2)]);
    assert_eq!(session.snapshot().active, Some(id(1)));
    send(&mut session, 2, Command::Select(vec![id(1), id(2)]))
        .outcome
        .unwrap();
    assert_eq!(session.snapshot().active, Some(id(2)));
    send(&mut session, 3, Command::SetActiveObject(Some(id(1))))
        .outcome
        .unwrap();
    send(&mut session, 4, Command::SetGizmoPivot(GizmoPivot::Center))
        .outcome
        .unwrap();
    let snap = SnapSettings {
        enabled: true,
        translation_m: 2.,
        rotation_degrees: 45.,
    };
    send(&mut session, 5, Command::SetSnap(snap))
        .outcome
        .unwrap();
    let view = session.snapshot();
    assert_eq!(view.snap, snap);
    assert_eq!(view.pivot, GizmoPivot::Center);
    assert_eq!(view.document_revision, initial.document_revision);
    assert_eq!(
        session.source_hash(),
        initial.status.saved_source_hash.unwrap()
    );
    assert!(!view.status.dirty);
    assert!(view.undo_label.is_none());
    assert!(view.regions.selection > initial.regions.selection);
    for value in [0., f32::NAN, 100.01] {
        assert_eq!(
            send(
                &mut session,
                6 + value.to_bits() as u64,
                Command::SetSnap(SnapSettings {
                    translation_m: value,
                    ..snap
                })
            )
            .outcome
            .unwrap_err()
            .code,
            "snap.range"
        );
        assert_eq!(session.snapshot().snap, snap);
    }
    assert_eq!(
        send(
            &mut session,
            10000000000,
            Command::SetActiveObject(Some(id(99)))
        )
        .outcome
        .unwrap_err()
        .code,
        "selection.active_not_selected"
    );
}

#[test]
fn removing_active_falls_back_to_first_sorted_remaining_id() {
    let mut session = EditorSession::new(boxes()).unwrap();
    send(&mut session, 1, Command::Select(vec![id(1), id(2)]))
        .outcome
        .unwrap();
    send(
        &mut session,
        2,
        Command::Edit {
            label_key: "delete".into(),
            mutations: vec![Mutation::Delete(id(2))],
        },
    )
    .outcome
    .unwrap();
    assert_eq!(session.snapshot().active, Some(id(1)));
    send(&mut session, 3, Command::Select(vec![]))
        .outcome
        .unwrap();
    assert_eq!(session.snapshot().active, None);
}

#[test]
fn assignment_keeps_unset_axes_and_full_size_is_one_atomic_undo_step() {
    let mut scene = boxes();
    scene.objects[0].local.translation_m = [1., 2., 3.];
    scene.objects[1].local.translation_m = [4., 5., 6.];
    let mut session = EditorSession::new(scene).unwrap();
    send(&mut session, 1, Command::Select(vec![id(1), id(2)]))
        .outcome
        .unwrap();
    let before = session.snapshot();
    send(
        &mut session,
        2,
        Command::AssignField {
            objects: vec![id(1), id(2)],
            assignment: FieldAssignment::Translation([Some(10.), None, None]),
            label_key: "translation".into(),
        },
    )
    .outcome
    .unwrap();
    assert_eq!(
        session.snapshot().document.objects[0].local.translation_m,
        [10., 2., 3.]
    );
    assert_eq!(
        session.snapshot().document.objects[1].local.translation_m,
        [10., 5., 6.]
    );
    send(
        &mut session,
        3,
        Command::AssignField {
            objects: vec![id(1), id(2)],
            assignment: FieldAssignment::Size([Some(8.), None, None]),
            label_key: "size".into(),
        },
    )
    .outcome
    .unwrap();
    assert_eq!(
        session.snapshot().document.objects[0].data,
        ObjectData::Box {
            half_extents_m: [4., 2., 3.]
        }
    );
    assert_eq!(
        session.snapshot().document.objects[1].data,
        ObjectData::Box {
            half_extents_m: [4., 5., 6.]
        }
    );
    send(&mut session, 4, Command::Undo).outcome.unwrap();
    assert_eq!(
        session.snapshot().document.objects[0].data,
        before.document.objects[0].data
    );
    send(&mut session, 5, Command::Undo).outcome.unwrap();
    assert_eq!(session.snapshot().document, before.document);
}

#[test]
fn assignment_requires_inspector_fence_and_refuses_mixed_kinds_atomically() {
    let mut scene = boxes();
    scene.objects[1].data = ObjectData::Spawn { order: 0 };
    let mut session = EditorSession::new(scene).unwrap();
    send(&mut session, 1, Command::Select(vec![id(1), id(2)]))
        .outcome
        .unwrap();
    let before = session.snapshot();
    assert!(!before
        .inspector
        .iter()
        .any(|field| field.id == "half_extents"));
    let command = Command::AssignField {
        objects: vec![id(1), id(2)],
        assignment: FieldAssignment::Size([Some(9.), None, None]),
        label_key: "size".into(),
    };
    assert_eq!(
        session
            .submit(CommandRequest {
                request: 2,
                session: before.session,
                expected_revision: before.document_revision,
                expected_inspector_revision: None,
                command: command.clone()
            })
            .outcome
            .unwrap_err()
            .code,
        "inspector.revision_required"
    );
    assert_eq!(
        send(&mut session, 3, command.clone())
            .outcome
            .unwrap_err()
            .code,
        "field.kind"
    );
    assert_eq!(session.snapshot().document, before.document);
    send(&mut session, 4, Command::Select(vec![id(1)]))
        .outcome
        .unwrap();
    assert_eq!(
        session
            .submit(CommandRequest {
                request: 5,
                session: before.session,
                expected_revision: before.document_revision,
                expected_inspector_revision: Some(before.regions.inspector),
                command
            })
            .outcome
            .unwrap_err()
            .code,
        "inspector.stale"
    );
}

fn tree() -> SceneDocument {
    let mut scene = boxes();
    scene.objects[0].data = ObjectData::Group;
    scene.objects[0].local.translation_m = [10., 0., 0.];
    scene.objects[1].parent = Some(id(1));
    scene.objects[1].sibling_order = 0;
    scene.objects[1].local.translation_m = [2., 1., 3.];
    scene
        .objects
        .push(object(3, Some(1), 1, ObjectData::Spawn { order: 0 }));
    scene
}
fn copies(scene: &SceneDocument) -> Vec<DuplicateObject> {
    scene
        .objects
        .iter()
        .map(|source| {
            let mut local = source.local.clone();
            if source.parent.is_none() {
                local.translation_m[0] += 1.;
            }
            DuplicateObject {
                source: source.id.clone(),
                id: id(u128::from_str_radix(&source.id.0, 16).unwrap() + 100),
                name: if source.parent.is_none() {
                    format!("Copy of {}", source.name)
                } else {
                    source.name.clone()
                },
                sibling_order: if source.parent.is_none() {
                    1
                } else {
                    source.sibling_order
                },
                spawn_order: matches!(source.data, ObjectData::Spawn { .. }).then_some(1),
                local,
            }
        })
        .collect()
}

#[test]
fn duplication_clones_selected_subtree_once_and_remaps_active_root() {
    let scene = tree();
    let copies = copies(&scene);
    let mut session = EditorSession::new(scene).unwrap();
    send(&mut session, 1, Command::Select(vec![id(1), id(2), id(3)]))
        .outcome
        .unwrap();
    let before = session.snapshot();
    let request = CommandRequest {
        request: 2,
        session: before.session,
        expected_revision: 0,
        expected_inspector_revision: None,
        command: Command::DuplicateObjects {
            copies,
            label_key: "duplicate".into(),
        },
    };
    session.submit(request.clone()).outcome.unwrap();
    assert_eq!(session.submit(request.clone()), session.submit(request));
    let view = session.snapshot();
    assert_eq!(view.document.objects.len(), 6);
    assert_eq!(&*view.selection, &[id(101)]);
    assert_eq!(view.active, Some(id(101)));
    assert_eq!(
        view.document.object(&id(101)).unwrap().local.translation_m,
        [11., 0., 0.]
    );
    assert_eq!(
        view.document.object(&id(102)).unwrap().parent,
        Some(id(101))
    );
    assert_eq!(
        view.document.object(&id(102)).unwrap().local,
        before.document.objects[1].local
    );
    assert_eq!(
        view.document.object(&id(103)).unwrap().data,
        ObjectData::Spawn { order: 1 }
    );
    send(&mut session, 3, Command::Undo).outcome.unwrap();
    assert_eq!(session.snapshot().document, before.document);
    assert!(session.snapshot().active.is_none());
    send(&mut session, 4, Command::Redo).outcome.unwrap();
    assert_eq!(session.snapshot().document, view.document);
}

#[test]
fn duplication_rejects_incomplete_ids_orders_and_terrain_without_partial_changes() {
    let scene = tree();
    let specs = copies(&scene);
    let mut session = EditorSession::new(scene).unwrap();
    send(&mut session, 1, Command::Select(vec![id(1)]))
        .outcome
        .unwrap();
    let before = session.snapshot();
    let mut reused = specs.clone();
    reused[0].id = id(2);
    let mut order = specs.clone();
    order[0].sibling_order = 0;
    let mut spawn = specs.clone();
    spawn[2].spawn_order = Some(0);
    for (request, copies, code) in [
        (2, vec![specs[0].clone()], "duplicate.incomplete"),
        (3, reused, "id.invalid_or_duplicate"),
        (4, order, "duplicate.append_order"),
        (5, spawn, "spawn.duplicate_order"),
    ] {
        assert_eq!(
            send(
                &mut session,
                request,
                Command::DuplicateObjects {
                    copies,
                    label_key: "duplicate".into()
                }
            )
            .outcome
            .unwrap_err()
            .code,
            code
        );
        assert_eq!(session.snapshot().document, before.document);
        assert_eq!(session.snapshot().selection, before.selection);
    }
    let mut scene = boxes();
    scene.objects[0].data = ObjectData::Terrain(
        scene_data::terrain::TerrainDocument::flat(
            1,
            [1., 1.],
            vec![(id(50), scene_data::terrain::TileKey { x: 0, z: 0 })],
        )
        .unwrap(),
    );
    let mut specs = copies(&scene);
    specs.truncate(1);
    specs[0].sibling_order = 2;
    let mut session = EditorSession::new(scene).unwrap();
    send(&mut session, 1, Command::Select(vec![id(1)]))
        .outcome
        .unwrap();
    let error = send(
        &mut session,
        2,
        Command::DuplicateObjects {
            copies: specs,
            label_key: "duplicate".into(),
        },
    )
    .outcome
    .unwrap_err();
    assert_eq!(error.code, "duplicate.terrain_unsupported");
    assert_eq!(error.object, Some(id(1)));
}

#[test]
fn keep_world_reparent_preserves_transforms_and_appends_in_drag_order() {
    let mut scene = boxes();
    let mut parent = object(3, None, 2, ObjectData::Group);
    parent.local.translation_m = [10., 3., 4.];
    parent.local.rotation_xyzw = glam::Quat::from_rotation_y(1.).to_array();
    scene.objects.push(parent);
    let before_world = scene_data::world_transforms(&scene).unwrap();
    let mut session = EditorSession::new(scene).unwrap();
    send(
        &mut session,
        1,
        Command::ReparentObjects {
            objects: vec![id(2), id(1)],
            parent: Some(id(3)),
            keep: TransformRetention::KeepWorld,
        },
    )
    .outcome
    .unwrap();
    let view = session.snapshot();
    let after = scene_data::world_transforms(&view.document).unwrap();
    assert_eq!(view.document.object(&id(2)).unwrap().sibling_order, 0);
    assert_eq!(view.document.object(&id(1)).unwrap().sibling_order, 1);
    for id in [id(1), id(2)] {
        for (a, b) in before_world[&id]
            .translation_m
            .into_iter()
            .zip(after[&id].translation_m)
        {
            assert!((a - b).abs() < 1e-5);
        }
        for (a, b) in before_world[&id]
            .rotation_xyzw
            .into_iter()
            .zip(after[&id].rotation_xyzw)
        {
            assert!((a - b).abs() < 1e-5);
        }
    }
    send(&mut session, 2, Command::Undo).outcome.unwrap();
    assert!(session
        .snapshot()
        .document
        .object(&id(1))
        .unwrap()
        .parent
        .is_none());
}

#[test]
fn reparent_cycle_is_atomic_and_keep_local_preserves_child_locals() {
    let scene = tree();
    let mut session = EditorSession::new(scene).unwrap();
    let before = session.snapshot();
    assert_eq!(
        send(
            &mut session,
            1,
            Command::ReparentObjects {
                objects: vec![id(1)],
                parent: Some(id(2)),
                keep: TransformRetention::KeepWorld
            }
        )
        .outcome
        .unwrap_err()
        .code,
        "hierarchy.cycle"
    );
    assert_eq!(session.snapshot().document, before.document);
    send(
        &mut session,
        2,
        Command::ReparentObjects {
            objects: vec![id(2)],
            parent: None,
            keep: TransformRetention::KeepLocal,
        },
    )
    .outcome
    .unwrap();
    assert_eq!(
        session.snapshot().document.object(&id(2)).unwrap().local,
        before.document.objects[1].local
    );
}
