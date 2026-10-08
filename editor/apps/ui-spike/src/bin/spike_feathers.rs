//! The fixed-panel shell with the engine's own UI and the Feathers widgets that exist at the pin.

use std::time::Instant;

use bevy::{
    asset::RenderAssetUsages,
    camera::RenderTarget,
    feathers::{
        containers::{pane, pane_body, pane_header},
        controls::{FeathersButton, FeathersNumberInput, NumberInputValue, UpdateNumberInput},
        dark_theme::create_dark_theme,
        theme::{ThemeBackgroundColor, ThemedText, UiTheme},
        tokens, FeathersPlugins,
    },
    prelude::*,
    render::render_resource::{TextureDimension, TextureFormat, TextureUsages},
    ui::widget::ViewportNode,
    window::{PresentMode, WindowResolution},
    winit::{UpdateMode, WinitSettings},
};
use ui_spike::{
    advance_stress, camera_transform, measure, row_label, screenshot, spawn_world, Measurement,
    Mode, Stress, HIERARCHY_ROWS, HIERARCHY_WIDTH, INSPECTOR_FIELDS, INSPECTOR_WIDTH, ROW_HEIGHT,
    STATUS_HEIGHT, TOOLBAR_HEIGHT, VISIBLE_ROWS, WINDOW_SIZE,
};

/// One of the pooled hierarchy rows; the number is its place in the panel.
#[derive(Component, Default, Clone)]
struct RowSlot(usize);

/// One inspector field; the number is its index.
#[derive(Component, Default, Clone)]
struct FieldSlot(usize);

/// The node the 3D view is shown in.
#[derive(Component, Default, Clone)]
struct ViewportSlot;

#[derive(Component, Default, Clone)]
struct StatusText;

/// The camera that draws the 3D view into the viewport's image.
#[derive(Resource)]
struct ViewCamera(Entity);

fn main() {
    let started = Instant::now();
    let mode = Mode::from_args();
    let winit = match mode {
        Mode::Load => WinitSettings {
            focused_mode: UpdateMode::Continuous,
            unfocused_mode: UpdateMode::Continuous,
        },
        Mode::Idle => WinitSettings::desktop_app(),
    };
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "ui-spike: feathers".into(),
                    resolution: WindowResolution::new(WINDOW_SIZE.0, WINDOW_SIZE.1),
                    present_mode: PresentMode::AutoNoVsync,
                    ..default()
                }),
                ..default()
            }),
            FeathersPlugins,
        ))
        .insert_resource(winit)
        .insert_resource(UiTheme(create_dark_theme()))
        .insert_resource(Measurement::new("feathers", mode, started))
        .init_resource::<Stress>()
        .add_systems(Startup, (setup, ui.spawn()))
        .add_systems(
            Update,
            (
                attach_viewport,
                advance_stress,
                (show_rows, show_fields, show_status).after(advance_stress),
                (screenshot, measure).chain(),
            ),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_world(&mut commands, &mut meshes, &mut materials);

    // The camera that draws the UI to the window.
    commands.spawn(Camera2d);

    // The 3D view is drawn into an image, which the viewport node shows and resizes.
    let mut image = Image::new_uninit(
        default(),
        TextureDimension::D2,
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::all(),
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let target = images.add(image);
    let camera = commands
        .spawn((
            Camera3d::default(),
            Camera {
                order: -1,
                ..default()
            },
            RenderTarget::Image(target.into()),
            camera_transform(),
        ))
        .id();
    commands.insert_resource(ViewCamera(camera));
}

fn ui() -> impl Scene {
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(2),
        }
        ThemeBackgroundColor(tokens::WINDOW_BG)
        Children [ toolbar(), middle(), status_bar() ]
    }
}

fn toolbar() -> impl Scene {
    let buttons: Vec<_> = [
        "Select", "Move", "Rotate", "Scale", "Terrain", "Cook", "Play",
    ]
    .into_iter()
    .map(tool_button)
    .collect();
    bsn! {
        Node {
            height: px(TOOLBAR_HEIGHT),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(4),
            padding: UiRect::horizontal(px(8)),
        }
        ThemeBackgroundColor(tokens::PANE_HEADER_BG)
        Children [ {buttons} ]
    }
}

fn tool_button(caption: &'static str) -> impl Scene {
    bsn! {
        @FeathersButton {
            @caption: bsn! { Text(caption) ThemedText },
        }
    }
}

fn middle() -> impl Scene {
    bsn! {
        Node {
            flex_grow: 1.0,
            min_height: px(0),
            flex_direction: FlexDirection::Row,
            column_gap: px(2),
        }
        Children [ hierarchy(), viewport(), inspector() ]
    }
}

fn hierarchy() -> impl Scene {
    let rows: Vec<_> = (0..VISIBLE_ROWS).map(hierarchy_row).collect();
    bsn! {
        pane()
        Node { width: px(HIERARCHY_WIDTH) }
        Children [
            (pane_header() Children [ (Text("Hierarchy") ThemedText) ]),
            (
                pane_body()
                Node { flex_grow: 1.0, min_height: px(0), row_gap: px(0), overflow: Overflow::clip() }
                Children [ {rows} ]
            ),
        ]
    }
}

fn hierarchy_row(slot: usize) -> impl Scene {
    bsn! {
        Node { height: px(ROW_HEIGHT), flex_shrink: 0.0 }
        Text("")
        ThemedText
        RowSlot(slot)
    }
}

fn viewport() -> impl Scene {
    bsn! {
        Node { flex_grow: 1.0, min_width: px(0) }
        ViewportSlot
    }
}

fn inspector() -> impl Scene {
    let rows: Vec<_> = (0..INSPECTOR_FIELDS / 3).map(field_row).collect();
    bsn! {
        pane()
        Node { width: px(INSPECTOR_WIDTH) }
        Children [
            (pane_header() Children [ (Text("Inspector") ThemedText) ]),
            (
                pane_body()
                Node { flex_grow: 1.0, min_height: px(0), overflow: Overflow::clip() }
                Children [ {rows} ]
            ),
        ]
    }
}

fn field_row(row: usize) -> impl Scene {
    let (x, y, z) = (row * 3, row * 3 + 1, row * 3 + 2);
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            column_gap: px(4),
            flex_shrink: 0.0,
        }
        Children [
            (
                @FeathersNumberInput { @sigil_color: tokens::TEXT_INPUT_X_AXIS, @label_text: "X" }
                FieldSlot(x)
                Node { flex_grow: 1.0, flex_basis: px(0) }
            ),
            (
                @FeathersNumberInput { @sigil_color: tokens::TEXT_INPUT_Y_AXIS, @label_text: "Y" }
                FieldSlot(y)
                Node { flex_grow: 1.0, flex_basis: px(0) }
            ),
            (
                @FeathersNumberInput { @sigil_color: tokens::TEXT_INPUT_Z_AXIS, @label_text: "Z" }
                FieldSlot(z)
                Node { flex_grow: 1.0, flex_basis: px(0) }
            ),
        ]
    }
}

fn status_bar() -> impl Scene {
    bsn! {
        Node {
            height: px(STATUS_HEIGHT),
            align_items: AlignItems::Center,
            padding: UiRect::horizontal(px(8)),
        }
        ThemeBackgroundColor(tokens::PANE_HEADER_BG)
        Children [ (Text("ui-spike: feathers") ThemedText StatusText) ]
    }
}

/// Puts the 3D view into its node once the UI exists.
fn attach_viewport(
    mut commands: Commands,
    camera: Res<ViewCamera>,
    slot: Query<Entity, (With<ViewportSlot>, Without<ViewportNode>)>,
) {
    for entity in &slot {
        commands.entity(entity).insert(ViewportNode::new(camera.0));
    }
}

/// Writes the visible window of the hierarchy into the pooled rows.
fn show_rows(
    stress: Res<Stress>,
    mut rows: Query<(&RowSlot, &mut Text)>,
    mut shown: Local<Option<usize>>,
) {
    if *shown == Some(stress.first_row) || rows.is_empty() {
        return;
    }
    *shown = Some(stress.first_row);
    for (slot, mut text) in &mut rows {
        let index = (stress.first_row + slot.0) % HIERARCHY_ROWS;
        text.0 = row_label(index);
    }
}

/// Sends the live value to the first three fields, and a fixed value to the others once.
fn show_fields(
    mut commands: Commands,
    stress: Res<Stress>,
    fields: Query<(Entity, &FieldSlot)>,
    mut filled: Local<bool>,
) {
    if fields.is_empty() {
        return;
    }
    let first_time = !*filled;
    *filled = true;
    if !first_time && !stress.is_changed() {
        return;
    }
    for (entity, slot) in &fields {
        let value = match slot.0 {
            0 => stress.value.x,
            1 => stress.value.y,
            2 => stress.value.z,
            _ if first_time => slot.0 as f32 * 0.25,
            _ => continue,
        };
        commands.trigger(UpdateNumberInput {
            entity,
            value: NumberInputValue::F32(value),
        });
    }
}

fn show_status(stress: Res<Stress>, mut status: Query<&mut Text, With<StatusText>>) {
    if !stress.is_changed() {
        return;
    }
    for mut text in &mut status {
        text.0 = format!(
            "ui-spike: feathers   rows {}..{} of {HIERARCHY_ROWS}",
            stress.first_row,
            stress.first_row + VISIBLE_ROWS
        );
    }
}
