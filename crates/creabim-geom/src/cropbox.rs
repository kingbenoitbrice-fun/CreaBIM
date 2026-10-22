//! Cropping a scene with crop boxes (see [`creabim_core::cropbox`]). Each
//! element a box crops keeps only what is inside it, cut plane by plane,
//! and every cut is closed with a cap so the element still reads as solid.
//! Caps are items of their own, with the element's id and a darker shade
//! of its look, so they pick and highlight with it and their outlines draw
//! as the cut edges.

use crate::scene::SceneItem;
use crate::TriMesh;
use creabim_core::{cropbox, CropBox, ElementId, ElementKind, Model};
use glam::{DVec2, DVec3, Vec3};
use std::collections::HashMap;

/// The switched-on crop boxes of a 3D view (`None`: the 3D window's own).
pub fn active(model: &Model, view: Option<ElementId>) -> Vec<CropBox> {
    cropbox::crop_boxes(model, view)
        .into_iter()
        .filter(|(_, c)| c.enabled)
        .map(|(_, c)| c.clone())
        .collect()
}

/// How much darker a cut face is than the element's surface.
const CAP_SHADE: f32 = 0.78;

/// Crop `scene` with `boxes`. An element several boxes crop keeps what is
/// inside all of them; elements no box crops are left alone. Returns the
/// outlines of the cuts, for drawing them as edges.
pub fn crop_scene(
    model: &Model,
    scene: &mut Vec<SceneItem>,
    boxes: &[CropBox],
) -> Vec<(DVec3, DVec3)> {
    if boxes.is_empty() {
        return vec![];
    }
    // doors and windows go with the wall they are in
    let hosts: HashMap<ElementId, ElementId> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Opening(o) => Some((e.id, o.host)),
            _ => None,
        })
        .collect();
    let mut out = Vec::with_capacity(scene.len());
    let mut edges = Vec::new();
    for item in scene.drain(..) {
        let host = hosts.get(&item.id).copied();
        let mine: Vec<&CropBox> = boxes.iter().filter(|b| b.crops(item.id, host)).collect();
        if mine.is_empty() {
            out.push(item);
            continue;
        }
        let Some((body, caps)) = crop_mesh(&item.mesh, &mine, !item.ghost) else {
            continue; // all of it is outside
        };
        let cap = (!caps.is_empty()).then(|| SceneItem {
            mesh: caps,
            color: item.color.map(|c| c * CAP_SHADE),
            ..item.clone()
        });
        if !body.is_empty() {
            out.push(SceneItem { mesh: body, ..item });
        }
        if let Some(cap) = cap {
            edges.extend(crate::section::feature_edges(&cap));
            out.push(cap);
        }
    }
    *scene = out;
    edges
}

/// What of `mesh` is inside every box: the cut surface, and the caps that
/// close it (when `capped`). `None` when nothing is left; the same mesh
/// and no caps when it is inside them all.
pub fn crop_mesh(mesh: &TriMesh, boxes: &[&CropBox], capped: bool) -> Option<(TriMesh, TriMesh)> {
    let (lo, hi) = mesh.bounds()?;
    let (lo, hi) = (lo.as_dvec3(), hi.as_dvec3());
    let mut body = mesh.clone();
    let mut caps = TriMesh::default();
    for b in boxes {
        if (0..3).any(|a| hi[a] < b.min[a] || lo[a] > b.max[a]) {
            return None;
        }
        for face in 0..6 {
            let axis = face / 2;
            // keep `side * (value - p) >= 0`: below the top, above the bottom
            let (value, side) = if face % 2 == 0 {
                (b.min[axis], -1.0)
            } else {
                (b.max[axis], 1.0)
            };
            if side * (value - hi[axis]) >= 0.0 && side * (value - lo[axis]) >= 0.0 {
                continue; // all of it is on the kept side
            }
            let cap = if capped {
                let mut closed = body.clone();
                closed.append(&caps);
                cap_at(&closed, axis, value, side)
            } else {
                TriMesh::default()
            };
            body = clip(&body, axis, value, side);
            caps = clip(&caps, axis, value, side);
            caps.append(&cap);
        }
    }
    if body.is_empty() && caps.is_empty() {
        return None;
    }
    Some((body, caps))
}

/// The part of `mesh` with `side * (value - p[axis]) >= 0`.
fn clip(mesh: &TriMesh, axis: usize, value: f64, side: f64) -> TriMesh {
    let mut out = TriMesh::default();
    let at = |i: u32| Vec3::from(mesh.positions[i as usize]).as_dvec3();
    for t in mesh.indices.chunks(3) {
        let p = [at(t[0]), at(t[1]), at(t[2])];
        let d = p.map(|q| side * (value - q[axis]));
        let n = mesh.normals[t[0] as usize];
        if d.iter().all(|v| *v >= 0.0) {
            let base = out.positions.len() as u32;
            for q in p {
                out.positions.push(q.as_vec3().to_array());
                out.normals.push(n);
            }
            out.indices.extend([base, base + 1, base + 2]);
            continue;
        }
        if d.iter().all(|v| *v <= 0.0) {
            continue;
        }
        // Sutherland–Hodgman against the one plane
        let mut poly = Vec::with_capacity(4);
        for i in 0..3 {
            let j = (i + 1) % 3;
            if d[i] >= 0.0 {
                poly.push(p[i]);
            }
            if (d[i] >= 0.0) != (d[j] >= 0.0) {
                let s = d[i] / (d[i] - d[j]);
                let mut q = p[i] + (p[j] - p[i]) * s;
                q[axis] = value;
                poly.push(q);
            }
        }
        let base = out.positions.len() as u32;
        for q in &poly {
            out.positions.push(q.as_vec3().to_array());
            out.normals.push(n);
        }
        for k in 1..poly.len().saturating_sub(1) as u32 {
            out.indices.extend([base, base + k, base + k + 1]);
        }
    }
    out
}

/// The faces closing a closed `mesh` where the plane `p[axis] = value`
/// cuts it, facing out of the kept side.
fn cap_at(mesh: &TriMesh, axis: usize, value: f64, side: f64) -> TriMesh {
    // turn the axes round so the plane is level, slice, and turn back
    let to_flat = |p: [f32; 3]| match axis {
        0 => [p[1], p[2], p[0]],
        1 => [p[2], p[0], p[1]],
        _ => p,
    };
    let from_flat = |q: DVec2| match axis {
        0 => DVec3::new(value, q.x, q.y),
        1 => DVec3::new(q.y, value, q.x),
        _ => DVec3::new(q.x, q.y, value),
    };
    let flat = TriMesh {
        positions: mesh.positions.iter().map(|p| to_flat(*p)).collect(),
        normals: mesh.normals.iter().map(|n| to_flat(*n)).collect(),
        indices: mesh.indices.clone(),
    };
    let mut normal = DVec3::ZERO;
    normal[axis] = side;
    let mut out = TriMesh::default();
    for outline in crate::slice::slice_closed(&flat, value) {
        let tris = crate::triangulate::triangulate(&outline);
        let pts: Vec<DVec3> = outline.iter().map(|q| from_flat(*q)).collect();
        out.add_polygon(&pts, &tris, normal);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::{CropScope, EditMesh};

    fn cuboid(size: DVec3, at: DVec3) -> TriMesh {
        crate::scene::edit_mesh_to_tri(&EditMesh::cuboid(size), at)
    }

    fn bounds(m: &TriMesh) -> (DVec3, DVec3) {
        let (lo, hi) = m.bounds().unwrap();
        (lo.as_dvec3(), hi.as_dvec3())
    }

    #[test]
    fn a_cut_box_stays_closed_with_caps_on_its_cut_faces() {
        let m = cuboid(DVec3::new(4.0, 4.0, 4.0), DVec3::ZERO);
        let (lo, hi) = bounds(&m);
        let mid = (lo + hi) / 2.0;
        // a crop box over one corner of the block
        let c = CropBox::new(None, mid - DVec3::splat(10.0), mid);
        let (body, caps) = crop_mesh(&m, &[&c], true).unwrap();
        let mut whole = body.clone();
        whole.append(&caps);
        // an eighth of the block is left, and it is closed
        assert!((whole.volume() - 8.0).abs() < 1e-3, "{}", whole.volume());
        let (l, h) = bounds(&whole);
        assert!((h - mid).abs().max_element() < 1e-5 && (l - lo).abs().max_element() < 1e-5);
        // three cut faces of 2 × 2
        let area: f64 = caps
            .indices
            .chunks(3)
            .map(|t| {
                let p = |i: u32| Vec3::from(caps.positions[i as usize]).as_dvec3();
                (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0])).length() / 2.0
            })
            .sum();
        assert!((area - 12.0).abs() < 1e-3, "{area}");
    }

    #[test]
    fn what_is_inside_is_untouched_and_what_is_outside_goes() {
        let m = cuboid(DVec3::ONE, DVec3::ZERO);
        let (lo, hi) = bounds(&m);
        let around = CropBox::new(None, lo - DVec3::ONE, hi + DVec3::ONE);
        let (body, caps) = crop_mesh(&m, &[&around], true).unwrap();
        assert_eq!(body, m);
        assert!(caps.is_empty());
        let away = CropBox::new(None, hi + DVec3::ONE, hi + DVec3::splat(2.0));
        assert!(crop_mesh(&m, &[&away], true).is_none());
    }

    #[test]
    fn a_box_for_some_elements_leaves_the_others_whole() {
        let model = Model::new();
        let (a_id, b_id) = (ElementId::new(), ElementId::new());
        let item = |id, at: DVec3| SceneItem {
            id,
            mesh: cuboid(DVec3::splat(2.0), at),
            color: [0.5; 3],
            surface: crate::scene::PLAIN_SURFACE,
            ghost: false,
            layer: None,
            finish: None,
        };
        let mut scene = vec![item(a_id, DVec3::ZERO), item(b_id, DVec3::ZERO)];
        let (lo, hi) = bounds(&scene[0].mesh);
        let mut c = CropBox::new(
            None,
            lo - DVec3::ONE,
            (lo + hi) / 2.0 + DVec3::new(10.0, 10.0, 0.0),
        );
        c.scope = CropScope::Only(vec![a_id]);
        let edges = crop_scene(&model, &mut scene, &[c]);
        assert!(!edges.is_empty(), "the cut is outlined");
        // a: its lower half and a cap on top; b: whole
        let a_items: Vec<&SceneItem> = scene.iter().filter(|i| i.id == a_id).collect();
        assert_eq!(a_items.len(), 2);
        assert!(a_items[1].color[0] < 0.5, "the cap is darker");
        let vol: f64 = a_items.iter().map(|i| i.mesh.volume()).sum();
        assert!((vol - 4.0).abs() < 1e-3, "{vol}");
        let b: Vec<&SceneItem> = scene.iter().filter(|i| i.id == b_id).collect();
        assert_eq!(b.len(), 1);
        assert!((b[0].mesh.volume() - 8.0).abs() < 1e-6);
    }

    #[test]
    fn two_boxes_keep_what_is_inside_both() {
        let m = cuboid(DVec3::splat(4.0), DVec3::ZERO);
        let (lo, _) = bounds(&m);
        let a = CropBox::new(None, lo - DVec3::ONE, lo + DVec3::new(3.0, 10.0, 10.0));
        let b = CropBox::new(
            None,
            lo + DVec3::new(1.0, -1.0, -1.0),
            lo + DVec3::splat(10.0),
        );
        let (body, caps) = crop_mesh(&m, &[&a, &b], true).unwrap();
        let mut whole = body;
        whole.append(&caps);
        // x from 1 to 3 of the 4 m block
        assert!((whole.volume() - 32.0).abs() < 1e-3, "{}", whole.volume());
    }

    #[test]
    fn a_3d_view_on_a_sheet_draws_cropped_by_its_own_boxes() {
        use creabim_core::{Document, Element, Transaction, ViewKind};
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let view = doc
            .model()
            .iter()
            .find(|e| matches!(&e.kind, ElementKind::View(v) if matches!(v.kind, ViewKind::Axonometric { .. })))
            .unwrap()
            .id;
        let height = |m: &Model| {
            let (lo, hi) = crate::section::view_drawing(m, view)
                .unwrap()
                .bounds()
                .unwrap();
            hi.y - lo.y
        };
        let whole = height(doc.model());
        // a box over the ground floor, for the 3D window: the view is not cropped
        let low = |owner| {
            Element::new(
                "Crop box 1",
                ElementKind::CropBox(CropBox::new(
                    owner,
                    DVec3::new(-50.0, -50.0, -5.0),
                    DVec3::new(50.0, 50.0, 1.0),
                )),
            )
        };
        doc.commit(Transaction::new("add").insert(low(None)))
            .unwrap();
        assert!((height(doc.model()) - whole).abs() < 1e-6);
        // the view's own box crops its drawing
        doc.commit(Transaction::new("add").insert(low(Some(view))))
            .unwrap();
        let cropped = height(doc.model());
        assert!(cropped < whole * 0.8, "{cropped} vs {whole}");
    }
}
