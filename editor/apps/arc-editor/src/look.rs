//! Game look: the 3D view lit and coloured as the game's client draws a level.
//!
//! The numbers come from the game's look profile, which the editor's services read from the
//! game's folder (`editor_services::load_look_profile`); the client reads the same file. This
//! module turns the profile into what the engine draws with, the same components the client
//! puts on its sun, its sky and its camera, the same box texture and box mesh, and the
//! profile's colour and roughness on the ground's material. Nothing of
//! the look is written down here a second time: a value that is not in the profile is not
//! guessed, and without a profile the view keeps the editor's own picture.
//!
//! What is chosen is the window's ([`ViewMode`]). Going back to the editor's picture takes
//! everything off again that Game look put on.
//!
//! The profile is the one the project names. It is read when the editor starts, when a
//! project is entered from the project browser, and again whenever the Project settings are
//! saved, so that a profile chosen there is what Game look is drawn from at once, without
//! another start. A project whose profile is not there has no Game look, and the view's
//! control says which file is missing.

use std::path::PathBuf;

use bevy::{
    asset::RenderAssetUsages,
    camera::{visibility::RenderLayers, Exposure, Hdr},
    core_pipeline::tonemapping::Tonemapping,
    gizmos::transform_gizmo::TransformGizmoCamera,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::{
        atmosphere::ScatteringMedium, Atmosphere, AtmosphereEnvironmentMapLight,
        CascadeShadowConfigBuilder, DirectionalLightShadowMap,
    },
    mesh::{Indices, PrimitiveTopology},
    pbr::AtmosphereSettings,
    post_process::bloom::Bloom,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::RequestRedraw,
};
use editor_core::Diagnostic;
use editor_ui::{
    look::{LoadedLook, LookBoxes, LookTexture},
    theme::{color, feathers::to_color},
    Editor, Launcher, LookStatus, Project, ViewMode,
};
use landscape_render::{GroundLook, GroundRenderer};
use scene_data::ObjectData;

use crate::viewport::{world_transform, ViewCamera};

/// The game's look profile, if the editor holds one.
#[derive(Resource, Default)]
pub struct GameLook(pub Option<LoadedLook>);

/// The editor's own light, which Game look puts out while the game's sun shines.
#[derive(Component)]
pub struct EditorSun;

/// The sun Game look makes from the profile.
#[derive(Component)]
pub struct LookSun;

/// The sky Game look makes from the profile.
#[derive(Component)]
pub struct LookSky;

/// How bright the editor's own light and its ambient light are, in the engine's units.
pub const EDITOR_SUN_ILLUMINANCE: f32 = 8000.0;
pub const EDITOR_AMBIENT_BRIGHTNESS: f32 = 400.0;

/// What the editor knows of the profile after looking for it, and the profile if it has it.
/// `found` is the file that was looked in and what reading it gave; `None` when the editor
/// was started without a game and had nowhere to look.
pub fn look_status(
    found: Option<(PathBuf, Result<LoadedLook, Diagnostic>)>,
) -> (LookStatus, Option<LoadedLook>) {
    match found {
        None => (LookStatus::NoGame, None),
        Some((path, Err(refused))) => (
            LookStatus::Refused {
                field: refused.parameters.get("field").cloned(),
                code: refused.code,
                path: path.display().to_string(),
            },
            None,
        ),
        Some((_, Ok(loaded))) => (
            LookStatus::Loaded {
                identity: Some(loaded.sha256_hex.clone()),
            },
            Some(loaded),
        ),
    }
}

/// What the editor knows and holds after the Project settings were saved: the profile the
/// project names now, as it was read then. `None` means that nothing changes: there is no
/// project that names one, or the profile is the one the editor holds already, so that a
/// save of other settings draws nothing anew.
pub fn look_after_save(
    held: (&LookStatus, Option<&LoadedLook>),
    found: Option<(PathBuf, Result<LoadedLook, Diagnostic>)>,
) -> Option<(LookStatus, Option<LoadedLook>)> {
    let (status, look) = look_status(Some(found?));
    (held != (&status, look.as_ref())).then_some((status, look))
}

/// Reads the project's look profile again when its settings were saved, and when a project
/// was entered from the project browser, and holds what was read in place of the profile of
/// the start. A profile that cannot be read then is said to be refused, as at the start, and
/// Game look goes back to the editor's own picture.
pub(crate) fn follow_saved_settings(
    (project, launcher): (Res<Project>, Res<Launcher>),
    mut status: ResMut<LookStatus>,
    mut look: ResMut<GameLook>,
    mut seen: Local<(u64, u64)>,
    mut redraw: MessageWriter<RequestRedraw>,
) {
    // A project that was entered counts its own saves from nought, so the saves alone do
    // not tell one project's from another's.
    let now = (project.settings_saves(), launcher.times_entered());
    if std::mem::replace(&mut *seen, now) == now {
        return;
    }
    let Some((read, loaded)) = look_after_save((&*status, look.0.as_ref()), project.look_profile())
    else {
        return;
    };
    *status = read;
    if look.0 != loaded {
        look.0 = loaded;
    }
    // The editor draws only when something happens, and what shows the profile was partly
    // drawn before this was known: one more frame shows it everywhere.
    redraw.write(RequestRedraw);
}

fn linear(rgba: [f32; 4]) -> Color {
    Color::linear_rgba(rgba[0], rgba[1], rgba[2], rgba[3])
}

/// The profile's box texture as an image: every mip level, sampled nearest, repeating.
pub fn look_image(texture: &LookTexture) -> Image {
    // An image is made with one level of data, so it is made without any and given the
    // whole chain, the largest level first.
    let mut image = Image::new_uninit(
        Extent3d {
            width: texture.size,
            height: texture.size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(texture.texels.clone());
    image.texture_descriptor.mip_level_count = texture.levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mipmap_filter: ImageFilterMode::Nearest,
        ..ImageSamplerDescriptor::nearest()
    });
    image
}

/// The material of a box in Game look.
pub fn box_material(boxes: &LookBoxes, texture: Handle<Image>) -> StandardMaterial {
    StandardMaterial {
        base_color: linear(boxes.base_color_linear_rgba),
        base_color_texture: Some(texture),
        perceptual_roughness: boxes.roughness,
        ..default()
    }
}

/// A box of this whole size, centred on the origin, whose every face carries one repeat of
/// its texture for each `repeat_m` metres along a side: the client's box mesh. The editor's
/// own flat materials do not look at the texture coordinates, so the view uses this mesh in
/// both of its pictures.
pub fn box_mesh(full_extents: Vec3, repeat_m: f32) -> Mesh {
    let half = full_extents / 2.0;
    // A face's normal and its two axes, with u x v = normal so that it faces outwards.
    let faces = [
        (Vec3::X, Vec3::NEG_Z, Vec3::Y),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, Vec3::NEG_Z),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
    ];
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for (normal, u, v) in faces {
        let (half_u, half_v, half_n) =
            (u.abs().dot(half), v.abs().dot(half), normal.abs().dot(half));
        let base = positions.len() as u32;
        for (side_u, side_v) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0f32)] {
            let point = normal * half_n + u * (side_u * half_u) + v * (side_v * half_v);
            positions.push(point.to_array());
            normals.push(normal.to_array());
            uvs.push([
                half_u * (side_u + 1.0) / repeat_m,
                half_v * (1.0 - side_v) / repeat_m,
            ]);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// The level's camera made anew, with nothing of the game's look on it, in place of the one
/// that had it. Returns the new camera.
///
/// The camera is not stripped of the game's look: what the engine keeps of a camera's sky for
/// drawing is not all taken away when the sky's settings are taken off the camera, and the
/// sky was then still drawn, in its HDR format, into a picture that was no longer HDR. The
/// renderer refuses that, and the editor ended. A camera that is made anew has nothing left
/// of the sky. It is the same camera to everything else: where it looks from, its rectangle
/// of the window, its place among the cameras, and the layer it draws.
fn camera_anew(
    commands: &mut Commands,
    old: Entity,
    camera: Camera,
    transform: Transform,
    layers: Option<&RenderLayers>,
) -> Entity {
    commands.entity(old).despawn();
    let mut again = commands.spawn((
        ViewCamera,
        TransformGizmoCamera,
        Camera3d::default(),
        camera,
        transform,
    ));
    if let Some(layers) = layers {
        again.insert(layers.clone());
    }
    again.id()
}

/// Puts the game's sun, sky and camera settings on when Game look is chosen, and takes them
/// off when the editor's own picture is. The view that was never in Game look is not touched.
/// A profile that takes the place of another while Game look is drawn is put on in the same
/// way: what the other profile made goes first, the camera with it.
pub(crate) fn apply_look(
    mut commands: Commands,
    (view, look): (Res<ViewMode>, Res<GameLook>),
    camera: Single<(Entity, &mut Camera, &Transform, Option<&RenderLayers>), With<ViewCamera>>,
    mut editor_sun: Query<&mut DirectionalLight, With<EditorSun>>,
    (made, other_cameras): (
        Query<Entity, Or<(With<LookSun>, With<LookSky>)>>,
        Query<Entity, (With<Camera>, Without<ViewCamera>)>,
    ),
    (mut ground, mut materials, mut media): (
        ResMut<GroundRenderer>,
        ResMut<Assets<StandardMaterial>>,
        ResMut<Assets<ScatteringMedium>>,
    ),
    // The picture that is drawn, and in Game look the profile it is drawn from.
    mut applied: Local<(ViewMode, Option<String>)>,
) {
    // Without a profile there is no Game look, whatever is chosen.
    let loaded = look.0.as_ref().filter(|_| *view == ViewMode::GameLook);
    let wanted = match loaded {
        Some(_) => ViewMode::GameLook,
        None => ViewMode::Editor,
    };
    let identity = loaded.map(|loaded| loaded.sha256_hex.as_str());
    if applied.0 == wanted && applied.1.as_deref() == identity {
        return;
    }
    let from_game_look = applied.0 == ViewMode::GameLook;
    *applied = (wanted, identity.map(str::to_owned));
    // Every camera of the window is HDR or none is. Cameras that draw into one window
    // draw over each other's picture only while they agree in this; when the level's
    // camera alone is HDR, the camera that draws the panels has a picture of its own, in
    // which the 3D view is the window's colour, and that picture is laid over the level's.
    // The view was then empty in Game look, with nothing said anywhere.
    for other in &other_cameras {
        if wanted == ViewMode::GameLook {
            commands.entity(other).insert(Hdr);
        } else {
            commands.entity(other).remove::<Hdr>();
        }
    }
    let (camera_entity, mut camera, transform, layers) = camera.into_inner();
    // The sun and the sky of the profile that was drawn from go, whatever is drawn next.
    for made in &made {
        commands.entity(made).despawn();
    }
    let Some(loaded) = loaded else {
        // Back to the editor's own picture: its light, its ambient light, and a camera
        // that has nothing the profile asked for (`camera_anew`).
        let mut plain = camera.clone();
        plain.clear_color = ClearColorConfig::Custom(to_color(color::SURFACE_SUNKEN));
        camera_anew(&mut commands, camera_entity, plain, *transform, layers);
        for mut light in &mut editor_sun {
            light.illuminance = EDITOR_SUN_ILLUMINANCE;
        }
        commands.insert_resource(GlobalAmbientLight {
            brightness: EDITOR_AMBIENT_BRIGHTNESS,
            ..default()
        });
        commands.insert_resource(DirectionalLightShadowMap::default());
        // The ground's material is the renderer's own again.
        ground.set_look(None, &mut materials);
        return;
    };
    let profile = &loaded.profile;
    let (camera_look, sky, sun) = (&profile.camera, &profile.sky, &profile.sun);
    // The profile's clear colour, on the camera the editor's picture was drawn with. A
    // camera that has another profile's look on it is not given this one's over it: it is
    // made anew, for the reason it is on the way back, and this profile is the only one
    // the new camera ever had.
    let clear_color = ClearColorConfig::Custom(linear(sky.clear_color_linear_rgba));
    let camera_entity = if from_game_look {
        let mut cleared = camera.clone();
        cleared.clear_color = clear_color;
        camera_anew(&mut commands, camera_entity, cleared, *transform, layers)
    } else {
        camera.clear_color = clear_color;
        camera_entity
    };
    let mut on_camera = commands.entity(camera_entity);

    // Camera: exposure, tone and bloom; the sky's settings and its light on the shadows.
    on_camera.insert((
        Exposure {
            ev100: camera_look.exposure_ev100,
        },
        // The profile names its tone curve and its bloom preset; the reader takes only the
        // names this is written for.
        Tonemapping::AcesFitted,
        Bloom {
            intensity: camera_look.bloom.intensity,
            ..Bloom::NATURAL
        },
        AtmosphereSettings::default(),
    ));
    // A profile that is read has `hdr` true: the sky and the bloom need it, and the reader
    // refuses another.
    on_camera.insert(Hdr);
    if sky.environment_map {
        on_camera.insert(AtmosphereEnvironmentMapLight::default());
    } else {
        on_camera.remove::<AtmosphereEnvironmentMapLight>();
    }

    // The ground's one material takes the profile's colour and roughness. Its paint stays
    // where it is, in the vertices, as the profile says the game has it.
    ground.set_look(
        Some(GroundLook {
            base_color_linear_rgba: profile.ground.base_color_linear_rgba,
            roughness: profile.ground.roughness,
        }),
        &mut materials,
    );

    // Sky: the atmosphere is an entity of its own. The profile's ambient light is none: the
    // sky lights the shadows.
    let [falloff, phase] = sky.scattering_lut_size;
    commands.spawn((
        LookSky,
        Atmosphere::earth(media.add(ScatteringMedium::earth(falloff, phase))),
    ));
    commands.insert_resource(GlobalAmbientLight::NONE);

    // Sun, with its shadows. The editor's own light goes out meanwhile.
    for mut light in &mut editor_sun {
        light.illuminance = 0.0;
    }
    commands.insert_resource(DirectionalLightShadowMap {
        size: sun.shadows.map_size as usize,
    });
    commands.spawn((
        LookSun,
        DirectionalLight {
            illuminance: sun.illuminance_lux,
            color: linear(sun.color_linear_rgba),
            shadow_maps_enabled: sun.shadows.enabled,
            ..default()
        },
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: sun.shadows.first_cascade_far_bound_m,
            maximum_distance: sun.shadows.maximum_distance_m,
            ..default()
        }
        .build(),
        Transform::from_translation(Vec3::from_array(sun.position_m))
            .looking_at(Vec3::from_array(sun.look_at_m), Vec3::Y),
    ));
}

/// How much bigger than a box its outline is, so that the lines are not in its faces.
const OUTLINE_MARGIN: f32 = 1.01;
/// The width of the outline of a spawn point, which is drawn as a ball of this diameter.
const SPAWN_OUTLINE_M: f32 = 0.6;

/// In Game look a selected object keeps the game's material and is marked by an outline in
/// the accent colour, drawn over the picture. In the editor's own picture the selection is
/// its accent material, as before, and nothing is outlined.
fn outline_selection(
    view: Res<ViewMode>,
    look: Res<GameLook>,
    editor: NonSend<Editor>,
    mut gizmos: Gizmos,
) {
    if *view != ViewMode::GameLook || look.0.is_none() {
        return;
    }
    let snapshot = editor.snapshot();
    let document = snapshot
        .preview
        .as_ref()
        .map_or(&snapshot.document, |preview| &preview.document);
    let accent = to_color(color::ACCENT);
    for object in snapshot
        .selection
        .iter()
        .filter_map(|id| document.object(id))
    {
        let size = match &object.data {
            ObjectData::Box { half_extents_m } => Vec3::from_array(*half_extents_m) * 2.0,
            ObjectData::Spawn { .. } | ObjectData::Emitter { .. } => Vec3::splat(SPAWN_OUTLINE_M),
            ObjectData::Group | ObjectData::Terrain(_) => continue,
        };
        let place = world_transform(document, object);
        gizmos.cube(place.with_scale(size * OUTLINE_MARGIN), accent);
    }
}

pub struct LookPlugin;

impl Plugin for LookPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameLook>().add_systems(
            Update,
            (follow_saved_settings, apply_look, outline_selection).chain(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    use editor_ui::look::{load_look_profile, parse_look_profile, procedural_brick_v1};
    use std::path::Path;

    /// The profile the scripted checks use, which is the game's first version.
    const FIXTURE: &str = include_str!("../checks/look-v1.json");

    /// What the editor knows after looking for a profile with these bytes, or for one that
    /// is not there.
    fn looked_for(profile: Option<&str>) -> (LookStatus, Option<LoadedLook>) {
        let path = PathBuf::from("/no/such/game/assets/render/look-v1.json");
        let read = match profile {
            Some(profile) => parse_look_profile(profile.as_bytes(), Path::new(&path)),
            None => load_look_profile(&path),
        };
        look_status(Some((path, read)))
    }

    /// The SHA-256 of the game's profile as it is in the game's repository, which the
    /// fixture is a copy of, byte for byte.
    const GAME_PROFILE_SHA256: &str =
        "3abf9a2fbcb48b748607934137653f334e85ab9a226bfb3383e92c78387715e6";

    #[test]
    fn without_a_game_there_is_no_profile_and_game_look_is_not_offered() {
        let (status, look) = look_status(None);
        assert_eq!(status, LookStatus::NoGame);
        assert!(look.is_none() && !status.loaded());
    }

    #[test]
    fn the_games_profile_is_loaded_and_named_by_the_hash_of_its_bytes() {
        let (status, look) = looked_for(Some(FIXTURE));
        let loaded = look.expect("the fixture is a profile");
        assert_eq!(
            status,
            LookStatus::Loaded {
                identity: Some(loaded.sha256_hex.clone())
            }
        );
        assert_eq!(loaded.profile.sun.illuminance_lux, 130_000.0);
        assert_eq!(loaded.sha256_hex, GAME_PROFILE_SHA256);
    }

    #[test]
    fn a_profile_that_is_missing_or_not_valid_is_refused_with_its_code_file_and_field() {
        let (missing, look) = looked_for(None);
        assert!(look.is_none());
        let LookStatus::Refused { code, path, field } = missing else {
            panic!("a game without a profile is refused");
        };
        assert_eq!((code.as_str(), field), ("look.missing", None));
        assert!(path.ends_with("look-v1.json"), "{path}");

        for (profile, code, field) in [
            ("{", "look.decode", None),
            (r#"{"schema": 2}"#, "look.schema", None),
            (
                &FIXTURE.replace("\"aces_fitted\"", "\"filmic\""),
                "look.decode",
                None,
            ),
            (
                &FIXTURE.replace("\"map_size\": 4096", "\"map_size\": 1000"),
                "look.value",
                Some("sun.shadows.map_size"),
            ),
        ] {
            let (status, look) = looked_for(Some(profile));
            assert!(look.is_none() && !status.loaded(), "{code}");
            let LookStatus::Refused {
                code: got,
                field: at,
                ..
            } = status
            else {
                panic!("{code}: refused");
            };
            assert_eq!((got.as_str(), at.as_deref()), (code, field));
        }
    }

    #[test]
    fn a_save_of_the_settings_gives_the_projects_profile_unless_it_is_the_one_held() {
        let path = || PathBuf::from("/no/such/game/assets/render/dusk.json");
        let read = |profile: &str| parse_look_profile(profile.as_bytes(), &path());
        let (status, look) = looked_for(Some(FIXTURE));
        let held = (&status, look.as_ref());

        // Without a project that names a profile there is nothing to take.
        assert_eq!(look_after_save(held, None), None);
        // The profile the editor holds, read again as it was: a save of other settings.
        assert_eq!(look_after_save(held, Some((path(), read(FIXTURE)))), None);

        // Another profile is the one held from then on, named by its own bytes.
        let dusk = FIXTURE.replace("\"illuminance_lux\": 130000", "\"illuminance_lux\": 40000");
        let (now, taken) = look_after_save(held, Some((path(), read(&dusk)))).unwrap();
        let taken = taken.expect("the other profile is a profile");
        assert_eq!(taken.profile.sun.illuminance_lux, 40_000.0);
        assert_ne!(taken.sha256_hex, GAME_PROFILE_SHA256);
        assert_eq!(
            now,
            LookStatus::Loaded {
                identity: Some(taken.sha256_hex.clone())
            }
        );

        // One that cannot be read is refused as at the start, and no profile is held.
        let (refused, none) = look_after_save(held, Some((path(), read("{")))).unwrap();
        assert!(none.is_none() && !refused.loaded());
        let LookStatus::Refused { code, path: at, .. } = &refused else {
            panic!("a profile that is not one is refused");
        };
        assert_eq!(code, "look.decode");
        assert!(at.ends_with("dusk.json"), "{at}");
        // Refused again in the same way, nothing changes; read at last, it is taken.
        let held = (&refused, None);
        assert_eq!(look_after_save(held, Some((path(), read("{")))), None);
        assert!(look_after_save(held, Some((path(), read(FIXTURE)))).is_some());

        // An editor that had no profile, as the one behind the project browser, takes the
        // one of the project that is entered; and where that project's profile is not
        // there, it holds none and says which file it looked for.
        let (nothing, _) = look_status(None);
        let absent = load_look_profile(&path());
        let (now, taken) = look_after_save((&nothing, None), Some((path(), absent))).unwrap();
        assert!(taken.is_none());
        let LookStatus::Refused { code, path: at, .. } = now else {
            panic!("a profile that is not there is refused");
        };
        assert_eq!(code, "look.missing");
        assert!(at.ends_with("dusk.json"), "{at}");
        let (now, taken) =
            look_after_save((&nothing, None), Some((path(), read(FIXTURE)))).unwrap();
        assert!(now.loaded() && taken.is_some());
    }

    #[test]
    fn the_box_texture_is_the_profiles_brick_with_every_level_nearest_and_repeating() {
        let brick = procedural_brick_v1();
        let image = look_image(&brick);
        assert_eq!((image.width(), image.height()), (16, 16));
        assert_eq!(image.texture_descriptor.mip_level_count, 5);
        assert_eq!(image.data.as_ref(), Some(&brick.texels));
        let ImageSampler::Descriptor(sampler) = &image.sampler else {
            panic!("a sampler of its own");
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::Repeat);
        assert_eq!(sampler.mag_filter, ImageFilterMode::Nearest);
        assert_eq!(sampler.mipmap_filter, ImageFilterMode::Nearest);
    }

    #[test]
    fn a_box_carries_one_repeat_of_its_texture_a_metre_on_every_face() {
        // 3 m wide, 2 m high, 4 m deep.
        let mesh = box_mesh(Vec3::new(3.0, 2.0, 4.0), 1.0);
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("texture coordinates");
        };
        assert_eq!(uvs.len(), 24);
        let reach = |face: usize| {
            uvs[face * 4..face * 4 + 4]
                .iter()
                .fold([0.0_f32; 2], |most, uv| {
                    [most[0].max(uv[0]), most[1].max(uv[1])]
                })
        };
        // The faces across X are depth by height, those across Y width by depth, those
        // across Z width by height.
        assert_eq!(reach(0), [4.0, 2.0]);
        assert_eq!(reach(2), [3.0, 4.0]);
        assert_eq!(reach(4), [3.0, 2.0]);
        // Twice as long a repeat, half as many of them.
        let coarser = box_mesh(Vec3::new(3.0, 2.0, 4.0), 2.0);
        let Some(VertexAttributeValues::Float32x2(uvs)) = coarser.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("texture coordinates");
        };
        assert!(uvs.iter().all(|uv| uv[0] <= 2.0 && uv[1] <= 2.0));
    }

    #[test]
    fn a_box_in_game_look_has_the_profiles_colour_roughness_and_texture() {
        let (_, look) = looked_for(Some(FIXTURE));
        let boxes = look.unwrap().profile.boxes;
        let material = box_material(&boxes, Handle::default());
        assert_eq!(material.base_color, Color::linear_rgba(1.0, 1.0, 1.0, 1.0));
        assert_eq!(material.perceptual_roughness, 1.0);
        assert!(material.base_color_texture.is_some());
    }
}
