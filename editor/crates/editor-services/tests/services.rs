use editor_core::*;
use editor_services::*;
use scene_data::*;
use std::{
    fs,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

fn finish_job(handle: &mut EditorHandle, ticket: &JobTicket) -> Result<JobOutput, Diagnostic> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut output = None;
    while Instant::now() < deadline {
        for event in handle.poll(32) {
            if let ServiceEvent::JobFinished { fact, result } = event {
                assert_eq!(fact.id, ticket.id);
                output = Some(result);
            }
        }
        if handle
            .snapshot()
            .jobs
            .iter()
            .any(|j| j.id == ticket.id && !matches!(j.state, JobState::Running))
            && let Some(result) = output.take()
        {
            return result;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("job {} did not publish its final snapshot", ticket.id);
}

fn rename_first(handle: &mut EditorHandle, request: u64, name: &str) {
    let before = handle.snapshot();
    handle
        .submit(CommandRequest {
            request,
            session: before.session,
            expected_revision: before.document_revision,
            expected_inspector_revision: None,
            command: Command::Edit {
                label_key: "rename".into(),
                mutations: vec![Mutation::Rename {
                    object: before.document.objects[0].id.clone(),
                    name: name.into(),
                }],
            },
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        for event in handle.poll(32) {
            if let ServiceEvent::Command(result) = event {
                result.outcome.unwrap();
            }
        }
        if handle.snapshot().document_revision == before.document_revision + 1 {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("rename did not publish its snapshot");
}

#[test]
fn opened_disk_hash_supports_save_and_external_conflict_preserves_unsaved_state() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("map.arcscene");
    // Whitespace changes disk identity without changing semantic source identity.
    // Open must report the exact bytes read, not a canonical re-encoding.
    let mut bytes = serde_json::to_vec(&current_map()).unwrap();
    bytes.extend_from_slice(b"\n \n");
    fs::write(&path, &bytes).unwrap();
    let mut handle = EditorHandle::start(current_map()).unwrap();
    let ticket = handle.job(JobOperation::Open(path.clone())).unwrap();
    let JobOutput::Opened {
        disk_hash,
        source_hash: semantic_hash,
        ..
    } = finish_job(&mut handle, &ticket).unwrap()
    else {
        panic!()
    };
    assert_eq!(disk_hash, payload_hash(&bytes));
    assert_eq!(semantic_hash, source_hash(&current_map()).unwrap());
    assert_ne!(disk_hash, semantic_hash);
    assert!(!handle.snapshot().status.dirty);
    rename_first(&mut handle, 1, "Edited after Open");
    let ticket = handle
        .job(JobOperation::Save {
            path: path.clone(),
            expected: DiskVersion::Hash(disk_hash),
        })
        .unwrap();
    let JobOutput::Saved(receipt) = finish_job(&mut handle, &ticket).unwrap() else {
        panic!()
    };
    assert!(!handle.snapshot().status.dirty);
    assert_eq!(
        load_document(&path, &AtomicBool::new(false))
            .unwrap()
            .disk_hash,
        receipt.disk_hash
    );

    // Reopen, then let a non-cooperating external editor change the file.
    let ticket = handle.job(JobOperation::Open(path.clone())).unwrap();
    let JobOutput::Opened { disk_hash, .. } = finish_job(&mut handle, &ticket).unwrap() else {
        panic!()
    };
    rename_first(&mut handle, 2, "Unsaved local edit");
    fs::write(&path, b"external edit").unwrap();
    let before = handle.snapshot();
    let ticket = handle
        .job(JobOperation::Save {
            path: path.clone(),
            expected: DiskVersion::Hash(disk_hash),
        })
        .unwrap();
    assert_eq!(
        finish_job(&mut handle, &ticket).unwrap_err().code,
        "save.conflict"
    );
    assert!(handle.snapshot().status.dirty);
    assert_eq!(handle.snapshot().document, before.document);
    assert_eq!(
        handle.snapshot().document_revision,
        before.document_revision
    );
    assert_eq!(fs::read(&path).unwrap(), b"external edit");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn parent_creation_io_failure_keeps_dirty_document_and_actionable_error() {
    let root = tempfile::tempdir().unwrap();
    let blocker = root.path().join("regular-file");
    fs::write(&blocker, b"keep me").unwrap();
    let mut handle = EditorHandle::start(current_map()).unwrap();
    rename_first(&mut handle, 1, "Unsaved");
    let before = handle.snapshot();
    let parent = blocker.join("levels");
    let path = parent.join("map.arcscene");
    let ticket = handle
        .job(JobOperation::Save {
            path: path.clone(),
            expected: DiskVersion::Absent,
        })
        .unwrap();
    let error = finish_job(&mut handle, &ticket).unwrap_err();
    assert_eq!(error.code, "save.parent_create");
    assert_eq!(error.parameters["path"], parent.display().to_string());
    assert!(!error.parameters["detail"].is_empty());
    assert!(handle.snapshot().status.dirty);
    assert_eq!(handle.snapshot().document, before.document);
    assert!(!path.exists());
    assert_eq!(fs::read(blocker).unwrap(), b"keep me");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn immutable_package_detects_corruption_and_simulation_does_not_read_visuals() {
    let root = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let package = editor_cook::cook(&current_map()).unwrap();
    let path = publish_package(root.path(), &package, &cancel).unwrap();
    assert_eq!(
        publish_package(root.path(), &package, &cancel).unwrap(),
        path
    );
    let (sim, _) = load_package(&path, &cancel).unwrap();
    assert_eq!(sim, package.simulation);
    fs::write(path.join("level.visual.json"), b"corrupt decoration").unwrap();
    assert_eq!(
        load_package(&path, &cancel).unwrap_err().code,
        "payload.hash_mismatch"
    );
    assert_eq!(load_simulation(&path, &cancel).unwrap(), package.simulation);
    fs::write(path.join("level.sim.json"), b"corrupt collision").unwrap();
    assert_eq!(
        load_simulation(&path, &cancel).unwrap_err().code,
        "payload.hash_mismatch"
    );
}
#[test]
fn queued_job_for_old_revision_does_not_write_and_cached_snapshot_never_waits() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("stale.arcscene");
    let mut handle = EditorHandle::start(current_map()).unwrap();
    let snapshot = handle.snapshot();
    let id = snapshot.document.objects[0].id.clone();
    handle
        .submit(CommandRequest {
            request: 1,
            session: snapshot.session,
            expected_revision: 0,
            expected_inspector_revision: None,
            command: Command::Edit {
                label_key: "rename".into(),
                mutations: vec![Mutation::Rename {
                    object: id,
                    name: "Updated".into(),
                }],
            },
        })
        .unwrap();
    let ticket = handle
        .job(JobOperation::Save {
            path: path.clone(),
            expected: DiskVersion::Absent,
        })
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(&snapshot, &handle.snapshot()));
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut finished = false;
    while Instant::now() < deadline && !finished {
        for event in handle.poll(32) {
            if let ServiceEvent::JobFinished { fact, result } = event {
                assert_eq!(fact.id, ticket.id);
                assert_eq!(result.unwrap_err().code, "job.stale");
                finished = true;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(finished);
    assert!(!path.exists());
    assert_eq!(handle.snapshot().document_revision, 1);
    assert!(handle.snapshot().status.dirty);
}

#[test]
fn stage_export_is_bounded_and_preserves_unmeasured_facts() {
    let run = RunMetadata::local("functional-test");
    assert_eq!(run.cache_condition, "uncontrolled");
    assert!(run.hardware.is_none());
    assert!(run.toolkit.is_none());
    let fact = JobFact {
        id: 1,
        kind: JobKind::Open,
        state: JobState::Failed(Diagnostic::new("source.decode")),
        elapsed_us: Some(12),
    };
    let record = MeasurementRecord::from_job(run, &fact, &Err(Diagnostic::new("source.decode")));
    let mut writer = MeasurementWriter::new(Vec::new(), 65536);
    writer.write(&record).unwrap();
    let bytes = writer.into_inner();
    assert_eq!(bytes.last(), Some(&b'\n'));
    let decoded: MeasurementRecord = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded.failure.unwrap().code, "source.decode");
    assert_eq!(decoded.stages[0].phase, "job_total");
    assert_eq!(decoded.stages[0].duration_us, 12);
    let mut limited = MeasurementWriter::new(Vec::new(), 1);
    assert_eq!(
        limited.write(&record).unwrap_err().code,
        "measurement.limit"
    );
    assert!(limited.into_inner().is_empty());
}

#[test]
fn geometry_job_uses_preview_without_writing_or_changing_committed_source() {
    let mut handle = EditorHandle::start(current_map()).unwrap();
    let before = handle.snapshot();
    handle
        .submit(CommandRequest {
            request: 1,
            session: before.session,
            expected_revision: 0,
            expected_inspector_revision: None,
            command: Command::BeginViewportEdit {
                selection_revision: 0,
            },
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut transaction = None;
    while Instant::now() < deadline && transaction.is_none() {
        for event in handle.poll(32) {
            if let ServiceEvent::Command(result) = event
                && let Ok(CommandOutcome::TransactionStarted(id)) = result.outcome
            {
                transaction = Some(id);
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let transaction = transaction.unwrap();
    let mut transform = before.document.objects[0].local.clone();
    transform.translation_m[0] = 9.0;
    handle
        .submit(CommandRequest {
            request: 2,
            session: before.session,
            expected_revision: 0,
            expected_inspector_revision: None,
            command: Command::Preview {
                transaction,
                sequence: 1,
                mutations: vec![Mutation::Transform {
                    object: before.document.objects[0].id.clone(),
                    value: transform,
                }],
            },
        })
        .unwrap();
    handle.job(JobOperation::Geometry).unwrap();
    let mut geometry = None;
    while Instant::now() < deadline && geometry.is_none() {
        for event in handle.poll(32) {
            if let ServiceEvent::JobFinished { result, .. } = event {
                geometry = Some(result.unwrap());
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let JobOutput::Geometry {
        binding,
        simulation,
        ..
    } = geometry.unwrap()
    else {
        panic!()
    };
    assert_eq!(simulation.boxes[0].transform.translation_m[0], 9.0);
    assert!(binding.matches(&handle.snapshot()));
    assert_eq!(handle.snapshot().document, before.document);
    assert_eq!(handle.snapshot().document_revision, 0);
    handle
        .submit(CommandRequest {
            request: 3,
            session: before.session,
            expected_revision: 0,
            expected_inspector_revision: None,
            command: Command::CancelViewportEdit { transaction },
        })
        .unwrap();
    while Instant::now() < deadline && handle.snapshot().preview.is_some() {
        handle.poll(32);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(handle.snapshot().preview.is_none());
    assert!(!binding.matches(&handle.snapshot()));
    assert_eq!(handle.snapshot().document, before.document);
}
