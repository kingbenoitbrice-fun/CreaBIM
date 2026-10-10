use glam::{DVec3, Vec3};

/// A triangle mesh ready for the GPU: flat-shaded, one normal per vertex.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl TriMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Add a planar polygon (convex, or with a supplied triangulation) facing `normal`.
    pub fn add_polygon(&mut self, points: &[DVec3], triangles: &[[usize; 3]], normal: DVec3) {
        let base = self.positions.len() as u32;
        let n = normal.normalize_or_zero().as_vec3().to_array();
        for p in points {
            self.positions.push(p.as_vec3().to_array());
            self.normals.push(n);
        }
        for t in triangles {
            let [a, b, c] = *t;
            // keep the winding consistent with the normal
            let (pa, pb, pc) = (points[a], points[b], points[c]);
            if (pb - pa).cross(pc - pa).dot(normal) >= 0.0 {
                self.indices
                    .extend([base + a as u32, base + b as u32, base + c as u32]);
            } else {
                self.indices
                    .extend([base + a as u32, base + c as u32, base + b as u32]);
            }
        }
    }

    pub fn add_quad(&mut self, q: [DVec3; 4], normal: DVec3) {
        self.add_polygon(&q, &[[0, 1, 2], [0, 2, 3]], normal);
    }

    pub fn append(&mut self, other: &TriMesh) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }

    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut it = self.positions.iter().map(|p| Vec3::from(*p));
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }

    /// Enclosed volume, for tests: positive when the mesh is closed and outward-facing.
    pub fn volume(&self) -> f64 {
        self.indices
            .chunks(3)
            .map(|t| {
                let p = |i: u32| Vec3::from(self.positions[i as usize]).as_dvec3();
                p(t[0]).dot(p(t[1]).cross(p(t[2]))) / 6.0
            })
            .sum()
    }
}
