use crate::persistence::*;
use editor_cook::CookedPackage;
use scene_data::*;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsStr,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub schema: u32,
    pub profile: String,
    pub source_hash: ContentHash,
    pub simulation_hash: ContentHash,
    pub visual_hash: ContentHash,
    pub engine_revision: String,
}
pub const ENGINE_REVISION: &str = "bd0883094533ae6789af1f99de69da16a78f160b";

/// Exact identity of a published package. The manifest digest hashes its original
/// bytes, including whitespace; source and simulation identities are distinct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageIdentity {
    pub manifest_hash: ContentHash,
    pub source_hash: ContentHash,
    pub simulation_hash: ContentHash,
    pub visual_hash: ContentHash,
    pub schema: u32,
    pub profile: String,
    pub engine_revision: String,
}

/// An immutable attestation of one completed verification, not a filesystem lock.
/// Construction is restricted to the verifier; reverify before delayed use.
/// Paths remain native, canonical and absolute, including non-UTF-8 parents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedPackageReceipt {
    path: PathBuf,
    identity: PackageIdentity,
}
impl VerifiedPackageReceipt {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn identity(&self) -> &PackageIdentity {
        &self.identity
    }
}

/// Bounded, read-only preflight: manifest <=8 KiB, each payload <=64 MiB,
/// followed by the existing decoded schema/resource/geometry validators.
/// `expected` fences every identity field, including the exact manifest digest.
/// No source state changes or processes are involved.
pub fn verify_package(
    path: &Path,
    expected: Option<&PackageIdentity>,
    cancel: &AtomicBool,
) -> Result<VerifiedPackageReceipt, Diagnostic> {
    verify_package_payloads(path, expected, cancel).map(|(receipt, _, _)| receipt)
}
/// Consume decoded payloads from the same verification reads, without reopening paths.
pub(crate) fn verify_package_payloads(
    path: &Path,
    expected: Option<&PackageIdentity>,
    cancel: &AtomicBool,
) -> Result<(VerifiedPackageReceipt, SimulationPayload, VisualPayload), Diagnostic> {
    check_cancel(cancel)?;
    let path = fs::canonicalize(path).map_err(io_error)?;
    let bytes = read_bounded(&path.join("manifest.json"), 8192, cancel).map_err(|e| {
        if e.code == "source.limit" {
            Diagnostic::new("package.manifest_limit")
        } else {
            e
        }
    })?;
    let manifest = decode_manifest(&bytes)?;
    let identity = manifest_identity(&manifest, &bytes);
    let key = identity.manifest_hash.hex();
    if path.file_name() != Some(OsStr::new(&key)) {
        return Err(Diagnostic::new("package.directory_identity")
            .with("expected", &key)
            .with(
                "found",
                path.file_name().unwrap_or_default().to_string_lossy(),
            ));
    }
    if let Some(expected) = expected
        && expected != &identity
    {
        let (field, expected, found) = identity_difference(expected, &identity);
        return Err(Diagnostic::new("package.identity_mismatch")
            .with("field", field)
            .with("expected", expected)
            .with("found", found));
    }
    check_cancel(cancel)?;
    let (simulation, visual) = load_payloads(&path, &manifest, cancel)?;
    check_cancel(cancel)?;
    Ok((
        VerifiedPackageReceipt { path, identity },
        simulation,
        visual,
    ))
}
fn identity_difference(
    expected: &PackageIdentity,
    found: &PackageIdentity,
) -> (&'static str, String, String) {
    for (field, expected, found) in [
        (
            "manifest_hash",
            expected.manifest_hash.hex(),
            found.manifest_hash.hex(),
        ),
        (
            "source_hash",
            expected.source_hash.hex(),
            found.source_hash.hex(),
        ),
        (
            "simulation_hash",
            expected.simulation_hash.hex(),
            found.simulation_hash.hex(),
        ),
        (
            "visual_hash",
            expected.visual_hash.hex(),
            found.visual_hash.hex(),
        ),
        (
            "schema",
            expected.schema.to_string(),
            found.schema.to_string(),
        ),
        ("profile", expected.profile.clone(), found.profile.clone()),
        (
            "engine_revision",
            expected.engine_revision.clone(),
            found.engine_revision.clone(),
        ),
    ] {
        if expected != found {
            return (field, expected, found);
        }
    }
    unreachable!("called only for differing identities")
}
fn manifest_identity(manifest: &PackageManifest, bytes: &[u8]) -> PackageIdentity {
    PackageIdentity {
        manifest_hash: payload_hash(bytes),
        source_hash: manifest.source_hash,
        simulation_hash: manifest.simulation_hash,
        visual_hash: manifest.visual_hash,
        schema: manifest.schema,
        profile: manifest.profile.clone(),
        engine_revision: manifest.engine_revision.clone(),
    }
}
fn package_manifest(package: &CookedPackage) -> PackageManifest {
    PackageManifest {
        schema: COOK_SCHEMA,
        profile: COLLISION_PROFILE.into(),
        source_hash: package.source_hash,
        simulation_hash: package.simulation_hash,
        visual_hash: package.visual_hash,
        engine_revision: ENGINE_REVISION.into(),
    }
}
pub(crate) fn cooked_package_identity(
    package: &CookedPackage,
) -> Result<PackageIdentity, Diagnostic> {
    let manifest = package_manifest(package);
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| Diagnostic::new("package.encode").with("detail", e))?;
    Ok(manifest_identity(&manifest, &bytes))
}
/// Publish a new immutable directory; Play holds this path, never the mutable source.
pub fn publish_package(
    root: &Path,
    package: &CookedPackage,
    cancel: &AtomicBool,
) -> Result<PathBuf, Diagnostic> {
    check_cancel(cancel)?;
    let verified_simulation =
        decode_simulation(&package.simulation_bytes, package.simulation_hash)?;
    let verified_visual: VisualPayload = serde_json::from_slice(&package.visual_bytes)
        .map_err(|e| Diagnostic::new("payload.decode").with("detail", e))?;
    verified_visual.validate(&verified_simulation.ground)?;
    if payload_hash(&package.visual_bytes) != package.visual_hash {
        return Err(Diagnostic::new("payload.hash_mismatch"));
    }
    fs::create_dir_all(root).map_err(io_error)?;
    let manifest = package_manifest(package);
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| Diagnostic::new("package.encode").with("detail", e))?;
    let destination = root.join(payload_hash(&manifest_bytes).hex());
    if destination.exists() {
        load_package(&destination, cancel)?;
        return Ok(destination);
    }
    let directory = tempfile::Builder::new()
        .prefix(".cook-")
        .tempdir_in(root)
        .map_err(io_error)?;
    for (name, bytes) in [
        ("manifest.json", &manifest_bytes),
        ("level.sim.json", &package.simulation_bytes),
        ("level.visual.json", &package.visual_bytes),
    ] {
        check_cancel(cancel)?;
        let mut file = File::create(directory.path().join(name)).map_err(io_error)?;
        file.write_all(bytes).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
    }
    sync_directory(directory.path())?;
    check_cancel(cancel)?;
    match fs::rename(directory.path(), &destination) {
        Ok(()) => {
            sync_directory(root)?;
        }
        Err(error) => {
            if !destination.exists() {
                return Err(io_error(error));
            }
            load_package(&destination, cancel)?;
        }
    }
    Ok(destination)
}
pub fn read_manifest(path: &Path, cancel: &AtomicBool) -> Result<PackageManifest, Diagnostic> {
    let bytes = read_bounded(&path.join("manifest.json"), 8192, cancel)?;
    decode_manifest(&bytes)
}
fn decode_manifest(bytes: &[u8]) -> Result<PackageManifest, Diagnostic> {
    let manifest: PackageManifest = serde_json::from_slice(bytes)
        .map_err(|e| Diagnostic::new("package.decode").with("detail", e))?;
    if manifest.schema != COOK_SCHEMA
        || manifest.profile != COLLISION_PROFILE
        || manifest.engine_revision != ENGINE_REVISION
    {
        return Err(Diagnostic::new("package.provenance"));
    }
    Ok(manifest)
}
/// Authoritative/predicted runtime needs only this file, never decoration decoding.
pub fn load_simulation(path: &Path, cancel: &AtomicBool) -> Result<SimulationPayload, Diagnostic> {
    let manifest = read_manifest(path, cancel)?;
    decode_simulation(
        &read_bounded(
            &path.join("level.sim.json"),
            MAX_PAYLOAD_BYTES as u64,
            cancel,
        )?,
        manifest.simulation_hash,
    )
}
pub fn load_package(
    path: &Path,
    cancel: &AtomicBool,
) -> Result<(SimulationPayload, VisualPayload), Diagnostic> {
    let manifest = read_manifest(path, cancel)?;
    load_payloads(path, &manifest, cancel)
}
fn load_payloads(
    path: &Path,
    manifest: &PackageManifest,
    cancel: &AtomicBool,
) -> Result<(SimulationPayload, VisualPayload), Diagnostic> {
    check_cancel(cancel)?;
    let simulation = decode_simulation(
        &read_bounded(
            &path.join("level.sim.json"),
            MAX_PAYLOAD_BYTES as u64,
            cancel,
        )
        .map_err(payload_read_error)?,
        manifest.simulation_hash,
    )?;
    check_cancel(cancel)?;
    let bytes = read_bounded(
        &path.join("level.visual.json"),
        MAX_PAYLOAD_BYTES as u64,
        cancel,
    )
    .map_err(payload_read_error)?;
    if payload_hash(&bytes) != manifest.visual_hash {
        return Err(Diagnostic::new("payload.hash_mismatch"));
    }
    let visual: VisualPayload = serde_json::from_slice(&bytes)
        .map_err(|e| Diagnostic::new("payload.decode").with("detail", e))?;
    // The visual payload has two schemas: the first, and the second for a level that
    // places particle emitters. Which of them a payload must be is checked with the rest.
    if visual.schema != COOK_SCHEMA && visual.schema != VISUAL_SCHEMA_EMITTERS {
        return Err(Diagnostic::new("payload.schema_or_profile"));
    }
    visual.validate(&simulation.ground)?;
    check_cancel(cancel)?;
    Ok((simulation, visual))
}
fn payload_read_error(error: Diagnostic) -> Diagnostic {
    if error.code == "source.limit" {
        Diagnostic::new("payload.limit")
    } else {
        error
    }
}
