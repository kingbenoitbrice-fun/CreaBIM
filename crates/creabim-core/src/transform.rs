//! Moving, turning, scaling and copying elements, the same way for every
//! tool that does it (3D, plans, sections, the keyboard or a drag).

use crate::{Element, ElementId, ElementKind, Model, ViewKind};
use glam::{DVec2, DVec3};

/// A change of position. Turns are about the vertical axis, as building
/// elements stand upright; scaling is uniform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Xform {
    Move(DVec3),
    Rotate { center: DVec2, degrees: f64 },
    Scale { center: DVec3, factor: f64 },
}

impl Xform {
    pub fn point2(&self, p: DVec2) -> DVec2 {
        match *self {
            Xform::Move(d) => p + d.truncate(),
            Xform::Rotate { center, degrees } => {
                center + DVec2::from_angle(degrees.to_radians()).rotate(p - center)
            }
            Xform::Scale { center, factor } => center.truncate() + (p - center.truncate()) * factor,
        }
    }

    pub fn point3(&self, p: DVec3) -> DVec3 {
        match *self {
            Xform::Move(d) => p + d,
            Xform::Rotate { .. } => self.point2(p.truncate()).extend(p.z),
            Xform::Scale { center, factor } => center + (p - center) * factor,
        }
    }

    /// A direction (no position) under the change: turned, never moved.
    fn direction(&self, v: DVec3) -> DVec3 {
        match *self {
            Xform::Rotate { degrees, .. } => DVec2::from_angle(degrees.to_radians())
                .rotate(v.truncate())
                .extend(v.z),
            _ => v,
        }
    }

    /// The same change in the opposite direction.
    pub fn inverse(&self) -> Xform {
        match *self {
            Xform::Move(d) => Xform::Move(-d),
            Xform::Rotate { center, degrees } => Xform::Rotate {
                center,
                degrees: -degrees,
            },
            Xform::Scale { center, factor } => Xform::Scale {
                center,
                factor: 1.0 / factor,
            },
        }
    }
}

/// `e` after `t`, or `None` when that kind of element can't take it (a door
/// can slide along its wall but not turn on its own; a level only moves up
/// or down). Walls and slabs scale in plan; their heights stay.
pub fn transformed(model: &Model, e: &Element, t: &Xform) -> Option<Element> {
    let mut out = e.clone();
    let elevation = |level: Option<ElementId>| {
        level
            .and_then(|l| model.level(l))
            .map_or(0.0, |l| l.elevation)
    };
    // origins of masses and sketches are relative to their level
    let origin = |o: DVec3, level: Option<ElementId>| {
        let z = DVec3::Z * elevation(level);
        t.point3(o + z) - z
    };
    match &mut out.kind {
        ElementKind::Level(l) => match t {
            Xform::Move(d) if d.z != 0.0 => l.elevation += d.z,
            _ => return None,
        },
        ElementKind::Grid(g) => {
            if matches!(t, Xform::Scale { .. }) {
                return None;
            }
            g.start = t.point2(g.start);
            g.end = t.point2(g.end);
        }
        ElementKind::Wall(w) => {
            w.start = t.point2(w.start);
            w.end = t.point2(w.end);
            if let Xform::Move(d) = t {
                w.base_offset += d.z;
            }
        }
        ElementKind::Slab(s) => {
            for p in &mut s.boundary {
                *p = t.point2(*p);
            }
            if matches!(t, Xform::Scale { factor, .. } if *factor < 0.0) {
                s.boundary.reverse();
            }
        }
        ElementKind::Opening(o) => {
            let Xform::Move(d) = t else { return None };
            let ElementKind::Wall(w) = &model.get(o.host)?.kind else {
                return None;
            };
            let max = (w.length() - o.width).max(0.0);
            o.offset = (o.offset + d.truncate().dot(w.direction())).clamp(0.0, max);
            o.sill = (o.sill + d.z).max(0.0);
        }
        ElementKind::FreeMesh(m) => {
            m.origin = origin(m.origin, m.level);
            match t {
                Xform::Rotate { degrees, .. } => {
                    m.rotation = (m.rotation + degrees).rem_euclid(360.0)
                }
                Xform::Scale { factor, .. } => m.scale *= factor,
                Xform::Move(_) => {}
            }
        }
        ElementKind::Sketch(s) => {
            s.plane.origin = origin(s.plane.origin, s.level);
            s.plane.x = t.direction(s.plane.x);
            s.plane.y = t.direction(s.plane.y);
            if let Xform::Scale { factor, .. } = t {
                for p in &mut s.points {
                    *p *= *factor;
                }
            }
        }
        ElementKind::View(v) => match &mut v.kind {
            ViewKind::Section { start, end, .. } if !matches!(t, Xform::Scale { .. }) => {
                *start = t.point2(*start);
                *end = t.point2(*end);
            }
            _ => return None,
        },
        ElementKind::Sheet(_)
        | ElementKind::Viewport(_)
        | ElementKind::Material(_)
        | ElementKind::Dimension(_)
        | ElementKind::Tag(_) => return None,
    }
    Some(out)
}

/// Where an element is, for turning and scaling it about itself.
pub fn pivot(model: &Model, id: ElementId) -> Option<DVec3> {
    let e = model.get(id)?;
    let elevation = |level: Option<ElementId>| {
        level
            .and_then(|l| model.level(l))
            .map_or(0.0, |l| l.elevation)
    };
    let mean = |pts: &[DVec2]| pts.iter().copied().sum::<DVec2>() / pts.len().max(1) as f64;
    Some(match &e.kind {
        ElementKind::Level(l) => DVec3::new(0.0, 0.0, l.elevation),
        ElementKind::Grid(g) => ((g.start + g.end) / 2.0).extend(0.0),
        ElementKind::Wall(w) => {
            ((w.start + w.end) / 2.0).extend(elevation(Some(w.level)) + w.base_offset)
        }
        ElementKind::Slab(s) => mean(&s.boundary).extend(elevation(Some(s.level))),
        ElementKind::Opening(o) => {
            let ElementKind::Wall(w) = &model.get(o.host)?.kind else {
                return None;
            };
            (w.start + w.direction() * (o.offset + o.width / 2.0))
                .extend(elevation(Some(w.level)) + w.base_offset + o.sill)
        }
        ElementKind::FreeMesh(m) => m.origin + DVec3::Z * elevation(m.level),
        ElementKind::Sketch(s) => {
            let pts = s.model_points();
            pts.iter().copied().sum::<DVec3>() / pts.len().max(1) as f64
                + DVec3::Z * elevation(s.level)
        }
        ElementKind::View(v) => match &v.kind {
            ViewKind::Section { start, end, .. } => ((*start + *end) / 2.0).extend(0.0),
            _ => return None,
        },
        _ => return None,
    })
}

/// Copies of an element with fresh ids, ready to insert: a wall comes with
/// its doors and windows. The copy is named "<name> copy".
pub fn duplicate(model: &Model, id: ElementId) -> Vec<Element> {
    let Some(e) = model.get(id) else {
        return vec![];
    };
    if !matches!(
        e.kind,
        ElementKind::Wall(_)
            | ElementKind::Slab(_)
            | ElementKind::Opening(_)
            | ElementKind::FreeMesh(_)
            | ElementKind::Sketch(_)
            | ElementKind::Grid(_)
    ) {
        return vec![];
    }
    let mut copy = Element::new(format!("{} copy", e.name), e.kind.clone());
    if let ElementKind::Opening(o) = &mut copy.kind {
        // a lone door copy goes beside the original, on the same wall
        if let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) {
            let next = o.offset + o.width + 0.3;
            o.offset = if next + o.width <= w.length() {
                next
            } else {
                (o.offset - o.width - 0.3).max(0.0)
            };
        }
    }
    let mut out = vec![copy];
    if matches!(e.kind, ElementKind::Wall(_)) {
        for d in model.dependents(id) {
            if let Some(h) = model.get(d) {
                if let ElementKind::Opening(o) = &h.kind {
                    let mut o = o.clone();
                    o.host = out[0].id;
                    out.push(Element::new(h.name.clone(), ElementKind::Opening(o)));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sample::sample_house, Wall};

    #[test]
    fn turning_a_wall_about_its_middle_keeps_its_length() {
        let m = sample_house();
        let (id, w) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some((e.id, w.clone())),
                _ => None,
            })
            .unwrap();
        let c = pivot(&m, id).unwrap();
        let t = Xform::Rotate {
            center: c.truncate(),
            degrees: 90.0,
        };
        let ElementKind::Wall(r) = transformed(&m, m.get(id).unwrap(), &t).unwrap().kind else {
            panic!()
        };
        assert!((r.length() - w.length()).abs() < 1e-9);
        assert!(r.direction().dot(w.direction()).abs() < 1e-9);
        let back = transformed(&m, &Element::new("w", ElementKind::Wall(r)), &t.inverse());
        let ElementKind::Wall(Wall { start, .. }) = back.unwrap().kind else {
            panic!()
        };
        assert!(start.distance(w.start) < 1e-9);
    }

    #[test]
    fn doors_slide_along_their_wall_and_stay_inside_it() {
        let m = sample_house();
        let (id, wall_len) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Opening(o) => match &m.get(o.host)?.kind {
                    ElementKind::Wall(w) => Some((e.id, w.length())),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let t = Xform::Move(DVec3::new(500.0, 500.0, 0.3));
        let ElementKind::Opening(o) = transformed(&m, m.get(id).unwrap(), &t).unwrap().kind else {
            panic!()
        };
        assert!((o.offset + o.width - wall_len).abs() < 1e-9);
        let turn = Xform::Rotate {
            center: DVec2::ZERO,
            degrees: 10.0,
        };
        assert!(transformed(&m, m.get(id).unwrap(), &turn).is_none());
    }

    #[test]
    fn duplicating_a_wall_brings_its_openings() {
        let m = sample_house();
        let wall = m
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)) && !m.dependents(e.id).is_empty())
            .unwrap();
        let copies = duplicate(&m, wall.id);
        let hosted = m
            .dependents(wall.id)
            .into_iter()
            .filter(|d| matches!(m.get(*d).unwrap().kind, ElementKind::Opening(_)))
            .count();
        assert_eq!(copies.len(), 1 + hosted);
        let mut tx = crate::Transaction::new("Duplicate");
        for c in copies {
            tx = tx.insert(c);
        }
        let mut doc = crate::Document::from_model(m);
        doc.commit(tx).unwrap();
    }
}
