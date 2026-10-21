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
//! Two walls that cross (an X) clean up too: the thicker one (or, as thick,
//! the one drawn first) runs through and the other stops at its faces on
//! either side, its core running on to the other's core when both are
//! layered. That leaves a [`Gap`] in the second wall.
//!
//! Ends don't have to coincide exactly: an end within a wall's thickness
//! of where the centrelines cross still joins, so a wall nudged a little
//! keeps clean corners. Moving a wall also takes the walls joined to it
//! along ([`follow`]), as in Revit: their ends stretch to stay joined.
//!
//! An end whose join is disallowed ([`Wall::disallow_join`]) joins nothing:
//! it stays square, butting into what it meets, and the walls it met no
//! longer follow it (nor it them).

use crate::{assembly, Element, ElementId, ElementKind, Model, Wall};
use glam::DVec2;
use std::collections::{HashMap, HashSet};

/// How far each face of a wall runs past its nominal end, in metres along the
/// wall. Index 0 is the face on the wall's left (start → end), index 1 the
/// face on its right. Negative values trim the wall.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WallEnds {
    pub start: [f64; 2],
    pub end: [f64; 2],
    /// How much further a layered wall's core runs than its faces, at the
    /// start and at the end: where it meets another layered wall's core
    /// through that wall's finishes.
    pub core: [f64; 2],
    /// Where walls crossing this one run through it, along it in order.
    pub gaps: Vec<Gap>,
}

/// Where another wall runs through a wall: the wall stops at `lo` and
/// starts again at `hi`, measured along it at its left face (index 0) and
/// its right face (index 1), so the cut is slanted when the walls cross
/// at an angle. A layered wall's core runs `core` further into the gap
/// from each side, to the core of the wall crossing it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gap {
    pub lo: [f64; 2],
    pub hi: [f64; 2],
    pub core: [f64; 2],
}

impl Gap {
    /// Where the gap starts on the line `s` across the wall.
    pub fn lo_at(&self, wall: &Wall, s: f64) -> f64 {
        let k = across(wall, s);
        self.lo[1] + (self.lo[0] - self.lo[1]) * k
    }

    /// Where the wall starts again on the line `s` across it.
    pub fn hi_at(&self, wall: &Wall, s: f64) -> f64 {
        let k = across(wall, s);
        self.hi[1] + (self.hi[0] - self.hi[1]) * k
    }

    /// Where the gap starts on the centreline: the `u` that stands for its
    /// start in drawings and meshes built along the wall.
    pub fn lo_mid(&self) -> f64 {
        (self.lo[0] + self.lo[1]) / 2.0
    }

    pub fn hi_mid(&self) -> f64 {
        (self.hi[0] + self.hi[1]) / 2.0
    }
}

/// One end of a piece of wall: the wall's own start or end, or one side
/// of a gap.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Bound {
    Start,
    End,
    /// The piece stops where gap `i` starts.
    GapLo(usize),
    /// The piece starts where gap `i` ends.
    GapHi(usize),
}

impl WallEnds {
    /// Where a mesh or drawing laid out along the wall's centreline puts
    /// the point `u` along it, on the line `s` across it: the wall's ends
    /// and the sides of its gaps (given by their centreline `u`) follow
    /// the joins; anything else stays at `u`. `core` for a layer that runs
    /// on past the faces.
    pub fn place(&self, wall: &Wall, u: f64, s: f64, core: bool) -> f64 {
        if u <= 0.0 {
            return self.layer_start(wall, s, core);
        }
        if u >= wall.length() {
            return self.layer_end(wall, s, core);
        }
        for g in &self.gaps {
            if (u - g.lo_mid()).abs() < 1e-9 {
                return g.lo_at(wall, s) + if core { g.core[0] } else { 0.0 };
            }
            if (u - g.hi_mid()).abs() < 1e-9 {
                return g.hi_at(wall, s) - if core { g.core[1] } else { 0.0 };
            }
        }
        u
    }

    /// The stretches of the wall between its gaps, as centreline `u`
    /// spans (see [`WallEnds::place`]): the whole wall when nothing crosses it.
    pub fn solid_spans(&self, wall: &Wall) -> Vec<(f64, f64)> {
        let mut out = vec![];
        let mut u = 0.0;
        for g in &self.gaps {
            out.push((u, g.lo_mid()));
            u = g.hi_mid();
        }
        out.push((u, wall.length()));
        out
    }

    /// Is centreline `u` inside a gap?
    pub fn in_gap(&self, u: f64) -> bool {
        self.gaps.iter().any(|g| u > g.lo_mid() && u < g.hi_mid())
    }

    fn bound_u(&self, wall: &Wall, b: Bound, s: f64) -> f64 {
        match b {
            Bound::Start => self.start_at(wall, s),
            Bound::End => self.end_at(wall, s),
            Bound::GapLo(i) => self.gaps[i].lo_at(wall, s),
            Bound::GapHi(i) => self.gaps[i].hi_at(wall, s),
        }
    }

    /// How much further the core runs past a piece's bound.
    fn bound_core(&self, b: Bound) -> f64 {
        match b {
            Bound::Start => self.core[0],
            Bound::End => self.core[1],
            Bound::GapLo(i) => self.gaps[i].core[0],
            Bound::GapHi(i) => self.gaps[i].core[1],
        }
    }

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
    /// Crossing walls' gaps are left out of it; see [`WallEnds::local_outlines`].
    pub fn local_outline(&self, wall: &Wall, core: (f64, f64)) -> Vec<DVec2> {
        self.piece_outline(wall, core, Bound::Start, Bound::End)
    }

    /// The footprint in wall-local `(u, s)` as one outline per piece
    /// between the gaps crossing walls leave, each like
    /// [`WallEnds::local_outline`].
    pub fn local_outlines(&self, wall: &Wall, core: (f64, f64)) -> Vec<Vec<DVec2>> {
        let mut bounds = vec![Bound::Start];
        for i in 0..self.gaps.len() {
            bounds.push(Bound::GapLo(i));
            bounds.push(Bound::GapHi(i));
        }
        bounds.push(Bound::End);
        bounds
            .chunks(2)
            .map(|b| self.piece_outline(wall, core, b[0], b[1]))
            .collect()
    }

    fn piece_outline(&self, wall: &Wall, core: (f64, f64), from: Bound, to: Bound) -> Vec<DVec2> {
        let t = wall.thickness / 2.0;
        let p = DVec2::new;
        let (c0, c1) = core;
        let (f, e) = (self.bound_core(from), self.bound_core(to));
        let at = |b: Bound, s: f64| self.bound_u(wall, b, s);
        // along the right face, up the far end, back along the left face
        // and down the start
        let mut out = vec![p(at(from, -t), -t), p(at(to, -t), -t)];
        if e > 1e-9 {
            out.extend([
                p(at(to, c0), c0),
                p(at(to, c0) + e, c0),
                p(at(to, c1) + e, c1),
                p(at(to, c1), c1),
            ]);
        }
        out.extend([p(at(to, t), t), p(at(from, t), t)]);
        if f > 1e-9 {
            out.extend([
                p(at(from, c1), c1),
                p(at(from, c1) - f, c1),
                p(at(from, c0) - f, c0),
                p(at(from, c0), c0),
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
        for (q, d_out, disallowed) in [
            (o.wall.start, o.wall.direction(), o.wall.disallow_join[0]),
            (o.wall.end, -o.wall.direction(), o.wall.disallow_join[1]),
        ] {
            if disallowed {
                continue;
            }
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
    for (i, me) in walls.iter().enumerate() {
        let w = me.wall;
        let mut ends = WallEnds::default();
        // walls running through this one, the thicker (or first drawn) winning
        for (j, o) in walls.iter().enumerate() {
            if o.id == me.id || o.wall.level != w.level {
                continue;
            }
            let wins = o.wall.thickness > w.thickness + 1e-9
                || ((o.wall.thickness - w.thickness).abs() <= 1e-9 && j < i);
            if wins {
                ends.gaps.extend(crossing_gap(me, o));
            }
        }
        ends.gaps.sort_by(|a, b| a.lo_mid().total_cmp(&b.lo_mid()));
        // gaps that overlap (walls crossing side by side) become one
        ends.gaps.dedup_by(|b, a| {
            if b.lo_mid() <= a.hi_mid() {
                a.hi = [a.hi[0].max(b.hi[0]), a.hi[1].max(b.hi[1])];
                a.core[1] = b.core[1];
                true
            } else {
                false
            }
        });
        for at_end in [false, true] {
            if w.disallow_join[usize::from(at_end)] {
                continue; // a square end, butting into what it meets
            }
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

/// The gap wall `o` leaves running through `me`, when the two cross: each
/// passes right through the other, both its ends well clear of the other
/// (an end closer than that makes a T or an L instead).
fn crossing_gap(me: &Joinable, o: &Joinable) -> Option<Gap> {
    let (w, ow) = (me.wall, o.wall);
    let (d, od) = (w.direction(), ow.direction());
    if d.perp_dot(od).abs() < 0.05 {
        return None;
    }
    // both ends of `a` on either side of `b`, further off than a T reaches
    let clear = |a: &Wall, b: &Wall| {
        let off = |p: DVec2| (p - b.start).perp_dot(b.direction());
        let (s0, s1) = (off(a.start), off(a.end));
        let reach = b.thickness / 2.0 + a.thickness + TOL;
        s0 * s1 < 0.0 && s0.abs() > reach && s1.abs() > reach
    };
    if !clear(w, ow) || !clear(ow, w) {
        return None;
    }
    let (n, on) = (d.perp(), od.perp());
    let (t, ot) = (w.thickness / 2.0, ow.thickness / 2.0);
    // where a line `s` across this wall meets one `so` across the other
    let u = |s: f64, so: f64| meet(w.start + n * s, d, ow.start + on * so, od);
    let mut lo = [0.0; 2];
    let mut hi = [0.0; 2];
    for (k, s) in [t, -t].into_iter().enumerate() {
        let (a, b) = (u(s, ot)?, u(s, -ot)?);
        lo[k] = a.min(b);
        hi[k] = a.max(b);
    }
    let len = w.length();
    if lo.iter().chain(&hi).any(|v| *v <= TOL || *v >= len - TOL) {
        return None;
    }
    // layered through layered: the core runs on to the other's core
    let core = match (me.core, o.core) {
        (Some(_), Some((c0, c1))) => {
            let (f0, f1) = (u(0.0, ot)?, u(0.0, -ot)?);
            let (k0, k1) = (u(0.0, c0)?, u(0.0, c1)?);
            [
                (k0.min(k1) - f0.min(f1)).max(0.0),
                (f0.max(f1) - k0.max(k1)).max(0.0),
            ]
        }
        _ => [0.0; 2],
    };
    Some(Gap { lo, hi, core })
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
            // an end whose join is disallowed takes nothing along
            if ow.disallow_join[usize::from(at_end)] {
                continue;
            }
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
                if o.wall.disallow_join[usize::from(oe)] {
                    continue;
                }
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
            let e = Element::new("W", ElementKind::Wall(Wall::new(lid, *a, *b, 0.2, 3.0)));
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
        let a = j[&ids[0]].clone();
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
        let b = j[&ids[1]].clone();
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

    fn set_wall(m: &Model, id: ElementId, f: impl FnOnce(&mut Wall)) -> Model {
        let mut doc = crate::Document::from_model(m.clone());
        doc.edit(id, "edit", |e| {
            if let ElementKind::Wall(w) = &mut e.kind {
                f(w)
            }
        })
        .unwrap();
        doc.model().clone()
    }

    #[test]
    fn crossing_walls_make_a_cross() {
        // the first wall runs through; the second stops at its faces
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(6.0, 0.0)),
            (DVec2::new(3.0, -2.0), DVec2::new(3.0, 2.0)),
        ]);
        let j = wall_joins(&m);
        assert!(j[&ids[0]].gaps.is_empty());
        let g = j[&ids[1]].gaps.clone();
        assert_eq!(g.len(), 1);
        // the second wall runs up from y = -2: the first's faces are at ±0.1
        assert!((g[0].lo_mid() - 1.9).abs() < 1e-9 && (g[0].hi_mid() - 2.1).abs() < 1e-9);
        let pieces = j[&ids[1]].local_outlines(&wall(&m, ids[1]), (-0.1, 0.1));
        assert_eq!(pieces.len(), 2);
        // the thicker wall wins whatever the order
        let m = set_wall(&m, ids[1], |w| w.thickness = 0.3);
        let j = wall_joins(&m);
        assert_eq!((j[&ids[0]].gaps.len(), j[&ids[1]].gaps.len()), (1, 0));
        // at an angle the gap is slanted, from face to face
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(6.0, 0.0)),
            (DVec2::new(1.0, -2.0), DVec2::new(5.0, 2.0)),
        ]);
        let j = wall_joins(&m);
        let g = j[&ids[1]].gaps[0];
        let w = wall(&m, ids[1]);
        let face = |u: f64, s: f64| w.start + w.direction() * u + w.direction().perp() * s;
        for (k, s) in [(0, 0.1), (1, -0.1)] {
            assert!((face(g.lo[k], s).y + 0.1).abs() < 1e-9);
            assert!((face(g.hi[k], s).y - 0.1).abs() < 1e-9);
        }
    }

    #[test]
    fn a_wall_ending_on_another_is_a_tee_not_a_cross() {
        // it stops just past the far face: still trimmed to the near face
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(6.0, 0.0)),
            (DVec2::new(3.0, -3.0), DVec2::new(3.0, 0.15)),
        ]);
        let j = wall_joins(&m);
        assert!(j[&ids[1]].gaps.is_empty() && j[&ids[0]].gaps.is_empty());
        assert!((j[&ids[1]].end[0] + 0.25).abs() < 1e-9);
    }

    #[test]
    fn a_disallowed_join_leaves_the_end_square_and_free() {
        let (m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(4.0, 0.0)),
            (DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0)),
        ]);
        let m = set_wall(&m, ids[0], |w| w.disallow_join = [false, true]);
        let j = wall_joins(&m);
        assert_eq!(j[&ids[0]], WallEnds::default());
        // the other wall no longer mitres with it either
        assert_eq!(j[&ids[1]].start, [0.0, 0.0]);
        // moving the first wall leaves the second where it is
        let first = m.get(ids[0]).unwrap();
        let mut moved = vec![crate::transform::transformed(
            &m,
            first,
            &crate::transform::Xform::Move(glam::DVec3::new(-0.5, 0.0, 0.0)),
        )
        .unwrap()];
        assert!(follow(&m, &mut moved).is_empty());
        // nor does moving the second take the first along
        let second = m.get(ids[1]).unwrap();
        let mut moved = vec![crate::transform::transformed(
            &m,
            second,
            &crate::transform::Xform::Move(glam::DVec3::new(0.5, 0.0, 0.0)),
        )
        .unwrap()];
        assert!(follow(&m, &mut moved).is_empty());
    }
}
