//! Cutting a closed triangle mesh with a horizontal plane, for plans of
//! shapes that have no simple footprint: masses, and walls or slabs with a
//! boolean cut. The result is closed outlines, outer ones counter-clockwise
//! with their holes bridged in, ready to fill and hatch.

use crate::TriMesh;
use glam::{DVec2, DVec3};
use std::collections::HashMap;

/// The outlines where the plane `z` cuts `mesh`, each a simple polygon
/// (holes are joined to their outline by a zero-width bridge).
pub fn slice(mesh: &TriMesh, z: f64) -> Vec<Vec<DVec2>> {
    let loops = loops(mesh, z);
    let (outer, holes): (Vec<_>, Vec<_>) = loops
        .into_iter()
        .filter(|l| l.len() >= 3)
        .partition(|l| crate::triangulate::signed_area2(l) > 0.0);
    let mut out = outer;
    for h in holes {
        let Some(i) = out.iter().position(|o| inside(h[0], o)) else {
            continue;
        };
        out[i] = bridge(&out[i], &h);
    }
    out
}

/// Line pieces where the plane cuts each triangle, oriented so the solid is
/// on their left, chained into loops.
fn loops(mesh: &TriMesh, z: f64) -> Vec<Vec<DVec2>> {
    let p = |i: u32| {
        let v = mesh.positions[i as usize];
        // nudge corners off the plane so every crossing is clean
        let mut q = DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64);
        if (q.z - z).abs() < 1e-6 {
            q.z = z + 1e-6;
        }
        q
    };
    let mut segs: Vec<(DVec2, DVec2)> = Vec::new();
    for t in mesh.indices.chunks(3) {
        let v = [p(t[0]), p(t[1]), p(t[2])];
        let n = (v[1] - v[0]).cross(v[2] - v[0]);
        let mut pts = Vec::new();
        for i in 0..3 {
            let (a, b) = (v[i], v[(i + 1) % 3]);
            if (a.z - z) * (b.z - z) < 0.0 {
                let s = (z - a.z) / (b.z - a.z);
                pts.push((a + (b - a) * s).truncate());
            }
        }
        if let [a, b] = pts[..] {
            // the outward normal's plan direction is on the segment's right
            let d = b - a;
            let right = DVec2::new(d.y, -d.x);
            if right.dot(n.truncate()) >= 0.0 {
                segs.push((a, b));
            } else {
                segs.push((b, a));
            }
        }
    }
    let key = |p: DVec2| ((p.x * 1e5).round() as i64, (p.y * 1e5).round() as i64);
    let mut from: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, (a, _)) in segs.iter().enumerate() {
        from.entry(key(*a)).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        let mut lp = vec![segs[start].0];
        let mut cur = start;
        used[cur] = true;
        loop {
            let end = segs[cur].1;
            let next = from
                .get(&key(end))
                .and_then(|c| c.iter().copied().find(|&j| !used[j]));
            match next {
                Some(j) => {
                    lp.push(end);
                    used[j] = true;
                    cur = j;
                }
                None => break,
            }
        }
        out.push(simplify(lp));
    }
    out
}

/// Drop corners on straight runs, left by the triangulation.
fn simplify(mut p: Vec<DVec2>) -> Vec<DVec2> {
    let mut changed = true;
    while changed && p.len() > 3 {
        changed = false;
        let n = p.len();
        for i in 0..n {
            let (a, b, c) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
            if (b - a).perp_dot(c - b).abs() < 1e-9 || a.distance(b) < 1e-7 {
                p.remove(i);
                changed = true;
                break;
            }
        }
    }
    p
}

fn inside(p: DVec2, poly: &[DVec2]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
    }
    c
}

/// Join a hole into its outline through the closest pair of corners.
fn bridge(outer: &[DVec2], hole: &[DVec2]) -> Vec<DVec2> {
    let mut best = (f64::MAX, 0, 0);
    for (i, a) in outer.iter().enumerate() {
        for (j, b) in hole.iter().enumerate() {
            let d = a.distance_squared(*b);
            if d < best.0 {
                best = (d, i, j);
            }
        }
    }
    let (_, i, j) = best;
    let mut out = outer[..=i].to_vec();
    out.extend(hole[j..].iter().copied());
    out.extend(hole[..=j].iter().copied());
    out.extend(outer[i..].iter().copied());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::boolean_mesh;
    use creabim_core::BoolOp;

    fn cuboid(size: DVec3, at: DVec3) -> TriMesh {
        crate::scene::edit_mesh_to_tri(&creabim_core::EditMesh::cuboid(size), at)
    }

    #[test]
    fn a_box_cuts_to_its_footprint() {
        let m = cuboid(DVec3::new(4.0, 2.0, 3.0), DVec3::ZERO);
        let s = slice(&m, 1.0);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].len(), 4);
        assert!((crate::triangulate::signed_area2(&s[0]) / 2.0 - 8.0).abs() < 1e-6);
    }

    #[test]
    fn a_box_with_a_hole_keeps_the_hole() {
        let a = cuboid(DVec3::new(4.0, 4.0, 3.0), DVec3::ZERO);
        let b = cuboid(DVec3::new(1.0, 1.0, 5.0), DVec3::new(0.0, 0.0, -1.0));
        let m = boolean_mesh(&a, &b, BoolOp::Difference);
        let s = slice(&m, 1.0);
        assert_eq!(s.len(), 1);
        let area: f64 = crate::triangulate::triangulate(&s[0])
            .iter()
            .map(|t| {
                let (p, q, r) = (s[0][t[0]], s[0][t[1]], s[0][t[2]]);
                (q - p).perp_dot(r - p).abs() / 2.0
            })
            .sum();
        assert!((area - 15.0).abs() < 1e-6, "{area}");
    }
}
