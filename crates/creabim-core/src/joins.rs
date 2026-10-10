//! Wall joins: how wall ends are trimmed or extended where walls meet.
//!
//! Two walls that share an endpoint (an L or a straight run) are mitred, so
//! their faces meet cleanly at the corner. A wall that ends on the middle of
//! another (a T) is trimmed back to that wall's face. Three or more walls at
//! one point are left as they are.

use crate::{ElementId, ElementKind, Model, Wall};
use glam::DVec2;
use std::collections::HashMap;

/// How far each face of a wall runs past its nominal end, in metres along the
/// wall. Index 0 is the face on the wall's left (start → end), index 1 the
/// face on its right. Negative values trim the wall.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WallEnds {
    pub start: [f64; 2],
    pub end: [f64; 2],
}

impl WallEnds {
    /// Wall-local `u` where the face on `side` (+1 left, -1 right) ends at the start.
    pub fn start_u(&self, side: f64) -> f64 {
        -self.start[if side > 0.0 { 0 } else { 1 }]
    }

    /// Wall-local `u` where the face on `side` ends at the far end.
    pub fn end_u(&self, wall: &Wall, side: f64) -> f64 {
        wall.length() + self.end[if side > 0.0 { 0 } else { 1 }]
    }

    /// The wall's footprint as a polygon (left start, left end, right end, right start).
    pub fn footprint(&self, wall: &Wall) -> [DVec2; 4] {
        let d = wall.direction();
        let n = d.perp();
        let t = wall.thickness / 2.0;
        let at = |u: f64, s: f64| wall.start + d * u + n * s;
        [
            at(self.start_u(1.0), t),
            at(self.end_u(wall, 1.0), t),
            at(self.end_u(wall, -1.0), -t),
            at(self.start_u(-1.0), -t),
        ]
    }
}

const TOL: f64 = 1e-3;

/// Join every wall in the model with its neighbours on the same level.
pub fn wall_joins(model: &Model) -> HashMap<ElementId, WallEnds> {
    let walls: Vec<(ElementId, &Wall)> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Wall(w) if w.length() > TOL => Some((e.id, w)),
            _ => None,
        })
        .collect();
    let mut out = HashMap::new();
    for &(id, w) in &walls {
        let mut ends = WallEnds::default();
        for at_end in [false, true] {
            let (p, other) = if at_end {
                (w.end, w.start)
            } else {
                (w.start, w.end)
            };
            // direction arriving at the joint, and the wall's own left side
            // expressed relative to that direction
            let d_in = (p - other).normalize();
            let local_left = if at_end { 1.0 } else { -1.0 };
            let t = w.thickness / 2.0;

            let neighbours: Vec<(&Wall, DVec2)> = walls
                .iter()
                .filter(|(oid, o)| *oid != id && o.level == w.level)
                .filter_map(|(_, o)| {
                    if o.start.distance(p) < TOL {
                        Some((*o, o.direction()))
                    } else if o.end.distance(p) < TOL {
                        Some((*o, -o.direction()))
                    } else {
                        None
                    }
                })
                .collect();

            // extensions of the faces left and right of d_in
            let ext = match neighbours.as_slice() {
                [(o, d_out)] => miter(p, d_in, t, *d_out, o.thickness / 2.0),
                [] => walls
                    .iter()
                    .filter(|(oid, o)| *oid != id && o.level == w.level)
                    .find_map(|(_, o)| tee(p, d_in, t, o)),
                _ => None,
            };
            let Some((left, right)) = ext else { continue };
            let (l, r) = if local_left > 0.0 {
                (left, right)
            } else {
                (right, left)
            };
            if at_end {
                ends.end = [l, r];
            } else {
                ends.start = [l, r];
            }
        }
        out.insert(id, ends);
    }
    out
}

/// Distance along `a` from `p` to where the line `p + a s` meets `q + b r`.
fn meet(p: DVec2, a: DVec2, q: DVec2, b: DVec2) -> Option<f64> {
    let den = a.perp_dot(b);
    (den.abs() > 1e-9).then(|| (q - p).perp_dot(b) / den)
}

/// Mitre a wall arriving along `d_in` with one leaving along `d_out`.
fn miter(p: DVec2, d_in: DVec2, t: f64, d_out: DVec2, t_other: f64) -> Option<(f64, f64)> {
    if d_in.perp_dot(d_out).abs() < 1e-6 {
        return None; // straight run: square ends already meet
    }
    let (n_in, n_out) = (d_in.perp(), d_out.perp());
    let left = meet(p + n_in * t, d_in, p + n_out * t_other, d_out)?;
    let right = meet(p - n_in * t, d_in, p - n_out * t_other, d_out)?;
    let limit = 4.0 * t.max(t_other);
    (left.abs() < limit && right.abs() < limit).then_some((left, right))
}

/// Trim a wall that ends at `p` on the body of wall `o` back to `o`'s near face.
fn tee(p: DVec2, d_in: DVec2, t: f64, o: &Wall) -> Option<(f64, f64)> {
    let d = o.direction();
    let u = (p - o.start).dot(d);
    let off = (p - o.start).perp_dot(d).abs();
    if u <= TOL || u >= o.length() - TOL || off > o.thickness / 2.0 + TOL {
        return None;
    }
    if d_in.perp_dot(d).abs() < 1e-6 {
        return None;
    }
    let n = d.perp();
    // the face on the side the joining wall comes from
    let side = if (-d_in).dot(n) > 0.0 { 1.0 } else { -1.0 };
    let face = o.start + n * (o.thickness / 2.0 * side);
    let n_in = d_in.perp();
    let left = meet(p + n_in * t, d_in, face, d)?;
    let right = meet(p - n_in * t, d_in, face, d)?;
    Some((left, right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Element, Level, Transaction};

    fn model(walls: &[(DVec2, DVec2)]) -> (Model, Vec<ElementId>) {
        let mut doc = crate::Document::new();
        let level = Element::new(
            "L0",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        );
        let lid = level.id;
        let mut tx = Transaction::new("setup").insert(level);
        let mut ids = vec![];
        for (a, b) in walls {
            let e = Element::new(
                "W",
                ElementKind::Wall(Wall {
                    level: lid,
                    start: *a,
                    end: *b,
                    thickness: 0.2,
                    height: 3.0,
                    base_offset: 0.0,
                }),
            );
            ids.push(e.id);
            tx = tx.insert(e);
        }
        doc.commit(tx).unwrap();
        (doc.model().clone(), ids)
    }

    fn wall(m: &Model, id: ElementId) -> Wall {
        match &m.get(id).unwrap().kind {
            ElementKind::Wall(w) => w.clone(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn l_corner_is_mitred() {
        // counter-clockwise corner at (4, 0): left faces are inside
        let (m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(4.0, 0.0)),
            (DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0)),
        ]);
        let j = wall_joins(&m);
        let a = j[&ids[0]];
        assert!((a.end[0] + 0.1).abs() < 1e-9, "inner face shortened");
        assert!((a.end[1] - 0.1).abs() < 1e-9, "outer face extended");
        // both walls' outer faces end at the same corner point
        let fa = a.footprint(&wall(&m, ids[0]));
        let fb = j[&ids[1]].footprint(&wall(&m, ids[1]));
        assert!(fa[2].distance(DVec2::new(4.1, -0.1)) < 1e-9);
        assert!(fb[3].distance(DVec2::new(4.1, -0.1)) < 1e-9);
    }

    #[test]
    fn tee_is_trimmed_to_the_face() {
        let (m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(6.0, 0.0)),
            (DVec2::new(3.0, 4.0), DVec2::new(3.0, 0.0)),
        ]);
        let j = wall_joins(&m);
        let b = j[&ids[1]];
        assert!((b.end[0] + 0.1).abs() < 1e-9 && (b.end[1] + 0.1).abs() < 1e-9);
        assert_eq!(j[&ids[0]], WallEnds::default());
    }

    #[test]
    fn straight_runs_and_free_ends_are_untouched() {
        let (m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(3.0, 0.0)),
            (DVec2::new(3.0, 0.0), DVec2::new(6.0, 0.0)),
        ]);
        let j = wall_joins(&m);
        assert_eq!(j[&ids[0]], WallEnds::default());
        assert_eq!(j[&ids[1]], WallEnds::default());
    }
}
