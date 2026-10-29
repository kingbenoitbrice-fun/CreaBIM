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

/// Do segments `a`–`b` and `c`–`d` cross or overlap? Touching only at a
/// shared end does not count.
pub fn segments_cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let eps = 1e-9;
    let orient = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p);
    let (o1, o2) = (orient(a, b, c), orient(a, b, d));
    let (o3, o4) = (orient(c, d, a), orient(c, d, b));
    if ((o1 > eps && o2 < -eps) || (o1 < -eps && o2 > eps))
        && ((o3 > eps && o4 < -eps) || (o3 < -eps && o4 > eps))
    {
        return true;
    }
    // collinear or touching: a point of one lying on the other, other than
    // an end they share
    let on = |p: DVec2, q: DVec2, r: DVec2| {
        orient(p, q, r).abs() <= eps * (q - p).length().max(1.0)
            && r.x >= p.x.min(q.x) - eps
            && r.x <= p.x.max(q.x) + eps
            && r.y >= p.y.min(q.y) - eps
            && r.y <= p.y.max(q.y) + eps
    };
    (on(a, b, c) && c.distance(a) > 1e-9 && c.distance(b) > 1e-9)
        || (on(a, b, d) && d.distance(a) > 1e-9 && d.distance(b) > 1e-9)
        || (on(c, d, a) && a.distance(c) > 1e-9 && a.distance(d) > 1e-9)
        || (on(c, d, b) && b.distance(c) > 1e-9 && b.distance(d) > 1e-9)
}

/// The edges of a ring of corners: each corner to the next, and the last
/// back to the first when it is `closed`.
fn edges(poly: &[DVec2], closed: bool) -> Vec<(DVec2, DVec2)> {
    let n = poly.len();
    let count = if closed { n } else { n.saturating_sub(1) };
    (0..count).map(|i| (poly[i], poly[(i + 1) % n])).collect()
}

/// Does an outline cross itself? Edges next to each other may share their
/// corner; any other contact counts.
pub fn self_intersects(poly: &[DVec2], closed: bool) -> bool {
    let e = edges(poly, closed);
    let n = e.len();
    for i in 0..n {
        for j in i + 1..n {
            let next = j == i + 1 || (closed && i == 0 && j == n - 1);
            let (a, b) = e[i];
            let (c, d) = e[j];
            if next {
                // neighbours meet at one corner; folding back onto each
                // other overlaps them
                let (p, q, r) = if j == i + 1 { (a, b, d) } else { (c, d, b) };
                let back = (q - p).normalize_or_zero().dot((r - q).normalize_or_zero());
                if (q - p).perp_dot(r - q).abs() < 1e-12 && back < 0.0 {
                    return true;
                }
                continue;
            }
            if segments_cross(a, b, c, d) {
                return true;
            }
        }
    }
    false
}

/// Do the edges of two closed outlines cross or touch?
pub fn outlines_cross(a: &[DVec2], b: &[DVec2]) -> bool {
    let (ea, eb) = (edges(a, true), edges(b, true));
    ea.iter()
        .any(|(p, q)| eb.iter().any(|(r, s)| segments_cross(*p, *q, *r, *s)))
}

/// Is closed outline `inner` inside `outer`, without touching its edges?
pub fn inside(inner: &[DVec2], outer: &[DVec2]) -> bool {
    !inner.is_empty() && inner.iter().all(|p| contains(outer, *p)) && !outlines_cross(inner, outer)
}

/// Do two closed outlines share any area (cross, or one inside the other)?
pub fn overlap(a: &[DVec2], b: &[DVec2]) -> bool {
    outlines_cross(a, b)
        || a.first().is_some_and(|p| contains(b, *p))
        || b.first().is_some_and(|p| contains(a, *p))
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
    fn crossings_and_containment_are_found() {
        let sq = rectangle(DVec2::ZERO, DVec2::new(4.0, 4.0));
        assert!(!self_intersects(&sq, true));
        // a bow tie crosses itself
        let bow = vec![
            DVec2::ZERO,
            DVec2::new(2.0, 2.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(0.0, 2.0),
        ];
        assert!(self_intersects(&bow, true));
        // an open path doubling back on itself
        let back = vec![DVec2::ZERO, DVec2::new(2.0, 0.0), DVec2::new(1.0, 0.0)];
        assert!(self_intersects(&back, false));
        let small = rectangle(DVec2::new(1.0, 1.0), DVec2::new(2.0, 2.0));
        assert!(inside(&small, &sq));
        let across = rectangle(DVec2::new(3.0, 1.0), DVec2::new(5.0, 2.0));
        assert!(!inside(&across, &sq) && outlines_cross(&across, &sq));
        assert!(overlap(&small, &sq) && overlap(&across, &sq));
        let apart = rectangle(DVec2::new(6.0, 6.0), DVec2::new(7.0, 7.0));
        assert!(!overlap(&apart, &sq));
        // corners shared by neighbouring edges are no crossing
        assert!(!segments_cross(
            DVec2::ZERO,
            DVec2::X,
            DVec2::X,
            DVec2::new(1.0, 1.0)
        ));
    }

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
