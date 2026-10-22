//! Push and pull: one face of an element moved along its normal, the
//! element's own parameters following. A wall's end face lengthens it, its
//! top raises it and a side thickens it; a slab's top or bottom thickens it
//! and an edge face moves that edge; a mass's face moves its vertices.
//! Columns, beams, roofs, stairs and railings, and doors and windows in
//! their walls, take their faces the same way (see [`push_face`]), so the
//! Move tool's face mode and the Align tool's Stretch work on all of them.
//! Furniture and fixtures keep the shape of their type: a face of one
//! moves the whole object, so aligning a sofa's back to a wall puts it
//! flush.

use crate::members::Profile;
use crate::{Element, ElementId, ElementKind, Model};
use glam::{DVec2, DVec3};

/// A face picked on an element's shape: a point on it and its outward
/// normal, in model coordinates, and for a mass the mesh face it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceRef {
    pub point: DVec3,
    pub normal: DVec3,
    pub mesh_face: Option<usize>,
}

/// Can elements of this kind have a face pushed or pulled?
pub fn can_push(kind: &ElementKind) -> bool {
    matches!(
        kind,
        ElementKind::Wall(_)
            | ElementKind::Slab(_)
            | ElementKind::FreeMesh(_)
            | ElementKind::Column(_)
            | ElementKind::Beam(_)
            | ElementKind::Roof(_)
            | ElementKind::Stair(_)
            | ElementKind::Railing(_)
            | ElementKind::Opening(_)
            | ElementKind::Object(_)
    )
}

/// What a push of `id` changes: the element, then the doors and windows
/// it hosts (their offsets run from the wall's start, which may move).
pub fn affected(model: &Model, id: ElementId) -> Vec<Element> {
    let mut out: Vec<Element> = model.get(id).cloned().into_iter().collect();
    if matches!(out.first().map(|e| &e.kind), Some(ElementKind::Wall(_))) {
        for d in model.dependents(id) {
            if let Some(e) = model
                .get(d)
                .filter(|e| matches!(e.kind, ElementKind::Opening(_)))
            {
                out.push(e.clone());
            }
        }
    }
    out
}

/// Which way a face looks, in an element's own terms.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Facing {
    Top,
    Bottom,
    /// Across the level, this way (a unit vector in plan).
    Side(DVec2),
    /// Neither upright nor level: a roof slope or a stair's soffit.
    Sloped,
}

fn facing(n: DVec3) -> Facing {
    if n.z > 0.9 {
        Facing::Top
    } else if n.z < -0.9 {
        Facing::Bottom
    } else if n.z.abs() < 0.1 {
        Facing::Side(n.truncate().normalize_or(DVec2::X))
    } else {
        Facing::Sloped
    }
}

/// `elements` (as [`affected`] lists them) with the face pushed out by `s`
/// along its normal (pulled in when negative). `None` when the shape
/// can't take it: the face isn't one that moves, or it would turn the
/// element inside out.
///
/// - a wall's ends lengthen it, its sides thicken it (the far face stays),
///   its top and bottom change its height;
/// - a slab's edges move, its top and underside thicken it;
/// - a column's top and bottom change its height and its sides widen it;
/// - a beam's ends lengthen it, its sides widen it, its top and bottom
///   deepen it;
/// - a roof's eaves and gable ends move that edge of its outline, its top
///   thickens it and its underside drops its base (the top stays);
/// - a stair's sides widen it, its start and end lengthen its going, its
///   top and bottom change its rise;
/// - a railing's top changes its height, its ends lengthen its path and
///   its sides move it;
/// - a door's or window's jambs widen it, its head and sill move;
/// - a mass's face moves its corners;
/// - an object (furniture, a fixture) moves whole, its shape being its
///   type's: a side face slides it across the level, its top or bottom
///   raises or lowers it.
pub fn push_face(
    model: &Model,
    elements: &[Element],
    face: &FaceRef,
    s: f64,
) -> Option<Vec<Element>> {
    let first = elements.first()?;
    let mut out = elements.to_vec();
    let n = face.normal.normalize_or_zero();
    let look = facing(n);
    let slopes = matches!(first.kind, ElementKind::Roof(_) | ElementKind::FreeMesh(_));
    if look == Facing::Sloped && !slopes {
        return None;
    }
    match &mut out[0].kind {
        ElementKind::Wall(w) => {
            let d = w.direction();
            let across = d.perp();
            match look {
                Facing::Top => {
                    w.height += s;
                    // a top tied to a level keeps its new height above it
                    if let crate::WallTop::UpToLevel { offset, .. } = &mut w.top {
                        *offset += s;
                    }
                }
                Facing::Bottom => {
                    w.base_offset -= s;
                    w.height += s;
                    // doors and windows stay where they are
                    for o in out.iter_mut().skip(1) {
                        if let ElementKind::Opening(o) = &mut o.kind {
                            o.sill += s;
                        }
                    }
                    return check(out, first);
                }
                Facing::Side(flat) if flat.dot(d).abs() >= flat.dot(across).abs() => {
                    if flat.dot(d) > 0.0 {
                        w.end += d * s;
                    } else {
                        w.start -= d * s;
                        // doors and windows stay where they are in the wall
                        for o in out.iter_mut().skip(1) {
                            if let ElementKind::Opening(o) = &mut o.kind {
                                o.offset += s;
                            }
                        }
                        return check(out, first);
                    }
                }
                Facing::Side(flat) => {
                    // the far face stays put: the centre line moves half way
                    let side = flat.dot(across).signum();
                    w.thickness += s;
                    w.start += across * side * s / 2.0;
                    w.end += across * side * s / 2.0;
                }
                Facing::Sloped => return None,
            }
        }
        ElementKind::Slab(sl) => match look {
            Facing::Top => {
                sl.top_offset += s;
                sl.thickness += s;
            }
            Facing::Bottom => sl.thickness += s,
            Facing::Side(_) => {
                let i = slab_edge(&sl.boundary, &sl.bulges, face)?;
                let k = (i + 1) % sl.boundary.len();
                let step = n.truncate().normalize_or_zero() * s;
                sl.boundary[i] += step;
                sl.boundary[k] += step;
            }
            Facing::Sloped => return None,
        },
        ElementKind::FreeMesh(m) => {
            let f = face.mesh_face?;
            let verts = m.mesh.faces.get(f)?.clone();
            let local = m.to_local(n * s);
            m.mesh.translate_vertices(&verts, local);
        }
        ElementKind::Column(c) => {
            let h = c.height_in(model);
            match look {
                Facing::Top => c.height = Some(h + s),
                Facing::Bottom => {
                    c.base_offset -= s;
                    c.height = Some(h + s);
                }
                Facing::Side(flat) => match &mut c.profile {
                    Profile::Rectangle { width, depth } => {
                        let x = DVec2::from_angle(c.rotation.to_radians());
                        // the far side stays: the centre moves half way
                        let (axis, size) = if flat.dot(x).abs() >= flat.dot(x.perp()).abs() {
                            (x, width)
                        } else {
                            (x.perp(), depth)
                        };
                        *size += s;
                        c.position += axis * flat.dot(axis).signum() * s / 2.0;
                    }
                    Profile::Round { diameter } => {
                        *diameter += s;
                        c.position += flat * s / 2.0;
                    }
                },
                Facing::Sloped => return None,
            }
        }
        ElementKind::Beam(b) => {
            let d = (b.end - b.start).normalize_or_zero();
            match look {
                Facing::Top => {
                    b.top_offset -= s;
                    b.depth += s;
                }
                Facing::Bottom => b.depth += s,
                Facing::Side(flat) if flat.dot(d).abs() >= flat.dot(d.perp()).abs() => {
                    if flat.dot(d) > 0.0 {
                        b.end += d * s;
                    } else {
                        b.start -= d * s;
                    }
                }
                Facing::Side(flat) => {
                    let side = flat.dot(d.perp()).signum();
                    b.width += s;
                    b.start += d.perp() * side * s / 2.0;
                    b.end += d.perp() * side * s / 2.0;
                }
                Facing::Sloped => return None,
            }
        }
        ElementKind::Roof(r) => {
            if n.z > 0.1 {
                // the top (a slope or a flat top) rises: thicker
                r.thickness += s;
            } else if n.z < -0.1 {
                // the underside drops and the top stays: thicker, its base lower
                r.thickness += s;
                r.base_offset -= s / n.z.abs();
            } else {
                // an eave or a gable end: that edge of the outline moves
                let want = n.truncate().normalize_or_zero();
                let i = roof_edge(&r.boundary, want, face.point.truncate())?;
                let k = (i + 1) % r.boundary.len();
                r.boundary[i] += want * s;
                r.boundary[k] += want * s;
            }
        }
        ElementKind::Stair(st) => {
            let rise = st.rise(model);
            let lay = st.layout(rise);
            let f = lay.flights.first().copied()?;
            let last = lay.flights.last().copied().unwrap_or(f);
            let treads = |risers: u32| risers.saturating_sub(1).max(1) as f64;
            // the start and end faces are the first riser and the last
            // nosing, not the risers between
            let at = face.point.truncate();
            match look {
                Facing::Top => st.height = Some(rise + s),
                Facing::Bottom => {
                    st.base_offset -= s;
                    st.height = Some(rise + s);
                }
                Facing::Side(flat)
                    if flat.dot(-f.dir) > 0.9 && (at - f.start).dot(f.dir) < st.tread / 2.0 =>
                {
                    // the start goes back and the top step stays: longer treads
                    st.start -= f.dir * s;
                    st.tread += s / treads(f.risers);
                }
                Facing::Side(flat)
                    if flat.dot(last.dir) > 0.9
                        && (at - last.end(st.tread)).dot(last.dir) > -st.tread / 2.0 =>
                {
                    st.tread += s / treads(last.risers);
                }
                Facing::Side(flat) if flat.dot(f.dir.perp()).abs() > 0.9 => {
                    // a side of the first flight: wider, the other side stays
                    let side = flat.dot(f.dir.perp()).signum();
                    st.width += s;
                    if st.landing.is_none() {
                        st.start += f.dir.perp() * side * s / 2.0;
                    }
                }
                Facing::Side(_) | Facing::Sloped => return None,
            }
        }
        ElementKind::Railing(r) => match look {
            Facing::Top => r.height += s,
            Facing::Bottom | Facing::Sloped => return None,
            Facing::Side(_) if r.stair.is_some() => return None,
            Facing::Side(flat) => {
                let path = &mut r.path;
                if path.len() < 2 {
                    return None;
                }
                let k = path.len() - 1;
                let first = (path[0] - path[1]).truncate().normalize_or_zero();
                let last = (path[k] - path[k - 1]).truncate().normalize_or_zero();
                if flat.dot(first) > 0.9 {
                    path[0] += (first * s).extend(0.0);
                } else if flat.dot(last) > 0.9 {
                    path[k] += (last * s).extend(0.0);
                } else {
                    // a railing has no thickness to change: a side moves it
                    for p in path.iter_mut() {
                        *p += (flat * s).extend(0.0);
                    }
                }
            }
        },
        ElementKind::Opening(o) => {
            let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
                return None;
            };
            let u = w.direction();
            match look {
                Facing::Top => o.height += s,
                Facing::Bottom => {
                    o.sill -= s;
                    o.height += s;
                }
                Facing::Side(flat) if flat.dot(u) > 0.9 => o.width += s,
                Facing::Side(flat) if flat.dot(u) < -0.9 => {
                    o.offset -= s;
                    o.width += s;
                }
                Facing::Side(_) | Facing::Sloped => return None,
            }
            let fits = o.offset >= -1e-9
                && o.sill >= -1e-9
                && o.offset + o.width <= w.length() + 1e-9
                && o.sill + o.height <= w.height + 1e-9
                && o.width > 0.1
                && o.height > 0.1;
            if !fits {
                return None;
            }
        }
        ElementKind::Object(o) => match look {
            Facing::Top => o.offset += s,
            Facing::Bottom => o.offset -= s,
            Facing::Side(flat) => o.position += flat * s,
            Facing::Sloped => return None,
        },
        _ => return None,
    }
    check(out, first)
}

/// The roof edge whose eave or gable end faces `want`: of those facing
/// that way, the nearest to `p`.
fn roof_edge(boundary: &[DVec2], want: DVec2, p: DVec2) -> Option<usize> {
    let n = boundary.len();
    let ccw = crate::polygon::signed_area2(boundary) > 0.0;
    (0..n)
        .filter_map(|i| {
            let (a, b) = (boundary[i], boundary[(i + 1) % n]);
            let e = (b - a).normalize_or_zero();
            let out = if ccw { -e.perp() } else { e.perp() };
            let off = (p - a).dot(out).abs();
            (out.dot(want) > 0.99).then_some((i, off))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Keep a push that leaves the element a sound shape.
fn check(out: Vec<Element>, before: &Element) -> Option<Vec<Element>> {
    let ok = match &out[0].kind {
        ElementKind::Wall(w) => w.thickness > 0.01 && w.height > 0.05 && w.length() > 0.05,
        ElementKind::Slab(s) => {
            s.thickness > 0.01 && crate::polygon::signed_area2(&s.boundary).abs() > 1e-4
        }
        ElementKind::Column(c) => c.height.is_none_or(|h| h > 0.05),
        ElementKind::Beam(b) => b.length() > 0.05 && b.width > 0.01 && b.depth > 0.01,
        ElementKind::Roof(r) => {
            r.thickness > 0.01 && crate::polygon::signed_area2(&r.boundary).abs() > 1e-4
        }
        ElementKind::Stair(s) => {
            s.width >= 0.3 && s.tread > 0.05 && s.height.is_none_or(|h| h > 0.1)
        }
        ElementKind::Railing(r) => r.height > 0.1,
        _ => true,
    };
    (ok && out[0].validate().is_ok() && out[0] != *before).then_some(out)
}

/// The straight slab edge the face lies on: facing the same way and
/// through the picked point.
fn slab_edge(boundary: &[DVec2], bulges: &[f64], face: &FaceRef) -> Option<usize> {
    let n = boundary.len();
    let ccw = crate::polygon::signed_area2(boundary) > 0.0;
    let want = face.normal.truncate().normalize_or_zero();
    let p = face.point.truncate();
    (0..n)
        .filter(|&i| bulges.get(i).is_none_or(|b| b.abs() < 1e-9))
        .filter_map(|i| {
            let (a, b) = (boundary[i], boundary[(i + 1) % n]);
            let e = (b - a).normalize_or_zero();
            let out = if ccw { -e.perp() } else { e.perp() };
            let off = (p - a).dot(out).abs();
            (out.dot(want) > 0.99 && off < 0.01).then_some((i, off))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sample::sample_house, Opening};

    fn first_of(model: &Model, pick: impl Fn(&ElementKind) -> bool) -> ElementId {
        model.iter().find(|e| pick(&e.kind)).unwrap().id
    }

    #[test]
    fn a_walls_faces_change_its_length_height_and_thickness() {
        let model = sample_house();
        let id = first_of(&model, |k| matches!(k, ElementKind::Wall(_)));
        let els = affected(&model, id);
        let ElementKind::Wall(w) = &els[0].kind else {
            panic!()
        };
        let (d, across) = (w.direction(), w.direction().perp());
        let mid = ((w.start + w.end) / 2.0).extend(1.0);
        let face = |normal: DVec3| FaceRef {
            point: mid,
            normal,
            mesh_face: None,
        };
        let wall = |v: &[Element]| match &v[0].kind {
            ElementKind::Wall(w) => w.clone(),
            _ => panic!(),
        };
        let top = wall(&push_face(&model, &els, &face(DVec3::Z), 0.5).unwrap());
        assert!((top.height - w.height - 0.5).abs() < 1e-9);
        let end = wall(&push_face(&model, &els, &face(d.extend(0.0)), 1.0).unwrap());
        assert!((end.length() - w.length() - 1.0).abs() < 1e-9);
        assert_eq!(end.start, w.start);
        // a side face: thicker, the other face where it was
        let side = wall(&push_face(&model, &els, &face(across.extend(0.0)), 0.1).unwrap());
        assert!((side.thickness - w.thickness - 0.1).abs() < 1e-9);
        let far = |x: &crate::Wall| (x.start - across * x.thickness / 2.0).dot(across);
        assert!((far(&side) - far(w)).abs() < 1e-9);
        // pulling the start back keeps the windows where they are
        let back = push_face(&model, &els, &face(-d.extend(0.0)), 0.5).unwrap();
        assert!(back.len() > 1);
        for (old, new) in els.iter().zip(&back).skip(1) {
            let (ElementKind::Opening(a), ElementKind::Opening(b)) = (&old.kind, &new.kind) else {
                panic!()
            };
            assert!((b.offset - a.offset - 0.5).abs() < 1e-9);
            let _: &Opening = a;
        }
        // a wall can't be pulled thinner than nothing
        assert!(push_face(&model, &els, &face(across.extend(0.0)), -1.0).is_none());
    }

    #[test]
    fn a_slabs_top_thickens_it_and_an_edge_moves() {
        let model = sample_house();
        let id = first_of(&model, |k| matches!(k, ElementKind::Slab(_)));
        let els = affected(&model, id);
        let ElementKind::Slab(s) = &els[0].kind else {
            panic!()
        };
        let top = push_face(
            &model,
            &els,
            &FaceRef {
                point: DVec3::new(5.0, 3.0, 0.0),
                normal: DVec3::Z,
                mesh_face: None,
            },
            0.1,
        )
        .unwrap();
        let ElementKind::Slab(t) = &top[0].kind else {
            panic!()
        };
        assert!((t.thickness - s.thickness - 0.1).abs() < 1e-9);
        assert!((t.top_offset - 0.1).abs() < 1e-9);
        // the sample's slab is 10 x 6: its east edge pushed out a metre
        let east = push_face(
            &model,
            &els,
            &FaceRef {
                point: DVec3::new(10.0, 3.0, -0.1),
                normal: DVec3::X,
                mesh_face: None,
            },
            1.0,
        )
        .unwrap();
        let ElementKind::Slab(e) = &east[0].kind else {
            panic!()
        };
        let area = crate::polygon::signed_area2(&e.boundary).abs(); // twice the area
        assert!((area - 2.0 * 66.0).abs() < 1e-9);
    }

    /// `kind` with the face through `point` facing `normal` pushed by `s`.
    #[test]
    fn an_object_moves_whole_by_its_faces() {
        let model = crate::sample::furnished_house();
        let e = model
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Object(_)))
            .unwrap();
        let ElementKind::Object(o) = &e.kind else {
            unreachable!()
        };
        assert!(can_push(&e.kind));
        let at = o.position.extend(0.5);
        let Some(ElementKind::Object(up)) = pushed(&model, e.kind.clone(), at, DVec3::Z, 0.3)
        else {
            panic!("its top raises it")
        };
        assert!((up.offset - o.offset - 0.3).abs() < 1e-9);
        assert_eq!(up.position, o.position);
        let Some(ElementKind::Object(east)) = pushed(&model, e.kind.clone(), at, DVec3::X, 0.25)
        else {
            panic!("a side slides it")
        };
        assert!((east.position - o.position - DVec2::new(0.25, 0.0)).length() < 1e-9);
        assert_eq!(east.rotation, o.rotation);
        assert!(pushed(&model, e.kind.clone(), at, DVec3::new(0.6, 0.0, 0.8), 0.1).is_none());
    }

    fn pushed(
        model: &Model,
        kind: ElementKind,
        point: DVec3,
        normal: DVec3,
        s: f64,
    ) -> Option<ElementKind> {
        let e = Element::new("Test", kind);
        let face = FaceRef {
            point,
            normal,
            mesh_face: None,
        };
        push_face(model, &[e], &face, s).map(|v| v[0].kind.clone())
    }

    fn level(model: &Model) -> ElementId {
        model.levels().next().unwrap().0.id
    }

    #[test]
    fn columns_and_beams_stretch_by_their_faces() {
        use crate::members::{Beam, Column};
        let model = sample_house();
        let l = level(&model);
        let col = Column::new(
            l,
            DVec2::new(1.0, 1.0),
            Profile::Rectangle {
                width: 0.4,
                depth: 0.3,
            },
        );
        let h = col.height_in(&model);
        let at = DVec3::new(1.0, 1.0, 1.0);
        let Some(ElementKind::Column(top)) =
            pushed(&model, ElementKind::Column(col.clone()), at, DVec3::Z, 0.5)
        else {
            panic!()
        };
        assert!((top.height.unwrap() - h - 0.5).abs() < 1e-9);
        let Some(ElementKind::Column(low)) =
            pushed(&model, ElementKind::Column(col.clone()), at, -DVec3::Z, 0.2)
        else {
            panic!()
        };
        assert!((low.base_offset + 0.2).abs() < 1e-9);
        assert!((low.height.unwrap() - h - 0.2).abs() < 1e-9);
        // its east side out 0.1: wider, the west side where it was
        let Some(ElementKind::Column(wide)) =
            pushed(&model, ElementKind::Column(col.clone()), at, DVec3::X, 0.1)
        else {
            panic!()
        };
        assert_eq!(
            wide.profile,
            Profile::Rectangle {
                width: 0.5,
                depth: 0.3
            }
        );
        assert!((wide.outline()[0].x - col.outline()[0].x).abs() < 1e-9);

        let beam = Beam::new(l, DVec2::new(0.0, 0.0), DVec2::new(4.0, 0.0));
        let Some(ElementKind::Beam(b)) = pushed(
            &model,
            ElementKind::Beam(beam.clone()),
            DVec3::new(4.0, 0.0, 2.5),
            DVec3::X,
            1.0,
        ) else {
            panic!()
        };
        assert_eq!((b.start, b.end), (beam.start, DVec2::new(5.0, 0.0)));
        let Some(ElementKind::Beam(b)) = pushed(
            &model,
            ElementKind::Beam(beam.clone()),
            DVec3::new(2.0, 0.0, 2.0),
            -DVec3::Z,
            0.1,
        ) else {
            panic!()
        };
        assert!((b.depth - beam.depth - 0.1).abs() < 1e-9);
        // a sloping face isn't one of a beam's
        assert!(pushed(
            &model,
            ElementKind::Beam(beam),
            DVec3::ZERO,
            DVec3::new(1.0, 0.0, 1.0),
            0.1
        )
        .is_none());
    }

    #[test]
    fn a_roofs_eave_moves_and_its_top_thickens() {
        use crate::members::{Roof, RoofShape};
        let model = sample_house();
        let square = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(6.0, 0.0),
            DVec2::new(6.0, 4.0),
            DVec2::new(0.0, 4.0),
        ];
        let roof = Roof::new(level(&model), square, RoofShape::Hip);
        // the east eave, beyond the overhang
        let Some(ElementKind::Roof(r)) = pushed(
            &model,
            ElementKind::Roof(roof.clone()),
            DVec3::new(6.5, 2.0, 3.0),
            DVec3::X,
            1.0,
        ) else {
            panic!()
        };
        assert_eq!(r.boundary[1], DVec2::new(7.0, 0.0));
        assert_eq!(r.boundary[2], DVec2::new(7.0, 4.0));
        assert_eq!(r.boundary[0], DVec2::ZERO);
        // a slope pushed out is a thicker roof
        let slope = DVec3::new(0.5, 0.0, 0.866);
        let Some(ElementKind::Roof(r)) = pushed(
            &model,
            ElementKind::Roof(roof.clone()),
            DVec3::ZERO,
            slope,
            0.05,
        ) else {
            panic!()
        };
        assert!((r.thickness - roof.thickness - 0.05).abs() < 1e-9);
        assert_eq!(r.boundary, roof.boundary);
    }

    #[test]
    fn a_stair_widens_by_a_side_and_climbs_by_its_top() {
        use crate::members::Stair;
        let model = sample_house();
        let stair = Stair::new(level(&model), DVec2::ZERO, 0.0);
        let rise = stair.rise(&model);
        let Some(ElementKind::Stair(s)) = pushed(
            &model,
            ElementKind::Stair(stair.clone()),
            DVec3::new(1.0, 0.5, 0.0),
            DVec3::Y,
            0.2,
        ) else {
            panic!()
        };
        assert!((s.width - 1.2).abs() < 1e-9);
        // the right side stays
        assert!((s.start.y - 0.1).abs() < 1e-9);
        let Some(ElementKind::Stair(s)) = pushed(
            &model,
            ElementKind::Stair(stair.clone()),
            DVec3::new(1.0, 0.0, rise),
            DVec3::Z,
            0.3,
        ) else {
            panic!()
        };
        assert!((s.height.unwrap() - rise - 0.3).abs() < 1e-9);
        // the first riser pulled back: the start goes back, treads longer
        let Some(ElementKind::Stair(s)) = pushed(
            &model,
            ElementKind::Stair(stair.clone()),
            DVec3::new(0.0, 0.0, 0.1),
            -DVec3::X,
            0.5,
        ) else {
            panic!()
        };
        assert!((s.start.x + 0.5).abs() < 1e-9);
        assert!(s.tread > stair.tread);
        // a riser half way up is not the start
        let mid = DVec3::new(stair.tread * 4.0, 0.0, 0.5);
        assert!(pushed(&model, ElementKind::Stair(stair), mid, -DVec3::X, 0.5).is_none());
    }

    #[test]
    fn a_railing_rises_and_a_door_widens_by_its_jamb() {
        use crate::members::Railing;
        let model = sample_house();
        let rail = Railing::new(level(&model), vec![DVec3::ZERO, DVec3::new(3.0, 0.0, 0.0)]);
        let Some(ElementKind::Railing(r)) = pushed(
            &model,
            ElementKind::Railing(rail.clone()),
            DVec3::new(1.0, 0.0, 1.0),
            DVec3::Z,
            0.1,
        ) else {
            panic!()
        };
        assert!((r.height - 1.1).abs() < 1e-9);
        let Some(ElementKind::Railing(r)) = pushed(
            &model,
            ElementKind::Railing(rail),
            DVec3::new(3.0, 0.0, 0.5),
            DVec3::X,
            1.0,
        ) else {
            panic!()
        };
        assert_eq!(r.path[1], DVec3::new(4.0, 0.0, 0.0));

        let (id, o) = model
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Opening(o) => Some((e.id, o.clone())),
                _ => None,
            })
            .unwrap();
        let ElementKind::Wall(w) = &model.get(o.host).unwrap().kind else {
            panic!()
        };
        let u = w.direction().extend(0.0);
        let els = affected(&model, id);
        let face = FaceRef {
            point: DVec3::ZERO,
            normal: -u,
            mesh_face: None,
        };
        let wider = push_face(&model, &els, &face, 0.1).unwrap();
        let ElementKind::Opening(n) = &wider[0].kind else {
            panic!()
        };
        assert!((n.width - o.width - 0.1).abs() < 1e-9);
        assert!((n.offset - o.offset + 0.1).abs() < 1e-9);
        // not wider than its wall
        assert!(push_face(&model, &els, &face, 100.0).is_none());
    }
}
