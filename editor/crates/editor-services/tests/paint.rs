use editor_core::*;
use editor_services::*;
use scene_data::{terrain::*, *};
use std::time::{Duration, Instant};
fn id(n: u128) -> StableId {
    StableId::from_u128(n)
}
fn command(handle: &mut EditorHandle, n: u64, cmd: Command) -> Result<CommandOutcome, Diagnostic> {
    let base = handle.snapshot();
    handle
        .submit(CommandRequest {
            request: n,
            session: base.session,
            expected_revision: base.document_revision,
            expected_inspector_revision: None,
            command: cmd,
        })
        .unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    let mut result = None;
    while Instant::now() < end {
        for event in handle.poll(128) {
            if let ServiceEvent::Command(reply) = event
                && reply.request == n
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
    panic!("command timeout")
}
fn geometry(handle: &mut EditorHandle) -> GeometryBinding {
    let ticket = handle.job(JobOperation::Geometry).unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        for event in handle.poll(128) {
            if let ServiceEvent::JobFinished { fact, result } = event
                && fact.id == ticket.id
            {
                let JobOutput::Geometry {
                    binding,
                    simulation,
                    visual,
                    ..
                } = result.unwrap()
                else {
                    panic!()
                };
                let snapshot = handle.snapshot();
                assert!(binding.matches(&snapshot));
                let source = snapshot
                    .preview
                    .as_ref()
                    .map(|p| &p.document)
                    .unwrap_or(&snapshot.document);
                let full = editor_cook::cook(source).unwrap();
                assert_eq!(
                    serde_json::to_vec(simulation.as_ref()).unwrap(),
                    full.simulation_bytes
                );
                assert_eq!(
                    serde_json::to_vec(visual.as_ref()).unwrap(),
                    full.visual_bytes
                );
                return binding;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("geometry timeout")
}
#[test]
fn worker_paint_preview_geometry_is_fenced_through_refusal_cancel_commit_and_undo() {
    let mut t = TerrainDocument::flat(
        2,
        [1.0, 1.0],
        vec![
            (id(100), TileKey { x: 0, z: 0 }),
            (id(101), TileKey { x: 1, z: 0 }),
        ],
    )
    .unwrap();
    let defs = (0..2)
        .map(|i| LayerDefinition {
            id: id(200 + i),
            name: format!("layer{i}"),
            kind: LayerKind::BlendWeight {
                group: id(300),
                default_weight: if i == 0 { 1.0 } else { 0.0 },
            },
            color_linear_rgba: Some([i as f32, 0.0, 1.0 - i as f32, 1.0]),
        })
        .collect();
    scene_data::layers::set_registry(&mut t, defs, Some(id(300)), 3).unwrap();
    let source = SceneDocument {
        schema: SCHEMA_VERSION,
        id: id(1),
        objects: vec![ObjectRecord {
            id: id(2),
            parent: None,
            sibling_order: 0,
            name: "terrain".into(),
            local: Default::default(),
            data: ObjectData::Terrain(t),
        }],
    };
    let mut handle = EditorHandle::start(source.clone()).unwrap();
    let original = geometry(&mut handle);
    let CommandOutcome::TransactionStarted(txn) = command(
        &mut handle,
        1,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    )
    .unwrap() else {
        panic!()
    };
    let request = TerrainPaintRequest {
        object: id(2),
        layer: id(201),
        path_world_m: vec![[2.0, 0.0, 1.0]],
        radius_m: 1.5,
        strength: 0.6,
        falloff: 1.0,
        mode: TerrainPaintMode::Paint,
    };
    let preview = Command::PreviewTerrainPaint {
        transaction: txn,
        sequence: 1,
        request: request.clone(),
    };
    let CommandOutcome::TerrainPainted { result, .. } =
        command(&mut handle, 2, preview.clone()).unwrap()
    else {
        panic!()
    };
    assert_eq!(result.dirty_regions.len(), 2);
    let binding = geometry(&mut handle);
    assert_ne!(binding, original);
    assert!(!original.matches(&handle.snapshot()));
    assert_eq!(
        command(&mut handle, 3, preview).unwrap_err().code,
        "preview.stale"
    );
    assert!(binding.matches(&handle.snapshot()));
    let mut bad = request.clone();
    bad.strength = 1.1;
    assert_eq!(
        command(
            &mut handle,
            4,
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
    assert!(binding.matches(&handle.snapshot()));
    command(
        &mut handle,
        5,
        Command::CancelViewportEdit { transaction: txn },
    )
    .unwrap();
    assert!(!binding.matches(&handle.snapshot()));
    geometry(&mut handle);
    assert_eq!(handle.snapshot().document.as_ref(), &source);
    let CommandOutcome::TransactionStarted(txn) = command(
        &mut handle,
        6,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    )
    .unwrap() else {
        panic!()
    };
    command(
        &mut handle,
        7,
        Command::PreviewTerrainPaint {
            transaction: txn,
            sequence: 1,
            request,
        },
    )
    .unwrap();
    let preview_binding = geometry(&mut handle);
    command(
        &mut handle,
        8,
        Command::CommitViewportEdit {
            transaction: txn,
            label_key: "paint".into(),
        },
    )
    .unwrap();
    assert!(!preview_binding.matches(&handle.snapshot()));
    geometry(&mut handle);
    assert_eq!(handle.snapshot().undo_label.as_deref(), Some("paint"));
    command(&mut handle, 9, Command::Undo).unwrap();
    geometry(&mut handle);
    assert_eq!(handle.snapshot().document.as_ref(), &source);
    assert!(handle.snapshot().undo_label.is_none());
    drop(handle);
}
