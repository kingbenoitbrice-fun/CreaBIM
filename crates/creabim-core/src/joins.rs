//! Wall joins: how wall ends are trimmed or extended where walls meet, and
//! how joined walls follow when one of them moves.
//!
//! Two walls whose ends meet (an L) are mitred, so their faces meet cleanly
//! at the corner; walls built of the same assembly then meet layer by layer
//! along the mitre. A wall that ends on the middle of another (a T) is
//! trimmed back to that wall's face; when both are layered, its core runs
//! on through the other's finishes to the other's core. Three or more walls
//! at one point are left as they are.
//!
//! Ends don't have to coincide exactly: an end within a wall's thickness
//! of where the centrelines cross still joins, so a wall nudged a little
//! keeps clean corners. Moving a wall also takes the walls joined to it
//! along ([`follow`]), as in Revit: their ends stretch to stay joined.

use crate::{assembly, Element, ElementId, ElementKind, Model, Wall};
use glam::DVec2;
use std::collections::{HashMap, HashSet};

/// How far each face of a wall runs past its nominal end, in metres along the
/// wall. Index 0 is the face on the wall's left (start → end), index 1 the
/// face on its right. Negative values trim the wall.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WallEnds {
    pub start: [f64; 2],
    pub end: [f64; 2],
    /// How much further a layered wall's core runs than its faces, at the
    /// start and at the end: where it meets another layered wall's core
    /// through that wall's finishes.
    pub core: [f64; 2],
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

    /// Where the start's end line crosses the line `s` across the wall
    /// (left positive): the end is straight from one face to the other.
    pub fn start_at(&self, wall: &Wall, s: f64) -> f64 {
        let k = across(wall, s);
        self.start_u(-1.0) + (self.start_u(1.0) - self.start_u(-1.0)) * k
    }

    /// Where the far end's line crosses the line `s` across the wall.
    pub fn end_at(&self, wall: &Wall, s: f64) -> f64 {
        let k = across(wall, s);
        self.end_u(wall, -1.0) + (self.end_u(wall, 1.0) - self.end_u(wall, -1.0)) * k
    }

    /// The start of a layer at `s`: the end line, or past it for the core.
    pub fn layer_start(&self, wall: &Wall, s: f64, core: bool) -> f64 {
        self.start_at(wall, s) - if core { self.core[0] } else { 0.0 }
    }

    /// The far end of a layer at `s`.
    pub fn layer_end(&self, wall: &Wall, s: f64, core: bool) -> f64 {
        self.end_at(wall, s) + if core { self.core[1] } else { 0.0 }
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

    /// The footprint in wall-local `(u, s)`, counter-clockwise, with the
    /// steps where the core (`core`, its span across the wall) runs on.
    pub fn local_outline(&self, wall: &Wall, core: (f64, f64)) -> Vec<DVec2> {
        let t = wall.thickness / 2.0;
        let p = DVec2::new;
        let (c0, c1) = core;
        // along the right face, up the far end, back along the left face
        // and down the start
        let mut out = vec![p(self.start_at(wall, -t), -t), p(self.end_at(wall, -t), -t)];
        if self.core[1] > 1e-9 {
            out.extend([
                p(self.end_at(wall, c0), c0),
                p(self.layer_end(wall, c0, true), c0),
                p(self.layer_end(wall, c1, true), c1),
                p(self.end_at(wall, c1), c1),
            ]);
        }
        out.extend([p(self.end_at(wall, t), t), p(self.start_at(wall, t), t)]);
        if self.core[0] > 1e-9 {
            out.extend([
                p(self.start_at(wall, c1), c1),
                p(self.layer_start(wall, c1, true), c1),
                p(self.layer_start(wall, c0, true), c0),
                p(self.start_at(wall, c0), c0),
            ]);
        }
        out.dedup_by(|a, b| a.distance(*b) < 1e-9);
        if out.len() > 1 && out[0].distance(out[out.len() - 1]) < 1e-9 {
            out.pop();
        }
        out
    }
}

/// `s` across a wall as a fraction from its right face (0) to its left (1).
fn across(wall: &Wall, s: f64) -> f64 {
    if wall.thickness > 1e-12 {
        (s + wall.thickness / 2.0) / wall.thickness
    } else {
        0.5
    }
}

const TOL: f64 = 1e-3;

/// A wall with what its joins need to know: its core's span across it,
/// when it is layered.
struct Joinable<'a> {
    id: ElementId,
    wall: &'a Wall,
    core: Option<(f64, f64)>,
}

/// Where the centrelines through `p` along `a` and through `q` along `b` cross.
fn crossing(p: DVec2, a: DVec2, q: DVec2, b: DVec2) -> Option<DVec2> {
    meet(p, a, q, b).map(|s| p + a * s)
}

/// Do the ends `p` (of a wall arriving along `d_in`) and `q` (of a wall
/// leaving along `d_out`) meet in an L? They do when they coincide, or
/// when both lie within `reach` of where the centrelines cross.
fn ends_meet(p: DVec2, d_in: DVec2, q: DVec2, d_out: DVec2, reach: f64) -> bool {
    if p.distance(q) < TOL {
        return true;
    }
    if d_in.perp_dot(d_out).abs() < 0.05 {
        return false; // (nearly) parallel: no corner to meet at
    }
    crossing(p, d_in, q, d_out).is_some_and(|x| x.distance(p) <= reach && x.distance(q) <= reach)
}

/// The other wall ends that meet `p`, the end of a wall arriving along
/// `d_in`: (wall, its direction leaving the joint). Ends that coincide
/// win over ends that are only near.
fn meeting<'a>(
    walls: &'a [Joinable<'a>],
    me: &Joinable,
    p: DVec2,
    d_in: DVec2,
) -> Vec<(&'a Joinable<'a>, DVec2)> {
    let w = me.wall;
    let mut exact = vec![];
    let mut near = vec![];
    for o in walls
        .iter()
        .filter(|o| o.id != me.id && o.wall.level == w.level)
    {
        for (q, d_out) in [
            (o.wall.start, o.wall.direction()),
            (o.wall.end, -o.wall.direction()),
        ] {
            let reach = w.thickness.max(o.wall.thickness);
            if q.distance(p) < TOL {
                exact.push((o, d_out));
            } else if ends_meet(p, d_in, q, d_out, reach) {
                near.push((o, d_out));
            }
        }
    }
    if exact.is_empty() {
        near
    } else {
        exact
    }
}

/// The walls of the model that can join, with their cores.
fn joinables(model: &Model) -> Vec<Joinable<'_>> {
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Wall(w) if w.length() > TOL => Some(Joinable {
                id: e.id,
                wall: w,
                core: assembly::assembly_of(model, e).map(|(_, a)| assembly::wall_core(a, w)),
            }),
            _ => None,
        })
        .collect()
}

/// Join every wall in the model with its neighbours on the same level.
pub fn wall_joins(model: &Model) -> HashMap<ElementId, WallEnds> {
    let walls = joinables(model);
    let mut out = HashMap::new();
    for me in &walls {
        let w = me.wall;
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

            let neighbours = meeting(&walls, me, p, d_in);
            // extensions of the faces left and right of d_in, and of the core
            let ext = match neighbours.as_slice() {
                [(o, d_out)] => {
                    // mitre about where the centrelines cross
                    let q = if o.wall.start.distance(p) <= o.wall.end.distance(p) {
                        o.wall.start
                    } else {
                        o.wall.end
                    };
                    miter(p, d_in, t, q, *d_out, o.wall.thickness / 2.0).map(|(l, r)| (l, r, 0.0))
                }
                [] => walls
                    .iter()
                    .filter(|o| o.id != me.id && o.wall.level == w.level)
                    .filter_map(|o| tee(p, d_in, me, o))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(e, _)| e),
                _ => None,
            };
            let Some((left, right, core)) = ext else {
                continue;
            };
            let (l, r) = if local_left > 0.0 {
                (left, right)
            } else {
                (right, left)
            };
            if at_end {
                ends.end = [l, r];
                ends.core[1] = core;
            } else {
                ends.start = [l, r];
                ends.core[0] = core;
            }
        }
        out.insert(me.id, ends);
    }
    out
}

/// Distance along `a` from `p` to where the line `p + a s` meets `q + b r`.
fn meet(p: DVec2, a: DVec2, q: DVec2, b: DVec2) -> Option<f64> {
    let den = a.perp_dot(b);
    (den.abs() > 1e-9).then(|| (q - p).perp_dot(b) / den)
}

/// Mitre a wall arriving at `p` along `d_in` with one leaving `q` along
/// `d_out`: how far each face of the arriving wall runs past `p`.
fn miter(
    p: DVec2,
    d_in: DVec2,
    t: f64,
    q: DVec2,
    d_out: DVec2,
    t_other: f64,
) -> Option<(f64, f64)> {
    if d_in.perp_dot(d_out).abs() < 1e-6 {
        return None; // straight run: square ends already meet
    }
    let (n_in, n_out) = (d_in.perp(), d_out.perp());
    let left = meet(p + n_in * t, d_in, q + n_out * t_other, d_out)?;
    let right = meet(p - n_in * t, d_in, q - n_out * t_other, d_out)?;
    let limit = 4.0 * t.max(t_other) + p.distance(q);
    (left.abs() < limit && right.abs() < limit).then_some((left, right))
}

/// Trim a wall that ends at `p` on the body of wall `o` back to `o`'s
/// near face: the face extensions and the core's further run, and how far
/// `p` is from `o`'s centreline (the nearest host wins).
fn tee(p: DVec2, d_in: DVec2, me: &Joinable, o: &Joinable) -> Option<((f64, f64, f64), f64)> {
    let (w, ow) = (me.wall, o.wall);
    let t = w.thickness / 2.0;
    let d = ow.direction();
    let u = (p - ow.start).dot(d);
    let off = (p - ow.start).perp_dot(d).abs();
    // an end short of the face, or through it, by up to its own thickness still joins
    if u <= TOL || u >= ow.length() - TOL || off > ow.thickness / 2.0 + w.thickness + TOL {
        return None;
    }
    if d_in.perp_dot(d).abs() < 1e-6 {
        return None;
    }
    let n = d.perp();
    // the face on the side the joining wall comes from
    let side = if (-d_in).dot(n) > 0.0 { 1.0 } else { -1.0 };
    let face = ow.start + n * (ow.thickness / 2.0 * side);
    let n_in = d_in.perp();
    let left = meet(p + n_in * t, d_in, face, d)?;
    let right = meet(p - n_in * t, d_in, face, d)?;
    // layered into layered: the core runs on to the other's core
    let core = match (me.core, o.core) {
        (Some(_), Some((c0, c1))) => {
            let s = if side > 0.0 { c1 } else { c0 };
            let core_face = ow.start + n * s;
            meet(p, d_in, core_face, d)
                .zip(meet(p, d_in, face, d))
                .map_or(0.0, |(c, f)| (c - f).max(0.0))
        }
        _ => 0.0,
    };
    Some(((left, right, core), off))
}

// ---------------------------------------------------------------- following

/// The other walls that follow when `changed` replace their originals in
/// `before` (moved, turned, stretched): ends that met a moved wall's end
/// go with it, ends on a moved wall's body slide along their own wall to
/// stay on it, and a wall moved bodily keeps its ends on the walls they
/// sat on. Doors and windows in a wall whose start moves stay where they
/// were. Changes `changed` in place and returns the other elements that
/// change, as (before, after).
pub fn follow(before: &Model, changed: &mut [Element]) -> Vec<(Element, Element)> {
    let moving: HashSet<ElementId> = changed.iter().map(|e| e.id).collect();
    let walls = joinables(before);
    let mut out: HashMap<ElementId, Element> = HashMap::new();
    let mut order: Vec<ElementId> = vec![];
    for c in changed.iter_mut() {
        let (Some(old), ElementKind::Wall(nw)) = (before.get(c.id), &mut c.kind) else {
            continue;
        };
        let ElementKind::Wall(ow) = &old.kind else {
            continue;
        };
        let Some(me) = walls.iter().find(|j| j.id == old.id) else {
            continue;
        };
        let rigid = (nw.start - ow.start).distance(nw.end - ow.end) < 1e-9;
        for at_end in [false, true] {
            let (p_old, other_old) = if at_end {
                (ow.end, ow.start)
            } else {
                (ow.start, ow.end)
            };
            let p_new = if at_end { nw.end } else { nw.start };
            let d_in = (p_old - other_old).normalize_or_zero();
            // what this end was joined to, before
            let corner = meeting(&walls, me, p_old, d_in);
            if p_old.distance(p_new) > 1e-9 {
                for (o, d_out) in &corner {
                    // which of its ends met this one
                    let (wall, oe) = (o.id, *d_out == -o.wall.direction());
                    if moving.contains(&wall) {
                        continue;
                    }
                    let delta = p_new - p_old;
                    let e = out
                        .entry(wall)
                        .or_insert_with(|| before.get(wall).cloned().expect("a joined wall"));
                    if !order.contains(&wall) {
                        order.push(wall);
                    }
                    if let ElementKind::Wall(o) = &mut e.kind {
                        if oe {
                            o.end += delta;
                        } else {
                            o.start += delta;
                        }
                    }
                }
            }
            // a wall moved bodily keeps a T end on the wall it sat on
            if rigid && corner.is_empty() && p_old.distance(p_new) > 1e-9 {
                let host = walls
                    .iter()
                    .filter(|o| o.id != me.id && o.wall.level == ow.level)
                    .filter_map(|o| tee(p_old, d_in, me, o).map(|(_, off)| (o, off)))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(o, _)| o);
                if let Some(host) = host.filter(|h| !moving.contains(&h.id)) {
                    let d_new =
                        (p_new - if at_end { nw.start } else { nw.end }).normalize_or_zero();
                    if let Some(q) = slide_onto(p_old, p_new, d_new, host.wall, host.wall) {
                        if at_end {
                            nw.end = q;
                        } else {
                            nw.start = q;
                        }
                    }
                }
            }
        }
        // ends of other walls on this one's body slide along to stay on it
        for o in walls
            .iter()
            .filter(|o| o.id != me.id && o.wall.level == ow.level)
        {
            if moving.contains(&o.id) {
                continue;
            }
            for oe in [false, true] {
                let (q, q_other) = if oe {
                    (o.wall.end, o.wall.start)
                } else {
                    (o.wall.start, o.wall.end)
                };
                let d_q = (q - q_other).normalize_or_zero();
                if !meeting(&walls, o, q, d_q).is_empty() || tee(q, d_q, o, me).is_none() {
                    continue;
                }
                // the nearest host must be this wall
                let nearest = walls
                    .iter()
                    .filter(|h| h.id != o.id && h.wall.level == o.wall.level)
                    .filter_map(|h| tee(q, d_q, o, h).map(|(_, off)| (h.id, off)))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(h, _)| h);
                if nearest != Some(me.id) {
                    continue;
                }
                let current = out
                    .get(&o.id)
                    .and_then(|e| match &e.kind {
                        ElementKind::Wall(w) => Some(if oe { w.end } else { w.start }),
                        _ => None,
                    })
                    .unwrap_or(q);
                let Some(nq) = slide_onto(q, current, d_q, ow, nw) else {
                    continue;
                };
                let e = out
                    .entry(o.id)
                    .or_insert_with(|| before.get(o.id).cloned().expect("a joined wall"));
                if !order.contains(&o.id) {
                    order.push(o.id);
                }
                if let ElementKind::Wall(w) = &mut e.kind {
                    if oe {
                        w.end = nq;
                    } else {
                        w.start = nq;
                    }
                }
            }
        }
    }
    let mut result = vec![];
    for id in order {
        let (Some(b), Some(mut a)) = (before.get(id), out.remove(&id)) else {
            continue;
        };
        if let ElementKind::Wall(w) = &a.kind {
            if w.length() < 0.05 || a == *b {
                continue;
            }
        }
        // its doors and windows stay where they were along it
        if let (ElementKind::Wall(ob), ElementKind::Wall(oa)) = (&b.kind, &mut a.kind) {
            let shift = (oa.start - ob.start).dot(oa.direction());
            if shift.abs() > 1e-9 {
                for d in before.dependents(id) {
                    let Some(de) = before.get(d) else { continue };
                    let ElementKind::Opening(op) = &de.kind else {
                        continue;
                    };
                    let mut na = de.clone();
                    if let ElementKind::Opening(nop) = &mut na.kind {
                        let max = (oa.length() - op.width).max(0.0);
                        nop.offset = (op.offset - shift).clamp(0.0, max);
                    }
                    if na != *de {
                        result.push((de.clone(), na));
                    }
                }
            }
        }
        result.insert(0, (b.clone(), a));
    }
    result
}

/// An end that sat at `p_old`, `off` across `host_old`, slid along its own
/// line (through `p_now` along `dir`) to sit as far across `host_new`.
/// `None` when the line runs along the host or leaves its body.
fn slide_onto(
    p_old: DVec2,
    p_now: DVec2,
    dir: DVec2,
    host_old: &Wall,
    host_new: &Wall,
) -> Option<DVec2> {
    let off = (p_old - host_old.start).perp_dot(host_old.direction());
    let d = host_new.direction();
    // the line along the new host, `off` across it
    let line = host_new.start + d.perp() * -off;
    let s = meet(p_now, dir, line, d)?;
    let q = p_now + dir * s;
    let u = (q - host_new.start).dot(d);
    (u > TOL && u < host_new.length() - TOL && s.abs() < 50.0).then_some(q)
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
                    material: None,
                    assembly: None,
                    flip: false,
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

    /// The outer corner both walls' footprints reach.
    fn corners_close(m: &Model, a: ElementId, b: ElementId) -> bool {
        let j = wall_joins(m);
        let fa = j[&a].footprint(&wall(m, a));
        let fb = j[&b].footprint(&wall(m, b));
        fa.iter()
            .filter(|p| fb.iter().any(|q| p.distance(*q) < 1e-6))
            .count()
            == 2
    }

    #[test]
    fn a_corner_a_notch_apart_still_mitres() {
        // the second wall moved 60 mm in: its end no longer meets the first's
        let (m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(4.0, 0.0)),
            (DVec2::new(3.94, 0.0), DVec2::new(3.94, 3.0)),
        ]);
        assert!(corners_close(&m, ids[0], ids[1]));
        // far apart, they don't
        let (m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(4.0, 0.0)),
            (DVec2::new(3.0, 0.5), DVec2::new(3.0, 3.0)),
        ]);
        assert!(!corners_close(&m, ids[0], ids[1]));
    }

    #[test]
    fn moving_a_wall_takes_its_corners_along() {
        // a U: the right wall moves 0.3 m left, the top and bottom follow
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(5.0, 0.0)),
            (DVec2::new(5.0, 0.0), DVec2::new(5.0, 4.0)),
            (DVec2::new(5.0, 4.0), DVec2::new(0.0, 4.0)),
        ]);
        let right = m.get(ids[1]).unwrap();
        let mut moved = vec![crate::transform::transformed(
            &m,
            right,
            &crate::transform::Xform::Move(glam::DVec3::new(-0.3, 0.0, 0.0)),
        )
        .unwrap()];
        let others = follow(&m, &mut moved);
        assert_eq!(others.len(), 2);
        let mut doc = crate::Document::from_model(m.clone());
        let mut tx = Transaction::new("Move").replace(right.clone(), moved[0].clone());
        for (b, a) in others {
            tx = tx.replace(b, a);
        }
        doc.commit(tx).unwrap();
        let m2 = doc.model();
        assert!(wall(m2, ids[0]).end.distance(DVec2::new(4.7, 0.0)) < 1e-9);
        assert!(wall(m2, ids[2]).start.distance(DVec2::new(4.7, 4.0)) < 1e-9);
        assert!(corners_close(m2, ids[0], ids[1]) && corners_close(m2, ids[1], ids[2]));
    }

    #[test]
    fn walls_on_a_moved_wall_slide_to_stay_on_it() {
        // a partition tees into a wall that moves up 0.5 m
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(6.0, 0.0)),
            (DVec2::new(3.0, -4.0), DVec2::new(3.0, 0.0)),
        ]);
        let host = m.get(ids[0]).unwrap();
        let mut moved = vec![crate::transform::transformed(
            &m,
            host,
            &crate::transform::Xform::Move(glam::DVec3::new(0.0, 0.5, 0.0)),
        )
        .unwrap()];
        let others = follow(&m, &mut moved);
        assert_eq!(others.len(), 1);
        let ElementKind::Wall(p) = &others[0].1.kind else {
            unreachable!()
        };
        assert!(p.end.distance(DVec2::new(3.0, 0.5)) < 1e-9);
        // and a partition moved along its length keeps both ends on its walls
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(6.0, 0.0)),
            (DVec2::new(0.0, 4.0), DVec2::new(6.0, 4.0)),
            (DVec2::new(3.0, 0.0), DVec2::new(3.0, 4.0)),
        ]);
        let part = m.get(ids[2]).unwrap();
        let mut moved = vec![crate::transform::transformed(
            &m,
            part,
            &crate::transform::Xform::Move(glam::DVec3::new(1.0, 0.3, 0.0)),
        )
        .unwrap()];
        assert!(follow(&m, &mut moved).is_empty());
        let ElementKind::Wall(p) = &moved[0].kind else {
            unreachable!()
        };
        assert!(
            p.start.distance(DVec2::new(4.0, 0.0)) < 1e-9,
            "{:?}",
            p.start
        );
        assert!(p.end.distance(DVec2::new(4.0, 4.0)) < 1e-9, "{:?}", p.end);
    }
}
