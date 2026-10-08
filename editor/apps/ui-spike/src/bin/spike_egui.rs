//! The same fixed-panel shell with egui through bevy_egui.

use std::time::Instant;

use bevy::{
    camera::{visibility::RenderLayers, CameraOutputMode, Viewport},
    prelude::*,
    render::render_resource::BlendState,
    window::{PresentMode, PrimaryWindow, WindowResolution},
    winit::{UpdateMode, WinitSettings},
};
use bevy_egui::{
    egui, EguiContext, EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass,
    PrimaryEguiContext,
};
use ui_spike::{
    advance_stress, camera_transform, measure, row_label, screenshot, spawn_world, Measurement,
    Mode, Stress, HIERARCHY_ROWS, HIERARCHY_WIDTH, INSPECTOR_FIELDS, INSPECTOR_WIDTH, ROW_HEIGHT,
    STATUS_HEIGHT, TOOLBAR_HEIGHT, VISIBLE_ROWS, WINDOW_SIZE,
};

/// The values of the inspector fields. The first three follow the workload.
#[derive(Resource)]
struct Fields(Vec<f32>);

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
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "ui-spike: egui".into(),
                resolution: WindowResolution::new(WINDOW_SIZE.0, WINDOW_SIZE.1),
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .insert_resource(winit)
        .insert_resource(Measurement::new("egui", mode, started))
        .insert_resource(Fields(
            (0..INSPECTOR_FIELDS).map(|i| i as f32 * 0.25).collect(),
        ))
        .init_resource::<Stress>()
        .add_systems(Startup, setup)
        .add_systems(Update, (advance_stress, (screenshot, measure).chain()))
        .add_systems(EguiPrimaryContextPass, shell)
        .run();
}

fn setup(
    mut commands: Commands,
    mut egui_settings: ResMut<EguiGlobalSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_world(&mut commands, &mut meshes, &mut materials);

    // The 3D view draws straight to the window, into the rectangle the panels leave free.
    commands.spawn((Camera3d::default(), camera_transform()));

    // egui gets a camera of its own that covers the window, so the 3D camera's rectangle does
    // not clip it.
    egui_settings.auto_create_primary_context = false;
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        RenderLayers::none(),
        Camera {
            order: 1,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
}

fn shell(
    mut contexts: EguiContexts,
    mut camera: Single<&mut Camera, (With<Camera3d>, Without<EguiContext>)>,
    window: Single<&Window, With<PrimaryWindow>>,
    stress: Res<Stress>,
    mut fields: ResMut<Fields>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    fields.0[0] = stress.value.x;
    fields.0[1] = stress.value.y;
    fields.0[2] = stress.value.z;

    let mut root = egui::Ui::new(
        ctx.clone(),
        "root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    let top = egui::Panel::top("toolbar")
        .exact_size(TOOLBAR_HEIGHT)
        .show_inside(&mut root, |ui| {
            ui.horizontal_centered(|ui| {
                for caption in [
                    "Select", "Move", "Rotate", "Scale", "Terrain", "Cook", "Play",
                ] {
                    let _ = ui.button(caption);
                }
            });
        })
        .response
        .rect
        .height();

    let bottom = egui::Panel::bottom("status")
        .exact_size(STATUS_HEIGHT)
        .show_inside(&mut root, |ui| {
            ui.label(format!(
                "ui-spike: egui   rows {}..{} of {HIERARCHY_ROWS}",
                stress.first_row,
                stress.first_row + VISIBLE_ROWS
            ));
        })
        .response
        .rect
        .height();

    let left = egui::Panel::left("hierarchy")
        .exact_size(HIERARCHY_WIDTH)
        .show_inside(&mut root, |ui| {
            ui.heading("Hierarchy");
            egui::ScrollArea::vertical()
                .vertical_scroll_offset(stress.first_row as f32 * ROW_HEIGHT)
                .show_rows(ui, ROW_HEIGHT, HIERARCHY_ROWS, |ui, range| {
                    for index in range {
                        ui.add_sized(
                            [ui.available_width(), ROW_HEIGHT],
                            egui::Label::new(row_label(index)).truncate(),
                        );
                    }
                });
        })
        .response
        .rect
        .width();

    let right = egui::Panel::right("inspector")
        .exact_size(INSPECTOR_WIDTH)
        .show_inside(&mut root, |ui| {
            ui.heading("Inspector");
            egui::Grid::new("fields").num_columns(3).show(ui, |ui| {
                for (index, value) in fields.0.iter_mut().enumerate() {
                    let prefix = ["X ", "Y ", "Z "][index % 3];
                    ui.add(egui::DragValue::new(value).speed(0.1).prefix(prefix));
                    if index % 3 == 2 {
                        ui.end_row();
                    }
                }
            });
        })
        .response
        .rect
        .width();

    // From logical to physical pixels.
    let scale = window.scale_factor();
    let position = UVec2::new((left * scale) as u32, (top * scale) as u32);
    let taken = position + UVec2::new((right * scale) as u32, (bottom * scale) as u32);
    let window_size = UVec2::new(window.physical_width(), window.physical_height());
    if taken.x < window_size.x && taken.y < window_size.y {
        camera.viewport = Some(Viewport {
            physical_position: position,
            physical_size: window_size - taken,
            ..default()
        });
    }
    Ok(())
}
