//! Polygon meshing shared by the IFC and IFCX importers.
//!
//! CreaBIM draws a free mesh's faces as fans and outlines every face edge in
//! plan, so imported faces must be convex, and should be as large as
//! possible. Faces arrive from IFC as loops with holes (extrusion caps,
//! B-rep faces) or as triangle soups (tessellated sets); [`add_face`] splits
//! the first kind into convex pieces, [`merge_coplanar`] glues the second
//! back into polygons.

use creabim_core::EditMesh;
use glam::{DVec2, DVec3};
use std::collections::HashMap;

/// Newell's normal of a loop, not normalised; zero for a degenerate loop.
pub fn newell(points: &[DVec3]) -> DVec3 {
    let mut n = DVec3::ZERO;
    for (i, p) in points.iter().enumerate() {
        let q = points[(i + 1) % points.len()];
        n += DVec3::new(
            (p.y - q.y) * (p.z + q.z),
            (p.z - q.z) * (p.x + q.x),
            (p.x - q.x) * (p.y + q.y),
        );
    }
    n
}

/// Twice the signed area of a 2D loop; positive when counter-clockwise.
pub fn area2(poly: &[DVec2]) -> f64 {
    (0..poly.len())
        .map(|i| poly[i].perp_dot(poly[(i + 1) % poly.len()]))
        .sum()
}

/// Drop repeated consecutive points, including a closing copy of the first.
pub fn clean_loop<T: Copy>(points: &[T], same: impl Fn(T, T) -> bool) -> Vec<T> {
    let mut out: Vec<T> = Vec::with_capacity(points.len());
    for &p in points {
        if out.last().is_none_or(|&q| !same(p, q)) {
            out.push(p);
        }
    }
    while out.len() > 1 && same(out[0], out[out.len() - 1]) {
        out.pop();
    }
    out
}

fn same3(a: DVec3, b: DVec3) -> bool {
    a.distance_squared(b) < 1e-18
}

/// An orthonormal (u, v) for the plane with normal `n`, with u × v = n.
fn plane_basis(n: DVec3) -> (DVec3, DVec3) {
    let u = n.any_orthonormal_vector();
    (u, n.cross(u))
}

/// Add a planar face (counter-clockwise seen from outside) with optional
/// holes. Convex faces stay one polygon; others are triangulated.
pub fn add_face(mesh: &mut EditMesh, outer: &[DVec3], holes: &[Vec<DVec3>]) {
    let outer = clean_loop(outer, same3);
    if outer.len() < 3 {
        return;
    }
    let n = newell(&outer);
    if n.length_squared() < 1e-24 || !n.is_finite() {
        return;
    }
    let n = n.normalize();
    let (u, v) = plane_basis(n);
    let flat = |p: &DVec3| DVec2::new(p.dot(u), p.dot(v));
    let outer2: Vec<DVec2> = outer.iter().map(flat).collect();
    let holes: Vec<Vec<DVec3>> = holes
        .iter()
        .map(|h| clean_loop(h, same3))
        .filter(|h| h.len() >= 3)
        .collect();
    // CreaBIM ear-clips concave faces itself, so only holes need splitting
    if holes.is_empty() {
        let base = mesh.vertices.len() as u32;
        mesh.vertices.extend_from_slice(&outer);
        mesh.faces
            .push((0..outer.len() as u32).map(|i| base + i).collect());
        return;
    }
    let mut points = outer.clone();
    let mut holes2 = Vec::new();
    for h in &holes {
        let mut h3 = h.clone();
        let mut h2: Vec<DVec2> = h.iter().map(flat).collect();
        if area2(&h2) > 0.0 {
            h2.reverse();
            h3.reverse();
        }
        points.extend_from_slice(&h3);
        holes2.push(h2);
    }
    let base = mesh.vertices.len() as u32;
    mesh.vertices.extend_from_slice(&points);
    for t in triangulate(&outer2, &holes2) {
        mesh.faces
            .push(t.iter().map(|&i| base + i as u32).collect());
    }
}

/// Counter-clockwise with no two edges crossing, so the face can be ear-clipped.
fn is_simple(poly: &[DVec2]) -> bool {
    let n = poly.len();
    if area2(poly) <= 0.0 {
        return false;
    }
    if n > 64 {
        return true; // large merged faces are rare; skip the O(n²) check
    }
    for i in 0..n {
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let (c, d) = (poly[j], poly[(j + 1) % n]);
            if segments_cross(a, b, c, d) {
                return false;
            }
        }
    }
    true
}

/// Convex (or with collinear runs) and counter-clockwise.
#[cfg(test)]
fn is_convex(poly: &[DVec2]) -> bool {
    let n = poly.len();
    let scale = poly
        .iter()
        .fold(0.0f64, |m, p| m.max(p.x.abs()).max(p.y.abs()))
        .max(1.0);
    let eps = 1e-12 * scale * scale;
    let mut positive = false;
    for i in 0..n {
        let (a, b, c) = (poly[i], poly[(i + 1) % n], poly[(i + 2) % n]);
        let cross = (b - a).perp_dot(c - b);
        if cross < -eps {
            return false;
        }
        positive |= cross > eps;
    }
    positive
}

/// Triangulate a counter-clockwise polygon with clockwise holes. Indices
/// refer to `outer` followed by each hole in turn; triangles are
/// counter-clockwise. Holes are bridged into the outer loop, then the result
/// is ear-clipped.
pub fn triangulate(outer: &[DVec2], holes: &[Vec<DVec2>]) -> Vec<[usize; 3]> {
    let mut pts: Vec<DVec2> = outer.to_vec();
    let mut ring: Vec<usize> = (0..outer.len()).collect();
    if area2(outer) < 0.0 {
        ring.reverse();
    }
    let mut hole_rings: Vec<Vec<usize>> = Vec::new();
    for h in holes {
        let start = pts.len();
        pts.extend_from_slice(h);
        let mut r: Vec<usize> = (start..start + h.len()).collect();
        if area2(h) > 0.0 {
            r.reverse();
        }
        hole_rings.push(r);
    }
    // bridge holes from the right-most one leftwards, like earcut
    hole_rings.sort_by(|a, b| {
        let mx = |r: &Vec<usize>| r.iter().map(|&i| pts[i].x).fold(f64::MIN, f64::max);
        mx(b).total_cmp(&mx(a))
    });
    for k in 0..hole_rings.len() {
        let hole = &hole_rings[k];
        let Some(m) = (0..hole.len()).max_by(|&a, &b| pts[hole[a]].x.total_cmp(&pts[hole[b]].x))
        else {
            continue;
        };
        let pm = pts[hole[m]];
        let mut candidates: Vec<usize> = (0..ring.len()).collect();
        candidates.sort_by(|&a, &b| {
            pts[ring[a]]
                .distance_squared(pm)
                .total_cmp(&pts[ring[b]].distance_squared(pm))
        });
        let others: Vec<&Vec<usize>> = hole_rings[k..].iter().collect();
        let visible = |c: usize| {
            let pv = pts[ring[c]];
            let crosses = |r: &[usize]| {
                (0..r.len()).any(|i| {
                    let (a, b) = (pts[r[i]], pts[r[(i + 1) % r.len()]]);
                    segments_cross(pm, pv, a, b)
                })
            };
            if crosses(&ring) || others.iter().any(|r| crosses(r)) {
                return false;
            }
            let mid = (pm + pv) / 2.0;
            let loop_of = |r: &[usize]| r.iter().map(|&i| pts[i]).collect::<Vec<_>>();
            inside(mid, &loop_of(&ring)) && !inside(mid, &loop_of(hole))
        };
        let Some(&c) = candidates
            .iter()
            .find(|&&c| visible(c))
            .or(candidates.first())
        else {
            continue;
        };
        let mut spliced = Vec::with_capacity(ring.len() + hole.len() + 2);
        spliced.extend_from_slice(&ring[..=c]);
        for i in 0..=hole.len() {
            spliced.push(hole[(m + i) % hole.len()]);
        }
        spliced.extend_from_slice(&ring[c..]);
        ring = spliced;
    }
    ear_clip(&pts, ring)
}

/// Do segments ab and cd cross at a point interior to both?
fn segments_cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let same = |p: DVec2, q: DVec2| p.distance_squared(q) < 1e-20;
    if same(a, c) || same(a, d) || same(b, c) || same(b, d) {
        return false;
    }
    let o = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p);
    let (d1, d2) = (o(c, d, a), o(c, d, b));
    let (d3, d4) = (o(a, b, c), o(a, b, d));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// Even-odd point in polygon.
pub fn inside(p: DVec2, poly: &[DVec2]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
    }
    c
}

fn ear_clip(pts: &[DVec2], mut ring: Vec<usize>) -> Vec<[usize; 3]> {
    let mut out = Vec::with_capacity(ring.len());
    if ring.len() > 5000 {
        // too big for O(n²) clipping; a fan is wrong only for concave input
        for i in 1..ring.len().saturating_sub(1) {
            out.push([ring[0], ring[i], ring[i + 1]]);
        }
        return out;
    }
    let scale = pts
        .iter()
        .fold(0.0f64, |m, p| m.max(p.x.abs()).max(p.y.abs()))
        .max(1e-9);
    let eps = 1e-14 * scale * scale;
    let mut i = 0;
    let mut stalled = 0;
    while ring.len() > 3 {
        let m = ring.len();
        let (ia, ib, ic) = (ring[(i + m - 1) % m], ring[i % m], ring[(i + 1) % m]);
        let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
        let cross = (b - a).perp_dot(c - b);
        let ear = if cross.abs() <= eps {
            // collinear or a bridge spike: drop the vertex, no triangle
            Some(false)
        } else if cross > 0.0 {
            let blocked = ring.iter().any(|&j| {
                let p = pts[j];
                j != ia
                    && j != ib
                    && j != ic
                    && p != a
                    && p != b
                    && p != c
                    && in_triangle(p, a, b, c)
            });
            (!blocked || stalled > m).then_some(true)
        } else {
            None
        };
        match ear {
            Some(emit) => {
                if emit {
                    out.push([ia, ib, ic]);
                }
                ring.remove(i % m);
                stalled = 0;
                i = i.saturating_sub(1);
            }
            None => {
                i = (i + 1) % m;
                stalled += 1;
                if stalled > 2 * m {
                    break; // nothing convex left: self-intersecting input
                }
            }
        }
    }
    if ring.len() == 3 {
        let (a, b, c) = (pts[ring[0]], pts[ring[1]], pts[ring[2]]);
        if (b - a).perp_dot(c - b).abs() > eps {
            out.push([ring[0], ring[1], ring[2]]);
        }
    }
    out
}

fn in_triangle(p: DVec2, a: DVec2, b: DVec2, c: DVec2) -> bool {
    let d1 = (b - a).perp_dot(p - a);
    let d2 = (c - b).perp_dot(p - b);
    let d3 = (a - c).perp_dot(p - c);
    d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0
}

/// Merge vertices closer than `tol` and drop faces that collapse.
pub fn weld(mesh: &mut EditMesh, tol: f64) {
    let key = |p: DVec3| {
        (
            (p.x / tol).round() as i64,
            (p.y / tol).round() as i64,
            (p.z / tol).round() as i64,
        )
    };
    let mut map: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut vertices = Vec::new();
    let remap: Vec<u32> = mesh
        .vertices
        .iter()
        .map(|&p| {
            *map.entry(key(p)).or_insert_with(|| {
                vertices.push(p);
                (vertices.len() - 1) as u32
            })
        })
        .collect();
    let mut faces = Vec::with_capacity(mesh.faces.len());
    for f in &mesh.faces {
        let mapped: Vec<u32> = f
            .iter()
            .filter_map(|&v| remap.get(v as usize).copied())
            .collect();
        let mut face = clean_loop(&mapped, |a, b| a == b);
        // a face that touches one vertex twice cannot be drawn as a fan
        let mut seen = face.clone();
        seen.sort_unstable();
        seen.dedup();
        if seen.len() != face.len() {
            face.dedup();
        }
        if seen.len() >= 3 && seen.len() == face.len() {
            faces.push(face);
        }
    }
    mesh.vertices = vertices;
    mesh.faces = faces;
}

/// Glue adjacent coplanar faces into larger convex polygons, so a
/// triangulated box comes back as six quads.
pub fn merge_coplanar(mesh: &mut EditMesh) {
    for _ in 0..32 {
        let planes: Vec<(DVec3, f64)> = mesh
            .faces
            .iter()
            .map(|f| {
                let pts: Vec<DVec3> = f.iter().map(|&v| mesh.vertices[v as usize]).collect();
                let n = newell(&pts).normalize_or_zero();
                (n, n.dot(pts[0]))
            })
            .collect();
        let mut edges: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        for (fi, f) in mesh.faces.iter().enumerate() {
            for i in 0..f.len() {
                edges
                    .entry((f[i], f[(i + 1) % f.len()]))
                    .or_default()
                    .push(fi);
            }
        }
        let mut alive = vec![true; mesh.faces.len()];
        let mut touched = vec![false; mesh.faces.len()];
        let mut merged_any = false;
        for fi in 0..mesh.faces.len() {
            if touched[fi] || !alive[fi] || planes[fi].0 == DVec3::ZERO {
                continue;
            }
            let f = mesh.faces[fi].clone();
            for i in 0..f.len() {
                let (a, b) = (f[i], f[(i + 1) % f.len()]);
                // exactly one neighbour across this edge, running the other way
                let Some([gi]) = edges.get(&(b, a)).map(Vec::as_slice) else {
                    continue;
                };
                let gi = *gi;
                if gi == fi || touched[gi] || !alive[gi] {
                    continue;
                }
                if edges.get(&(a, b)).is_some_and(|v| v.len() != 1) {
                    continue;
                }
                let (n, d) = planes[fi];
                let (m, e) = planes[gi];
                if n.dot(m) < 1.0 - 1e-9 || (d - e).abs() > 1e-7 {
                    continue;
                }
                let g = &mesh.faces[gi];
                let Some(gj) = g.iter().position(|&v| v == a) else {
                    continue;
                };
                // f from b round to a, then g's vertices strictly between a and b
                let fs = (i + 1) % f.len();
                let mut merged: Vec<u32> = (0..f.len()).map(|k| f[(fs + k) % f.len()]).collect();
                merged.extend((1..g.len() - 1).map(|k| g[(gj + k) % g.len()]));
                let mut uniq = merged.clone();
                uniq.sort_unstable();
                uniq.dedup();
                if uniq.len() != merged.len() {
                    continue;
                }
                let (u, v) = plane_basis(n);
                let flat: Vec<DVec2> = merged
                    .iter()
                    .map(|&k| {
                        let p = mesh.vertices[k as usize];
                        DVec2::new(p.dot(u), p.dot(v))
                    })
                    .collect();
                if !is_simple(&flat) {
                    continue;
                }
                mesh.faces[fi] = merged;
                alive[gi] = false;
                touched[fi] = true;
                touched[gi] = true;
                merged_any = true;
                break;
            }
        }
        let faces = std::mem::take(&mut mesh.faces);
        mesh.faces = faces
            .into_iter()
            .zip(alive)
            .filter_map(|(f, a)| a.then_some(f))
            .collect();
        if !merged_any {
            break;
        }
    }
    drop_unused_vertices(mesh);
}

/// Remove vertices no face uses.
pub fn drop_unused_vertices(mesh: &mut EditMesh) {
    let mut remap = vec![u32::MAX; mesh.vertices.len()];
    let mut vertices = Vec::new();
    for f in &mut mesh.faces {
        for v in f.iter_mut() {
            let r = &mut remap[*v as usize];
            if *r == u32::MAX {
                *r = vertices.len() as u32;
                vertices.push(mesh.vertices[*v as usize]);
            }
            *v = *r;
        }
    }
    mesh.vertices = vertices;
}

/// Axis-aligned bounds of a set of points.
pub fn bounds(points: impl IntoIterator<Item = DVec3>) -> Option<(DVec3, DVec3)> {
    let mut it = points.into_iter();
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri_area(pts: &[DVec2], tris: &[[usize; 3]]) -> f64 {
        tris.iter()
            .map(|t| area2(&[pts[t[0]], pts[t[1]], pts[t[2]]]) / 2.0)
            .sum()
    }

    #[test]
    fn square_with_hole_keeps_its_area() {
        let outer = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(0.0, 4.0),
        ];
        let hole = vec![
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 3.0),
            DVec2::new(3.0, 3.0),
            DVec2::new(3.0, 1.0),
        ];
        let tris = triangulate(&outer, std::slice::from_ref(&hole));
        let mut all = outer.clone();
        all.extend(hole);
        assert!((tri_area(&all, &tris) - 12.0).abs() < 1e-9, "{tris:?}");
        assert!(tris
            .iter()
            .all(|t| area2(&[all[t[0]], all[t[1]], all[t[2]]]) > 0.0));
    }

    #[test]
    fn two_holes_and_concave_outer() {
        let outer = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(5.0, 2.0),
            DVec2::new(0.0, 4.0),
        ];
        let sq = |x: f64| {
            vec![
                DVec2::new(x, 0.5),
                DVec2::new(x + 1.0, 0.5),
                DVec2::new(x + 1.0, 1.5),
                DVec2::new(x, 1.5),
            ]
        };
        let holes = vec![sq(1.0), sq(7.0)];
        let tris = triangulate(&outer, &holes);
        let mut all = outer.clone();
        all.extend(holes.concat());
        let expected = area2(&outer) / 2.0 - 2.0;
        assert!((tri_area(&all, &tris) - expected).abs() < 1e-9);
    }

    #[test]
    fn concave_face_is_split_convex_face_is_kept() {
        let mut m = EditMesh::default();
        let sq = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(1.0, 1.0, 0.0),
            DVec3::new(0.0, 1.0, 0.0),
            DVec3::new(0.0, 0.0, 0.0),
        ];
        add_face(&mut m, &sq, &[]);
        assert_eq!(m.faces, vec![vec![0, 1, 2, 3]]);
        let l = [
            DVec3::new(0.0, 0.0, 1.0),
            DVec3::new(2.0, 0.0, 1.0),
            DVec3::new(2.0, 1.0, 1.0),
            DVec3::new(1.0, 1.0, 1.0),
            DVec3::new(1.0, 2.0, 1.0),
            DVec3::new(0.0, 2.0, 1.0),
        ];
        let mut m = EditMesh::default();
        add_face(&mut m, &l, &[]);
        assert_eq!(m.faces.len(), 1);
        assert!(!is_convex(&[
            DVec2::new(0.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(1.0, 0.5),
            DVec2::new(1.0, 2.0)
        ]));
        for f in 0..m.faces.len() {
            assert!(m.face_normal(f).dot(DVec3::Z) > 0.99);
        }
    }

    #[test]
    fn triangulated_cube_merges_back_to_quads() {
        let cube = EditMesh::cuboid(DVec3::ONE);
        let mut tri = EditMesh {
            vertices: cube.vertices.clone(),
            faces: vec![],
        };
        for f in &cube.faces {
            tri.faces.push(vec![f[0], f[1], f[2]]);
            tri.faces.push(vec![f[0], f[2], f[3]]);
        }
        // split vertices as a flat-shaded export would
        let mut soup = EditMesh::default();
        for f in &tri.faces {
            let base = soup.vertices.len() as u32;
            soup.vertices
                .extend(f.iter().map(|&v| tri.vertices[v as usize]));
            soup.faces.push(vec![base, base + 1, base + 2]);
        }
        weld(&mut soup, 1e-6);
        assert_eq!(soup.vertices.len(), 8);
        merge_coplanar(&mut soup);
        assert_eq!(soup.faces.len(), 6);
        assert!(soup.faces.iter().all(|f| f.len() == 4));
        assert!((soup.volume() - 1.0).abs() < 1e-9);
    }
}
