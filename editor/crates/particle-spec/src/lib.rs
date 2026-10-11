//! Renderer-independent, versioned particle emitter contract.
//!
//! V1 stores reusable solid-billboard emitters as closed JSON. The exact file bytes
//! are the content identity; [`canonical_bytes`] is the canonical editor writer.
//!
//! # Preview and placement convention
//! Shapes and initial velocity directions use emitter-local axes. A cone points
//! along local +Y. At spawn, apply the emitter translation and rotation exactly
//! once (ignore transform scale); gravity is world-space. Particles then simulate
//! in world space and do not follow later emitter motion.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

pub const SCHEMA: u32 = 1;
pub const MAX_FILE_BYTES: usize = 16 * 1024;
pub const ASSET_DIRECTORY: &str = "assets/particles";
pub const ASSET_SUFFIX: &str = ".emitter-v1.json";

pub const NAME_MIN_BYTES: usize = 1;
pub const NAME_MAX_BYTES: usize = 96;
pub const RATE_PER_SECOND_MAX: f32 = 10_000.0;
pub const BURST_COUNT_MAX: u32 = 10_000;
pub const DURATION_SECONDS_MIN: f32 = 0.001;
pub const DURATION_SECONDS_MAX: f32 = 3_600.0;
pub const LIFETIME_SECONDS_MIN: f32 = 0.01;
pub const LIFETIME_SECONDS_MAX: f32 = 300.0;
pub const SHAPE_DIMENSION_MIN_M: f32 = 0.001;
pub const SHAPE_DIMENSION_MAX_M: f32 = 100.0;
pub const CONE_ANGLE_DEGREES_MAX: f32 = 90.0;
pub const SPEED_MAX_M_PER_S: f32 = 1_000.0;
pub const GRAVITY_MAX_M_PER_S2: f32 = 100.0;
pub const DRAG_PER_S_MAX: f32 = 100.0;
pub const SIZE_MAX_M: f32 = 100.0;
pub const COLOR_RGB_MAX: f32 = 16.0;
pub const COLOR_ALPHA_MAX: f32 = 1.0;
pub const MAX_PARTICLES_MAX: u32 = 100_000;

/// Public scalar ranges. Most entries use the same dotted name stored in
/// [`EmitterError::field`]. RGBA values additionally expose `.rgb` and `.alpha`
/// bounds so clients can apply their component-specific limits without
/// duplicating policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumericBound {
    pub field: &'static str,
    pub min: f64,
    pub max: f64,
}

pub const NUMERIC_BOUNDS: &[NumericBound] = &[
    NumericBound {
        field: "emission.rate_per_second",
        min: 0.0,
        max: RATE_PER_SECOND_MAX as f64,
    },
    NumericBound {
        field: "emission.burst_count",
        min: 0.0,
        max: BURST_COUNT_MAX as f64,
    },
    NumericBound {
        field: "emission.duration_seconds",
        min: DURATION_SECONDS_MIN as f64,
        max: DURATION_SECONDS_MAX as f64,
    },
    NumericBound {
        field: "lifetime.min_seconds",
        min: LIFETIME_SECONDS_MIN as f64,
        max: LIFETIME_SECONDS_MAX as f64,
    },
    NumericBound {
        field: "lifetime.max_seconds",
        min: LIFETIME_SECONDS_MIN as f64,
        max: LIFETIME_SECONDS_MAX as f64,
    },
    NumericBound {
        field: "shape.radius_m",
        min: SHAPE_DIMENSION_MIN_M as f64,
        max: SHAPE_DIMENSION_MAX_M as f64,
    },
    NumericBound {
        field: "shape.angle_degrees",
        min: 0.0,
        max: CONE_ANGLE_DEGREES_MAX as f64,
    },
    NumericBound {
        field: "shape.size_m",
        min: SHAPE_DIMENSION_MIN_M as f64,
        max: SHAPE_DIMENSION_MAX_M as f64,
    },
    NumericBound {
        field: "motion.speed_min_m_per_s",
        min: 0.0,
        max: SPEED_MAX_M_PER_S as f64,
    },
    NumericBound {
        field: "motion.speed_max_m_per_s",
        min: 0.0,
        max: SPEED_MAX_M_PER_S as f64,
    },
    NumericBound {
        field: "motion.gravity_m_per_s2",
        min: -(GRAVITY_MAX_M_PER_S2 as f64),
        max: GRAVITY_MAX_M_PER_S2 as f64,
    },
    NumericBound {
        field: "motion.drag_per_s",
        min: 0.0,
        max: DRAG_PER_S_MAX as f64,
    },
    NumericBound {
        field: "look.size_start_m",
        min: 0.0,
        max: SIZE_MAX_M as f64,
    },
    NumericBound {
        field: "look.size_end_m",
        min: 0.0,
        max: SIZE_MAX_M as f64,
    },
    NumericBound {
        field: "look.color_start_linear_rgba",
        min: 0.0,
        max: COLOR_RGB_MAX as f64,
    },
    NumericBound {
        field: "look.color_start_linear_rgba.rgb",
        min: 0.0,
        max: COLOR_RGB_MAX as f64,
    },
    NumericBound {
        field: "look.color_start_linear_rgba.alpha",
        min: 0.0,
        max: COLOR_ALPHA_MAX as f64,
    },
    NumericBound {
        field: "look.color_end_linear_rgba",
        min: 0.0,
        max: COLOR_RGB_MAX as f64,
    },
    NumericBound {
        field: "look.color_end_linear_rgba.rgb",
        min: 0.0,
        max: COLOR_RGB_MAX as f64,
    },
    NumericBound {
        field: "look.color_end_linear_rgba.alpha",
        min: 0.0,
        max: COLOR_ALPHA_MAX as f64,
    },
    NumericBound {
        field: "limits.max_particles",
        min: 1.0,
        max: MAX_PARTICLES_MAX as f64,
    },
];

pub fn numeric_bound(field: &str) -> Option<&'static NumericBound> {
    NUMERIC_BOUNDS.iter().find(|bound| bound.field == field)
}

/// Returns the conventional reusable-asset path for a valid stable ID.
pub fn asset_path(game_root: &Path, id: &str) -> Result<std::path::PathBuf, EmitterError> {
    if !valid_id(id) {
        return Err(EmitterError::value(
            "id",
            "must be 32 lowercase hexadecimal characters and nonzero",
        ));
    }
    Ok(game_root
        .join(ASSET_DIRECTORY)
        .join(format!("{id}{ASSET_SUFFIX}")))
}

/// Extracts a valid stable ID from a filename using the canonical suffix.
pub fn id_from_filename(filename: &str) -> Option<&str> {
    let id = filename.strip_suffix(ASSET_SUFFIX)?;
    valid_id(id).then_some(id)
}

/// Extracts a valid stable ID from an asset path using the canonical suffix.
pub fn id_from_asset_path(path: &Path) -> Option<&str> {
    id_from_filename(path.file_name()?.to_str()?)
}

/// Loads by stable ID and checks that the document's ID matches its path.
pub fn load_asset(game_root: &Path, id: &str) -> Result<LoadedEmitter, EmitterError> {
    let path = asset_path(game_root, id)?;
    let loaded = load(&path)?;
    if loaded.spec.id != id {
        return Err(EmitterError::value(
            "id",
            "document ID does not match the requested asset ID",
        ));
    }
    Ok(loaded)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EmitterSpec {
    pub schema: u32,
    /// Stable 32-character lowercase hexadecimal ID, excluding all-zero.
    pub id: String,
    pub name: String,
    pub emission: Emission,
    pub lifetime: Lifetime,
    pub shape: Shape,
    pub motion: Motion,
    pub look: Look,
    pub limits: Limits,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Emission {
    /// Continuous emission rate, emitted over each active duration cycle.
    pub rate_per_second: f32,
    /// Number emitted once at each cycle start, including time zero.
    pub burst_count: u32,
    /// Each cycle lasts this long; looping repeats the same cycle indefinitely.
    pub duration_seconds: f32,
    pub looping: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Lifetime {
    pub min_seconds: f32,
    pub max_seconds: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    Point {},
    Sphere {
        radius_m: f32,
    },
    Cone {
        radius_m: f32,
        angle_degrees: f32,
    },
    /// Full side lengths along local X, Y and Z.
    Box {
        size_m: [f32; 3],
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Motion {
    pub speed_min_m_per_s: f32,
    pub speed_max_m_per_s: f32,
    pub gravity_m_per_s2: [f32; 3],
    pub drag_per_s: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Look {
    pub size_start_m: f32,
    pub size_end_m: f32,
    pub color_start_linear_rgba: [f32; 4],
    pub color_end_linear_rgba: [f32; 4],
    pub blend: Blend,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Blend {
    Alpha,
    Additive,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_particles: u32,
}

/// Shared, documented starting point for the editor's New emitter action.
pub fn default_emitter() -> EmitterSpec {
    EmitterSpec {
        schema: SCHEMA,
        id: "00000000000000000000000000000001".into(),
        name: "New Emitter".into(),
        emission: Emission {
            rate_per_second: 10.0,
            burst_count: 0,
            duration_seconds: 5.0,
            looping: true,
        },
        lifetime: Lifetime {
            min_seconds: 1.0,
            max_seconds: 2.0,
        },
        shape: Shape::Point {},
        motion: Motion {
            speed_min_m_per_s: 1.0,
            speed_max_m_per_s: 2.0,
            gravity_m_per_s2: [0.0, -9.81, 0.0],
            drag_per_s: 0.0,
        },
        look: Look {
            size_start_m: 0.1,
            size_end_m: 0.0,
            color_start_linear_rgba: [1.0, 1.0, 1.0, 1.0],
            color_end_linear_rgba: [1.0, 1.0, 1.0, 0.0],
            blend: Blend::Alpha,
        },
        limits: Limits {
            max_particles: 1_000,
        },
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmitterError {
    pub code: &'static str,
    pub field: Option<&'static str>,
    pub detail: String,
}

impl EmitterError {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            field: None,
            detail: detail.into(),
        }
    }
    fn value(field: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code: "particle.value",
            field: Some(field),
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for EmitterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.field {
            Some(field) => write!(f, "{} ({field}): {}", self.code, self.detail),
            None => write!(f, "{}: {}", self.code, self.detail),
        }
    }
}
impl std::error::Error for EmitterError {}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadedEmitter {
    pub spec: EmitterSpec,
    pub sha256_hex: String,
}

/// Bounded read and validation. Refusal order is missing/read failure, byte
/// limit, JSON syntax, schema, typed/closed shape, then values in validation order.
pub fn load(path: &Path) -> Result<LoadedEmitter, EmitterError> {
    let mut file = File::open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            EmitterError::new("particle.missing", path.display().to_string())
        } else {
            EmitterError::new(
                "particle.read_failed",
                format!("{}: {error}", path.display()),
            )
        }
    })?;
    let mut bytes = Vec::with_capacity(MAX_FILE_BYTES.min(4096));
    file.by_ref()
        .take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| EmitterError::new("particle.read_failed", error.to_string()))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(EmitterError::new(
            "particle.limit",
            format!("file exceeds {MAX_FILE_BYTES} bytes"),
        ));
    }
    let raw: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| EmitterError::new("particle.decode", error.to_string()))?;
    let schema = raw.get("schema");
    let schema_found = schema
        .and_then(serde_json::Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| schema.map_or_else(|| "none".into(), serde_json::Value::to_string));
    if schema.and_then(serde_json::Value::as_u64) != Some(SCHEMA as u64) {
        return Err(EmitterError::new(
            "particle.schema",
            format!("found {schema_found}, expected {SCHEMA}"),
        ));
    }
    let spec: EmitterSpec = serde_json::from_slice(&bytes)
        .map_err(|error| EmitterError::new("particle.decode", error.to_string()))?;
    validate(&spec)?;
    Ok(LoadedEmitter {
        spec,
        sha256_hex: sha256_hex(&bytes),
    })
}

/// Validate a typed emitter in deterministic field order.
pub fn validate(spec: &EmitterSpec) -> Result<(), EmitterError> {
    if spec.schema != SCHEMA {
        return Err(EmitterError::new(
            "particle.schema",
            format!("found {}, expected {SCHEMA}", spec.schema),
        ));
    }
    if !valid_id(&spec.id) {
        return Err(EmitterError::value(
            "id",
            "must be 32 lowercase hexadecimal characters and nonzero",
        ));
    }
    if !(NAME_MIN_BYTES..=NAME_MAX_BYTES).contains(&spec.name.len()) {
        return Err(EmitterError::value(
            "name",
            format!("must be {NAME_MIN_BYTES}..={NAME_MAX_BYTES} UTF-8 bytes"),
        ));
    }
    finite_range(
        "emission.rate_per_second",
        spec.emission.rate_per_second,
        0.0,
        RATE_PER_SECOND_MAX,
    )?;
    if spec.emission.burst_count > BURST_COUNT_MAX {
        return Err(EmitterError::value(
            "emission.burst_count",
            "exceeds maximum",
        ));
    }
    finite_range(
        "emission.duration_seconds",
        spec.emission.duration_seconds,
        DURATION_SECONDS_MIN,
        DURATION_SECONDS_MAX,
    )?;
    finite_range(
        "lifetime.min_seconds",
        spec.lifetime.min_seconds,
        LIFETIME_SECONDS_MIN,
        LIFETIME_SECONDS_MAX,
    )?;
    finite_range(
        "lifetime.max_seconds",
        spec.lifetime.max_seconds,
        LIFETIME_SECONDS_MIN,
        LIFETIME_SECONDS_MAX,
    )?;
    if spec.lifetime.min_seconds > spec.lifetime.max_seconds {
        return Err(EmitterError::value(
            "lifetime.min_seconds",
            "must not exceed lifetime.max_seconds",
        ));
    }
    match &spec.shape {
        Shape::Point {} => {}
        Shape::Sphere { radius_m } => finite_range(
            "shape.radius_m",
            *radius_m,
            SHAPE_DIMENSION_MIN_M,
            SHAPE_DIMENSION_MAX_M,
        )?,
        Shape::Cone {
            radius_m,
            angle_degrees,
        } => {
            finite_range(
                "shape.radius_m",
                *radius_m,
                SHAPE_DIMENSION_MIN_M,
                SHAPE_DIMENSION_MAX_M,
            )?;
            finite_range(
                "shape.angle_degrees",
                *angle_degrees,
                0.0,
                CONE_ANGLE_DEGREES_MAX,
            )?;
        }
        Shape::Box { size_m } => {
            for value in size_m {
                finite_range(
                    "shape.size_m",
                    *value,
                    SHAPE_DIMENSION_MIN_M,
                    SHAPE_DIMENSION_MAX_M,
                )?;
            }
        }
    }
    finite_range(
        "motion.speed_min_m_per_s",
        spec.motion.speed_min_m_per_s,
        0.0,
        SPEED_MAX_M_PER_S,
    )?;
    finite_range(
        "motion.speed_max_m_per_s",
        spec.motion.speed_max_m_per_s,
        0.0,
        SPEED_MAX_M_PER_S,
    )?;
    if spec.motion.speed_min_m_per_s > spec.motion.speed_max_m_per_s {
        return Err(EmitterError::value(
            "motion.speed_min_m_per_s",
            "must not exceed speed_max_m_per_s",
        ));
    }
    for value in spec.motion.gravity_m_per_s2 {
        finite_range(
            "motion.gravity_m_per_s2",
            value,
            -GRAVITY_MAX_M_PER_S2,
            GRAVITY_MAX_M_PER_S2,
        )?;
    }
    finite_range(
        "motion.drag_per_s",
        spec.motion.drag_per_s,
        0.0,
        DRAG_PER_S_MAX,
    )?;
    finite_range("look.size_start_m", spec.look.size_start_m, 0.0, SIZE_MAX_M)?;
    finite_range("look.size_end_m", spec.look.size_end_m, 0.0, SIZE_MAX_M)?;
    for (field, color) in [
        (
            "look.color_start_linear_rgba",
            spec.look.color_start_linear_rgba,
        ),
        (
            "look.color_end_linear_rgba",
            spec.look.color_end_linear_rgba,
        ),
    ] {
        for (index, value) in color.into_iter().enumerate() {
            let max = if index == 3 {
                COLOR_ALPHA_MAX
            } else {
                COLOR_RGB_MAX
            };
            finite_range(field, value, 0.0, max)?;
        }
    }
    if !(1..=MAX_PARTICLES_MAX).contains(&spec.limits.max_particles) {
        return Err(EmitterError::value(
            "limits.max_particles",
            "outside supported range",
        ));
    }
    Ok(())
}

fn finite_range(field: &'static str, value: f32, min: f32, max: f32) -> Result<(), EmitterError> {
    if !value.is_finite() || value < min || value > max {
        Err(EmitterError::value(
            field,
            format!("must be finite and in {min}..={max}"),
        ))
    } else {
        Ok(())
    }
}

fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && id.bytes().any(|byte| byte != b'0')
}

/// Canonical two-space pretty JSON with stable struct field order, no negative
/// zero, and exactly one trailing newline.
pub fn canonical_bytes(spec: &EmitterSpec) -> Result<Vec<u8>, EmitterError> {
    validate(spec)?;
    let mut normalized = spec.clone();
    normalize_zeros(&mut normalized);
    let mut bytes = serde_json::to_vec_pretty(&normalized)
        .map_err(|error| EmitterError::new("particle.decode", error.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn normalize_zeros(spec: &mut EmitterSpec) {
    fn value(value: &mut f32) {
        if *value == 0.0 {
            *value = 0.0;
        }
    }
    value(&mut spec.emission.rate_per_second);
    value(&mut spec.emission.duration_seconds);
    value(&mut spec.lifetime.min_seconds);
    value(&mut spec.lifetime.max_seconds);
    match &mut spec.shape {
        Shape::Point {} => {}
        Shape::Sphere { radius_m } => value(radius_m),
        Shape::Cone {
            radius_m,
            angle_degrees,
        } => {
            value(radius_m);
            value(angle_degrees);
        }
        Shape::Box { size_m } => size_m.iter_mut().for_each(value),
    }
    value(&mut spec.motion.speed_min_m_per_s);
    value(&mut spec.motion.speed_max_m_per_s);
    spec.motion.gravity_m_per_s2.iter_mut().for_each(value);
    value(&mut spec.motion.drag_per_s);
    value(&mut spec.look.size_start_m);
    value(&mut spec.look.size_end_m);
    spec.look.color_start_linear_rgba.iter_mut().for_each(value);
    spec.look.color_end_linear_rgba.iter_mut().for_each(value);
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Conservative upper estimate of simultaneously live particles. Rate emission
/// is continuous for each duration cycle; burst fires at each cycle start,
/// including t=0. Looping repeats the same duration forever. Arithmetic saturates
/// before clamping to the authored particle limit. This is an upper bound, not an
/// exact time-varying count. Callers should pass a validated spec.
pub fn max_live_estimate(spec: &EmitterSpec) -> u32 {
    let rate_lifetime = if spec.emission.looping {
        spec.lifetime.max_seconds
    } else {
        spec.lifetime
            .max_seconds
            .min(spec.emission.duration_seconds)
    };
    let lifetime = f64::from(rate_lifetime);
    let rate = (f64::from(spec.emission.rate_per_second) * lifetime).ceil();
    let cycle_count = if spec.emission.looping {
        (f64::from(spec.lifetime.max_seconds) / f64::from(spec.emission.duration_seconds)).ceil()
    } else {
        1.0
    };
    let burst = f64::from(spec.emission.burst_count) * cycle_count;
    let rate_count = if rate.is_finite() {
        rate.min(u32::MAX as f64) as u32
    } else {
        u32::MAX
    };
    let burst_count = if burst.is_finite() {
        burst.min(u32::MAX as f64) as u32
    } else {
        u32::MAX
    };
    rate_count
        .saturating_add(burst_count)
        .min(spec.limits.max_particles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_valid_and_canonical_round_trips() {
        let spec = default_emitter();
        validate(&spec).unwrap();
        let bytes = canonical_bytes(&spec).unwrap();
        let decoded: EmitterSpec = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded, spec);
        assert_eq!(canonical_bytes(&decoded).unwrap(), bytes);
        assert!(bytes.ends_with(b"\n"));
        assert!(bytes.windows(2).any(|window| window == b"\n "));
    }

    #[test]
    fn rejects_unknown_fields_and_reports_schema_before_typed_decode() {
        let mut value: serde_json::Value =
            serde_json::from_slice(&canonical_bytes(&default_emitter()).unwrap()).unwrap();
        value["extra"] = true.into();
        let path = temp_path("unknown");
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(load(&path).unwrap_err().code, "particle.decode");
        value["schema"] = 9.into();
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let error = load(&path).unwrap_err();
        assert_eq!(error.code, "particle.schema");
        assert!(error.detail.contains("found 9"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_hashes_exact_bytes_and_enforces_bound() {
        let path = temp_path("valid");
        let bytes = canonical_bytes(&default_emitter()).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(load(&path).unwrap().sha256_hex, sha256_hex(&bytes));
        std::fs::write(&path, vec![b' '; MAX_FILE_BYTES + 1]).unwrap();
        assert_eq!(load(&path).unwrap_err().code, "particle.limit");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_asset_has_its_own_code_and_id_path_is_stable() {
        let root = std::env::temp_dir().join(format!("arc-particle-spec-{}", std::process::id()));
        let id = "00000000000000000000000000000002";
        assert_eq!(
            asset_path(&root, id).unwrap(),
            root.join("assets/particles/00000000000000000000000000000002.emitter-v1.json")
        );
        assert_eq!(
            id_from_asset_path(&asset_path(&root, id).unwrap()),
            Some(id)
        );
        assert_eq!(load_asset(&root, id).unwrap_err().code, "particle.missing");
    }

    #[test]
    fn canonical_writer_normalizes_negative_zero() {
        let mut spec = default_emitter();
        spec.motion.gravity_m_per_s2[0] = -0.0;
        let bytes = canonical_bytes(&spec).unwrap();
        assert!(!bytes.windows(4).any(|window| window == b"-0.0"));
    }

    #[test]
    fn reports_stable_value_field_and_public_bounds() {
        let mut spec = default_emitter();
        spec.motion.drag_per_s = f32::NAN;
        let error = validate(&spec).unwrap_err();
        assert_eq!(error.code, "particle.value");
        assert_eq!(error.field, Some("motion.drag_per_s"));
        assert_eq!(
            numeric_bound("motion.drag_per_s").unwrap().max,
            DRAG_PER_S_MAX as f64
        );
        assert_eq!(
            numeric_bound("look.color_start_linear_rgba.rgb")
                .unwrap()
                .max,
            COLOR_RGB_MAX as f64
        );
        assert_eq!(
            numeric_bound("look.color_start_linear_rgba.alpha")
                .unwrap()
                .max,
            COLOR_ALPHA_MAX as f64
        );
    }

    #[test]
    fn estimates_looping_burst_and_continuous_emission_conservatively() {
        let mut spec = default_emitter();
        spec.emission.rate_per_second = 2.0;
        spec.emission.burst_count = 3;
        spec.emission.duration_seconds = 1.0;
        spec.lifetime.max_seconds = 2.1;
        // ceil(2*2.1) + 3*ceil(2.1/1)
        assert_eq!(max_live_estimate(&spec), 14);
        spec.limits.max_particles = 7;
        assert_eq!(max_live_estimate(&spec), 7);
    }

    #[test]
    fn looping_rate_estimate_uses_full_particle_lifetime() {
        let mut spec = default_emitter();
        spec.emission.rate_per_second = 10.0;
        spec.emission.duration_seconds = 1.0;
        spec.lifetime.max_seconds = 5.0;
        spec.limits.max_particles = 1_000;
        assert_eq!(max_live_estimate(&spec), 50);
    }

    fn temp_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "arc-particle-spec-{}-{label}.json",
            std::process::id()
        ))
    }
}
