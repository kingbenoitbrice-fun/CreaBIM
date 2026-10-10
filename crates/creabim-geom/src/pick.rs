//! Ray picking against scene geometry, for clicking elements in 3D.

use crate::SceneItem;
use creabim_core::ElementId;
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub id: ElementId,
    /// Distance along the ray.
    pub t: f32,
    /// Index of the hit triangle within the element's mesh.
    pub triangle: usize,
}

/// Closest element hit by a ray, using Möller–Trumbore intersection.
pub fn pick(scene: &[SceneItem], origin: Vec3, dir: Vec3) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    for item in scene {
        let p = &item.mesh.positions;
        for (tri, idx) in item.mesh.indices.chunks(3).enumerate() {
            let (a, b, c) = (
                Vec3::from(p[idx[0] as usize]),
                Vec3::from(p[idx[1] as usize]),
                Vec3::from(p[idx[2] as usize]),
            );
            if let Some(t) = ray_triangle(origin, dir, a, b, c) {
                if best.is_none_or(|h| t < h.t) {
                    best = Some(Hit {
                        id: item.id,
                        t,
                        triangle: tri,
                    });
                }
            }
        }
    }
    best
}

fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-4).then_some(t)
}

/// Which polygon face of an edit mesh a triangle from
/// [`crate::scene::edit_mesh_to_tri`] belongs to.
pub fn face_of_triangle(mesh: &creabim_core::EditMesh, triangle: usize) -> Option<usize> {
    let mut seen = 0;
    for (f, face) in mesh.faces.iter().enumerate() {
        seen += face.len() - 2;
        if triangle < seen {
            return Some(f);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    #[test]
    fn ray_down_the_middle_hits_the_first_floor_slab() {
        let model = sample_house();
        let scene = crate::build_scene(&model);
        let hit = pick(&scene, Vec3::new(5.0, 3.0, 50.0), -Vec3::Z).unwrap();
        assert_eq!(model.get(hit.id).unwrap().name, "FF Slab");
        assert!((hit.t - 47.0).abs() < 1e-3);
    }

    #[test]
    fn ray_through_a_window_misses_the_wall() {
        let model = sample_house();
        let scene = crate::build_scene(&model);
        // GF wall 3 (y = 6) runs from x = 10 towards x = 0 with a window at
        // offset 1.5, so the opening spans x 7.3..8.5 and z 0.9..2.2. A ray
        // through it passes the wall and lands on the window panel.
        let hit = pick(&scene, Vec3::new(8.0, 20.0, 1.0), -Vec3::Y).unwrap();
        let name = &model.get(hit.id).unwrap().name;
        assert_eq!(name, "GF Window 3.1", "hit {name}");
    }

    #[test]
    fn face_lookup_walks_fan_triangles() {
        let mut m = creabim_core::EditMesh::cuboid(glam::DVec3::ONE);
        m.faces.push(vec![0, 1, 2, 3, 4]); // a pentagon: 3 triangles
        assert_eq!(face_of_triangle(&m, 0), Some(0));
        assert_eq!(face_of_triangle(&m, 11), Some(5));
        assert_eq!(face_of_triangle(&m, 12), Some(6));
        assert_eq!(face_of_triangle(&m, 14), Some(6));
        assert_eq!(face_of_triangle(&m, 15), None);
    }
}
