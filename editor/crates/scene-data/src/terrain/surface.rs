//! Single terrain cell diagonal and global-sample world placement convention.
use super::SampleKey;
use crate::RigidTransform;
use glam::{Quat, Vec3};

/// Row-major tile vertex indices, winding toward terrain up; diagonal p10--p01.
/// Invalid/unbounded cell arguments return None instead of overflowing indices.
pub fn cell_triangles(cells: u32, x: u32, z: u32) -> Option<[[u32; 3]; 2]> {
    if !(1..=1024).contains(&cells) || x >= cells || z >= cells {
        return None;
    }
    let p00 = z * (cells + 1) + x;
    let p10 = p00 + cells + 1;
    let p01 = p00 + 1;
    let p11 = p10 + 1;
    Some([[p00, p10, p01], [p10, p11, p01]])
}

/// Exact f32 expression used by collision cook and placement queries.
/// A global SampleKey, never tile-local arithmetic, determines each shared position.
/// Caller validates source/transform and rejects nonfinite results.
pub fn sample_world_position(
    key: SampleKey,
    height_m: f32,
    spacing_xz_m: [f32; 2],
    transform: &RigidTransform,
) -> [f32; 3] {
    (Quat::from_array(transform.rotation_xyzw)
        * Vec3::new(
            key.x as f32 * spacing_xz_m[0],
            height_m,
            key.z as f32 * spacing_xz_m[1],
        )
        + Vec3::from_array(transform.translation_m))
    .to_array()
}
