//! The game's look profile: sun, sky, camera and materials as the client draws them.
//!
//! The game keeps one small JSON file, `assets/render/look-v1.json`, that its client reads
//! before it opens a window. The editor reads the same file so that its "Game look" draws
//! from the same numbers. This module is the editor's reader: it mirrors the file's schema
//! and depends on nothing of the game's.
//!
//! Schema 1 is closed: every field is required and an unknown field is refused, so a file
//! this reader takes says everything the look needs and nothing this reader ignores. The
//! names of presets (`aces_fitted`, `natural`, `earth`, `procedural_brick_v1`) stand for
//! engine presets and code recipes that the client pins; a name this reader does not know is
//! refused, never approximated.
//!
//! Refusals: `look.missing {path}`, `look.read_failed {path, error}`, `look.limit {path,
//! max_bytes}`, `look.decode {path, error}`, `look.schema {path, expected, found}`,
//! `look.value {path, field}`.

use scene_data::Diagnostic;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

/// The schema this reader takes.
pub const LOOK_SCHEMA: u64 = 1;
/// The largest profile file that is read, in bytes.
pub const LOOK_MAX_BYTES: u64 = 32 * 1024;

/// Where the game keeps its look profile, given the game's folder.
pub fn look_profile_path(game: &Path) -> PathBuf {
    game.join("assets").join("render").join("look-v1.json")
}

/// A profile that was read and found valid, and the SHA-256 of the exact bytes it was read
/// from, in hexadecimal: what the editor and the client can be compared by.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedLook {
    pub profile: LookProfile,
    pub sha256_hex: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookProfile {
    pub schema: u64,
    pub camera: LookCamera,
    pub sky: LookSky,
    pub sun: LookSun,
    pub boxes: LookBoxes,
    pub ground: LookGround,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookCamera {
    /// Must be `true` in schema 1: the engine's sky and its bloom, which the profile also
    /// asks for, only work on a camera that draws in high dynamic range. `false` is refused
    /// as a value that cannot be drawn.
    pub hdr: bool,
    pub exposure_ev100: f32,
    pub tonemapping: LookTonemapping,
    pub bloom: LookBloom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum LookTonemapping {
    #[serde(rename = "aces_fitted")]
    AcesFitted,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookBloom {
    pub preset: LookBloomPreset,
    pub intensity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum LookBloomPreset {
    #[serde(rename = "natural")]
    Natural,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookSky {
    pub atmosphere: LookAtmosphere,
    pub scattering_lut_size: [u32; 2],
    pub environment_map: bool,
    pub ambient_light: LookAmbient,
    pub clear_color_linear_rgba: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum LookAtmosphere {
    #[serde(rename = "earth")]
    Earth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum LookAmbient {
    #[serde(rename = "none")]
    None,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookSun {
    pub illuminance_lux: f32,
    pub color_linear_rgba: [f32; 4],
    /// The sun shines from here towards `look_at_m`.
    pub position_m: [f32; 3],
    pub look_at_m: [f32; 3],
    pub shadows: LookShadows,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookShadows {
    pub enabled: bool,
    pub map_size: u32,
    pub first_cascade_far_bound_m: f32,
    pub maximum_distance_m: f32,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookBoxes {
    pub base_color_linear_rgba: [f32; 4],
    pub roughness: f32,
    pub texture: LookBoxTexture,
    /// How many metres one repeat of the texture covers.
    pub texture_repeat_m: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum LookBoxTexture {
    /// [`procedural_brick_v1`].
    #[serde(rename = "procedural_brick_v1")]
    ProceduralBrickV1,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookGround {
    pub base_color_linear_rgba: [f32; 4],
    pub roughness: f32,
    pub paint: LookGroundPaint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum LookGroundPaint {
    /// The ground's paint is in its vertices, as linear RGBA.
    #[serde(rename = "linear_vertex_rgba")]
    LinearVertexRgba,
}

/// Reads and checks the profile at `path`. At most [`LOOK_MAX_BYTES`] are read; nothing is
/// started and nothing waits.
pub fn load_look_profile(path: &Path) -> Result<LoadedLook, Diagnostic> {
    let read_failed = |error: std::io::Error| {
        let code = if error.kind() == std::io::ErrorKind::NotFound {
            "look.missing"
        } else {
            "look.read_failed"
        };
        let refused = Diagnostic::new(code).with("path", path.display());
        if code == "look.missing" {
            refused
        } else {
            refused.with("error", error)
        }
    };
    let file = std::fs::File::open(path).map_err(read_failed)?;
    let mut bytes = Vec::new();
    // One byte more than is allowed tells a file that is too long from one that just fits.
    file.take(LOOK_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(read_failed)?;
    if bytes.len() as u64 > LOOK_MAX_BYTES {
        return Err(Diagnostic::new("look.limit")
            .with("path", path.display())
            .with("max_bytes", LOOK_MAX_BYTES));
    }
    parse_look_profile(&bytes, path)
}

/// Checks the bytes of a profile. `path` is only named in a refusal.
pub fn parse_look_profile(bytes: &[u8], path: &Path) -> Result<LoadedLook, Diagnostic> {
    let decode = |error: serde_json::Error| {
        Diagnostic::new("look.decode")
            .with("path", path.display())
            .with("error", error)
    };
    // The schema is read first and by itself: a file of another schema is said to be of
    // another schema, not to have fields this reader does not know.
    let json: serde_json::Value = serde_json::from_slice(bytes).map_err(decode)?;
    let found = json.get("schema").and_then(serde_json::Value::as_u64);
    if found != Some(LOOK_SCHEMA) {
        let found = match json.get("schema") {
            Some(value) => value.to_string(),
            None => "none".into(),
        };
        return Err(Diagnostic::new("look.schema")
            .with("path", path.display())
            .with("expected", LOOK_SCHEMA)
            .with("found", found));
    }
    let profile: LookProfile = serde_json::from_value(json).map_err(decode)?;
    if let Some(field) = first_bad_value(&profile) {
        return Err(Diagnostic::new("look.value")
            .with("path", path.display())
            .with("field", field));
    }
    Ok(LoadedLook {
        profile,
        sha256_hex: Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    })
}

/// Where the engine's shadow cascades begin, in metres: the default of its cascade builder,
/// which neither the client nor the editor changes. The builder refuses, by stopping the
/// program, a first cascade that does not end beyond it, so the profile may not ask for one.
pub const SHADOW_MINIMUM_DISTANCE_M: f32 = 0.1;

/// The first field whose value a renderer cannot take, by its place in the file.
fn first_bad_value(profile: &LookProfile) -> Option<&'static str> {
    let finite = |values: &[f32]| values.iter().all(|value| value.is_finite());
    let color = |values: &[f32; 4]| finite(values) && values.iter().all(|value| *value >= 0.0);
    let unit = |value: f32| (0.0..=1.0).contains(&value);
    let (camera, sky, sun) = (&profile.camera, &profile.sky, &profile.sun);
    let (shadows, boxes, ground) = (&sun.shadows, &profile.boxes, &profile.ground);
    let checks: [(&'static str, bool); 17] = [
        ("camera.hdr", camera.hdr),
        (
            "camera.exposure_ev100",
            (-16.0..=32.0).contains(&camera.exposure_ev100),
        ),
        ("camera.bloom.intensity", unit(camera.bloom.intensity)),
        (
            "sky.scattering_lut_size",
            sky.scattering_lut_size
                .iter()
                .all(|size| (1..=4096).contains(size)),
        ),
        (
            "sky.clear_color_linear_rgba",
            color(&sky.clear_color_linear_rgba),
        ),
        (
            "sun.illuminance_lux",
            sun.illuminance_lux.is_finite() && sun.illuminance_lux >= 0.0,
        ),
        ("sun.color_linear_rgba", color(&sun.color_linear_rgba)),
        ("sun.position_m", finite(&sun.position_m)),
        (
            "sun.look_at_m",
            finite(&sun.look_at_m) && sun.look_at_m != sun.position_m,
        ),
        (
            "sun.shadows.map_size",
            shadows.map_size.is_power_of_two() && (16..=16384).contains(&shadows.map_size),
        ),
        (
            "sun.shadows.first_cascade_far_bound_m",
            shadows.first_cascade_far_bound_m.is_finite()
                && shadows.first_cascade_far_bound_m > SHADOW_MINIMUM_DISTANCE_M,
        ),
        (
            "sun.shadows.maximum_distance_m",
            shadows.maximum_distance_m.is_finite()
                && shadows.maximum_distance_m > shadows.first_cascade_far_bound_m,
        ),
        (
            "boxes.base_color_linear_rgba",
            color(&boxes.base_color_linear_rgba),
        ),
        ("boxes.roughness", unit(boxes.roughness)),
        (
            "boxes.texture_repeat_m",
            boxes.texture_repeat_m.is_finite() && boxes.texture_repeat_m > 0.0,
        ),
        (
            "ground.base_color_linear_rgba",
            color(&ground.base_color_linear_rgba),
        ),
        ("ground.roughness", unit(ground.roughness)),
    ];
    checks
        .into_iter()
        .find(|(_, good)| !good)
        .map(|(field, _)| field)
}

/// A square texture with every mip level, as sRGB RGBA8 texels: the largest level first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LookTexture {
    /// The width and the height of the largest level, in texels.
    pub size: u32,
    /// One level after another in one buffer, each half as wide as the one before, down to
    /// one texel.
    pub texels: Vec<u8>,
    pub levels: u32,
}

/// The side of [`procedural_brick_v1`], in texels.
pub const BRICK_V1_SIZE: u32 = 16;

/// The box texture the profile names `procedural_brick_v1`: the game's brick pattern, 16
/// texels square, bricks 8 wide and 4 tall with every other row offset by half a brick, and
/// its mip chain, each level the average of 2 by 2 texels of the one before in linear light.
/// It is sampled nearest and repeats.
///
/// The recipe is the client's (`crates/client/src/level_mesh.rs`), texel for texel. It is a
/// version: another pattern gets another name in the profile, and this reader refuses a name
/// it has no recipe for.
pub fn procedural_brick_v1() -> LookTexture {
    let size = BRICK_V1_SIZE;
    let mut base = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let row = y / 4;
            let across = (x + if row % 2 == 0 { 0 } else { 4 }) % 8;
            let mortar = y % 4 == 3 || across == 7;
            let shade = ((x * 7 + y * 13) % 5) as u8 * 6;
            let texel: [u8; 4] = if mortar {
                [150, 145, 135, 255]
            } else {
                [176 - shade, 92 - shade / 2, 70 - shade / 2, 255]
            };
            base.extend_from_slice(&texel);
        }
    }
    let (texels, levels) = mip_chain(&base, size);
    LookTexture {
        size,
        texels,
        levels,
    }
}

fn srgb_to_linear(channel: u8) -> f32 {
    let channel = f32::from(channel) / 255.0;
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(channel: f32) -> u8 {
    let channel = channel.clamp(0.0, 1.0);
    let srgb = if channel <= 0.003_130_8 {
        channel * 12.92
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0).round() as u8
}

/// Every mip level of a square sRGB RGBA8 image whose side is a power of two, the largest
/// first, in one buffer, and how many levels there are. Colour is averaged in linear light,
/// alpha as it is.
fn mip_chain(base: &[u8], size: u32) -> (Vec<u8>, u32) {
    let mut data = base.to_vec();
    let mut levels = 1;
    let (mut start, mut width) = (0, size as usize);
    while width > 1 {
        let next = width / 2;
        let mut level = Vec::with_capacity(next * next * 4);
        for y in 0..next {
            for x in 0..next {
                let texel = |dx: usize, dy: usize| {
                    let at = start + ((2 * y + dy) * width + 2 * x + dx) * 4;
                    &data[at..at + 4]
                };
                let quad = [texel(0, 0), texel(1, 0), texel(0, 1), texel(1, 1)];
                for channel in 0..3 {
                    let sum: f32 = quad.iter().map(|t| srgb_to_linear(t[channel])).sum();
                    level.push(linear_to_srgb(sum / 4.0));
                }
                let alpha: u32 = quad.iter().map(|t| u32::from(t[3])).sum();
                level.push(((alpha + 2) / 4) as u8);
            }
        }
        start += width * width * 4;
        data.extend_from_slice(&level);
        width = next;
        levels += 1;
    }
    (data, levels)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The profile as the game's side gave its first version.
    pub(crate) const V1: &str = r#"{
        "schema": 1,
        "camera": {
            "hdr": true,
            "exposure_ev100": 13,
            "tonemapping": "aces_fitted",
            "bloom": { "preset": "natural", "intensity": 0.12 }
        },
        "sky": {
            "atmosphere": "earth",
            "scattering_lut_size": [256, 256],
            "environment_map": true,
            "ambient_light": "none",
            "clear_color_linear_rgba": [0, 0, 0, 1]
        },
        "sun": {
            "illuminance_lux": 130000,
            "color_linear_rgba": [1, 1, 1, 1],
            "position_m": [1, 0.9, 0.6],
            "look_at_m": [0, 0, 0],
            "shadows": {
                "enabled": true,
                "map_size": 4096,
                "first_cascade_far_bound_m": 8,
                "maximum_distance_m": 60
            }
        },
        "boxes": {
            "base_color_linear_rgba": [1, 1, 1, 1],
            "roughness": 1,
            "texture": "procedural_brick_v1",
            "texture_repeat_m": 1
        },
        "ground": {
            "base_color_linear_rgba": [1, 1, 1, 1],
            "roughness": 0.9,
            "paint": "linear_vertex_rgba"
        }
    }"#;

    fn parse(json: &str) -> Result<LoadedLook, Diagnostic> {
        parse_look_profile(json.as_bytes(), Path::new("look-v1.json"))
    }

    fn refusal(json: &str) -> Diagnostic {
        parse(json).expect_err("the profile is refused")
    }

    #[test]
    fn the_first_version_is_read_with_every_number_and_named_by_its_bytes() {
        let loaded = parse(V1).unwrap();
        let profile = &loaded.profile;
        assert_eq!(profile.schema, 1);
        assert_eq!(profile.camera.exposure_ev100, 13.0);
        assert_eq!(profile.camera.tonemapping, LookTonemapping::AcesFitted);
        assert_eq!(profile.camera.bloom.intensity, 0.12);
        assert_eq!(profile.sky.scattering_lut_size, [256, 256]);
        assert_eq!(profile.sky.ambient_light, LookAmbient::None);
        assert_eq!(profile.sun.illuminance_lux, 130_000.0);
        assert_eq!(profile.sun.position_m, [1.0, 0.9, 0.6]);
        assert_eq!(profile.sun.shadows.map_size, 4096);
        assert_eq!(profile.boxes.texture, LookBoxTexture::ProceduralBrickV1);
        assert_eq!(profile.ground.roughness, 0.9);
        // The identity is of the bytes: the same bytes, the same name; one more, another.
        assert_eq!(loaded.sha256_hex.len(), 64);
        assert_eq!(parse(V1).unwrap().sha256_hex, loaded.sha256_hex);
        let spaced = format!("{V1}\n");
        assert_ne!(parse(&spaced).unwrap().sha256_hex, loaded.sha256_hex);
        assert_eq!(parse(&spaced).unwrap().profile, loaded.profile);
    }

    #[test]
    fn a_file_that_is_not_there_too_long_or_not_json_is_refused_by_its_own_code() {
        let folder = tempfile::tempdir().unwrap();
        let path = look_profile_path(folder.path());
        assert!(path.ends_with("assets/render/look-v1.json"));
        let missing = load_look_profile(&path).unwrap_err();
        assert_eq!(missing.code, "look.missing");
        assert_eq!(
            missing.parameters.get("path"),
            Some(&path.display().to_string())
        );

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, V1).unwrap();
        assert_eq!(load_look_profile(&path).unwrap(), parse(V1).unwrap());

        // As long as is allowed, and one byte longer.
        let padded = |length: u64| format!("{V1}{}", " ".repeat(length as usize - V1.len()));
        std::fs::write(&path, padded(LOOK_MAX_BYTES)).unwrap();
        assert!(load_look_profile(&path).is_ok());
        std::fs::write(&path, padded(LOOK_MAX_BYTES + 1)).unwrap();
        let long = load_look_profile(&path).unwrap_err();
        assert_eq!(long.code, "look.limit");
        assert_eq!(
            long.parameters.get("max_bytes").map(String::as_str),
            Some("32768")
        );

        std::fs::write(&path, "not json").unwrap();
        let undecodable = load_look_profile(&path).unwrap_err();
        assert_eq!(undecodable.code, "look.decode");
        assert!(undecodable.parameters.contains_key("error"));

        // A folder where the file should be cannot be read.
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert_eq!(
            load_look_profile(&path).unwrap_err().code,
            "look.read_failed"
        );
    }

    #[test]
    fn another_schema_is_said_to_be_another_schema() {
        let newer = refusal(&V1.replace("\"schema\": 1", "\"schema\": 2"));
        assert_eq!(newer.code, "look.schema");
        assert_eq!(
            (
                newer.parameters.get("expected").map(String::as_str),
                newer.parameters.get("found").map(String::as_str)
            ),
            (Some("1"), Some("2"))
        );
        let none = refusal(&V1.replace("\"schema\": 1,", ""));
        assert_eq!(none.code, "look.schema");
        assert_eq!(
            none.parameters.get("found").map(String::as_str),
            Some("none")
        );
        assert_eq!(
            refusal(&V1.replace("\"schema\": 1", "\"schema\": \"1\"")).code,
            "look.schema"
        );
    }

    #[test]
    fn a_field_too_many_a_field_too_few_and_an_unknown_preset_are_not_decoded() {
        for changed in [
            // Unknown fields, at the top and further in.
            V1.replace("\"schema\": 1,", "\"schema\": 1, \"fog\": true,"),
            V1.replace("\"hdr\": true,", "\"hdr\": true, \"gamma\": 2.2,"),
            // A required field is not there.
            V1.replace("\"roughness\": 0.9,", ""),
            V1.replace("\"environment_map\": true,", ""),
            // Presets this reader has no recipe for.
            V1.replace("aces_fitted", "agx"),
            V1.replace("procedural_brick_v1", "procedural_brick_v2"),
            V1.replace("linear_vertex_rgba", "srgb_vertex_rgba"),
            V1.replace("\"earth\"", "\"mars\""),
            // A number where a list is wanted.
            V1.replace("[256, 256]", "256"),
        ] {
            assert_ne!(changed, V1, "the replacement found its text");
            assert_eq!(refusal(&changed).code, "look.decode", "{changed}");
        }
    }

    #[test]
    fn a_value_a_renderer_cannot_take_is_refused_with_the_name_of_its_field() {
        for (old, new, field) in [
            // The sky and the bloom the profile asks for need a camera in high dynamic range.
            ("\"hdr\": true", "\"hdr\": false", "camera.hdr"),
            (
                "\"exposure_ev100\": 13",
                "\"exposure_ev100\": 99",
                "camera.exposure_ev100",
            ),
            (
                "\"intensity\": 0.12",
                "\"intensity\": 1.5",
                "camera.bloom.intensity",
            ),
            ("[256, 256]", "[256, 0]", "sky.scattering_lut_size"),
            (
                "\"illuminance_lux\": 130000",
                "\"illuminance_lux\": -1",
                "sun.illuminance_lux",
            ),
            (
                "\"look_at_m\": [0, 0, 0]",
                "\"look_at_m\": [1, 0.9, 0.6]",
                "sun.look_at_m",
            ),
            (
                "\"map_size\": 4096",
                "\"map_size\": 3000",
                "sun.shadows.map_size",
            ),
            (
                "\"maximum_distance_m\": 60",
                "\"maximum_distance_m\": 4",
                "sun.shadows.maximum_distance_m",
            ),
            // The engine's cascades begin at 0.1 m; a first cascade must end beyond that.
            (
                "\"first_cascade_far_bound_m\": 8",
                "\"first_cascade_far_bound_m\": 0.1",
                "sun.shadows.first_cascade_far_bound_m",
            ),
            (
                "\"first_cascade_far_bound_m\": 8",
                "\"first_cascade_far_bound_m\": 0.05",
                "sun.shadows.first_cascade_far_bound_m",
            ),
            ("\"roughness\": 1,", "\"roughness\": 2,", "boxes.roughness"),
            (
                "\"texture_repeat_m\": 1",
                "\"texture_repeat_m\": 0",
                "boxes.texture_repeat_m",
            ),
        ] {
            let changed = V1.replace(old, new);
            assert_ne!(changed, V1, "{old}");
            let refused = refusal(&changed);
            assert_eq!(refused.code, "look.value", "{field}");
            assert_eq!(
                refused.parameters.get("field").map(String::as_str),
                Some(field)
            );
        }
    }

    #[test]
    fn the_brick_is_the_games_sixteen_texels_with_a_mip_chain_down_to_one() {
        let brick = procedural_brick_v1();
        assert_eq!((brick.size, brick.levels), (16, 5));
        let texels: usize = [16, 8, 4, 2, 1].iter().map(|width| width * width).sum();
        assert_eq!(brick.texels.len(), texels * 4);
        let at = |x: usize, y: usize| &brick.texels[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4];
        // The first texel is brick, unshaded; every fourth row and every eighth column of a
        // row is mortar, and the next row of bricks is offset by half a brick.
        assert_eq!(at(0, 0), [176, 92, 70, 255]);
        assert_eq!(at(0, 3), [150, 145, 135, 255]);
        assert_eq!(at(7, 0), [150, 145, 135, 255]);
        assert_eq!(at(3, 4), [150, 145, 135, 255]);
        assert_ne!(at(7, 4), [150, 145, 135, 255]);
        assert!(brick.texels.chunks(4).all(|texel| texel[3] == 255));
    }

    #[test]
    fn a_mip_level_is_the_average_in_linear_light() {
        // Two white and two black texels average to linear 0.5, which is sRGB 188.
        let base = [
            255, 255, 255, 255, 0, 0, 0, 255, //
            0, 0, 0, 255, 255, 255, 255, 255,
        ];
        let (data, levels) = mip_chain(&base, 2);
        assert_eq!(levels, 2);
        assert_eq!(&data[16..], &[188, 188, 188, 255]);
    }
}
