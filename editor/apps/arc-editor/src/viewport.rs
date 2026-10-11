//! The 3D view.
//!
//! The level is drawn from the core's snapshot: each box and spawn point of the document
//! becomes a mesh here, found again by its stable ID. The view never changes the document.
//! A click asks the core to select, and the highlight follows the core's answer; with Shift
//! held the clicked object joins the selection or leaves it. A drag on the gizmo is one
//! transaction of the core: the view sends the values it has reached as a preview, and the
//! release commits them as one undo step.
//!
//! The gizmo holds the first selected object that is drawn, in the core's order of the
//! selection, which is by the objects' IDs and not by which was clicked last. With several
//! selected, the others go with it as one rigid group: the same move, and the same turn about the held object.
//! Snapping puts the held object on the grid and moves the others by exactly as much, so that
//! what was built keeps its shape.
//!
//! The view is drawn straight to the window, in the rectangle the panels leave free. The
//! engine's transform gizmo reads the window's cursor, so it needs the view's camera to draw
//! to the window and not into an image.
//!
//! Camera: drag with the right button to turn, with the middle button to move, wheel to zoom.
//!
//! A drag on the gizmo is cancelled by Escape, by the window losing focus, and by the cursor
//! leaving the window. The last one is not a matter of taste: the engine's gizmo does not see
//! the button released while the cursor is outside, and would hold the object for ever. The
//! editor ends the drag itself, in the gizmo and in the core, and does not rely on the desktop
//! keeping the cursor inside.

use bevy::{
    camera::{visibility::RenderLayers, NormalizedRenderTarget, Viewport},
    ecs::system::SystemParam,
    gizmos::transform_gizmo::{
        TransformGizmoCamera, TransformGizmoFocus, TransformGizmoPlugin, TransformGizmoSettings,
        TransformGizmoState, TransformGizmoSystems,
    },
    picking::pointer::PointerButton,
    platform::collections::HashMap,
    prelude::*,
    render::{
        camera::ExtractedCamera,
        render_resource::{CachedPipelineState, PipelineCache, PipelineDescriptor},
        Render, RenderApp, RenderSystems,
    },
    ui::{ComputedNode, IsDefaultUiCamera, UiGlobalTransform},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow, RequestRedraw},
};
use editor_core::{GizmoPivot, Mutation, ObjectId, SceneDocument, ViewSnapshot};
use editor_ui::{
    look::procedural_brick_v1,
    panels::{
        hierarchy::adds_to_selection,
        inspector::{gizmo_object, gizmo_roots},
    },
    theme::{color, feathers::to_color},
    Editor, Modal, PlaceAt, Snap, Tool, ViewMode, ViewportSlot,
};

use crate::look::{
    box_material, box_mesh, look_image, EditorSun, GameLook, EDITOR_AMBIENT_BRIGHTNESS,
    EDITOR_SUN_ILLUMINANCE,
};
use landscape_render::{GrassRenderer, GroundRenderer, GroundTile};
use scene_data::{ObjectData, ObjectRecord, RigidTransform};

pub struct ViewportPlugin;

impl Plugin for ViewportPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TransformGizmoPlugin)
            .init_resource::<Orbit>()
            .init_resource::<Shown>()
            .init_resource::<GroundShown>()
            .init_resource::<GroundRenderer>()
            .init_resource::<GrassRenderer>()
            .init_resource::<CamerasStarted>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    start_cameras,
                    listen,
                    fit_camera.after(start_cameras),
                    place_camera,
                    show_level,
                    show_ground,
                    snap_turns,
                ),
            )
            .add_systems(PostUpdate, drag_with_gizmo.after(TransformGizmoSystems))
            .add_observer(choose_landscape);
        // A scripted check also gets the facts of the window's first frames from where they
        // are drawn.
        if crate::check::is_scripted()
            && let Some(render) = app.get_sub_app_mut(RenderApp)
        {
            render.add_systems(Render, report_first_frames.in_set(RenderSystems::Cleanup));
        }
    }
}

/// How many of the window's first frames a scripted check reports.
const FRAMES_REPORTED: u32 = 12;

/// Says of each of the window's first frames what could write the window in it, for a
/// scripted check to judge. One line a frame:
///
/// `ARC_EDITOR_FIRST_FRAME frame=4 cameras=4 orders=[-1, 0, 1, 2] blit_ready=3 blit_waiting=0 written=cameras clear_colour=[31, 31, 36]`
///
/// A frame's picture of the window is written in one of two ways. With no camera aimed at the
/// window the engine clears it itself with the application's clear colour (`written=clear`).
/// With cameras, each copies its picture to the window with the engine's "blit pipeline", and
/// only once that exists (`written=cameras`). Cameras without it leave the window as the
/// graphics driver handed it over (`written=nothing`): that is the frame that must not occur.
/// A picture taken of the window cannot tell these apart: it is taken of what the cameras
/// drew, and so shows black for a frame the engine cleared.
fn report_first_frames(
    mut frame: Local<u32>,
    cameras: Query<&ExtractedCamera>,
    pipelines: Res<PipelineCache>,
    clear: Res<ClearColor>,
) {
    if *frame >= FRAMES_REPORTED {
        return;
    }
    *frame += 1;
    // The cameras aimed at the window in this frame, by their place in the drawing order:
    // -1 clears, 0 is the level, 1 the engine's gizmo overlay, 2 the UI.
    let mut orders: Vec<isize> = cameras
        .iter()
        .filter(|camera| matches!(camera.target, Some(NormalizedRenderTarget::Window(_))))
        .map(|camera| camera.order)
        .collect();
    orders.sort_unstable();
    let cameras = orders.len();
    let (mut ready, mut waiting) = (0, 0);
    for pipeline in pipelines.pipelines() {
        let PipelineDescriptor::RenderPipelineDescriptor(descriptor) = &pipeline.descriptor else {
            continue;
        };
        if descriptor.label.as_deref() != Some("blit pipeline") {
            continue;
        }
        if matches!(pipeline.state, CachedPipelineState::Ok(_)) {
            ready += 1;
        } else {
            waiting += 1;
        }
    }
    let written = match (cameras, ready, waiting) {
        (0, ..) => "clear",
        (_, 1.., 0) => "cameras",
        _ => "nothing",
    };
    println!(
        "ARC_EDITOR_FIRST_FRAME frame={} cameras={cameras} orders={orders:?} blit_ready={ready} \
         blit_waiting={waiting} written={written} clear_colour={:?}",
        *frame,
        // What the engine clears with when no camera draws, as 8-bit sRGB.
        clear.0.to_srgba().to_u8_array_no_alpha()
    );
}

/// How far boxes are pushed back in depth so that a landscape in the same plane is drawn in
/// front of them. In the renderer's smallest depth steps; negative is away from the camera.
/// Large enough to cover the rounding of depth across the map's 120 m slab, and still far
/// below the depth of a few centimetres at any distance the camera reaches.
const BOX_DEPTH_BIAS: f32 = -2048.0;

/// The camera that draws the level.
#[derive(Component)]
pub struct ViewCamera;

/// The meshes and materials every object shares.
#[derive(Resource)]
struct Looks {
    spawn_mesh: Handle<Mesh>,
    solid: Handle<StandardMaterial>,
    spawn: Handle<StandardMaterial>,
    selected: Handle<StandardMaterial>,
    /// A box as the game draws it, if the editor holds the game's look profile.
    game_box: Option<Handle<StandardMaterial>>,
    /// How many metres one repeat of a box's texture covers: the profile's, or a metre.
    box_repeat_m: f32,
}

/// Where the camera looks from: around `target`, `distance` away.
#[derive(Resource)]
pub struct Orbit {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for Orbit {
    fn default() -> Self {
        Self {
            target: Vec3::new(0.0, 1.0, 2.0),
            yaw: 0.6,
            pitch: 0.55,
            distance: 38.0,
        }
    }
}

impl Orbit {
    fn transform(&self) -> Transform {
        let rotation = Quat::from_euler(EulerRot::YXZ, self.yaw, -self.pitch, 0.0);
        Transform::from_translation(self.target + rotation * Vec3::new(0.0, 0.0, self.distance))
            .looking_at(self.target, Vec3::Y)
    }
}

/// A mesh in the view and the stable ID of the object it shows.
#[derive(Component)]
pub struct Projected(pub ObjectId);

/// What the gizmo holds when it is at the middle of several selected objects: a place in the
/// view with nothing drawn at it. It is no object of the level.
#[derive(Component)]
pub struct MiddleHandle;

/// Identifies what the view was built from.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ShownKey {
    session: u64,
    document_revision: u64,
    /// The transaction and sequence of a drag in progress.
    preview: Option<(u64, u64)>,
    selection_revision: u64,
    /// Whether the gizmo may hold the selection: not while the landscape tool is chosen, and
    /// not behind a dialog.
    gizmo_allowed: bool,
    /// Whether the level is drawn as the game draws it.
    game_look: bool,
}

/// One object as the view shows it.
struct ShownObject {
    entity: Entity,
    /// The half extents its mesh was built for, if it is a box.
    half_extents: Option<[f32; 3]>,
}

/// What the view shows now.
#[derive(Resource, Default)]
pub struct Shown {
    key: Option<ShownKey>,
    objects: HashMap<String, ShownObject>,
    /// The selection the view was last drawn with.
    selection: Vec<ObjectId>,
    /// Whether the view has shown the level at least once.
    pub ready: bool,
    /// How many objects have been drawn or redrawn in all. For checks: a change of one object
    /// must not redraw the level.
    pub redrawn: u64,
}

/// How much landscape the view shows.
#[derive(Resource, Default)]
pub struct GroundShown {
    /// The ground pieces of the geometry last drawn.
    pub tiles: usize,
    /// The blades of grass the rules made of it.
    pub grass: usize,
    /// The average weight of each layer a rule fills, in the order of the layers' IDs: a
    /// plain mean of what the core's geometry holds, for the checks to read.
    pub filled: Vec<f32>,
}

/// Whether the window's first frames are over and the cameras draw.
#[derive(Resource, Default)]
struct CamerasStarted(bool);

/// How many frames the engine alone writes the window before the cameras take over.
///
/// In its first two frames a camera cannot yet write the window: the step that copies its
/// picture there is not ready. But a window that a camera is aimed at counts as taken care of,
/// so the engine leaves it alone, and those frames would go to the desktop holding whatever
/// the graphics driver had in them: on the machine this was found on, pink, for a quarter of a
/// second. While no camera is switched on, the engine itself clears the window with the
/// application's clear colour, which is the editor's window colour (`main.rs`). So the first
/// thing on screen is the editor's own background, and the cameras come in once they can draw.
/// Two frames were measured as needed; a third is margin.
///
/// Every camera is held back, not only the editor's three: the engine's transform gizmo
/// brings an overlay camera of its own, and one camera aimed at the window is enough for the
/// engine to leave the window alone.
pub const FRAMES_BEFORE_CAMERAS: u32 = 3;

/// How many frames after the cameras came in this system still asks for the next one, so
/// that the first complete picture is followed up even when nothing else asks.
const FRAMES_AFTER_CAMERAS: u32 = 4;

/// Keeps every camera switched off for [`FRAMES_BEFORE_CAMERAS`] frames and then switches
/// them all on, and keeps frames coming until then and a little beyond: the editor otherwise
/// draws only when something happens.
fn start_cameras(
    mut frame: Local<u32>,
    mut started: ResMut<CamerasStarted>,
    mut cameras: Query<&mut Camera>,
    mut redraw: MessageWriter<RequestRedraw>,
) {
    if *frame > FRAMES_BEFORE_CAMERAS + FRAMES_AFTER_CAMERAS {
        return;
    }
    *frame += 1;
    redraw.write(RequestRedraw);
    if *frame <= FRAMES_BEFORE_CAMERAS {
        // Each frame anew: a camera may appear, or be switched on, after the first.
        for mut camera in &mut cameras {
            if camera.is_active {
                camera.is_active = false;
            }
        }
    } else if !started.0 {
        started.0 = true;
        for mut camera in &mut cameras {
            camera.is_active = true;
        }
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    (orbit, look): (Res<Orbit>, Res<GameLook>),
) {
    let window_color = to_color(color::SURFACE_WINDOW);
    // Three cameras of ours, drawn in this order: one that only clears the window, the level
    // in its rectangle, and the UI on top of everything. They begin switched off: see
    // `start_cameras`.
    commands.spawn((
        Camera2d,
        Camera {
            order: -1,
            is_active: false,
            clear_color: ClearColorConfig::Custom(window_color),
            ..default()
        },
        RenderLayers::none(),
    ));
    commands.spawn((
        ViewCamera,
        TransformGizmoCamera,
        Camera3d::default(),
        Camera {
            order: 0,
            is_active: false,
            clear_color: ClearColorConfig::Custom(to_color(color::SURFACE_SUNKEN)),
            ..default()
        },
        orbit.transform(),
    ));
    // The engine's gizmo draws with a camera of its own at order 1.
    commands.spawn((
        Camera2d,
        IsDefaultUiCamera,
        Camera {
            order: 2,
            is_active: false,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        RenderLayers::none(),
    ));

    commands.spawn((MiddleHandle, Transform::IDENTITY));

    commands.spawn((
        EditorSun,
        DirectionalLight {
            illuminance: EDITOR_SUN_ILLUMINANCE,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.5, 0.0)),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: EDITOR_AMBIENT_BRIGHTNESS,
        ..default()
    });

    let matte = |color: Color| StandardMaterial {
        base_color: color,
        perceptual_roughness: 1.0,
        ..default()
    };
    // Where a box face and the landscape lie in the same plane, as the current map's ground
    // slab and a new flat landscape do, the landscape is drawn. Without this the two flicker
    // through each other. It changes only this picture: the level and its collision keep both.
    let box_look = |color: Color| StandardMaterial {
        depth_bias: BOX_DEPTH_BIAS,
        ..matte(color)
    };
    commands.insert_resource(Looks {
        spawn_mesh: meshes.add(Sphere::new(0.3)),
        solid: materials.add(box_look(to_color(color::TEXT_DIM))),
        spawn: materials.add(matte(to_color(color::SIGNAL_PLAY))),
        selected: materials.add(box_look(to_color(color::ACCENT))),
        // The game's box, made only from a profile that was read: its colour, its roughness
        // and the texture it names.
        game_box: look.0.as_ref().map(|loaded| {
            let texture = images.add(look_image(&procedural_brick_v1()));
            materials.add(box_material(&loaded.profile.boxes, texture))
        }),
        box_repeat_m: look
            .0
            .as_ref()
            .map_or(1.0, |loaded| loaded.profile.boxes.texture_repeat_m),
    });
}

/// The node the view shows through takes the camera input, once the UI exists.
fn listen(mut commands: Commands, slot: Query<Entity, Added<ViewportSlot>>) {
    for entity in &slot {
        commands.entity(entity).observe(turn_or_move).observe(zoom);
    }
}

/// Keeps the camera's rectangle on the node the view shows through.
fn fit_camera(
    started: Res<CamerasStarted>,
    slot: Query<(&ComputedNode, &UiGlobalTransform), With<ViewportSlot>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut camera: Single<&mut Camera, With<ViewCamera>>,
) {
    // In the window's first frames no camera draws: see `FRAMES_BEFORE_CAMERAS`.
    if !started.0 {
        return;
    }
    let Ok((node, transform)) = slot.single() else {
        return;
    };
    // Both are in physical pixels; the transform's translation is the node's centre.
    let size = node.size;
    let min = transform.translation - size / 2.0;
    let window_size = Vec2::new(
        window.physical_width() as f32,
        window.physical_height() as f32,
    );
    let min = min.clamp(Vec2::ZERO, window_size);
    let max = (min + size).clamp(Vec2::ZERO, window_size);
    let viewport = ((max - min).min_element() >= 1.0).then(|| Viewport {
        physical_position: min.as_uvec2(),
        physical_size: (max - min).as_uvec2(),
        ..default()
    });
    let shown = camera
        .viewport
        .as_ref()
        .map(|v| (v.physical_position, v.physical_size));
    let wanted = viewport
        .as_ref()
        .map(|v| (v.physical_position, v.physical_size));
    if shown != wanted {
        // Without room for the view, the camera draws nothing.
        camera.is_active = viewport.is_some();
        if viewport.is_some() {
            camera.viewport = viewport;
        }
    }
}

fn turn_or_move(drag: On<Pointer<Drag>>, mut orbit: ResMut<Orbit>) {
    match drag.button {
        PointerButton::Secondary => {
            orbit.yaw -= drag.delta.x * 0.006;
            orbit.pitch = (orbit.pitch + drag.delta.y * 0.006).clamp(-1.5, 1.5);
        }
        PointerButton::Middle => {
            let view = orbit.transform();
            let step = orbit.distance * 0.0015;
            let offset = view.right() * -drag.delta.x * step + view.up() * drag.delta.y * step;
            orbit.target += offset;
        }
        PointerButton::Primary => {}
    }
}

fn zoom(scroll: On<Pointer<Scroll>>, mut orbit: ResMut<Orbit>) {
    orbit.distance = (orbit.distance * (1.0 - scroll.y * 0.1)).clamp(1.0, 500.0);
}

fn place_camera(
    orbit: Res<Orbit>,
    mut camera: Single<&mut Transform, With<ViewCamera>>,
    mut place: ResMut<PlaceAt>,
    mut redraw: MessageWriter<RequestRedraw>,
) {
    if orbit.is_changed() {
        **camera = orbit.transform();
        // New objects go where the view looks: along this line to the landscape, or to the
        // point the view turns about, on the level's zero height.
        *place = PlaceAt {
            origin: camera.translation,
            direction: camera.forward().as_vec3(),
            fallback: Vec3::new(orbit.target.x, 0.0, orbit.target.z),
        };
        redraw.write(RequestRedraw);
    }
}

/// The transform of an object in the level, through its parents.
pub(crate) fn world_transform(document: &SceneDocument, object: &ObjectRecord) -> Transform {
    let local = Transform {
        translation: Vec3::from_array(object.local.translation_m),
        rotation: Quat::from_array(object.local.rotation_xyzw),
        scale: Vec3::ONE,
    };
    match parent_of(document, object) {
        Some(parent) => world_transform(document, parent) * local,
        None => local,
    }
}

fn parent_of<'a>(document: &'a SceneDocument, object: &ObjectRecord) -> Option<&'a ObjectRecord> {
    object
        .parent
        .as_ref()
        .and_then(|parent| document.object(parent))
}

/// Which objects a change of the snapshot can have touched.
enum Scope {
    Everything,
    Objects(Vec<ObjectId>),
}

/// Works out what must be redrawn between two states of the level. `None` means nothing.
///
/// The core says which objects a revision changed. A drag changes the selected objects, and
/// a change of selection changes how the old and the new selection look. Anything this cannot
/// account for, such as another level or several revisions at once, redraws everything.
fn scope(
    before: Option<ShownKey>,
    now: ShownKey,
    selection_before: &[ObjectId],
    snapshot: &editor_core::ViewSnapshot,
) -> Option<Scope> {
    let Some(before) = before else {
        return Some(Scope::Everything);
    };
    if before == now {
        return None;
    }
    // Another level, or another way of drawing it: everything is drawn anew.
    if before.session != now.session || before.game_look != now.game_look {
        return Some(Scope::Everything);
    }
    let mut objects: Vec<ObjectId> = Vec::new();
    if now.document_revision == before.document_revision + 1 {
        objects.extend(snapshot.changed_objects.iter().cloned());
    } else if now.document_revision != before.document_revision {
        return Some(Scope::Everything);
    }
    if now.preview != before.preview
        || now.selection_revision != before.selection_revision
        || now.gizmo_allowed != before.gizmo_allowed
    {
        objects.extend(selection_before.iter().cloned());
        objects.extend(snapshot.selection.iter().cloned());
    }
    objects.sort();
    objects.dedup();
    Some(Scope::Objects(objects))
}

/// Draws the level as the core's snapshot has it, a drag in progress included.
///
/// Only what changed is touched. Objects keep their entity while they exist, so that the gizmo
/// keeps its hold during a drag. The object under the gizmo is left alone while it is dragged:
/// the gizmo moves it, and what the core answers is the same values one frame later.
fn show_level(
    mut commands: Commands,
    editor: NonSend<Editor>,
    looks: Res<Looks>,
    gizmo: Res<TransformGizmoState>,
    (tool, modal, view): (Res<Tool>, Res<Modal>, Res<ViewMode>),
    mut meshes: ResMut<Assets<Mesh>>,
    (mut shown, middle): (ResMut<Shown>, Single<Entity, With<MiddleHandle>>),
) {
    let snapshot = editor.snapshot();
    let preview = snapshot.preview.as_ref();
    let key = ShownKey {
        session: snapshot.session,
        document_revision: snapshot.document_revision,
        preview: preview.map(|preview| (preview.transaction, preview.sequence)),
        selection_revision: snapshot.regions.selection,
        gizmo_allowed: *tool == Tool::Objects && !modal.0,
        // Game look is drawn only with the game's box material, which needs the profile.
        game_look: *view == ViewMode::GameLook && looks.game_box.is_some(),
    };
    let Some(scope) = scope(shown.key, key, &shown.selection, &snapshot) else {
        return;
    };
    shown.key = Some(key);
    shown.selection = snapshot.selection.to_vec();
    shown.ready = true;
    let document = preview.map_or(&snapshot.document, |preview| &preview.document);
    let dragged = gizmo.active.then_some(gizmo.entity).flatten();
    // What the gizmo holds, unless the landscape tool is chosen or a dialog is open.
    let target = gizmo_target(&snapshot).filter(|_| key.gizmo_allowed);
    // At the middle of several objects it holds the handle, which is put there unless it is
    // being dragged: the gizmo moves it then.
    match target {
        Some(GizmoTarget::Middle(at)) => {
            let mut handle = commands.entity(*middle);
            handle.insert(TransformGizmoFocus);
            if dragged != Some(*middle) {
                handle.insert(Transform::from_translation(at));
            }
        }
        _ => {
            commands.entity(*middle).remove::<TransformGizmoFocus>();
        }
    }

    let ids: Vec<ObjectId> = match scope {
        Scope::Objects(ids) => ids,
        Scope::Everything => {
            // Whatever is shown and no longer in the level goes; then every object is drawn.
            let gone: Vec<String> = shown
                .objects
                .keys()
                .filter(|id| !document.objects.iter().any(|object| object.id.0 == **id))
                .cloned()
                .collect();
            for id in gone {
                if let Some(object) = shown.objects.remove(&id) {
                    commands.entity(object.entity).despawn();
                }
            }
            document
                .objects
                .iter()
                .map(|object| object.id.clone())
                .collect()
        }
    };
    shown.redrawn += ids.len() as u64;

    for id in &ids {
        let Some(object) = document.object(id) else {
            // The object was removed.
            if let Some(object) = shown.objects.remove(&id.0) {
                commands.entity(object.entity).despawn();
            }
            continue;
        };
        let half_extents = match &object.data {
            ObjectData::Box { half_extents_m } => Some(*half_extents_m),
            ObjectData::Spawn { .. } => None,
            // Groups have nothing to draw; the landscape is drawn by the ground renderer.
            ObjectData::Group | ObjectData::Terrain(_) => continue,
        };
        let mesh = |meshes: &mut Assets<Mesh>| match half_extents {
            Some(half) => meshes.add(box_mesh(Vec3::from_array(half) * 2.0, looks.box_repeat_m)),
            None => looks.spawn_mesh.clone(),
        };
        // In Game look a box has the game's material whether it is selected or not: the
        // selection is an outline drawn over the picture (`look::outline_selection`). A
        // spawn point is the editor's own mark in both pictures; the game draws none.
        let game_box = looks
            .game_box
            .as_ref()
            .filter(|_| key.game_look && half_extents.is_some());
        let material = if let Some(game_box) = game_box {
            game_box.clone()
        } else if snapshot.selection.contains(&object.id) {
            looks.selected.clone()
        } else if half_extents.is_none() {
            looks.spawn.clone()
        } else {
            looks.solid.clone()
        };
        let transform = world_transform(document, object);
        let entity = match shown.objects.get_mut(&object.id.0) {
            Some(existing) => {
                let mut entity = commands.entity(existing.entity);
                entity.insert(MeshMaterial3d(material));
                if existing.half_extents != half_extents {
                    existing.half_extents = half_extents;
                    entity.insert(Mesh3d(mesh(&mut meshes)));
                }
                if dragged != Some(existing.entity) {
                    entity.insert(transform);
                }
                existing.entity
            }
            None => {
                let entity = commands
                    .spawn((
                        Mesh3d(mesh(&mut meshes)),
                        MeshMaterial3d(material),
                        transform,
                        Projected(object.id.clone()),
                    ))
                    .observe(choose)
                    .id();
                shown.objects.insert(
                    object.id.0.clone(),
                    ShownObject {
                        entity,
                        half_extents,
                    },
                );
                entity
            }
        };
        if target == Some(GizmoTarget::Object(&object.id)) {
            commands.entity(entity).insert(TransformGizmoFocus);
        } else {
            commands.entity(entity).remove::<TransformGizmoFocus>();
        }
    }
}

/// Draws the landscape from the geometry the core cooks: the same triangles the game's
/// collision is made of. The geometry is asked for when the level, or a brush stroke in
/// progress, changed and there is a landscape to draw or to remove.
fn show_ground(
    mut commands: Commands,
    mut editor: NonSendMut<Editor>,
    (mut renderer, mut grass): (ResMut<GroundRenderer>, ResMut<GrassRenderer>),
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ground: ResMut<GroundShown>,
    mut redraw: MessageWriter<RequestRedraw>,
) {
    let snapshot = editor.snapshot();
    let document = snapshot
        .preview
        .as_ref()
        .map_or(&snapshot.document, |preview| &preview.document);
    let has_landscape = document
        .objects
        .iter()
        .any(|object| matches!(object.data, ObjectData::Terrain(_)));
    // A drag that moves boxes does not change the ground.
    let stroke = snapshot
        .preview
        .as_ref()
        .is_some_and(|preview| !preview.dirty.is_empty());
    if (has_landscape || ground.tiles > 0) && (snapshot.preview.is_none() || stroke) {
        editor.want_geometry();
    }
    let Some(geometry) = editor.take_geometry() else {
        return;
    };
    let dirty = snapshot
        .preview
        .as_ref()
        .map_or(&[][..], |preview| preview.dirty.as_ref());
    match renderer.sync_with_visual(
        &mut commands,
        &mut meshes,
        &mut materials,
        &geometry.simulation.ground,
        Some(&geometry.visual),
        dirty,
    ) {
        Ok(_) => {
            ground.tiles = geometry.simulation.ground.len();
            redraw.write(RequestRedraw);
        }
        Err(problem) => warn!("the landscape could not be drawn: {problem}"),
    }
    // What the rules scatter on that ground, from the same geometry.
    match grass.sync(
        &mut commands,
        &mut meshes,
        &mut materials,
        &geometry.visual,
        &geometry.simulation.ground,
    ) {
        Ok(facts) => {
            ground.grass = facts.update.cpu_allocated_instances;
            let mut sums: std::collections::BTreeMap<&str, (f64, usize)> = default();
            for filled in geometry
                .visual
                .terrain
                .iter()
                .flat_map(|terrain| &terrain.derived_layers)
            {
                let entry = sums.entry(filled.layer.0.as_str()).or_default();
                entry.0 += filled
                    .weights
                    .iter()
                    .map(|weight| f64::from(*weight))
                    .sum::<f64>();
                entry.1 += filled.weights.len();
            }
            ground.filled = sums
                .values()
                .map(|(sum, count)| (sum / (*count).max(1) as f64) as f32)
                .collect();
        }
        Err(problem) => warn!("the grass could not be drawn: {problem}"),
    }
}

/// A click on the ground chooses the landscape it belongs to. Only with the object tools: with
/// the landscape tool a click on the ground is a stroke.
fn choose_landscape(
    click: On<Pointer<Click>>,
    tiles: Query<&GroundTile>,
    tool: Res<Tool>,
    gizmo: Res<TransformGizmoState>,
    keys: Res<ButtonInput<KeyCode>>,
    mut editor: NonSendMut<Editor>,
) {
    if click.button != PointerButton::Primary
        || *tool != Tool::Objects
        || gizmo.active
        || gizmo.hovered_axis.is_some()
    {
        return;
    }
    if let Ok(tile) = tiles.get(click.entity) {
        editor.pick(tile.object.clone(), adds_to_selection(&keys));
    }
}

/// How much farther than a clicked surface a landscape may lie and still be what was clicked,
/// in metres. Where a landscape and a box share a plane the view draws the landscape in front,
/// and the click must choose what is seen; which of the two the pointer reports is chance.
const SAME_SURFACE: f32 = 0.02;

/// Whether a click that hit an object at `object_depth` along the pointer's ray belongs to the
/// landscape, which the same ray meets at `ground_distance`. It does when the landscape is in
/// front or in the same surface; a landscape clearly behind the object is hidden by it, and
/// the object is what was clicked.
fn click_is_on_landscape(ground_distance: f32, object_depth: f32) -> bool {
    ground_distance <= object_depth + SAME_SURFACE
}

fn choose(
    click: On<Pointer<Click>>,
    projected: Query<&Projected>,
    tiles: Query<&GroundTile>,
    camera: Single<(&Camera, &GlobalTransform), With<ViewCamera>>,
    mut ray_cast: MeshRayCast,
    (gizmo, tool, keys): (
        Res<TransformGizmoState>,
        Res<Tool>,
        Res<ButtonInput<KeyCode>>,
    ),
    mut editor: NonSendMut<Editor>,
) {
    // A click that ends a drag on the gizmo, or lands on one of its handles, is not a choice;
    // nor is a click while the pointer has a landscape brush.
    if click.button != PointerButton::Primary
        || gizmo.active
        || gizmo.hovered_axis.is_some()
        || tool.uses_brush()
    {
        return;
    }
    let Ok(object) = projected.get(click.entity) else {
        return;
    };
    let landscape_there = camera
        .0
        .viewport_to_world(camera.1, click.pointer_location.position)
        .ok()
        .and_then(|ray| {
            let only_ground = |entity| tiles.contains(entity);
            let settings = MeshRayCastSettings::default().with_filter(&only_ground);
            let (entity, hit) = ray_cast.cast_ray(ray, &settings).first()?;
            click_is_on_landscape(hit.distance, click.hit.depth)
                .then(|| tiles.get(*entity).ok())
                .flatten()
                .map(|tile| tile.object.clone())
        });
    editor.pick(
        landscape_there.unwrap_or_else(|| object.0.clone()),
        adds_to_selection(&keys),
    );
}

/// What the gizmo holds.
#[derive(Clone, Copy, PartialEq, Debug)]
enum GizmoTarget<'a> {
    /// One object of the level: the gizmo moves it, and the other selected objects with it.
    Object(&'a ObjectId),
    /// The middle of several selected objects, at this place in the level.
    Middle(Vec3),
}

/// What the gizmo holds for this selection.
///
/// It is on the object the core calls active, or on the first selected one that is drawn if
/// that one is not: a landscape and a group have nothing for the gizmo to hold on to. With
/// the middle chosen it is at the middle of the selected boxes and spawn points that are not
/// under another selected object, which is the mean of where they are in the level; its axes
/// are the level's. One such object has no middle but itself.
fn gizmo_target(snapshot: &ViewSnapshot) -> Option<GizmoTarget<'_>> {
    if snapshot.pivot == GizmoPivot::Center {
        let roots = gizmo_roots(snapshot);
        if roots.len() > 1 {
            let sum: Vec3 = roots
                .iter()
                .map(|object| world_transform(&snapshot.document, object).translation)
                .sum();
            return Some(GizmoTarget::Middle(sum / roots.len() as f32));
        }
    }
    gizmo_object(snapshot).map(|object| GizmoTarget::Object(&object.id))
}

/// Whether an object above `object` in the hierarchy, however far up, is selected too. Such an
/// object goes where that one goes and is not moved a second time.
fn under_a_selected(
    document: &SceneDocument,
    object: &ObjectRecord,
    selection: &[ObjectId],
) -> bool {
    let mut above = parent_of(document, object);
    while let Some(parent) = above {
        if selection.contains(&parent.id) {
            return true;
        }
        above = parent_of(document, parent);
    }
    false
}

/// The transform an object has under its parent when it is at `world` in the level.
fn local_of(document: &SceneDocument, object: &ObjectRecord, world: &Transform) -> RigidTransform {
    let local = match parent_of(document, object) {
        Some(parent) => {
            let parent = world_transform(document, parent);
            let inverse = parent.rotation.inverse();
            Transform {
                translation: inverse * (world.translation - parent.translation),
                rotation: inverse * world.rotation,
                scale: Vec3::ONE,
            }
        }
        None => *world,
    };
    RigidTransform {
        translation_m: local.translation.to_array(),
        rotation_xyzw: local.rotation.normalize().to_array(),
    }
}

/// Where the gizmo's object is put when the gizmo has dragged it from `start` to `dragged`:
/// there, or with snapping on the grid. `half_extents` is the size of a box, which a scale
/// changes; the scale that comes back is the one that shows the size the box will have.
///
/// A turn is not changed here: the gizmo itself turns in steps (`snap_turns`).
fn snapped(
    snap: &Snap,
    start: &Transform,
    dragged: &Transform,
    half_extents: Option<[f32; 3]>,
) -> Transform {
    let mut to = *dragged;
    to.translation = snap.position(start.translation, dragged.translation);
    if let Some(half) = half_extents
        && snap.on
        && dragged.scale != Vec3::ONE
    {
        let sized = snap.half_extents(half, dragged.scale);
        to.scale = Vec3::from_array(sized) / Vec3::from_array(half);
    }
    to
}

/// What a drag on the gizmo means for the document, when the gizmo is on the object `held`
/// and has put it at `world`. See [`group_mutations`].
#[cfg(test)]
fn drag_mutations(
    document: &SceneDocument,
    selection: &[ObjectId],
    held: &ObjectRecord,
    world: &Transform,
    snap: &Snap,
) -> Vec<Mutation> {
    let start = world_transform(document, held);
    group_mutations(document, selection, Some(&held.id), &start, world, snap)
}

/// What a drag on the gizmo means for the document. The gizmo began at `start` and is at
/// `world` now; `held` is the object it is on, or `None` when it is at the middle of several.
/// `document` is the level as it was when the drag began.
///
/// Every selected object goes as part of one rigid group: moved by as much as the gizmo was
/// moved, and turned by as much about the gizmo's place. The distances between them stay
/// what they were. An object under another selected one is left to follow that one. A
/// landscape is not moved by the gizmo.
///
/// A scale makes every selected box bigger or smaller by the same factors, each about its own
/// middle: the document has no scale, and a scaled box is a bigger box.
fn group_mutations(
    document: &SceneDocument,
    selection: &[ObjectId],
    held: Option<&ObjectId>,
    start: &Transform,
    world: &Transform,
    snap: &Snap,
) -> Vec<Mutation> {
    // One move and one turn for the whole group. A drag that does not turn leaves every
    // rotation exactly as it was.
    let moved = world.translation - start.translation;
    let turned = (world.rotation != start.rotation)
        .then(|| (world.rotation * start.rotation.inverse()).normalize());
    let mut mutations = Vec::new();
    let selected = selection.iter().filter_map(|id| document.object(id));
    for object in selected.clone() {
        if matches!(object.data, ObjectData::Terrain(_))
            || under_a_selected(document, object, selection)
        {
            continue;
        }
        let to = if held == Some(&object.id) {
            *world
        } else {
            let was = world_transform(document, object);
            match turned {
                Some(turn) => Transform {
                    translation: world.translation + turn * (was.translation - start.translation),
                    rotation: turn * was.rotation,
                    scale: Vec3::ONE,
                },
                None => Transform {
                    translation: was.translation + moved,
                    ..was
                },
            }
        };
        mutations.push(Mutation::Transform {
            object: object.id.clone(),
            value: local_of(document, object, &to),
        });
    }
    if world.scale != Vec3::ONE {
        for object in selected {
            if let ObjectData::Box { half_extents_m } = &object.data {
                mutations.push(Mutation::BoxExtents {
                    object: object.id.clone(),
                    value: snap.half_extents(*half_extents_m, world.scale),
                });
            }
        }
    }
    mutations
}

/// The gizmo turns in the steps of the snap setting while snapping is on. Moving and scaling
/// are put on the grid by this view (`snapped`), which knows the level's grid and a box's size;
/// the gizmo's own snapping of those counts steps from where the drag began.
fn snap_turns(snap: Res<Snap>, mut gizmo: ResMut<TransformGizmoSettings>) {
    let wanted = snap.angle_step();
    if gizmo.snap_rotate != wanted {
        gizmo.snap_rotate = wanted;
    }
}

/// What the gizmo can hold: an object of the level as the view shows it, or the handle at
/// the middle of several.
type GizmoHeld = Or<(With<Projected>, With<MiddleHandle>)>;

/// What the gizmo drag has done so far.
#[derive(Default)]
struct GizmoDrag {
    /// What the gizmo holds and the transform last sent to the core.
    held: Option<(Entity, Transform)>,
}

/// What ends a drag from outside the gizmo.
#[derive(SystemParam)]
struct CancelInput<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    window: Single<'w, 's, (&'static Window, &'static mut CursorOptions), With<PrimaryWindow>>,
}

/// Why a drag is cancelled, if it is.
fn cancel_reason(
    escape: bool,
    focused: bool,
    cursor_inside: bool,
    button_down: bool,
) -> Option<&'static str> {
    if escape {
        Some("Escape")
    } else if !focused {
        Some("the window lost focus")
    } else if !cursor_inside {
        Some("the cursor left the window")
    } else if !button_down {
        // The gizmo still holds the object although the button is up: it missed the release.
        Some("the release was missed")
    } else {
        None
    }
}

/// Turns a drag on the gizmo into one transaction of the core.
fn drag_with_gizmo(
    mut gizmo: ResMut<TransformGizmoState>,
    mut cancel: CancelInput,
    mut objects: Query<(Option<&Projected>, &mut Transform, &mut GlobalTransform), GizmoHeld>,
    (snap, mut editor): (Res<Snap>, NonSendMut<Editor>),
    mut shown: ResMut<Shown>,
    mut drag: Local<GizmoDrag>,
    mut redraw: MessageWriter<RequestRedraw>,
) {
    let dragged = gizmo.active.then_some(gizmo.entity).flatten();
    match (dragged, drag.held) {
        // The drag goes on.
        (Some(entity), _) => {
            let Ok((projected, mut transform, mut global)) = objects.get_mut(entity) else {
                return;
            };
            let reason = cancel_reason(
                cancel.keys.just_pressed(KeyCode::Escape),
                cancel.window.0.focused,
                cancel.window.0.cursor_position().is_some(),
                cancel.mouse.pressed(MouseButton::Left),
            );
            if let Some(reason) = reason {
                debug!("drag cancelled: {reason}");
                drag.held = None;
                editor.end_drag(false);
                // Let go in the gizmo too. It only lets go by itself on a release it sees, and
                // it does not see one while the cursor is outside the window.
                gizmo.active = false;
                gizmo.axis = None;
                gizmo.entity = None;
                gizmo.hovered_axis = None;
                // The gizmo confines the cursor for a drag and frees it on the release it sees.
                cancel.window.1.grab_mode = CursorGrabMode::None;
                // Put everything back where the document has it.
                shown.key = None;
                redraw.write(RequestRedraw);
                return;
            }
            if drag.held.is_none() {
                editor.begin_drag();
            }
            let snapshot = editor.snapshot();
            // What the gizmo is on: an object of the level, or the middle of several.
            let held = match projected {
                Some(projected) => match snapshot.document.object(&projected.0) {
                    Some(object) => Some(object),
                    None => return,
                },
                None => None,
            };
            // With snapping the object is put on the grid before it is drawn. The gizmo works
            // each frame from where the drag began, so what is written here does not add up.
            // The object has no parent in the view, and this runs after the engine has worked
            // out where things are: what is drawn is set here as well. A middle is on no grid
            // line: it goes by whole steps, so that what was on the grid stays on it.
            if snap.on {
                let to = match held {
                    Some(object) => {
                        let half = match &object.data {
                            ObjectData::Box { half_extents_m } => Some(*half_extents_m),
                            _ => None,
                        };
                        snapped(&snap, &gizmo.start_transform, &transform, half)
                    }
                    None => Transform {
                        translation: snap
                            .shifted(gizmo.start_transform.translation, transform.translation),
                        ..*transform
                    },
                };
                if *transform != to {
                    *transform = to;
                    *global = GlobalTransform::from(to);
                }
            }
            if drag.held.map(|(_, sent)| sent) != Some(*transform) {
                drag.held = Some((entity, *transform));
                let start = held.map_or(gizmo.start_transform, |object| {
                    world_transform(&snapshot.document, object)
                });
                editor.preview_drag(group_mutations(
                    &snapshot.document,
                    &snapshot.selection,
                    held.map(|object| &object.id),
                    &start,
                    &transform,
                    &snap,
                ));
            }
        }
        // The button was released.
        (None, Some((entity, _))) => {
            drag.held = None;
            editor.end_drag(true);
            // A scaled box becomes a bigger box: the mesh takes the size, the scale goes back.
            if let Ok((_, mut transform, _)) = objects.get_mut(entity) {
                transform.scale = Vec3::ONE;
            }
            shown.key = None;
            redraw.write(RequestRedraw);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_takes_the_landscape_in_the_same_surface_and_the_object_in_front_of_it() {
        // The same plane, and the hair by which either may be reported nearer.
        assert!(click_is_on_landscape(40.0, 40.0));
        assert!(click_is_on_landscape(40.004, 40.0));
        assert!(click_is_on_landscape(39.996, 40.0));
        // The landscape raised above the object's surface.
        assert!(click_is_on_landscape(38.5, 40.0));
        // The landscape dug half a metre below it: the object hides it and is what is clicked.
        assert!(!click_is_on_landscape(40.5, 40.0));
        assert!(!click_is_on_landscape(40.05, 40.0));
    }

    fn document() -> SceneDocument {
        scene_data::current_map()
    }

    fn alone(object: &ObjectRecord) -> Vec<ObjectId> {
        vec![object.id.clone()]
    }

    fn snapping() -> Snap {
        Snap {
            on: true,
            ..default()
        }
    }

    /// Three boxes of the map put where no grid line is, none under another, and their IDs.
    fn off_grid() -> (SceneDocument, Vec<ObjectId>) {
        let mut document = document();
        let places = [[0.2, 1.37, -4.11], [3.33, 0.6, 2.05], [-5.71, 2.2, 0.49]];
        let mut ids = Vec::new();
        for (object, place) in document.objects.iter_mut().skip(1).zip(places) {
            object.parent = None;
            object.local.translation_m = place;
            ids.push(object.id.clone());
        }
        (document, ids)
    }

    /// Where each of `ids` is after `mutations`, in the level.
    fn places(
        document: &SceneDocument,
        ids: &[ObjectId],
        mutations: &[Mutation],
    ) -> Vec<Transform> {
        let mut after = document.clone();
        for mutation in mutations {
            if let Mutation::Transform { object, value } = mutation {
                let record = after
                    .objects
                    .iter_mut()
                    .find(|record| record.id == *object)
                    .expect("the object is in the level");
                record.local = value.clone();
            }
        }
        ids.iter()
            .map(|id| world_transform(&after, after.object(id).unwrap()))
            .collect()
    }

    #[test]
    fn a_snapped_move_puts_the_held_object_on_the_grid_and_keeps_every_distance_in_the_group() {
        let (document, ids) = off_grid();
        let held = document.object(&ids[0]).unwrap();
        let start = world_transform(&document, held);
        // Dragged 1.17 m along X: the held object lands on the grid line at 1.5 m.
        let dragged = Transform {
            translation: start.translation + Vec3::new(1.17, 0.0, 0.0),
            ..start
        };
        let to = snapped(&snapping(), &start, &dragged, None);
        assert_eq!(to.translation, Vec3::new(1.5, 1.37, -4.11));
        let mutations = drag_mutations(&document, &ids, held, &to, &snapping());
        assert_eq!(mutations.len(), 3, "one transform an object, and no size");
        let before = places(&document, &ids, &[]);
        let after = places(&document, &ids, &mutations);
        // One move for all of them, the held object's: where each stands from the held
        // object is what it was.
        for (was, is) in before.iter().zip(&after) {
            let offset_was = was.translation - before[0].translation;
            let offset_is = is.translation - after[0].translation;
            assert!(
                offset_was.distance(offset_is) < 1e-5,
                "{offset_was} became {offset_is}"
            );
            // A rotation is written back as a unit quaternion, which the map's are only nearly.
            assert!(
                is.rotation.angle_between(was.rotation) < 1e-4,
                "a move turns nothing"
            );
        }
        // The others began off the grid and are not pulled onto it one by one.
        assert_ne!(
            after[1].translation.x,
            snapping().position(Vec3::ZERO, after[1].translation).x
        );
    }

    #[test]
    fn a_turn_turns_the_group_as_one_about_the_held_object() {
        let (document, ids) = off_grid();
        let held = document.object(&ids[0]).unwrap();
        let start = world_transform(&document, held);
        let turn = Quat::from_rotation_y(15.0_f32.to_radians());
        let to = Transform {
            rotation: turn * start.rotation,
            ..start
        };
        let mutations = drag_mutations(&document, &ids, held, &to, &snapping());
        let before = places(&document, &ids, &[]);
        let after = places(&document, &ids, &mutations);
        assert_eq!(
            after[0].translation, before[0].translation,
            "the held object stays"
        );
        for (was, is) in before.iter().zip(&after) {
            // Every object is turned by the same angle…
            let by = (is.rotation * was.rotation.inverse()).normalize();
            assert!(by.angle_between(turn) < 1e-4, "{by:?}");
            // …and is as far from the held object, and from every other, as before.
            for (other_was, other_is) in before.iter().zip(&after) {
                let was = was.translation.distance(other_was.translation);
                let is = is.translation.distance(other_is.translation);
                assert!((was - is).abs() < 1e-4, "{was} became {is}");
            }
        }
    }

    #[test]
    fn an_object_under_a_selected_one_follows_it_and_is_not_moved_twice() {
        let (mut document, ids) = off_grid();
        document.objects[2].parent = Some(ids[0].clone());
        let held = document.object(&ids[0]).unwrap();
        let start = world_transform(&document, held);
        let to = Transform {
            translation: start.translation + Vec3::X,
            ..start
        };
        let mutations = drag_mutations(&document, &ids, held, &to, &Snap::default());
        let moved: Vec<&ObjectId> = mutations
            .iter()
            .map(|mutation| match mutation {
                Mutation::Transform { object, .. } => object,
                other => panic!("expected a transform, got {other:?}"),
            })
            .collect();
        assert_eq!(
            moved,
            [&ids[0], &ids[2]],
            "the child has no change of its own"
        );
        // In the level it has gone exactly as far as the others.
        let before = places(&document, &ids, &[]);
        let after = places(&document, &ids, &mutations);
        for (was, is) in before.iter().zip(&after) {
            assert!((is.translation - was.translation).distance(Vec3::X) < 1e-5);
        }
    }

    #[test]
    fn a_snapped_scale_shows_the_size_the_box_gets_and_sizes_every_selected_box() {
        let (document, ids) = off_grid();
        let held = document.object(&ids[0]).unwrap();
        let ObjectData::Box { half_extents_m } = held.data else {
            panic!("the map's objects are boxes");
        };
        let start = world_transform(&document, held);
        let dragged = start.with_scale(Vec3::new(1.3, 1.0, 1.0));
        let to = snapped(&snapping(), &start, &dragged, Some(half_extents_m));
        let wide = snapping().half_extents(half_extents_m, dragged.scale)[0];
        assert!((half_extents_m[0] * to.scale.x - wide).abs() < 1e-5);
        assert_eq!(
            (wide * 2.0 / 0.5).fract(),
            0.0,
            "a whole number of grid steps"
        );
        let mutations = drag_mutations(&document, &ids, held, &to, &snapping());
        let sized = mutations
            .iter()
            .filter(|mutation| matches!(mutation, Mutation::BoxExtents { .. }))
            .count();
        assert_eq!(sized, 3);
    }

    /// The level as the core shows it with these objects selected, in this order, and the
    /// gizmo wanted there.
    fn shown_with(
        document: SceneDocument,
        selection: Vec<ObjectId>,
        pivot: GizmoPivot,
    ) -> ViewSnapshot {
        use editor_core::{Command, CommandRequest, EditorSession};
        let mut session = EditorSession::new(document).unwrap();
        let commands = [Command::Select(selection), Command::SetGizmoPivot(pivot)];
        for (number, command) in commands.into_iter().enumerate() {
            let before = session.snapshot();
            session
                .submit(CommandRequest {
                    request: number as u64 + 1,
                    session: before.session,
                    expected_revision: before.document_revision,
                    expected_inspector_revision: None,
                    command,
                })
                .outcome
                .unwrap();
        }
        (*session.snapshot()).clone()
    }

    #[test]
    fn the_gizmo_is_on_the_object_selected_last_and_on_nothing_without_a_selection() {
        let (document, ids) = off_grid();
        // The core puts the gizmo on the last object it is given, whatever its ID.
        let order = vec![ids[2].clone(), ids[0].clone(), ids[1].clone()];
        let snapshot = shown_with(document.clone(), order, GizmoPivot::Active);
        assert_eq!(gizmo_target(&snapshot), Some(GizmoTarget::Object(&ids[1])));
        let nothing = shown_with(document, Vec::new(), GizmoPivot::Active);
        assert_eq!(gizmo_target(&nothing), None);
    }

    #[test]
    fn the_middle_is_the_mean_of_where_the_selected_objects_are_and_one_object_has_none() {
        let (document, ids) = off_grid();
        let snapshot = shown_with(document.clone(), ids.clone(), GizmoPivot::Center);
        let Some(GizmoTarget::Middle(at)) = gizmo_target(&snapshot) else {
            panic!("three objects have a middle");
        };
        // The three places of `off_grid`, summed and divided by three.
        let mean = Vec3::new(0.2 + 3.33 - 5.71, 1.37 + 0.6 + 2.2, -4.11 + 2.05 + 0.49) / 3.0;
        assert!(at.distance(mean) < 1e-5, "{at}");
        // One object: the gizmo is on it, whatever was chosen.
        let one = shown_with(document.clone(), vec![ids[0].clone()], GizmoPivot::Center);
        assert_eq!(gizmo_target(&one), Some(GizmoTarget::Object(&ids[0])));
        // An object under another selected one is no part of the middle: it goes where that
        // one goes.
        let mut nested = document;
        nested.object_mut(&ids[2]).unwrap().parent = Some(ids[0].clone());
        let snapshot = shown_with(nested, ids.clone(), GizmoPivot::Center);
        let Some(GizmoTarget::Middle(at)) = gizmo_target(&snapshot) else {
            panic!("two objects are left that have a middle");
        };
        let mean = Vec3::new(0.2 + 3.33, 1.37 + 0.6, -4.11 + 2.05) / 2.0;
        assert!(at.distance(mean) < 1e-5, "{at}");
    }

    #[test]
    fn a_turn_about_the_middle_keeps_the_middle_and_a_move_of_it_moves_all_alike() {
        let (document, ids) = off_grid();
        let before = places(&document, &ids, &[]);
        let middle = before.iter().map(|place| place.translation).sum::<Vec3>() / 3.0;
        let start = Transform::from_translation(middle);

        // Half a turn about Y: each object goes to the other side of the middle.
        let turned = start.with_rotation(Quat::from_rotation_y(std::f32::consts::PI));
        let mutations = group_mutations(&document, &ids, None, &start, &turned, &Snap::default());
        assert_eq!(mutations.len(), 3);
        let after = places(&document, &ids, &mutations);
        let middle_after = after.iter().map(|place| place.translation).sum::<Vec3>() / 3.0;
        assert!(middle_after.distance(middle) < 1e-4, "{middle_after}");
        for (was, is) in before.iter().zip(&after) {
            let mirrored = middle - (was.translation - middle) * Vec3::new(1.0, -1.0, 1.0);
            assert!(
                is.translation.distance(mirrored) < 1e-4,
                "{}",
                is.translation
            );
        }

        // A move of the middle by whole steps: every object by exactly as much, so that the
        // fractions each stood at are the fractions it stands at.
        let dragged = Transform::from_translation(middle + Vec3::new(1.17, 0.0, 0.0));
        let to = Transform {
            translation: snapping().shifted(start.translation, dragged.translation),
            ..dragged
        };
        let mutations = group_mutations(&document, &ids, None, &start, &to, &snapping());
        let after = places(&document, &ids, &mutations);
        for (was, is) in before.iter().zip(&after) {
            let moved = is.translation - was.translation;
            assert!(moved.distance(Vec3::new(1.0, 0.0, 0.0)) < 1e-4, "{moved}");
            // Each rotation is written back as a unit quaternion, a rounding away from what
            // it was.
            assert!(
                is.rotation.angle_between(was.rotation) < 1e-4,
                "a move turns nothing"
            );
        }
    }

    #[test]
    fn a_moved_and_turned_object_becomes_one_transform_mutation() {
        let document = document();
        let ramp = &document.objects[1];
        let world = Transform::from_xyz(1.0, 2.0, 3.0).with_rotation(Quat::from_rotation_y(0.5));
        let mutations = drag_mutations(&document, &alone(ramp), ramp, &world, &Snap::default());
        assert_eq!(mutations.len(), 1);
        let Mutation::Transform { object, value } = &mutations[0] else {
            panic!("expected a transform");
        };
        assert_eq!(object, &ramp.id);
        assert_eq!(value.translation_m, [1.0, 2.0, 3.0]);
        let length: f32 = value.rotation_xyzw.iter().map(|c| c * c).sum();
        assert!(
            (length - 1.0).abs() < 1e-5,
            "the rotation is a unit quaternion"
        );
    }

    #[test]
    fn a_scaled_box_becomes_a_bigger_box_and_not_a_scale() {
        let document = document();
        let ramp = &document.objects[1];
        let ObjectData::Box { half_extents_m } = ramp.data else {
            panic!("the ramp is a box");
        };
        let world = world_transform(&document, ramp).with_scale(Vec3::new(2.0, 1.0, 0.5));
        let mutations = drag_mutations(&document, &alone(ramp), ramp, &world, &Snap::default());
        let Some(Mutation::BoxExtents { value, .. }) = mutations.get(1) else {
            panic!("expected new extents");
        };
        assert_eq!(
            *value,
            [
                half_extents_m[0] * 2.0,
                half_extents_m[1],
                half_extents_m[2] * 0.5
            ]
        );
    }

    #[test]
    fn a_drag_goes_on_only_with_focus_the_cursor_inside_and_the_button_down() {
        assert_eq!(cancel_reason(false, true, true, true), None);
        assert!(cancel_reason(true, true, true, true).is_some());
        assert!(cancel_reason(false, false, true, true).is_some());
        assert!(cancel_reason(false, true, false, true).is_some());
        assert!(cancel_reason(false, true, true, false).is_some());
    }

    fn key(revision: u64, selection: u64) -> ShownKey {
        ShownKey {
            session: 1,
            document_revision: revision,
            preview: None,
            selection_revision: selection,
            gizmo_allowed: true,
            game_look: false,
        }
    }

    fn snapshot_with(selection: &[ObjectId], changed: &[ObjectId]) -> editor_core::ViewSnapshot {
        let session = editor_core::EditorSession::new(document()).unwrap();
        let mut snapshot = (*session.snapshot()).clone();
        snapshot.selection = selection.to_vec().into();
        snapshot.changed_objects = changed.to_vec().into();
        snapshot
    }

    #[test]
    fn the_first_drawing_and_another_level_redraw_everything() {
        let snapshot = snapshot_with(&[], &[]);
        assert!(matches!(
            scope(None, key(1, 1), &[], &snapshot),
            Some(Scope::Everything)
        ));
        let other = ShownKey {
            session: 2,
            ..key(1, 1)
        };
        assert!(matches!(
            scope(Some(key(1, 1)), other, &[], &snapshot),
            Some(Scope::Everything)
        ));
        // And so does another way of drawing the same level: every box gets another material.
        let game_look = ShownKey {
            game_look: true,
            ..key(1, 1)
        };
        assert!(matches!(
            scope(Some(key(1, 1)), game_look, &[], &snapshot),
            Some(Scope::Everything)
        ));
        assert!(scope(Some(game_look), game_look, &[], &snapshot).is_none());
    }

    #[test]
    fn nothing_changed_redraws_nothing() {
        let snapshot = snapshot_with(&[], &[]);
        assert!(scope(Some(key(4, 2)), key(4, 2), &[], &snapshot).is_none());
    }

    #[test]
    fn one_revision_redraws_only_the_objects_the_core_names() {
        let document = document();
        let changed = document.objects[3].id.clone();
        let snapshot = snapshot_with(&[], std::slice::from_ref(&changed));
        let Some(Scope::Objects(objects)) = scope(Some(key(4, 2)), key(5, 2), &[], &snapshot)
        else {
            panic!("expected a list of objects");
        };
        assert_eq!(objects, vec![changed]);
    }

    #[test]
    fn a_new_selection_redraws_the_old_and_the_new_selection_only() {
        let document = document();
        let old = document.objects[1].id.clone();
        let new = document.objects[7].id.clone();
        let snapshot = snapshot_with(std::slice::from_ref(&new), &[]);
        let Some(Scope::Objects(mut objects)) = scope(
            Some(key(4, 2)),
            key(4, 3),
            std::slice::from_ref(&old),
            &snapshot,
        ) else {
            panic!("expected a list of objects");
        };
        objects.sort();
        let mut wanted = vec![old, new];
        wanted.sort();
        assert_eq!(objects, wanted);
    }

    #[test]
    fn several_revisions_at_once_redraw_everything() {
        let snapshot = snapshot_with(&[], &[]);
        assert!(matches!(
            scope(Some(key(4, 2)), key(7, 2), &[], &snapshot),
            Some(Scope::Everything)
        ));
    }

    #[test]
    fn the_camera_looks_at_its_target_from_its_distance() {
        let orbit = Orbit::default();
        let transform = orbit.transform();
        assert!((transform.translation.distance(orbit.target) - orbit.distance).abs() < 1e-3);
        let to_target = (orbit.target - transform.translation).normalize();
        assert!(transform.forward().dot(to_target) > 0.999);
    }
}
