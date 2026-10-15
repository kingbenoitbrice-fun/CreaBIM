//! Moving, turning, scaling and copying elements, the same way for every
//! tool that does it (3D, plans, sections, the keyboard or a drag).

use crate::{Element, ElementId, ElementKind, Model, ViewKind};
use glam::{DQuat, DVec2, DVec3};

/// A change of position. `Rotate` turns about the vertical, as building
/// elements stand upright; `Tilt` turns about any other axis and suits only
/// free shapes. `Scale` is uniform; `Stretch` scales each axis on its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Xform {
    Move(DVec3),
    Rotate {
        center: DVec2,
        degrees: f64,
    },
    Tilt {
        center: DVec3,
        /// Unit axis to turn about, right-handed.
        axis: DVec3,
        degrees: f64,
    },
    Scale {
        center: DVec3,
        factor: f64,
    },
    Stretch {
        center: DVec3,
        factor: DVec3,
    },
}

impl Xform {
    pub fn point2(&self, p: DVec2) -> DVec2 {
        match *self {
            Xform::Move(d) => p + d.truncate(),
            Xform::Rotate { center, degrees } => {
                center + DVec2::from_angle(degrees.to_radians()).rotate(p - center)
            }
            Xform::Scale { center, factor } => center.truncate() + (p - center.truncate()) * factor,
            Xform::Stretch { center, factor } => {
                center.truncate() + (p - center.truncate()) * factor.truncate()
            }
            Xform::Tilt { .. } => self.point3(p.extend(0.0)).truncate(),
        }
    }

    pub fn point3(&self, p: DVec3) -> DVec3 {
        match *self {
            Xform::Move(d) => p + d,
            Xform::Rotate { .. } => self.point2(p.truncate()).extend(p.z),
            Xform::Scale { center, factor } => center + (p - center) * factor,
            Xform::Stretch { center, factor } => center + (p - center) * factor,
            Xform::Tilt {
                center,
                axis,
                degrees,
            } => center + DQuat::from_axis_angle(axis, degrees.to_radians()) * (p - center),
        }
    }

    /// A direction (no position) under the change: turned, never moved.
    fn direction(&self, v: DVec3) -> DVec3 {
        match *self {
            Xform::Rotate { degrees, .. } => DVec2::from_angle(degrees.to_radians())
                .rotate(v.truncate())
                .extend(v.z),
            Xform::Tilt { axis, degrees, .. } => {
                DQuat::from_axis_angle(axis, degrees.to_radians()) * v
            }
            _ => v,
        }
    }

    /// Does it keep shapes as they are, only moving or turning them?
    fn is_rigid(&self) -> bool {
        matches!(
            self,
            Xform::Move(_) | Xform::Rotate { .. } | Xform::Tilt { .. }
        )
    }

    /// Does it keep upright things upright?
    pub(crate) fn keeps_upright(&self) -> bool {
        !matches!(self, Xform::Tilt { .. })
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
            Xform::Stretch { center, factor } => Xform::Stretch {
                center,
                factor: factor.recip(),
            },
            Xform::Tilt {
                center,
                axis,
                degrees,
            } => Xform::Tilt {
                center,
                axis,
                degrees: -degrees,
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
            if matches!(t, Xform::Scale { .. }) || !t.keeps_upright() {
                return None;
            }
            g.start = t.point2(g.start);
            g.end = t.point2(g.end);
        }
        ElementKind::Wall(w) => {
            if !t.keeps_upright() {
                return None;
            }
            w.start = t.point2(w.start);
            w.end = t.point2(w.end);
            match t {
                Xform::Move(d) => w.base_offset += d.z,
                Xform::Stretch { center, factor } => {
                    if factor.z <= 0.0 {
                        return None;
                    }
                    // a mirror keeps the exterior on the same side of the building
                    if factor.x * factor.y < 0.0 {
                        w.flip = !w.flip;
                    }
                    let level = elevation(Some(w.level));
                    let base = level + w.base_offset;
                    w.base_offset = center.z + (base - center.z) * factor.z - level;
                    w.height *= factor.z;
                }
                _ => {}
            }
        }
        ElementKind::Slab(s) => {
            if !t.keeps_upright() {
                return None;
            }
            for p in &mut s.boundary {
                *p = t.point2(*p);
            }
            // a mirror in plan turns arcs the other way
            if let Xform::Stretch { factor, .. } = t {
                if factor.x * factor.y < 0.0 {
                    for b in &mut s.bulges {
                        *b = -*b;
                    }
                }
            }
            s.make_ccw();
            match t {
                Xform::Move(d) => s.top_offset += d.z,
                Xform::Stretch { center, factor } => {
                    if factor.z <= 0.0 {
                        return None;
                    }
                    let level = elevation(Some(s.level));
                    let top = s.top(level);
                    s.top_offset = center.z + (top - center.z) * factor.z - level;
                    s.thickness *= factor.z;
                }
                _ => {}
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
            let z = DVec3::Z * elevation(m.level);
            let before = m.origin + z;
            m.origin = origin(m.origin, m.level);
            match t {
                Xform::Rotate { degrees, .. } => {
                    m.rotation = (m.rotation + degrees).rem_euclid(360.0)
                }
                Xform::Scale { factor, .. } => m.scale *= factor,
                Xform::Move(_) => {}
                Xform::Tilt { .. } | Xform::Stretch { .. } => {
                    // no turn or size factor holds these: build them into
                    // the corners, which the turn and scale then apply to
                    let after = m.origin + z;
                    let old = m.clone();
                    for v in &mut m.mesh.vertices {
                        let p = t.point3(before + old.to_model(*v));
                        *v = old.to_local(p - after);
                    }
                    if let Xform::Stretch { factor, .. } = t {
                        if factor.x * factor.y * factor.z < 0.0 {
                            for f in &mut m.mesh.faces {
                                f.reverse();
                            }
                        }
                        if factor.x * factor.y * factor.z == 0.0 {
                            return None;
                        }
                    }
                }
            }
        }
        ElementKind::Sketch(s) => match t {
            Xform::Stretch { .. } => {
                // curves and dimensions don't survive uneven scaling
                if s.circle.is_some()
                    || !s.constraints.is_empty()
                    || s.kinds.iter().any(|k| *k != crate::SegmentKind::Line)
                {
                    return None;
                }
                // a stretched plane is still a plane: refit its axes
                let z = DVec3::Z * elevation(s.level);
                let pts: Vec<DVec3> = s
                    .model_points()
                    .into_iter()
                    .map(|p| t.point3(p + z) - z)
                    .collect();
                let o = origin(s.plane.origin, s.level);
                let x = (t.point3(s.plane.origin + s.plane.x + z) - z - o).try_normalize()?;
                let y0 = t.point3(s.plane.origin + s.plane.y + z) - z - o;
                let y = (y0 - x * y0.dot(x)).try_normalize()?;
                s.plane.origin = o;
                s.plane.x = x;
                s.plane.y = y;
                s.points = pts.iter().map(|p| s.plane.to_plane(*p)).collect();
            }
            _ => {
                s.plane.origin = origin(s.plane.origin, s.level);
                s.plane.x = t.direction(s.plane.x);
                s.plane.y = t.direction(s.plane.y);
                if let Xform::Scale { factor, .. } = t {
                    s.scale(*factor);
                }
            }
        },
        ElementKind::View(v) => match &mut v.kind {
            ViewKind::Section { start, end, .. } if t.is_rigid() && t.keeps_upright() => {
                *start = t.point2(*start);
                *end = t.point2(*end);
            }
            _ => return None,
        },
        ElementKind::Guide(g) => {
            g.a = origin(g.a, g.level);
            g.b = origin(g.b, g.level);
        }
        ElementKind::Column(_)
        | ElementKind::Beam(_)
        | ElementKind::Roof(_)
        | ElementKind::Stair(_)
        | ElementKind::Railing(_) => crate::members::transform(model, &mut out.kind, t)?,
        ElementKind::Sheet(_)
        | ElementKind::Viewport(_)
        | ElementKind::Material(_)
        | ElementKind::Assembly(_)
        | ElementKind::Dimension(_)
        | ElementKind::Tag(_)
        | ElementKind::Note(_)
        | ElementKind::TextStyle(_)
        | ElementKind::Units(_)
        | ElementKind::Cut(_)
        | ElementKind::GridGroup(_) => return None,
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
        ElementKind::Slab(s) => mean(&s.boundary).extend(s.top(elevation(Some(s.level)))),
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
        ElementKind::Guide(g) => (g.a + g.b) / 2.0 + DVec3::Z * elevation(g.level),
        _ => return crate::members::pivot(model, e),
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
            | ElementKind::Guide(_)
    ) && !crate::members::is_member(&e.kind)
    {
        return vec![];
    }
    let mut copy = Element::new(format!("{} copy", e.name), e.kind.clone());
    if let ElementKind::Grid(g) = &mut copy.kind {
        // a copied grid line follows on in its group
        g.label = crate::grids::next_label(model, g.group, g.start, g.end);
        copy.name = format!("Grid {}", crate::grids::display_label(model, g));
    }
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

/// What copying `ids` takes: the elements themselves, the doors and windows
/// in copied walls, and cuts whose target is copied (with their cutters).
pub fn copy_set(model: &Model, ids: &[ElementId]) -> Vec<ElementId> {
    let mut out: Vec<ElementId> = Vec::new();
    let push = |id: ElementId, out: &mut Vec<ElementId>| {
        if !out.contains(&id) && model.get(id).is_some() {
            out.push(id);
        }
    };
    for id in ids {
        push(*id, &mut out);
    }
    let mut i = 0;
    while i < out.len() {
        let id = out[i];
        for d in model.dependents(id) {
            match model.get(d).map(|e| &e.kind) {
                Some(ElementKind::Opening(_)) => push(d, &mut out),
                Some(ElementKind::Cut(c)) if c.target == id => {
                    push(c.cutter, &mut out);
                    push(d, &mut out);
                }
                _ => {}
            }
        }
        i += 1;
    }
    out
}

/// Fresh copies of `elements`, ready to insert into `model`: new ids, with
/// references among them pointed at the copies. A reference to something
/// neither copied nor in `model` (pasting into another project) goes to the
/// model's first level when it was a level, else the copy is dropped.
/// Returns the copies and how many were dropped.
pub fn fresh_copies(model: &Model, elements: &[Element]) -> (Vec<Element>, usize) {
    let map: std::collections::HashMap<String, String> = elements
        .iter()
        .map(|e| (e.id.0.to_string(), ElementId::new().0.to_string()))
        .collect();
    fn remap(v: &mut serde_json::Value, map: &std::collections::HashMap<String, String>) {
        match v {
            serde_json::Value::String(s) => {
                if let Some(n) = map.get(s.as_str()) {
                    *s = n.clone();
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(|x| remap(x, map)),
            serde_json::Value::Object(o) => o.values_mut().for_each(|x| remap(x, map)),
            _ => {}
        }
    }
    let first_level = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Level(l) => Some((e.id, l.elevation)),
            _ => None,
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|l| l.0);
    let mut copies: Vec<Element> = Vec::new();
    for e in elements {
        let Ok(mut v) = serde_json::to_value(e) else {
            continue;
        };
        remap(&mut v, &map);
        let Ok(mut c) = serde_json::from_value::<Element>(v) else {
            continue;
        };
        // a copy is a new object, not a mirror of the linked file's
        c.source = None;
        copies.push(c);
    }
    // point missing levels at this model's own
    let known = |id: ElementId, copies: &[Element]| {
        model.get(id).is_some() || copies.iter().any(|c| c.id == id)
    };
    for i in 0..copies.len() {
        let missing: Vec<ElementId> = copies[i]
            .references()
            .into_iter()
            .filter(|r| !known(*r, &copies))
            .collect();
        if missing.is_empty() {
            continue;
        }
        if let Some(level) = first_level {
            let c = &mut copies[i];
            let fix = |l: &mut ElementId| {
                if missing.contains(l) {
                    *l = level;
                }
            };
            match &mut c.kind {
                ElementKind::Wall(w) => fix(&mut w.level),
                ElementKind::Slab(s) => fix(&mut s.level),
                ElementKind::FreeMesh(m) => m.level.iter_mut().for_each(fix),
                ElementKind::Sketch(s) => s.level.iter_mut().for_each(fix),
                ElementKind::Guide(g) => g.level.iter_mut().for_each(fix),
                k => {
                    if let Some(l) = crate::members::level_mut(k) {
                        fix(l);
                    }
                }
            }
        }
    }
    // drop what still points nowhere, repeatedly (a door whose wall went)
    let before = copies.len();
    loop {
        let ids: Vec<ElementId> = copies.iter().map(|c| c.id).collect();
        let n = copies.len();
        copies.retain(|c| {
            c.references()
                .into_iter()
                .all(|r| model.get(r).is_some() || ids.contains(&r))
        });
        if copies.len() == n {
            break;
        }
    }
    let dropped = before - copies.len();
    (copies, dropped)
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

    #[test]
    fn a_tilted_mass_keeps_its_volume_and_a_stretched_wall_grows() {
        let m = sample_house();
        let wall = m
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)))
            .unwrap();
        let ElementKind::Wall(w0) = &wall.kind else {
            panic!()
        };
        let t = Xform::Stretch {
            center: DVec3::ZERO,
            factor: DVec3::new(2.0, 2.0, 1.5),
        };
        let ElementKind::Wall(w) = transformed(&m, wall, &t).unwrap().kind else {
            panic!()
        };
        assert!((w.length() - 2.0 * w0.length()).abs() < 1e-9);
        assert!((w.height - 1.5 * w0.height).abs() < 1e-9);
        let tilt = Xform::Tilt {
            center: DVec3::ZERO,
            axis: DVec3::X,
            degrees: 30.0,
        };
        assert!(transformed(&m, wall, &tilt).is_none());

        let mut mass = crate::FreeMesh::new(
            None,
            DVec3::new(1.0, 2.0, 0.0),
            crate::EditMesh::cuboid(DVec3::ONE),
        );
        mass.rotation = 30.0;
        mass.scale = 2.0;
        let e = Element::new("m", ElementKind::FreeMesh(mass.clone()));
        let world = |f: &crate::FreeMesh| -> Vec<DVec3> {
            f.mesh
                .vertices
                .iter()
                .map(|v| f.origin + f.to_model(*v))
                .collect()
        };
        let tilt = Xform::Tilt {
            center: DVec3::new(1.0, 2.0, 0.0),
            axis: DVec3::Y,
            degrees: 90.0,
        };
        let ElementKind::FreeMesh(r) = transformed(&m, &e, &tilt).unwrap().kind else {
            panic!()
        };
        for (a, b) in world(&mass).iter().zip(world(&r)) {
            assert!(tilt.point3(*a).distance(b) < 1e-9);
        }
    }

    #[test]
    fn pasted_copies_get_new_ids_and_keep_their_doors_in_their_walls() {
        let m = sample_house();
        let wall = m
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)) && !m.dependents(e.id).is_empty())
            .unwrap();
        let set = copy_set(&m, &[wall.id]);
        assert!(set.len() > 1);
        let elements: Vec<Element> = set.iter().map(|id| m.get(*id).unwrap().clone()).collect();
        let (copies, dropped) = fresh_copies(&m, &elements);
        assert_eq!(dropped, 0);
        assert!(copies.iter().all(|c| m.get(c.id).is_none()));
        let new_wall = copies[0].id;
        for c in &copies[1..] {
            if let ElementKind::Opening(o) = &c.kind {
                assert_eq!(o.host, new_wall);
            }
        }
        // a door pasted alone into an empty project has nowhere to go
        let door = elements[1].clone();
        let empty = Model::new();
        let (c, dropped) = fresh_copies(&empty, &[door]);
        assert!(c.is_empty());
        assert_eq!(dropped, 1);
    }
}
