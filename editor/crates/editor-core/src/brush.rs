//! Shared capsule/falloff and world transform kernel for height and authored layers.
use crate::*;
use glam::{Quat, Vec3};

pub(crate) fn validate_path(
    object: &ObjectId,
    path: &[[f32; 3]],
    radius: f32,
    strength: f32,
    falloff: f32,
) -> Result<(), Diagnostic> {
    if path.is_empty()
        || path.len() > MAX_SCULPT_POINTS
        || !path.iter().flatten().all(|v| v.is_finite())
        || !radius.is_finite()
        || radius <= 0.0
        || radius > MAX_SCULPT_RADIUS_M
        || !strength.is_finite()
        || !(0.0..=1.0).contains(&strength)
        || !falloff.is_finite()
        || !(0.0..=1.0).contains(&falloff)
    {
        return Err(Diagnostic::new("terrain.brush_invalid").at(object));
    }
    Ok(())
}
pub(crate) fn local_path(
    scene: &SceneDocument,
    object: &ObjectId,
    path: &[[f32; 3]],
) -> Result<Vec<[f32; 3]>, Diagnostic> {
    let worlds = scene_data::world_transforms(scene)?;
    let t = worlds
        .get(object)
        .ok_or_else(|| Diagnostic::new("object.missing").at(object))?;
    let rotation = Quat::from_array(t.rotation_xyzw).conjugate();
    let translation = Vec3::from_array(t.translation_m);
    let points: Vec<_> = path
        .iter()
        .map(|p| (rotation * (Vec3::from_array(*p) - translation)).to_array())
        .collect();
    if points.iter().flatten().any(|v| !v.is_finite()) {
        return Err(Diagnostic::new("terrain.brush_invalid").at(object));
    }
    Ok(points)
}
pub(crate) fn influence(
    start: [f32; 3],
    end: [f32; 3],
    sample: [f64; 2],
    radius: f64,
    falloff: f32,
    strength: f32,
) -> f32 {
    let dx = f64::from(end[0]) - f64::from(start[0]);
    let dz = f64::from(end[2]) - f64::from(start[2]);
    let length_squared = dx * dx + dz * dz;
    let [sx, sz] = sample;
    let projection = if length_squared > 0.0 {
        (((sx - f64::from(start[0])) * dx + (sz - f64::from(start[2])) * dz) / length_squared)
            .clamp(0.0, 1.0)
    } else {
        0.0
    };
    let ex = f64::from(start[0]) + projection * dx;
    let ez = f64::from(start[2]) + projection * dz;
    let distance_squared = (sx - ex).powi(2) + (sz - ez).powi(2);
    if distance_squared >= radius * radius {
        return 0.0;
    }
    let inner = radius * (1.0 - f64::from(falloff));
    let t = if radius == inner {
        0.0
    } else {
        ((distance_squared.sqrt() - inner) / (radius - inner)).clamp(0.0, 1.0)
    };
    (f64::from(strength) * (1.0 - t * t)) as f32
}
