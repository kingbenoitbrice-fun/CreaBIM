//! Faces picked in plans, sections and elevations. A face seen edge on is
//! a line of the drawing: the edge under the pointer stands for the face
//! through it, square to the paper, and its outward side is the one away
//! from the element's own drawing. The Move tool's face mode pushes and
//! pulls it (see `creabim_core::pushpull`), and the Align tool's Stretch
//! mode stretches it to a reference.

use crate::modal::{FacePick, Space};
use creabim_core::pushpull::FaceRef;
use creabim_core::ElementId;
use creabim_geom::plan::Pen;
use creabim_geom::PlanItem;
use glam::{DVec2, DVec3};

/// A face picked in a 2D pane: the face, and the outward normal of the
/// edge it is seen as, in the pane's drawing.
#[derive(Clone, Debug)]
pub struct PaneFace {
    pub pick: FacePick,
    pub out: DVec2,
}

/// A direction in the pane's drawing as a model direction.
pub fn to_model_dir(space: Space, v: DVec2) -> DVec3 {
    match space {
        Space::Section { start, end } => {
            (end - start).normalize_or_zero().extend(0.0) * v.x + DVec3::Z * v.y
        }
        Space::Plan | Space::Model => v.extend(0.0),
    }
}

/// A point of the pane's drawing in the model, `z` high in a plan.
pub fn to_model_point(space: Space, p: DVec2, z: f64) -> DVec3 {
    match space {
        Space::Section { start, end } => {
            (start + (end - start).normalize_or_zero() * p.x).extend(p.y)
        }
        Space::Plan | Space::Model => p.extend(z),
    }
}

/// The straight edge of an element that `takes` under `p` (within `tol`),
/// as its element and ends.
pub fn edge_at(
    items: &[PlanItem],
    p: DVec2,
    tol: f64,
    takes: &dyn Fn(ElementId) -> bool,
) -> Option<(ElementId, DVec2, DVec2)> {
    let mut best: Option<(f64, ElementId, DVec2, DVec2)> = None;
    let mut consider = |id: ElementId, a: DVec2, b: DVec2| {
        if a.distance(b) < 1e-9 || !takes(id) {
            return;
        }
        let d = segment_distance(p, a, b);
        if d < tol && best.is_none_or(|(bd, ..)| d < bd) {
            best = Some((d, id, a, b));
        }
    };
    for it in items {
        match it {
            PlanItem::Line { id, a, b, pen } if *pen != Pen::Underlay => consider(*id, *a, *b),
            PlanItem::Polygon { id, points, .. } | PlanItem::Fill { id, points, .. } => {
                for i in 0..points.len() {
                    consider(*id, points[i], points[(i + 1) % points.len()]);
                }
            }
            _ => {}
        }
    }
    best.map(|(_, id, a, b)| (id, a, b))
}

/// Which side of the edge `a`–`b` of `id` is outside it: the side away
/// from the element's filled shapes, or else from the middle of its
/// drawing.
pub fn outward(items: &[PlanItem], id: ElementId, a: DVec2, b: DVec2, at: DVec2) -> DVec2 {
    let n = (b - a).normalize_or_zero().perp();
    let polys: Vec<&[DVec2]> = items
        .iter()
        .filter_map(|it| match it {
            PlanItem::Fill { id: i, points, .. } | PlanItem::Polygon { id: i, points, .. }
                if *i == id && points.len() >= 3 =>
            {
                Some(points.as_slice())
            }
            _ => None,
        })
        .collect();
    let eps = (a.distance(b) * 0.01).clamp(1e-4, 0.005);
    let mid = nearest(at, a, b);
    let inside = |q: DVec2| polys.iter().any(|poly| point_in_polygon(q, poly));
    match (inside(mid + n * eps), inside(mid - n * eps)) {
        (true, false) => return -n,
        (false, true) => return n,
        _ => {}
    }
    let mut sum = DVec2::ZERO;
    let mut count = 0.0;
    for it in items.iter().filter(|it| it.id() == id) {
        let pts: Vec<DVec2> = match it {
            PlanItem::Line { a, b, .. } => vec![*a, *b],
            PlanItem::Polygon { points, .. } | PlanItem::Fill { points, .. } => points.clone(),
            _ => vec![],
        };
        for q in pts {
            sum += q;
            count += 1.0;
        }
    }
    let c = if count > 0.0 { sum / count } else { mid };
    if (mid - c).dot(n) >= 0.0 {
        n
    } else {
        -n
    }
}

/// The face of an element that `takes` under `p` in a pane: its edge, and
/// the face as the model sees it. `z` is the height a plan is drawn at;
/// `band` how wide the highlight is drawn, in the drawing's units.
pub fn face_at(
    items: &[PlanItem],
    space: Space,
    p: DVec2,
    tol: f64,
    z: f64,
    band: f64,
    takes: &dyn Fn(ElementId) -> bool,
) -> Option<PaneFace> {
    let (id, a, b) = edge_at(items, p, tol, takes)?;
    Some(face_of_edge(items, space, id, a, b, p, z, band))
}

/// The face seen as the edge `a`–`b` of `id`, picked at `at`.
#[allow(clippy::too_many_arguments)]
pub fn face_of_edge(
    items: &[PlanItem],
    space: Space,
    id: ElementId,
    a: DVec2,
    b: DVec2,
    at: DVec2,
    z: f64,
    band: f64,
) -> PaneFace {
    let out = outward(items, id, a, b, at);
    let point = to_model_point(space, nearest(at, a, b), z);
    let normal = to_model_dir(space, out);
    // the highlight: a band along the edge on the element's side
    let m = |q: DVec2| to_model_point(space, q, z);
    let (ia, ib) = (a - out * band, b - out * band);
    let tris = vec![[m(a), m(b), m(ib)], [m(a), m(ib), m(ia)]];
    PaneFace {
        pick: FacePick {
            id,
            face: FaceRef {
                point,
                normal,
                mesh_face: None,
            },
            tris,
        },
        out,
    }
}

fn nearest(p: DVec2, a: DVec2, b: DVec2) -> DVec2 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    a + ab * t
}

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    p.distance(nearest(p, a, b))
}

fn point_in_polygon(p: DVec2, poly: &[DVec2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(id: ElementId) -> Vec<PlanItem> {
        let pts = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(2.0, 1.0),
            DVec2::new(0.0, 1.0),
        ];
        vec![PlanItem::Fill {
            id,
            points: pts,
            hatch: creabim_geom::plan::Hatch::Solid,
        }]
    }

    #[test]
    fn an_edge_faces_away_from_its_element() {
        let id = ElementId::new();
        let items = square(id);
        let all = |_: ElementId| true;
        // the right edge of the square, picked in a plan
        let f = face_at(&items, Space::Plan, DVec2::new(2.01, 0.5), 0.05, 3.0, 0.02, &all).unwrap();
        assert_eq!(f.pick.id, id);
        assert!((f.pick.face.normal - DVec3::X).length() < 1e-9);
        assert!((f.pick.face.point - DVec3::new(2.0, 0.5, 3.0)).length() < 1e-9);
        // its top edge in a section looking north (running east): up
        let sec = Space::Section {
            start: DVec2::new(10.0, 0.0),
            end: DVec2::new(20.0, 0.0),
        };
        let f = face_at(&items, sec, DVec2::new(1.0, 0.99), 0.05, 0.0, 0.02, &all).unwrap();
        assert!((f.pick.face.normal - DVec3::Z).length() < 1e-9);
        assert!((f.pick.face.point - DVec3::new(11.0, 0.0, 1.0)).length() < 1e-9);
        // and its left edge there faces back along the section line
        let f = face_at(&items, sec, DVec2::new(0.0, 0.5), 0.05, 0.0, 0.02, &all).unwrap();
        assert!((f.pick.face.normal + DVec3::X).length() < 1e-9);
        // nothing the filter refuses is picked
        let none = |_: ElementId| false;
        assert!(face_at(&items, sec, DVec2::new(0.0, 0.5), 0.05, 0.0, 0.02, &none).is_none());
    }

    #[test]
    fn lines_alone_face_away_from_their_middle() {
        let id = ElementId::new();
        let line = |a: DVec2, b: DVec2| PlanItem::Line {
            id,
            pen: Pen::Projection,
            a,
            b,
        };
        let items = vec![
            line(DVec2::new(0.0, 0.0), DVec2::new(4.0, 0.0)),
            line(DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0)),
            line(DVec2::new(4.0, 3.0), DVec2::new(0.0, 3.0)),
        ];
        let n = outward(&items, id, items_end(&items, 2).0, items_end(&items, 2).1, DVec2::new(2.0, 3.0));
        assert!((n - DVec2::Y).length() < 1e-9);
        let n = outward(&items, id, DVec2::new(0.0, 0.0), DVec2::new(4.0, 0.0), DVec2::new(2.0, 0.0));
        assert!((n + DVec2::Y).length() < 1e-9);
    }

    fn items_end(items: &[PlanItem], i: usize) -> (DVec2, DVec2) {
        match &items[i] {
            PlanItem::Line { a, b, .. } => (*a, *b),
            _ => unreachable!(),
        }
    }
}
