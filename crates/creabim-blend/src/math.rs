//! Column-major 4x4 matrices, as Blender stores them (`m[col][row]`).

pub(crate) type Mat4 = [[f64; 4]; 4];

pub(crate) const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

pub(crate) type Mat3 = [[f64; 3]; 3];

const IDENTITY3: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

pub(crate) fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for (c, col) in out.iter_mut().enumerate() {
        for (r, v) in col.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    out
}

fn mul3(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (c, col) in out.iter_mut().enumerate() {
        for (r, v) in col.iter_mut().enumerate() {
            *v = (0..3).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    out
}

/// A 16-float field (`float m[4][4]`) as a matrix.
pub(crate) fn from_floats(v: &[f64]) -> Option<Mat4> {
    if v.len() != 16 || v.iter().any(|x| !x.is_finite()) {
        return None;
    }
    let mut m = [[0.0; 4]; 4];
    for (i, x) in v.iter().enumerate() {
        m[i / 4][i % 4] = *x;
    }
    Some(m)
}

fn axis_rot(axis: usize, angle: f64) -> Mat3 {
    let (s, c) = angle.sin_cos();
    let (i, j) = ((axis + 1) % 3, (axis + 2) % 3);
    let mut m = IDENTITY3;
    m[i][i] = c;
    m[i][j] = s;
    m[j][i] = -s;
    m[j][j] = c;
    m
}

/// Euler rotation for Blender's `rotmode` 1..=6 (XYZ, XZY, YXZ, YZX,
/// ZXY, ZYX); the first named axis is applied first.
pub(crate) fn euler(e: [f64; 3], rotmode: i64) -> Mat3 {
    let order: [usize; 3] = match rotmode {
        2 => [0, 2, 1],
        3 => [1, 0, 2],
        4 => [1, 2, 0],
        5 => [2, 0, 1],
        6 => [2, 1, 0],
        _ => [0, 1, 2],
    };
    order
        .iter()
        .fold(IDENTITY3, |m, &a| mul3(&axis_rot(a, e[a]), &m))
}

/// Unit quaternion `[w, x, y, z]` to a rotation matrix.
pub(crate) fn quat(q: [f64; 4]) -> Mat3 {
    let n = q.iter().map(|x| x * x).sum::<f64>().sqrt();
    if n < 1e-12 || !n.is_finite() {
        return IDENTITY3;
    }
    let [w, x, y, z] = q.map(|v| v / n);
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + w * z),
            2.0 * (x * z - w * y),
        ],
        [
            2.0 * (x * y - w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + w * x),
        ],
        [
            2.0 * (x * z + w * y),
            2.0 * (y * z - w * x),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

pub(crate) fn axis_angle(axis: [f64; 3], angle: f64) -> Mat3 {
    let n = axis.iter().map(|x| x * x).sum::<f64>().sqrt();
    if n < 1e-12 || !n.is_finite() {
        return IDENTITY3;
    }
    let (s, c) = (angle / 2.0).sin_cos();
    quat([c, axis[0] / n * s, axis[1] / n * s, axis[2] / n * s])
}

/// Location, rotation and per-axis scale combined as `T * R * S`.
pub(crate) fn compose(loc: [f64; 3], rot: &Mat3, scale: [f64; 3]) -> Mat4 {
    let mut m = IDENTITY;
    for c in 0..3 {
        for r in 0..3 {
            m[c][r] = rot[c][r] * scale[c];
        }
    }
    m[3] = [loc[0], loc[1], loc[2], 1.0];
    m
}

pub(crate) fn rot_mul(a: &Mat3, b: &Mat3) -> Mat3 {
    mul3(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euler_xyz_applies_x_first() {
        let half = std::f64::consts::FRAC_PI_2;
        // X then Z by 90 degrees sends +Y to +Z, then (Z about Z) stays +Z.
        let m = euler([half, 0.0, half], 1);
        let y = [m[1][0], m[1][1], m[1][2]];
        assert!((y[2] - 1.0).abs() < 1e-12, "{y:?}");
        // And +X goes to +Y.
        assert!((m[0][1] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn quat_matches_axis_angle() {
        let a = axis_angle([0.0, 0.0, 1.0], 0.7);
        let e = euler([0.0, 0.0, 0.7], 1);
        for c in 0..3 {
            for r in 0..3 {
                assert!((a[c][r] - e[c][r]).abs() < 1e-12);
            }
        }
    }
}
