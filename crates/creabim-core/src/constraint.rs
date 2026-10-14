//! Dimension constraints on sketches and the solver that keeps them.
//!
//! The solver moves a sketch's corners (all but the first, which anchors
//! it), its arcs' bulges and a circle's radius as little as it can until
//! every constraint holds: damped Gauss-Newton steps of least norm. Corners
//! shared by joining segments are one point, so joins always stay together.
//! A constraint that repeats or contradicts the others is refused, and the
//! sketch is left as it was.

use crate::sketch::{SegmentKind, Sketch};
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// A dimension or relation on a sketch. Segments and corners are numbered
/// from 0; segment `i` runs from corner `i` to corner `i + 1`. Directions
/// and lengths are measured end to end, so an arc's are its chord's.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Constraint {
    /// A typed length, in metres.
    Length {
        segment: usize,
        value: f64,
    },
    /// Along the plane's x axis.
    Horizontal {
        segment: usize,
    },
    /// Along the plane's y axis.
    Vertical {
        segment: usize,
    },
    Perpendicular {
        a: usize,
        b: usize,
    },
    Parallel {
        a: usize,
        b: usize,
    },
    /// Two segments the same length.
    Equal {
        a: usize,
        b: usize,
    },
    /// An arc's or the circle's radius, in metres.
    Radius {
        segment: usize,
        value: f64,
    },
    /// Two corners at the same place.
    Coincident {
        a: usize,
        b: usize,
    },
}

/// Why a solve was refused.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SolveError {
    #[error("{0}")]
    Invalid(String),
    #[error("“{0}” is already fixed by the other constraints.")]
    Redundant(String),
    #[error("“{0}” can't be met together with the other constraints.")]
    Conflict(String),
}

impl Constraint {
    /// The short mark drawn on the sketch.
    pub fn glyph(&self) -> &'static str {
        match self {
            Constraint::Length { .. } => "",
            Constraint::Horizontal { .. } => "H",
            Constraint::Vertical { .. } => "V",
            Constraint::Perpendicular { .. } => "⟂",
            Constraint::Parallel { .. } => "∥",
            Constraint::Equal { .. } => "=",
            Constraint::Radius { .. } => "R",
            Constraint::Coincident { .. } => "●",
        }
    }

    /// A plain description, numbering segments and corners from 1.
    pub fn describe(&self) -> String {
        match *self {
            Constraint::Length { segment, value } => {
                format!("Length of segment {}: {value:.3} m", segment + 1)
            }
            Constraint::Horizontal { segment } => format!("Segment {} horizontal", segment + 1),
            Constraint::Vertical { segment } => format!("Segment {} vertical", segment + 1),
            Constraint::Perpendicular { a, b } => {
                format!("Segments {} and {} perpendicular", a + 1, b + 1)
            }
            Constraint::Parallel { a, b } => format!("Segments {} and {} parallel", a + 1, b + 1),
            Constraint::Equal { a, b } => format!("Segments {} and {} equal", a + 1, b + 1),
            Constraint::Radius { segment, value } => {
                format!("Radius of segment {}: {value:.3} m", segment + 1)
            }
            Constraint::Coincident { a, b } => format!("Corners {} and {} together", a + 1, b + 1),
        }
    }

    /// The typed value of a length or radius.
    pub fn value(&self) -> Option<f64> {
        match *self {
            Constraint::Length { value, .. } | Constraint::Radius { value, .. } => Some(value),
            _ => None,
        }
    }

    pub fn value_mut(&mut self) -> Option<&mut f64> {
        match self {
            Constraint::Length { value, .. } | Constraint::Radius { value, .. } => Some(value),
            _ => None,
        }
    }

    /// The segments it is drawn on.
    pub fn segments(&self) -> Vec<usize> {
        match *self {
            Constraint::Length { segment, .. }
            | Constraint::Horizontal { segment }
            | Constraint::Vertical { segment }
            | Constraint::Radius { segment, .. } => vec![segment],
            Constraint::Perpendicular { a, b }
            | Constraint::Parallel { a, b }
            | Constraint::Equal { a, b } => vec![a, b],
            Constraint::Coincident { .. } => vec![],
        }
    }

    /// The corners it pins, for a coincidence.
    pub fn corners(&self) -> Vec<usize> {
        match *self {
            Constraint::Coincident { a, b } => vec![a, b],
            _ => vec![],
        }
    }

    /// Whether its segments and corners exist in a sketch of this size.
    pub fn fits(&self, segments: usize, corners: usize) -> bool {
        self.segments().iter().all(|s| *s < segments) && self.corners().iter().all(|c| *c < corners)
    }

    /// Why it can't apply to `sketch`, if it can't.
    pub fn check(&self, sketch: &Sketch) -> Result<(), String> {
        if !self.fits(sketch.segment_count(), sketch.points.len()) {
            return Err(format!("“{}” refers to a missing segment", self.describe()));
        }
        if let Some(v) = self.value() {
            if !(v.is_finite() && v > 1e-9) {
                return Err("a dimension must be more than zero".into());
            }
        }
        match *self {
            Constraint::Perpendicular { a, b }
            | Constraint::Parallel { a, b }
            | Constraint::Equal { a, b }
                if a == b =>
            {
                Err("pick two different segments".into())
            }
            Constraint::Coincident { a, b } if a == b => Err("pick two different corners".into()),
            Constraint::Radius { segment, .. } if sketch.radius(segment).is_none() => {
                Err(format!("segment {} is not an arc", segment + 1))
            }
            _ if sketch.circle.is_some() && !matches!(self, Constraint::Radius { .. }) => {
                Err("a circle only takes a radius".into())
            }
            _ => Ok(()),
        }
    }

    /// The length or radius the sketch has now, for a new dimension.
    pub fn measure(&self, sketch: &Sketch) -> Option<f64> {
        match *self {
            Constraint::Length { segment, .. } => Some(sketch.chord(segment)),
            Constraint::Radius { segment, .. } => sketch.radius(segment),
            _ => None,
        }
    }

    /// How far from holding it is: one number, or two for a coincidence.
    fn residual(&self, s: &Sketch, out: &mut Vec<f64>) {
        let d = |i: usize| {
            let (a, b) = s.ends(i);
            b - a
        };
        let unit = |u: DVec2, v: DVec2| (u.length() * v.length()).max(1e-12);
        match *self {
            Constraint::Length { segment, value } => out.push(d(segment).length() - value),
            Constraint::Horizontal { segment } => out.push(d(segment).y),
            Constraint::Vertical { segment } => out.push(d(segment).x),
            Constraint::Perpendicular { a, b } => {
                let (u, v) = (d(a), d(b));
                out.push(u.dot(v) / unit(u, v))
            }
            Constraint::Parallel { a, b } => {
                let (u, v) = (d(a), d(b));
                out.push(u.perp_dot(v) / unit(u, v))
            }
            Constraint::Equal { a, b } => out.push(d(a).length() - d(b).length()),
            Constraint::Radius { segment, value } => {
                out.push(s.radius(segment).unwrap_or(0.0) - value)
            }
            Constraint::Coincident { a, b } => {
                let e = s.points[a] - s.points[b];
                out.extend([e.x, e.y]);
            }
        }
    }
}

/// The numbers the solver may change: every corner but the first, each
/// arc's bulge, and a circle's radius.
fn read(s: &Sketch) -> Vec<f64> {
    if let Some(c) = s.circle {
        return vec![c.radius];
    }
    let mut x: Vec<f64> = s.points.iter().skip(1).flat_map(|p| [p.x, p.y]).collect();
    for i in 0..s.segment_count() {
        if let SegmentKind::Arc { bulge } = s.kind(i) {
            x.push(bulge);
        }
    }
    x
}

fn write(s: &mut Sketch, x: &[f64]) {
    if let Some(c) = &mut s.circle {
        c.radius = x[0];
        return;
    }
    let n = s.points.len();
    for (i, p) in s.points.iter_mut().skip(1).enumerate() {
        *p = DVec2::new(x[2 * i], x[2 * i + 1]);
    }
    let mut k = 2 * (n - 1);
    for i in 0..s.segment_count() {
        if let Some(SegmentKind::Arc { bulge }) = s.kinds.get_mut(i) {
            *bulge = x[k];
            k += 1;
        }
    }
}

fn residuals(s: &Sketch, cons: &[Constraint]) -> Vec<f64> {
    let mut r = Vec::new();
    for c in cons {
        c.residual(s, &mut r);
    }
    r
}

fn norm2(r: &[f64]) -> f64 {
    r.iter().map(|v| v * v).sum()
}

/// The residuals' derivatives by the variables, by central differences:
/// one row per residual.
fn jacobian(work: &mut Sketch, cons: &[Constraint], x: &[f64], rows: usize) -> Vec<Vec<f64>> {
    let mut j = vec![vec![0.0; x.len()]; rows];
    let mut xp = x.to_vec();
    for k in 0..x.len() {
        let h = 1e-7 * x[k].abs().max(1.0);
        xp[k] = x[k] + h;
        write(work, &xp);
        let hi = residuals(work, cons);
        xp[k] = x[k] - h;
        write(work, &xp);
        let lo = residuals(work, cons);
        xp[k] = x[k];
        for (row, (a, b)) in j.iter_mut().zip(hi.iter().zip(&lo)) {
            row[k] = (a - b) / (2.0 * h);
        }
    }
    write(work, x);
    j
}

/// Solve `a · y = b` by Gaussian elimination; `None` when singular.
fn solve_linear(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for c in 0..n {
        let p = (c..n).max_by(|i, j| a[*i][c].abs().total_cmp(&a[*j][c].abs()))?;
        if a[p][c].abs() < 1e-300 {
            return None;
        }
        a.swap(c, p);
        b.swap(c, p);
        let (top, rest) = a.split_at_mut(c + 1);
        let pivot = &top[c];
        for (r, row) in rest.iter_mut().enumerate() {
            let f = row[c] / pivot[c];
            if f != 0.0 {
                for (x, p) in row.iter_mut().zip(pivot).skip(c) {
                    *x -= f * p;
                }
                b[c + 1 + r] -= f * b[c];
            }
        }
    }
    let mut y = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|k| a[r][k] * y[k]).sum();
        y[r] = (b[r] - s) / a[r][r];
    }
    Some(y)
}

/// Whether some row of `j` is (nearly) a combination of the rows before
/// it; returns the first such row.
fn dependent_row(j: &[Vec<f64>]) -> Option<usize> {
    let mut basis: Vec<Vec<f64>> = Vec::new();
    for (i, row) in j.iter().enumerate() {
        let size = norm2(row).sqrt();
        if size < 1e-9 {
            return Some(i);
        }
        let mut v: Vec<f64> = row.iter().map(|x| x / size).collect();
        // twice, for the round-off
        for _ in 0..2 {
            for b in &basis {
                let d: f64 = v.iter().zip(b).map(|(x, y)| x * y).sum();
                for (x, y) in v.iter_mut().zip(b) {
                    *x -= d * y;
                }
            }
        }
        let left = norm2(&v).sqrt();
        if left < 1e-6 {
            return Some(i);
        }
        basis.push(v.iter().map(|x| x / left).collect());
    }
    None
}

enum Attempt {
    Solved(Sketch),
    /// Converged, but a constraint adds nothing to the ones before it.
    Redundant,
    NoSolution,
}

/// Run the solver on `start` with `cons`.
fn attempt(start: &Sketch, cons: &[Constraint]) -> Attempt {
    let mut work = start.clone();
    let mut x = read(start);
    let mut r = residuals(&work, cons);
    let rows = r.len();
    if rows == 0 {
        return Attempt::Solved(work);
    }
    let mut cost = norm2(&r);
    let mut lambda = 1e-6;
    for _ in 0..200 {
        if r.iter().all(|v| v.abs() < 1e-12) {
            break;
        }
        let j = jacobian(&mut work, cons, &x, rows);
        // the step of least norm: dx = -Jᵀ (J Jᵀ + λI)⁻¹ r
        let jjt: Vec<Vec<f64>> = (0..rows)
            .map(|a| {
                (0..rows)
                    .map(|b| j[a].iter().zip(&j[b]).map(|(p, q)| p * q).sum())
                    .collect()
            })
            .collect();
        let mut stepped = false;
        for _ in 0..30 {
            let mut m = jjt.clone();
            for (i, row) in m.iter_mut().enumerate() {
                row[i] += lambda * (1.0 + row[i]);
            }
            let Some(y) = solve_linear(m, r.clone()) else {
                lambda *= 10.0;
                continue;
            };
            let next: Vec<f64> = (0..x.len())
                .map(|k| x[k] - (0..rows).map(|i| j[i][k] * y[i]).sum::<f64>())
                .collect();
            write(&mut work, &next);
            let rn = residuals(&work, cons);
            let cn = norm2(&rn);
            if cn.is_finite() && cn < cost {
                x = next;
                r = rn;
                cost = cn;
                lambda = (lambda / 10.0).max(1e-15);
                stepped = true;
                break;
            }
            lambda *= 10.0;
        }
        write(&mut work, &x);
        if !stepped {
            break;
        }
    }
    if r.iter().any(|v| v.abs() > 1e-9) {
        return Attempt::NoSolution;
    }
    let j = jacobian(&mut work, cons, &x, rows);
    match dependent_row(&j) {
        Some(_) => Attempt::Redundant,
        None => Attempt::Solved(work),
    }
}

impl Sketch {
    /// Move the sketch until every constraint holds. When they can't all
    /// hold, or one repeats the others, the sketch is left as it was and
    /// the error names the first constraint at fault.
    pub fn solve(&mut self) -> Result<(), SolveError> {
        self.check().map_err(SolveError::Invalid)?;
        if let Attempt::Solved(s) = attempt(self, &self.constraints) {
            s.check().map_err(SolveError::Invalid)?;
            *self = s;
            return Ok(());
        }
        // find the first constraint the ones before it can't take
        for k in 1..=self.constraints.len() {
            let c = &self.constraints[k - 1];
            match attempt(self, &self.constraints[..k]) {
                Attempt::Solved(_) => {}
                Attempt::Redundant => return Err(SolveError::Redundant(c.describe())),
                Attempt::NoSolution => return Err(SolveError::Conflict(c.describe())),
            }
        }
        Err(SolveError::Conflict("the constraints".into()))
    }

    /// Add a constraint and solve; refused (and nothing changed) when the
    /// sketch can't take it.
    pub fn constrain(&mut self, c: Constraint) -> Result<(), SolveError> {
        let mut next = self.clone();
        next.constraints.push(c);
        next.solve()?;
        *self = next;
        Ok(())
    }

    /// Change constraint `index`'s length or radius and solve again.
    pub fn set_dimension(&mut self, index: usize, value: f64) -> Result<(), SolveError> {
        let mut next = self.clone();
        if let Some(v) = next.constraints.get_mut(index).and_then(|c| c.value_mut()) {
            *v = value;
        }
        next.solve()?;
        *self = next;
        Ok(())
    }

    /// Remove constraint `index`. The shape stays as it is.
    pub fn unconstrain(&mut self, index: usize) {
        if index < self.constraints.len() {
            self.constraints.remove(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sketch::Curve, Workplane};

    fn close(a: DVec2, x: f64, y: f64) -> bool {
        a.distance(DVec2::new(x, y)) < 1e-9
    }

    /// A rectangle drawn roughly by hand.
    fn rough_rectangle() -> Sketch {
        Sketch::polyline(
            None,
            Workplane::ground(0.0),
            vec![
                DVec2::new(0.0, 0.0),
                DVec2::new(3.8, 0.2),
                DVec2::new(4.1, 2.7),
                DVec2::new(-0.1, 3.2),
            ],
            true,
        )
    }

    fn four_by_three() -> Sketch {
        let mut s = rough_rectangle();
        for c in [
            Constraint::Horizontal { segment: 0 },
            Constraint::Vertical { segment: 1 },
            Constraint::Horizontal { segment: 2 },
            Constraint::Vertical { segment: 3 },
            Constraint::Length {
                segment: 0,
                value: 4.0,
            },
            Constraint::Length {
                segment: 1,
                value: 3.0,
            },
        ] {
            s.constrain(c).unwrap();
        }
        s
    }

    #[test]
    fn a_rectangle_solves_to_four_by_three() {
        let s = four_by_three();
        let p = &s.points;
        assert!(close(p[0], 0.0, 0.0), "{p:?}");
        assert!(close(p[1], 4.0, 0.0), "{p:?}");
        assert!(close(p[2], 4.0, 3.0), "{p:?}");
        assert!(close(p[3], 0.0, 3.0), "{p:?}");
    }

    #[test]
    fn changing_a_length_solves_again() {
        let mut s = four_by_three();
        s.set_dimension(4, 5.0).unwrap();
        let p = &s.points;
        assert!(close(p[1], 5.0, 0.0) && close(p[2], 5.0, 3.0) && close(p[3], 0.0, 3.0));
        assert_eq!(s.constraints[4].value(), Some(5.0));
    }

    #[test]
    fn an_over_constrained_sketch_is_refused_and_left_alone() {
        let mut s = four_by_three();
        let before = s.clone();
        // the opposite side's length is already fixed
        let e = s
            .constrain(Constraint::Length {
                segment: 2,
                value: 4.0,
            })
            .unwrap_err();
        assert!(matches!(e, SolveError::Redundant(_)), "{e}");
        // and a different one contradicts it
        let e = s
            .constrain(Constraint::Length {
                segment: 2,
                value: 6.0,
            })
            .unwrap_err();
        assert!(matches!(e, SolveError::Conflict(_)), "{e}");
        let e = s
            .constrain(Constraint::Horizontal { segment: 1 })
            .unwrap_err();
        assert!(!e.to_string().is_empty());
        let e = s
            .constrain(Constraint::Perpendicular { a: 0, b: 1 })
            .unwrap_err();
        assert!(matches!(e, SolveError::Redundant(_)), "{e}");
        assert_eq!(s, before);
        // a length that can't be met is refused too
        let mut t = s.clone();
        assert!(t.set_dimension(4, -1.0).is_err());
        assert_eq!(t, before);
    }

    #[test]
    fn perpendicular_parallel_and_equal_make_a_square() {
        let mut s = rough_rectangle();
        for c in [
            Constraint::Horizontal { segment: 0 },
            Constraint::Perpendicular { a: 0, b: 1 },
            Constraint::Parallel { a: 0, b: 2 },
            Constraint::Parallel { a: 1, b: 3 },
            Constraint::Equal { a: 0, b: 1 },
            Constraint::Length {
                segment: 0,
                value: 2.0,
            },
        ] {
            s.constrain(c).unwrap();
        }
        let p = &s.points;
        for i in 0..4 {
            assert!((s.chord(i) - 2.0).abs() < 1e-9, "{p:?}");
        }
        assert!((p[0].distance(p[2]) - 8f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn arcs_and_circles_take_a_radius() {
        let plane = Workplane::ground(0.0);
        let mut s = Sketch::polyline(
            None,
            plane,
            vec![DVec2::ZERO, DVec2::new(4.0, 0.0), DVec2::new(4.0, 2.0)],
            false,
        );
        s.kinds = vec![SegmentKind::Line, SegmentKind::Arc { bulge: 0.3 }];
        s.constrain(Constraint::Radius {
            segment: 1,
            value: 1.5,
        })
        .unwrap();
        assert!((s.radius(1).unwrap() - 1.5).abs() < 1e-9);
        // the radius of a straight segment makes no sense
        assert!(matches!(
            s.constrain(Constraint::Radius {
                segment: 0,
                value: 1.0
            }),
            Err(SolveError::Invalid(_))
        ));
        assert!(matches!(s.curve(1), Curve::Arc { .. }));

        let mut c = Sketch::circle(None, plane, DVec2::new(1.0, 1.0), 2.0);
        c.constrain(Constraint::Radius {
            segment: 0,
            value: 0.75,
        })
        .unwrap();
        assert!((c.circle.unwrap().radius - 0.75).abs() < 1e-12);
        assert!(c.constrain(Constraint::Horizontal { segment: 0 }).is_err());
    }

    #[test]
    fn coincident_corners_meet() {
        let mut s = Sketch::polyline(
            None,
            Workplane::ground(0.0),
            vec![
                DVec2::ZERO,
                DVec2::new(2.0, 0.0),
                DVec2::new(2.0, 2.0),
                DVec2::new(0.1, 0.2),
            ],
            false,
        );
        s.constrain(Constraint::Coincident { a: 0, b: 3 }).unwrap();
        assert!(s.points[3].length() < 1e-9);
        // constraints survive a save
        let json = serde_json::to_string(&s).unwrap();
        let back: Sketch = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }
}
