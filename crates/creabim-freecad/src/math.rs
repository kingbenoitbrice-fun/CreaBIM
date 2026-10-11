//! Minimal f64 vector and rigid-transform helpers.
//!
//! The crate keeps its own tiny math types so that it stays independent of
//! the rest of the workspace and of any particular linear-algebra crate.

use std::ops::{Add, Mul, Neg, Sub};

/// A 3D point or vector.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// A 2D point in a surface's parameter space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V2 {
    pub x: f64,
    pub y: f64,
}

pub const fn v3(x: f64, y: f64, z: f64) -> V3 {
    V3 { x, y, z }
}

pub const fn v2(x: f64, y: f64) -> V2 {
    V2 { x, y }
}

impl V3 {
    pub fn dot(self, o: V3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: V3) -> V3 {
        v3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
    /// Unit vector, or zero when the input has no usable direction.
    pub fn normalized(self) -> V3 {
        let l = self.length();
        if l > 1e-300 && l.is_finite() {
            self * (1.0 / l)
        } else {
            V3::default()
        }
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
    pub fn to_array(self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }
    pub fn from_array(a: [f64; 3]) -> V3 {
        v3(a[0], a[1], a[2])
    }
    /// Any unit vector perpendicular to `self` (assumed non-zero).
    pub fn any_perpendicular(self) -> V3 {
        let a = if self.x.abs() < 0.9 {
            v3(1.0, 0.0, 0.0)
        } else {
            v3(0.0, 1.0, 0.0)
        };
        self.cross(a).normalized()
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f64> for V3 {
    type Output = V3;
    fn mul(self, s: f64) -> V3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}
impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
}

impl V2 {
    pub fn dot(self, o: V2) -> f64 {
        self.x * o.x + self.y * o.y
    }
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
}
impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        v2(self.x + o.x, self.y + o.y)
    }
}
impl Sub for V2 {
    type Output = V2;
    fn sub(self, o: V2) -> V2 {
        v2(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for V2 {
    type Output = V2;
    fn mul(self, s: f64) -> V2 {
        v2(self.x * s, self.y * s)
    }
}

/// 2D cross product (z of the 3D cross product).
pub fn cross2(a: V2, b: V2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// An affine transform stored like OpenCascade's `gp_Trsf`: three rows of
/// `[r0 r1 r2 t]`. Locations in BREP files are written in exactly this form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trsf {
    pub m: [[f64; 4]; 3],
}

impl Default for Trsf {
    fn default() -> Self {
        Trsf::IDENTITY
    }
}

impl Trsf {
    pub const IDENTITY: Trsf = Trsf {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ],
    };

    /// `self * o`: applies `o` first, then `self` (OpenCascade convention).
    pub fn then_after(&self, o: &Trsf) -> Trsf {
        let a = &self.m;
        let b = &o.m;
        let mut r = [[0.0; 4]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                let mut s = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
                if j == 3 {
                    s += a[i][3];
                }
                *cell = s;
            }
        }
        Trsf { m: r }
    }

    pub fn point(&self, p: V3) -> V3 {
        let m = &self.m;
        v3(
            m[0][0] * p.x + m[0][1] * p.y + m[0][2] * p.z + m[0][3],
            m[1][0] * p.x + m[1][1] * p.y + m[1][2] * p.z + m[1][3],
            m[2][0] * p.x + m[2][1] * p.y + m[2][2] * p.z + m[2][3],
        )
    }

    pub fn vector(&self, p: V3) -> V3 {
        let m = &self.m;
        v3(
            m[0][0] * p.x + m[0][1] * p.y + m[0][2] * p.z,
            m[1][0] * p.x + m[1][1] * p.y + m[1][2] * p.z,
            m[2][0] * p.x + m[2][1] * p.y + m[2][2] * p.z,
        )
    }

    fn det3(&self) -> f64 {
        let m = &self.m;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    /// Inverse, or `None` for a singular matrix.
    pub fn inverse(&self) -> Option<Trsf> {
        let d = self.det3();
        if !d.is_finite() || d.abs() < 1e-300 {
            return None;
        }
        let m = &self.m;
        let inv_d = 1.0 / d;
        let mut r = [[0.0; 4]; 3];
        r[0][0] = (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv_d;
        r[0][1] = (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv_d;
        r[0][2] = (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv_d;
        r[1][0] = (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv_d;
        r[1][1] = (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv_d;
        r[1][2] = (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv_d;
        r[2][0] = (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv_d;
        r[2][1] = (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv_d;
        r[2][2] = (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv_d;
        let t = v3(m[0][3], m[1][3], m[2][3]);
        for row in r.iter_mut() {
            row[3] = -(row[0] * t.x + row[1] * t.y + row[2] * t.z);
        }
        Some(Trsf { m: r })
    }

    /// `self^n` as used by composite BREP locations (negative = inverse).
    pub fn powered(&self, n: i64) -> Option<Trsf> {
        let base = if n < 0 { self.inverse()? } else { *self };
        let mut r = Trsf::IDENTITY;
        // Real files only use +-1; cap to keep hostile input cheap.
        for _ in 0..n.unsigned_abs().min(64) {
            r = r.then_after(&base);
        }
        Some(r)
    }

    /// True when the 3x3 part flips handedness (mirror).
    pub fn is_mirror(&self) -> bool {
        self.det3() < 0.0
    }

    /// Column-major 4x4 matrix.
    pub fn to_cols(self) -> [[f64; 4]; 4] {
        let m = &self.m;
        [
            [m[0][0], m[1][0], m[2][0], 0.0],
            [m[0][1], m[1][1], m[2][1], 0.0],
            [m[0][2], m[1][2], m[2][2], 0.0],
            [m[0][3], m[1][3], m[2][3], 1.0],
        ]
    }

    /// From a column-major 4x4 matrix (the projective row is ignored).
    pub fn from_cols(c: &[[f64; 4]; 4]) -> Trsf {
        Trsf {
            m: [
                [c[0][0], c[1][0], c[2][0], c[3][0]],
                [c[0][1], c[1][1], c[2][1], c[3][1]],
                [c[0][2], c[1][2], c[2][2], c[3][2]],
            ],
        }
    }

    /// Rotation from a unit quaternion `(x, y, z, w)` plus a translation,
    /// which is how FreeCAD stores `App::PropertyPlacement`.
    pub fn from_quat(t: V3, q: [f64; 4]) -> Trsf {
        let [x, y, z, w] = q;
        let n = (x * x + y * y + z * z + w * w).sqrt();
        let (x, y, z, w) = if n > 1e-300 && n.is_finite() {
            (x / n, y / n, z / n, w / n)
        } else {
            (0.0, 0.0, 0.0, 1.0)
        };
        Trsf {
            m: [
                [
                    1.0 - 2.0 * (y * y + z * z),
                    2.0 * (x * y - z * w),
                    2.0 * (x * z + y * w),
                    t.x,
                ],
                [
                    2.0 * (x * y + z * w),
                    1.0 - 2.0 * (x * x + z * z),
                    2.0 * (y * z - x * w),
                    t.y,
                ],
                [
                    2.0 * (x * z - y * w),
                    2.0 * (y * z + x * w),
                    1.0 - 2.0 * (x * x + y * y),
                    t.z,
                ],
            ],
        }
    }

    /// Splits into translation and unit quaternion `(x, y, z, w)` when the
    /// 3x3 part is a proper rotation (orthonormal, det = +1).
    pub fn to_quat(self) -> Option<(V3, [f64; 4])> {
        let m = &self.m;
        let c0 = v3(m[0][0], m[1][0], m[2][0]);
        let c1 = v3(m[0][1], m[1][1], m[2][1]);
        let c2 = v3(m[0][2], m[1][2], m[2][2]);
        let ortho = (c0.length() - 1.0).abs() < 1e-6
            && (c1.length() - 1.0).abs() < 1e-6
            && (c2.length() - 1.0).abs() < 1e-6
            && c0.dot(c1).abs() < 1e-6
            && c0.dot(c2).abs() < 1e-6
            && c1.dot(c2).abs() < 1e-6
            && self.det3() > 0.0;
        if !ortho {
            return None;
        }
        let tr = m[0][0] + m[1][1] + m[2][2];
        let q = if tr > 0.0 {
            let s = (tr + 1.0).sqrt() * 2.0;
            [
                (m[2][1] - m[1][2]) / s,
                (m[0][2] - m[2][0]) / s,
                (m[1][0] - m[0][1]) / s,
                0.25 * s,
            ]
        } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
            let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
            [
                0.25 * s,
                (m[0][1] + m[1][0]) / s,
                (m[0][2] + m[2][0]) / s,
                (m[2][1] - m[1][2]) / s,
            ]
        } else if m[1][1] > m[2][2] {
            let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
            [
                (m[0][1] + m[1][0]) / s,
                0.25 * s,
                (m[1][2] + m[2][1]) / s,
                (m[0][2] - m[2][0]) / s,
            ]
        } else {
            let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
            [
                (m[0][2] + m[2][0]) / s,
                (m[1][2] + m[2][1]) / s,
                0.25 * s,
                (m[1][0] - m[0][1]) / s,
            ]
        };
        let n = q.iter().map(|v| v * v).sum::<f64>().sqrt();
        let mut q = q.map(|v| v / n);
        // FreeCAD writes the quaternion with w >= 0; keep files stable.
        if q[3] < 0.0 {
            q = q.map(|v| -v);
        }
        Some((v3(m[0][3], m[1][3], m[2][3]), q))
    }

    pub fn approx_eq(&self, o: &Trsf, tol: f64) -> bool {
        self.m
            .iter()
            .flatten()
            .zip(o.m.iter().flatten())
            .all(|(a, b)| (a - b).abs() <= tol * (1.0 + a.abs().max(b.abs())))
    }

    pub fn is_finite(&self) -> bool {
        self.m.iter().flatten().all(|v| v.is_finite())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quat_round_trip() {
        let q = [
            0.0,
            0.0,
            (15f64).to_radians().sin(),
            (15f64).to_radians().cos(),
        ];
        let t = Trsf::from_quat(v3(1.0, 2.0, 3.0), q);
        let (p, q2) = t.to_quat().unwrap();
        assert_eq!(p, v3(1.0, 2.0, 3.0));
        for (a, b) in q.iter().zip(q2) {
            assert!((a - b).abs() < 1e-12);
        }
        let inv = t.inverse().unwrap();
        assert!(inv.then_after(&t).approx_eq(&Trsf::IDENTITY, 1e-12));
    }
}
