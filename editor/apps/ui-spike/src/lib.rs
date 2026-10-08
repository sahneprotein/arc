//! What both shells share: the workload, the 3D scene and the measurement.
//!
//! Run a shell with `load` (default) or `idle` as its first argument. It opens one window,
//! measures, prints one line of JSON that starts with `SPIKE_RESULT` and exits by itself.

use std::time::{Duration, Instant};

use bevy::{
    math::ops,
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
    window::PrimaryWindow,
};

/// Rows the hierarchy holds. Only the visible ones are ever built.
pub const HIERARCHY_ROWS: usize = 5000;
/// Rows that fit the hierarchy panel.
pub const VISIBLE_ROWS: usize = 34;
/// Number fields in the inspector.
pub const INSPECTOR_FIELDS: usize = 60;
pub const ROW_HEIGHT: f32 = 24.0;
pub const HIERARCHY_WIDTH: f32 = 260.0;
pub const INSPECTOR_WIDTH: f32 = 320.0;
pub const TOOLBAR_HEIGHT: f32 = 36.0;
pub const STATUS_HEIGHT: f32 = 22.0;
pub const WINDOW_SIZE: (u32, u32) = (1440, 900);

const WARMUP_FRAMES: u64 = 120;
const MEASURED_FRAMES: usize = 600;
const IDLE_WARMUP: Duration = Duration::from_secs(3);
const IDLE_WINDOW: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Everything changes every frame: the hierarchy scrolls, three fields and one cube move.
    Load,
    /// Nothing changes. Counts the frames and the processor time the shell still spends.
    Idle,
}

impl Mode {
    pub fn from_args() -> Self {
        match std::env::args().nth(1).as_deref() {
            Some("idle") => Mode::Idle,
            _ => Mode::Load,
        }
    }
}

/// What the workload shows this frame.
#[derive(Resource, Default)]
pub struct Stress {
    /// Index of the first visible hierarchy row.
    pub first_row: usize,
    /// The value of the three live inspector fields.
    pub value: Vec3,
}

#[derive(Resource)]
pub struct Measurement {
    pub toolkit: &'static str,
    pub mode: Mode,
    started: Instant,
    frames: u64,
    first_frame: Option<Duration>,
    samples: Vec<f32>,
    idle_from: Option<(Instant, u64, f64)>,
}

impl Measurement {
    pub fn new(toolkit: &'static str, mode: Mode, started: Instant) -> Self {
        Self {
            toolkit,
            mode,
            started,
            frames: 0,
            first_frame: None,
            samples: Vec::with_capacity(MEASURED_FRAMES),
            idle_from: None,
        }
    }
}

/// The cube the workload moves.
#[derive(Component)]
pub struct Moving;

pub fn row_label(index: usize) -> String {
    let depth = index % 4;
    format!("{:width$}entity_{index:04}   box", "", width = depth * 2)
}

/// Ground, a field of cubes and a light. The camera is the shell's.
pub fn spawn_world(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let grey = materials.add(Color::srgb(0.55, 0.55, 0.58));
    let accent = materials.add(Color::srgb(0.13, 0.43, 0.79));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(60.0, 60.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.25, 0.27, 0.25))),
    ));
    for x in 0..8 {
        for z in 0..8 {
            commands.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(grey.clone()),
                Transform::from_xyz(x as f32 * 3.0 - 10.5, 0.5, z as f32 * 3.0 - 10.5),
            ));
        }
    }
    commands.spawn((
        Mesh3d(cube),
        MeshMaterial3d(accent),
        Transform::from_xyz(0.0, 2.0, 0.0),
        Moving,
    ));
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.4, 0.0)),
    ));
}

pub fn camera_transform() -> Transform {
    Transform::from_xyz(0.0, 14.0, 22.0).looking_at(Vec3::ZERO, Vec3::Y)
}

/// Advances the workload. Does nothing in idle mode, so that nothing on screen changes.
pub fn advance_stress(
    measurement: Res<Measurement>,
    time: Res<Time>,
    mut stress: ResMut<Stress>,
    mut moving: Query<&mut Transform, With<Moving>>,
) {
    if measurement.mode != Mode::Load {
        return;
    }
    let t = time.elapsed_secs();
    stress.first_row = (stress.first_row + 1) % (HIERARCHY_ROWS - VISIBLE_ROWS);
    stress.value = Vec3::new(
        ops::sin(t) * 8.0,
        2.0 + ops::sin(t * 0.7),
        ops::cos(t) * 8.0,
    );
    for mut transform in &mut moving {
        transform.translation = stress.value;
    }
}

/// Saves one picture of the window to the path in `SPIKE_SCREENSHOT`, at frame 100, so that
/// what was measured can be looked at.
pub fn screenshot(mut commands: Commands, measurement: Res<Measurement>) {
    if measurement.frames != 100 {
        return;
    }
    if let Some(path) = std::env::var_os("SPIKE_SCREENSHOT") {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(std::path::PathBuf::from(path)));
    }
}

pub fn measure(
    mut measurement: ResMut<Measurement>,
    time: Res<Time<Real>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
) {
    measurement.frames += 1;
    if measurement.frames == 3 {
        measurement.first_frame = Some(measurement.started.elapsed());
    }
    match measurement.mode {
        Mode::Load => {
            if measurement.frames > WARMUP_FRAMES {
                measurement.samples.push(time.delta_secs() * 1000.0);
            }
            if measurement.samples.len() >= MEASURED_FRAMES {
                let mut sorted = measurement.samples.clone();
                sorted.sort_by(f32::total_cmp);
                let at = |q: f32| sorted[((sorted.len() - 1) as f32 * q) as usize];
                report(
                    &measurement,
                    &window,
                    format!(
                        "\"frames\":{},\"frame_ms_median\":{:.3},\"frame_ms_p95\":{:.3},\"frame_ms_p99\":{:.3},\"frame_ms_max\":{:.3}",
                        sorted.len(),
                        at(0.5),
                        at(0.95),
                        at(0.99),
                        sorted[sorted.len() - 1]
                    ),
                );
                exit.write(AppExit::Success);
            }
        }
        Mode::Idle => {
            let elapsed = measurement.started.elapsed();
            match measurement.idle_from {
                None if elapsed >= IDLE_WARMUP => {
                    measurement.idle_from =
                        Some((Instant::now(), measurement.frames, cpu_seconds()));
                }
                Some((from, frames, cpu)) if from.elapsed() >= IDLE_WINDOW => {
                    let seconds = from.elapsed().as_secs_f64();
                    report(
                        &measurement,
                        &window,
                        format!(
                            "\"idle_seconds\":{:.2},\"idle_frames\":{},\"idle_cpu_seconds\":{:.3},\"idle_cpu_percent_of_one_core\":{:.1}",
                            seconds,
                            measurement.frames - frames,
                            cpu_seconds() - cpu,
                            (cpu_seconds() - cpu) / seconds * 100.0
                        ),
                    );
                    exit.write(AppExit::Success);
                }
                _ => {}
            }
        }
    }
}

fn report(measurement: &Measurement, window: &Window, body: String) {
    println!(
        "SPIKE_RESULT {{\"toolkit\":\"{}\",\"mode\":\"{:?}\",\"first_frame_ms\":{:.0},\"window\":\"{}x{}\",\"scale_factor\":{:.2},\"focused\":{},\"rss_mb\":{:.0},{}}}",
        measurement.toolkit,
        measurement.mode,
        measurement
            .first_frame
            .map_or(-1.0, |d| d.as_secs_f64() * 1000.0),
        window.physical_width(),
        window.physical_height(),
        window.scale_factor(),
        window.focused,
        rss_megabytes(),
        body
    );
}

/// Processor time this process has used, user and system, in seconds. Linux only; 0 elsewhere.
fn cpu_seconds() -> f64 {
    let Ok(stat) = std::fs::read_to_string("/proc/self/stat") else {
        return 0.0;
    };
    // The fields after the command name, which is in parentheses and may hold spaces.
    let Some(rest) = stat.rsplit_once(')').map(|(_, rest)| rest) else {
        return 0.0;
    };
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // utime and stime are fields 14 and 15 of the line, 12 and 13 after the name (index 11, 12).
    let ticks: f64 = [11, 12]
        .iter()
        .filter_map(|&i| fields.get(i)?.parse::<f64>().ok())
        .sum();
    ticks / 100.0
}

/// Resident memory in megabytes. Linux only; 0 elsewhere.
fn rss_megabytes() -> f64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0.0;
    };
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|rest| rest.split_whitespace().next()?.parse::<f64>().ok())
        .map_or(0.0, |kb| kb / 1024.0)
}
