//! Centre lines: the planes through the middle of an element that the
//! Align tool can take as a reference, or align an element by. A wall's
//! and a beam's run along their centreline (a beam's also through its
//! middle depth), a column's cross at its axis, and a slab's, a roof's or
//! a mass's halve it each way; a door's or window's runs across its wall
//! through its middle. A plane seen edge on (in a plan or a section) is a
//! line of the drawing; in 3D it shows where it crosses the face under
//! the pointer.

use creabim_core::members::Profile;
use creabim_core::{ElementId, ElementKind, Model};
use creabim_geom::SceneItem;
use glam::{DVec2, DVec3};

/// A centre plane of an element: a point on it, its normal, and its
/// extent (the element's box cut by it), corner after corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CentrePlane {
    pub id: ElementId,
    pub point: DVec3,
    pub normal: DVec3,
    pub corners: [DVec3; 4],
}

/// The box `id`'s 3D shape fills.
fn bounds(scene: &[SceneItem], id: ElementId) -> Option<(DVec3, DVec3)> {
    scene
        .iter()
        .filter(|s| s.id == id)
        .filter_map(|s| s.mesh.bounds())
        .map(|(a, b)| (a.as_dvec3(), b.as_dvec3()))
        .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
}

/// The plane through `c` with normal `n`, `e1` each way along `t1` and
/// `e2` along `t2` (both in it).
fn rect(id: ElementId, c: DVec3, n: DVec3, t1: DVec3, e1: f64, t2: DVec3, e2: f64) -> CentrePlane {
    CentrePlane {
        id,
        point: c,
        normal: n,
        corners: [
            c - t1 * e1 - t2 * e2,
            c + t1 * e1 - t2 * e2,
            c + t1 * e1 + t2 * e2,
            c - t1 * e1 + t2 * e2,
        ],
    }
}

/// The plane through `c` with normal `n`, as far as the box `lo`–`hi`
/// reaches along it.
fn boxed(id: ElementId, c: DVec3, n: DVec3, lo: DVec3, hi: DVec3) -> CentrePlane {
    let t1 = if n.z.abs() > 0.9 {
        DVec3::X
    } else {
        DVec3::Z.cross(n).normalize()
    };
    let t2 = n.cross(t1).normalize();
    let corners = (0..8).map(|i| {
        DVec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    });
    let (mut e1, mut e2) = (0.0f64, 0.0f64);
    for k in corners {
        e1 = e1.max((k - c).dot(t1).abs());
        e2 = e2.max((k - c).dot(t2).abs());
    }
    rect(id, c, n, t1, e1, t2, e2)
}

/// The centre planes of element `id`, as its shape in `scene` is.
pub fn of(model: &Model, scene: &[SceneItem], id: ElementId) -> Vec<CentrePlane> {
    let (Some(e), Some((lo, hi))) = (model.get(id), bounds(scene, id)) else {
        return vec![];
    };
    let mid = (lo + hi) / 2.0;
    let hz = (hi.z - lo.z) / 2.0;
    // along a line in plan from `a` to `b`, upright through it
    let along = |a: DVec2, b: DVec2| {
        let u = (b - a).normalize_or_zero();
        let c = ((a + b) / 2.0).extend(mid.z);
        (u != DVec2::ZERO).then(|| (u.extend(0.0), u.perp().extend(0.0), c, a.distance(b) / 2.0))
    };
    match &e.kind {
        ElementKind::Wall(w) => along(w.start, w.end)
            .map(|(u, v, c, l)| vec![rect(id, c, v, u, l, DVec3::Z, hz)])
            .unwrap_or_default(),
        ElementKind::Beam(b) => along(b.start, b.end)
            .map(|(u, v, c, l)| {
                vec![
                    rect(id, c, v, u, l, DVec3::Z, hz),
                    rect(id, c, DVec3::Z, u, l, v, b.width / 2.0),
                ]
            })
            .unwrap_or_default(),
        ElementKind::Column(col) => {
            let u = DVec2::from_angle(col.rotation.to_radians()).extend(0.0);
            let v = DVec3::Z.cross(u);
            let (hw, hd) = match col.profile {
                Profile::Rectangle { width, depth } => (width / 2.0, depth / 2.0),
                Profile::Round { diameter } => (diameter / 2.0, diameter / 2.0),
            };
            let c = col.position.extend(mid.z);
            vec![
                rect(id, c, u, v, hd, DVec3::Z, hz),
                rect(id, c, v, u, hw, DVec3::Z, hz),
            ]
        }
        ElementKind::Opening(o) => {
            let u = match model.get(o.host).map(|h| &h.kind) {
                Some(ElementKind::Wall(w)) => (w.end - w.start).normalize_or(DVec2::X),
                _ => DVec2::X,
            };
            vec![
                boxed(id, mid, u.extend(0.0), lo, hi),
                boxed(id, mid, DVec3::Z, lo, hi),
            ]
        }
        ElementKind::Slab(_) | ElementKind::FreeMesh(_) => vec![
            boxed(id, mid, DVec3::X, lo, hi),
            boxed(id, mid, DVec3::Y, lo, hi),
            boxed(id, mid, DVec3::Z, lo, hi),
        ],
        ElementKind::Roof(_) | ElementKind::Stair(_) => vec![
            boxed(id, mid, DVec3::X, lo, hi),
            boxed(id, mid, DVec3::Y, lo, hi),
        ],
        _ => vec![],
    }
}

/// The centre planes of everything in `scene` that `keep` takes.
pub fn all(
    model: &Model,
    scene: &[SceneItem],
    keep: &dyn Fn(ElementId) -> bool,
) -> Vec<CentrePlane> {
    let mut ids: Vec<ElementId> = scene.iter().map(|s| s.id).filter(|i| keep(*i)).collect();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
        .flat_map(|id| of(model, scene, id))
        .collect()
}

/// `c` seen edge on along `look`: the line it is in a drawing whose points
/// `to_pane` gives. None when it is seen at a slant.
pub fn edge_on(
    c: &CentrePlane,
    look: DVec3,
    to_pane: &dyn Fn(DVec3) -> DVec2,
) -> Option<(DVec2, DVec2)> {
    if c.normal.dot(look).abs() > 0.02 {
        return None;
    }
    let pts = c.corners.map(to_pane);
    let mut best = (0.0, pts[0], pts[0]);
    for i in 0..4 {
        for j in i + 1..4 {
            let d = pts[i].distance(pts[j]);
            if d > best.0 {
                best = (d, pts[i], pts[j]);
            }
        }
    }
    (best.0 > 1e-6).then_some((best.1, best.2))
}

/// Where `c` crosses the plane through `p` with normal `n` (a face it
/// stands square to): the centre line drawn on that face, its extent
/// seen square to the face. None when it lies along the face.
pub fn trace_on(c: &CentrePlane, p: DVec3, n: DVec3) -> Option<(DVec3, DVec3)> {
    if c.normal.dot(n).abs() > 0.1 {
        return None;
    }
    let pts = c.corners.map(|k| k - n * (k - p).dot(n));
    let mut best = (0.0, pts[0], pts[0]);
    for i in 0..4 {
        for j in i + 1..4 {
            let d = pts[i].distance(pts[j]);
            if d > best.0 {
                best = (d, pts[i], pts[j]);
            }
        }
    }
    (best.0 > 1e-6).then_some((best.1, best.2))
}

/// The line `a`–`b` on a face (normal `n`) cut to where it crosses the
/// face's triangles `tris`: none when it misses them. Without triangles
/// it is kept as it is.
pub fn clip_to(a: DVec3, b: DVec3, n: DVec3, tris: &[[DVec3; 3]]) -> Option<(DVec3, DVec3)> {
    if tris.is_empty() {
        return Some((a, b));
    }
    let d = (b - a).normalize_or_zero();
    let m = d.cross(n).normalize_or_zero();
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for t in tris {
        let side = t.map(|v| (v - a).dot(m));
        let (smin, smax) = side
            .iter()
            .fold((f64::MAX, f64::MIN), |(x, y), s| (x.min(*s), y.max(*s)));
        if smin > 1e-6 || smax < -1e-6 {
            continue;
        }
        // where the line crosses this triangle's edges
        for i in 0..3 {
            let (p, q) = (t[i], t[(i + 1) % 3]);
            let (sp, sq) = (side[i], side[(i + 1) % 3]);
            let x = if (sp - sq).abs() < 1e-12 {
                if sp.abs() > 1e-6 {
                    continue;
                }
                lo = lo.min((p - a).dot(d));
                hi = hi.max((p - a).dot(d));
                q
            } else if (sp <= 0.0) != (sq <= 0.0) || sp.abs() < 1e-9 {
                p + (q - p) * (sp / (sp - sq))
            } else {
                continue;
            };
            let along = (x - a).dot(d);
            lo = lo.min(along);
            hi = hi.max(along);
        }
    }
    (hi - lo > 1e-6).then(|| (a + d * lo, a + d * hi))
}

/// How far `p` is from the segment `a`–`b`.
pub fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-18)).clamp(0.0, 1.0);
    (a + ab * t).distance(p)
}

/// The centre line of a drawing nearest `p` within `tol`: its element,
/// ends and distance.
pub fn line_at(
    lines: &[(ElementId, DVec2, DVec2)],
    p: DVec2,
    tol: f64,
) -> Option<(ElementId, DVec2, DVec2, f64)> {
    lines
        .iter()
        .map(|&(id, a, b)| (id, a, b, segment_distance(p, a, b)))
        .filter(|l| l.3 < tol)
        .min_by(|x, y| x.3.total_cmp(&y.3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    #[test]
    fn a_walls_centre_line_runs_down_its_middle_in_plan() {
        let model = sample_house();
        let scene = creabim_geom::build_scene(&model);
        let (id, w) = model
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some((e.id, w.clone())),
                _ => None,
            })
            .unwrap();
        let planes = of(&model, &scene, id);
        assert_eq!(planes.len(), 1);
        let (a, b) = edge_on(&planes[0], -DVec3::Z, &|p| p.truncate()).unwrap();
        // the line is the wall's centreline, end to end
        for p in [a, b] {
            assert!(segment_distance(p, w.start, w.end) < 1e-9);
        }
        assert!((a.distance(b) - w.length()).abs() < 1e-9);
        // seen from the side (looking along its normal) it is no line
        let n = planes[0].normal;
        assert!(edge_on(&planes[0], n, &|p| p.truncate()).is_none());
        // on its top face, it is a line along it
        let top = planes[0].corners[2].z;
        let (a, b) = trace_on(&planes[0], DVec3::new(0.0, 0.0, top), DVec3::Z).unwrap();
        assert!((a.distance(b) - w.length()).abs() < 1e-6);
    }

    #[test]
    fn a_centre_line_is_cut_to_the_face_it_is_drawn_on() {
        // a unit square face at z = 0, and a line along y through x = 0.5
        let tris = [
            [DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0)],
            [DVec3::ZERO, DVec3::new(1.0, 1.0, 0.0), DVec3::Y],
        ];
        let (a, b) = clip_to(
            DVec3::new(0.5, -3.0, 0.0),
            DVec3::new(0.5, 4.0, 0.0),
            DVec3::Z,
            &tris,
        )
        .unwrap();
        assert!((a - DVec3::new(0.5, 0.0, 0.0)).length() < 1e-9, "{a}");
        assert!((b - DVec3::new(0.5, 1.0, 0.0)).length() < 1e-9, "{b}");
        // one that misses it is gone
        assert!(clip_to(
            DVec3::new(2.5, -3.0, 0.0),
            DVec3::new(2.5, 4.0, 0.0),
            DVec3::Z,
            &tris
        )
        .is_none());
    }

    #[test]
    fn a_column_has_two_centre_planes_crossing_at_its_axis() {
        let mut doc = creabim_core::Document::from_model(sample_house());
        let level = doc.model().levels().next().unwrap().0.id;
        let mut c = creabim_core::members::Column::new(
            level,
            DVec2::new(40.0, 5.0),
            Profile::Rectangle {
                width: 0.4,
                depth: 0.2,
            },
        );
        c.rotation = 30.0;
        let e = creabim_core::Element::new("Post", ElementKind::Column(c));
        let id = e.id;
        doc.commit(creabim_core::Transaction::new("Post").insert(e))
            .unwrap();
        let m = doc.model();
        let scene = creabim_geom::build_scene(m);
        let planes = of(m, &scene, id);
        assert_eq!(planes.len(), 2);
        assert!(planes[0].normal.dot(planes[1].normal).abs() < 1e-9);
        for p in &planes {
            assert!((p.point.truncate() - DVec2::new(40.0, 5.0)).length() < 1e-9);
            // in plan each is a line through the axis
            let (a, b) = edge_on(p, -DVec3::Z, &|q| q.truncate()).unwrap();
            assert!(segment_distance(DVec2::new(40.0, 5.0), a, b) < 1e-9);
        }
    }
}
