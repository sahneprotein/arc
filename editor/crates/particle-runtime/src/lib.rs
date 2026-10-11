//! Shared mapping from the pure `particle-spec` contract to Bevy Hanabi.
//!
//! The `hanabi-3d` feature gates all renderer dependencies. Applications use
//! [`build_effect`] to create the same effect asset and [`ParticleRuntimePlugin`]
//! to install its renderer runtime.
//!
//! # Current capability boundary
//!
//! The adapter currently supports point emitters with zero drag. It maps the
//! spec's seeded uniform speed and lifetime, world-space gravity, full-width
//! size and linear color gradients, alpha/additive blend, and particle limit
//! to Hanabi. Rate emission and cycle-start bursts (including finite and
//! looping durations) are scheduled by [`EmissionSchedule`] so both forms can
//! run together and finite emitters stop emitting at the requested duration.
//! Hanabi does not expose a live GPU particle count through this adapter; use
//! the pure particle-spec upper-bound estimate in overlays.
//!
//! Sphere, cone and box sources, and nonzero drag, are refused with
//! [`AdapterError::UnsupportedField`]. They are not approximated. The
//! `hanabi-3d` feature requires a Bevy 3D render app and a compute-capable GPU.
//! An HDR camera is needed to preserve/display color values above one. Effects
//! use Bevy visibility, including `RenderLayers`, so the effect and camera can
//! share a dedicated preview layer.
//!
//! This crate does not yet expose a typed Hanabi device/readiness error. A
//! successful [`build_effect`] only confirms that the spec maps to an effect;
//! it does not confirm that Hanabi initialized or that the GPU can run it.
//! Until the host app adds that readiness signal, editor previews should keep
//! the CPU reference simulation available and use it when GPU readiness is
//! absent or uncertain.

/// The emitter format this adapter is built on. An application that loads emitters and
/// builds effects from them uses the format through here, so that both are one type from
/// one source.
pub use particle_spec;

#[cfg(feature = "hanabi-3d")]
use particle_spec::Blend;
use particle_spec::{EmitterSpec, Shape};

#[cfg(feature = "hanabi-3d")]
use bevy::ecs::schedule::IntoScheduleConfigs;
#[cfg(feature = "hanabi-3d")]
use bevy_hanabi::EvalContext;

/// An effect asset and the CPU schedule needed to reproduce the spec's
/// simultaneous continuous-rate and cycle-start burst emission.
#[cfg(feature = "hanabi-3d")]
pub struct BuiltEffect {
    pub asset: bevy_hanabi::EffectAsset,
    pub emission: EmissionSchedule,
}

/// A typed refusal from the shared adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdapterError {
    InvalidSpec {
        field: Option<String>,
        detail: String,
    },
    UnsupportedField {
        field: &'static str,
        reason: &'static str,
    },
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSpec { field, detail } => match field {
                Some(field) => write!(f, "invalid emitter field {field}: {detail}"),
                None => f.write_str(detail),
            },
            Self::UnsupportedField { field, reason } => {
                write!(f, "unsupported emitter field {field}: {reason}")
            }
        }
    }
}

impl std::error::Error for AdapterError {}

impl From<particle_spec::EmitterError> for AdapterError {
    fn from(error: particle_spec::EmitterError) -> Self {
        Self::InvalidSpec {
            field: error.field.map(str::to_owned),
            detail: error.detail,
        }
    }
}

/// Per-instance emission schedule. Attach this alongside `ParticleEffect` and
/// `EffectSpawner`; the plugin writes the exact per-frame count after Hanabi's
/// ordinary spawner tick.
#[cfg(feature = "hanabi-3d")]
#[derive(bevy::prelude::Component, Clone, Debug)]
pub struct EmissionSchedule {
    rate_per_second: f64,
    burst_count: u32,
    duration_seconds: f64,
    looping: bool,
    elapsed_seconds: f64,
    started: bool,
    pub active: bool,
}

#[cfg(feature = "hanabi-3d")]
impl EmissionSchedule {
    fn from_spec(spec: &EmitterSpec) -> Self {
        Self {
            rate_per_second: f64::from(spec.emission.rate_per_second),
            burst_count: spec.emission.burst_count,
            duration_seconds: f64::from(spec.emission.duration_seconds),
            looping: spec.emission.looping,
            elapsed_seconds: 0.0,
            started: false,
            active: true,
        }
    }

    fn spawned_through(&self, time: f64) -> u64 {
        if self.looping {
            let cycles = (time / self.duration_seconds).floor() as u64;
            let bursts = cycles
                .saturating_add(1)
                .saturating_mul(self.burst_count as u64);
            bursts.saturating_add((self.rate_per_second * time).floor() as u64)
        } else {
            let active_time = time.min(self.duration_seconds);
            self.burst_count as u64 + (self.rate_per_second * active_time).floor() as u64
        }
    }

    /// Rewind the emission schedule. To clear particles already on the GPU,
    /// despawn and recreate the effect entity using the same asset and seed.
    pub fn restart(&mut self) {
        self.elapsed_seconds = 0.0;
        self.started = false;
        self.active = true;
    }

    /// Advance the deterministic schedule and return the spawn count for this
    /// frame. The caller supplies Hanabi's simulation delta.
    pub fn advance(&mut self, delta_seconds: f64) -> u32 {
        if !self.active || !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return 0;
        }
        let previous = self.elapsed_seconds;
        let next = if self.looping {
            previous + delta_seconds
        } else {
            (previous + delta_seconds).min(self.duration_seconds)
        };
        let before = if self.started {
            self.spawned_through(previous)
        } else {
            0
        };
        let after = self.spawned_through(next);
        self.started = true;
        self.elapsed_seconds = next;
        if !self.looping && next >= self.duration_seconds {
            self.active = false;
        }
        after.saturating_sub(before).min(u64::from(u32::MAX)) as u32
    }
}

/// Whether this adapter draws `spec`: the same refusals [`build_effect`] gives, without
/// building anything. It needs no renderer and no feature, so that an editor's review, a
/// cook and a client that has not started its renderer all say the same of an emitter.
pub fn supported(spec: &EmitterSpec) -> Result<(), AdapterError> {
    particle_spec::validate(spec)?;
    if !matches!(spec.shape, Shape::Point {}) {
        return Err(AdapterError::UnsupportedField {
            field: "shape",
            reason: "this adapter slice supports point emitters; sphere, cone and box are refused",
        });
    }
    if spec.motion.drag_per_s != 0.0 {
        return Err(AdapterError::UnsupportedField {
            field: "motion.drag_per_s",
            reason: "exact exponential drag mapping is not in this adapter slice",
        });
    }
    Ok(())
}

/// Build the shared Hanabi asset and per-instance emission schedule.
///
/// `seed` is copied to the Hanabi asset so all random expressions use the
/// same deterministic sequence for a given effect instance.
#[cfg(feature = "hanabi-3d")]
pub fn build_effect(spec: &EmitterSpec, seed: u32) -> Result<BuiltEffect, AdapterError> {
    supported(spec)?;

    let expressions = bevy_hanabi::ExprWriter::new();
    let lifetime = expressions
        .lit(spec.lifetime.min_seconds)
        .uniform(expressions.lit(spec.lifetime.max_seconds))
        .expr();
    let speed = expressions
        .lit(spec.motion.speed_min_m_per_s)
        .uniform(expressions.lit(spec.motion.speed_max_m_per_s))
        .expr();
    let initial_color = expressions
        .lit(bevy::prelude::Vec4::from_array(
            spec.look.color_start_linear_rgba,
        ))
        .expr();
    let gravity = (spec.motion.gravity_m_per_s2 != [0.0; 3]).then(|| {
        expressions
            .lit(bevy::prelude::Vec3::from_array(
                spec.motion.gravity_m_per_s2,
            ))
            .expr()
    });
    let initial_position = expressions.lit(bevy::prelude::Vec3::ZERO).expr();
    let module = expressions.finish();
    let spawner = bevy_hanabi::SpawnerSettings::once(1.0_f32.into());
    let mut asset = bevy_hanabi::EffectAsset::new(spec.limits.max_particles, spawner, module)
        .with_name(spec.name.clone())
        .with_simulation_space(bevy_hanabi::SimulationSpace::Global)
        .with_alpha_mode(match spec.look.blend {
            Blend::Alpha => bevy_hanabi::AlphaMode::Blend,
            Blend::Additive => bevy_hanabi::AlphaMode::Add,
        })
        .init(bevy_hanabi::SetAttributeModifier::new(
            bevy_hanabi::Attribute::POSITION,
            initial_position,
        ))
        .init(bevy_hanabi::SetAttributeModifier::new(
            bevy_hanabi::Attribute::LIFETIME,
            lifetime,
        ))
        .init(bevy_hanabi::SetAttributeModifier::new(
            bevy_hanabi::Attribute::HDR_COLOR,
            initial_color,
        ))
        .init(PointVelocity { speed });
    if let Some(gravity) = gravity {
        asset = asset.update(bevy_hanabi::AccelModifier::new(gravity));
    }
    asset.prng_seed = seed;
    // The two render gradients use the spec's normalized-age, linear endpoint
    // contract. `HDR_COLOR` keeps RGB values above one representable.
    asset = asset
        .render(bevy_hanabi::ColorOverLifetimeModifier::new(
            bevy_hanabi::Gradient::linear(
                bevy::prelude::Vec4::from_array(spec.look.color_start_linear_rgba),
                bevy::prelude::Vec4::from_array(spec.look.color_end_linear_rgba),
            ),
        ))
        .render(bevy_hanabi::SizeOverLifetimeModifier {
            gradient: bevy_hanabi::Gradient::linear(
                bevy::prelude::Vec3::splat(spec.look.size_start_m),
                bevy::prelude::Vec3::splat(spec.look.size_end_m),
            ),
            screen_space_size: false,
        });
    Ok(BuiltEffect {
        asset,
        emission: EmissionSchedule::from_spec(spec),
    })
}

#[cfg(feature = "hanabi-3d")]
#[derive(bevy::prelude::Reflect, Clone, Copy)]
struct PointVelocity {
    speed: bevy_hanabi::ExprHandle,
}

#[cfg(feature = "hanabi-3d")]
impl bevy_hanabi::Modifier for PointVelocity {
    fn context(&self) -> bevy_hanabi::ModifierContext {
        bevy_hanabi::ModifierContext::Init
    }

    fn attributes(&self) -> &[bevy_hanabi::Attribute] {
        &[bevy_hanabi::Attribute::VELOCITY]
    }

    fn boxed_clone(&self) -> bevy_hanabi::BoxedModifier {
        Box::new(*self)
    }

    fn apply(
        &self,
        module: &mut bevy_hanabi::Module,
        context: &mut bevy_hanabi::ShaderWriter,
    ) -> Result<(), bevy_hanabi::ExprError> {
        let speed = context.eval(module, self.speed)?;
        context.main_code.push_str(&format!(
            "particle.velocity = normalize(transform[1].xyz) * ({speed});\n"
        ));
        Ok(())
    }
}

/// Install Hanabi 3D rendering and the exact spec emission schedule override.
#[cfg(feature = "hanabi-3d")]
pub struct ParticleRuntimePlugin;

#[cfg(feature = "hanabi-3d")]
impl bevy::app::Plugin for ParticleRuntimePlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.add_plugins(bevy_hanabi::HanabiPlugin).add_systems(
            bevy::app::PostUpdate,
            drive_emission_schedules
                .after(bevy_hanabi::EffectSystems::CompileEffects)
                .after(bevy_hanabi::EffectSystems::TickSpawners),
        );
    }
}

#[cfg(feature = "hanabi-3d")]
fn drive_emission_schedules(
    time: bevy::prelude::Res<bevy::prelude::Time<bevy_hanabi::EffectSimulation>>,
    mut emitters: bevy::prelude::Query<(
        &bevy_hanabi::CompiledParticleEffect,
        &mut EmissionSchedule,
        &mut bevy_hanabi::EffectSpawner,
    )>,
) {
    let dt = time.delta_secs_f64();
    for (compiled, mut schedule, mut spawner) in &mut emitters {
        schedule_when_ready(&mut schedule, &mut spawner, compiled.is_ready(), dt);
    }
}

#[cfg(feature = "hanabi-3d")]
fn schedule_when_ready(
    schedule: &mut EmissionSchedule,
    spawner: &mut bevy_hanabi::EffectSpawner,
    ready: bool,
    delta_seconds: f64,
) {
    if ready {
        spawner.spawn_count = schedule.advance(delta_seconds);
    } else {
        spawner.spawn_count = 0;
    }
}

#[cfg(all(test, feature = "hanabi-3d"))]
mod tests {
    use super::*;

    #[test]
    fn builds_hanabi_asset_with_seed_capacity_and_blend() {
        let mut spec = particle_spec::default_emitter();
        spec.look.blend = Blend::Additive;
        spec.limits.max_particles = 777;
        let built = build_effect(&spec, 0x1234).unwrap();
        assert_eq!(built.asset.capacity(), 777);
        assert_eq!(built.asset.prng_seed, 0x1234);
        assert_eq!(built.asset.alpha_mode, bevy_hanabi::AlphaMode::Add);
        assert_eq!(
            built.asset.simulation_space,
            bevy_hanabi::SimulationSpace::Global
        );
        assert!(built
            .asset
            .particle_layout()
            .contains(bevy_hanabi::Attribute::POSITION));
    }

    #[test]
    fn finite_cycles_emit_rate_and_burst_together_then_stop() {
        let mut spec = particle_spec::default_emitter();
        spec.emission.rate_per_second = 2.0;
        spec.emission.burst_count = 3;
        spec.emission.duration_seconds = 1.0;
        spec.emission.looping = false;
        let mut schedule = EmissionSchedule::from_spec(&spec);
        assert_eq!(schedule.advance(0.25), 3);
        assert_eq!(schedule.advance(0.25), 1);
        assert_eq!(schedule.advance(0.5), 1);
        assert!(!schedule.active);
        assert_eq!(schedule.advance(1.0), 0);
    }

    #[test]
    fn looping_schedule_emits_burst_at_each_cycle_start() {
        let mut spec = particle_spec::default_emitter();
        spec.emission.rate_per_second = 2.0;
        spec.emission.burst_count = 3;
        spec.emission.duration_seconds = 1.0;
        let mut schedule = EmissionSchedule::from_spec(&spec);
        assert_eq!(schedule.advance(0.5), 4);
        assert_eq!(schedule.advance(0.5), 4);
        assert_eq!(schedule.advance(0.1), 0);
        schedule.restart();
        assert_eq!(schedule.advance(0.01), 3);
    }

    #[test]
    fn looping_rate_accumulates_fractional_counts_across_cycles() {
        let mut spec = particle_spec::default_emitter();
        spec.emission.rate_per_second = 10.0;
        spec.emission.duration_seconds = 0.25;
        spec.emission.burst_count = 0;
        let mut schedule = EmissionSchedule::from_spec(&spec);

        let counts = [
            schedule.advance(0.25),
            schedule.advance(0.25),
            schedule.advance(0.25),
            schedule.advance(0.25),
        ];
        assert_eq!(counts, [2, 3, 2, 3]);
        assert_eq!(counts.into_iter().sum::<u32>(), 10);
    }

    #[test]
    fn initial_burst_waits_until_compiled_effect_is_ready() {
        let mut spec = particle_spec::default_emitter();
        spec.emission.rate_per_second = 0.0;
        spec.emission.burst_count = 5;
        let mut schedule = EmissionSchedule::from_spec(&spec);
        let mut spawner = bevy_hanabi::EffectSpawner::default();

        schedule_when_ready(&mut schedule, &mut spawner, false, 0.1);
        assert_eq!(spawner.spawn_count, 0);
        assert_eq!(schedule.elapsed_seconds, 0.0);
        assert!(!schedule.started);

        schedule_when_ready(&mut schedule, &mut spawner, true, 0.1);
        assert_eq!(spawner.spawn_count, 5);
        assert_eq!(schedule.elapsed_seconds, 0.1);
        assert!(schedule.started);
    }

    #[test]
    fn unsupported_shapes_and_drag_fail_closed() {
        let mut spec = particle_spec::default_emitter();
        spec.shape = Shape::Sphere { radius_m: 1.0 };
        assert!(matches!(
            build_effect(&spec, 1),
            Err(AdapterError::UnsupportedField { field: "shape", .. })
        ));

        spec.shape = Shape::Point {};
        spec.motion.drag_per_s = 0.5;
        assert!(matches!(
            build_effect(&spec, 1),
            Err(AdapterError::UnsupportedField {
                field: "motion.drag_per_s",
                ..
            })
        ));
    }
}

#[cfg(not(feature = "hanabi-3d"))]
pub fn feature_is_enabled() -> bool {
    false
}

#[cfg(test)]
mod support_tests {
    use super::*;

    #[test]
    fn support_is_said_without_a_renderer_and_names_the_field_that_is_refused() {
        let point = particle_spec::default_emitter();
        assert_eq!(supported(&point), Ok(()));

        let mut cone = point.clone();
        cone.shape = Shape::Cone {
            radius_m: 0.25,
            angle_degrees: 25.0,
        };
        assert!(matches!(
            supported(&cone),
            Err(AdapterError::UnsupportedField { field: "shape", .. })
        ));

        let mut dragged = point.clone();
        dragged.motion.drag_per_s = 0.5;
        assert!(matches!(
            supported(&dragged),
            Err(AdapterError::UnsupportedField {
                field: "motion.drag_per_s",
                ..
            })
        ));

        // An emitter the format refuses is not one the adapter was asked about.
        let mut invalid = point;
        invalid.lifetime.min_seconds = 5.0;
        invalid.lifetime.max_seconds = 1.0;
        assert!(matches!(
            supported(&invalid),
            Err(AdapterError::InvalidSpec { .. })
        ));
    }
}
