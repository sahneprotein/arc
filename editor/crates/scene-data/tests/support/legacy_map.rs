// Frozen arc-game shared/src/level.rs, SHA256 5bfbe0aae57dfb618ab48420bcc425b940c34d97db3524240cead4ed526feb93
// Source-extracted reference; no dependency on the current-map converter.
use glam::{Quat, Vec3};
/// One static box: `half_extents` along the box's own axes, rotated by `rotation`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StaticBox {
    pub center: Vec3,
    pub half_extents: Vec3,
    pub rotation: Quat,
}

impl StaticBox {
    pub fn new(center: Vec3, half_extents: Vec3) -> Self {
        Self {
            center,
            half_extents,
            rotation: Quat::IDENTITY,
        }
    }
}

/// Where the ramp starts (foot, on the ground), its slope and size.
pub const RAMP_FOOT_X: f32 = 5.0;
pub const RAMP_ANGLE_DEG: f32 = 20.0;
pub const RAMP_LENGTH: f32 = 8.0;
/// First stair face is at this `z`; steps are `STEP_RISE` high and `STEP_RUN` deep.
pub const STAIRS_START_Z: f32 = 5.0;
pub const STEP_RISE: f32 = 0.15;
pub const STEP_RUN: f32 = 0.4;
pub const STEP_COUNT: usize = 8;
/// The wall face (normal +X) is at this `x`.
pub const WALL_FACE_X: f32 = -8.0;

/// The level: ground slab, a ramp rising along +X, a staircase rising along +Z,
/// a wall at `x = -8` and a few boxes.
pub fn test_level() -> Vec<StaticBox> {
    let mut boxes = Vec::new();

    // Ground slab, top surface at y = 0.
    boxes.push(StaticBox::new(
        Vec3::new(0.0, -0.5, 0.0),
        Vec3::new(60.0, 0.5, 60.0),
    ));

    // Ramp: a thin box tilted about Z. The top surface starts at (RAMP_FOOT_X, 0).
    let theta = RAMP_ANGLE_DEG.to_radians();
    let half_len = RAMP_LENGTH * 0.5;
    let half_thick = 0.25;
    let dir = Vec3::new(theta.cos(), theta.sin(), 0.0);
    let normal = Vec3::new(-theta.sin(), theta.cos(), 0.0);
    let top_center = Vec3::new(RAMP_FOOT_X, 0.0, 0.0) + dir * half_len;
    boxes.push(StaticBox {
        center: top_center - normal * half_thick,
        half_extents: Vec3::new(half_len, half_thick, 3.0),
        rotation: Quat::from_rotation_z(theta),
    });

    // Staircase: each step is a solid box from the ground up to its top.
    // The last step is deep so there is a landing to stand on.
    for i in 0..STEP_COUNT {
        let top = STEP_RISE * (i as f32 + 1.0);
        let run = if i + 1 == STEP_COUNT { 4.0 } else { STEP_RUN };
        let z0 = STAIRS_START_Z + STEP_RUN * i as f32;
        boxes.push(StaticBox::new(
            Vec3::new(0.0, top * 0.5, z0 + run * 0.5),
            Vec3::new(2.0, top * 0.5, run * 0.5),
        ));
    }

    // Wall: thick, tall, long, face at x = WALL_FACE_X.
    boxes.push(StaticBox::new(
        Vec3::new(WALL_FACE_X - 0.5, 3.0, 0.0),
        Vec3::new(0.5, 3.0, 20.0),
    ));

    // A few boxes: a tall one (blocks), a low one (steppable), a pillar.
    boxes.push(StaticBox::new(Vec3::new(0.0, 0.5, -8.0), Vec3::splat(0.5)));
    boxes.push(StaticBox::new(
        Vec3::new(3.0, 0.15, -8.0),
        Vec3::new(1.0, 0.15, 1.0),
    ));
    boxes.push(StaticBox::new(
        Vec3::new(-3.0, 1.5, -12.0),
        Vec3::new(0.5, 1.5, 0.5),
    ));

    boxes
}

/// Number of distinct spawn slots; `spawn_point` wraps after this many.
pub const SPAWN_POINT_COUNT: u64 = 8;

/// Where the character with join counter `index` spawns. There are [`SPAWN_POINT_COUNT`] slots,
/// 2 m apart on a line along x at `z = 16` (x from -7 to 7), 2 m above the ground so characters
/// drop onto it. `index` may grow without bound: it wraps, so index 8 is the same slot as 0.
///
/// The line is clear of the ramp (`x >= 5`, `|z| <= 3`), the stairs (`|x| <= 2`, `z <= 11.8`),
/// the wall (`x <= -8`) and the boxes (`z <= -11.5`).
pub fn spawn_point(index: u64) -> Vec3 {
    let slot = (index % SPAWN_POINT_COUNT) as f32;
    Vec3::new((slot - 3.5) * 2.0, 2.0, 16.0)
}
