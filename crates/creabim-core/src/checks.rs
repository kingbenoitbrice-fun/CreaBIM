//! Model checks: furniture and fixtures standing in a doorway, in the way
//! of a door's swing or across a window, and where an object put against
//! a wall can go clear of its doors and windows.
//!
//! A door keeps clear the gap in its wall, a strip in front of it on the
//! side it does not open to (to walk through), and the quarter circle its
//! leaf sweeps on the side it opens to. A window keeps clear only the gap
//! in its wall. Both only up to their head and from their sill: a light
//! over a door or a radiator under a window is fine.

use crate::objects::{self, Object, ObjectType};
use crate::{ElementId, ElementKind, Model, Opening, OpeningKind, Wall};
use glam::DVec2;

/// How far in front of a door, on the side it doesn't open to, is kept
/// clear to walk through.
pub const PASSAGE: f64 = 0.3;
/// Clearance either side of a door or window kept free of objects put
/// against its wall: the frame and architrave.
pub const JAMB: f64 = 0.05;
/// How far in front of a window's wall faces an object counts as fixed
/// across it (a socket in the glass, a radiator in front of a screen).
pub const FACE: f64 = 0.02;

/// What an object is in the way of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocks {
    /// It stands in the gap of the wall, or right in front of a door.
    Opening,
    /// It is in the way of a door's leaf as it opens.
    Swing,
}

/// An object in the way of a door or window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clash {
    pub object: ElementId,
    pub opening: ElementId,
    pub blocks: Blocks,
}

/// The plan zones an opening keeps clear, each a convex polygon, and the
/// heights (absolute) between which it does.
struct Zones {
    parts: Vec<(Blocks, Vec<DVec2>)>,
    z: (f64, f64),
}

fn zones(model: &Model, o: &Opening, w: &Wall) -> Zones {
    let d = w.direction();
    let n = d.perp();
    let t = w.thickness / 2.0;
    let (u0, u1) = (o.offset, o.offset + o.width);
    let at = |u: f64, s: f64| w.start + d * u + n * s;
    let base = crate::walls::base_z(model, w);
    let z = (base + o.sill, base + o.sill + o.height);
    let mut parts = vec![];
    if o.kind == OpeningKind::Door {
        let side = if o.faces_left(w) { 1.0 } else { -1.0 };
        // the gap and the strip in front of it, on the side it doesn't open to
        let back = -side * (t + PASSAGE);
        parts.push((
            Blocks::Opening,
            vec![at(u0, back), at(u1, back), at(u1, side * t), at(u0, side * t)],
        ));
        // the leaf's sweep: a quarter circle about its hinge
        let (hinge_u, towards) = if o.flip_hand {
            (u1, -1.0)
        } else {
            (u0, 1.0)
        };
        let hinge = at(hinge_u, side * t);
        let along = d * towards;
        let out = n * side;
        let r = o.width;
        let mut sweep = vec![hinge];
        for i in 0..=8 {
            let a = std::f64::consts::FRAC_PI_2 * i as f64 / 8.0;
            sweep.push(hinge + (along * a.cos() + out * a.sin()) * r);
        }
        parts.push((Blocks::Swing, sweep));
    } else {
        // and a sliver in front of each face: what is fixed to the wall there
        let t = t + FACE;
        parts.push((
            Blocks::Opening,
            vec![at(u0, -t), at(u1, -t), at(u1, t), at(u0, t)],
        ));
    }
    Zones { parts, z }
}

/// Do two convex polygons overlap by more than touching? (Separating axes.)
fn convex_overlap(a: &[DVec2], b: &[DVec2]) -> bool {
    const EPS: f64 = 1e-4;
    for poly in [a, b] {
        let n = poly.len();
        for i in 0..n {
            let e = poly[(i + 1) % n] - poly[i];
            if e.length_squared() < 1e-18 {
                continue;
            }
            let axis = e.perp().normalize();
            let range = |p: &[DVec2]| {
                p.iter().fold((f64::MAX, f64::MIN), |(lo, hi), q| {
                    let v = q.dot(axis);
                    (lo.min(v), hi.max(v))
                })
            };
            let (a0, a1) = range(a);
            let (b0, b1) = range(b);
            if a1 <= b0 + EPS || b1 <= a0 + EPS {
                return false;
            }
        }
    }
    true
}

/// A placed object's footprint (the hull of its parts in plan) and the
/// heights it spans.
fn footprint(model: &Model, o: &Object, t: &ObjectType) -> (Vec<DVec2>, (f64, f64)) {
    let pts: Vec<DVec2> = t
        .parts
        .iter()
        .flat_map(|p| p.mesh.vertices.iter())
        .map(|v| o.to_plan(v.truncate()))
        .collect();
    let z0 = objects::base_z_of(model, o, t);
    let (lo, hi) = t.bounds();
    (objects::hull(&pts), (z0 + lo.z, z0 + hi.z))
}

/// Every object in the way of a door or window on its level.
pub fn objects_in_openings(model: &Model) -> Vec<Clash> {
    let mut openings: Vec<(ElementId, ElementId, Zones)> = vec![];
    for e in model.iter() {
        let ElementKind::Opening(o) = &e.kind else {
            continue;
        };
        let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
            continue;
        };
        openings.push((e.id, w.level, zones(model, o, w)));
    }
    let mut out = vec![];
    for e in model.iter() {
        let ElementKind::Object(o) = &e.kind else {
            continue;
        };
        let Some(t) = objects::type_of(model, e) else {
            continue;
        };
        let (hull, (z0, z1)) = footprint(model, o, t);
        if hull.len() < 3 {
            continue;
        }
        for (oid, level, zs) in &openings {
            if *level != o.level || z1 <= zs.z.0 + 1e-6 || z0 >= zs.z.1 - 1e-6 {
                continue;
            }
            // the gap first: an object in a doorway is in its swing too
            if let Some((blocks, _)) = zs.parts.iter().find(|(_, p)| convex_overlap(&hull, p)) {
                out.push(Clash {
                    object: e.id,
                    opening: *oid,
                    blocks: *blocks,
                });
            }
        }
    }
    out
}

/// The stretches along `wall` (distances from its start) that an object
/// spanning heights `z` must keep clear of: its doors and windows at those
/// heights, and their jambs.
pub fn blocked_spans(model: &Model, wall: ElementId, z: (f64, f64)) -> Vec<(f64, f64)> {
    let Some(ElementKind::Wall(w)) = model.get(wall).map(|e| &e.kind) else {
        return vec![];
    };
    let base = crate::walls::base_z(model, w);
    let mut out: Vec<(f64, f64)> = model
        .dependents(wall)
        .into_iter()
        .filter_map(|id| match &model.get(id)?.kind {
            ElementKind::Opening(o) => {
                let (o0, o1) = (base + o.sill, base + o.sill + o.height);
                (z.1 > o0 + 1e-6 && z.0 < o1 - 1e-6)
                    .then_some((o.offset - JAMB, o.offset + o.width + JAMB))
            }
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// Where along a wall an object `span` wide about its origin (its own x
/// from `span.0` to `span.1`, the way the wall runs) goes nearest `u`
/// without covering any of `blocked`, within `0..len`. `None` when it fits
/// nowhere.
pub fn clear_spot(u: f64, span: (f64, f64), blocked: &[(f64, f64)], len: f64) -> Option<f64> {
    let free = |v: f64| {
        let (a, b) = (v + span.0, v + span.1);
        a >= -1e-9 && b <= len + 1e-9 && blocked.iter().all(|g| b <= g.0 + 1e-9 || a >= g.1 - 1e-9)
    };
    if free(u) {
        return Some(u);
    }
    let mut candidates = vec![-span.0, len - span.1];
    for g in blocked {
        candidates.push(g.0 - span.1);
        candidates.push(g.1 - span.0);
    }
    candidates
        .into_iter()
        .filter(|v| free(*v))
        .min_by(|a, b| (a - u).abs().total_cmp(&(b - u).abs()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spot_slides_off_a_door_to_the_nearer_side() {
        let blocked = [(2.0, 3.0)];
        // a 1 m wide object centred at 2.4 overlaps the door: it goes left
        assert_eq!(clear_spot(2.4, (-0.5, 0.5), &blocked, 6.0), Some(1.5));
        assert_eq!(clear_spot(2.8, (-0.5, 0.5), &blocked, 6.0), Some(3.5));
        assert_eq!(clear_spot(5.0, (-0.5, 0.5), &blocked, 6.0), Some(5.0));
        // nowhere to go on a wall the door fills
        assert_eq!(clear_spot(0.5, (-0.5, 0.5), &[(0.0, 1.0)], 1.0), None);
    }

    #[test]
    fn convex_shapes_that_only_touch_do_not_overlap() {
        let sq = |x: f64| crate::polygon::rectangle(DVec2::new(x, 0.0), DVec2::new(x + 1.0, 1.0));
        assert!(convex_overlap(&sq(0.0), &sq(0.5)));
        assert!(!convex_overlap(&sq(0.0), &sq(1.0)));
        assert!(!convex_overlap(&sq(0.0), &sq(2.0)));
    }
}
