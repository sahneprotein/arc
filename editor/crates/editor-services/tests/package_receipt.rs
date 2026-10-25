use editor_core::{Command, CommandRequest, JobKind, JobState, Mutation};
use editor_services::*;
use scene_data::*;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

fn published(root: &Path) -> (PathBuf, editor_cook::CookedPackage) {
    let package = editor_cook::cook(&current_map()).unwrap();
    let path = publish_package(root, &package, &AtomicBool::new(false)).unwrap();
    (path, package)
}
fn verify(path: &Path) -> Result<VerifiedPackageReceipt, Diagnostic> {
    verify_package(path, None, &AtomicBool::new(false))
}
/// Re-key a deliberately changed test package, so negative tests reach decoding
/// and validation rather than stopping at the outer directory identity check.
fn rekey(path: PathBuf, update: impl FnOnce(&mut PackageManifest)) -> PathBuf {
    let mut manifest = read_manifest(&path, &AtomicBool::new(false)).unwrap();
    update(&mut manifest);
    let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    fs::write(path.join("manifest.json"), &bytes).unwrap();
    let destination = path.parent().unwrap().join(payload_hash(&bytes).hex());
    fs::rename(path, &destination).unwrap();
    destination
}

#[test]
fn valid_receipt_is_absolute_exact_and_reverification_preserves_identity() {
    let cwd = std::env::current_dir().unwrap();
    let root = tempfile::tempdir_in(&cwd).unwrap();
    let (path, package) = published(root.path());
    // A native relative spelling of the same absolute path, without changing cwd.
    let receipt = verify(path.strip_prefix(&cwd).unwrap()).unwrap();
    assert!(receipt.path().is_absolute());
    assert_eq!(receipt.path(), fs::canonicalize(&path).unwrap());
    let identity = receipt.identity();
    assert_eq!(identity.source_hash, package.source_hash);
    assert_eq!(identity.simulation_hash, package.simulation_hash);
    assert_eq!(identity.visual_hash, package.visual_hash);
    assert_eq!(identity.schema, COOK_SCHEMA);
    assert_eq!(identity.profile, COLLISION_PROFILE);
    assert_eq!(identity.engine_revision, ENGINE_REVISION);
    assert_eq!(
        identity.manifest_hash,
        payload_hash(&fs::read(path.join("manifest.json")).unwrap())
    );
    assert_eq!(
        receipt.path().file_name().unwrap(),
        identity.manifest_hash.hex().as_str()
    );
    assert_eq!(
        verify_package(receipt.path(), Some(identity), &AtomicBool::new(false)).unwrap(),
        receipt
    );
}
#[test]
fn corruption_of_either_payload_is_refused_without_changing_receipt() {
    for filename in ["level.sim.json", "level.visual.json"] {
        let root = tempfile::tempdir().unwrap();
        let (path, _) = published(root.path());
        let receipt = verify(&path).unwrap();
        let expected = receipt.identity().clone();
        fs::write(path.join(filename), b"tampered").unwrap();
        assert_eq!(
            verify_package(&path, Some(&expected), &AtomicBool::new(false))
                .unwrap_err()
                .code,
            "payload.hash_mismatch"
        );
        assert_eq!(receipt.identity(), &expected);
    }
}

#[test]
fn manifest_tampering_and_wrong_directory_key_report_expected_and_found() {
    let root = tempfile::tempdir().unwrap();
    let (path, _) = published(root.path());
    let original = fs::read(path.join("manifest.json")).unwrap();
    let mut tampered = original.clone();
    tampered.push(b'\n');
    fs::write(path.join("manifest.json"), &tampered).unwrap();
    let error = verify(&path).unwrap_err();
    assert_eq!(error.code, "package.directory_identity");
    assert_eq!(error.parameters["expected"], payload_hash(&tampered).hex());
    assert_eq!(error.parameters["found"], payload_hash(&original).hex());
    fs::write(path.join("manifest.json"), &original).unwrap();
    let wrong = root.path().join("wrong-key");
    fs::rename(path, &wrong).unwrap();
    let error = verify(&wrong).unwrap_err();
    assert_eq!(error.code, "package.directory_identity");
    assert_eq!(error.parameters["expected"], payload_hash(&original).hex());
    assert_eq!(error.parameters["found"], "wrong-key");
}

#[test]
fn provenance_and_manifest_decode_fail_closed() {
    for field in ["schema", "profile", "engine_revision"] {
        let root = tempfile::tempdir().unwrap();
        let (path, _) = published(root.path());
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(path.join("manifest.json")).unwrap()).unwrap();
        manifest[field] = if field == "schema" {
            999.into()
        } else {
            "wrong".into()
        };
        fs::write(
            path.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert_eq!(verify(&path).unwrap_err().code, "package.provenance");
    }
    let root = tempfile::tempdir().unwrap();
    let (path, _) = published(root.path());
    fs::write(path.join("manifest.json"), b"{").unwrap();
    assert_eq!(verify(&path).unwrap_err().code, "package.decode");
}

#[test]
fn hash_consistent_invalid_simulation_and_visual_payloads_are_refused() {
    for filename in ["level.sim.json", "level.visual.json"] {
        let root = tempfile::tempdir().unwrap();
        let (path, _) = published(root.path());
        let bytes = b"{broken json";
        fs::write(path.join(filename), bytes).unwrap();
        let path = rekey(path, |m| {
            if filename == "level.sim.json" {
                m.simulation_hash = payload_hash(bytes);
            } else {
                m.visual_hash = payload_hash(bytes);
            }
        });
        assert_eq!(verify(&path).unwrap_err().code, "payload.decode");
    }
    let root = tempfile::tempdir().unwrap();
    let (path, package) = published(root.path());
    let mut simulation = package.simulation;
    simulation.boxes[0].half_extents_m[0] = -1.0;
    let bytes = serde_json::to_vec(&simulation).unwrap();
    fs::write(path.join("level.sim.json"), &bytes).unwrap();
    let path = rekey(path, |m| m.simulation_hash = payload_hash(&bytes));
    assert_eq!(verify(&path).unwrap_err().code, "payload.box_invalid");

    let root = tempfile::tempdir().unwrap();
    let (path, _) = published(root.path());
    let bytes = br#"{"schema":999,"terrain":[]}"#;
    fs::write(path.join("level.visual.json"), bytes).unwrap();
    let path = rekey(path, |m| m.visual_hash = payload_hash(bytes));
    assert_eq!(verify(&path).unwrap_err().code, "payload.schema_or_profile");
}

#[test]
fn visual_validation_checks_tile_references_against_exact_simulation() {
    let root = tempfile::tempdir().unwrap();
    let mut document = current_map();
    document.objects.push(ObjectRecord {
        id: StableId::from_u128(9000),
        parent: None,
        sibling_order: 9000,
        name: "Ground".into(),
        local: Default::default(),
        data: ObjectData::Terrain(
            terrain::TerrainDocument::flat(
                1,
                [1.0, 1.0],
                vec![(StableId::from_u128(9001), terrain::TileKey { x: 0, z: 0 })],
            )
            .unwrap(),
        ),
    });
    let package = editor_cook::cook(&document).unwrap();
    let path = publish_package(root.path(), &package, &AtomicBool::new(false)).unwrap();
    verify(&path).unwrap();
    let mut visual = package.visual;
    visual.terrain[0].tiles[0].key.x = 1;
    let bytes = serde_json::to_vec(&visual).unwrap();
    fs::write(path.join("level.visual.json"), &bytes).unwrap();
    let path = rekey(path, |m| m.visual_hash = payload_hash(&bytes));
    assert_eq!(verify(&path).unwrap_err().code, "payload.visual_tile");
}

#[test]
fn every_expected_identity_field_is_fenced_and_values_are_actionable() {
    let root = tempfile::tempdir().unwrap();
    let (path, _) = published(root.path());
    let original = verify(&path).unwrap();
    for field in [
        "manifest_hash",
        "source_hash",
        "simulation_hash",
        "visual_hash",
        "schema",
        "profile",
        "engine_revision",
    ] {
        let mut expected = original.identity().clone();
        match field {
            "manifest_hash" => expected.manifest_hash.0[0] ^= 1,
            "source_hash" => expected.source_hash.0[0] ^= 1,
            "simulation_hash" => expected.simulation_hash.0[0] ^= 1,
            "visual_hash" => expected.visual_hash.0[0] ^= 1,
            "schema" => expected.schema += 1,
            "profile" => expected.profile.push_str("-stale"),
            "engine_revision" => expected.engine_revision.push_str("-stale"),
            _ => unreachable!(),
        }
        let error = verify_package(&path, Some(&expected), &AtomicBool::new(false)).unwrap_err();
        assert_eq!(error.code, "package.identity_mismatch");
        assert_eq!(error.parameters["field"], field);
        assert_ne!(error.parameters["expected"], error.parameters["found"]);
    }
    // A well-formed newly keyed manifest still cannot replace a delayed receipt.
    let path = rekey(path, |m| m.source_hash.0[0] ^= 1);
    assert!(verify(&path).is_ok());
    assert_eq!(
        verify_package(&path, Some(original.identity()), &AtomicBool::new(false))
            .unwrap_err()
            .code,
        "package.identity_mismatch"
    );
}

#[test]
fn oversized_manifest_and_payload_and_cancelled_io_are_bounded() {
    let root = tempfile::tempdir().unwrap();
    let (path, _) = published(root.path());
    assert_eq!(
        verify_package(&path, None, &AtomicBool::new(true))
            .unwrap_err()
            .code,
        "job.cancelled"
    );
    fs::write(path.join("manifest.json"), vec![b' '; 8193]).unwrap();
    assert_eq!(verify(&path).unwrap_err().code, "package.manifest_limit");

    let root = tempfile::tempdir().unwrap();
    let (path, _) = published(root.path());
    fs::File::create(path.join("level.sim.json"))
        .unwrap()
        .set_len(MAX_PAYLOAD_BYTES as u64 + 1)
        .unwrap();
    assert_eq!(verify(&path).unwrap_err().code, "payload.limit");
}

/// What the system answers when a file system cannot hold a name: EILSEQ, "illegal byte
/// sequence". Its number differs between systems.
#[cfg(all(unix, any(target_os = "macos", target_os = "ios")))]
const NAME_NOT_REPRESENTABLE: i32 = 92;
#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
const NAME_NOT_REPRESENTABLE: i32 = 84;

#[cfg(unix)]
#[test]
fn non_utf8_parent_remains_a_native_path_across_publish_and_verify() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let root = tempfile::tempdir().unwrap();
    let parent = root
        .path()
        .join(OsString::from_vec(b"packages-\xff space".to_vec()));
    // Some file systems hold only names that are valid UTF-8 (APFS and HFS+ on macOS) and
    // refuse this folder. Nothing can be published under such a name there, so there is no
    // native path to keep and nothing to assert. Any other failure to make the folder, and
    // everything after it where the folder can exist, is still checked.
    match fs::create_dir(&parent) {
        Ok(()) => {}
        Err(error) if error.raw_os_error() == Some(NAME_NOT_REPRESENTABLE) => {
            eprintln!("not checked: this file system cannot hold a name that is not UTF-8");
            return;
        }
        Err(error) => panic!("the folder could not be made: {error}"),
    }
    let (path, _) = published(&parent);
    assert!(path.to_str().is_none());
    let receipt = verify(&path).unwrap();
    assert!(receipt.path().to_str().is_none());
    assert_eq!(receipt.path(), fs::canonicalize(path).unwrap());
    assert_eq!(verify(receipt.path()).unwrap(), receipt);
}

#[cfg(unix)]
#[test]
fn canonical_alias_resolves_to_receipt_but_cook_refuses_another_package() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let (path, _) = published(root.path());
    let receipt = verify(&path).unwrap();
    let alias = root.path().join("alias");
    symlink(&path, &alias).unwrap();
    assert_eq!(verify(&alias).unwrap(), receipt);

    let mut other = current_map();
    other.objects[0].name = "Different source".into();
    let package = editor_cook::cook(&other).unwrap();
    let other = publish_package(root.path(), &package, &AtomicBool::new(false)).unwrap();
    fs::remove_dir_all(&path).unwrap();
    symlink(other, &path).unwrap();
    assert_eq!(
        verify(&path).unwrap().identity().source_hash,
        package.source_hash
    );
    let mut handle = EditorHandle::start(current_map()).unwrap();
    let ticket = handle
        .job(JobOperation::Cook {
            package_root: root.path().into(),
            game: None,
        })
        .unwrap();
    assert_eq!(
        finish(&mut handle, ticket.id).unwrap_err().code,
        "package.identity_mismatch"
    );
}

fn finish(handle: &mut EditorHandle, id: u64) -> Result<JobOutput, Diagnostic> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut counts = BTreeMap::new();
    let mut output = None;
    while Instant::now() < deadline {
        for event in handle.poll(128) {
            if let ServiceEvent::JobFinished { fact, result } = event {
                *counts.entry(fact.id).or_insert(0) += 1;
                if fact.id == id {
                    output = Some(*result);
                }
            }
        }
        if handle
            .snapshot()
            .jobs
            .iter()
            .any(|j| j.id == id && !matches!(j.state, JobState::Running))
        {
            assert_eq!(counts.get(&id), Some(&1));
            return output.expect("terminal snapshot follows result");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("job {id} timed out");
}

#[test]
fn async_cook_receipt_and_preflight_keep_authoring_state_and_compatibility_fields() {
    let root = tempfile::tempdir().unwrap();
    let mut handle = EditorHandle::start(current_map()).unwrap();
    let before = handle.snapshot();
    let ticket = handle
        .job(JobOperation::Cook {
            package_root: root.path().into(),
            game: None,
        })
        .unwrap();
    let JobOutput::Cooked {
        receipt,
        path,
        source_hash,
        simulation_hash,
        ..
    } = finish(&mut handle, ticket.id).unwrap()
    else {
        panic!()
    };
    assert_eq!(path, receipt.path());
    assert_eq!(source_hash, receipt.identity().source_hash);
    assert_eq!(simulation_hash, receipt.identity().simulation_hash);
    let ticket = handle
        .job(JobOperation::VerifyPackage {
            path,
            expected: Some(receipt.identity().clone()),
        })
        .unwrap();
    let JobOutput::PackageVerified {
        receipt: reverified,
        ..
    } = finish(&mut handle, ticket.id).unwrap()
    else {
        panic!()
    };
    assert_eq!(reverified, receipt);
    assert_eq!(handle.snapshot().document, before.document);
    assert_eq!(
        handle.snapshot().document_revision,
        before.document_revision
    );
    assert_eq!(handle.snapshot().status.dirty, before.status.dirty);
    assert_eq!(handle.snapshot().undo_label, before.undo_label);
    assert_eq!(handle.snapshot().redo_label, before.redo_label);
    assert_eq!(handle.jobs().back().unwrap().kind, JobKind::VerifyPackage);
    let mut stale = receipt.identity().clone();
    stale.simulation_hash.0[0] ^= 1;
    let ticket = handle
        .job(JobOperation::VerifyPackage {
            path: receipt.path().into(),
            expected: Some(stale),
        })
        .unwrap();
    assert_eq!(
        finish(&mut handle, ticket.id).unwrap_err().code,
        "package.identity_mismatch"
    );
    assert_eq!(handle.snapshot().document, before.document);
}

#[test]
fn async_preflight_for_old_revision_is_stale_without_reading_package() {
    let mut handle = EditorHandle::start(current_map()).unwrap();
    let before = handle.snapshot();
    handle
        .submit(CommandRequest {
            request: 1,
            session: before.session,
            expected_revision: before.document_revision,
            expected_inspector_revision: None,
            command: Command::Edit {
                label_key: "rename".into(),
                mutations: vec![Mutation::Rename {
                    object: before.document.objects[0].id.clone(),
                    name: "Changed".into(),
                }],
            },
        })
        .unwrap();
    let ticket = handle
        .job(JobOperation::VerifyPackage {
            path: PathBuf::from("does-not-exist"),
            expected: None,
        })
        .unwrap();
    assert_eq!(
        finish(&mut handle, ticket.id).unwrap_err().code,
        "job.stale"
    );
    assert_eq!(
        handle.snapshot().document_revision,
        before.document_revision + 1
    );
}
