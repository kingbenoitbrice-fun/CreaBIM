//! Turns an ear-clipped parameter-space triangulation of a curved face into
//! a mesh that follows the surface.
//!
//! Ear clipping only uses boundary points, so on a curved face it makes long
//! triangles that cut through the surface. We therefore insert interior grid
//! points (spacing from the surface's curvature), restore a Delaunay-like
//! shape with edge flips in a metric that approximates 3D lengths, and
//! finally split edges that still deviate from the surface. Boundary edges
//! are never changed: they are shared with neighbouring faces.

use super::geom::{Surface, ANGLE_STEP};
use crate::math::{cross2, v2, Trsf, V2, V3};
use std::collections::{HashMap, HashSet};

const MAX_FACE_VERTICES: usize = 50_000;
const MAX_GRID_POINTS: usize = 8_000;
/// Interior grid spacing on doubly curved surfaces: the edge sampling
/// step; deviation splits add points where the surface needs more.
const GRID_ANGLE: f64 = ANGLE_STEP;

pub fn edge_key(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

/// Grid spacing in (u, v), `None` where the surface is straight.
fn grid_steps(surf: &Surface, span: (f64, f64)) -> (Option<f64>, Option<f64>) {
    let by_count = |range: f64, n: usize| Some(range / n.clamp(1, 512) as f64);
    match surf {
        Surface::Plane(_) => (None, None),
        Surface::Cylinder(..) | Surface::Cone(..) => (Some(ANGLE_STEP), None),
        Surface::Sphere(..) | Surface::Torus(..) => (Some(GRID_ANGLE), Some(GRID_ANGLE)),
        Surface::Revolution { basis, .. } => {
            let n = basis.segments(0.0, span.1, ANGLE_STEP).max(4);
            (Some(GRID_ANGLE), by_count(span.1, n))
        }
        Surface::Extrusion { basis, .. } => {
            let n = basis.segments(0.0, span.0, ANGLE_STEP).max(4);
            (by_count(span.0, n), None)
        }
        Surface::Spline {
            du,
            dv,
            uknots,
            vknots,
            ..
        } => {
            let spans = |k: &[f64]| k.windows(2).filter(|w| w[1] > w[0]).count().max(1);
            let nu = spans(uknots) * (du * 2).clamp(2, 8);
            let nv = spans(vknots) * (dv * 2).clamp(2, 8);
            (
                if *du > 1 || spans(uknots) > 1 {
                    by_count(span.0, nu)
                } else {
                    None
                },
                if *dv > 1 || spans(vknots) > 1 {
                    by_count(span.1, nv)
                } else {
                    None
                },
            )
        }
        Surface::Trimmed(s) => grid_steps(s, span),
        Surface::Offset(s, _) => grid_steps(s, span),
    }
}

/// Refines `tris` (indices into `verts`, counter-clockwise in UV).
pub fn refine_curved(
    surf: &Surface,
    surf_trsf: &Trsf,
    verts: &mut Vec<(V2, V3)>,
    mut tris: Vec<[usize; 3]>,
    loops: &[Vec<V2>],
    boundary: &HashSet<(usize, usize)>,
) -> Vec<[usize; 3]> {
    if verts.is_empty() || tris.is_empty() {
        return tris;
    }
    let (mut lo, mut hi) = (v2(f64::MAX, f64::MAX), v2(f64::MIN, f64::MIN));
    for (uv, _) in verts.iter() {
        lo = v2(lo.x.min(uv.x), lo.y.min(uv.y));
        hi = v2(hi.x.max(uv.x), hi.y.max(uv.y));
    }
    let eval = |uv: V2| surf_trsf.point(surf.eval(uv));
    // Scale factors making UV distances roughly 3D distances.
    let c = (lo + hi) * 0.5;
    let h = 1e-4 * (1.0 + (hi - lo).length());
    let su = ((eval(c + v2(h, 0.0)) - eval(c - v2(h, 0.0))).length() / (2.0 * h)).max(1e-9);
    let sv = ((eval(c + v2(0.0, h)) - eval(c - v2(0.0, h))).length() / (2.0 * h)).max(1e-9);
    let scaled = |p: V2| v2(p.x * su, p.y * sv);

    let (gu, gv) = grid_steps(surf, (hi.x - lo.x, hi.y - lo.y));
    let us = grid_values(lo.x, hi.x, gu);
    let vs = grid_values(lo.y, hi.y, gv);
    if us.len() * vs.len() <= MAX_GRID_POINTS && gu.is_some() && gv.is_some() {
        let segs: Vec<(V2, V2)> = loops
            .iter()
            .flat_map(|l| (0..l.len()).map(move |i| (l[i], l[(i + 1) % l.len()])))
            .collect();
        let step = gu
            .map(|s| s * su)
            .unwrap_or(f64::MAX)
            .min(gv.map(|s| s * sv).unwrap_or(f64::MAX));
        let clearance = 0.35 * step;
        for &v in &vs {
            for &u in &us {
                let p = v2(u, v);
                if !inside(loops, p) {
                    continue;
                }
                let near = segs
                    .iter()
                    .any(|&(a, b)| seg_dist(scaled(p), scaled(a), scaled(b)) < clearance);
                if !near {
                    insert_point(verts, &mut tris, p, eval(p));
                }
            }
        }
        flip_edges(verts, &mut tris, boundary, &scaled);
    }
    split_deviating(&eval, verts, tris, boundary)
}

/// Grid coordinates strictly inside (lo, hi).
fn grid_values(lo: f64, hi: f64, step: Option<f64>) -> Vec<f64> {
    match step {
        Some(s) if s > 0.0 && (hi - lo) / s < 4096.0 => {
            let n = ((hi - lo) / s).round().max(1.0) as usize;
            (1..n)
                .map(|i| lo + (hi - lo) * i as f64 / n as f64)
                .collect()
        }
        _ => vec![(lo + hi) * 0.5],
    }
}

/// Even-odd point-in-polygon over all loops (holes included).
fn inside(loops: &[Vec<V2>], p: V2) -> bool {
    let mut odd = false;
    for l in loops {
        let n = l.len();
        for i in 0..n {
            let (a, b) = (l[i], l[(i + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) {
                let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
                if x > p.x {
                    odd = !odd;
                }
            }
        }
    }
    odd
}

fn seg_dist(p: V2, a: V2, b: V2) -> f64 {
    let ab = b - a;
    let l2 = ab.dot(ab);
    let t = if l2 > 0.0 {
        ((p - a).dot(ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p - (a + ab * t)).length()
}

/// Inserts `p` into the triangle containing it (three new triangles), or
/// into the edge it lies on (both neighbours split in two).
fn insert_point(verts: &mut Vec<(V2, V3)>, tris: &mut Vec<[usize; 3]>, p: V2, xyz: V3) {
    if verts.len() >= MAX_FACE_VERTICES || !xyz.is_finite() {
        return;
    }
    let mut hit = None;
    for (ti, t) in tris.iter().enumerate() {
        let [a, b, c] = t.map(|i| verts[i].0);
        let area = cross2(b - a, c - a);
        if area <= 0.0 {
            continue;
        }
        let eps = 1e-9 * area;
        let w = [
            cross2(b - a, p - a),
            cross2(c - b, p - b),
            cross2(a - c, p - c),
        ];
        if w.iter().all(|&x| x > eps) {
            hit = Some((ti, None));
            break;
        }
        if w.iter().all(|&x| x > -eps) {
            // On an edge: k is the edge t[k] -> t[k + 1].
            let k = (0..3).min_by(|&i, &j| w[i].abs().total_cmp(&w[j].abs()));
            hit = Some((ti, k));
            break;
        }
    }
    let Some((ti, on_edge)) = hit else {
        return;
    };
    let m = verts.len();
    match on_edge {
        None => {
            let [a, b, c] = tris[ti];
            verts.push((p, xyz));
            tris[ti] = [a, b, m];
            tris.push([b, c, m]);
            tris.push([c, a, m]);
        }
        Some(k) => {
            let t = tris[ti];
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let other = tris
                .iter()
                .position(|o| (0..3).any(|j| o[j] == b && o[(j + 1) % 3] == a));
            let Some(oi) = other else {
                return; // boundary edge: leave it untouched
            };
            verts.push((p, xyz));
            for idx in [ti, oi] {
                let t = tris[idx];
                let r = (0..3)
                    .find(|&j| edge_key(t[j], t[(j + 1) % 3]) == edge_key(a, b))
                    .unwrap_or(0);
                let (x, y, z) = (t[r], t[(r + 1) % 3], t[(r + 2) % 3]);
                tris[idx] = [x, m, z];
                tris.push([m, y, z]);
            }
        }
    }
}

/// Lawson edge flips towards a Delaunay triangulation in the scaled metric.
fn flip_edges(
    verts: &[(V2, V3)],
    tris: &mut [[usize; 3]],
    boundary: &HashSet<(usize, usize)>,
    scaled: &dyn Fn(V2) -> V2,
) {
    for _ in 0..200 {
        let mut edges: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for (ti, t) in tris.iter().enumerate() {
            for k in 0..3 {
                edges
                    .entry(edge_key(t[k], t[(k + 1) % 3]))
                    .or_default()
                    .push(ti);
            }
        }
        let mut keys: Vec<_> = edges.keys().copied().collect();
        keys.sort_unstable();
        let mut busy = vec![false; tris.len()];
        let mut flipped = 0;
        for e in keys {
            let adj = &edges[&e];
            if adj.len() != 2 || boundary.contains(&e) || busy[adj[0]] || busy[adj[1]] {
                continue;
            }
            let (t1, t2) = (tris[adj[0]], tris[adj[1]]);
            let r1 = (0..3).find(|&k| edge_key(t1[k], t1[(k + 1) % 3]) == e);
            let r2 = (0..3).find(|&k| edge_key(t2[k], t2[(k + 1) % 3]) == e);
            let (Some(r1), Some(r2)) = (r1, r2) else {
                continue;
            };
            let (a, b, c) = (t1[r1], t1[(r1 + 1) % 3], t1[(r1 + 2) % 3]);
            let d = t2[(r2 + 2) % 3];
            if t2[r2] != b || d == c {
                continue; // inconsistent winding: leave it alone
            }
            let p = |i: usize| scaled(verts[i].0);
            let (pa, pb, pc, pd) = (p(a), p(b), p(c), p(d));
            // The new triangles (a, d, c) and (d, b, c) must stay valid.
            if cross2(pd - pa, pc - pa) <= 0.0 || cross2(pb - pd, pc - pd) <= 0.0 {
                continue;
            }
            if in_circle(pa, pb, pc, pd) {
                tris[adj[0]] = [a, d, c];
                tris[adj[1]] = [d, b, c];
                busy[adj[0]] = true;
                busy[adj[1]] = true;
                flipped += 1;
            }
        }
        if flipped == 0 {
            break;
        }
    }
}

/// True when `d` lies strictly inside the circumcircle of CCW `(a, b, c)`.
fn in_circle(a: V2, b: V2, c: V2, d: V2) -> bool {
    let (ax, ay) = (a.x - d.x, a.y - d.y);
    let (bx, by) = (b.x - d.x, b.y - d.y);
    let (cx, cy) = (c.x - d.x, c.y - d.y);
    let det = (ax * ax + ay * ay) * (bx * cy - cx * by) - (bx * bx + by * by) * (ax * cy - cx * ay)
        + (cx * cx + cy * cy) * (ax * by - bx * ay);
    let scale = (ax.abs() + ay.abs() + bx.abs() + by.abs() + cx.abs() + cy.abs()).powi(4);
    det > 1e-12 * scale
}

/// Splits interior edges whose midpoint is too far from the surface. Passes of independent splits (each triangle loses at most one
/// edge per pass) keep the mesh conforming without complex bookkeeping.
fn split_deviating(
    eval: &dyn Fn(V2) -> V3,
    verts: &mut Vec<(V2, V3)>,
    mut tris: Vec<[usize; 3]>,
    boundary: &HashSet<(usize, usize)>,
) -> Vec<[usize; 3]> {
    // Angular criterion: a chord of length L on radius R deviates by about
    // L^2 / 8R, i.e. L * angle / 8, so this bounds the angle per segment
    // independently of the radius (like the edge sampling).
    let max_ratio = GRID_ANGLE * 1.6 / 8.0;
    for _ in 0..32 {
        let mut edges: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for (ti, t) in tris.iter().enumerate() {
            for k in 0..3 {
                edges
                    .entry(edge_key(t[k], t[(k + 1) % 3]))
                    .or_default()
                    .push(ti);
            }
        }
        let mut cand: Vec<(f64, (usize, usize), V2, V3)> = Vec::new();
        for &e in edges.keys() {
            if boundary.contains(&e) {
                continue;
            }
            let (a, b) = e;
            let muv = (verts[a].0 + verts[b].0) * 0.5;
            let mp = eval(muv);
            let dev = (mp - (verts[a].1 + verts[b].1) * 0.5).length();
            let chord = (verts[a].1 - verts[b].1).length();
            if mp.is_finite() && dev > (chord * max_ratio).max(1e-6) {
                cand.push((dev, e, muv, mp));
            }
        }
        if cand.is_empty() {
            break;
        }
        cand.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.cmp(&y.1)));
        let mut busy = vec![false; tris.len()];
        for (_, e, muv, mp) in cand {
            if verts.len() >= MAX_FACE_VERTICES {
                return tris;
            }
            let adj = &edges[&e];
            if adj.iter().any(|&t| busy[t]) {
                continue;
            }
            let mi = verts.len();
            verts.push((muv, mp));
            for &ti in adj {
                busy[ti] = true;
                let t = tris[ti];
                let r = (0..3)
                    .find(|&k| edge_key(t[k], t[(k + 1) % 3]) == e)
                    .unwrap_or(0);
                let (x, y, z) = (t[r], t[(r + 1) % 3], t[(r + 2) % 3]);
                tris[ti] = [x, mi, z];
                tris.push([mi, y, z]);
                busy.push(true);
            }
        }
    }
    tris
}
