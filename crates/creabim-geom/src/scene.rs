use crate::{triangulate::triangulate, wall::wall_mesh, TriMesh};
use creabim_core::{csg, BoolOp, ElementId, ElementKind, Model, Opening};
use glam::{DVec2, DVec3};

/// One element's renderable geometry.
#[derive(Clone, Debug)]
pub struct SceneItem {
    pub id: ElementId,
    pub mesh: TriMesh,
    pub color: [f32; 3],
    /// A cutter: drawn as a see-through outline in 3D, left out of
    /// drawings and exports, but still clickable.
    pub ghost: bool,
}

/// The lines of every sketch, in model coordinates: (sketch, from, to).
pub fn sketch_lines(model: &Model) -> Vec<(ElementId, DVec3, DVec3)> {
    let mut out = Vec::new();
    for e in model.iter() {
        if let ElementKind::Sketch(s) = &e.kind {
            let z = s
                .level
                .and_then(|l| model.level(l))
                .map_or(0.0, |l| l.elevation);
            for (a, b) in s.segments() {
                out.push((e.id, a + DVec3::Z * z, b + DVec3::Z * z));
            }
        }
    }
    out
}

/// Build viewport geometry for every element with a body.
pub fn build_scene(model: &Model) -> Vec<SceneItem> {
    let mut out = Vec::new();
    let joins = creabim_core::wall_joins(model);
    for e in model.iter() {
        let (mesh, color) = match &e.kind {
            ElementKind::Wall(w) => {
                let Some(level) = model.level(w.level) else {
                    continue;
                };
                let openings: Vec<&Opening> = model
                    .dependents(e.id)
                    .into_iter()
                    .filter_map(|d| match &model.get(d)?.kind {
                        ElementKind::Opening(o) => Some(o),
                        _ => None,
                    })
                    .collect();
                let ends = joins.get(&e.id).copied().unwrap_or_default();
                (
                    wall_mesh(w, level.elevation, &openings, &ends),
                    [0.86, 0.84, 0.80],
                )
            }
            ElementKind::Slab(s) => {
                let Some(level) = model.level(s.level) else {
                    continue;
                };
                (
                    slab_mesh(&s.outline(), s.top(level.elevation), s.thickness),
                    [0.70, 0.70, 0.72],
                )
            }
            ElementKind::Opening(o) => {
                let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
                    continue;
                };
                let Some(level) = model.level(w.level) else {
                    continue;
                };
                (opening_panel(o, w, level.elevation), opening_color(o))
            }
            ElementKind::FreeMesh(f) => {
                let z = f
                    .level
                    .and_then(|l| model.level(l))
                    .map_or(0.0, |l| l.elevation);
                (
                    edit_mesh_to_tri(&f.evaluated().mesh, f.origin + DVec3::Z * z),
                    [0.62, 0.75, 0.90],
                )
            }
            ElementKind::Level(_)
            | ElementKind::Grid(_)
            | ElementKind::View(_)
            | ElementKind::Sheet(_)
            | ElementKind::Viewport(_)
            | ElementKind::Material(_)
            | ElementKind::Dimension(_)
            | ElementKind::Tag(_)
            | ElementKind::Sketch(_)
            | ElementKind::Cut(_)
            | ElementKind::Guide(_)
            | ElementKind::GridGroup(_) => continue,
            ElementKind::Column(_)
            | ElementKind::Beam(_)
            | ElementKind::Roof(_)
            | ElementKind::Stair(_)
            | ElementKind::Railing(_) => match crate::members::member_mesh(model, e) {
                Some(m) => m,
                None => continue,
            },
        };
        // a material's colour wins over the element type's default
        let color = creabim_core::material_of(model, e).map_or(color, |(_, m)| m.color);
        out.push(SceneItem {
            id: e.id,
            mesh,
            color,
            ghost: false,
        });
    }
    apply_cuts(model, &mut out);
    out
}

/// The scene as drawings show it: cutters left out.
pub fn drawn_scene(model: &Model) -> Vec<SceneItem> {
    let mut s = build_scene(model);
    s.retain(|i| !i.ghost);
    s
}

/// Every cut in the model, in order: (target, cutter, operation).
pub fn cuts(model: &Model) -> Vec<(ElementId, ElementId, BoolOp)> {
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Cut(c) => Some((c.target, c.cutter, c.op)),
            _ => None,
        })
        .collect()
}

/// Is `id` used to cut something? Cutters are hidden from drawings.
pub fn is_cutter(model: &Model, id: ElementId) -> bool {
    model
        .iter()
        .any(|e| matches!(&e.kind, ElementKind::Cut(c) if c.cutter == id))
}

/// Apply every cut to the meshes of its target, using the cutters' own
/// shapes (a cutter that is itself cut cuts with its uncut shape, so cuts
/// can never loop), and turn the cutters into ghosts.
fn apply_cuts(model: &Model, items: &mut [SceneItem]) {
    let cuts = cuts(model);
    if cuts.is_empty() {
        return;
    }
    let original: std::collections::HashMap<ElementId, TriMesh> = cuts
        .iter()
        .filter_map(|(_, c, _)| {
            let item = items.iter().find(|i| i.id == *c)?;
            Some((*c, item.mesh.clone()))
        })
        .collect();
    for (target, cutter, op) in &cuts {
        let Some(tool) = original.get(cutter) else {
            continue;
        };
        if let Some(item) = items.iter_mut().find(|i| i.id == *target) {
            item.mesh = boolean_mesh(&item.mesh, tool, *op);
        }
    }
    for item in items.iter_mut() {
        if original.contains_key(&item.id) {
            item.ghost = true;
        }
    }
}

/// `a op b` for two closed triangle meshes.
pub fn boolean_mesh(a: &TriMesh, b: &TriMesh, op: BoolOp) -> TriMesh {
    let polys = |m: &TriMesh| -> Vec<csg::Polygon> {
        m.indices
            .chunks_exact(3)
            .filter_map(|t| {
                let p = |i: u32| {
                    let v = m.positions[i as usize];
                    DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64)
                };
                csg::Polygon::new(vec![p(t[0]), p(t[1]), p(t[2])])
            })
            .collect()
    };
    let result = csg::boolean(polys(a), polys(b), op);
    let mut out = TriMesh::default();
    for p in &result {
        let n = (p.points[1] - p.points[0])
            .cross(p.points[2] - p.points[0])
            .normalize_or_zero();
        let fan: Vec<[usize; 3]> = (1..p.points.len() - 1).map(|i| [0, i, i + 1]).collect();
        // the fan's winding already follows the polygon's
        out.add_polygon(&p.points, &fan, newell(&p.points).normalize_or(n));
    }
    out
}

fn newell(p: &[DVec3]) -> DVec3 {
    let mut n = DVec3::ZERO;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        n += DVec3::new(
            (a.y - b.y) * (a.z + b.z),
            (a.z - b.z) * (a.x + b.x),
            (a.x - b.x) * (a.y + b.y),
        );
    }
    n
}

fn opening_color(o: &Opening) -> [f32; 3] {
    match o.kind {
        creabim_core::OpeningKind::Door => [0.55, 0.38, 0.24],
        creabim_core::OpeningKind::Window => [0.55, 0.75, 0.90],
    }
}

/// Extruded slab: top face at `top_z`, `thickness` downwards.
pub fn slab_mesh(boundary: &[DVec2], top_z: f64, thickness: f64) -> TriMesh {
    let mut m = TriMesh::default();
    let tris = triangulate(boundary);
    let top: Vec<DVec3> = boundary.iter().map(|p| p.extend(top_z)).collect();
    let bottom: Vec<DVec3> = boundary
        .iter()
        .map(|p| p.extend(top_z - thickness))
        .collect();
    m.add_polygon(&top, &tris, DVec3::Z);
    m.add_polygon(&bottom, &tris, -DVec3::Z);
    let ccw = crate::triangulate::signed_area2(boundary) > 0.0;
    for i in 0..boundary.len() {
        let (a, b) = (boundary[i], boundary[(i + 1) % boundary.len()]);
        let e = (b - a).normalize_or_zero();
        let out = if ccw {
            DVec2::new(e.y, -e.x)
        } else {
            DVec2::new(-e.y, e.x)
        };
        m.add_quad(
            [
                bottom[i],
                bottom[(i + 1) % boundary.len()],
                top[(i + 1) % boundary.len()],
                top[i],
            ],
            out.extend(0.0),
        );
    }
    m
}

/// A thin panel filling the opening, so doors and windows show in 3D.
fn opening_panel(o: &Opening, w: &creabim_core::Wall, level_z: f64) -> TriMesh {
    let d = w.direction();
    let n = DVec2::new(-d.y, d.x);
    let half = 0.025;
    let z0 = level_z + w.base_offset + o.sill;
    let u0 = o.offset.max(0.0);
    let u1 = (o.offset + o.width).min(w.length());
    let mut m = TriMesh::default();
    let p = |u: f64, z: f64, s: f64| (w.start + d * u + n * s).extend(z);
    let (za, zb) = (z0, z0 + o.height);
    let across = n.extend(0.0);
    m.add_quad(
        [
            p(u0, za, half),
            p(u1, za, half),
            p(u1, zb, half),
            p(u0, zb, half),
        ],
        across,
    );
    m.add_quad(
        [
            p(u0, za, -half),
            p(u1, za, -half),
            p(u1, zb, -half),
            p(u0, zb, -half),
        ],
        -across,
    );
    m
}

/// Flat-shaded triangles from a polygon mesh, offset to `origin`.
pub fn edit_mesh_to_tri(mesh: &creabim_core::EditMesh, origin: DVec3) -> TriMesh {
    let mut m = TriMesh::default();
    for f in 0..mesh.faces.len() {
        let pts: Vec<DVec3> = mesh.faces[f]
            .iter()
            .map(|&v| mesh.vertices[v as usize] + origin)
            .collect();
        m.add_polygon(&pts, &mesh.face_corners(f), mesh.face_normal(f));
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slab_volume_matches_area_times_thickness() {
        let l = [
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 2.0),
            DVec2::new(2.0, 2.0),
            DVec2::new(2.0, 4.0),
            DVec2::new(0.0, 4.0),
        ];
        let m = slab_mesh(&l, 0.0, 0.25);
        assert!((m.volume() - 12.0 * 0.25).abs() < 1e-5);
        let mut cw = l.to_vec();
        cw.reverse();
        assert!((slab_mesh(&cw, 0.0, 0.25).volume() - 3.0).abs() < 1e-5);
    }

    #[test]
    fn a_slab_top_offset_lifts_its_mesh_and_arcs_round_it() {
        let mut model = creabim_core::sample::sample_house();
        let (id, elevation, t) = model
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Slab(s) => Some((e.id, model.level(s.level)?.elevation, s.thickness)),
                _ => None,
            })
            .unwrap();
        let mut e = model.get(id).unwrap().clone();
        let ElementKind::Slab(s) = &mut e.kind else {
            panic!()
        };
        // a 4 x 2 m slab whose far side is a half circle, its top 0.6 m up
        s.boundary = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 2.0),
            DVec2::new(0.0, 2.0),
        ];
        s.bulges = vec![0.0, 1.0, 0.0, 0.0];
        s.top_offset = 0.6;
        creabim_core::Transaction::new("edit")
            .replace(model.get(id).unwrap().clone(), e)
            .apply(&mut model)
            .unwrap();
        let scene = build_scene(&model);
        let item = scene.iter().find(|i| i.id == id).unwrap();
        let (lo, hi) = item.mesh.bounds().unwrap();
        assert!((hi.z as f64 - (elevation + 0.6)).abs() < 1e-5);
        assert!((lo.z as f64 - (elevation + 0.6 - t)).abs() < 1e-5);
        // the arc bulges a metre past the straight side
        assert!((hi.x - 5.0).abs() < 1e-4);
        let area = 8.0 + std::f64::consts::PI / 2.0;
        assert!((item.mesh.volume() - area * t).abs() < 0.01);
    }

    #[test]
    fn edit_mesh_volume_is_preserved() {
        let mut e = creabim_core::EditMesh::cuboid(DVec3::new(2.0, 2.0, 2.0));
        e.extrude_face(1, 1.0);
        let m = edit_mesh_to_tri(&e, DVec3::new(10.0, 0.0, 0.0));
        assert!((m.volume() - 12.0).abs() < 1e-4);
    }
}
