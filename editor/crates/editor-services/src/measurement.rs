//! Exportable stage records. Unknown hardware/cache facts stay explicit rather than inferred.
use crate::*;
use scene_data::*;
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunMetadata {
    pub run_id: String,
    pub engine_revision: String,
    pub workspace_lock_hash: ContentHash,
    pub os: String,
    pub architecture: String,
    pub profile: String,
    pub toolkit: Option<String>,
    pub cache_condition: String,
    pub workload: String,
    pub measurement_conditions: String,
    pub hardware: Option<String>,
    pub gpu_driver: Option<String>,
    pub rustc: Option<String>,
    pub viewport_pixels: Option<[u32; 2]>,
    pub scale_factor: Option<f32>,
}
impl RunMetadata {
    pub fn local(workload: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self {
            run_id: format!("{}-{stamp}", std::process::id()),
            engine_revision: ENGINE_REVISION.into(),
            workspace_lock_hash: payload_hash(include_bytes!("../../../Cargo.lock")),
            os: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            profile: if cfg!(debug_assertions) {
                "dev"
            } else {
                "release"
            }
            .into(),
            toolkit: None,
            cache_condition: "uncontrolled".into(),
            workload: workload.into(),
            measurement_conditions: "functional run; other machine work not excluded".into(),
            hardware: None,
            gpu_driver: None,
            rustc: None,
            viewport_pixels: None,
            scale_factor: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StageMeasurement {
    pub phase: String,
    pub duration_us: u64,
    pub bytes: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeasurementRecord {
    pub schema: u32,
    pub run: RunMetadata,
    pub job_id: Option<u64>,
    pub operation: String,
    pub source_hash: Option<ContentHash>,
    pub simulation_hash: Option<ContentHash>,
    pub stages: Vec<StageMeasurement>,
    pub failure: Option<Diagnostic>,
}
impl MeasurementRecord {
    pub fn from_job(
        run: RunMetadata,
        fact: &JobFact,
        result: &Result<JobOutput, Diagnostic>,
    ) -> Self {
        let mut record = Self {
            schema: 1,
            run,
            job_id: Some(fact.id),
            operation: format!("{:?}", fact.kind).to_lowercase(),
            source_hash: None,
            simulation_hash: None,
            stages: Vec::new(),
            failure: result.as_ref().err().cloned(),
        };
        match result {
            Ok(JobOutput::Geometry { timings, .. }) => {
                // Viewport CPU work does not encode/hash a publication package.
                record.stage("geometry_validate", timings.validation_us, None);
                record.stage("geometry_prepare", timings.geometry_us, None);
                record.stage("geometry_rules", timings.rule_us, None);
                record.stage("geometry_total", timings.total_us, None);
            }
            Ok(JobOutput::Opened {
                timings,
                source_hash,
                ..
            }) => {
                record.source_hash = Some(*source_hash);
                record.open(*timings);
            }
            Ok(JobOutput::Saved(receipt)) => {
                record.source_hash = Some(receipt.source_hash);
                record.stage("source_save_total", receipt.total_us, None);
            }
            Ok(JobOutput::Cooked {
                source_hash,
                simulation_hash,
                timings,
                publication_us,
                ..
            }) => {
                record.source_hash = Some(*source_hash);
                record.simulation_hash = Some(*simulation_hash);
                record.cook(*timings);
                record.stage("package_publish", *publication_us, None);
            }
            Ok(JobOutput::PackageVerified {
                receipt,
                verification_us,
            }) => {
                record.source_hash = Some(receipt.identity().source_hash);
                record.simulation_hash = Some(receipt.identity().simulation_hash);
                record.stage("package_verify", *verification_us, None);
            }
            Err(_) => {}
        }
        if let Some(total) = fact.elapsed_us {
            record.stage("job_total", total, None);
        }
        record
    }
    pub fn stage(&mut self, phase: &str, duration_us: u64, bytes: Option<u64>) {
        self.stages.push(StageMeasurement {
            phase: phase.into(),
            duration_us,
            bytes,
        });
    }
    pub fn open(&mut self, t: OpenTimings) {
        self.stage("source_read", t.read_us, None);
        self.stage("source_decode", t.decode_us, None);
        self.stage("source_validate_hash", t.validation_us, None);
        self.stage("source_open_total", t.total_us, None);
    }
    pub fn cook(&mut self, t: editor_cook::CookTimings) {
        self.stage("cook_validate_hash", t.validation_us, None);
        self.stage("cook_geometry", t.geometry_us, None);
        self.stage("cook_rules", t.rule_us, None);
        self.stage("cook_encode_hash", t.encoding_us, None);
        self.stage("cook_total", t.total_us, None);
    }
}
/// Bounded writer: no unbounded trace file growth and no logging from a frame system.
pub struct MeasurementWriter<W: Write> {
    writer: W,
    written: usize,
    limit: usize,
}
impl<W: Write> MeasurementWriter<W> {
    pub fn new(writer: W, limit_bytes: usize) -> Self {
        Self {
            writer,
            written: 0,
            limit: limit_bytes,
        }
    }
    pub fn write(&mut self, record: &MeasurementRecord) -> Result<(), Diagnostic> {
        let mut bytes = serde_json::to_vec(record)
            .map_err(|e| Diagnostic::new("measurement.encode").with("detail", e))?;
        bytes.push(b'\n');
        if bytes.len() > 65536 || self.written.saturating_add(bytes.len()) > self.limit {
            return Err(Diagnostic::new("measurement.limit"));
        }
        self.writer
            .write_all(&bytes)
            .map_err(crate::persistence::io_error)?;
        self.written += bytes.len();
        Ok(())
    }
    pub fn into_inner(self) -> W {
        self.writer
    }
}
