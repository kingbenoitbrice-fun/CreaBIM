//! Ear-clipping triangulation of a 2D polygon with holes.
//!
//! Holes are first joined to the outer loop with bridge edges (the classic
//! method from D. Eberly, "Triangulation by Ear Clipping"), giving one simple
//! polygon that is then clipped ear by ear. O(n^2), which is plenty for the
//! faces of architectural models.

use crate::math::{cross2, V2, V3};

/// Triangulates `loops` (the first is the outer boundary, the rest holes;
/// orientation does not matter) and returns triangles as indices into the
/// concatenation of all loops, counter-clockwise in the 2D plane.
#[cfg(test)]
pub fn triangulate(loops: &[Vec<V2>]) -> Vec<[usize; 3]> {
    triangulate_with_metric(loops, None)
}

/// Like [`triangulate`], with optional 3D positions (indexed like the
/// concatenated loops) used to prefer short diagonals on curved surfaces.
pub fn triangulate_with_metric(loops: &[Vec<V2>], metric: Option<&[V3]>) -> Vec<[usize; 3]> {
    let mut pts: Vec<V2> = Vec::new();
    let mut rings: Vec<Vec<usize>> = Vec::new();
    for l in loops {
        let start = pts.len();
        pts.extend_from_slice(l);
        let mut ring: Vec<usize> = (start..pts.len()).collect();
        dedup_ring(&pts, &mut ring);
        rings.push(ring);
    }
    let Some(outer) = rings.first().cloned() else {
        return Vec::new();
    };
    if outer.len() < 3 {
        return Vec::new();
    }
    let scale = bbox_scale(&pts);
    let mut poly = outer;
    if signed_area(&pts, &poly) < 0.0 {
        poly.reverse();
    }
    let mut holes: Vec<Vec<usize>> = rings[1..]
        .iter()
        .filter(|r| r.len() >= 3)
        .cloned()
        .map(|mut r| {
            if signed_area(&pts, &r) > 0.0 {
                r.reverse();
            }
            r
        })
        .collect();
    // Bridge the right-most holes first so later bridges cannot cross them.
    holes.sort_by(|a, b| max_x(&pts, b).total_cmp(&max_x(&pts, a)));
    for hole in holes {
        bridge_hole(&pts, &mut poly, &hole);
    }
    ear_clip(&pts, poly, scale, metric)
}

fn dedup_ring(pts: &[V2], ring: &mut Vec<usize>) {
    ring.dedup_by(|a, b| pts[*a] == pts[*b]);
    while ring.len() > 1 && pts[ring[0]] == pts[*ring.last().unwrap_or(&0)] {
        ring.pop();
    }
}

fn bbox_scale(pts: &[V2]) -> f64 {
    let (mut lo, mut hi) = (
        V2 {
            x: f64::MAX,
            y: f64::MAX,
        },
        V2 {
            x: f64::MIN,
            y: f64::MIN,
        },
    );
    for p in pts {
        lo.x = lo.x.min(p.x);
        lo.y = lo.y.min(p.y);
        hi.x = hi.x.max(p.x);
        hi.y = hi.y.max(p.y);
    }
    ((hi.x - lo.x).abs() + (hi.y - lo.y).abs()).max(1e-300)
}

pub fn signed_area(pts: &[V2], ring: &[usize]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| cross2(pts[ring[i]], pts[ring[(i + 1) % n]]))
        .sum::<f64>()
        * 0.5
}

fn max_x(pts: &[V2], ring: &[usize]) -> f64 {
    ring.iter().map(|&i| pts[i].x).fold(f64::MIN, f64::max)
}

/// Joins `hole` into `poly` through a bridge to a mutually visible vertex.
fn bridge_hole(pts: &[V2], poly: &mut Vec<usize>, hole: &[usize]) {
    let Some((hi, _)) = hole
        .iter()
        .enumerate()
        .max_by(|a, b| pts[*a.1].x.total_cmp(&pts[*b.1].x))
    else {
        return;
    };
    let m = pts[hole[hi]];
    // Closest edge hit by the ray from M towards +x.
    let n = poly.len();
    let mut best: Option<(f64, usize)> = None;
    for i in 0..n {
        let a = pts[poly[i]];
        let b = pts[poly[(i + 1) % n]];
        if (a.y > m.y) == (b.y > m.y) && a.y != m.y && b.y != m.y {
            continue;
        }
        if a.y == b.y {
            continue;
        }
        let t = (m.y - a.y) / (b.y - a.y);
        if !(0.0..=1.0).contains(&t) {
            continue;
        }
        let x = a.x + t * (b.x - a.x);
        if x < m.x {
            continue;
        }
        if best.is_none_or(|(bx, _)| x < bx) {
            best = Some((x, i));
        }
    }
    let Some((ix, edge)) = best else {
        return; // Hole outside the outer loop: ignore it.
    };
    let i_pt = V2 { x: ix, y: m.y };
    let a = edge;
    let b = (edge + 1) % n;
    let mut p = if pts[poly[a]].x > pts[poly[b]].x {
        a
    } else {
        b
    };
    // A reflex vertex inside triangle (M, I, P) would block the view of P;
    // pick the one making the smallest angle with the ray instead.
    let mut best_angle = f64::MAX;
    let tri = [m, i_pt, pts[poly[p]]];
    for j in 0..n {
        if j == p {
            continue;
        }
        let q = pts[poly[j]];
        let prev = pts[poly[(j + n - 1) % n]];
        let next = pts[poly[(j + 1) % n]];
        let reflex = cross2(q - prev, next - q) <= 0.0;
        if reflex && q.x >= m.x && in_triangle(q, tri[0], tri[1], tri[2], 0.0) {
            let d = q - m;
            let angle = d.y.abs().atan2(d.x);
            if angle < best_angle
                || (angle == best_angle && d.length() < (pts[poly[p]] - m).length())
            {
                best_angle = angle;
                p = j;
            }
        }
    }
    // After earlier bridges a vertex can occur twice in `poly`; splice into
    // the occurrence whose interior wedge contains the bridge, or the new
    // bridge would cross the old one.
    let target = poly[p];
    let d = m - pts[target];
    let occurrences = std::iter::once(p).chain((0..n).filter(|&k| k != p && poly[k] == target));
    for k in occurrences {
        let a = pts[poly[(k + n - 1) % n]] - pts[target];
        let b = pts[poly[(k + 1) % n]] - pts[target];
        let inside = if cross2(b, a) >= 0.0 {
            cross2(b, d) >= 0.0 && cross2(d, a) >= 0.0
        } else {
            cross2(b, d) >= 0.0 || cross2(d, a) >= 0.0
        };
        if inside {
            p = k;
            break;
        }
    }
    let mut merged = Vec::with_capacity(poly.len() + hole.len() + 2);
    merged.extend_from_slice(&poly[..=p]);
    for k in 0..=hole.len() {
        merged.push(hole[(hi + k) % hole.len()]);
    }
    merged.push(poly[p]);
    merged.extend_from_slice(&poly[p + 1..]);
    *poly = merged;
}

fn in_triangle(p: V2, a: V2, b: V2, c: V2, eps: f64) -> bool {
    let d1 = cross2(b - a, p - a);
    let d2 = cross2(c - b, p - b);
    let d3 = cross2(a - c, p - c);
    d1 >= -eps && d2 >= -eps && d3 >= -eps
}

fn ear_clip(pts: &[V2], poly: Vec<usize>, scale: f64, metric: Option<&[V3]>) -> Vec<[usize; 3]> {
    let n = poly.len();
    let mut out = Vec::with_capacity(n.saturating_sub(2));
    if n < 3 {
        return out;
    }
    let eps = 1e-12 * scale * scale;
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let mut alive = vec![true; n];
    let mut remaining = n;
    let corner = |prev: &[usize], next: &[usize], i: usize| {
        (pts[poly[prev[i]]], pts[poly[i]], pts[poly[next[i]]])
    };
    let is_ear = |prev: &[usize], next: &[usize], alive: &[bool], i: usize| -> bool {
        let (a, b, c) = corner(prev, next, i);
        if cross2(b - a, c - b) <= eps {
            return false;
        }
        let (ip, inx) = (prev[i], next[i]);
        let mut k = next[inx];
        while k != ip {
            if alive[k] {
                let q = pts[poly[k]];
                if q != a && q != b && q != c && in_triangle(q, a, b, c, 0.0) {
                    return false;
                }
            }
            k = next[k];
        }
        true
    };
    // Shortest new diagonal first (in 3D when available): on curved faces
    // this produces strips instead of long fans across the surface.
    let key = |prev: &[usize], next: &[usize], i: usize| -> f64 {
        let (a, c) = (poly[prev[i]], poly[next[i]]);
        match metric {
            Some(m) if a < m.len() && c < m.len() => (m[a] - m[c]).length(),
            _ => (pts[a] - pts[c]).length(),
        }
    };
    let mut ear: Vec<bool> = (0..n).map(|i| is_ear(&prev, &next, &alive, i)).collect();
    let mut start = 0;
    while remaining > 3 {
        let mut best: Option<(f64, usize)> = None;
        let mut i = start;
        for _ in 0..remaining {
            if ear[i] {
                let k = key(&prev, &next, i);
                if best.is_none_or(|(bk, _)| k < bk) {
                    best = Some((k, i));
                }
            }
            i = next[i];
        }
        if best.is_none() {
            // Ears can appear when a blocking vertex was clipped elsewhere.
            let mut i = start;
            for _ in 0..remaining {
                ear[i] = is_ear(&prev, &next, &alive, i);
                if ear[i] {
                    let k = key(&prev, &next, i);
                    if best.is_none_or(|(bk, _)| k < bk) {
                        best = Some((k, i));
                    }
                }
                i = next[i];
            }
        }
        let (i, emit) = match best {
            Some((_, i)) => (i, true),
            None => {
                // Degenerate input (self-touching or collinear): drop the
                // flattest vertex so the loop always terminates.
                let mut flat = (f64::MAX, start);
                let mut j = start;
                for _ in 0..remaining {
                    let (a, b, c) = corner(&prev, &next, j);
                    let cr = cross2(b - a, c - b).abs();
                    if cr < flat.0 {
                        flat = (cr, j);
                    }
                    j = next[j];
                }
                let (a, b, c) = corner(&prev, &next, flat.1);
                (flat.1, cross2(b - a, c - b) > eps)
            }
        };
        let (ip, inx) = (prev[i], next[i]);
        if emit {
            out.push([poly[ip], poly[i], poly[inx]]);
        }
        alive[i] = false;
        next[ip] = inx;
        prev[inx] = ip;
        remaining -= 1;
        start = ip;
        ear[ip] = is_ear(&prev, &next, &alive, ip);
        ear[inx] = is_ear(&prev, &next, &alive, inx);
    }
    let i = start;
    let (a, b, c) = corner(&prev, &next, i);
    if cross2(b - a, c - b).abs() > eps {
        out.push([poly[prev[i]], poly[i], poly[next[i]]]);
    }
    // Forced clips of degenerate input may be clockwise; fix their winding.
    for t in out.iter_mut() {
        if cross2(pts[t[1]] - pts[t[0]], pts[t[2]] - pts[t[1]]) < 0.0 {
            t.swap(1, 2);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::v2;

    fn area(pts: &[V2], tris: &[[usize; 3]]) -> f64 {
        tris.iter()
            .map(|t| 0.5 * cross2(pts[t[1]] - pts[t[0]], pts[t[2]] - pts[t[0]]))
            .sum()
    }

    #[test]
    fn square_with_two_holes() {
        let outer = vec![v2(0.0, 0.0), v2(10.0, 0.0), v2(10.0, 10.0), v2(0.0, 10.0)];
        let h1 = vec![v2(1.0, 1.0), v2(3.0, 1.0), v2(3.0, 3.0), v2(1.0, 3.0)];
        let h2 = vec![v2(6.0, 6.0), v2(6.0, 8.0), v2(8.0, 8.0), v2(8.0, 6.0)];
        let loops = vec![outer, h1, h2];
        let tris = triangulate(&loops);
        let all: Vec<V2> = loops.concat();
        assert!((area(&all, &tris) - (100.0 - 8.0)).abs() < 1e-9);
    }

    #[test]
    fn slab_with_square_and_round_hole_any_orientation() {
        let outer = vec![
            v2(0.0, 0.0),
            v2(2000.0, 0.0),
            v2(2000.0, 1000.0),
            v2(0.0, 1000.0),
        ];
        let sq = vec![
            v2(300.0, 300.0),
            v2(700.0, 300.0),
            v2(700.0, 700.0),
            v2(300.0, 700.0),
        ];
        let circle: Vec<V2> = (0..48)
            .map(|i| {
                let a = i as f64 * TAU48;
                v2(1500.0 + 150.0 * a.cos(), 500.0 + 150.0 * a.sin())
            })
            .collect();
        let ring_area = |l: &[V2]| {
            let idx: Vec<usize> = (0..l.len()).collect();
            signed_area(l, &idx).abs()
        };
        let expected = ring_area(&outer) - ring_area(&sq) - ring_area(&circle);
        let shift = |l: &[V2], k: usize| -> Vec<V2> {
            let mut v: Vec<V2> = l.iter().map(|p| v2(p.x - 1000.0, p.y - 500.0)).collect();
            let n = v.len();
            v.rotate_left(k % n);
            v
        };
        for flip in 0..64 {
            let mut o = shift(&outer, flip >> 3);
            let mut s = shift(&sq, flip >> 4);
            let mut c = shift(&circle, flip);
            if flip & 1 != 0 {
                o.reverse();
            }
            if flip & 2 != 0 {
                s.reverse();
            }
            if flip & 4 != 0 {
                c.reverse();
            }
            for order in [vec![o.clone(), s.clone(), c.clone()], vec![o, c, s]] {
                let tris = triangulate(&order);
                let all: Vec<V2> = order.concat();
                let a = area(&all, &tris);
                assert!(
                    (a - expected).abs() < 1e-6,
                    "flip {flip}: {a} vs {expected}"
                );
            }
        }
    }

    #[test]
    fn concave_l_shape_clockwise() {
        let l = vec![
            v2(0.0, 0.0),
            v2(0.0, 2.0),
            v2(1.0, 2.0),
            v2(1.0, 1.0),
            v2(2.0, 1.0),
            v2(2.0, 0.0),
        ];
        let tris = triangulate(std::slice::from_ref(&l));
        assert_eq!(tris.len(), 4);
        assert!((area(&l, &tris) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn anisotropic_strip_with_collinear_points() {
        // A cylinder's parameter rectangle: many points along u, few in v.
        let mut l = Vec::new();
        l.push(v2(0.0, -1000.0));
        for i in 0..=48 {
            l.push(v2(i as f64 * TAU48, 0.0));
        }
        for i in (0..=48).rev() {
            l.push(v2(i as f64 * TAU48, -1000.0));
        }
        l.pop();
        let tris = triangulate(std::slice::from_ref(&l));
        assert_eq!(tris.len(), l.len() - 2);
        for t in &tris {
            let a = 0.5 * cross2(l[t[1]] - l[t[0]], l[t[2]] - l[t[0]]);
            assert!(a > 1e-9, "degenerate triangle {t:?}");
        }
        let a = area(&l, &tris);
        assert!(
            (a - 2.0 * std::f64::consts::PI * 1000.0).abs() < 1e-6,
            "{a}"
        );
    }
    const TAU48: f64 = std::f64::consts::TAU / 48.0;

    #[test]
    fn degenerate_input_does_not_panic() {
        assert!(triangulate(&[]).is_empty());
        assert!(triangulate(&[vec![v2(0.0, 0.0), v2(1.0, 1.0)]]).is_empty());
        let line = vec![v2(0.0, 0.0), v2(1.0, 0.0), v2(2.0, 0.0)];
        assert!(triangulate(&[line]).is_empty());
        let nan = vec![v2(f64::NAN, 0.0), v2(1.0, 0.0), v2(0.0, 1.0)];
        let _ = triangulate(&[nan]);
    }
}
