use editor_core::*;
use scene_data::{current_map, RigidTransform};
fn send(session: &mut EditorSession, id: u64, command: Command) -> CommandResult {
    let view = session.snapshot();
    session.submit(CommandRequest {
        request: id,
        session: view.session,
        expected_revision: view.document_revision,
        expected_inspector_revision: None,
        command,
    })
}
#[test]
fn invalid_batch_is_atomic_and_requests_revisions_are_fenced() {
    let mut session = EditorSession::new(current_map()).unwrap();
    let old = session.snapshot();
    let id = old.document.objects[0].id.clone();
    let bad = send(
        &mut session,
        1,
        Command::Edit {
            label_key: "edit".into(),
            mutations: vec![
                Mutation::Rename {
                    object: id.clone(),
                    name: "Valid".into(),
                },
                Mutation::Transform {
                    object: id.clone(),
                    value: RigidTransform {
                        rotation_xyzw: [0.0; 4],
                        ..Default::default()
                    },
                },
            ],
        },
    );
    assert!(bad.outcome.is_err());
    assert_eq!(session.snapshot().document, old.document);
    assert_eq!(session.snapshot().document_revision, 0);
    let request = CommandRequest {
        request: 2,
        session: old.session,
        expected_revision: 0,
        expected_inspector_revision: None,
        command: Command::Edit {
            label_key: "rename".into(),
            mutations: vec![Mutation::Rename {
                object: id,
                name: "Ground 2".into(),
            }],
        },
    };
    assert!(session.submit(request.clone()).outcome.is_ok());
    let current = session.snapshot();
    assert_eq!(current.document_revision, 1);
    assert_eq!(session.submit(request.clone()), session.submit(request));
    assert_eq!(session.snapshot().document_revision, 1);
    assert_eq!(
        session
            .submit(CommandRequest {
                request: 3,
                session: old.session,
                expected_revision: 0,
                expected_inspector_revision: None,
                command: Command::Undo
            })
            .outcome
            .unwrap_err()
            .code,
        "revision.stale"
    );
    assert_eq!(old.document.objects[0].name, "Ground");
}
#[test]
fn undo_restores_semantic_savepoint_and_selection_never_dirties_it() {
    let mut session = EditorSession::new(current_map()).unwrap();
    session.mark_saved(session.source_hash());
    let id = session.snapshot().document.objects[0].id.clone();
    send(&mut session, 1, Command::Select(vec![id.clone()]));
    assert!(!session.snapshot().status.dirty);
    send(
        &mut session,
        2,
        Command::Edit {
            label_key: "rename".into(),
            mutations: vec![Mutation::Rename {
                object: id.clone(),
                name: "Other".into(),
            }],
        },
    );
    assert!(session.snapshot().status.dirty);
    assert_eq!(session.snapshot().undo_label.as_deref(), Some("rename"));
    send(&mut session, 3, Command::Undo);
    assert!(!session.snapshot().status.dirty);
    send(&mut session, 4, Command::Redo);
    assert!(session.snapshot().status.dirty);
    assert_eq!(
        session.snapshot().document.object(&id).unwrap().name,
        "Other"
    );
    send(&mut session, 5, Command::Undo);
    send(
        &mut session,
        6,
        Command::Edit {
            label_key: "branch".into(),
            mutations: vec![Mutation::Rename {
                object: id,
                name: "New branch".into(),
            }],
        },
    );
    assert!(session.snapshot().redo_label.is_none());
}
#[test]
fn viewport_preview_cancel_and_commit_leave_exactly_one_history_entry() {
    let mut session = EditorSession::new(current_map()).unwrap();
    session.mark_saved(session.source_hash());
    let before = session.snapshot();
    let id = before.document.objects[0].id.clone();
    let begin = send(
        &mut session,
        1,
        Command::BeginViewportEdit {
            selection_revision: before.regions.selection,
        },
    );
    let CommandOutcome::TransactionStarted(transaction) = begin.outcome.unwrap() else {
        panic!()
    };
    for (sequence, name) in [(1, "first"), (2, "second")] {
        send(
            &mut session,
            sequence + 1,
            Command::Preview {
                transaction,
                sequence,
                mutations: vec![Mutation::Rename {
                    object: id.clone(),
                    name: name.into(),
                }],
            },
        );
    }
    assert_eq!(session.snapshot().document, before.document);
    assert!(!session.snapshot().status.dirty);
    assert_eq!(
        session
            .snapshot()
            .preview
            .as_ref()
            .unwrap()
            .document
            .object(&id)
            .unwrap()
            .name,
        "second"
    );
    assert_eq!(
        send(
            &mut session,
            4,
            Command::Preview {
                transaction,
                sequence: 1,
                mutations: vec![]
            }
        )
        .outcome
        .unwrap_err()
        .code,
        "preview.stale"
    );
    send(&mut session, 5, Command::CancelViewportEdit { transaction });
    assert_eq!(session.snapshot().document, before.document);
    assert!(session.snapshot().undo_label.is_none());
    let CommandOutcome::TransactionStarted(transaction) = send(
        &mut session,
        6,
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
        7,
        Command::Preview {
            transaction,
            sequence: 1,
            mutations: vec![Mutation::Rename {
                object: id.clone(),
                name: "Committed".into(),
            }],
        },
    );
    send(
        &mut session,
        8,
        Command::CommitViewportEdit {
            transaction,
            label_key: "drag".into(),
        },
    );
    assert_eq!(session.snapshot().document_revision, 1);
    send(&mut session, 9, Command::Undo);
    assert_eq!(session.snapshot().document, before.document);
    assert!(session.snapshot().undo_label.is_none());
}
#[test]
fn unchanged_regions_keep_arc_identity_and_inspector_fences_selection() {
    let mut session = EditorSession::new(current_map()).unwrap();
    let before = session.snapshot();
    let id = before.document.objects[0].id.clone();
    send(&mut session, 1, Command::Select(vec![id.clone()]));
    let selected = session.snapshot();
    assert!(std::sync::Arc::ptr_eq(
        &before.hierarchy,
        &selected.hierarchy
    ));
    let stale = session.submit(CommandRequest {
        request: 2,
        session: selected.session,
        expected_revision: 0,
        expected_inspector_revision: Some(0),
        command: Command::Edit {
            label_key: "edit".into(),
            mutations: vec![Mutation::Rename {
                object: id,
                name: "Stale field".into(),
            }],
        },
    });
    assert_eq!(stale.outcome.unwrap_err().code, "inspector.stale");
}
