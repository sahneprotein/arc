//! The Particles tool's stage: what the 3D view shows while that tool is chosen.
//!
//! The level is not shown then. In its place are a ground grid, the outline of the chosen
//! emitter's shape a metre above the grid's middle, and the particles of the preview, each a
//! square that faces the camera. The particles are those of the editor's reference
//! simulation (`editor_ui::particles::Preview`): this draws what that says and works out
//! nothing of its own. It is not the game's particle runtime, and the panel says so.
//!
//! The stage is on a render layer of its own, and the view's camera is moved to that layer
//! while the tool is chosen: the level's meshes stay as they are and are neither drawn nor
//! clicked on. The camera turns about the emitter; where it was in the level is remembered
//! and put back when another tool is chosen, and so is the way the level was drawn.

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::{NoFrustumCulling, RenderLayers},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use editor_ui::{
    particles::{PreviewParticle, Shape},
    theme::{color, feathers::to_color},
    Particles, Preview, Tool, ViewMode,
};

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
        app.init_gizmo_group::<StageLines>()
            .init_resource::<Stage>()
            .add_systems(Startup, setup)
            .add_systems(Update, (switch_stage, draw_stage).chain());
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
fn quads(particles: &[PreviewParticle], right: Vec3, up: Vec3, back: Vec3) -> Mesh {
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
    (particles, preview, looks): (Res<Particles>, Res<Preview>, Res<ParticleLooks>),
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

    let simulation = preview.simulation();
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
    fn no_particles_make_an_empty_mesh_and_a_size_below_nothing_is_nothing() {
        let empty = quads(&[], Vec3::X, Vec3::Y, Vec3::Z);
        assert_eq!(positions(&empty).len(), 0);
        let shrunk = quads(&[particle([0.0; 3], -1.0)], Vec3::X, Vec3::Y, Vec3::Z);
        assert!(positions(&shrunk)
            .iter()
            .all(|corner| corner.distance(EMITTER_AT) < 1e-6));
    }
}
