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

use std::path::PathBuf;

use bevy::{
    asset::RenderAssetUsages,
    camera::{Exposure, Hdr},
    core_pipeline::tonemapping::Tonemapping,
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
};
use editor_core::Diagnostic;
use editor_ui::{
    look::{LoadedLook, LookBoxes, LookTexture},
    theme::{color, feathers::to_color},
    Editor, LookStatus, ViewMode,
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
struct LookSky;

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

/// Puts the game's sun, sky and camera settings on when Game look is chosen, and takes them
/// off when the editor's own picture is. The view that was never in Game look is not touched.
fn apply_look(
    mut commands: Commands,
    (view, look): (Res<ViewMode>, Res<GameLook>),
    camera: Single<(Entity, &mut Camera), With<ViewCamera>>,
    mut editor_sun: Query<&mut DirectionalLight, With<EditorSun>>,
    made: Query<Entity, Or<(With<LookSun>, With<LookSky>)>>,
    (mut ground, mut materials, mut media): (
        ResMut<GroundRenderer>,
        ResMut<Assets<StandardMaterial>>,
        ResMut<Assets<ScatteringMedium>>,
    ),
    mut applied: Local<ViewMode>,
) {
    // Without a profile there is no Game look, whatever is chosen.
    let wanted = match (*view, &look.0) {
        (ViewMode::GameLook, Some(_)) => ViewMode::GameLook,
        _ => ViewMode::Editor,
    };
    if *applied == wanted {
        return;
    }
    *applied = wanted;
    let (camera_entity, mut camera) = camera.into_inner();
    let mut on_camera = commands.entity(camera_entity);
    let Some(loaded) = look.0.as_ref().filter(|_| wanted == ViewMode::GameLook) else {
        // Back to the editor's own picture: its light, its ambient light, the engine's own
        // camera settings, and nothing the profile asked for.
        on_camera
            .remove::<(
                Hdr,
                Bloom,
                AtmosphereSettings,
                AtmosphereEnvironmentMapLight,
            )>()
            .insert((Exposure::default(), Tonemapping::default()));
        camera.clear_color = ClearColorConfig::Custom(to_color(color::SURFACE_SUNKEN));
        for made in &made {
            commands.entity(made).despawn();
        }
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
    camera.clear_color = ClearColorConfig::Custom(linear(sky.clear_color_linear_rgba));

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
        app.init_resource::<GameLook>()
            .add_systems(Update, (apply_look, outline_selection));
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
