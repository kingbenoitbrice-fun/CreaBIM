//! Sections and elevations generated from the model's 3D geometry.
//!
//! A section slices every element's mesh with a vertical plane and fills the
//! cut outlines; everything beyond the plane is drawn as projected edges with
//! hidden lines removed. An elevation is the same drawing with nothing cut.
//!
//! Hidden-line removal uses a software depth buffer: all faces beyond the
//! cut plane are rasterised, then each feature edge is sampled along its
//! length and kept only where it is not behind a face.

use crate::plan::{Pen, PlanItem, PlanView};
use crate::{build_scene, SceneItem};
use creabim_core::{ElementId, ElementKind, Model};
use glam::{DVec2, DVec3, Vec3};
use std::collections::HashMap;

/// Drawing coordinates: `x` runs along the section line, `y` is height.
pub type Drawing = PlanView;

/// The view frame of a section line.
#[derive(Clone, Copy, Debug)]
pub struct SectionFrame {
    pub origin: DVec2,
    /// Unit vector along the section line.
    pub along: DVec2,
    /// Unit vector the viewer looks along (left of `along`).
    pub look: DVec2,
    pub length: f64,
    pub depth: f64,
}

impl SectionFrame {
    pub fn new(start: DVec2, end: DVec2, depth: f64) -> Self {
        let along = (end - start).normalize_or_zero();
        Self {
            origin: start,
            along,
            look: along.perp(),
            length: start.distance(end),
            depth,
        }
    }

    /// World point → (along, height, distance in front of the cut plane).
    pub fn project(&self, p: DVec3) -> DVec3 {
        let d = p.truncate() - self.origin;
        DVec3::new(d.dot(self.along), p.z, d.dot(self.look))
    }
}

pub fn section_view(model: &Model, start: DVec2, end: DVec2, depth: f64, cut: bool) -> Drawing {
    let frame = SectionFrame::new(start, end, depth);
    let scene = build_scene(model);
    let mut items = Vec::new();

    // Faces beyond the cut plane, in drawing coordinates, per element.
    let mut faces: Vec<(ElementId, Vec<[DVec3; 3]>)> = Vec::new();
    for s in &scene {
        let mut tris = Vec::new();
        for t in s.mesh.indices.chunks(3) {
            let p = |i: u32| frame.project(Vec3::from(s.mesh.positions[i as usize]).as_dvec3());
            let tri = [p(t[0]), p(t[1]), p(t[2])];
            tris.extend(clip_triangle(tri, frame.depth));
        }
        faces.push((s.id, tris));
    }

    let (lo, hi) = extents(&faces, &frame);

    let zbuf = DepthBuffer::render(&faces, lo, hi);

    // Projected feature edges, hidden parts removed.
    for s in &scene {
        for (a, b) in feature_edges(s) {
            let (a, b) = (frame.project(a), frame.project(b));
            let Some((a, b)) = clip_segment(a, b, 1e-4, frame.depth) else {
                continue;
            };
            let Some((a, b)) = crop_along(a, b, 0.0, frame.length) else {
                continue;
            };
            for (p, q) in zbuf.visible_runs(a, b) {
                items.push(PlanItem::Line {
                    id: s.id,
                    pen: Pen::Projection,
                    a: p,
                    b: q,
                });
            }
        }
    }

    // Cut fill, drawn over the projection.
    if cut {
        for s in &scene {
            for loop_ in cut_loops(s, &frame) {
                let pts: Vec<DVec2> = loop_.iter().map(|p| DVec2::new(p.x, p.y)).collect();
                if pts.iter().all(|p| p.x < -1e-6 || p.x > frame.length + 1e-6) {
                    continue;
                }
                items.push(PlanItem::Polygon {
                    id: s.id,
                    pen: Pen::Cut,
                    points: pts,
                });
            }
        }
    }

    // Level datums across the drawing.
    for (e, l) in model.levels() {
        let (x0, x1) = (-1.0, frame.length + 1.0);
        items.push(PlanItem::Line {
            id: e.id,
            pen: Pen::Grid,
            a: DVec2::new(x0, l.elevation),
            b: DVec2::new(x1, l.elevation),
        });
        items.push(PlanItem::Label {
            id: e.id,
            at: DVec2::new(x1 + 1.6, l.elevation),
            text: format!("{} {:+.2}", e.name, l.elevation),
        });
    }
    PlanView { items }
}

/// Section of a view element, if it is a section or elevation.
pub fn view_drawing(model: &Model, view: ElementId) -> Option<Drawing> {
    match &model.get(view)?.kind {
        ElementKind::View(v) => Some(match &v.kind {
            creabim_core::ViewKind::Plan { level, cut_height } => {
                crate::plan_view(model, *level, *cut_height)
            }
            creabim_core::ViewKind::Section {
                start,
                end,
                depth,
                cut,
            } => section_view(model, *start, *end, *depth, *cut),
        }),
        _ => None,
    }
}

fn extents(faces: &[(ElementId, Vec<[DVec3; 3]>)], frame: &SectionFrame) -> (DVec2, DVec2) {
    let mut lo = DVec2::new(0.0, f64::MAX);
    let mut hi = DVec2::new(frame.length, f64::MIN);
    for (_, tris) in faces {
        for t in tris {
            for p in t {
                lo.y = lo.y.min(p.y);
                hi.y = hi.y.max(p.y);
            }
        }
    }
    if lo.y > hi.y {
        (lo.y, hi.y) = (0.0, 1.0);
    }
    (lo - DVec2::splat(0.1), hi + DVec2::splat(0.1))
}

/// Keep the part of a triangle with `0 <= d <= depth` (d = z component).
fn clip_triangle(tri: [DVec3; 3], depth: f64) -> Vec<[DVec3; 3]> {
    let mut poly: Vec<DVec3> = tri.to_vec();
    for (sign, limit) in [(1.0, 0.0), (-1.0, -depth)] {
        // keep points with sign * d >= limit
        let inside = |p: &DVec3| sign * p.z >= limit;
        let mut out = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            if inside(&a) {
                out.push(a);
            }
            if inside(&a) != inside(&b) {
                let t = (limit - sign * a.z) / (sign * (b.z - a.z));
                out.push(a + (b - a) * t);
            }
        }
        poly = out;
        if poly.len() < 3 {
            return vec![];
        }
    }
    (1..poly.len() - 1)
        .map(|i| [poly[0], poly[i], poly[i + 1]])
        .collect()
}

fn clip_segment(a: DVec3, b: DVec3, near: f64, far: f64) -> Option<(DVec3, DVec3)> {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let dz = b.z - a.z;
    for (num, den) in [(a.z - near, -dz), (far - a.z, dz)] {
        // constraint: num + t * (-den) >= 0  rewritten per Liang–Barsky
        if den.abs() < 1e-12 {
            if num < 0.0 {
                return None;
            }
        } else {
            let t = num / den;
            if den > 0.0 {
                t1 = t1.min(t);
            } else {
                t0 = t0.max(t);
            }
        }
    }
    (t0 <= t1).then(|| (a + (b - a) * t0, a + (b - a) * t1))
}

fn crop_along(a: DVec3, b: DVec3, lo: f64, hi: f64) -> Option<(DVec3, DVec3)> {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let dx = b.x - a.x;
    if dx.abs() < 1e-12 {
        return (a.x >= lo && a.x <= hi).then_some((a, b));
    }
    let (ta, tb) = ((lo - a.x) / dx, (hi - a.x) / dx);
    t0 = t0.max(ta.min(tb));
    t1 = t1.min(ta.max(tb));
    (t0 <= t1).then(|| (a + (b - a) * t0, a + (b - a) * t1))
}

/// Edges where the surface creases, plus open boundaries. Coplanar
/// triangulation edges inside a flat face are skipped.
pub fn feature_edges(item: &SceneItem) -> Vec<(DVec3, DVec3)> {
    let key = |p: [f32; 3]| {
        let q = |v: f32| (v as f64 * 1e4).round() as i64;
        (q(p[0]), q(p[1]), q(p[2]))
    };
    type Key = (i64, i64, i64);
    let mut edges: HashMap<(Key, Key), (DVec3, DVec3, Vec<Vec3>)> = HashMap::new();
    let m = &item.mesh;
    for t in m.indices.chunks(3) {
        let n = Vec3::from(m.normals[t[0] as usize]);
        for k in 0..3 {
            let (pa, pb) = (
                m.positions[t[k] as usize],
                m.positions[t[(k + 1) % 3] as usize],
            );
            let (ka, kb) = (key(pa), key(pb));
            if ka == kb {
                continue;
            }
            let id = if ka < kb { (ka, kb) } else { (kb, ka) };
            edges
                .entry(id)
                .or_insert_with(|| (Vec3::from(pa).as_dvec3(), Vec3::from(pb).as_dvec3(), vec![]))
                .2
                .push(n);
        }
    }
    let mut out: Vec<(DVec3, DVec3)> = edges
        .into_values()
        .filter(|(_, _, ns)| {
            ns.len() == 1 || ns.iter().any(|a| ns.iter().any(|b| a.dot(*b) < 0.995))
        })
        .map(|(a, b, _)| (a, b))
        .collect();
    out.sort_by(|x, y| {
        (x.0.x, x.0.y, x.0.z)
            .partial_cmp(&(y.0.x, y.0.y, y.0.z))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Closed outlines where the plane d = 0 slices an element's mesh, in drawing coordinates.
fn cut_loops(item: &SceneItem, frame: &SectionFrame) -> Vec<Vec<DVec2>> {
    let m = &item.mesh;
    let mut segs: Vec<(DVec2, DVec2)> = Vec::new();
    for t in m.indices.chunks(3) {
        let p: Vec<DVec3> = t
            .iter()
            .map(|&i| frame.project(Vec3::from(m.positions[i as usize]).as_dvec3()))
            .collect();
        let mut hits = Vec::new();
        for k in 0..3 {
            let (a, b) = (p[k], p[(k + 1) % 3]);
            if (a.z > 0.0) != (b.z > 0.0) {
                let s = a.z / (a.z - b.z);
                let q = a + (b - a) * s;
                hits.push(DVec2::new(q.x, q.y));
            }
        }
        if hits.len() == 2 && hits[0].distance(hits[1]) > 1e-9 {
            segs.push((hits[0], hits[1]));
        }
    }
    chain(segs)
}

/// Join segments that share endpoints into loops.
fn chain(mut segs: Vec<(DVec2, DVec2)>) -> Vec<Vec<DVec2>> {
    let close = |a: DVec2, b: DVec2| a.distance(b) < 1e-6;
    let mut loops = Vec::new();
    while let Some((a, b)) = segs.pop() {
        let mut pts = vec![a, b];
        loop {
            let end = *pts.last().expect("non-empty");
            let Some(i) = segs
                .iter()
                .position(|(p, q)| close(*p, end) || close(*q, end))
            else {
                break;
            };
            let (p, q) = segs.swap_remove(i);
            let next = if close(p, end) { q } else { p };
            if close(next, pts[0]) {
                break;
            }
            pts.push(next);
        }
        loops.push(simplify(pts));
    }
    loops.retain(|l| l.len() >= 3);
    loops
}

/// Drop points that sit on a straight line between their neighbours.
fn simplify(pts: Vec<DVec2>) -> Vec<DVec2> {
    let n = pts.len();
    if n < 4 {
        return pts;
    }
    (0..n)
        .filter(|&i| {
            let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            (b - a).perp_dot(c - b).abs() > 1e-9 * (b - a).length().max(1.0)
        })
        .map(|i| pts[i])
        .collect()
}

struct DepthBuffer {
    lo: DVec2,
    px: f64,
    w: usize,
    h: usize,
    depth: Vec<f32>,
}

impl DepthBuffer {
    fn render(faces: &[(ElementId, Vec<[DVec3; 3]>)], lo: DVec2, hi: DVec2) -> Self {
        let size = hi - lo;
        let px = (size.x.max(size.y) / 2048.0).max(0.005);
        let w = (size.x / px).ceil() as usize + 1;
        let h = (size.y / px).ceil() as usize + 1;
        let mut buf = Self {
            lo,
            px,
            w,
            h,
            depth: vec![f32::INFINITY; w * h],
        };
        for (_, tris) in faces {
            for t in tris {
                buf.raster(t);
            }
        }
        buf
    }

    fn to_px(&self, p: DVec3) -> DVec3 {
        DVec3::new(
            (p.x - self.lo.x) / self.px,
            (p.y - self.lo.y) / self.px,
            p.z,
        )
    }

    fn raster(&mut self, t: &[DVec3; 3]) {
        let [a, b, c] = t.map(|p| self.to_px(p));
        let area = (b - a).truncate().perp_dot((c - a).truncate());
        if area.abs() < 1e-12 {
            return; // seen edge-on
        }
        let x0 = a.x.min(b.x).min(c.x).floor().max(0.0) as usize;
        let x1 = (a.x.max(b.x).max(c.x).ceil() as usize).min(self.w - 1);
        let y0 = a.y.min(b.y).min(c.y).floor().max(0.0) as usize;
        let y1 = (a.y.max(b.y).max(c.y).ceil() as usize).min(self.h - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = glam::DVec2::new(x as f64 + 0.5, y as f64 + 0.5);
                let w0 = (c - b).truncate().perp_dot(p - b.truncate()) / area;
                let w1 = (a - c).truncate().perp_dot(p - c.truncate()) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let d = (w0 * a.z + w1 * b.z + w2 * c.z) as f32;
                let cell = &mut self.depth[y * self.w + x];
                if d < *cell {
                    *cell = d;
                }
            }
        }
    }

    /// Is the point no further than the nearest face around it (with tolerance)?
    fn visible(&self, p: DVec3) -> bool {
        let q = self.to_px(p);
        let (cx, cy) = (q.x.floor() as isize, q.y.floor() as isize);
        let mut far = f32::NEG_INFINITY;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
                    return true;
                }
                far = far.max(self.depth[y as usize * self.w + x as usize]);
            }
        }
        p.z as f32 <= far + 0.02
    }

    fn visible_runs(&self, a: DVec3, b: DVec3) -> Vec<(DVec2, DVec2)> {
        let len_px = ((b - a).truncate().length() / self.px).ceil().max(1.0) as usize;
        let steps = len_px * 2;
        let mut runs = Vec::new();
        let mut start: Option<f64> = None;
        let mut last = 0.0;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let vis = self.visible(a + (b - a) * t);
            match (vis, start) {
                (true, None) => start = Some(t),
                (false, Some(s)) => {
                    runs.push((s, last));
                    start = None;
                }
                _ => {}
            }
            if vis {
                last = t;
            }
        }
        if let Some(s) = start {
            runs.push((s, 1.0));
        }
        runs.into_iter()
            .filter(|(s, e)| e > s)
            .map(|(s, e)| {
                let p = a + (b - a) * s;
                let q = a + (b - a) * e;
                (DVec2::new(p.x, p.y), DVec2::new(q.x, q.y))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    fn sample() -> Model {
        sample_house()
    }

    #[test]
    fn section_through_the_front_door_cuts_walls_and_slabs() {
        let m = sample();
        // line at x = 2.1 running north, looking west
        let d = section_view(&m, DVec2::new(2.1, -3.0), DVec2::new(2.1, 9.0), 20.0, true);
        let cut_names: std::collections::BTreeSet<String> = d
            .items
            .iter()
            .filter_map(|i| match i {
                PlanItem::Polygon {
                    id, pen: Pen::Cut, ..
                } => Some(m.get(*id).unwrap().name.clone()),
                _ => None,
            })
            .collect();
        for n in [
            "GF Wall 1",
            "GF Wall 3",
            "FF Wall 1",
            "FF Wall 3",
            "GF Slab",
            "FF Slab",
            "Entrance Canopy",
        ] {
            assert!(cut_names.contains(n), "{n} not cut; got {cut_names:?}");
        }
        assert!(
            !cut_names.contains("GF Partition"),
            "partition at x = 6 is behind the viewer"
        );
        // the front door opening splits GF Wall 1's cut: the slice below the door is
        // missing, so the wall is cut as a lintel above the door only
        let wall1: Vec<_> = d
            .items
            .iter()
            .filter_map(|i| match i {
                PlanItem::Polygon {
                    id,
                    pen: Pen::Cut,
                    points,
                } if m.get(*id).unwrap().name == "GF Wall 1" => Some(points.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(wall1.len(), 1);
        let min_z = wall1[0].iter().map(|p| p.y).fold(f64::MAX, f64::min);
        assert!(
            (min_z - 2.2).abs() < 1e-6,
            "lintel starts at the door head, got {min_z}"
        );
    }

    #[test]
    fn elevation_cuts_nothing_and_hides_the_back_wall() {
        let m = sample();
        // south elevation, standing at y = -4 looking north
        let d = section_view(
            &m,
            DVec2::new(-2.0, -4.0),
            DVec2::new(12.0, -4.0),
            30.0,
            false,
        );
        assert!(!d
            .items
            .iter()
            .any(|i| matches!(i, PlanItem::Polygon { pen: Pen::Cut, .. })));
        let names: std::collections::BTreeSet<String> = d
            .items
            .iter()
            .filter_map(|i| match i {
                PlanItem::Line {
                    id,
                    pen: Pen::Projection,
                    ..
                } => Some(m.get(*id).unwrap().name.clone()),
                _ => None,
            })
            .collect();
        assert!(names.contains("GF Wall 1") && names.contains("Front Door"));
        assert!(names.contains("Entrance Canopy"));
        assert!(
            !names.contains("GF Window 3.1"),
            "a window in the back wall is hidden"
        );
        assert!(
            !names.contains("GF Partition"),
            "the inner partition is hidden"
        );
    }

    #[test]
    fn clipping_keeps_only_the_part_beyond_the_plane() {
        let t = [
            DVec3::new(0.0, 0.0, -1.0),
            DVec3::new(1.0, 0.0, 1.0),
            DVec3::new(0.0, 1.0, 1.0),
        ];
        let parts = clip_triangle(t, 10.0);
        assert!(!parts.is_empty());
        assert!(parts.iter().flatten().all(|p| p.z >= -1e-12));
    }
}
