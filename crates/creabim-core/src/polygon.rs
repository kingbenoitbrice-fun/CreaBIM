//! Plane polygon helpers shared by meshes, slabs and drawings.

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
            // corners repeated by a hole's bridge sit on the ear, not in it
            let same = |q: DVec2| q == pa || q == pb || q == pc;
            let inside = idx.iter().any(|&j| {
                j != a
                    && j != b
                    && j != c
                    && !same(poly[j])
                    && point_in_triangle(poly[j], pa, pb, pc)
            });
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

/// Is `p` inside the polygon (even-odd)?
pub fn contains(poly: &[DVec2], p: DVec2) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

/// One simple polygon for an outline with holes: the outline made
/// counter-clockwise and each hole, clockwise, joined to it by a zero-width
/// bridge through the closest pair of corners. Triangulating it fills the
/// outline and leaves the holes empty.
pub fn bridged(outer: &[DVec2], holes: &[Vec<DVec2>]) -> Vec<DVec2> {
    let mut out = outer.to_vec();
    if signed_area2(&out) < 0.0 {
        out.reverse();
    }
    for h in holes.iter().filter(|h| h.len() >= 3) {
        let mut h = h.clone();
        if signed_area2(&h) > 0.0 {
            h.reverse();
        }
        let mut best = (f64::MAX, 0, 0);
        for (i, a) in out.iter().enumerate() {
            for (j, b) in h.iter().enumerate() {
                let d = a.distance_squared(*b);
                if d < best.0 {
                    best = (d, i, j);
                }
            }
        }
        let (_, i, j) = best;
        let mut next = out[..=i].to_vec();
        next.extend(h[j..].iter().copied());
        next.extend(h[..=j].iter().copied());
        next.extend(out[i..].iter().copied());
        out = next;
    }
    out
}

/// A rectangle's corners, counter-clockwise, from two opposite corners.
pub fn rectangle(a: DVec2, b: DVec2) -> Vec<DVec2> {
    let (lo, hi) = (a.min(b), a.max(b));
    vec![lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)]
}

#[cfg(test)]
mod hole_tests {
    use super::*;

    #[test]
    fn a_bridged_outline_triangulates_round_its_hole() {
        let outer = rectangle(DVec2::ZERO, DVec2::new(4.0, 4.0));
        let hole = rectangle(DVec2::new(1.0, 1.0), DVec2::new(2.0, 3.0));
        let p = bridged(&outer, &[hole]);
        assert_eq!(p.len(), 4 + 4 + 2);
        let area: f64 = triangulate(&p)
            .iter()
            .map(|t| signed_area2(&[p[t[0]], p[t[1]], p[t[2]]]) / 2.0)
            .sum();
        assert!((area - 14.0).abs() < 1e-9, "{area}");
        assert!(contains(&outer, DVec2::new(1.5, 2.0)));
    }
}
