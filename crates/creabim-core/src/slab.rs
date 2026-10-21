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
fn room_in_lines(segs: &[(DVec2, DVec2)], p: DVec2) -> Option<Vec<DVec2>> {
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
