use crate::{triangulate::triangulate, wall::wall_mesh, TriMesh};
use creabim_core::{ElementId, ElementKind, Model, Opening};
use glam::{DVec2, DVec3};

/// One element's renderable geometry.
#[derive(Clone, Debug)]
pub struct SceneItem {
    pub id: ElementId,
    pub mesh: TriMesh,
    pub color: [f32; 3],
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
                    slab_mesh(&s.boundary, level.elevation, s.thickness),
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
                    edit_mesh_to_tri(&f.mesh, f.origin + DVec3::Z * z),
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
            | ElementKind::Tag(_) => continue,
        };
        // a material's colour wins over the element type's default
        let color = creabim_core::material_of(model, e).map_or(color, |(_, m)| m.color);
        out.push(SceneItem {
            id: e.id,
            mesh,
            color,
        });
    }
    out
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
        let tris: Vec<[usize; 3]> = (1..pts.len() - 1).map(|i| [0, i, i + 1]).collect();
        m.add_polygon(&pts, &tris, mesh.face_normal(f));
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
    fn edit_mesh_volume_is_preserved() {
        let mut e = creabim_core::EditMesh::cuboid(DVec3::new(2.0, 2.0, 2.0));
        e.extrude_face(1, 1.0);
        let m = edit_mesh_to_tri(&e, DVec3::new(10.0, 0.0, 0.0));
        assert!((m.volume() - 12.0).abs() < 1e-4);
    }
}
