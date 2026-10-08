use glam::DVec3;

/// Dominant-axis shearing keeps shared edge expressions identical (opposite signs)
/// for adjacent triangles. Closed edges prevent diagonal/seam cracks; caller resolves ties.
pub(super) fn hit(
    origin: DVec3,
    direction: DVec3,
    vertices: [DVec3; 3],
    limit: f64,
) -> Option<f64> {
    // Closed world AABB: cheap rejection without inverse-transforming rounded vertices.
    let mut near: f64 = 0.0;
    let mut far = limit;
    for axis in 0..3 {
        let lo = vertices
            .iter()
            .map(|p| p[axis])
            .fold(f64::INFINITY, f64::min);
        let hi = vertices
            .iter()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        if direction[axis] == 0.0 {
            if origin[axis] < lo || origin[axis] > hi {
                return None;
            }
        } else {
            let a = (lo - origin[axis]) / direction[axis];
            let b = (hi - origin[axis]) / direction[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    let abs = direction.abs();
    let kz = if abs.x > abs.y {
        if abs.x > abs.z {
            0
        } else {
            2
        }
    } else if abs.y > abs.z {
        1
    } else {
        2
    };
    let mut kx = (kz + 1) % 3;
    let mut ky = (kx + 1) % 3;
    if direction[kz] < 0.0 {
        std::mem::swap(&mut kx, &mut ky);
    }
    let sx = direction[kx] / direction[kz];
    let sy = direction[ky] / direction[kz];
    let sz = 1.0 / direction[kz];
    let points = vertices.map(|p| {
        let p = p - origin;
        [p[kx] - sx * p[kz], p[ky] - sy * p[kz], sz * p[kz]]
    });
    let [a, b, c] = points;
    let u = c[0] * b[1] - c[1] * b[0];
    let v = a[0] * c[1] - a[1] * c[0];
    let w = b[0] * a[1] - b[1] * a[0];
    if (u < 0.0 || v < 0.0 || w < 0.0) && (u > 0.0 || v > 0.0 || w > 0.0) {
        return None;
    }
    let determinant = u + v + w;
    if determinant == 0.0 {
        return None;
    }
    // Boundary distances use only the shared vertex/edge, so different incident
    // triangle areas cannot round an identical hit into different distances.
    let distance = if v == 0.0 && w == 0.0 {
        a[2]
    } else if u == 0.0 && w == 0.0 {
        b[2]
    } else if u == 0.0 && v == 0.0 {
        c[2]
    } else if u == 0.0 {
        edge_depth(b, c)
    } else if v == 0.0 {
        edge_depth(a, c)
    } else if w == 0.0 {
        edge_depth(a, b)
    } else {
        (u * a[2] + v * b[2] + w * c[2]) / determinant
    };
    (distance.is_finite() && distance > 0.0 && distance <= limit).then_some(distance)
}

fn edge_depth(mut a: [f64; 3], mut b: [f64; 3]) -> f64 {
    for axis in 0..3 {
        match a[axis].total_cmp(&b[axis]) {
            std::cmp::Ordering::Greater => {
                std::mem::swap(&mut a, &mut b);
                break;
            }
            std::cmp::Ordering::Less => break,
            std::cmp::Ordering::Equal => {}
        }
    }
    let axis = usize::from((b[1] - a[1]).abs() > (b[0] - a[0]).abs());
    let alpha = -a[axis] / (b[axis] - a[axis]);
    a[2] + (b[2] - a[2]) * alpha
}
