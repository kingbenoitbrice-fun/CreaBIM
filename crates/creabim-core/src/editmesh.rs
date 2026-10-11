use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// A polygon mesh for Blender-style editing: vertices plus faces as vertex
/// loops, counter-clockwise when seen from outside.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EditMesh {
    pub vertices: Vec<DVec3>,
    pub faces: Vec<Vec<u32>>,
}

impl EditMesh {
    /// An axis-aligned box with its base centred on the origin.
    pub fn cuboid(size: DVec3) -> Self {
        let (x, y, z) = (size.x / 2.0, size.y / 2.0, size.z);
        let vertices = vec![
            DVec3::new(-x, -y, 0.0),
            DVec3::new(x, -y, 0.0),
            DVec3::new(x, y, 0.0),
            DVec3::new(-x, y, 0.0),
            DVec3::new(-x, -y, z),
            DVec3::new(x, -y, z),
            DVec3::new(x, y, z),
            DVec3::new(-x, y, z),
        ];
        let faces = vec![
            vec![0, 3, 2, 1], // bottom
            vec![4, 5, 6, 7], // top
            vec![0, 1, 5, 4], // front (-y)
            vec![1, 2, 6, 5], // right (+x)
            vec![2, 3, 7, 6], // back (+y)
            vec![3, 0, 4, 7], // left (-x)
        ];
        Self { vertices, faces }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.faces.is_empty() {
            return Err("mesh has no faces".into());
        }
        for (i, f) in self.faces.iter().enumerate() {
            if f.len() < 3 {
                return Err(format!("face {i} has fewer than 3 vertices"));
            }
            if let Some(v) = f.iter().find(|&&v| v as usize >= self.vertices.len()) {
                return Err(format!("face {i} uses missing vertex {v}"));
            }
        }
        if self.vertices.iter().any(|v| !v.is_finite()) {
            return Err("mesh has a non-finite vertex".into());
        }
        Ok(())
    }

    /// Newell's method: robust for any planar or near-planar polygon.
    pub fn face_normal(&self, face: usize) -> DVec3 {
        let loop_ = &self.faces[face];
        let mut n = DVec3::ZERO;
        for (i, &a) in loop_.iter().enumerate() {
            let p = self.vertices[a as usize];
            let q = self.vertices[loop_[(i + 1) % loop_.len()] as usize];
            n += DVec3::new(
                (p.y - q.y) * (p.z + q.z),
                (p.z - q.z) * (p.x + q.x),
                (p.x - q.x) * (p.y + q.y),
            );
        }
        n.normalize_or_zero()
    }

    /// Triangles covering face `face`, as vertex indices, counter-clockwise
    /// seen from outside. There are always `len - 2`, so triangle k of a
    /// flattened mesh maps back to its face.
    pub fn face_triangles(&self, face: usize) -> Vec<[u32; 3]> {
        let f = &self.faces[face];
        self.face_corners(face)
            .into_iter()
            .map(|t| [f[t[0]], f[t[1]], f[t[2]]])
            .collect()
    }

    /// Like [`EditMesh::face_triangles`], as positions in the face's loop.
    /// Concave faces (an L-shaped extrusion's cap) are ear-clipped; convex
    /// ones are fanned from the first corner.
    pub fn face_corners(&self, face: usize) -> Vec<[usize; 3]> {
        let f = &self.faces[face];
        let m = f.len();
        let fan = || -> Vec<[usize; 3]> { (1..m - 1).map(|i| [0, i, i + 1]).collect() };
        let n = self.face_normal(face);
        if m <= 3 || n == DVec3::ZERO {
            return fan();
        }
        let u = n.any_orthonormal_vector();
        let v = n.cross(u);
        let pts: Vec<DVec2> = f
            .iter()
            .map(|&i| {
                let p = self.vertices[i as usize];
                DVec2::new(p.dot(u), p.dot(v))
            })
            .collect();
        let convex = (0..m).all(|i| {
            let (a, b, c) = (pts[i], pts[(i + 1) % m], pts[(i + 2) % m]);
            (b - a).perp_dot(c - b) >= -1e-12
        });
        if convex {
            return fan();
        }
        let tris = crate::polygon::triangulate(&pts);
        if tris.len() == m - 2 {
            tris
        } else {
            fan()
        }
    }

    pub fn face_center(&self, face: usize) -> DVec3 {
        let loop_ = &self.faces[face];
        loop_
            .iter()
            .map(|&v| self.vertices[v as usize])
            .sum::<DVec3>()
            / loop_.len() as f64
    }

    pub fn translate_vertices(&mut self, vertices: &[u32], delta: DVec3) {
        let mut moved = vertices.to_vec();
        moved.sort_unstable();
        moved.dedup();
        for v in moved {
            self.vertices[v as usize] += delta;
        }
    }

    /// Extrude one face along its normal by `distance`, adding a side quad
    /// per edge, like Blender's E key on a single face.
    pub fn extrude_face(&mut self, face: usize, distance: f64) {
        let offset = self.face_normal(face) * distance;
        let old = self.faces[face].clone();
        let new: Vec<u32> = old
            .iter()
            .map(|&v| {
                self.vertices.push(self.vertices[v as usize] + offset);
                (self.vertices.len() - 1) as u32
            })
            .collect();
        for i in 0..old.len() {
            let j = (i + 1) % old.len();
            self.faces.push(vec![old[i], old[j], new[j], new[i]]);
        }
        self.faces[face] = new;
    }

    /// Scale a face's vertices about its centre, Blender's S key on a face.
    pub fn scale_face(&mut self, face: usize, factor: f64) {
        let c = self.face_center(face);
        for &v in &self.faces[face].clone() {
            let p = &mut self.vertices[v as usize];
            *p = c + (*p - c) * factor;
        }
    }

    /// One level of Catmull-Clark subdivision. Every face becomes quads, which
    /// is the basis for SubD modelling.
    pub fn subdivide(&self) -> EditMesh {
        use std::collections::HashMap;
        let nv = self.vertices.len();
        let face_points: Vec<DVec3> = (0..self.faces.len()).map(|f| self.face_center(f)).collect();

        // edge (min,max) -> adjacent faces
        let mut edge_faces: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        for (fi, f) in self.faces.iter().enumerate() {
            for i in 0..f.len() {
                let (a, b) = (f[i], f[(i + 1) % f.len()]);
                edge_faces.entry((a.min(b), a.max(b))).or_default().push(fi);
            }
        }

        let mut vertices = Vec::with_capacity(nv + edge_faces.len() + self.faces.len());
        // original vertices first; their smoothed positions are filled in below
        vertices.extend(self.vertices.iter().copied());

        let mut edge_index: HashMap<(u32, u32), u32> = HashMap::new();
        let mut edge_mid: HashMap<(u32, u32), DVec3> = HashMap::new();
        let mut keys: Vec<_> = edge_faces.keys().copied().collect();
        keys.sort_unstable();
        for key in keys {
            let faces = &edge_faces[&key];
            let (a, b) = (self.vertices[key.0 as usize], self.vertices[key.1 as usize]);
            edge_mid.insert(key, (a + b) / 2.0);
            let p = if faces.len() == 2 {
                (a + b + face_points[faces[0]] + face_points[faces[1]]) / 4.0
            } else {
                (a + b) / 2.0 // boundary edge
            };
            edge_index.insert(key, vertices.len() as u32);
            vertices.push(p);
        }

        // move original vertices
        let mut v_faces: Vec<Vec<usize>> = vec![vec![]; nv];
        let mut v_edges: Vec<Vec<(u32, u32)>> = vec![vec![]; nv];
        for (fi, f) in self.faces.iter().enumerate() {
            for &v in f {
                v_faces[v as usize].push(fi);
            }
        }
        for &key in edge_faces.keys() {
            v_edges[key.0 as usize].push(key);
            v_edges[key.1 as usize].push(key);
        }
        for v in 0..nv {
            let p = self.vertices[v];
            let boundary: Vec<_> = v_edges[v]
                .iter()
                .filter(|e| edge_faces[e].len() == 1)
                .collect();
            vertices[v] = if !boundary.is_empty() {
                let mids: DVec3 = boundary.iter().map(|e| edge_mid[e]).sum();
                (p * 2.0 + mids) / (2.0 + boundary.len() as f64)
            } else if !v_faces[v].is_empty() {
                let n = v_faces[v].len() as f64;
                let f: DVec3 = v_faces[v].iter().map(|&fi| face_points[fi]).sum::<DVec3>() / n;
                let r: DVec3 =
                    v_edges[v].iter().map(|e| edge_mid[e]).sum::<DVec3>() / v_edges[v].len() as f64;
                (f + r * 2.0 + p * (n - 3.0)) / n
            } else {
                p
            };
        }

        let mut faces = Vec::new();
        for (fi, f) in self.faces.iter().enumerate() {
            let center = vertices.len() as u32;
            vertices.push(face_points[fi]);
            for i in 0..f.len() {
                let prev = f[(i + f.len() - 1) % f.len()];
                let cur = f[i];
                let next = f[(i + 1) % f.len()];
                let e_prev = edge_index[&(prev.min(cur), prev.max(cur))];
                let e_next = edge_index[&(cur.min(next), cur.max(next))];
                faces.push(vec![cur, e_next, center, e_prev]);
            }
        }
        EditMesh { vertices, faces }
    }

    /// Signed volume; positive when faces point outward.
    pub fn volume(&self) -> f64 {
        let mut v = 0.0;
        for f in &self.faces {
            let p0 = self.vertices[f[0] as usize];
            for i in 1..f.len() - 1 {
                let p1 = self.vertices[f[i] as usize];
                let p2 = self.vertices[f[i + 1] as usize];
                v += p0.dot(p1.cross(p2)) / 6.0;
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuboid_has_outward_faces() {
        let m = EditMesh::cuboid(DVec3::new(2.0, 3.0, 4.0));
        assert!((m.volume() - 24.0).abs() < 1e-9);
        assert_eq!(m.face_normal(1), DVec3::Z);
        assert_eq!(m.face_normal(0), -DVec3::Z);
    }

    #[test]
    fn extrude_top_face_grows_volume() {
        let mut m = EditMesh::cuboid(DVec3::ONE);
        m.extrude_face(1, 2.0);
        assert_eq!(m.faces.len(), 10);
        assert!((m.volume() - 3.0).abs() < 1e-9);
        m.validate().unwrap();
    }

    #[test]
    fn scale_face_tapers() {
        let mut m = EditMesh::cuboid(DVec3::ONE);
        m.scale_face(1, 0.0); // pyramid
        assert!((m.volume() - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn subdivide_cube_gives_24_quads_and_shrinks() {
        let m = EditMesh::cuboid(DVec3::ONE);
        let s = m.subdivide();
        assert_eq!(s.faces.len(), 24);
        assert_eq!(s.vertices.len(), 8 + 12 + 6);
        assert!(s.faces.iter().all(|f| f.len() == 4));
        assert!(s.volume() > 0.0 && s.volume() < 1.0);
        s.validate().unwrap();
    }
}
