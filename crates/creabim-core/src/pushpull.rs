//! Push and pull: one face of an element moved along its normal, the
//! element's own parameters following. A wall's end face lengthens it, its
//! top raises it and a side thickens it; a slab's top or bottom thickens it
//! and an edge face moves that edge; a mass's face moves its vertices.

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
        ElementKind::Wall(_) | ElementKind::Slab(_) | ElementKind::FreeMesh(_)
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

/// `elements` (as [`affected`] lists them) with the face pushed out by `s`
/// along its normal (pulled in when negative). `None` when the shape
/// can't take it: the face isn't one that moves, or it would turn the
/// element inside out.
pub fn push_face(elements: &[Element], face: &FaceRef, s: f64) -> Option<Vec<Element>> {
    let first = elements.first()?;
    let mut out = elements.to_vec();
    let n = face.normal.normalize_or_zero();
    let up = n.z.abs() > 0.9;
    match &mut out[0].kind {
        ElementKind::Wall(w) => {
            let d = w.direction();
            let across = d.perp();
            let flat = n.truncate();
            if up && n.z > 0.0 {
                w.height += s;
            } else if up {
                w.base_offset -= s;
                w.height += s;
            } else if flat.dot(d).abs() >= flat.dot(across).abs() {
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
            } else {
                // the far face stays put: the centre line moves half way
                let side = flat.dot(across).signum();
                w.thickness += s;
                w.start += across * side * s / 2.0;
                w.end += across * side * s / 2.0;
            }
        }
        ElementKind::Slab(sl) => {
            if up && n.z > 0.0 {
                sl.top_offset += s;
                sl.thickness += s;
            } else if up {
                sl.thickness += s;
            } else {
                let i = slab_edge(&sl.boundary, &sl.bulges, face)?;
                let k = (i + 1) % sl.boundary.len();
                let step = n.truncate().normalize_or_zero() * s;
                sl.boundary[i] += step;
                sl.boundary[k] += step;
            }
        }
        ElementKind::FreeMesh(m) => {
            let f = face.mesh_face?;
            let verts = m.mesh.faces.get(f)?.clone();
            let local = m.to_local(n * s);
            m.mesh.translate_vertices(&verts, local);
        }
        _ => return None,
    }
    check(out, first)
}

/// Keep a push that leaves the element a sound shape.
fn check(out: Vec<Element>, before: &Element) -> Option<Vec<Element>> {
    let ok = match &out[0].kind {
        ElementKind::Wall(w) => w.thickness > 0.01 && w.height > 0.05 && w.length() > 0.05,
        ElementKind::Slab(s) => {
            s.thickness > 0.01 && crate::polygon::signed_area2(&s.boundary).abs() > 1e-4
        }
        _ => true,
    };
    (ok && out[0] != *before).then_some(out)
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
        let top = wall(&push_face(&els, &face(DVec3::Z), 0.5).unwrap());
        assert!((top.height - w.height - 0.5).abs() < 1e-9);
        let end = wall(&push_face(&els, &face(d.extend(0.0)), 1.0).unwrap());
        assert!((end.length() - w.length() - 1.0).abs() < 1e-9);
        assert_eq!(end.start, w.start);
        // a side face: thicker, the other face where it was
        let side = wall(&push_face(&els, &face(across.extend(0.0)), 0.1).unwrap());
        assert!((side.thickness - w.thickness - 0.1).abs() < 1e-9);
        let far = |x: &crate::Wall| (x.start - across * x.thickness / 2.0).dot(across);
        assert!((far(&side) - far(w)).abs() < 1e-9);
        // pulling the start back keeps the windows where they are
        let back = push_face(&els, &face(-d.extend(0.0)), 0.5).unwrap();
        assert!(back.len() > 1);
        for (old, new) in els.iter().zip(&back).skip(1) {
            let (ElementKind::Opening(a), ElementKind::Opening(b)) = (&old.kind, &new.kind) else {
                panic!()
            };
            assert!((b.offset - a.offset - 0.5).abs() < 1e-9);
            let _: &Opening = a;
        }
        // a wall can't be pulled thinner than nothing
        assert!(push_face(&els, &face(across.extend(0.0)), -1.0).is_none());
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
}
