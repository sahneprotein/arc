//! The Particles tool's stage: what the 3D view shows while that tool is chosen.
//!
//! The level is not shown then. In its place are a ground grid, the outline of the chosen
//! emitter's shape a metre above the grid's middle, and the particles of the preview, each a
//! square that faces the camera. The particles are those of the editor's reference
//! simulation (`editor_ui::particles::Preview`): this draws what that says and works out
//! nothing of its own. It is not the game's particle runtime, and the panel says so.
//!
//! The chosen emitter can also be drawn by the game's particle runtime, on the graphics
//! card: the GPU preview. The effect is built by the adapter the game shares
//! (`particle_runtime::build_effect`), so what is drawn is what the game would make of the
//! same settings. The stage says what became of it (`editor_ui::GpuPreview`) and nothing it
//! does not know: the effect counts as running only once the graphics card has said that it
//! can draw it. An emitter the adapter refuses, a device that cannot run the runtime and an
//! effect that does not start are each said as that, and the reference simulation is drawn
//! in their place. The stage never draws both.
//!
//! The stage is on a render layer of its own, and the view's camera is moved to that layer
//! while the tool is chosen: the level's meshes stay as they are and are neither drawn nor
//! clicked on. The camera turns about the emitter; where it was in the level is remembered
//! and put back when another tool is chosen, and so is the way the level was drawn.

use std::time::{Duration, Instant};

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::{NoFrustumCulling, RenderLayers},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::renderer::RenderDevice,
    window::RequestRedraw,
};
use bevy_hanabi::{
    CompiledParticleEffect, EffectAsset, EffectSimulation, EffectSimulationTime, ParticleEffect,
};
use editor_ui::{
    particles::{EmitterSpec, PreviewParticle, Shape},
    theme::{color, feathers::to_color},
    GpuPreview, Particles, Preview, PreviewMode, Tool, ViewMode,
};
use particle_runtime::{build_effect, AdapterError, ParticleRuntimePlugin};

use crate::viewport::{Orbit, ViewCamera};

/// The render layer of the stage. The level is on the first layer, which is layer zero.
const STAGE_LAYER: usize = 1;

/// Where the emitter is on the stage: a metre above the middle of the grid.
pub const EMITTER_AT: Vec3 = Vec3::new(0.0, 1.0, 0.0);

/// How many cells the ground grid has along each side, of a metre each.
const GRID_CELLS: u32 = 10;

/// Half the length of the mark of an emitter that is a point, in metres.
const POINT_MARK_M: f32 = 0.1;

/// How long the lines are that show how far a cone opens, in metres.
const CONE_LINE_M: f32 = 1.0;

/// Where the camera looks from when the stage is first shown: at the emitter, from near
/// enough to see single particles.
const STAGE_ORBIT: Orbit = Orbit {
    target: Vec3::new(0.0, 1.5, 0.0),
    yaw: 0.6,
    pitch: 0.25,
    distance: 8.0,
};

/// The lines of the stage: its grid and the emitter's shape. A group of their own, so that
/// they are drawn on the stage's layer and nothing else is.
#[derive(Default, Reflect, GizmoConfigGroup)]
struct StageLines;

/// The mesh that holds every particle of the preview, and the two ways it is laid over what
/// is behind it.
#[derive(Resource)]
struct ParticleLooks {
    mesh: Handle<Mesh>,
    alpha: Handle<StandardMaterial>,
    additive: Handle<StandardMaterial>,
}

/// The entity that draws the particles.
#[derive(Component)]
struct ParticleQuads;

/// Whether the stage is shown, and what is put back when it no longer is.
#[derive(Resource, Default)]
pub struct Stage {
    on: bool,
    /// Where the camera was in the level, and how the level was drawn.
    level: Option<(Orbit, ViewMode)>,
    /// Where the camera was on the stage when it was last left.
    orbit: Option<Orbit>,
    /// How many particles were drawn in the last frame, for a scripted check.
    pub drawn: usize,
}

impl Stage {
    pub fn on(&self) -> bool {
        self.on
    }
}

pub struct StagePlugin;

impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ParticleRuntimePlugin)
            .init_gizmo_group::<StageLines>()
            .init_resource::<Stage>()
            .init_resource::<GpuStage>()
            .add_systems(Startup, setup)
            .add_systems(Update, (switch_stage, drive_gpu, draw_stage).chain());
    }
}

/// How many bind groups the game's particle runtime needs of the graphics device.
const BIND_GROUPS_NEEDED: u32 = 4;

/// How long an effect may take until the graphics card says it can draw it. Its shaders
/// are compiled in that time; an effect that is not drawn by then is taken as not started.
const GPU_START: Duration = Duration::from_secs(5);

/// What a graphics device with this many bind groups lacks for the particle runtime, if it
/// lacks anything: the one thing the runtime itself asks of a device before it starts.
pub fn device_lacks(bind_groups: u32) -> Option<String> {
    (bind_groups < BIND_GROUPS_NEEDED).then(|| {
        format!(
            "it has {bind_groups} bind groups and the particle runtime needs {BIND_GROUPS_NEEDED}"
        )
    })
}

/// What became of an emitter the adapter refused, as the panel is told it.
pub fn refused(refusal: AdapterError) -> GpuPreview {
    match refusal {
        AdapterError::UnsupportedField { field, reason } => GpuPreview::Unsupported {
            field: field.to_owned(),
            reason: reason.to_owned(),
        },
        AdapterError::InvalidSpec { field, detail } => GpuPreview::Failed(match field {
            Some(field) => format!("the particle runtime refused {field}: {detail}"),
            None => format!("the particle runtime refused the emitter: {detail}"),
        }),
    }
}

/// The GPU preview's effect and what is known of it.
#[derive(Resource, Default)]
struct GpuStage {
    /// What the graphics device lacks for the runtime, once it was looked at.
    lacks: Option<Option<String>>,
    /// The emitter an effect was last asked for, and how many times the preview had been
    /// begun again by then: another emitter, or another beginning, is another effect.
    asked: Option<(EmitterSpec, u32)>,
    /// The effect, if the adapter built one.
    effect: Option<Entity>,
    /// When the effect was made, or was last known to be drawn.
    alive_at: Option<Instant>,
    /// What became of the last asking, if it is settled: refused, or not started.
    settled: Option<GpuPreview>,
}

impl GpuStage {
    fn remove_effect(&mut self, commands: &mut Commands) {
        if let Some(effect) = self.effect.take() {
            commands.entity(effect).despawn();
        }
        self.alive_at = None;
    }
}

/// Has the game's particle runtime draw the chosen emitter while the stage is shown and the
/// GPU preview is asked for, and says what became of it. The effect is made anew for
/// another emitter, for a changed one, and when the preview is begun again: an effect on the
/// graphics card cannot be wound back. Pause and the speeds are the runtime's own clock.
fn drive_gpu(
    mut commands: Commands,
    (stage, particles, preview): (Res<Stage>, Res<Particles>, Res<Preview>),
    (mut gpu, mut status): (ResMut<GpuStage>, ResMut<GpuPreview>),
    (device, compiled): (Option<Res<RenderDevice>>, Query<&CompiledParticleEffect>),
    (mut effects, mut clock): (ResMut<Assets<EffectAsset>>, ResMut<Time<EffectSimulation>>),
    mut redraw: MessageWriter<RequestRedraw>,
) {
    let emitter = particles
        .chosen()
        .filter(|_| stage.on && preview.mode == PreviewMode::Gpu);
    let now = match emitter {
        None => {
            // Not asked for: nothing is kept, and the next asking begins anew.
            if gpu.effect.is_some() || gpu.asked.is_some() {
                gpu.remove_effect(&mut commands);
                gpu.asked = None;
                gpu.settled = None;
            }
            GpuPreview::Off
        }
        Some(emitter) => {
            if gpu.lacks.is_none() {
                // The engine gives the main world the device it renders with. Where it is
                // not to be had, nothing is known to be lacking: whether the effect is
                // drawn is then the graphics card's to say, below.
                gpu.lacks = Some(
                    device
                        .as_ref()
                        .and_then(|device| device_lacks(device.limits().max_bind_groups)),
                );
            }
            if let Some(Some(lacks)) = &gpu.lacks {
                GpuPreview::Unavailable(lacks.clone())
            } else {
                let asked = gpu.asked.as_ref().is_some_and(|(spec, restarts)| {
                    spec == emitter && *restarts == preview.restarts()
                });
                if !asked {
                    gpu.remove_effect(&mut commands);
                    gpu.asked = Some((emitter.clone(), preview.restarts()));
                    gpu.settled = match build_effect(emitter, Preview::SEED as u32) {
                        Ok(built) => {
                            let effect = commands
                                .spawn((
                                    ParticleEffect::new(effects.add(built.asset)),
                                    built.emission,
                                    Transform::from_translation(EMITTER_AT),
                                    RenderLayers::layer(STAGE_LAYER),
                                    Pickable::IGNORE,
                                ))
                                .id();
                            gpu.effect = Some(effect);
                            gpu.alive_at = Some(Instant::now());
                            None
                        }
                        Err(refusal) => Some(refused(refusal)),
                    };
                }
                let ready = gpu
                    .effect
                    .and_then(|effect| compiled.get(effect).ok())
                    .is_some_and(CompiledParticleEffect::is_ready);
                if let Some(settled) = &gpu.settled {
                    settled.clone()
                } else if ready {
                    gpu.alive_at = Some(Instant::now());
                    GpuPreview::Running
                } else if gpu.alive_at.is_some_and(|at| at.elapsed() > GPU_START) {
                    // Whatever the reason, it is in the log and not known here.
                    gpu.remove_effect(&mut commands);
                    let failed = GpuPreview::Failed(format!(
                        "the graphics card did not draw the effect within {} seconds, and \
                         the editor's log says why",
                        GPU_START.as_secs()
                    ));
                    gpu.settled = Some(failed.clone());
                    failed
                } else {
                    // Frames must keep coming for the answer to arrive.
                    redraw.write(RequestRedraw);
                    GpuPreview::Starting
                }
            }
        }
    };
    if *status != now {
        *status = now;
    }
    // The runtime's clock follows the preview's controls, whether an effect is there or not.
    if clock.is_paused() == preview.running {
        if preview.running {
            clock.unpause();
        } else {
            clock.pause();
        }
    }
    let speed = preview.speed.factor() as f32;
    if clock.relative_speed() != speed {
        clock.set_relative_speed(speed);
    }
    if status.running() && preview.running {
        redraw.write(RequestRedraw);
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut gizmos: ResMut<GizmoConfigStore>,
) {
    gizmos.config_mut::<StageLines>().0.render_layers = RenderLayers::layer(STAGE_LAYER);
    // A particle's colour is the one the simulation gives it, whatever light there is, and
    // it is seen from both sides.
    let look = |alpha_mode| StandardMaterial {
        unlit: true,
        alpha_mode,
        cull_mode: None,
        double_sided: true,
        ..default()
    };
    let looks = ParticleLooks {
        mesh: meshes.add(quads(&[], Vec3::X, Vec3::Y, Vec3::Z)),
        alpha: materials.add(look(AlphaMode::Blend)),
        additive: materials.add(look(AlphaMode::Add)),
    };
    commands.spawn((
        ParticleQuads,
        Mesh3d(looks.mesh.clone()),
        MeshMaterial3d(looks.alpha.clone()),
        RenderLayers::layer(STAGE_LAYER),
        // The mesh changes every frame, and what it holds is small and near the emitter.
        NoFrustumCulling,
        Pickable::IGNORE,
        Visibility::Hidden,
    ));
    commands.insert_resource(looks);
}

/// Shows the stage when the Particles tool is chosen and the level again when another is.
fn switch_stage(
    mut commands: Commands,
    tool: Res<Tool>,
    mut stage: ResMut<Stage>,
    mut orbit: ResMut<Orbit>,
    mut view: ResMut<ViewMode>,
    camera: Single<Entity, With<ViewCamera>>,
) {
    let on = *tool == Tool::Particles;
    if on == stage.on {
        return;
    }
    stage.on = on;
    if on {
        stage.level = Some((*orbit, *view));
        *orbit = stage.orbit.unwrap_or(STAGE_ORBIT);
        // The stage is drawn the editor's own way: the game's look is of the level.
        if *view != ViewMode::Editor {
            *view = ViewMode::Editor;
        }
        commands
            .entity(*camera)
            .insert(RenderLayers::layer(STAGE_LAYER));
    } else {
        stage.orbit = Some(*orbit);
        if let Some((level_orbit, level_view)) = stage.level.take() {
            *orbit = level_orbit;
            if *view != level_view {
                *view = level_view;
            }
        }
        commands.entity(*camera).remove::<RenderLayers>();
    }
}

/// The particles as one mesh: a square for each, as wide as the particle, facing along
/// `back` and spanned by `right` and `up`, in the particle's colour. The farthest along
/// `back` come first, so that those in front are drawn over those behind.
///
/// Without particles the mesh is one square of no size at the emitter, which draws nothing.
/// It is never without vertices: the engine keeps no place for such a mesh and then says,
/// as an error, that it was asked to fill one.
fn quads(particles: &[PreviewParticle], right: Vec3, up: Vec3, back: Vec3) -> Mesh {
    const NOTHING: [PreviewParticle; 1] = [PreviewParticle {
        position_m: [0.0; 3],
        size_m: 0.0,
        color_linear_rgba: [0.0; 4],
    }];
    let particles = if particles.is_empty() {
        &NOTHING[..]
    } else {
        particles
    };
    let mut order: Vec<&PreviewParticle> = particles.iter().collect();
    order.sort_by(|a, b| {
        let depth = |particle: &PreviewParticle| Vec3::from_array(particle.position_m).dot(back);
        depth(a).total_cmp(&depth(b))
    });
    let mut positions = Vec::with_capacity(order.len() * 4);
    let mut colors = Vec::with_capacity(order.len() * 4);
    let mut uvs = Vec::with_capacity(order.len() * 4);
    let mut indices = Vec::with_capacity(order.len() * 6);
    for (index, particle) in order.into_iter().enumerate() {
        let centre = EMITTER_AT + Vec3::from_array(particle.position_m);
        let half = particle.size_m.max(0.0) / 2.0;
        for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            positions.push((centre + (right * x + up * y) * half).to_array());
            colors.push(particle.color_linear_rgba);
            uvs.push([(x + 1.0) / 2.0, (1.0 - y) / 2.0]);
        }
        let first = index as u32 * 4;
        indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    let normals = vec![back.to_array(); positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

/// The outline of the place an emitter's particles start in, at the emitter.
fn outline(shape: &Shape, lines: &mut Gizmos<StageLines>, color: Color) {
    // A circle of the engine lies in the plane of X and Y: turned to lie flat.
    let flat = Isometry3d::new(
        EMITTER_AT,
        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
    );
    match shape {
        Shape::Point {} => lines.cross(
            Isometry3d::from_translation(EMITTER_AT),
            POINT_MARK_M,
            color,
        ),
        Shape::Sphere { radius_m } => {
            lines.sphere(Isometry3d::from_translation(EMITTER_AT), *radius_m, color);
        }
        Shape::Box { size_m } => lines.cube(
            Transform::from_translation(EMITTER_AT).with_scale(Vec3::from_array(*size_m)),
            color,
        ),
        Shape::Cone {
            radius_m,
            angle_degrees,
        } => {
            lines.circle(flat, *radius_m, color);
            // From the disk's edge, as far from straight up as the cone opens.
            let (sin, cos) = angle_degrees.to_radians().sin_cos();
            for outward in [Vec3::X, Vec3::Z, Vec3::NEG_X, Vec3::NEG_Z] {
                let from = EMITTER_AT + outward * *radius_m;
                lines.line(
                    from,
                    from + (Vec3::Y * cos + outward * sin) * CONE_LINE_M,
                    color,
                );
            }
        }
    }
}

/// Draws the stage while it is shown: the grid, the chosen emitter's shape, and the
/// preview's particles as they are in this frame.
fn draw_stage(
    mut stage: ResMut<Stage>,
    (particles, preview, gpu, looks): (
        Res<Particles>,
        Res<Preview>,
        Res<GpuPreview>,
        Res<ParticleLooks>,
    ),
    camera: Single<&Transform, With<ViewCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut quad: Single<(&mut MeshMaterial3d<StandardMaterial>, &mut Visibility), With<ParticleQuads>>,
    mut lines: Gizmos<StageLines>,
) {
    if !stage.on {
        if stage.drawn != 0 {
            stage.drawn = 0;
        }
        return;
    }
    lines
        .grid(
            Isometry3d::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            UVec2::splat(GRID_CELLS),
            Vec2::splat(1.0),
            to_color(color::BORDER),
        )
        .outer_edges();
    if let Some(emitter) = particles.chosen() {
        outline(&emitter.shape, &mut lines, to_color(color::TEXT_DIM));
    }

    // The reference simulation is drawn unless the graphics card draws the GPU preview.
    let simulation = preview
        .simulation()
        .filter(|_| !(preview.mode == PreviewMode::Gpu && gpu.running()));
    let alive: Vec<PreviewParticle> = simulation
        .map(|simulation| simulation.particles().collect())
        .unwrap_or_default();
    let (material, visibility) = &mut *quad;
    let wanted = if alive.is_empty() {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    if **visibility != wanted {
        **visibility = wanted;
    }
    let additive = simulation.is_some_and(|simulation| {
        simulation.spec().look.blend == editor_ui::particles::Blend::Additive
    });
    let look = if additive {
        &looks.additive
    } else {
        &looks.alpha
    };
    if material.0 != *look {
        material.0 = look.clone();
    }
    if alive.is_empty() && stage.drawn == 0 {
        return;
    }
    if let Some(mut mesh) = meshes.get_mut(&looks.mesh) {
        *mesh = quads(
            &alive,
            camera.right().as_vec3(),
            camera.up().as_vec3(),
            camera.back().as_vec3(),
        );
    }
    stage.drawn = alive.len();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn particle(position_m: [f32; 3], size_m: f32) -> PreviewParticle {
        PreviewParticle {
            position_m,
            size_m,
            color_linear_rgba: [1.0, 0.5, 0.25, 0.5],
        }
    }

    fn positions(mesh: &Mesh) -> Vec<Vec3> {
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|values| values.as_float3())
            .unwrap()
            .iter()
            .map(|position| Vec3::from_array(*position))
            .collect()
    }

    #[test]
    fn a_particle_is_a_square_of_its_size_about_its_place_from_the_emitter_in_its_colour() {
        let mesh = quads(&[particle([0.5, 1.0, 0.0], 0.2)], Vec3::X, Vec3::Y, Vec3::Z);
        let corners = positions(&mesh);
        assert_eq!(corners.len(), 4);
        let centre = corners.iter().sum::<Vec3>() / 4.0;
        assert!(centre.distance(EMITTER_AT + Vec3::new(0.5, 1.0, 0.0)) < 1e-6);
        for corner in &corners {
            let from = *corner - centre;
            assert!((from.x.abs() - 0.1).abs() < 1e-6 && (from.y.abs() - 0.1).abs() < 1e-6);
            assert!(from.z.abs() < 1e-6, "it faces the camera");
        }
        assert_eq!(mesh.indices().unwrap().len(), 6);
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("the mesh has no colours");
        };
        assert_eq!(colors, &vec![[1.0, 0.5, 0.25, 0.5]; 4]);
    }

    #[test]
    fn the_particles_behind_are_drawn_before_those_in_front() {
        // The camera looks along -Z: what has the smaller Z is farther from it.
        let mesh = quads(
            &[
                particle([0.0, 0.0, 2.0], 0.1),
                particle([0.0, 0.0, -3.0], 0.1),
                particle([0.0, 0.0, 0.5], 0.1),
            ],
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
        );
        let depths: Vec<f32> = positions(&mesh)
            .chunks(4)
            .map(|corners| corners[0].z)
            .collect();
        assert_eq!(depths, [-3.0, 0.5, 2.0]);
    }

    #[test]
    fn a_device_with_too_few_bind_groups_is_said_to_lack_them_and_any_other_lacks_nothing() {
        assert_eq!(device_lacks(4), None);
        assert_eq!(device_lacks(8), None);
        let lacks = device_lacks(2).unwrap();
        assert!(
            lacks.contains("2 bind groups") && lacks.contains("needs 4"),
            "{lacks}"
        );
    }

    #[test]
    fn the_adapter_builds_a_point_emitter_and_what_it_refuses_is_said_as_that() {
        let emitter = editor_ui::particles::default_emitter();
        assert!(build_effect(&emitter, Preview::SEED as u32).is_ok());

        // A shape the adapter does not draw: its field and reason, not a failure to start.
        let unsupported = AdapterError::UnsupportedField {
            field: "shape",
            reason: "point emitters only",
        };
        assert_eq!(
            refused(unsupported),
            GpuPreview::Unsupported {
                field: "shape".into(),
                reason: "point emitters only".into()
            }
        );
        // An emitter the format itself refuses could not be chosen; if it comes, it is said.
        let invalid = AdapterError::InvalidSpec {
            field: Some("lifetime.max_seconds".into()),
            detail: "must be at least the minimum".into(),
        };
        let GpuPreview::Failed(why) = refused(invalid) else {
            panic!("an invalid emitter is a preview that did not start");
        };
        assert!(
            why.contains("lifetime.max_seconds") && why.contains("at least"),
            "{why}"
        );
    }

    #[test]
    fn no_particles_make_one_square_of_no_size_and_never_a_mesh_without_vertices() {
        let empty = quads(&[], Vec3::X, Vec3::Y, Vec3::Z);
        // Four corners in one place and two triangles of no area: nothing to see, and
        // something for the engine to keep a place for.
        let corners = positions(&empty);
        assert_eq!(corners.len(), 4);
        assert!(corners.iter().all(|corner| *corner == EMITTER_AT));
        assert_eq!(empty.indices().unwrap().len(), 6);
        assert!(empty.get_vertex_buffer_size() > 0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            empty.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("the mesh has no colours");
        };
        assert!(
            colors.iter().all(|color| color[3] == 0.0),
            "and it has no colour"
        );
        // Every attribute a mesh with particles has, so that its layout does not change.
        let drawn = quads(&[particle([0.0; 3], 0.1)], Vec3::X, Vec3::Y, Vec3::Z);
        assert_eq!(
            empty
                .attributes()
                .map(|(attribute, _)| attribute.id)
                .collect::<Vec<_>>(),
            drawn
                .attributes()
                .map(|(attribute, _)| attribute.id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_size_below_nothing_is_nothing() {
        let shrunk = quads(&[particle([0.0; 3], -1.0)], Vec3::X, Vec3::Y, Vec3::Z);
        assert!(positions(&shrunk)
            .iter()
            .all(|corner| corner.distance(EMITTER_AT) < 1e-6));
    }
}
