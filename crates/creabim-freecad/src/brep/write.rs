//! Writes a polyhedral mesh as text BREP ("CASCADE Topology V1").
//!
//! Every polygon becomes a planar face bounded by straight edges, with
//! p-curves on the planes so OpenCascade does not have to rebuild them.
//! Each connected part of the mesh becomes a solid (with outward normals)
//! when it is closed and consistently oriented, otherwise a shell; several
//! parts are wrapped in a compound.

use crate::math::{Trsf, V3};
use std::collections::HashMap;
use std::fmt::Write as _;

/// Result of [`write_brep`]: the BREP text and the number of faces written.
pub struct BrepText {
    pub text: String,
    pub faces: usize,
}

struct Face {
    loop_: Vec<u32>,
    origin: V3,
    n: V3,
    x: V3,
    y: V3,
}

/// Writes `faces` (indices into `vertices`, planar polygons) with `root`
/// as the location of the whole shape (FreeCAD's Placement).
pub fn write_brep(vertices: &[V3], faces: &[Vec<u32>], root: Option<&Trsf>) -> BrepText {
    let mut polys: Vec<Face> = Vec::new();
    for f in faces {
        let mut l: Vec<u32> = f
            .iter()
            .copied()
            .filter(|&i| (i as usize) < vertices.len() && vertices[i as usize].is_finite())
            .collect();
        l.dedup();
        while l.len() > 1 && l.first() == l.last() {
            l.pop();
        }
        if l.len() < 3 {
            continue;
        }
        // Newell's method: robust normal for slightly non-planar polygons.
        let mut n = V3::default();
        for i in 0..l.len() {
            let a = vertices[l[i] as usize];
            let b = vertices[l[(i + 1) % l.len()] as usize];
            n = n + a.cross(b);
        }
        let n = n.normalized();
        if n.length() == 0.0 {
            continue;
        }
        let origin = vertices[l[0] as usize];
        let e = vertices[l[1] as usize] - origin;
        let mut x = (e - n * e.dot(n)).normalized();
        if x.length() == 0.0 {
            x = n.any_perpendicular();
        }
        let y = n.cross(x);
        polys.push(Face {
            loop_: l,
            origin,
            n,
            x,
            y,
        });
    }

    // Connected components (faces sharing an edge) each become a shell:
    // OpenCascade rejects a shell that is not connected, e.g. a window's
    // frame and glass written together.
    let comps = components(&polys);
    let mut closed_comp = Vec::with_capacity(comps.len());
    for comp in &comps {
        let closed = is_closed(&polys, comp);
        if closed && signed_volume(vertices, &polys, comp) < 0.0 {
            // Inside-out part: flip it so the solid has outward normals.
            for &fi in comp {
                let f = &mut polys[fi];
                f.loop_.reverse();
                f.n = -f.n;
                f.y = -f.y;
            }
        }
        closed_comp.push(closed);
    }

    // Shape table layout: vertices, edges, wires, faces, then per component
    // a shell and (when closed) its solid, and a compound when needed.
    let mut vmap: HashMap<u32, usize> = HashMap::new();
    let mut vlist: Vec<u32> = Vec::new();
    let mut edges: Vec<(u32, u32)> = Vec::new();
    let mut emap: HashMap<(u32, u32), usize> = HashMap::new();
    for f in &polys {
        for i in 0..f.loop_.len() {
            let a = f.loop_[i];
            let b = f.loop_[(i + 1) % f.loop_.len()];
            for v in [a, b] {
                vmap.entry(v).or_insert_with(|| {
                    vlist.push(v);
                    vlist.len() - 1
                });
            }
            let key = (a.min(b), a.max(b));
            emap.entry(key).or_insert_with(|| {
                edges.push(key);
                edges.len() - 1
            });
        }
    }
    let nv = vlist.len();
    let ne = edges.len();
    let nf = polys.len();
    let mut tops = Vec::with_capacity(comps.len()); // (shell position, closed)
    let mut pos = nv + ne + 2 * nf;
    for &closed in &closed_comp {
        tops.push((pos, closed));
        pos += 1 + usize::from(closed);
    }
    let compound = comps.len() > 1;
    let total = pos + usize::from(compound);
    let r = |pos: usize| total - pos; // references count from the end
    let wire_pos = |i: usize| nv + ne + i;
    let face_pos = |i: usize| nv + ne + nf + i;

    // p-curves: one per (edge, face) use, in the plane's (X, Y) frame.
    let mut pcurves: Vec<[f64; 4]> = Vec::new();
    let mut edge_uses: Vec<Vec<(usize, usize)>> = vec![Vec::new(); ne]; // (pcurve, surface)
    for (fi, f) in polys.iter().enumerate() {
        let mut seen = Vec::new();
        for i in 0..f.loop_.len() {
            let a = f.loop_[i];
            let b = f.loop_[(i + 1) % f.loop_.len()];
            let ei = emap[&(a.min(b), a.max(b))];
            if seen.contains(&ei) {
                continue;
            }
            seen.push(ei);
            let (p, q) = (
                vertices[edges[ei].0 as usize],
                vertices[edges[ei].1 as usize],
            );
            let d = (q - p).normalized();
            let o = p - f.origin;
            pcurves.push([o.dot(f.x), o.dot(f.y), d.dot(f.x), d.dot(f.y)]);
            edge_uses[ei].push((pcurves.len(), fi + 1));
        }
    }

    let mut s = String::with_capacity(256 * (nv + ne + nf) + 1024);
    s.push_str("\nCASCADE Topology V1, (c) Matra-Datavision\n");
    match root {
        Some(t) => {
            s.push_str("Locations 1\n1\n");
            for row in &t.m {
                let _ = writeln!(
                    s,
                    "{} {} {} {} ",
                    num(row[0]),
                    num(row[1]),
                    num(row[2]),
                    num(row[3])
                );
            }
        }
        None => s.push_str("Locations 0\n"),
    }
    let _ = writeln!(s, "Curve2ds {}", pcurves.len());
    for p in &pcurves {
        let _ = writeln!(
            s,
            "1 {} {} {} {} ",
            num(p[0]),
            num(p[1]),
            num(p[2]),
            num(p[3])
        );
    }
    let _ = writeln!(s, "Curves {ne}");
    for &(a, b) in &edges {
        let (p, q) = (vertices[a as usize], vertices[b as usize]);
        let d = (q - p).normalized();
        let _ = writeln!(s, "1 {} {} ", v(p), v(d));
    }
    s.push_str("Polygon3D 0\nPolygonOnTriangulations 0\n");
    let _ = writeln!(s, "Surfaces {nf}");
    for f in &polys {
        let _ = writeln!(s, "1 {} {} {} {} ", v(f.origin), v(f.n), v(f.x), v(f.y));
    }
    s.push_str("Triangulations 0\n\n");
    let _ = writeln!(s, "TShapes {total}");
    for &vi in &vlist {
        let _ = write!(
            s,
            "Ve\n1e-07\n{}\n0 0\n\n0101101\n*\n",
            v(vertices[vi as usize])
        );
    }
    for (ei, &(a, b)) in edges.iter().enumerate() {
        let len = (vertices[b as usize] - vertices[a as usize]).length();
        let _ = write!(s, "Ed\n 1e-07 1 1 0\n1  {} 0 0 {}\n", ei + 1, num(len));
        for &(pc, surf) in &edge_uses[ei] {
            let _ = writeln!(s, "2  {pc} {surf} 0 0 {}", num(len));
        }
        let _ = write!(s, "0\n\n0101000\n+{} 0 -{} 0 *\n", r(vmap[&a]), r(vmap[&b]));
    }
    for f in &polys {
        s.push_str("Wi\n\n0101100\n");
        for i in 0..f.loop_.len() {
            let a = f.loop_[i];
            let b = f.loop_[(i + 1) % f.loop_.len()];
            let ei = emap[&(a.min(b), a.max(b))];
            let sign = if a < b { '+' } else { '-' };
            let _ = write!(s, "{sign}{} 0 ", r(nv + ei));
            if i % 10 == 9 {
                s.push('\n');
            }
        }
        s.push_str("*\n");
    }
    for fi in 0..nf {
        let _ = write!(
            s,
            "Fa\n0  1e-07 {} 0\n\n0101000\n+{} 0 *\n",
            fi + 1,
            r(wire_pos(fi))
        );
    }
    for (comp, &(shell, closed)) in comps.iter().zip(&tops) {
        s.push_str(if closed {
            "Sh\n\n0101100\n"
        } else {
            "Sh\n\n0101000\n"
        });
        for (k, &fi) in comp.iter().enumerate() {
            let _ = write!(s, "+{} 0 ", r(face_pos(fi)));
            if k % 10 == 9 {
                s.push('\n');
            }
        }
        s.push_str("*\n");
        if closed {
            let _ = write!(s, "So\n\n1100000\n+{} 0 *\n", r(shell));
        }
    }
    if compound {
        s.push_str("Co\n\n1100000\n");
        for (k, &(shell, closed)) in tops.iter().enumerate() {
            let top = if closed { shell + 1 } else { shell };
            let _ = write!(s, "+{} 0 ", r(top));
            if k % 10 == 9 {
                s.push('\n');
            }
        }
        s.push_str("*\n");
    }
    let loc = usize::from(root.is_some());
    if nf > 0 {
        let _ = write!(s, "\n+1 {loc} \n");
    } else {
        s.push_str("\n*\n");
    }
    BrepText { text: s, faces: nf }
}

/// Groups faces that share an edge (union-find over edges).
fn components(polys: &[Face]) -> Vec<Vec<usize>> {
    fn find(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..polys.len()).collect();
    let mut owner: HashMap<(u32, u32), usize> = HashMap::new();
    for (fi, f) in polys.iter().enumerate() {
        for i in 0..f.loop_.len() {
            let (a, b) = (f.loop_[i], f.loop_[(i + 1) % f.loop_.len()]);
            match owner.entry((a.min(b), a.max(b))) {
                std::collections::hash_map::Entry::Occupied(o) => {
                    let (x, y) = (find(&mut parent, *o.get()), find(&mut parent, fi));
                    parent[x] = y;
                }
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(fi);
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut order = Vec::new();
    for fi in 0..polys.len() {
        let root = find(&mut parent, fi);
        groups
            .entry(root)
            .or_insert_with(|| {
                order.push(root);
                Vec::new()
            })
            .push(fi);
    }
    order
        .into_iter()
        .filter_map(|r| groups.remove(&r))
        .collect()
}

/// Closed and consistently oriented: every directed edge appears once and
/// its reverse once.
fn is_closed(polys: &[Face], comp: &[usize]) -> bool {
    let mut directed: HashMap<(u32, u32), usize> = HashMap::new();
    for &fi in comp {
        let l = &polys[fi].loop_;
        for i in 0..l.len() {
            *directed.entry((l[i], l[(i + 1) % l.len()])).or_insert(0) += 1;
        }
    }
    !directed.is_empty()
        && directed
            .iter()
            .all(|(&(a, b), &c)| c == 1 && directed.get(&(b, a)) == Some(&1))
}

fn signed_volume(vertices: &[V3], polys: &[Face], comp: &[usize]) -> f64 {
    comp.iter()
        .map(|&fi| {
            let l = &polys[fi].loop_;
            let p0 = vertices[l[0] as usize];
            (1..l.len() - 1)
                .map(|i| p0.dot(vertices[l[i] as usize].cross(vertices[l[i + 1] as usize])))
                .sum::<f64>()
        })
        .sum()
}

/// Formats a number so C++ `operator>>` reads it back exactly.
fn num(x: f64) -> String {
    if x.is_finite() {
        // Shortest round-trip representation; Debug switches to exponent
        // notation for very small/large values instead of long digit runs.
        let s = format!("{x:?}");
        s.strip_suffix(".0").map(str::to_owned).unwrap_or(s)
    } else {
        "0".into()
    }
}

fn v(p: V3) -> String {
    format!("{} {} {}", num(p.x), num(p.y), num(p.z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brep::{parse::parse, tess::tessellate};
    use crate::math::v3;

    fn cube() -> (Vec<V3>, Vec<Vec<u32>>) {
        let v = vec![
            v3(0.0, 0.0, 0.0),
            v3(1.0, 0.0, 0.0),
            v3(1.0, 1.0, 0.0),
            v3(0.0, 1.0, 0.0),
            v3(0.0, 0.0, 1.0),
            v3(1.0, 0.0, 1.0),
            v3(1.0, 1.0, 1.0),
            v3(0.0, 1.0, 1.0),
        ];
        let f = vec![
            vec![0, 3, 2, 1],
            vec![4, 5, 6, 7],
            vec![0, 1, 5, 4],
            vec![1, 2, 6, 5],
            vec![2, 3, 7, 6],
            vec![3, 0, 4, 7],
        ];
        (v, f)
    }

    #[test]
    fn cube_round_trips_through_parser() {
        let (v, f) = cube();
        let out = write_brep(&v, &f, None);
        assert!(out.text.contains("\nSo\n"));
        let m = parse(&out.text).unwrap();
        let mesh = tessellate(&m);
        assert_eq!(mesh.vertices.len(), 8);
        assert_eq!(mesh.triangles.len(), 12);
    }

    #[test]
    fn numbers_are_plain() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(-0.5), "-0.5");
        assert_eq!(num(1e-7), "1e-7");
        assert_eq!(num(f64::NAN), "0");
    }
}
