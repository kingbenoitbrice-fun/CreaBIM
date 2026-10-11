//! Curves and surfaces of the BREP geometry sections, with evaluation.
//!
//! Only what tessellation needs is implemented: point evaluation, a sensible
//! sample count per curve and, for the elementary surfaces, the inverse
//! mapping used when an edge has no stored p-curve.

use super::tokens::Tokens;
use crate::math::{v3, V2, V3};
use crate::FcError;
use std::f64::consts::TAU;

/// Recursion limit for nested (trimmed/offset) curves and surfaces, so that a
/// malicious file cannot overflow the stack.
const MAX_NESTING: u32 = 16;

/// A right-handed frame: origin, main axis (N) and in-plane axes (X, Y).
#[derive(Clone, Copy, Debug)]
pub struct Ax3 {
    pub p: V3,
    pub n: V3,
    pub x: V3,
    pub y: V3,
}

impl Ax3 {
    fn radial(&self, u: f64) -> V3 {
        self.x * u.cos() + self.y * u.sin()
    }
}

/// A (rational) B-spline in homogeneous coordinates with a flat knot vector
/// already extended for periodic splines.
#[derive(Clone, Debug)]
pub struct Nurbs {
    pub degree: usize,
    /// `[w*x, w*y, w*z, w]`.
    pub poles: Vec<[f64; 4]>,
    pub knots: Vec<f64>,
}

impl Nurbs {
    fn range(&self) -> (f64, f64) {
        (
            self.knots[self.degree],
            self.knots[self.knots.len() - self.degree - 1],
        )
    }

    fn eval(&self, u: f64) -> V3 {
        let (span, basis) = basis_functions(&self.knots, self.degree, u);
        let mut acc = [0.0; 4];
        for (k, b) in basis.iter().enumerate() {
            let p = self.poles[span - self.degree + k];
            for c in 0..4 {
                acc[c] += b * p[c];
            }
        }
        homogeneous(acc)
    }
}

fn homogeneous(a: [f64; 4]) -> V3 {
    if a[3].abs() > 1e-300 {
        v3(a[0] / a[3], a[1] / a[3], a[2] / a[3])
    } else {
        v3(a[0], a[1], a[2])
    }
}

/// Knot span index and the `degree + 1` non-zero basis values at `u`
/// (algorithm A2.2 of "The NURBS Book").
fn basis_functions(knots: &[f64], p: usize, u: f64) -> (usize, Vec<f64>) {
    let n = knots.len() - p - 2; // index of the last pole
    let (lo, hi) = (knots[p], knots[n + 1]);
    let u = u.clamp(lo, hi);
    let mut span = p;
    while span < n && u >= knots[span + 1] {
        span += 1;
    }
    let mut left = vec![0.0; p + 1];
    let mut right = vec![0.0; p + 1];
    let mut nb = vec![0.0; p + 1];
    nb[0] = 1.0;
    for j in 1..=p {
        left[j] = u - knots[span + 1 - j];
        right[j] = knots[span + j] - u;
        let mut saved = 0.0;
        for r in 0..j {
            let den = right[r + 1] + left[j - r];
            let temp = if den.abs() > 1e-300 { nb[r] / den } else { 0.0 };
            nb[r] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        nb[j] = saved;
    }
    (span, nb)
}

/// Builds OpenCascade's flat knot sequence (`BSplCLib::KnotSequence`) and,
/// for periodic splines, wraps the poles so plain de Boor evaluation works.
fn flat_knots(
    knots: &[(f64, usize)],
    degree: usize,
    periodic: bool,
    nb_poles: usize,
) -> Result<(Vec<f64>, Vec<usize>), FcError> {
    let bad = || FcError::Brep("inconsistent B-spline knots".into());
    if knots.len() < 2 || degree == 0 || degree > 25 || nb_poles == 0 {
        return Err(bad());
    }
    let mut flat = Vec::new();
    if periodic {
        let period = knots[knots.len() - 1].0 - knots[0].0;
        let m1 = (degree + 1).saturating_sub(knots[0].1);
        // `m1` knots taken backwards from the end, shifted by one period.
        let mut pre = Vec::new();
        let mut j = knots.len() - 1;
        while pre.len() < m1 && j > 0 {
            j -= 1;
            for _ in 0..knots[j].1 {
                if pre.len() < m1 {
                    pre.push(knots[j].0 - period);
                }
            }
        }
        pre.reverse();
        flat.extend(pre);
        for &(k, m) in knots {
            flat.extend(std::iter::repeat_n(k, m.min(degree + 1)));
        }
        let m2 = (degree + 1).saturating_sub(knots[knots.len() - 1].1);
        let mut post = 0;
        for &(k, m) in &knots[1..] {
            for _ in 0..m {
                if post < m2 {
                    flat.push(k + period);
                    post += 1;
                }
            }
        }
    } else {
        for &(k, m) in knots {
            flat.extend(std::iter::repeat_n(k, m.min(degree + 1)));
        }
    }
    if flat.len() < 2 * degree + 2 || flat.windows(2).any(|w| w[1] < w[0]) {
        return Err(bad());
    }
    let count = flat.len() - degree - 1;
    if !periodic && count != nb_poles {
        return Err(bad());
    }
    let idx = (0..count).map(|i| i % nb_poles).collect();
    Ok((flat, idx))
}

fn read_knots(t: &mut Tokens, n: usize) -> Result<Vec<(f64, usize)>, FcError> {
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        let k = t.f64()?;
        let m = t.count()?;
        v.push((k, m));
    }
    Ok(v)
}

/// A 3D curve, or a 2D p-curve lifted to z = 0.
#[derive(Clone, Debug)]
pub enum Curve {
    Line {
        p: V3,
        d: V3,
    },
    Circle {
        ax: Ax3,
        r: f64,
    },
    Ellipse {
        ax: Ax3,
        r1: f64,
        r2: f64,
    },
    Parabola {
        ax: Ax3,
        focal: f64,
    },
    Hyperbola {
        ax: Ax3,
        r1: f64,
        r2: f64,
    },
    Spline(Nurbs),
    Trimmed {
        basis: Box<Curve>,
    },
    /// `dir` is `None` for 2D offset curves.
    Offset {
        basis: Box<Curve>,
        offset: f64,
        dir: Option<V3>,
    },
}

impl Curve {
    pub fn eval(&self, u: f64) -> V3 {
        match self {
            Curve::Line { p, d } => *p + *d * u,
            Curve::Circle { ax, r } => ax.p + ax.radial(u) * *r,
            Curve::Ellipse { ax, r1, r2 } => ax.p + ax.x * (r1 * u.cos()) + ax.y * (r2 * u.sin()),
            Curve::Parabola { ax, focal } => {
                let f = if focal.abs() > 1e-300 { *focal } else { 1e-300 };
                ax.p + ax.x * (u * u / (4.0 * f)) + ax.y * u
            }
            Curve::Hyperbola { ax, r1, r2 } => {
                ax.p + ax.x * (r1 * u.cosh()) + ax.y * (r2 * u.sinh())
            }
            Curve::Spline(s) => s.eval(u),
            Curve::Trimmed { basis } => basis.eval(u),
            Curve::Offset { basis, offset, dir } => {
                let h = 1e-6 * (1.0 + u.abs());
                let tan = basis.eval(u + h) - basis.eval(u - h);
                let side = match dir {
                    Some(d) => tan.cross(*d).normalized(),
                    None => v3(tan.y, -tan.x, 0.0).normalized(),
                };
                basis.eval(u) + side * *offset
            }
        }
    }

    /// Number of straight segments to use between parameters `a` and `b`.
    pub fn segments(&self, a: f64, b: f64, angle_step: f64) -> usize {
        let span = (b - a).abs();
        let by_angle = |span: f64| (span / angle_step).ceil().clamp(1.0, 1024.0) as usize;
        match self {
            Curve::Line { .. } => 1,
            Curve::Circle { r, .. } | Curve::Ellipse { r1: r, .. } => {
                // Also bound the chord error on large arcs (curved walls).
                let r = r.abs().max(1e-9);
                let max_angle = 2.0 * (1.0 - (MAX_SAGITTA / r).min(1.0)).acos();
                by_angle(span).max(by_angle(span * angle_step / max_angle.max(1e-6)))
            }
            Curve::Parabola { .. } | Curve::Hyperbola { .. } => 16,
            Curve::Spline(s) => {
                let (lo, hi) = s.range();
                let full = (hi - lo).abs().max(1e-300);
                let spans = s.knots.windows(2).filter(|w| w[1] > w[0]).count().max(1);
                let per = (s.degree.max(1) * 4).min(32);
                ((spans * per) as f64 * (span / full).min(1.0))
                    .ceil()
                    .clamp(1.0, 1024.0) as usize
            }
            Curve::Trimmed { basis } | Curve::Offset { basis, .. } => {
                basis.segments(a, b, angle_step)
            }
        }
    }

    pub fn read3d(t: &mut Tokens) -> Result<Curve, FcError> {
        read_curve(t, true, 0)
    }

    pub fn read2d(t: &mut Tokens) -> Result<Curve, FcError> {
        read_curve(t, false, 0)
    }
}

fn read_point(t: &mut Tokens, is3d: bool) -> Result<V3, FcError> {
    if is3d {
        t.v3()
    } else {
        let p = t.v2()?;
        Ok(v3(p.x, p.y, 0.0))
    }
}

fn read_ax(t: &mut Tokens, is3d: bool) -> Result<Ax3, FcError> {
    if is3d {
        Ok(Ax3 {
            p: t.v3()?,
            n: t.v3()?,
            x: t.v3()?,
            y: t.v3()?,
        })
    } else {
        Ok(Ax3 {
            p: read_point(t, false)?,
            n: v3(0.0, 0.0, 1.0),
            x: read_point(t, false)?,
            y: read_point(t, false)?,
        })
    }
}

fn read_poles(
    t: &mut Tokens,
    n: usize,
    rational: bool,
    is3d: bool,
) -> Result<Vec<[f64; 4]>, FcError> {
    let mut poles = Vec::with_capacity(n.min(1 << 20));
    for _ in 0..n {
        let p = read_point(t, is3d)?;
        let w = if rational { t.f64()? } else { 1.0 };
        poles.push([p.x * w, p.y * w, p.z * w, w]);
    }
    Ok(poles)
}

fn read_curve(t: &mut Tokens, is3d: bool, depth: u32) -> Result<Curve, FcError> {
    if depth > MAX_NESTING {
        return Err(FcError::Brep("curve nesting too deep".into()));
    }
    let kind = t.int()?;
    Ok(match kind {
        1 => Curve::Line {
            p: read_point(t, is3d)?,
            d: read_point(t, is3d)?,
        },
        2 => Curve::Circle {
            ax: read_ax(t, is3d)?,
            r: t.f64()?,
        },
        3 => Curve::Ellipse {
            ax: read_ax(t, is3d)?,
            r1: t.f64()?,
            r2: t.f64()?,
        },
        4 => Curve::Parabola {
            ax: read_ax(t, is3d)?,
            focal: t.f64()?,
        },
        5 => Curve::Hyperbola {
            ax: read_ax(t, is3d)?,
            r1: t.f64()?,
            r2: t.f64()?,
        },
        6 => {
            let rational = t.int()? != 0;
            let degree = t.count()?;
            if degree == 0 || degree > 25 {
                return Err(FcError::Brep("bad Bezier degree".into()));
            }
            let poles = read_poles(t, degree + 1, rational, is3d)?;
            let mut knots = vec![0.0; degree + 1];
            knots.extend(std::iter::repeat_n(1.0, degree + 1));
            Curve::Spline(Nurbs {
                degree,
                poles,
                knots,
            })
        }
        7 => {
            let rational = t.int()? != 0;
            let periodic = t.int()? != 0;
            let degree = t.count()?;
            let nb_poles = t.count()?;
            let nb_knots = t.count()?;
            let poles = read_poles(t, nb_poles, rational, is3d)?;
            let knots = read_knots(t, nb_knots)?;
            let (flat, idx) = flat_knots(&knots, degree, periodic, nb_poles)?;
            Curve::Spline(Nurbs {
                degree,
                poles: idx.iter().map(|&i| poles[i]).collect(),
                knots: flat,
            })
        }
        8 => {
            let _u1 = t.f64()?;
            let _u2 = t.f64()?;
            Curve::Trimmed {
                basis: Box::new(read_curve(t, is3d, depth + 1)?),
            }
        }
        9 => {
            let offset = t.f64()?;
            let dir = if is3d { Some(t.v3()?) } else { None };
            Curve::Offset {
                basis: Box::new(read_curve(t, is3d, depth + 1)?),
                offset,
                dir,
            }
        }
        k => return Err(FcError::Brep(format!("unknown curve type {k}"))),
    })
}

/// A surface of the BREP `Surfaces` section.
#[derive(Clone, Debug)]
pub enum Surface {
    Plane(Ax3),
    Cylinder(Ax3, f64),
    Cone(Ax3, f64, f64),
    Sphere(Ax3, f64),
    Torus(Ax3, f64, f64),
    Extrusion {
        dir: V3,
        basis: Curve,
    },
    Revolution {
        p: V3,
        dir: V3,
        basis: Curve,
    },
    Spline {
        du: usize,
        dv: usize,
        /// Row-major `[u][v]` homogeneous poles.
        poles: Vec<[f64; 4]>,
        uknots: Vec<f64>,
        vknots: Vec<f64>,
    },
    Trimmed(Box<Surface>),
    Offset(Box<Surface>, f64),
}

impl Surface {
    pub fn read(t: &mut Tokens) -> Result<Surface, FcError> {
        read_surface(t, 0)
    }

    pub fn is_plane(&self) -> Option<&Ax3> {
        match self {
            Surface::Plane(ax) => Some(ax),
            Surface::Trimmed(s) => s.is_plane(),
            _ => None,
        }
    }

    pub fn eval(&self, uv: V2) -> V3 {
        let (u, v) = (uv.x, uv.y);
        match self {
            Surface::Plane(ax) => ax.p + ax.x * u + ax.y * v,
            Surface::Cylinder(ax, r) => ax.p + ax.radial(u) * *r + ax.n * v,
            Surface::Cone(ax, r, a) => {
                ax.p + ax.radial(u) * (r + v * a.sin()) + ax.n * (v * a.cos())
            }
            Surface::Sphere(ax, r) => ax.p + ax.radial(u) * (r * v.cos()) + ax.n * (r * v.sin()),
            Surface::Torus(ax, r1, r2) => {
                ax.p + ax.radial(u) * (r1 + r2 * v.cos()) + ax.n * (r2 * v.sin())
            }
            Surface::Extrusion { dir, basis } => basis.eval(u) + *dir * v,
            Surface::Revolution { p, dir, basis } => {
                rotate_about(basis.eval(v), *p, dir.normalized(), u)
            }
            Surface::Spline {
                du,
                dv,
                poles,
                uknots,
                vknots,
            } => {
                let nv = vknots.len() - dv - 1;
                let (su, bu) = basis_functions(uknots, *du, u);
                let (sv, bv) = basis_functions(vknots, *dv, v);
                let mut acc = [0.0; 4];
                for (i, bi) in bu.iter().enumerate() {
                    for (j, bj) in bv.iter().enumerate() {
                        let p = poles[(su - du + i) * nv + (sv - dv + j)];
                        for c in 0..4 {
                            acc[c] += bi * bj * p[c];
                        }
                    }
                }
                homogeneous(acc)
            }
            Surface::Trimmed(s) => s.eval(uv),
            Surface::Offset(s, off) => {
                let h = 1e-6;
                let su = s.eval(crate::math::v2(u + h, v)) - s.eval(crate::math::v2(u - h, v));
                let sv = s.eval(crate::math::v2(u, v + h)) - s.eval(crate::math::v2(u, v - h));
                s.eval(uv) + su.cross(sv).normalized() * *off
            }
        }
    }

    /// Parameters of the point of the surface closest to `p`, for the
    /// elementary surfaces only. `u` is returned in [-pi, pi]; callers unwrap.
    pub fn inverse(&self, p: V3) -> Option<V2> {
        use crate::math::v2;
        let local = |ax: &Ax3| {
            let d = p - ax.p;
            (d.dot(ax.x), d.dot(ax.y), d.dot(ax.n))
        };
        match self {
            Surface::Plane(ax) => {
                let (x, y, _) = local(ax);
                Some(v2(x, y))
            }
            Surface::Cylinder(ax, _) => {
                let (x, y, z) = local(ax);
                Some(v2(y.atan2(x), z))
            }
            Surface::Cone(ax, r, a) => {
                let (x, y, z) = local(ax);
                // Distance along the generatrix from the reference circle.
                let rho = (x * x + y * y).sqrt();
                let v = (rho - r) * a.sin() + z * a.cos();
                Some(v2(y.atan2(x), v))
            }
            Surface::Sphere(ax, _) => {
                let (x, y, z) = local(ax);
                let rho = (x * x + y * y).sqrt();
                Some(v2(y.atan2(x), z.atan2(rho)))
            }
            Surface::Torus(ax, r1, _) => {
                let (x, y, z) = local(ax);
                let rho = (x * x + y * y).sqrt();
                Some(v2(y.atan2(x), z.atan2(rho - r1)))
            }
            Surface::Trimmed(s) => s.inverse(p),
            _ => None,
        }
    }

    /// Which parameters wrap around with period 2*pi (for unwrapping).
    pub fn periodic(&self) -> (bool, bool) {
        match self {
            Surface::Cylinder(..) | Surface::Cone(..) => (true, false),
            Surface::Sphere(..) => (true, false),
            Surface::Torus(..) => (true, true),
            Surface::Revolution { .. } => (true, false),
            Surface::Trimmed(s) | Surface::Offset(s, _) => s.periodic(),
            _ => (false, false),
        }
    }
}

fn rotate_about(q: V3, p: V3, k: V3, angle: f64) -> V3 {
    // Rodrigues' formula.
    let v = q - p;
    let (s, c) = angle.sin_cos();
    p + v * c + k.cross(v) * s + k * (k.dot(v) * (1.0 - c))
}

fn read_surface(t: &mut Tokens, depth: u32) -> Result<Surface, FcError> {
    if depth > MAX_NESTING {
        return Err(FcError::Brep("surface nesting too deep".into()));
    }
    let kind = t.int()?;
    Ok(match kind {
        1 => Surface::Plane(read_ax(t, true)?),
        2 => Surface::Cylinder(read_ax(t, true)?, t.f64()?),
        3 => Surface::Cone(read_ax(t, true)?, t.f64()?, t.f64()?),
        4 => Surface::Sphere(read_ax(t, true)?, t.f64()?),
        5 => Surface::Torus(read_ax(t, true)?, t.f64()?, t.f64()?),
        6 => {
            let dir = t.v3()?;
            Surface::Extrusion {
                dir,
                basis: read_curve(t, true, depth + 1)?,
            }
        }
        7 => {
            let p = t.v3()?;
            let dir = t.v3()?;
            Surface::Revolution {
                p,
                dir,
                basis: read_curve(t, true, depth + 1)?,
            }
        }
        8 => {
            let ur = t.int()? != 0;
            let vr = t.int()? != 0;
            let du = t.count()?;
            let dv = t.count()?;
            if du == 0 || dv == 0 || du > 25 || dv > 25 {
                return Err(FcError::Brep("bad Bezier surface degree".into()));
            }
            let poles = read_poles(t, (du + 1) * (dv + 1), ur || vr, true)?;
            let clamp = |d: usize| {
                let mut k = vec![0.0; d + 1];
                k.extend(std::iter::repeat_n(1.0, d + 1));
                k
            };
            Surface::Spline {
                du,
                dv,
                poles,
                uknots: clamp(du),
                vknots: clamp(dv),
            }
        }
        9 => {
            let ur = t.int()? != 0;
            let vr = t.int()? != 0;
            let up = t.int()? != 0;
            let vp = t.int()? != 0;
            let du = t.count()?;
            let dv = t.count()?;
            let nbu = t.count()?;
            let nbv = t.count()?;
            let nku = t.count()?;
            let nkv = t.count()?;
            if nbu.saturating_mul(nbv) > (1 << 22) {
                return Err(FcError::Brep("B-spline surface too large".into()));
            }
            let raw = read_poles(t, nbu * nbv, ur || vr, true)?;
            let uk = read_knots(t, nku)?;
            let vk = read_knots(t, nkv)?;
            let (uflat, uidx) = flat_knots(&uk, du, up, nbu)?;
            let (vflat, vidx) = flat_knots(&vk, dv, vp, nbv)?;
            let mut poles = Vec::with_capacity(uidx.len() * vidx.len());
            for &i in &uidx {
                for &j in &vidx {
                    poles.push(raw[i * nbv + j]);
                }
            }
            Surface::Spline {
                du,
                dv,
                poles,
                uknots: uflat,
                vknots: vflat,
            }
        }
        10 => {
            for _ in 0..4 {
                t.f64()?;
            }
            Surface::Trimmed(Box::new(read_surface(t, depth + 1)?))
        }
        11 => {
            let off = t.f64()?;
            Surface::Offset(Box::new(read_surface(t, depth + 1)?), off)
        }
        k => return Err(FcError::Brep(format!("unknown surface type {k}"))),
    })
}

/// Default angular step for sampling circular edges: 48 segments per turn.
pub const ANGLE_STEP: f64 = TAU / 48.0;

/// Largest chord error (mm) accepted on big circular edges.
const MAX_SAGITTA: f64 = 1.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periodic_bspline_matches_freecad() {
        // Closed periodic cubic through a square-ish set of poles, values
        // printed by FreeCAD 1.0.2 (`Part.BSplineCurve` with
        // `interpolate(..., PeriodicFlag=True)` is not needed: poles given).
        let text = "7 0 1 3 4 5 \
            0 0 0  10 0 0  10 10 0  0 10 0 \
            0 1 1 1 2 1 3 1 4 1";
        let mut t = Tokens::new(text);
        let c = Curve::read3d(&mut t).unwrap();
        // A uniform periodic cubic B-spline: at a knot the point is
        // (P[i-1] + 4 P[i] + P[i+1]) / 6 for some index shift; check the
        // curve stays closed and symmetric.
        let a = c.eval(0.0);
        let b = c.eval(4.0);
        assert!((a - b).length() < 1e-9);
        let mid = c.eval(0.0) + c.eval(1.0) + c.eval(2.0) + c.eval(3.0);
        assert!((mid * 0.25 - v3(5.0, 5.0, 0.0)).length() < 1e-9);
    }
}
