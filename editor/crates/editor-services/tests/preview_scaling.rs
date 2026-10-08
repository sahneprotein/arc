use editor_core::*;
use editor_services::*;
use scene_data::{terrain::*, *};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

fn command(
    handle: &mut EditorHandle,
    id: u64,
    command: Command,
) -> Result<CommandOutcome, Diagnostic> {
    let base = handle.snapshot();
    handle
        .submit(CommandRequest {
            request: id,
            session: base.session,
            expected_revision: base.document_revision,
            expected_inspector_revision: None,
            command,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut result = None;
    while Instant::now() < deadline {
        for event in handle.poll(128) {
            if let ServiceEvent::Command(reply) = event
                && reply.request == id
            {
                result = Some(reply.outcome);
            }
        }
        if (handle.snapshot().view_revision > base.view_revision
            || result.as_ref().is_some_and(Result::is_err))
            && let Some(result) = result.take()
        {
            return result;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("command timeout");
}
fn geometry(handle: &mut EditorHandle) -> JobOutput {
    let ticket = handle.job(JobOperation::Geometry).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        for event in handle.poll(128) {
            if let ServiceEvent::JobFinished { fact, result } = event
                && fact.id == ticket.id
            {
                return result.unwrap();
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("job timeout");
}
fn assert_current(handle: &mut EditorHandle) -> GeometryBinding {
    let JobOutput::Geometry {
        binding,
        simulation,
        visual,
        ..
    } = geometry(handle)
    else {
        panic!()
    };
    assert!(binding.matches(&handle.snapshot()));
    let view = handle.snapshot();
    let document = view
        .preview
        .as_ref()
        .map(|p| &p.document)
        .unwrap_or(&view.document);
    let full = editor_cook::cook(document).unwrap();
    assert_eq!(
        serde_json::to_vec(simulation.as_ref()).unwrap(),
        full.simulation_bytes
    );
    assert_eq!(
        serde_json::to_vec(visual.as_ref()).unwrap(),
        full.visual_bytes
    );
    binding
}
fn scene() -> SceneDocument {
    let mut source = current_map();
    source.objects.push(ObjectRecord {
        id: StableId::from_u128(9000),
        parent: None,
        sibling_order: 9000,
        name: "terrain".into(),
        local: Default::default(),
        data: ObjectData::Terrain(
            TerrainDocument::flat(
                4,
                [1.0, 1.0],
                (-2..2)
                    .flat_map(|z| {
                        (-2..2).map(move |x| {
                            (
                                StableId::from_u128(10_000 + ((z + 2) * 4 + x + 2) as u128),
                                TileKey { x, z },
                            )
                        })
                    })
                    .collect(),
            )
            .unwrap(),
        ),
    });
    source
}
fn preview(transaction: u64, sequence: u64, amount: f32) -> Command {
    Command::PreviewTerrainSculpt {
        transaction,
        sequence,
        request: TerrainSculptRequest {
            object: StableId::from_u128(9000),
            path_world_m: vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            radius_m: 2.0,
            strength: 1.0,
            falloff: 1.0,
            mode: TerrainSculptMode::Raise { amount_m: amount },
        },
    }
}
#[test]
fn latest_preview_terminal_events_binding_cancel_and_one_undo_remain_exact() {
    let source = scene();
    let mut handle = EditorHandle::start(source.clone()).unwrap();
    assert_current(&mut handle);
    let CommandOutcome::TransactionStarted(transaction) = command(
        &mut handle,
        1,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    )
    .unwrap() else {
        panic!()
    };
    let base = handle.snapshot();
    let mut accepted = vec![];
    for sequence in 1..=20 {
        handle
            .submit(CommandRequest {
                request: sequence + 1,
                session: base.session,
                expected_revision: 0,
                expected_inspector_revision: None,
                command: preview(transaction, sequence, sequence as f32 * 0.02),
            })
            .unwrap();
        accepted.push(handle.job(JobOperation::Geometry).unwrap().id);
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut finished = BTreeMap::new();
    let mut commands = 0;
    let mut last = None;
    while Instant::now() < deadline
        && (finished.len() < accepted.len()
            || handle.snapshot().preview.as_ref().map(|p| p.sequence) != Some(20))
    {
        for event in handle.poll(128) {
            match event {
                ServiceEvent::Command(reply) => {
                    reply.outcome.unwrap();
                    commands += 1;
                }
                ServiceEvent::JobFinished { fact, result } => {
                    assert!(accepted.contains(&fact.id));
                    assert!(
                        finished.insert(fact.id, ()).is_none(),
                        "duplicate terminal event"
                    );
                    match result {
                        Ok(JobOutput::Geometry { binding, .. }) => {
                            last = Some(binding);
                        }
                        Err(e) => assert_eq!(e.code, "job.cancelled"),
                        _ => panic!(),
                    }
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(commands, 20);
    assert_eq!(finished.len(), accepted.len());
    let binding = last.unwrap();
    assert_eq!(binding.preview, Some((transaction, 20)));
    assert!(binding.matches(&handle.snapshot()));
    assert_current(&mut handle);
    let valid = handle.snapshot().preview.as_ref().unwrap().document.clone();
    assert!(command(&mut handle, 25, preview(transaction, 19, 0.7)).is_err());
    assert_eq!(handle.snapshot().preview.as_ref().unwrap().document, valid);
    assert_current(&mut handle);
    command(&mut handle, 26, Command::CancelViewportEdit { transaction }).unwrap();
    assert!(!binding.matches(&handle.snapshot()));
    assert_eq!(handle.snapshot().document.as_ref(), &source);
    assert_current(&mut handle);
    let CommandOutcome::TransactionStarted(next) = command(
        &mut handle,
        27,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    )
    .unwrap() else {
        panic!()
    };
    assert!(command(&mut handle, 28, preview(transaction, 21, 0.7)).is_err());
    command(&mut handle, 29, preview(next, 1, 0.7)).unwrap();
    assert_current(&mut handle);
    let preview = handle.snapshot().preview.as_ref().unwrap().document.clone();
    command(
        &mut handle,
        30,
        Command::CommitViewportEdit {
            transaction: next,
            label_key: "sculpt".into(),
        },
    )
    .unwrap();
    assert_eq!(handle.snapshot().document, preview);
    assert_current(&mut handle);
    command(&mut handle, 31, Command::Undo).unwrap();
    assert_eq!(handle.snapshot().document.as_ref(), &source);
    assert_current(&mut handle);
    command(&mut handle, 32, Command::Redo).unwrap();
    assert_eq!(handle.snapshot().document, preview);
    assert_current(&mut handle);
}
#[test]
fn queued_geometry_cannot_cross_open_session_or_document_revision() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("empty.arcscene");
    let empty = SceneDocument {
        schema: SCHEMA_VERSION,
        id: StableId::from_u128(42),
        objects: vec![],
    };
    std::fs::write(&path, serde_json::to_vec(&empty).unwrap()).unwrap();
    let mut handle = EditorHandle::start(scene()).unwrap();
    let opened = handle.job(JobOperation::Open(path)).unwrap();
    let stale = handle.job(JobOperation::Geometry).unwrap();
    let mut results = BTreeMap::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline
        && (results.len() < 2 || handle.snapshot().document.as_ref() != &empty)
    {
        for event in handle.poll(128) {
            if let ServiceEvent::JobFinished { fact, result } = event {
                assert!(results.insert(fact.id, result).is_none());
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(results.remove(&opened.id).unwrap().is_ok());
    assert_eq!(
        results.remove(&stale.id).unwrap().unwrap_err().code,
        "job.stale"
    );
    assert_current(&mut handle);
    let base = handle.snapshot();
    handle
        .submit(CommandRequest {
            request: 70,
            session: base.session,
            expected_revision: 0,
            expected_inspector_revision: None,
            command: Command::Edit {
                label_key: "create".into(),
                mutations: vec![Mutation::Create(ObjectRecord {
                    id: StableId::from_u128(71),
                    parent: None,
                    sibling_order: 0,
                    name: "box".into(),
                    local: Default::default(),
                    data: ObjectData::Box {
                        half_extents_m: [1.0; 3],
                    },
                })],
            },
        })
        .unwrap();
    let stale = handle.job(JobOperation::Geometry).unwrap();
    let mut output = None;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline
        && (output.is_none() || handle.snapshot().document_revision != 1)
    {
        for event in handle.poll(128) {
            match event {
                ServiceEvent::Command(reply) => {
                    reply.outcome.unwrap();
                }
                ServiceEvent::JobFinished { fact, result } if fact.id == stale.id => {
                    output = Some(result)
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(output.unwrap().unwrap_err().code, "job.stale");
    assert_current(&mut handle);
}
