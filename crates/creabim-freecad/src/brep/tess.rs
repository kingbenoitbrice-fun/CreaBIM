//! Tessellation of a parsed BREP into a triangle mesh in world coordinates.
//!
//! * A face with a stored triangulation uses it as is.
//! * A planar face is triangulated from its sampled wires (holes included).
//! * Any other face is triangulated in its (u, v) parameter domain, using the
//!   edges' p-curves (or an analytic inverse for elementary surfaces), then
//!   interior edges are split until the mesh follows the surface.
//!
//! Boundary points are always computed from the 3D edge curves, so faces
//! sharing an edge share its points and the result welds into a closed mesh.

use super::geom::{Surface, ANGLE_STEP};
use super::parse::{BrepModel, Orient, ShapeKind, SubRef};
use super::uvmesh::{edge_key, refine_curved};
use crate::math::{v2, Trsf, V2, V3};
use crate::triangulate::{signed_area, triangulate_with_metric};
use std::collections::{HashMap, HashSet};
use std::f64::consts::TAU;

/// Hard limits that keep hostile or huge files from exhausting memory.
const MAX_DEPTH: usize = 64;
const MAX_TRIANGLES: usize = 4_000_000;

#[derive(Default)]
pub struct TessMesh {
    pub vertices: Vec<V3>,
    pub triangles: Vec<[u32; 3]>,
    pub warnings: Vec<String>,
}

/// Tessellates the root shape of `m`.
pub fn tessellate(m: &BrepModel) -> TessMesh {
    let mut ctx = Ctx {
        m,
        out: TessMesh::default(),
        skipped: HashMap::new(),
    };
    if let Some(root) = m.root {
        ctx.visit(root, Trsf::IDENTITY, Orient::Forward, 0, usize::MAX);
    }
    let mut out = ctx.out;
    for (why, n) in ctx.skipped {
        out.warnings.push(format!("{n} face(s) skipped: {why}"));
    }
    out.warnings.sort();
    weld(&mut out);
    out
}

struct Ctx<'a> {
    m: &'a BrepModel,
    out: TessMesh,
    skipped: HashMap<&'static str, usize>,
}

/// One sampled, oriented edge of a face boundary.
struct Polyline {
    pts: Vec<V3>,
    uv: Option<Vec<V2>>,
}

impl Ctx<'_> {
    fn skip(&mut self, why: &'static str) {
        *self.skipped.entry(why).or_insert(0) += 1;
    }

    fn visit(&mut self, r: SubRef, parent: Trsf, porient: Orient, depth: usize, max_index: usize) {
        // References always point to shapes written earlier in the file; a
        // reference forward (or too deep) would mean a cycle.
        if depth > MAX_DEPTH || r.shape >= max_index.min(self.m.shapes.len()) {
            return;
        }
        let trsf = parent.then_after(&self.m.location(r.loc));
        let orient = porient.compose(r.orient);
        let shape = &self.m.shapes[r.shape];
        match shape.kind {
            ShapeKind::Face { .. } => self.face(r.shape, trsf, orient),
            ShapeKind::Vertex { .. } | ShapeKind::Edge { .. } | ShapeKind::Wire => {}
            _ => {
                for sub in &shape.subs {
                    self.visit(*sub, trsf, orient, depth + 1, r.shape);
                }
            }
        }
    }

    fn face(&mut self, idx: usize, trsf: Trsf, orient: Orient) {
        if self.out.triangles.len() > MAX_TRIANGLES {
            self.skip("triangle budget exhausted");
            return;
        }
        if matches!(orient, Orient::Internal | Orient::External) {
            return;
        }
        let m = self.m;
        let ShapeKind::Face {
            surface,
            loc,
            triangulation,
        } = m.shapes[idx].kind
        else {
            return;
        };
        let reversed = orient == Orient::Reversed;
        if let Some(t) = triangulation.and_then(|t| m.triangulations.get(t)) {
            let base = self.out.vertices.len() as u32;
            self.out
                .vertices
                .extend(t.nodes.iter().map(|p| trsf.point(*p)));
            for tri in &t.triangles {
                let [a, b, c] = tri.map(|i| base + i as u32);
                self.out
                    .triangles
                    .push(if reversed { [a, c, b] } else { [a, b, c] });
            }
            return;
        }
        let Some(surf) = m.surface(surface) else {
            self.skip("face without surface");
            return;
        };
        let surf_trsf = trsf.then_after(&m.location(loc));
        let planar = surf.is_plane().is_some();

        // Sample every wire into one polyline per edge.
        let mut wires: Vec<Vec<Polyline>> = Vec::new();
        for w in &m.shapes[idx].subs {
            if w.shape >= idx || !matches!(m.shapes[w.shape].kind, ShapeKind::Wire) {
                continue;
            }
            let wt = trsf.then_after(&m.location(w.loc));
            let wo = orient.compose(w.orient);
            let mut lines = Vec::new();
            for e in &m.shapes[w.shape].subs {
                if e.shape >= w.shape {
                    continue;
                }
                let et = wt.then_after(&m.location(e.loc));
                let eo = wo.compose(e.orient);
                // Orientation as seen from a forward face picks the p-curve.
                let local = if reversed {
                    Orient::Reversed.compose(eo)
                } else {
                    eo
                };
                match self.edge(e.shape, et, eo, local, surface, &surf_trsf, planar) {
                    Some(p) => lines.push(p),
                    None => {
                        self.skip("edge could not be sampled");
                        return;
                    }
                }
            }
            if !lines.is_empty() {
                wires.push(lines);
            }
        }
        if wires.is_empty() {
            self.skip("face without boundary");
            return;
        }

        let mut loops: Vec<(Vec<V3>, Vec<V2>)> = Vec::new();
        for w in wires {
            let use_uv = !planar && w.iter().all(|p| p.uv.is_some());
            let (pts, uv) = chain(w, use_uv);
            if pts.len() < 3 {
                continue;
            }
            let uv = if planar {
                let ax = surf.is_plane().copied();
                let Some(ax) = ax else { continue };
                let o = surf_trsf.point(ax.p);
                let (x, y) = (surf_trsf.vector(ax.x), surf_trsf.vector(ax.y));
                pts.iter()
                    .map(|p| v2((*p - o).dot(x), (*p - o).dot(y)))
                    .collect()
            } else if let Some(uv) = uv {
                uv
            } else {
                match inverse_uv(surf, &surf_trsf, &pts) {
                    Some(uv) => uv,
                    None => {
                        self.skip("curved face without p-curves on an unsupported surface");
                        return;
                    }
                }
            };
            loops.push((pts, uv));
        }
        if loops.is_empty() {
            self.skip("degenerate boundary");
            return;
        }
        // The outer wire encloses the largest parameter area.
        let outer = (0..loops.len())
            .max_by(|&a, &b| {
                let aa = ring_area(&loops[a].1).abs();
                let bb = ring_area(&loops[b].1).abs();
                aa.total_cmp(&bb)
            })
            .unwrap_or(0);
        loops.swap(0, outer);
        let uv_loops: Vec<Vec<V2>> = loops.iter().map(|l| l.1.clone()).collect();
        let mut verts: Vec<(V2, V3)> = loops
            .iter()
            .flat_map(|(p, uv)| uv.iter().copied().zip(p.iter().copied()))
            .collect();
        let tris = if planar {
            triangulate_with_metric(&uv_loops, None)
        } else {
            let p3: Vec<V3> = verts.iter().map(|v| v.1).collect();
            let tris = triangulate_with_metric(&uv_loops, Some(&p3));
            let mut boundary = HashSet::new();
            let mut start = 0;
            for (p, _) in &loops {
                let n = p.len();
                for i in 0..n {
                    boundary.insert(edge_key(start + i, start + (i + 1) % n));
                }
                start += n;
            }
            refine_curved(surf, &surf_trsf, &mut verts, tris, &uv_loops, &boundary)
        };
        let base = self.out.vertices.len() as u32;
        self.out.vertices.extend(verts.iter().map(|v| v.1));
        for [a, b, c] in tris {
            let (a, b, c) = (base + a as u32, base + b as u32, base + c as u32);
            self.out
                .triangles
                .push(if reversed { [a, c, b] } else { [a, b, c] });
        }
    }

    /// Samples one edge in its oriented direction. `local` is the edge's
    /// orientation relative to a forward face (selects seam p-curves).
    #[allow(clippy::too_many_arguments)]
    fn edge(
        &self,
        idx: usize,
        trsf: Trsf,
        orient: Orient,
        local: Orient,
        surface: usize,
        surf_trsf: &Trsf,
        planar: bool,
    ) -> Option<Polyline> {
        let m = self.m;
        let shape = &m.shapes[idx];
        let ShapeKind::Edge {
            curve,
            pcurves,
            degenerated,
        } = &shape.kind
        else {
            return None;
        };
        let mut first_v = None;
        let mut last_v = None;
        for v in &shape.subs {
            if v.shape >= idx {
                continue;
            }
            if let ShapeKind::Vertex { pnt } = m.shapes[v.shape].kind {
                let p = trsf.then_after(&m.location(v.loc)).point(pnt);
                match v.orient {
                    Orient::Forward => first_v = Some(p),
                    Orient::Reversed => last_v = Some(p),
                    _ => {}
                }
            }
        }
        // p-curve on this face's surface whose placement matches the face.
        let pcurve = if planar {
            None
        } else {
            let mut matching = pcurves.iter().filter(|p| p.surface == surface);
            let all: Vec<_> = matching.by_ref().collect();
            all.iter()
                .find(|p| {
                    trsf.then_after(&m.location(p.loc))
                        .approx_eq(surf_trsf, 1e-9)
                })
                .or(all.first())
                .copied()
        };
        let pc = pcurve.and_then(|p| {
            let i = if local == Orient::Reversed {
                p.pcurve2.unwrap_or(p.pcurve)
            } else {
                p.pcurve
            };
            m.curve2d(i).map(|c| (c, p.first, p.last))
        });
        let c3 = curve
            .as_ref()
            .and_then(|c| m.curve(c.curve).map(|cv| (cv, c.loc, c.first, c.last)));
        let surf = m.surface(surface);

        let (first, last, n) = match (c3, pc) {
            (Some((cv, _, f, l)), _) if !degenerated => (f, l, cv.segments(f, l, ANGLE_STEP)),
            (_, Some((pcv, f, l))) => {
                let n = if *degenerated {
                    ((l - f).abs() / ANGLE_STEP).ceil().clamp(1.0, 256.0) as usize
                } else {
                    pcv.segments(f, l, ANGLE_STEP)
                };
                (f, l, n)
            }
            _ => {
                // No geometry at all: a straight segment between vertices.
                let (a, b) = (first_v?, last_v.or(first_v)?);
                let mut pts = vec![a, b];
                if orient == Orient::Reversed {
                    pts.reverse();
                }
                return Some(Polyline { pts, uv: None });
            }
        };
        let mut pts = Vec::with_capacity(n + 1);
        let mut uv = pc.map(|_| Vec::with_capacity(n + 1));
        for i in 0..=n {
            let t = first + (last - first) * (i as f64 / n as f64);
            let mut uvp = None;
            if let (Some((pcv, pf, pl)), Some(list)) = (pc, uv.as_mut()) {
                let s = if (last - first).abs() > 1e-300 {
                    pf + (pl - pf) * ((t - first) / (last - first))
                } else {
                    pf
                };
                let q = pcv.eval(s);
                let q = v2(q.x, q.y);
                list.push(q);
                uvp = Some(q);
            }
            let p = if *degenerated {
                first_v.or(last_v)?
            } else if let Some((cv, cl, _, _)) = c3 {
                trsf.then_after(&m.location(cl)).point(cv.eval(t))
            } else {
                surf_trsf.point(surf?.eval(uvp?))
            };
            pts.push(p);
        }
        // Snap the ends onto the shared vertices so faces weld exactly.
        if let (Some(a), Some(p)) = (first_v, pts.first_mut()) {
            *p = a;
        }
        if let (Some(b), Some(p)) = (last_v, pts.last_mut()) {
            *p = b;
        }
        if orient == Orient::Reversed {
            pts.reverse();
            if let Some(u) = uv.as_mut() {
                u.reverse();
            }
        }
        Some(Polyline { pts, uv })
    }
}

fn ring_area(uv: &[V2]) -> f64 {
    let idx: Vec<usize> = (0..uv.len()).collect();
    signed_area(uv, &idx)
}

/// Joins edge polylines head to tail into one closed loop (the closing
/// point is not repeated). Edges are taken in wire order when they connect;
/// otherwise the nearest free end is used, reversing an edge if needed.
fn chain(mut lines: Vec<Polyline>, use_uv: bool) -> (Vec<V3>, Option<Vec<V2>>) {
    let dist = |l: &Polyline, at_end: bool, p3: V3, p2: Option<V2>| -> f64 {
        match (use_uv, &l.uv, p2) {
            (true, Some(uv), Some(q)) => {
                let x = if at_end { uv[uv.len() - 1] } else { uv[0] };
                (x - q).length()
            }
            _ => {
                let x = if at_end {
                    l.pts[l.pts.len() - 1]
                } else {
                    l.pts[0]
                };
                (x - p3).length()
            }
        }
    };
    let mut pts: Vec<V3> = Vec::new();
    let mut uvs: Vec<V2> = Vec::new();
    let first = lines.remove(0);
    pts.extend(first.pts);
    if use_uv {
        uvs.extend(first.uv.unwrap_or_default());
    }
    while !lines.is_empty() {
        let end3 = pts[pts.len() - 1];
        let end2 = uvs.last().copied();
        let tol = 1e-6 * (1.0 + end3.length().max(end2.map_or(0.0, |q| q.length())));
        // Wire order first; otherwise the nearest free end of any edge.
        let mut best = (dist(&lines[0], false, end3, end2), 0usize, false);
        if best.0 > tol {
            for (i, l) in lines.iter().enumerate() {
                for at_end in [false, true] {
                    let d = dist(l, at_end, end3, end2);
                    if d < best.0 {
                        best = (d, i, at_end);
                    }
                }
            }
        }
        let mut l = lines.remove(best.1);
        if best.2 {
            l.pts.reverse();
            if let Some(u) = l.uv.as_mut() {
                u.reverse();
            }
        }
        pts.extend(l.pts.into_iter().skip(1));
        if use_uv {
            uvs.extend(l.uv.unwrap_or_default().into_iter().skip(1));
        }
    }
    if pts.len() > 1 && (pts[0] - pts[pts.len() - 1]).length() < 1e-9 * (1.0 + pts[0].length()) {
        pts.pop();
        if use_uv {
            uvs.pop();
        }
    }
    if use_uv && uvs.len() == pts.len() {
        (pts, Some(uvs))
    } else {
        (pts, None)
    }
}

/// Parameters of world points on an elementary surface, unwrapped so the
/// loop is continuous across the seam.
fn inverse_uv(surf: &Surface, surf_trsf: &Trsf, pts: &[V3]) -> Option<Vec<V2>> {
    let inv = surf_trsf.inverse()?;
    let (pu, pv) = surf.periodic();
    let mut out: Vec<V2> = Vec::with_capacity(pts.len());
    for p in pts {
        let mut q = surf.inverse(inv.point(*p))?;
        if let Some(prev) = out.last() {
            if pu {
                q.x += ((prev.x - q.x) / TAU).round() * TAU;
            }
            if pv {
                q.y += ((prev.y - q.y) / TAU).round() * TAU;
            }
        }
        out.push(q);
    }
    Some(out)
}

/// Merges coincident vertices (faces sharing an edge produce the same
/// points) and drops triangles that collapse.
fn weld(mesh: &mut TessMesh) {
    let mut lo = V3 {
        x: f64::MAX,
        y: f64::MAX,
        z: f64::MAX,
    };
    let mut hi = V3 {
        x: f64::MIN,
        y: f64::MIN,
        z: f64::MIN,
    };
    for p in &mesh.vertices {
        lo = V3 {
            x: lo.x.min(p.x),
            y: lo.y.min(p.y),
            z: lo.z.min(p.z),
        };
        hi = V3 {
            x: hi.x.max(p.x),
            y: hi.y.max(p.y),
            z: hi.z.max(p.z),
        };
    }
    if mesh.vertices.is_empty() {
        return;
    }
    let cell = ((hi - lo).length() * 1e-7).max(1e-9);
    let key = |p: V3| {
        (
            ((p.x - lo.x) / cell).floor() as i64,
            ((p.y - lo.y) / cell).floor() as i64,
            ((p.z - lo.z) / cell).floor() as i64,
        )
    };
    let mut grid: HashMap<(i64, i64, i64), Vec<u32>> = HashMap::new();
    let mut remap = Vec::with_capacity(mesh.vertices.len());
    let mut out: Vec<V3> = Vec::new();
    for p in &mesh.vertices {
        let k = key(*p);
        let mut found = None;
        'search: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(list) = grid.get(&(k.0 + dx, k.1 + dy, k.2 + dz)) {
                        for &i in list {
                            if (out[i as usize] - *p).length() <= cell {
                                found = Some(i);
                                break 'search;
                            }
                        }
                    }
                }
            }
        }
        let i = found.unwrap_or_else(|| {
            out.push(*p);
            let i = (out.len() - 1) as u32;
            grid.entry(k).or_default().push(i);
            i
        });
        remap.push(i);
    }
    mesh.vertices = out;
    mesh.triangles = mesh
        .triangles
        .iter()
        .map(|t| t.map(|i| remap[i as usize]))
        .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
        .collect();
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse;
    use super::*;

    /// A unit right triangle face stored with a triangulation (format V3).
    const TRIANGULATED: &str = "CASCADE Topology V3, (c) Open Cascade
Locations 0
Curve2ds 0
Curves 0
Polygon3D 0
PolygonOnTriangulations 0
Surfaces 1
1 0 0 0 0 0 1 1 0 0 0 1 0
Triangulations 1
4 2 0 0 0
0 0 0 2 0 0 2 2 0 0 2 0
1 2 3 1 3 4

TShapes 1
Fa
0  1e-07 1 0
2  1
0101000
*

-1 0";

    #[test]
    fn stored_triangulation_is_used_and_reversed() {
        let m = parse(TRIANGULATED).unwrap();
        let mesh = tessellate(&m);
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.triangles.len(), 2);
        // The face is used reversed, so the normal must point down.
        let t = mesh.triangles[0];
        let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
        assert!((b - a).cross(c - a).z < 0.0);
    }
}
