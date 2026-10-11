//! Measuring meshes: bounds, plan hulls and the tightest rectangle around
//! them, for fitting walls, slabs and openings to modelled shapes.

use creabim_core::{EditMesh, ElementId, ElementKind, Model};
use creabim_geom::mesh::TriMesh;
use glam::{DVec2, DVec3};

/// Lowest and highest corners.
pub fn bounds(points: &[DVec3]) -> Option<(DVec3, DVec3)> {
    let first = *points.first()?;
    Some(
        points
            .iter()
            .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))),
    )
}

/// Convex hull in plan, counter-clockwise (Andrew's monotone chain).
pub fn hull(points: &[DVec2]) -> Vec<DVec2> {
    let mut p: Vec<DVec2> = points.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if p.len() < 3 {
        return p;
    }
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let mut lower: Vec<DVec2> = Vec::new();
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<DVec2> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// The smallest-area rectangle around a convex hull: its centre, the unit
/// direction of its long side, and its half length and half width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub center: DVec2,
    pub along: DVec2,
    pub half_len: f64,
    pub half_width: f64,
}

pub fn min_rect(hull: &[DVec2]) -> Option<Rect> {
    if hull.len() < 2 {
        return None;
    }
    let mut best: Option<(f64, Rect)> = None;
    for i in 0..hull.len() {
        let Some(u) = (hull[(i + 1) % hull.len()] - hull[i]).try_normalize() else {
            continue;
        };
        let v = u.perp();
        let (mut u0, mut u1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in hull {
            let (a, b) = (p.dot(u), p.dot(v));
            u0 = u0.min(a);
            u1 = u1.max(a);
            v0 = v0.min(b);
            v1 = v1.max(b);
        }
        let area = (u1 - u0) * (v1 - v0);
        if best.is_none_or(|(a, _)| area < a - 1e-12) {
            let center = u * (u0 + u1) / 2.0 + v * (v0 + v1) / 2.0;
            let (mut along, mut hl, mut hw) = (u, (u1 - u0) / 2.0, (v1 - v0) / 2.0);
            if hw > hl {
                along = v;
                std::mem::swap(&mut hl, &mut hw);
            }
            // a stable direction: pointing right, or up
            if along.x < -1e-9 || (along.x.abs() <= 1e-9 && along.y < 0.0) {
                along = -along;
            }
            best = Some((
                area,
                Rect {
                    center,
                    along,
                    half_len: hl,
                    half_width: hw,
                },
            ));
        }
    }
    best.map(|b| b.1)
}

/// The level an object standing at `z` belongs to: the highest at or
/// below it (within a centimetre), else the lowest.
pub fn level_at(model: &Model, z: f64) -> Option<(ElementId, f64)> {
    let mut levels: Vec<(ElementId, f64)> =
        model.levels().map(|(e, l)| (e.id, l.elevation)).collect();
    levels.sort_by(|a, b| a.1.total_cmp(&b.1));
    levels
        .iter()
        .rev()
        .find(|(_, e)| *e <= z + 0.01)
        .or(levels.first())
        .copied()
}

/// The level whose elevation is nearest `z`.
pub fn nearest_level(model: &Model, z: f64) -> Option<(ElementId, f64)> {
    model
        .levels()
        .map(|(e, l)| (e.id, l.elevation))
        .min_by(|a, b| (a.1 - z).abs().total_cmp(&(b.1 - z).abs()))
}

pub fn tri(mesh: &EditMesh) -> TriMesh {
    creabim_geom::scene::edit_mesh_to_tri(mesh, DVec3::ZERO)
}

pub fn area(m: &TriMesh) -> f64 {
    m.indices
        .chunks(3)
        .map(|t| {
            let p = |i: u32| glam::Vec3::from(m.positions[i as usize]).as_dvec3();
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            (b - a).cross(c - a).length() / 2.0
        })
        .sum()
}

pub fn points(m: &TriMesh) -> Vec<DVec3> {
    m.positions
        .iter()
        .map(|p| glam::Vec3::from(*p).as_dvec3())
        .collect()
}

/// A free mesh's corners in model coordinates.
pub fn world_mesh(model: &Model, id: ElementId) -> Option<EditMesh> {
    let e = model.get(id)?;
    let ElementKind::FreeMesh(f) = &e.kind else {
        return None;
    };
    let z = f
        .level
        .and_then(|l| model.level(l))
        .map_or(0.0, |l| l.elevation);
    let mut m = f.evaluated().mesh;
    for v in &mut m.vertices {
        *v += f.origin + DVec3::Z * z;
    }
    Some(m)
}

/// Do two shapes look the same: same bounds within a millimetre and the
/// same surface area within a tenth of a percent?
pub fn same_shape(a: &TriMesh, b: &TriMesh) -> bool {
    let (Some(ba), Some(bb)) = (bounds(&points(a)), bounds(&points(b))) else {
        return a.indices.is_empty() && b.indices.is_empty();
    };
    let close = |p: DVec3, q: DVec3| (p - q).abs().max_element() < 1e-3;
    let (aa, ab) = (area(a), area(b));
    close(ba.0, bb.0) && close(ba.1, bb.1) && (aa - ab).abs() <= aa.max(ab) * 1e-3 + 1e-6
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turned_box_gives_its_own_rectangle() {
        let d = DVec2::from_angle(0.5);
        let n = d.perp();
        let pts: Vec<DVec2> = [(-2.0, -0.1), (2.0, -0.1), (2.0, 0.1), (-2.0, 0.1)]
            .iter()
            .map(|(u, v)| DVec2::new(5.0, 1.0) + d * *u + n * *v)
            .collect();
        let r = min_rect(&hull(&pts)).unwrap();
        assert!((r.half_len - 2.0).abs() < 1e-9);
        assert!((r.half_width - 0.1).abs() < 1e-9);
        assert!(r.center.distance(DVec2::new(5.0, 1.0)) < 1e-9);
        assert!(r.along.dot(d).abs() > 1.0 - 1e-9);
    }
}
