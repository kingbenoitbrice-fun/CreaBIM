//! Slab outlines found from the model: the room enclosed by walls around a
//! point, and the outline of a flat sketch.

use crate::{
    sketch::{SegmentKind, Sketch},
    ElementId, ElementKind, Model, Slab,
};
use glam::DVec2;
use std::collections::HashSet;

const TOL: f64 = 1e-6;

/// The outline, counter-clockwise, of the smallest space around `p` that
/// the centre lines of the walls on `level` close off; `None` when `p` is
/// not enclosed. Walls that stop at another wall's face (short of its centre
/// line) still close the space; loose ends are ignored.
pub fn room_outline(model: &Model, level: ElementId, p: DVec2) -> Option<Vec<DVec2>> {
    let walls: Vec<(DVec2, DVec2, f64)> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Wall(w) if w.level == level && w.length() > 1e-3 => {
                Some((w.start, w.end, w.thickness))
            }
            _ => None,
        })
        .collect();
    let reach = walls.iter().map(|w| w.2 / 2.0).fold(0.0, f64::max) + 0.01;
    // centre lines, run on far enough to meet the centre line of a wall
    // they stop against
    let segs: Vec<(DVec2, DVec2)> = walls
        .iter()
        .map(|&(a, b, _)| {
            let d = (b - a).normalize();
            (a - d * reach, b + d * reach)
        })
        .collect();
    room_in_lines(&segs, p)
}

/// The smallest closed face of the line arrangement `segs` around `p`.
pub(crate) fn room_in_lines(segs: &[(DVec2, DVec2)], p: DVec2) -> Option<Vec<DVec2>> {
    // split every segment where others cross it
    let mut nodes: Vec<DVec2> = Vec::new();
    let mut node = |q: DVec2| -> usize {
        match nodes.iter().position(|n| n.distance(q) < 1e-6) {
            Some(i) => i,
            None => {
                nodes.push(q);
                nodes.len() - 1
            }
        }
    };
    let mut edges: HashSet<(usize, usize)> = HashSet::new();
    for (i, &(a, b)) in segs.iter().enumerate() {
        let mut ts = vec![0.0, 1.0];
        for (j, &(c, d)) in segs.iter().enumerate() {
            if i != j {
                if let Some(t) = crossing(a, b, c, d) {
                    ts.push(t);
                }
                // and where another one, running along it, ends on it
                let len2 = (b - a).length_squared();
                for q in [c, d] {
                    let t = (q - a).dot(b - a) / len2;
                    if (0.0..=1.0).contains(&t) && a.lerp(b, t).distance(q) < 1e-9 {
                        ts.push(t);
                    }
                }
            }
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() * a.distance(b) < 1e-6);
        let ids: Vec<usize> = ts.iter().map(|t| node(a.lerp(b, *t))).collect();
        for w in ids.windows(2) {
            if w[0] != w[1] {
                edges.insert((w[0].min(w[1]), w[0].max(w[1])));
            }
        }
    }
    // loose ends bound nothing
    loop {
        let mut degree = vec![0usize; nodes.len()];
        for &(u, v) in &edges {
            degree[u] += 1;
            degree[v] += 1;
        }
        let before = edges.len();
        edges.retain(|&(u, v)| degree[u] > 1 && degree[v] > 1);
        if edges.len() == before {
            break;
        }
    }
    let mut around: Vec<Vec<usize>> = vec![vec![]; nodes.len()];
    for &(u, v) in &edges {
        around[u].push(v);
        around[v].push(u);
    }
    // walk each face with it on the left: at each corner take the first
    // edge clockwise from the way back
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut best: Option<(f64, Vec<DVec2>)> = None;
    for &(u0, v0) in &edges {
        for (u, v) in [(u0, v0), (v0, u0)] {
            if seen.contains(&(u, v)) {
                continue;
            }
            let mut face = vec![];
            let (mut a, mut b) = (u, v);
            while seen.insert((a, b)) {
                face.push(nodes[a]);
                let back = (nodes[a] - nodes[b]).to_angle();
                let next = around[b]
                    .iter()
                    .copied()
                    .filter(|&w| w != a || around[b].len() == 1)
                    .min_by(|&x, &y| {
                        let cw = |w: usize| {
                            (back - (nodes[w] - nodes[b]).to_angle())
                                .rem_euclid(std::f64::consts::TAU)
                        };
                        cw(x).total_cmp(&cw(y))
                    })?;
                (a, b) = (b, next);
            }
            let area = crate::polygon::signed_area2(&face) / 2.0;
            if area > TOL && contains(&face, p) && best.as_ref().is_none_or(|(ba, _)| area < *ba) {
                best = Some((area, face));
            }
        }
    }
    best.map(|(_, face)| drop_straight_corners(face))
}

/// Where segment ab meets segment cd, as a fraction along ab.
fn crossing(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<f64> {
    let (r, s) = (b - a, d - c);
    let den = r.perp_dot(s);
    if den.abs() < 1e-12 {
        return None;
    }
    let t = (c - a).perp_dot(s) / den;
    let u = (c - a).perp_dot(r) / den;
    let e = 1e-9;
    ((-e..=1.0 + e).contains(&t) && (-e..=1.0 + e).contains(&u)).then_some(t.clamp(0.0, 1.0))
}

fn contains(poly: &[DVec2], p: DVec2) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

/// The polygon without corners where it runs straight on.
fn drop_straight_corners(mut poly: Vec<DVec2>) -> Vec<DVec2> {
    let mut i = 0;
    while poly.len() > 3 && i < poly.len() {
        let n = poly.len();
        let (a, b, c) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
        let (u, v) = ((b - a).normalize(), (c - b).normalize());
        if u.perp_dot(v).abs() < 1e-9 && u.dot(v) > 0.0 {
            poly.remove(i);
        } else {
            i += 1;
        }
    }
    poly
}

/// A slab with the outline of a closed sketch that lies flat, its top on
/// the sketch's plane, arcs kept. The slab hangs off the sketch's level, or
/// the level nearest below it.
pub fn slab_from_sketch(model: &Model, sk: &Sketch, thickness: f64) -> Result<Slab, String> {
    let normal = sk.plane.normal();
    if normal.z.abs() < 0.999 {
        return Err("The sketch must lie flat (on a level or a floor) to become a slab.".into());
    }
    if !sk.closed {
        return Err("Close the sketch first: a slab needs a closed outline.".into());
    }
    sk.check()?;
    let elevation = |l: Option<ElementId>| l.and_then(|l| model.level(l)).map(|l| l.elevation);
    // the plane's height is relative to the sketch's level
    let z = sk.plane.origin.z + elevation(sk.level).unwrap_or(0.0);
    let level = sk
        .level
        .filter(|l| model.level(*l).is_some())
        .or_else(|| {
            let levels: Vec<(ElementId, f64)> =
                model.levels().map(|(e, l)| (e.id, l.elevation)).collect();
            levels
                .iter()
                .filter(|(_, el)| *el <= z + 1e-6)
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .or_else(|| levels.iter().min_by(|a, b| a.1.total_cmp(&b.1)))
                .map(|(l, _)| *l)
        })
        .ok_or("Add a level first.")?;
    // a plane facing down mirrors the sketch's own axes, so its arcs turn
    // the other way in plan
    let flip = if normal.z < 0.0 { -1.0 } else { 1.0 };
    let plan = |q: DVec2| sk.plane.to_model(q).truncate();
    let (boundary, bulges) = match sk.circle {
        Some(c) => {
            let r = DVec2::new(c.radius, 0.0);
            (vec![plan(c.center + r), plan(c.center - r)], vec![flip; 2])
        }
        None => {
            let bulges = (0..sk.points.len())
                .map(|i| match sk.kind(i) {
                    SegmentKind::Line => 0.0,
                    SegmentKind::Arc { bulge } => bulge * flip,
                })
                .collect();
            (sk.points.iter().map(|q| plan(*q)).collect(), bulges)
        }
    };
    let mut slab = Slab::new(level, boundary, thickness);
    slab.top_offset = z - elevation(Some(level)).unwrap_or(0.0);
    if bulges.iter().any(|b: &f64| b.abs() > 1e-9) {
        slab.bulges = bulges;
    }
    slab.make_ccw();
    Ok(slab)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sample, Element, Level, Transaction, Wall, Workplane};
    use glam::DVec3;

    fn wall(level: ElementId, a: (f64, f64), b: (f64, f64)) -> Element {
        Element::new(
            "Wall",
            ElementKind::Wall(Wall {
                level,
                start: DVec2::new(a.0, a.1),
                end: DVec2::new(b.0, b.1),
                thickness: 0.2,
                height: 3.0,
                base_offset: 0.0,
                material: None,
                assembly: None,
                flip: false,
                location: Default::default(),
                top: Default::default(),
                disallow_join: [false; 2],
                attach_top: vec![],
                curtain: None,
            }),
        )
    }

    fn level_model() -> (Model, ElementId) {
        let lv = Element::new(
            "Ground",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        );
        let id = lv.id;
        (Model::from_elements([lv]).unwrap(), id)
    }

    #[test]
    fn four_walls_close_a_room_to_their_centre_lines() {
        let (mut m, l) = level_model();
        let mut tx = Transaction::new("walls");
        for (a, b) in [
            ((0.0, 0.0), (6.0, 0.0)),
            ((6.0, 0.0), (6.0, 4.0)),
            ((6.0, 4.0), (0.0, 4.0)),
            ((0.0, 4.0), (0.0, 0.0)),
        ] {
            tx = tx.insert(wall(l, a, b));
        }
        tx.apply(&mut m).unwrap();
        let room = room_outline(&m, l, DVec2::new(2.0, 1.0)).expect("enclosed");
        assert_eq!(room.len(), 4);
        assert!((crate::polygon::signed_area2(&room) / 2.0 - 24.0).abs() < 1e-9);
        for c in [(0.0, 0.0), (6.0, 0.0), (6.0, 4.0), (0.0, 4.0)] {
            assert!(room.iter().any(|q| q.distance(DVec2::new(c.0, c.1)) < 1e-9));
        }
        assert!(room_outline(&m, l, DVec2::new(7.0, 1.0)).is_none());

        // a wall across it, stopping at the outer walls' faces, splits it
        Transaction::new("split")
            .insert(wall(l, (3.0, 0.1), (3.0, 3.9)))
            .apply(&mut m)
            .unwrap();
        let left = room_outline(&m, l, DVec2::new(1.0, 1.0)).expect("enclosed");
        assert!((crate::polygon::signed_area2(&left) / 2.0 - 12.0).abs() < 1e-9);
        assert_eq!(left.len(), 4);
    }

    #[test]
    fn three_walls_leave_the_room_open() {
        let (mut m, l) = level_model();
        let mut tx = Transaction::new("walls");
        for (a, b) in [
            ((0.0, 0.0), (6.0, 0.0)),
            ((6.0, 0.0), (6.0, 4.0)),
            ((6.0, 4.0), (0.0, 4.0)),
        ] {
            tx = tx.insert(wall(l, a, b));
        }
        tx.apply(&mut m).unwrap();
        assert!(room_outline(&m, l, DVec2::new(2.0, 1.0)).is_none());
    }

    #[test]
    fn the_sample_house_has_rooms() {
        let m = sample::sample_house();
        let (gf, _) = m
            .levels()
            .min_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation))
            .unwrap();
        let mut found = false;
        for e in m.iter() {
            if let ElementKind::Wall(w) = &e.kind {
                if w.level == gf.id {
                    let mid = (w.start + w.end) / 2.0 + w.direction().perp() * 0.5;
                    found |= room_outline(&m, gf.id, mid).is_some();
                }
            }
        }
        assert!(found);
    }

    #[test]
    fn slabs_saved_before_top_offsets_and_arcs_load() {
        let level = ElementId::new();
        let json = format!(
            r#"{{"level":"{}","boundary":[[0.0,0.0],[3.0,0.0],[3.0,2.0]],"thickness":0.25,"material":null}}"#,
            level.0
        );
        let s: Slab = serde_json::from_str(&json).unwrap();
        assert_eq!(s, Slab::new(level, s.boundary.clone(), 0.25));
        assert_eq!(s.top_offset, 0.0);
        assert!(s.bulges.is_empty() && s.outline() == s.boundary);
        // and saved again, a slab without arcs writes no bulges
        let again = serde_json::to_string(&s).unwrap();
        assert!(!again.contains("bulges"));
        assert_eq!(serde_json::from_str::<Slab>(&again).unwrap(), s);
    }

    #[test]
    fn thickness_keeps_the_top_or_the_bottom() {
        let mut s = Slab::new(ElementId::new(), vec![DVec2::ZERO, DVec2::X, DVec2::Y], 0.2);
        s.set_thickness(0.3, false);
        assert_eq!((s.top_offset, s.thickness), (0.0, 0.3));
        let bottom = s.top_offset - s.thickness;
        s.set_thickness(0.5, true);
        assert!((s.top_offset - s.thickness - bottom).abs() < 1e-12);
        assert!((s.top_offset - 0.2).abs() < 1e-12);
    }

    #[test]
    fn arcs_follow_reversing_splitting_and_removing_corners() {
        let mut s = Slab::new(
            ElementId::new(),
            vec![
                DVec2::new(0.0, 0.0),
                DVec2::new(4.0, 0.0),
                DVec2::new(4.0, 2.0),
                DVec2::new(0.0, 2.0),
            ],
            0.2,
        );
        s.bulges = vec![0.0, 1.0, 0.0, 0.0];
        let area = s.area();
        let outline = s.outline();
        s.reverse();
        assert!((s.area() - area).abs() < 1e-9);
        assert!(crate::polygon::signed_area2(&s.outline()) < 0.0);
        for p in s.outline() {
            assert!(outline.iter().any(|q| q.distance(p) < 1e-9));
        }
        s.make_ccw();
        s.split_edge(1);
        assert_eq!(s.boundary.len(), 5);
        assert!(s.boundary[2].distance(DVec2::new(5.0, 1.0)) < 1e-9);
        assert!((s.area() - area).abs() < 0.01);
        s.remove_corner(2);
        assert_eq!(s.boundary.len(), 4);
        assert!((s.area() - 8.0).abs() < 1e-9);
    }

    #[test]
    fn a_flat_sketch_becomes_a_slab_with_its_arcs() {
        let (m, l) = level_model();
        // a D on a plane 0.5 m above the level: a straight side and a half circle
        let plane = Workplane {
            origin: DVec3::new(1.0, 1.0, 0.5),
            ..Workplane::ground(0.5)
        };
        let mut sk = Sketch::polyline(
            Some(l),
            plane,
            vec![DVec2::new(0.0, 2.0), DVec2::new(0.0, -2.0)],
            true,
        );
        sk.kinds = vec![SegmentKind::Line, SegmentKind::Arc { bulge: 1.0 }];
        let s = slab_from_sketch(&m, &sk, 0.2).unwrap();
        assert_eq!(s.level, l);
        assert!((s.top_offset - 0.5).abs() < 1e-12);
        assert_eq!(s.boundary.len(), 2);
        assert!(s.has_arcs());
        assert!((s.area() - std::f64::consts::PI * 2.0).abs() < 0.05);
        assert!(crate::polygon::signed_area2(&s.outline()) > 0.0);
        assert!(s
            .boundary
            .iter()
            .any(|p| p.distance(DVec2::new(1.0, 3.0)) < 1e-12));
        Element::new("Slab", ElementKind::Slab(s))
            .validate()
            .unwrap();

        // a circle, and a sketch that isn't flat or closed
        let c = Sketch::circle(Some(l), Workplane::ground(0.0), DVec2::ZERO, 1.0);
        let s = slab_from_sketch(&m, &c, 0.2).unwrap();
        assert!((s.area() - std::f64::consts::PI).abs() < 0.01);
        let wall_plane = Workplane::facing(DVec3::ZERO, DVec3::Y);
        let up = Sketch::circle(Some(l), wall_plane, DVec2::ZERO, 1.0);
        assert!(slab_from_sketch(&m, &up, 0.2).is_err());
        sk.closed = false;
        assert!(slab_from_sketch(&m, &sk, 0.2).is_err());
    }
}

/// A shaft: a vertical hole through every slab and roof between its base
/// (`base_offset` above `level`) and its top (`top_offset` above
/// `top_level`), for a stair well, a lift or a riser. Like Revit's shaft
/// opening, it is one element: move or reshape it and every floor it
/// passes through follows.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Shaft {
    pub level: ElementId,
    /// Its outline in plan, counter-clockwise.
    pub boundary: Vec<DVec2>,
    #[serde(default)]
    pub base_offset: f64,
    /// The level its top is tied to; none (or one that is gone) stops it at
    /// its own level's height above its base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_level: Option<ElementId>,
    #[serde(default)]
    pub top_offset: f64,
}

impl Shaft {
    pub fn new(level: ElementId, boundary: Vec<DVec2>, top_level: Option<ElementId>) -> Self {
        let mut boundary = boundary;
        if crate::polygon::signed_area2(&boundary) < 0.0 {
            boundary.reverse();
        }
        Self {
            level,
            boundary,
            base_offset: 0.0,
            top_level,
            top_offset: 0.0,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.boundary.len() < 3 || self.boundary.iter().any(|p| !p.is_finite()) {
            return Err("a shaft needs an outline of at least 3 points".into());
        }
        if crate::polygon::signed_area2(&self.boundary).abs() < 1e-6 {
            return Err("a shaft's outline has no area".into());
        }
        if !(self.base_offset.is_finite() && self.top_offset.is_finite()) {
            return Err("shaft offsets must be numbers".into());
        }
        Ok(())
    }

    /// Absolute heights of its base and its top.
    pub fn span(&self, model: &Model) -> (f64, f64) {
        let own = model.level(self.level);
        let z0 = own.map_or(0.0, |l| l.elevation) + self.base_offset;
        let z1 = match self.top_level.and_then(|t| model.level(t)) {
            Some(t) => t.elevation + self.top_offset,
            None => z0 + own.map_or(3.0, |l| l.height) + self.top_offset,
        };
        (z0, z1.max(z0 + 0.01))
    }
}

/// Holes must each be a polygon of at least 3 finite corners with an area.
pub fn check_holes(holes: &[Vec<DVec2>]) -> Result<(), String> {
    for h in holes {
        if h.len() < 3 || h.iter().any(|p| !p.is_finite()) {
            return Err("a hole needs at least 3 corners".into());
        }
        if crate::polygon::signed_area2(h).abs() < 1e-8 {
            return Err("a hole has no area".into());
        }
    }
    Ok(())
}

/// What is wrong with an outline and its holes, if anything: an outline or
/// hole crossing itself, a hole outside or across the outline, two holes
/// overlapping. `closed` is false for an open path (a railing's), which
/// has no holes. Checked live while an outline is reshaped, so a shape
/// like this is never kept.
pub fn outline_problem(outline: &[DVec2], closed: bool, holes: &[Vec<DVec2>]) -> Option<String> {
    use crate::polygon::{inside, outlines_cross, overlap, self_intersects};
    if self_intersects(outline, closed) {
        return Some("the outline would cross itself".into());
    }
    for (k, h) in holes.iter().enumerate() {
        if self_intersects(h, true) {
            return Some(format!("hole {} would cross itself", k + 1));
        }
        if closed && outlines_cross(h, outline) {
            return Some(format!("hole {} would cross the outline", k + 1));
        }
        if closed && !inside(h, outline) {
            return Some(format!("hole {} would be outside the outline", k + 1));
        }
        if let Some(j) = holes[..k].iter().position(|o| overlap(o, h)) {
            return Some(format!("holes {} and {} would overlap", j + 1, k + 1));
        }
    }
    None
}

/// The slabs and roofs a shaft passes through but doesn't cut because it
/// crosses their edge: it only cuts those whose outline holds it whole.
pub fn shaft_misses(model: &Model, shaft: &Shaft) -> Vec<ElementId> {
    let (b, t) = shaft.span(model);
    model
        .iter()
        .filter(|e| {
            let (outline, z0, z1) = match &e.kind {
                ElementKind::Slab(s) => {
                    let top = s.top(model.level(s.level).map_or(0.0, |l| l.elevation));
                    (s.outline(), top - s.thickness, top)
                }
                ElementKind::Roof(r) => {
                    let base = r.base(model);
                    (
                        r.eave_outline(),
                        base - r.vertical_thickness(),
                        base + r.peak(),
                    )
                }
                _ => return false,
            };
            let through = b < z1 - 1e-6 && t > z0 + 1e-6;
            through
                && crate::polygon::overlap(&shaft.boundary, &outline)
                && !shaft
                    .boundary
                    .iter()
                    .all(|p| crate::polygon::contains(&outline, *p))
        })
        .map(|e| e.id)
        .collect()
}

/// The outlines of the model's shafts passing through the heights `z0..z1`.
fn shafts_through(model: &Model, z0: f64, z1: f64) -> Vec<Vec<DVec2>> {
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Shaft(s) => {
                let (b, t) = s.span(model);
                (b < z1 - 1e-6 && t > z0 + 1e-6).then(|| s.boundary.clone())
            }
            _ => None,
        })
        .collect()
}

/// Every hole through a slab or roof as built: its own holes, and the
/// shafts passing through it that lie inside its outline. Empty for other
/// elements.
pub fn holes_of(model: &Model, e: &crate::Element) -> Vec<Vec<DVec2>> {
    let (outline, mut holes, z0, z1) = match &e.kind {
        ElementKind::Slab(s) => {
            let top = s.top(model.level(s.level).map_or(0.0, |l| l.elevation));
            (s.outline(), s.holes.clone(), top - s.thickness, top)
        }
        ElementKind::Roof(r) => {
            let base = r.base(model);
            (
                r.eave_outline(),
                r.holes.clone(),
                base - r.vertical_thickness(),
                base + r.peak(),
            )
        }
        _ => return vec![],
    };
    for h in shafts_through(model, z0, z1) {
        if h.iter().all(|p| crate::polygon::contains(&outline, *p)) {
            holes.push(h);
        }
    }
    holes.retain(|h| h.len() >= 3);
    holes
}

/// The slabs and roofs a shaft cuts.
pub fn cut_by_shaft(model: &Model, shaft: &Shaft) -> Vec<ElementId> {
    model
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Slab(_) | ElementKind::Roof(_)))
        .filter(|e| holes_of(model, e).iter().any(|h| h == &shaft.boundary))
        .map(|e| e.id)
        .collect()
}

#[cfg(test)]
mod shaft_tests {
    use super::*;
    use crate::{Element, Level, Transaction};

    #[test]
    fn a_shaft_cuts_the_slabs_between_its_levels_and_old_slabs_have_no_holes() {
        let mut model = Model::new();
        let levels: Vec<Element> = [0.0, 3.0, 6.0]
            .iter()
            .map(|z| {
                Element::new(
                    format!("L{z}"),
                    ElementKind::Level(Level {
                        elevation: *z,
                        height: 3.0,
                    }),
                )
            })
            .collect();
        let mut tx = Transaction::new("setup");
        for l in &levels {
            tx = tx.insert(l.clone());
        }
        let outline = crate::polygon::rectangle(DVec2::ZERO, DVec2::new(8.0, 6.0));
        let slabs: Vec<Element> = levels
            .iter()
            .map(|l| {
                Element::new(
                    "Floor",
                    ElementKind::Slab(Slab::new(l.id, outline.clone(), 0.25)),
                )
            })
            .collect();
        for s in &slabs {
            tx = tx.insert(s.clone());
        }
        let well = crate::polygon::rectangle(DVec2::new(1.0, 1.0), DVec2::new(3.0, 4.0));
        let shaft = Shaft::new(levels[0].id, well.clone(), Some(levels[2].id));
        tx = tx.insert(Element::new("Shaft", ElementKind::Shaft(shaft.clone())));
        tx.apply(&mut model).unwrap();
        // the ground slab is under the shaft's base; the floors above are cut
        let cut = cut_by_shaft(&model, &shaft);
        assert_eq!(cut, vec![slabs[1].id, slabs[2].id]);
        // own holes count against the area, and round-trip
        let mut s = Slab::new(levels[0].id, outline.clone(), 0.25);
        s.holes = vec![well.clone()];
        assert!((s.area() - 42.0).abs() < 1e-9);
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Slab>(&text).unwrap(), s);
        let plain = serde_json::to_string(&Slab::new(levels[0].id, outline, 0.25)).unwrap();
        assert!(!plain.contains("holes"));
        assert!(check_holes(&[vec![DVec2::ZERO, DVec2::X]]).is_err());
    }

    #[test]
    fn a_shaft_across_a_slab_edge_is_reported_not_silently_ignored() {
        let mut model = Model::new();
        let level = Element::new(
            "L0",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        );
        let upper = Element::new(
            "L1",
            ElementKind::Level(Level {
                elevation: 3.0,
                height: 3.0,
            }),
        );
        let outline = crate::polygon::rectangle(DVec2::ZERO, DVec2::new(8.0, 6.0));
        let floor = Element::new(
            "Floor",
            ElementKind::Slab(Slab::new(upper.id, outline.clone(), 0.25)),
        );
        // half in, half out of the floor above
        let well = crate::polygon::rectangle(DVec2::new(7.0, 1.0), DVec2::new(9.0, 3.0));
        let shaft = Shaft::new(level.id, well, Some(upper.id));
        let tx = Transaction::new("setup")
            .insert(level.clone())
            .insert(upper.clone())
            .insert(floor.clone())
            .insert(Element::new("Shaft", ElementKind::Shaft(shaft.clone())));
        tx.apply(&mut model).unwrap();
        assert!(cut_by_shaft(&model, &shaft).is_empty());
        assert_eq!(shaft_misses(&model, &shaft), vec![floor.id]);
        // moved inside, it cuts and nothing is reported
        let inner = Shaft::new(
            level.id,
            crate::polygon::rectangle(DVec2::new(5.0, 1.0), DVec2::new(6.0, 3.0)),
            Some(upper.id),
        );
        assert!(shaft_misses(&model, &inner).is_empty());
    }

    #[test]
    fn crossing_outlines_and_stray_holes_are_problems() {
        let sq = crate::polygon::rectangle(DVec2::ZERO, DVec2::new(4.0, 4.0));
        let h = |a: f64, b: f64, c: f64, d: f64| {
            crate::polygon::rectangle(DVec2::new(a, b), DVec2::new(c, d))
        };
        assert!(outline_problem(&sq, true, &[h(1.0, 1.0, 2.0, 2.0)]).is_none());
        let bow = vec![
            DVec2::ZERO,
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(0.0, 4.0),
        ];
        assert!(outline_problem(&bow, true, &[])
            .unwrap()
            .contains("cross itself"));
        assert!(outline_problem(&sq, true, &[h(3.0, 1.0, 5.0, 2.0)])
            .unwrap()
            .contains("cross the outline"));
        assert!(outline_problem(&sq, true, &[h(6.0, 1.0, 7.0, 2.0)])
            .unwrap()
            .contains("outside"));
        assert!(
            outline_problem(&sq, true, &[h(1.0, 1.0, 2.0, 2.0), h(1.5, 1.5, 3.0, 3.0)])
                .unwrap()
                .contains("overlap")
        );
    }
}
