//! Particle emitters as files of the game: listing, saving and removing them, editing one
//! with a history of its own, and a reference simulation to look at one by.
//!
//! An emitter is one file, `assets/particles/<id>.emitter-v1.json` under the game's folder,
//! in the shape `particle_spec` defines. That crate is the only description of an emitter
//! here: what a file may hold, what its numbers may be, how it is written and how many
//! particles it can have alive. This module adds what an editor needs around it and
//! depends on no renderer.
//!
//! - [`list_emitters`], [`save_emitter`], [`remove_emitter`]: the files. A save writes the
//!   spec's canonical bytes to a temporary file beside the target and renames it, and both
//!   a save and a removal are refused (`particle.conflict`) when the file is not what the
//!   editor last saw of it, so that nothing another editor wrote is lost.
//!
//!   Looking at the file and then writing or removing it are two steps, and between them
//!   another editor could write it. So each save and removal holds the emitter's file for
//!   itself from the look to the write, with a `.lock` file beside it that only one editor
//!   can make at a time; the other is refused (`particle.locked`) and has written nothing.
//!   A new emitter is also put in place in a way that fails if a file is there. What this
//!   does not cover: a program that makes no such lock file can still write between the
//!   look and the write, and an editor that stops in the middle leaves its lock file
//!   behind, which then has to be removed by hand.
//! - [`EmitterEdit`]: one emitter being edited. Every change is checked by the spec before
//!   it is taken, and can be undone and redone; the history is this emitter's and has
//!   nothing to do with a level's.
//! - [`EmitterPreview`]: the spec's written semantics, simulated on the processor with a
//!   seed, so that the same emitter always gives the same particles. It is a reference for
//!   reading an emitter's numbers by, and for the game's particle runtime to be compared
//!   with. It is not that runtime, and what it shows is not what the game draws.

use crate::persistence::PathLock;
use bevy_math::ops;
use editor_core::{EmitterAsset, EmitterAssetState, EmitterAssets};
use particle_runtime::AdapterError;
use particle_spec::{self as spec, Blend, EmitterError, EmitterSpec, LoadedEmitter, Shape};
use scene_data::{ContentHash, Diagnostic};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// The folder of the game that holds its emitters.
pub fn emitters_folder(game: &Path) -> PathBuf {
    game.join(spec::ASSET_DIRECTORY)
}

/// A refusal of the spec as the editor reports problems, with the file it is about.
pub fn emitter_diagnostic(error: &EmitterError, path: &Path) -> Diagnostic {
    value_diagnostic(error).with("path", path.display())
}

/// A refusal of the spec that is about no file: a value that was typed.
fn value_diagnostic(error: &EmitterError) -> Diagnostic {
    let refused = Diagnostic::new(error.code).with("detail", &error.detail);
    match error.field {
        Some(field) => refused.with("field", field),
        None => refused,
    }
}

fn io_refusal(code: &str, path: &Path, error: std::io::Error) -> Diagnostic {
    Diagnostic::new(code)
        .with("path", path.display())
        .with("error", error)
}

/// One file of the emitters' folder whose name ends as an emitter's does: read, or refused
/// with the reason. A file that cannot be used is listed all the same; it is there, and
/// hiding it would hide why an emitter is missing.
#[derive(Clone, Debug, PartialEq)]
pub struct ListedEmitter {
    /// The file's name without the emitters' ending. It is the emitter's ID if the file was
    /// read; a file that was refused may have a name that is no ID.
    pub id: String,
    pub path: PathBuf,
    pub loaded: Result<LoadedEmitter, Diagnostic>,
}

impl ListedEmitter {
    /// The emitter's name, if its file could be read.
    pub fn name(&self) -> Option<&str> {
        self.loaded
            .as_ref()
            .ok()
            .map(|loaded| loaded.spec.name.as_str())
    }
}

/// The emitters of a game: those that can be read first, by name and then by ID, then the
/// files that cannot. A game without the folder has none. Every file whose name ends as an
/// emitter's does is listed, one whose name is no ID as refused; other files are not looked
/// at.
pub fn list_emitters(game: &Path) -> Result<Vec<ListedEmitter>, Diagnostic> {
    let folder = emitters_folder(game);
    let entries = match std::fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_refusal("particle.read_failed", &folder, error)),
    };
    let mut listed = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| io_refusal("particle.read_failed", &folder, error))?;
        // The name as far as it is text: a name that is not is no ID either, and is refused
        // as one below.
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(spec::ASSET_SUFFIX) else {
            continue;
        };
        let path = entry.path();
        // A name that is no ID is refused by the spec before anything is read.
        listed.push(ListedEmitter {
            id: id.to_owned(),
            loaded: spec::load_asset(game, id).map_err(|error| emitter_diagnostic(&error, &path)),
            path,
        });
    }
    listed.sort_by(|a, b| {
        let key = |emitter: &ListedEmitter| {
            (
                emitter.loaded.is_err(),
                emitter.name().unwrap_or_default().to_lowercase(),
            )
        };
        key(a).cmp(&key(b)).then_with(|| a.id.cmp(&b.id))
    });
    Ok(listed)
}

/// The SHA-256 a hash's 64 hexadecimal characters stand for.
fn hash_of(hex: &str) -> Option<ContentHash> {
    let mut hash = [0u8; 32];
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    for (byte, pair) in hash.iter_mut().zip(hex.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(ContentHash(hash))
}

/// What the game's particle runtime says of an emitter it does not draw, as a refusal.
fn unsupported(refusal: AdapterError) -> Diagnostic {
    match refusal {
        AdapterError::UnsupportedField { field, reason } => {
            Diagnostic::new("emitter.runtime_unsupported")
                .with("field", field)
                .with("reason", reason)
        }
        AdapterError::InvalidSpec { detail, .. } => Diagnostic::new("emitter.asset_refused")
            .with("code", "particle.value")
            .with("detail", detail),
    }
}

/// The emitter asset with this stable ID as a level's cook, and the check before the game
/// is started, take it: the SHA-256 of the exact bytes of its saved file, read now. Never
/// a draft that is being edited, and never what was known of the file earlier.
///
/// Refused, and never passed over, when the level cannot be shown with it: there is no game
/// folder (`emitter.no_game`), the file is not there (`emitter.asset_missing`), it cannot be
/// read as an emitter (`emitter.asset_refused`, with the reader's `code`), or the game's
/// particle runtime does not draw it as it is (`emitter.runtime_unsupported`, with the
/// `field`). The caller says which placement it was.
pub fn cooked_emitter(game: Option<&Path>, emitter_id: &str) -> Result<ContentHash, Diagnostic> {
    let Some(game) = game else {
        return Err(Diagnostic::new("emitter.no_game"));
    };
    let loaded = spec::load_asset(game, emitter_id).map_err(|refusal| {
        if refusal.code == "particle.missing" {
            Diagnostic::new("emitter.asset_missing")
        } else {
            Diagnostic::new("emitter.asset_refused")
                .with("code", refusal.code)
                .with("detail", &refusal.detail)
        }
    })?;
    particle_runtime::supported(&loaded.spec).map_err(unsupported)?;
    hash_of(&loaded.sha256_hex).ok_or_else(|| {
        Diagnostic::new("emitter.asset_refused").with("code", "particle.read_failed")
    })
}

/// What is known of the game's emitter assets, for the editor's review of a level that
/// places emitters ([`editor_core::Command::SetEmitterAssets`]): each file of the game's
/// folder that is named as an emitter, and whether it can be read and drawn. Read from the
/// saved files now. Without a game folder there is no game and no asset.
///
/// This is for saying what is wrong while a level is edited. A cook does not go by it: it
/// reads each file again ([`cooked_emitter`]).
pub fn emitter_assets(game: Option<&Path>) -> EmitterAssets {
    let Some(game) = game else {
        return EmitterAssets::default();
    };
    // A folder that cannot be listed has no asset to name: its placements are then said
    // to miss their emitters, which is what a cook would find.
    let assets = list_emitters(game)
        .unwrap_or_default()
        .into_iter()
        .map(|listed| EmitterAsset {
            state: match &listed.loaded {
                Err(refusal) => EmitterAssetState::Refused {
                    code: refusal.code.clone(),
                },
                Ok(loaded) => match particle_runtime::supported(&loaded.spec) {
                    Ok(()) => EmitterAssetState::Usable,
                    Err(AdapterError::UnsupportedField { field, .. }) => {
                        EmitterAssetState::Unsupported {
                            field: field.to_owned(),
                        }
                    }
                    Err(AdapterError::InvalidSpec { .. }) => EmitterAssetState::Refused {
                        code: "particle.value".into(),
                    },
                },
            },
            id: listed.id,
        })
        .collect();
    EmitterAssets { game: true, assets }
}

/// What the editor last saw of an emitter's file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OnDisk {
    /// There was no file: the emitter is new.
    Absent,
    /// The file's bytes had this SHA-256.
    Sha256(String),
}

/// What is at `path` now.
fn on_disk_now(path: &Path) -> Result<OnDisk, Diagnostic> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(OnDisk::Absent),
        Err(error) => return Err(io_refusal("particle.read_failed", path, error)),
    };
    let mut bytes = Vec::new();
    // A file longer than an emitter can be is no emitter the editor wrote; its first bytes
    // are enough to tell that it is another file than the one last seen.
    file.take(spec::MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_refusal("particle.read_failed", path, error))?;
    Ok(OnDisk::Sha256(spec::sha256_hex(&bytes)))
}

/// Holds the emitter's file at `path` for one save or removal: a file of the same name with
/// `.lock` after it, which another editor that saves or removes the same emitter cannot make
/// while it is there. It is gone again when what is returned is dropped.
fn hold(path: &Path) -> Result<PathLock, Diagnostic> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    let lock = path.with_file_name(name);
    PathLock::acquire(lock.clone()).map_err(|refusal| {
        if refusal.code == "save.locked" {
            Diagnostic::new("particle.locked")
                .with("path", path.display())
                .with("lock", lock.display())
        } else {
            let error = refusal.parameters.get("detail").unwrap_or(&refusal.code);
            Diagnostic::new("particle.write_failed")
                .with("path", path.display())
                .with("error", error)
        }
    })
}

/// An emitter as it was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedEmitter {
    pub sha256_hex: String,
    pub path: PathBuf,
}

/// Writes an emitter to its file in the game's folder, as the spec's canonical bytes.
///
/// `expected` is what the editor last saw of that file. If the file is something else now,
/// nothing is written and the refusal is `particle.conflict`: a new emitter never takes the
/// place of a file that is there, and a save never undoes what another editor wrote. The
/// file is held from the look to the write ([`hold`]); while another editor holds it the
/// refusal is `particle.locked`.
pub fn save_emitter(
    game: &Path,
    emitter: &EmitterSpec,
    expected: &OnDisk,
) -> Result<SavedEmitter, Diagnostic> {
    let folder = emitters_folder(game);
    let path =
        spec::asset_path(game, &emitter.id).map_err(|error| emitter_diagnostic(&error, &folder))?;
    let bytes =
        spec::canonical_bytes(emitter).map_err(|error| emitter_diagnostic(&error, &path))?;
    if bytes.len() > spec::MAX_FILE_BYTES {
        return Err(Diagnostic::new("particle.limit")
            .with("path", path.display())
            .with("max_bytes", spec::MAX_FILE_BYTES));
    }
    let conflict = || Diagnostic::new("particle.conflict").with("path", path.display());
    let write_failed = |error: std::io::Error| io_refusal("particle.write_failed", &path, error);
    // A file that is something else is refused before anything is made, the folder too.
    if on_disk_now(&path)? != *expected {
        return Err(conflict());
    }
    std::fs::create_dir_all(&folder).map_err(write_failed)?;
    let _held = hold(&path)?;
    // Looked at again now that no other editor can write it.
    if on_disk_now(&path)? != *expected {
        return Err(conflict());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(&folder).map_err(write_failed)?;
    temporary.write_all(&bytes).map_err(write_failed)?;
    temporary.as_file().sync_all().map_err(write_failed)?;
    match expected {
        // A new emitter is put in place only if nothing is there, as one step: a file that
        // a program without the lock made in the meantime stays.
        OnDisk::Absent => temporary.persist_noclobber(&path).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                conflict()
            } else {
                write_failed(error.error)
            }
        })?,
        OnDisk::Sha256(_) => temporary
            .persist(&path)
            .map_err(|error| write_failed(error.error))?,
    };
    Ok(SavedEmitter {
        sha256_hex: spec::sha256_hex(&bytes),
        path,
    })
}

/// Removes an emitter's file. Refused with `particle.conflict` when the file is not what the
/// editor last saw of it, with `particle.missing` when there is none, and with
/// `particle.locked` while another editor holds it ([`hold`]).
pub fn remove_emitter(game: &Path, id: &str, expected: &OnDisk) -> Result<(), Diagnostic> {
    let folder = emitters_folder(game);
    let path = spec::asset_path(game, id).map_err(|error| emitter_diagnostic(&error, &folder))?;
    let seen = |now: OnDisk| {
        if now == OnDisk::Absent {
            Err(Diagnostic::new("particle.missing").with("path", path.display()))
        } else if now != *expected {
            Err(Diagnostic::new("particle.conflict").with("path", path.display()))
        } else {
            Ok(())
        }
    };
    // Before the file is held: without a file there may be no folder to hold it in.
    seen(on_disk_now(&path)?)?;
    let _held = hold(&path)?;
    // And again now that no other editor can write it.
    seen(on_disk_now(&path)?)?;
    std::fs::remove_file(&path).map_err(|error| io_refusal("particle.write_failed", &path, error))
}

/// The kinds of place an emitter's particles start in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Point,
    Sphere,
    Cone,
    Box,
}

impl ShapeKind {
    pub const ALL: [ShapeKind; 4] = [Self::Point, Self::Sphere, Self::Cone, Self::Box];

    pub fn of(shape: &Shape) -> Self {
        match shape {
            Shape::Point {} => Self::Point,
            Shape::Sphere { .. } => Self::Sphere,
            Shape::Cone { .. } => Self::Cone,
            Shape::Box { .. } => Self::Box,
        }
    }
}

/// The sizes a shape begins with when an emitter is given that kind of shape: half a metre
/// of radius for a sphere, a quarter for a cone that opens 25 degrees, a metre a side for a
/// box.
const NEW_SPHERE_RADIUS_M: f32 = 0.5;
const NEW_CONE_RADIUS_M: f32 = 0.25;
const NEW_CONE_ANGLE_DEGREES: f32 = 25.0;
const NEW_BOX_SIZE_M: [f32; 3] = [1.0; 3];

/// A number of an emitter by the dotted name the spec's bounds and refusals use for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumberField {
    pub field: &'static str,
    /// How many numbers the field holds: three for a size or a gravity, four for a colour.
    pub components: usize,
    /// Whether the number is a count, which has no fraction.
    pub whole: bool,
}

/// Every number of an emitter, in the order of the spec. Those of a shape are there for the
/// kinds of shape that have them ([`number`] says which).
pub const NUMBER_FIELDS: &[NumberField] = &[
    scalar("emission.rate_per_second"),
    count("emission.burst_count"),
    scalar("emission.duration_seconds"),
    scalar("lifetime.min_seconds"),
    scalar("lifetime.max_seconds"),
    scalar("shape.radius_m"),
    scalar("shape.angle_degrees"),
    vector("shape.size_m", 3),
    scalar("motion.speed_min_m_per_s"),
    scalar("motion.speed_max_m_per_s"),
    vector("motion.gravity_m_per_s2", 3),
    scalar("motion.drag_per_s"),
    scalar("look.size_start_m"),
    scalar("look.size_end_m"),
    vector("look.color_start_linear_rgba", 4),
    vector("look.color_end_linear_rgba", 4),
    count("limits.max_particles"),
];

const fn scalar(field: &'static str) -> NumberField {
    vector(field, 1)
}

const fn vector(field: &'static str, components: usize) -> NumberField {
    NumberField {
        field,
        components,
        whole: false,
    }
}

const fn count(field: &'static str) -> NumberField {
    NumberField {
        field,
        components: 1,
        whole: true,
    }
}

/// The least and the most a number of an emitter may be, from the spec's own bounds. The
/// first three numbers of a colour are its red, green and blue, the fourth its alpha, which
/// the spec bounds apart.
pub fn number_bound(field: &str, component: usize) -> Option<(f64, f64)> {
    let named = match (field.contains("color"), component) {
        (true, 3) => format!("{field}.alpha"),
        (true, _) => format!("{field}.rgb"),
        (false, _) => field.to_owned(),
    };
    spec::numeric_bound(&named)
        .or_else(|| spec::numeric_bound(field))
        .map(|bound| (bound.min, bound.max))
}

/// The number of `field` at `component`, or `None` when the emitter has no such number: the
/// name is not one of [`NUMBER_FIELDS`], the component is past its end, or it belongs to a
/// kind of shape the emitter does not have.
pub fn number(emitter: &EmitterSpec, field: &str, component: usize) -> Option<f64> {
    let at = |values: &[f32]| values.get(component).map(|value| f64::from(*value));
    let one = |value: f32| at(&[value]);
    let (emission, lifetime, motion, look) = (
        &emitter.emission,
        &emitter.lifetime,
        &emitter.motion,
        &emitter.look,
    );
    match field {
        "emission.rate_per_second" => one(emission.rate_per_second),
        "emission.burst_count" => (component == 0).then_some(f64::from(emission.burst_count)),
        "emission.duration_seconds" => one(emission.duration_seconds),
        "lifetime.min_seconds" => one(lifetime.min_seconds),
        "lifetime.max_seconds" => one(lifetime.max_seconds),
        "shape.radius_m" => match &emitter.shape {
            Shape::Sphere { radius_m } | Shape::Cone { radius_m, .. } => one(*radius_m),
            _ => None,
        },
        "shape.angle_degrees" => match &emitter.shape {
            Shape::Cone { angle_degrees, .. } => one(*angle_degrees),
            _ => None,
        },
        "shape.size_m" => match &emitter.shape {
            Shape::Box { size_m } => at(size_m),
            _ => None,
        },
        "motion.speed_min_m_per_s" => one(motion.speed_min_m_per_s),
        "motion.speed_max_m_per_s" => one(motion.speed_max_m_per_s),
        "motion.gravity_m_per_s2" => at(&motion.gravity_m_per_s2),
        "motion.drag_per_s" => one(motion.drag_per_s),
        "look.size_start_m" => one(look.size_start_m),
        "look.size_end_m" => one(look.size_end_m),
        "look.color_start_linear_rgba" => at(&look.color_start_linear_rgba),
        "look.color_end_linear_rgba" => at(&look.color_end_linear_rgba),
        "limits.max_particles" => {
            (component == 0).then_some(f64::from(emitter.limits.max_particles))
        }
        _ => None,
    }
}

/// Puts `value` where [`number`] reads it. `false` when the emitter has no such number.
fn set_number(emitter: &mut EmitterSpec, field: &str, component: usize, value: f64) -> bool {
    if number(emitter, field, component).is_none() {
        return false;
    }
    let number = value as f32;
    // A count has no fraction and is never less than nothing; what is typed is rounded.
    let whole = value.round().clamp(0.0, f64::from(u32::MAX)) as u32;
    let (emission, lifetime, motion, look) = (
        &mut emitter.emission,
        &mut emitter.lifetime,
        &mut emitter.motion,
        &mut emitter.look,
    );
    match field {
        "emission.rate_per_second" => emission.rate_per_second = number,
        "emission.burst_count" => emission.burst_count = whole,
        "emission.duration_seconds" => emission.duration_seconds = number,
        "lifetime.min_seconds" => lifetime.min_seconds = number,
        "lifetime.max_seconds" => lifetime.max_seconds = number,
        "shape.radius_m" => match &mut emitter.shape {
            Shape::Sphere { radius_m } | Shape::Cone { radius_m, .. } => *radius_m = number,
            _ => return false,
        },
        "shape.angle_degrees" => match &mut emitter.shape {
            Shape::Cone { angle_degrees, .. } => *angle_degrees = number,
            _ => return false,
        },
        "shape.size_m" => match &mut emitter.shape {
            Shape::Box { size_m } => size_m[component] = number,
            _ => return false,
        },
        "motion.speed_min_m_per_s" => motion.speed_min_m_per_s = number,
        "motion.speed_max_m_per_s" => motion.speed_max_m_per_s = number,
        "motion.gravity_m_per_s2" => motion.gravity_m_per_s2[component] = number,
        "motion.drag_per_s" => motion.drag_per_s = number,
        "look.size_start_m" => look.size_start_m = number,
        "look.size_end_m" => look.size_end_m = number,
        "look.color_start_linear_rgba" => look.color_start_linear_rgba[component] = number,
        "look.color_end_linear_rgba" => look.color_end_linear_rgba[component] = number,
        "limits.max_particles" => emitter.limits.max_particles = whole,
        _ => return false,
    }
    true
}

/// One change to an emitter.
#[derive(Clone, Debug, PartialEq)]
pub enum EmitterChange {
    Name(String),
    /// A number, by the dotted name the spec's bounds use and its place in a vector.
    Number {
        field: String,
        component: usize,
        value: f64,
    },
    /// Another kind of shape. A radius is kept between a sphere and a cone; what the new
    /// kind has and the old had not begins at a size of its own.
    Shape(ShapeKind),
    Blend(Blend),
    Looping(bool),
}

/// The emitter after `change`, if the spec takes it. A change the emitter has no place for,
/// or that makes a value the spec refuses, is refused with the spec's code and the field.
pub fn changed(emitter: &EmitterSpec, change: &EmitterChange) -> Result<EmitterSpec, Diagnostic> {
    let mut next = emitter.clone();
    match change {
        EmitterChange::Name(name) => next.name = name.trim().to_owned(),
        EmitterChange::Number {
            field,
            component,
            value,
        } => {
            if !value.is_finite() || !set_number(&mut next, field, *component, *value) {
                return Err(Diagnostic::new("particle.value").with("field", field).with(
                    "detail",
                    "the emitter has no such number, or it is not a number",
                ));
            }
        }
        EmitterChange::Shape(kind) => {
            let radius = match &emitter.shape {
                Shape::Sphere { radius_m } | Shape::Cone { radius_m, .. } => Some(*radius_m),
                _ => None,
            };
            if ShapeKind::of(&emitter.shape) != *kind {
                next.shape = match kind {
                    ShapeKind::Point => Shape::Point {},
                    ShapeKind::Sphere => Shape::Sphere {
                        radius_m: radius.unwrap_or(NEW_SPHERE_RADIUS_M),
                    },
                    ShapeKind::Cone => Shape::Cone {
                        radius_m: radius.unwrap_or(NEW_CONE_RADIUS_M),
                        angle_degrees: NEW_CONE_ANGLE_DEGREES,
                    },
                    ShapeKind::Box => Shape::Box {
                        size_m: NEW_BOX_SIZE_M,
                    },
                };
            }
        }
        EmitterChange::Blend(blend) => next.look.blend = *blend,
        EmitterChange::Looping(looping) => next.emission.looping = *looping,
    }
    spec::validate(&next).map_err(|error| value_diagnostic(&error))?;
    Ok(next)
}

/// One emitter being edited, with a history of its own.
#[derive(Clone, Debug)]
pub struct EmitterEdit {
    emitter: EmitterSpec,
    /// The emitter as its file has it, if it has a file that was read or written here.
    saved: Option<EmitterSpec>,
    on_disk: OnDisk,
    undo: Vec<EmitterSpec>,
    redo: Vec<EmitterSpec>,
}

impl EmitterEdit {
    /// How many changes can be undone. Older ones are forgotten.
    pub const HISTORY: usize = 200;

    /// An emitter that has no file yet.
    pub fn new(emitter: EmitterSpec) -> Self {
        Self {
            emitter,
            saved: None,
            on_disk: OnDisk::Absent,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// An emitter as it was read from its file.
    pub fn loaded(loaded: LoadedEmitter) -> Self {
        Self {
            saved: Some(loaded.spec.clone()),
            emitter: loaded.spec,
            on_disk: OnDisk::Sha256(loaded.sha256_hex),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn spec(&self) -> &EmitterSpec {
        &self.emitter
    }

    /// What the editor last saw of the emitter's file.
    pub fn on_disk(&self) -> &OnDisk {
        &self.on_disk
    }

    /// Whether the emitter differs from its file, or has none.
    pub fn unsaved(&self) -> bool {
        self.saved.as_ref() != Some(&self.emitter)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Makes a change, as one step that can be undone. `Ok(false)` when the change leaves
    /// the emitter as it is, which is no step. A change the spec refuses changes nothing.
    pub fn apply(&mut self, change: &EmitterChange) -> Result<bool, Diagnostic> {
        let next = changed(&self.emitter, change)?;
        if next == self.emitter {
            return Ok(false);
        }
        self.undo.push(std::mem::replace(&mut self.emitter, next));
        if self.undo.len() > Self::HISTORY {
            self.undo.remove(0);
        }
        self.redo.clear();
        Ok(true)
    }

    /// Takes the last change back. `false` when there is none.
    pub fn undo(&mut self) -> bool {
        let Some(before) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.emitter, before));
        true
    }

    /// Makes the change again that was last taken back. `false` when there is none.
    pub fn redo(&mut self) -> bool {
        let Some(after) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.emitter, after));
        true
    }

    /// Writes the emitter to its file in the game's folder ([`save_emitter`]). The history
    /// stays: a saved change can still be undone, and the emitter is unsaved again then.
    pub fn save(&mut self, game: &Path) -> Result<SavedEmitter, Diagnostic> {
        let saved = save_emitter(game, &self.emitter, &self.on_disk)?;
        self.saved = Some(self.emitter.clone());
        self.on_disk = OnDisk::Sha256(saved.sha256_hex.clone());
        Ok(saved)
    }
}

/// One particle of the reference simulation, as something to draw: where it is from the
/// emitter, in the emitter's own space with the emitter unturned, how wide it is, and its
/// colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewParticle {
    pub position_m: [f32; 3],
    pub size_m: f32,
    pub color_linear_rgba: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Live {
    position: [f32; 3],
    velocity: [f32; 3],
    age: f32,
    lifetime: f32,
}

/// The spec's semantics as a simulation: a reference, not the game's particle runtime.
///
/// It follows what `particle_spec` writes down. Particles are emitted continuously at the
/// rate while a cycle is active and in a burst at each cycle's start; each gets a lifetime
/// and a speed uniformly between their least and most, a place and a direction by its shape;
/// gravity pulls and drag slows it; its size and colour go linearly from start to end over
/// its life; it is gone when its age reaches its lifetime; and no more are alive than the
/// emitter's limit.
///
/// Time goes in fixed steps ([`EmitterPreview::STEP_SECONDS`]) and every random number
/// comes from the seed, so the same emitter and seed give the same particles at the same
/// step on every run. A particle is emitted at its exact time within a step and is as old
/// at the step's end as that makes it, so the count of the living never exceeds the spec's
/// `max_live_estimate`.
#[derive(Clone, Debug)]
pub struct EmitterPreview {
    emitter: EmitterSpec,
    seed: u64,
    random: u64,
    /// Whole steps taken since the start.
    steps: u64,
    /// Time handed to [`EmitterPreview::advance`] that has not made a whole step yet.
    owed_seconds: f64,
    /// Particles the rate has emitted, and cycles whose burst has fired.
    emitted: u64,
    cycles: u64,
    live: Vec<Live>,
}

impl EmitterPreview {
    /// The length of one step, in seconds.
    pub const STEP_SECONDS: f64 = 1.0 / 60.0;
    /// The most steps one call of [`EmitterPreview::advance`] takes: a preview that was
    /// left alone for a while catches up by this much and no more.
    pub const STEPS_AT_ONCE: u32 = 30;

    pub fn new(emitter: EmitterSpec, seed: u64) -> Self {
        let mut preview = Self {
            emitter,
            seed,
            random: 0,
            steps: 0,
            owed_seconds: 0.0,
            emitted: 0,
            cycles: 0,
            live: Vec::new(),
        };
        preview.restart();
        preview
    }

    /// Begins again at time zero, with the same seed: the same particles as before.
    pub fn restart(&mut self) {
        self.random = self.seed;
        self.steps = 0;
        self.owed_seconds = 0.0;
        self.emitted = 0;
        self.cycles = 0;
        self.live.clear();
    }

    /// Simulates another emitter from time zero, with the same seed.
    pub fn set_spec(&mut self, emitter: EmitterSpec) {
        self.emitter = emitter;
        self.restart();
    }

    pub fn spec(&self) -> &EmitterSpec {
        &self.emitter
    }

    /// The time the simulation has come to, in seconds.
    pub fn time_seconds(&self) -> f64 {
        self.steps as f64 * Self::STEP_SECONDS
    }

    /// How many particles are alive.
    pub fn live(&self) -> usize {
        self.live.len()
    }

    /// Whether nothing more will happen: an emitter that does not loop has emitted for its
    /// whole duration and its last particle is gone.
    pub fn finished(&self) -> bool {
        !self.emitter.emission.looping
            && self.time_seconds() >= f64::from(self.emitter.emission.duration_seconds)
            && self.live.is_empty()
    }

    /// Lets `seconds` pass: as many whole steps as that makes, with what is left over kept
    /// for the next call.
    pub fn advance(&mut self, seconds: f64) {
        if !(seconds.is_finite() && seconds > 0.0) {
            return;
        }
        self.owed_seconds += seconds;
        let mut taken = 0;
        while self.owed_seconds >= Self::STEP_SECONDS && taken < Self::STEPS_AT_ONCE {
            self.owed_seconds -= Self::STEP_SECONDS;
            self.step();
            taken += 1;
        }
        // What could not be caught up with is let go, not owed for ever.
        if taken == Self::STEPS_AT_ONCE {
            self.owed_seconds = 0.0;
        }
    }

    /// Takes one step.
    pub fn step(&mut self) {
        let dt = Self::STEP_SECONDS as f32;
        let end = (self.steps + 1) as f64 * Self::STEP_SECONDS;
        let emission = self.emitter.emission.clone();
        let duration = f64::from(emission.duration_seconds);

        // Those that are alive move, age and die.
        let motion = self.emitter.motion.clone();
        for particle in &mut self.live {
            moved(particle, &motion, dt);
        }
        self.live
            .retain(|particle| particle.age < particle.lifetime);

        // Bursts: one at the start of each cycle that has begun by the end of this step.
        while (emission.looping || self.cycles == 0) && self.cycles as f64 * duration <= end {
            let began = self.cycles as f64 * duration;
            for _ in 0..emission.burst_count {
                self.emit((end - began) as f32);
            }
            self.cycles += 1;
        }

        // The rate: the n-th particle leaves at n / rate seconds of emitting. An emitter
        // that does not loop emits for its duration and then no more.
        let rate = f64::from(emission.rate_per_second);
        if rate > 0.0 {
            let emitting_until = if emission.looping {
                end
            } else {
                end.min(duration)
            };
            let due = (rate * emitting_until).floor() as u64;
            while self.emitted < due {
                let left = (self.emitted + 1) as f64 / rate;
                self.emitted += 1;
                self.emit((end - left) as f32);
            }
        }
        self.steps += 1;
    }

    /// Emits one particle that left the emitter `age` seconds before the end of the step.
    fn emit(&mut self, age: f32) {
        let lifetime = {
            let (least, most) = (
                self.emitter.lifetime.min_seconds,
                self.emitter.lifetime.max_seconds,
            );
            least + self.unit() * (most - least)
        };
        let speed = {
            let (least, most) = (
                self.emitter.motion.speed_min_m_per_s,
                self.emitter.motion.speed_max_m_per_s,
            );
            least + self.unit() * (most - least)
        };
        let (position, direction) = self.place();
        // The random numbers are drawn whether the particle is kept or not, so that what
        // comes after is the same with a limit and without.
        if age >= lifetime || self.live.len() >= self.emitter.limits.max_particles as usize {
            return;
        }
        let mut particle = Live {
            position,
            velocity: direction.map(|axis| axis * speed),
            age: 0.0,
            lifetime,
        };
        let motion = self.emitter.motion.clone();
        moved(&mut particle, &motion, age);
        self.live.push(particle);
    }

    /// Where a particle starts and which way it goes, by the emitter's shape.
    fn place(&mut self) -> ([f32; 3], [f32; 3]) {
        const UP: [f32; 3] = [0.0, 1.0, 0.0];
        let outward = |position: [f32; 3]| {
            let length = position.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
            if length > f32::EPSILON {
                position.map(|axis| axis / length)
            } else {
                UP
            }
        };
        match self.emitter.shape.clone() {
            Shape::Point {} => ([0.0; 3], UP),
            Shape::Sphere { radius_m } => {
                // Uniform in the volume: a direction uniform on the sphere, and a distance
                // by the cube root.
                let height = 2.0 * self.unit() - 1.0;
                let turn = std::f32::consts::TAU * self.unit();
                let across = (1.0 - height * height).max(0.0).sqrt();
                let distance = radius_m * ops::cbrt(self.unit());
                let position = [across * ops::cos(turn), height, across * ops::sin(turn)]
                    .map(|axis| axis * distance);
                (position, outward(position))
            }
            Shape::Box { size_m } => {
                let position = [
                    (self.unit() - 0.5) * size_m[0],
                    (self.unit() - 0.5) * size_m[1],
                    (self.unit() - 0.5) * size_m[2],
                ];
                (position, outward(position))
            }
            Shape::Cone {
                radius_m,
                angle_degrees,
            } => {
                // Uniform on the disk, and a direction uniform in solid angle within the
                // half-angle about +Y.
                let distance = radius_m * self.unit().sqrt();
                let turn = std::f32::consts::TAU * self.unit();
                let position = [distance * ops::cos(turn), 0.0, distance * ops::sin(turn)];
                let least = ops::cos(angle_degrees.to_radians());
                let height = 1.0 - self.unit() * (1.0 - least);
                let around = std::f32::consts::TAU * self.unit();
                let across = (1.0 - height * height).max(0.0).sqrt();
                (
                    position,
                    [across * ops::cos(around), height, across * ops::sin(around)],
                )
            }
        }
    }

    /// The next random number, uniform from zero up to but not including one. SplitMix64:
    /// small, and the same on every machine.
    fn unit(&mut self) -> f32 {
        self.random = self.random.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut mixed = self.random;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        mixed ^= mixed >> 31;
        // The top 24 bits make every value a float holds exactly, and never one.
        (mixed >> 40) as f32 / (1u64 << 24) as f32
    }

    /// The particles that are alive, as things to draw.
    pub fn particles(&self) -> impl Iterator<Item = PreviewParticle> + '_ {
        let look = &self.emitter.look;
        self.live.iter().map(|particle| {
            let life = (particle.age / particle.lifetime).clamp(0.0, 1.0);
            let between = |start: f32, end: f32| start + (end - start) * life;
            PreviewParticle {
                position_m: particle.position,
                size_m: between(look.size_start_m, look.size_end_m),
                color_linear_rgba: std::array::from_fn(|channel| {
                    between(
                        look.color_start_linear_rgba[channel],
                        look.color_end_linear_rgba[channel],
                    )
                }),
            }
        })
    }
}

/// Moves a particle through `dt` seconds: gravity pulls it, drag slows it, and it is older.
fn moved(particle: &mut Live, motion: &spec::Motion, dt: f32) {
    // Drag takes the same share of the speed in every equal time, whatever the step.
    let kept = ops::exp(-motion.drag_per_s * dt);
    for axis in 0..3 {
        particle.velocity[axis] =
            (particle.velocity[axis] + motion.gravity_m_per_s2[axis] * dt) * kept;
        particle.position[axis] += particle.velocity[axis] * dt;
    }
    particle.age += dt;
}

#[cfg(test)]
mod tests {
    use super::*;
    use particle_spec::{default_emitter, max_live_estimate};

    fn id(number: u128) -> String {
        format!("{number:032x}")
    }

    /// The spec's starting emitter under this ID and name.
    fn emitter(number: u128, name: &str) -> EmitterSpec {
        EmitterSpec {
            id: id(number),
            name: name.into(),
            ..default_emitter()
        }
    }

    fn number_change(field: &str, component: usize, value: f64) -> EmitterChange {
        EmitterChange::Number {
            field: field.into(),
            component,
            value,
        }
    }

    #[test]
    fn a_game_without_emitters_has_none_and_saved_ones_are_listed_by_name() {
        let game = tempfile::tempdir().unwrap();
        assert_eq!(list_emitters(game.path()).unwrap(), []);

        for (number, name) in [(3, "smoke"), (1, "Sparks"), (2, "dust")] {
            save_emitter(game.path(), &emitter(number, name), &OnDisk::Absent).unwrap();
        }
        // Other files in the folder are not emitters and are not looked at.
        std::fs::write(emitters_folder(game.path()).join("notes.txt"), "x").unwrap();
        let listed = list_emitters(game.path()).unwrap();
        let names: Vec<_> = listed.iter().filter_map(ListedEmitter::name).collect();
        assert_eq!(names, ["dust", "smoke", "Sparks"]);
        assert_eq!(listed[0].id, id(2));
        assert!(listed[0]
            .path
            .ends_with(format!("{}.emitter-v1.json", id(2))));
    }

    #[test]
    fn a_file_that_is_no_emitter_is_listed_with_the_reason_and_after_the_others() {
        let game = tempfile::tempdir().unwrap();
        save_emitter(game.path(), &emitter(1, "Sparks"), &OnDisk::Absent).unwrap();
        let folder = emitters_folder(game.path());
        std::fs::write(folder.join(format!("{}.emitter-v1.json", id(9))), "{").unwrap();
        // A file whose name says one emitter and whose content says another.
        let other = spec::canonical_bytes(&emitter(7, "Other")).unwrap();
        std::fs::write(folder.join(format!("{}.emitter-v1.json", id(8))), other).unwrap();

        let listed = list_emitters(game.path()).unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].name(), Some("Sparks"));
        let refused: Vec<_> = listed[1..]
            .iter()
            .map(|emitter| {
                let refusal = emitter.loaded.as_ref().unwrap_err();
                assert!(refusal.parameters.contains_key("path"));
                (emitter.id.clone(), refusal.code.clone())
            })
            .collect();
        assert_eq!(
            refused,
            [
                (id(8), "particle.value".to_owned()),
                (id(9), "particle.decode".to_owned())
            ]
        );
    }

    #[test]
    fn a_file_named_as_an_emitter_with_a_name_that_is_no_id_is_listed_as_refused() {
        let game = tempfile::tempdir().unwrap();
        save_emitter(game.path(), &emitter(1, "Sparks"), &OnDisk::Absent).unwrap();
        let folder = emitters_folder(game.path());
        // A whole emitter under a name that is no ID: the name is what is wrong with it.
        let bytes = spec::canonical_bytes(&emitter(2, "Dust")).unwrap();
        std::fs::write(folder.join("bad-id.emitter-v1.json"), bytes).unwrap();

        let listed = list_emitters(game.path()).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].name(), Some("Sparks"));
        assert_eq!(listed[1].id, "bad-id");
        assert_eq!(listed[1].path, folder.join("bad-id.emitter-v1.json"));
        let refusal = listed[1].loaded.as_ref().unwrap_err();
        assert_eq!(refusal.code, "particle.value");
        assert_eq!(refusal.parameters["field"], "id");
        assert_eq!(
            refusal.parameters["path"],
            listed[1].path.display().to_string()
        );
    }

    #[test]
    fn a_save_writes_the_canonical_bytes_and_never_over_what_it_has_not_seen() {
        let game = tempfile::tempdir().unwrap();
        let sparks = emitter(1, "Sparks");
        let saved = save_emitter(game.path(), &sparks, &OnDisk::Absent).unwrap();
        let bytes = std::fs::read(&saved.path).unwrap();
        assert_eq!(bytes, spec::canonical_bytes(&sparks).unwrap());
        assert_eq!(saved.sha256_hex, spec::sha256_hex(&bytes));
        // Nothing is left in the folder but the emitter.
        assert_eq!(
            std::fs::read_dir(emitters_folder(game.path()))
                .unwrap()
                .count(),
            1
        );

        // As a new emitter again: the file is there, and stays.
        let conflict = save_emitter(game.path(), &sparks, &OnDisk::Absent).unwrap_err();
        assert_eq!(conflict.code, "particle.conflict");

        // With what was last seen, a changed emitter is written.
        let seen = OnDisk::Sha256(saved.sha256_hex.clone());
        let renamed = emitter(1, "Embers");
        let again = save_emitter(game.path(), &renamed, &seen).unwrap();
        assert_ne!(again.sha256_hex, saved.sha256_hex);
        // With what was seen before that, it is not: another writer has been here.
        let stale = save_emitter(game.path(), &sparks, &seen).unwrap_err();
        assert_eq!(stale.code, "particle.conflict");
        assert_eq!(
            list_emitters(game.path()).unwrap()[0].name(),
            Some("Embers")
        );
    }

    #[test]
    fn an_emitter_the_spec_refuses_is_not_written() {
        let game = tempfile::tempdir().unwrap();
        let mut broken = emitter(1, "Sparks");
        broken.lifetime.min_seconds = 5.0;
        broken.lifetime.max_seconds = 1.0;
        let refused = save_emitter(game.path(), &broken, &OnDisk::Absent).unwrap_err();
        assert_eq!(refused.code, "particle.value");
        assert!(refused.parameters.contains_key("field"));
        assert!(!emitters_folder(game.path()).exists());
        // Nor one whose ID is none.
        let mut nameless = emitter(1, "Sparks");
        nameless.id = "sparks".into();
        assert_eq!(
            save_emitter(game.path(), &nameless, &OnDisk::Absent)
                .unwrap_err()
                .code,
            "particle.value"
        );
    }

    #[test]
    fn a_removal_takes_the_file_that_was_last_seen_and_no_other() {
        let game = tempfile::tempdir().unwrap();
        let saved = save_emitter(game.path(), &emitter(1, "Sparks"), &OnDisk::Absent).unwrap();
        let seen = OnDisk::Sha256(saved.sha256_hex);
        let other = OnDisk::Sha256(spec::sha256_hex(b"something else"));
        assert_eq!(
            remove_emitter(game.path(), &id(1), &other)
                .unwrap_err()
                .code,
            "particle.conflict"
        );
        assert!(saved.path.exists());
        remove_emitter(game.path(), &id(1), &seen).unwrap();
        assert!(!saved.path.exists());
        assert_eq!(
            remove_emitter(game.path(), &id(1), &seen).unwrap_err().code,
            "particle.missing"
        );
    }

    #[test]
    fn a_file_another_editor_holds_is_neither_written_nor_removed_and_the_hold_is_let_go() {
        let game = tempfile::tempdir().unwrap();
        let sparks = emitter(1, "Sparks");
        let saved = save_emitter(game.path(), &sparks, &OnDisk::Absent).unwrap();
        let seen = OnDisk::Sha256(saved.sha256_hex.clone());
        let before = std::fs::read(&saved.path).unwrap();
        // A save and a removal let go of the file: no lock is left beside it.
        let lock = saved
            .path
            .with_file_name(format!("{}.emitter-v1.json.lock", id(1)));
        assert!(!lock.exists());

        // Another editor is in the middle of writing this emitter.
        std::fs::write(&lock, "").unwrap();
        let held = save_emitter(game.path(), &emitter(1, "Embers"), &seen).unwrap_err();
        assert_eq!(held.code, "particle.locked");
        assert_eq!(held.parameters["path"], saved.path.display().to_string());
        assert_eq!(held.parameters["lock"], lock.display().to_string());
        assert_eq!(
            remove_emitter(game.path(), &id(1), &seen).unwrap_err().code,
            "particle.locked"
        );
        assert_eq!(std::fs::read(&saved.path).unwrap(), before);
        // The other editor's lock is not this one's to take away, and it is no emitter.
        assert!(lock.exists());
        assert_eq!(list_emitters(game.path()).unwrap().len(), 1);
        // Another emitter is not held by it.
        save_emitter(game.path(), &emitter(2, "Dust"), &OnDisk::Absent).unwrap();

        // Once it is done, both go through, and a refused save leaves no lock either.
        std::fs::remove_file(&lock).unwrap();
        let stale = OnDisk::Sha256(spec::sha256_hex(b"something else"));
        assert_eq!(
            save_emitter(game.path(), &sparks, &stale).unwrap_err().code,
            "particle.conflict"
        );
        assert!(!lock.exists());
        save_emitter(game.path(), &emitter(1, "Embers"), &seen).unwrap();
        assert!(!lock.exists());
        let embers = on_disk_now(&saved.path).unwrap();
        remove_emitter(game.path(), &id(1), &embers).unwrap();
        assert!(!lock.exists() && !saved.path.exists());
    }

    #[test]
    fn a_cook_takes_the_hash_of_the_saved_file_or_says_why_it_cannot() {
        let game = tempfile::tempdir().unwrap();
        let code = |game: Option<&Path>, emitter: &str| cooked_emitter(game, emitter).unwrap_err();
        assert_eq!(code(None, &id(1)).code, "emitter.no_game");
        assert_eq!(
            code(Some(game.path()), &id(1)).code,
            "emitter.asset_missing"
        );

        // A point emitter: the hash is of the file's exact bytes.
        let saved = save_emitter(game.path(), &emitter(1, "Sparks"), &OnDisk::Absent).unwrap();
        let hash = cooked_emitter(Some(game.path()), &id(1)).unwrap();
        assert_eq!(hash.hex(), saved.sha256_hex);
        assert_eq!(
            hash,
            scene_data::payload_hash(&std::fs::read(&saved.path).unwrap())
        );

        // One the game's runtime does not draw: its field, and never a hash.
        let cone = changed(
            &emitter(2, "Fountain"),
            &EmitterChange::Shape(ShapeKind::Cone),
        )
        .unwrap();
        save_emitter(game.path(), &cone, &OnDisk::Absent).unwrap();
        let refused = code(Some(game.path()), &id(2));
        assert_eq!(refused.code, "emitter.runtime_unsupported");
        assert_eq!(refused.parameters["field"], "shape");

        // A file that is no emitter: the reader's code.
        let folder = emitters_folder(game.path());
        std::fs::write(folder.join(format!("{}.emitter-v1.json", id(3))), "{").unwrap();
        let refused = code(Some(game.path()), &id(3));
        assert_eq!(refused.code, "emitter.asset_refused");
        assert_eq!(refused.parameters["code"], "particle.decode");
        // And what is no ID names no file.
        assert_eq!(
            code(Some(game.path()), "sparks").code,
            "emitter.asset_refused"
        );
    }

    #[test]
    fn the_review_is_told_of_each_emitter_file_whether_it_can_be_read_and_drawn() {
        assert_eq!(emitter_assets(None), EmitterAssets::default());
        let game = tempfile::tempdir().unwrap();
        assert_eq!(
            emitter_assets(Some(game.path())),
            EmitterAssets {
                game: true,
                assets: vec![]
            }
        );
        save_emitter(game.path(), &emitter(1, "Sparks"), &OnDisk::Absent).unwrap();
        let cone = changed(
            &emitter(2, "Fountain"),
            &EmitterChange::Shape(ShapeKind::Cone),
        )
        .unwrap();
        save_emitter(game.path(), &cone, &OnDisk::Absent).unwrap();
        let folder = emitters_folder(game.path());
        std::fs::write(folder.join(format!("{}.emitter-v1.json", id(3))), "{").unwrap();

        let mut known = emitter_assets(Some(game.path()));
        assert!(known.game);
        known.assets.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(
            known.assets,
            [
                EmitterAsset {
                    id: id(1),
                    state: EmitterAssetState::Usable
                },
                EmitterAsset {
                    id: id(2),
                    state: EmitterAssetState::Unsupported {
                        field: "shape".into()
                    }
                },
                EmitterAsset {
                    id: id(3),
                    state: EmitterAssetState::Refused {
                        code: "particle.decode".into()
                    }
                },
            ]
        );
    }

    #[test]
    fn every_number_of_an_emitter_is_read_and_written_by_the_specs_own_name() {
        // An emitter of each kind of shape, so that every shape's numbers are there once.
        for kind in ShapeKind::ALL {
            let shaped = changed(&emitter(1, "Sparks"), &EmitterChange::Shape(kind)).unwrap();
            assert_eq!(ShapeKind::of(&shaped.shape), kind);
            for field in NUMBER_FIELDS {
                for component in 0..field.components {
                    let Some(was) = number(&shaped, field.field, component) else {
                        assert!(field.field.starts_with("shape."), "{}", field.field);
                        continue;
                    };
                    let (least, most) = number_bound(field.field, component)
                        .unwrap_or_else(|| panic!("{} has bounds", field.field));
                    assert!((least..=most).contains(&was), "{}: {was}", field.field);
                    // The number itself, written back, changes nothing.
                    let same = changed(&shaped, &number_change(field.field, component, was));
                    assert_eq!(same.unwrap(), shaped, "{}", field.field);
                }
                assert_eq!(number(&shaped, field.field, field.components), None);
            }
        }
        assert_eq!(number(&emitter(1, "Sparks"), "look.sparkle", 0), None);
        // A colour's alpha has bounds of its own.
        assert_eq!(
            number_bound("look.color_end_linear_rgba", 3),
            Some((0.0, 1.0))
        );
        assert_eq!(
            number_bound("look.color_end_linear_rgba", 0),
            Some((0.0, 16.0))
        );
    }

    #[test]
    fn a_change_is_checked_by_the_spec_and_a_refused_one_names_its_field() {
        let sparks = emitter(1, "Sparks");
        let faster = changed(
            &sparks,
            &number_change("emission.rate_per_second", 0, 250.0),
        );
        assert_eq!(faster.unwrap().emission.rate_per_second, 250.0);
        // A count is rounded to a whole number.
        let burst = changed(&sparks, &number_change("emission.burst_count", 0, 11.6)).unwrap();
        assert_eq!(burst.emission.burst_count, 12);

        for (change, field) in [
            (
                number_change("emission.rate_per_second", 0, 1.0e9),
                "emission.rate_per_second",
            ),
            (
                number_change("lifetime.min_seconds", 0, 50.0),
                "lifetime.min_seconds",
            ),
            (
                number_change("look.color_end_linear_rgba", 3, 2.0),
                "look.color_end_linear_rgba",
            ),
            // The default emitter is a point: it has no radius.
            (number_change("shape.radius_m", 0, 1.0), "shape.radius_m"),
            (
                number_change("look.size_start_m", 0, f64::NAN),
                "look.size_start_m",
            ),
            (EmitterChange::Name("   ".into()), "name"),
        ] {
            let refused = changed(&sparks, &change).unwrap_err();
            assert_eq!(refused.code, "particle.value", "{change:?}");
            let named = refused.parameters.get("field").expect("the field is named");
            assert!(named.starts_with(field), "{named} for {change:?}");
        }
    }

    #[test]
    fn another_kind_of_shape_keeps_a_radius_and_begins_the_rest() {
        let sparks = emitter(1, "Sparks");
        let sphere = changed(&sparks, &EmitterChange::Shape(ShapeKind::Sphere)).unwrap();
        let wider = changed(&sphere, &number_change("shape.radius_m", 0, 2.0)).unwrap();
        let cone = changed(&wider, &EmitterChange::Shape(ShapeKind::Cone)).unwrap();
        assert_eq!(
            cone.shape,
            Shape::Cone {
                radius_m: 2.0,
                angle_degrees: NEW_CONE_ANGLE_DEGREES
            }
        );
        let boxed = changed(&cone, &EmitterChange::Shape(ShapeKind::Box)).unwrap();
        assert_eq!(number(&boxed, "shape.size_m", 2), Some(1.0));
        assert_eq!(number(&boxed, "shape.radius_m", 0), None);
        // The same kind again is no change.
        assert_eq!(
            changed(&boxed, &EmitterChange::Shape(ShapeKind::Box)).unwrap(),
            boxed
        );
    }

    #[test]
    fn an_emitters_changes_are_undone_and_redone_in_its_own_history() {
        let mut edit = EmitterEdit::new(emitter(1, "Sparks"));
        assert!(edit.unsaved(), "it has no file yet");
        assert!(!edit.can_undo() && !edit.undo());

        assert!(edit.apply(&EmitterChange::Name("Embers".into())).unwrap());
        assert!(edit
            .apply(&number_change("emission.rate_per_second", 0, 40.0))
            .unwrap());
        // The same value again is no step.
        assert!(!edit
            .apply(&number_change("emission.rate_per_second", 0, 40.0))
            .unwrap());
        // A refused change is no step either, and changes nothing.
        let before = edit.spec().clone();
        assert!(edit
            .apply(&number_change("lifetime.max_seconds", 0, -1.0))
            .is_err());
        assert_eq!(edit.spec(), &before);

        assert!(edit.undo());
        assert_eq!(edit.spec().emission.rate_per_second, 10.0);
        assert_eq!(edit.spec().name, "Embers");
        assert!(edit.undo());
        assert_eq!(edit.spec().name, "Sparks");
        assert!(!edit.undo());
        assert!(edit.redo() && edit.redo() && !edit.redo());
        assert_eq!(edit.spec().emission.rate_per_second, 40.0);
        // A new change after an undo forgets what was undone.
        edit.undo();
        edit.apply(&EmitterChange::Looping(false)).unwrap();
        assert!(!edit.can_redo());
    }

    #[test]
    fn an_edited_emitter_is_saved_and_says_when_it_differs_from_its_file() {
        let game = tempfile::tempdir().unwrap();
        let mut edit = EmitterEdit::new(emitter(1, "Sparks"));
        let first = edit.save(game.path()).unwrap();
        assert!(!edit.unsaved());
        assert_eq!(edit.on_disk(), &OnDisk::Sha256(first.sha256_hex.clone()));

        edit.apply(&EmitterChange::Blend(Blend::Additive)).unwrap();
        assert!(edit.unsaved());
        edit.save(game.path()).unwrap();
        assert!(!edit.unsaved());
        // Undone, it differs from its file again; saved, the file has the older emitter.
        edit.undo();
        assert!(edit.unsaved());
        let back = edit.save(game.path()).unwrap();
        assert_eq!(back.sha256_hex, first.sha256_hex);

        // Opened from its file, an emitter is saved and has no history.
        let listed = list_emitters(game.path()).unwrap().remove(0);
        let mut opened = EmitterEdit::loaded(listed.loaded.unwrap());
        assert!(!opened.unsaved() && !opened.can_undo());
        // If another program writes the file meanwhile, the save is refused and nothing of
        // the edit is lost.
        std::fs::write(
            &back.path,
            spec::canonical_bytes(&emitter(1, "Theirs")).unwrap(),
        )
        .unwrap();
        opened.apply(&EmitterChange::Name("Mine".into())).unwrap();
        assert_eq!(
            opened.save(game.path()).unwrap_err().code,
            "particle.conflict"
        );
        assert!(opened.unsaved());
        assert_eq!(opened.spec().name, "Mine");
    }

    #[test]
    fn the_history_forgets_its_oldest_change_when_it_is_full() {
        let mut edit = EmitterEdit::new(emitter(1, "Sparks"));
        for step in 0..EmitterEdit::HISTORY + 5 {
            let value = 1.0 + step as f64;
            edit.apply(&number_change("emission.rate_per_second", 0, value))
                .unwrap();
        }
        let mut undone = 0;
        while edit.undo() {
            undone += 1;
        }
        assert_eq!(undone, EmitterEdit::HISTORY);
        assert_eq!(edit.spec().emission.rate_per_second, 5.0);
    }

    /// Runs a preview for so many seconds of its own time.
    fn after(emitter: &EmitterSpec, seed: u64, seconds: f64) -> EmitterPreview {
        let mut preview = EmitterPreview::new(emitter.clone(), seed);
        let steps = (seconds / EmitterPreview::STEP_SECONDS).round() as u64;
        for _ in 0..steps {
            preview.step();
        }
        preview
    }

    fn drawn(preview: &EmitterPreview) -> Vec<PreviewParticle> {
        preview.particles().collect()
    }

    #[test]
    fn the_same_emitter_and_seed_give_the_same_particles_and_a_restart_replays_them() {
        let sparks = changed(
            &emitter(1, "Sparks"),
            &EmitterChange::Shape(ShapeKind::Cone),
        )
        .unwrap();
        let first = after(&sparks, 7, 1.5);
        let second = after(&sparks, 7, 1.5);
        assert!(first.live() > 0);
        assert_eq!(drawn(&first), drawn(&second));
        assert_ne!(drawn(&first), drawn(&after(&sparks, 8, 1.5)));

        let mut replayed = first.clone();
        replayed.restart();
        assert_eq!((replayed.live(), replayed.time_seconds()), (0, 0.0));
        for _ in 0..90 {
            replayed.step();
        }
        assert_eq!(drawn(&replayed), drawn(&first));

        // Time handed over in pieces makes the same steps as time handed over whole.
        let mut pieces = EmitterPreview::new(sparks.clone(), 7);
        for _ in 0..180 {
            pieces.advance(EmitterPreview::STEP_SECONDS / 2.0);
        }
        assert_eq!(drawn(&pieces), drawn(&first));
    }

    #[test]
    fn no_more_particles_are_alive_than_the_spec_estimates_at_any_step() {
        let base = emitter(1, "Sparks");
        let with = |changes: &[EmitterChange]| {
            changes.iter().fold(base.clone(), |emitter, change| {
                changed(&emitter, change).unwrap()
            })
        };
        let cases = [
            base.clone(),
            // Looping in cycles shorter than a particle lives: emission never pauses.
            with(&[
                number_change("emission.duration_seconds", 0, 0.5),
                number_change("lifetime.max_seconds", 0, 5.0),
                number_change("emission.rate_per_second", 0, 37.0),
                number_change("emission.burst_count", 0, 9.0),
            ]),
            // Once, with a burst, for a time shorter than a life.
            with(&[
                EmitterChange::Looping(false),
                number_change("emission.duration_seconds", 0, 0.7),
                number_change("emission.rate_per_second", 0, 120.0),
                number_change("emission.burst_count", 0, 50.0),
            ]),
            // Bursts only.
            with(&[
                number_change("emission.rate_per_second", 0, 0.0),
                number_change("emission.burst_count", 0, 25.0),
                number_change("emission.duration_seconds", 0, 0.3),
            ]),
            // More than the limit lets live.
            with(&[
                number_change("emission.rate_per_second", 0, 5000.0),
                number_change("limits.max_particles", 0, 64.0),
            ]),
        ];
        for emitter in cases {
            let estimate = max_live_estimate(&emitter) as usize;
            let mut preview = EmitterPreview::new(emitter.clone(), 3);
            let mut most = 0;
            for _ in 0..(12.0 / EmitterPreview::STEP_SECONDS) as u32 {
                preview.step();
                most = most.max(preview.live());
                assert!(
                    preview.live() <= estimate,
                    "{} alive, at most {estimate} estimated: {emitter:?}",
                    preview.live()
                );
            }
            assert!(most > 0, "something was emitted: {emitter:?}");
            assert!(most <= emitter.limits.max_particles as usize);
        }
    }

    #[test]
    fn an_emitter_that_does_not_loop_emits_for_its_duration_and_then_ends() {
        let once = [
            EmitterChange::Looping(false),
            number_change("emission.duration_seconds", 0, 1.0),
            number_change("emission.burst_count", 0, 4.0),
        ]
        .iter()
        .fold(emitter(1, "Sparks"), |emitter, change| {
            changed(&emitter, change).unwrap()
        });
        // The burst at time zero and the rate's first particles are alive at once.
        let early = after(&once, 1, 0.5);
        assert!(early.live() >= 4 && !early.finished());
        // The longest life is two seconds: after duration plus that, none is left.
        let late = after(&once, 1, 3.1);
        assert_eq!(late.live(), 0);
        assert!(late.finished());
        // A looping emitter never ends.
        assert!(!after(&emitter(1, "Sparks"), 1, 3.1).finished());
    }

    #[test]
    fn particles_start_by_their_shape_fall_by_gravity_and_change_over_their_life() {
        // A point: every particle goes straight up at first and is pulled down. At ten a
        // second the first leaves after a tenth of a second, so it has risen a tenth later.
        let point = emitter(1, "Sparks");
        let risen = after(&point, 5, 0.2);
        assert!(risen.live() > 0);
        assert!(drawn(&risen)
            .iter()
            .all(|particle| { particle.position_m[0] == 0.0 && particle.position_m[2] == 0.0 }));
        assert!(drawn(&risen)
            .iter()
            .any(|particle| particle.position_m[1] > 0.0));
        let fallen = after(&point, 5, 1.9);
        assert!(drawn(&fallen)
            .iter()
            .any(|particle| particle.position_m[1] < 0.0));

        // The default emitter shrinks to nothing and fades out: no particle is wider than
        // it began, and the older are the more transparent.
        for particle in drawn(&fallen) {
            assert!((0.0..=0.1).contains(&particle.size_m), "{particle:?}");
            assert!((0.0..=1.0).contains(&particle.color_linear_rgba[3]));
        }

        // A cone without gravity: every particle stays within its half-angle about +Y, as
        // seen from where it started on the disk, and one that has just left is still on the
        // disk; and a box's particles start inside it.
        let cone = [
            EmitterChange::Shape(ShapeKind::Cone),
            number_change("motion.gravity_m_per_s2", 1, 0.0),
            number_change("shape.radius_m", 0, 0.001),
            number_change("shape.angle_degrees", 0, 20.0),
        ]
        .iter()
        .fold(emitter(1, "Sparks"), |emitter, change| {
            changed(&emitter, change).unwrap()
        });
        let widest = ops::tan(20.0_f32.to_radians()) + 0.01;
        for particle in drawn(&after(&cone, 2, 0.8)) {
            let [x, y, z] = particle.position_m;
            assert!(
                y >= 0.0 && (x * x + z * z).sqrt() <= y * widest + 0.002,
                "{particle:?}"
            );
        }
        let boxed = [
            number_change("motion.speed_min_m_per_s", 0, 0.0),
            number_change("motion.speed_max_m_per_s", 0, 0.0),
            number_change("motion.gravity_m_per_s2", 1, 0.0),
            EmitterChange::Shape(ShapeKind::Box),
        ]
        .iter()
        .fold(emitter(1, "Sparks"), |emitter, change| {
            changed(&emitter, change).unwrap()
        });
        let inside = drawn(&after(&boxed, 4, 0.5));
        assert!(!inside.is_empty());
        assert!(inside
            .iter()
            .all(|particle| particle.position_m.iter().all(|axis| axis.abs() <= 0.5)));
    }
}
