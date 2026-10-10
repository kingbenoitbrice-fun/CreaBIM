use glam::DVec2;

/// Twice the signed area; positive for counter-clockwise polygons.
pub fn signed_area2(poly: &[DVec2]) -> f64 {
    (0..poly.len())
        .map(|i| poly[i].perp_dot(poly[(i + 1) % poly.len()]))
        .sum()
}

/// Ear-clipping triangulation of a simple polygon (either winding).
/// Returns triangles as indices into `poly`, counter-clockwise.
pub fn triangulate(poly: &[DVec2]) -> Vec<[usize; 3]> {
    let n = poly.len();
    if n < 3 {
        return vec![];
    }
    let mut idx: Vec<usize> = (0..n).collect();
    if signed_area2(poly) < 0.0 {
        idx.reverse();
    }
    let mut out = Vec::with_capacity(n - 2);
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let (a, b, c) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            let (pa, pb, pc) = (poly[a], poly[b], poly[c]);
            if (pb - pa).perp_dot(pc - pb) <= 1e-12 {
                continue; // reflex or degenerate corner
            }
            let inside = idx
                .iter()
                .any(|&j| j != a && j != b && j != c && point_in_triangle(poly[j], pa, pb, pc));
            if !inside {
                out.push([a, b, c]);
                idx.remove(i);
                clipped = true;
                break;
            }
        }
        if !clipped {
            break; // self-intersecting input; return what we have
        }
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    out
}

fn point_in_triangle(p: DVec2, a: DVec2, b: DVec2, c: DVec2) -> bool {
    let d1 = (b - a).perp_dot(p - a);
    let d2 = (c - b).perp_dot(p - b);
    let d3 = (a - c).perp_dot(p - c);
    d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_shape_triangulates_to_full_area() {
        let l = [
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 2.0),
            DVec2::new(2.0, 2.0),
            DVec2::new(2.0, 4.0),
            DVec2::new(0.0, 4.0),
        ];
        let tris = triangulate(&l);
        assert_eq!(tris.len(), 4);
        let area: f64 = tris
            .iter()
            .map(|t| signed_area2(&[l[t[0]], l[t[1]], l[t[2]]]) / 2.0)
            .sum();
        assert!((area - 12.0).abs() < 1e-9);
    }

    #[test]
    fn clockwise_input_is_handled() {
        let sq = [
            DVec2::new(0.0, 0.0),
            DVec2::new(0.0, 1.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 0.0),
        ];
        let tris = triangulate(&sq);
        assert_eq!(tris.len(), 2);
        for t in tris {
            assert!(signed_area2(&[sq[t[0]], sq[t[1]], sq[t[2]]]) > 0.0);
        }
    }
}
