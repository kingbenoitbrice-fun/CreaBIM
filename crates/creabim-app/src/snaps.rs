//! Object snaps shared by every pane: ends, midpoints, intersections, points
//! on edges, perpendicular feet, guide lines and the axes through the point
//! a line started from. Geometry is held as segments in some 3D space (the
//! model for the 3D view, drawing coordinates with z = 0 for plans and
//! sections) and compared on screen through a projection, so the same
//! pixel tolerance applies however far the view is zoomed.

use creabim_geom::{PlanItem, SceneItem};
use eframe::egui::{self, Color32, Painter, Pos2, Stroke};
use glam::{DVec2, DVec3};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    Endpoint,
    Midpoint,
    Intersection,
    Perpendicular,
    OnEdge,
    Guide,
    /// Along the X or Y axis through the start point.
    AxisX,
    AxisY,
    Grid,
}

impl SnapKind {
    pub fn label(self) -> &'static str {
        match self {
            SnapKind::Endpoint => "Endpoint",
            SnapKind::Midpoint => "Midpoint",
            SnapKind::Intersection => "Intersection",
            SnapKind::Perpendicular => "Perpendicular",
            SnapKind::OnEdge => "On edge",
            SnapKind::Guide => "On guide",
            SnapKind::AxisX => "On X axis",
            SnapKind::AxisY => "On Y axis",
            SnapKind::Grid => "",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    pub p: DVec3,
    pub kind: SnapKind,
}

/// Snap geometry for one pane.
#[derive(Clone, Debug, Default)]
pub struct Snapper {
    /// Edges, and whether each is a guide line.
    segments: Vec<(DVec3, DVec3, bool)>,
    /// Corner and midpoint candidates with their kind.
    points: Vec<(DVec3, SnapKind)>,
    /// Distance between grid points; 0 turns the grid off.
    pub grid: f64,
}

/// Pixels within which a snap takes over the pointer.
pub const TOLERANCE: f32 = 10.0;

impl Snapper {
    pub fn new() -> Self {
        Self {
            grid: 0.05,
            ..Self::default()
        }
    }

    pub fn add_segment(&mut self, a: DVec3, b: DVec3, guide: bool) {
        if a.distance(b) < 1e-6 {
            return;
        }
        self.segments.push((a, b, guide));
        if !guide {
            self.points.push((a, SnapKind::Endpoint));
            self.points.push((b, SnapKind::Endpoint));
            self.points.push(((a + b) / 2.0, SnapKind::Midpoint));
        }
    }

    /// Where segments cross. Only meaningful for flat drawings; the 3D view
    /// snaps to corners and edges instead.
    pub fn add_intersections(&mut self) {
        let flat = |p: DVec3| p.truncate();
        let n = self.segments.len().min(4000);
        let mut out = Vec::new();
        for i in 0..n {
            let (a, b, ga) = self.segments[i];
            for j in i + 1..n {
                let (c, d, gb) = self.segments[j];
                if let Some(p) = crossing(flat(a), flat(b), flat(c), flat(d), ga || gb) {
                    out.push((p.extend(0.0), SnapKind::Intersection));
                }
            }
        }
        self.points.extend(out);
    }

    /// Snaps from a plan or section drawing, excluding `exclude`'s linework
    /// (an element being dragged must not snap to itself).
    pub fn from_drawing(items: &[PlanItem], exclude: &[creabim_core::ElementId]) -> Self {
        let mut s = Self::new();
        let p3 = |p: DVec2| p.extend(0.0);
        for it in items {
            if exclude.contains(&it.id()) {
                continue;
            }
            match it {
                PlanItem::Line { a, b, .. } => s.add_segment(p3(*a), p3(*b), false),
                PlanItem::Polygon { points, .. } => {
                    for i in 0..points.len() {
                        let j = (i + 1) % points.len();
                        s.add_segment(p3(points[i]), p3(points[j]), false);
                    }
                }
                _ => {}
            }
        }
        s.add_intersections();
        s
    }

    /// Snaps from the 3D scene: the corners and creases of every mesh (edges
    /// between faces that meet at an angle, and open edges), not the
    /// triangulation's inner diagonals.
    pub fn from_scene(scene: &[SceneItem], exclude: &[creabim_core::ElementId]) -> Self {
        let mut s = Self::new();
        for item in scene {
            if exclude.contains(&item.id) {
                continue;
            }
            for (a, b) in feature_edges(&item.mesh) {
                s.add_segment(a, b, false);
            }
        }
        s
    }

    /// The best snap for the pointer at `cursor`. `from` is where the line
    /// being drawn started, for perpendicular and axis snaps; `free` is the
    /// unsnapped point under the pointer, used for the grid.
    pub fn snap(
        &self,
        cursor: Pos2,
        project: &dyn Fn(DVec3) -> Option<Pos2>,
        from: Option<DVec3>,
        free: Option<DVec3>,
    ) -> Option<Snap> {
        let near = |p: DVec3| {
            project(p)
                .map(|s| s.distance(cursor))
                .filter(|d| *d < TOLERANCE)
        };
        // points beat edges: the nearest point within tolerance, by priority
        let rank = |k: SnapKind| match k {
            SnapKind::Endpoint => 0.0,
            SnapKind::Intersection => 0.5,
            SnapKind::Midpoint => 1.0,
            _ => 2.0,
        };
        let best_point = self
            .points
            .iter()
            .filter_map(|(p, k)| near(*p).map(|d| (d + rank(*k) * 2.0, *p, *k)))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, p, kind)) = best_point {
            return Some(Snap { p, kind });
        }
        if let Some(f) = from {
            for &(a, b, _) in &self.segments {
                let foot = closest_on_segment(a, b, f);
                if foot.distance(f) > 1e-6 && (foot - a).length() > 1e-6 && near(foot).is_some() {
                    let along = (b - a).normalize();
                    if (foot - f).normalize().dot(along).abs() < 1e-6 {
                        return Some(Snap {
                            p: foot,
                            kind: SnapKind::Perpendicular,
                        });
                    }
                }
            }
        }
        // the nearest point on an edge, in screen space
        let mut best: Option<(f32, Snap)> = None;
        for &(a, b, guide) in &self.segments {
            let (Some(sa), Some(sb)) = (project(a), project(b)) else {
                continue;
            };
            let t = screen_t(sa, sb, cursor);
            let s = sa + (sb - sa) * t;
            let d = s.distance(cursor);
            if d < TOLERANCE && best.is_none_or(|(bd, _)| d < bd) {
                let p = a + (b - a) * t as f64;
                let kind = if guide {
                    SnapKind::Guide
                } else {
                    SnapKind::OnEdge
                };
                best = Some((d, Snap { p, kind }));
            }
        }
        if let Some((_, s)) = best {
            return Some(s);
        }
        // the X and Y axes through the start
        if let (Some(f), Some(free)) = (from, free) {
            for (axis, kind) in [(DVec3::X, SnapKind::AxisX), (DVec3::Y, SnapKind::AxisY)] {
                let p = f + axis * (free - f).dot(axis);
                if let (Some(sp), Some(sf)) = (project(p), project(free)) {
                    if sp.distance(sf) < TOLERANCE && p.distance(f) > 1e-3 {
                        let len = grid_round((p - f).dot(axis), self.grid);
                        return Some(Snap {
                            p: f + axis * len,
                            kind,
                        });
                    }
                }
            }
        }
        free.map(|p| Snap {
            p: if self.grid > 0.0 {
                DVec3::new(grid_round(p.x, self.grid), grid_round(p.y, self.grid), p.z)
            } else {
                p
            },
            kind: SnapKind::Grid,
        })
    }
}

fn grid_round(v: f64, step: f64) -> f64 {
    if step > 0.0 {
        (v / step).round() * step
    } else {
        v
    }
}

fn screen_t(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 < 1e-6 {
        0.0
    } else {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    }
}

pub fn closest_on_segment(a: DVec3, b: DVec3, p: DVec3) -> DVec3 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    a + ab * t
}

/// Where segments ab and cd cross, if they do. A guide is endless, so it
/// crosses wherever its line does.
fn crossing(a: DVec2, b: DVec2, c: DVec2, d: DVec2, endless: bool) -> Option<DVec2> {
    let r = b - a;
    let s = d - c;
    let den = r.perp_dot(s);
    if den.abs() < 1e-12 {
        return None;
    }
    let t = (c - a).perp_dot(s) / den;
    let u = (c - a).perp_dot(r) / den;
    let inside = |v: f64| (-1e-9..=1.0 + 1e-9).contains(&v);
    // shared corners are already endpoints
    if !endless && (!inside(t) || !inside(u)) {
        return None;
    }
    let p = a + r * t;
    let corner = [a, b, c, d].iter().any(|q| q.distance(p) < 1e-6);
    (!corner).then_some(p)
}

/// The creases and open edges of a triangle mesh: where neighbouring faces
/// meet at more than about 20°, or where a face has no neighbour.
pub fn feature_edges(mesh: &creabim_geom::mesh::TriMesh) -> Vec<(DVec3, DVec3)> {
    // weld positions, since flat-shaded meshes repeat corners per face
    let key = |p: [f32; 3]| {
        (
            (p[0] * 1000.0).round() as i64,
            (p[1] * 1000.0).round() as i64,
            (p[2] * 1000.0).round() as i64,
        )
    };
    let mut ids: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut pts: Vec<DVec3> = Vec::new();
    let mut welded = |p: [f32; 3]| -> u32 {
        *ids.entry(key(p)).or_insert_with(|| {
            pts.push(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
            pts.len() as u32 - 1
        })
    };
    let mut tris = Vec::new();
    for t in mesh.indices.as_chunks::<3>().0 {
        let v = [
            welded(mesh.positions[t[0] as usize]),
            welded(mesh.positions[t[1] as usize]),
            welded(mesh.positions[t[2] as usize]),
        ];
        if v[0] != v[1] && v[1] != v[2] && v[0] != v[2] {
            tris.push(v);
        }
    }
    let normal = |t: &[u32; 3]| {
        let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
        (b - a).cross(c - a).normalize_or_zero()
    };
    let mut edges: HashMap<(u32, u32), Vec<DVec3>> = HashMap::new();
    for t in &tris {
        let n = normal(t);
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push(n);
        }
    }
    let mut out: Vec<(DVec3, DVec3)> = edges
        .into_iter()
        .filter(|(_, ns)| ns.len() == 1 || ns.iter().any(|n| n.dot(ns[0]) < 0.94))
        .map(|((a, b), _)| (pts[a as usize], pts[b as usize]))
        .collect();
    out.sort_by(|a, b| {
        (a.0.x, a.0.y, a.0.z)
            .partial_cmp(&(b.0.x, b.0.y, b.0.z))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Draw the snap marker at `at`: a square for ends, a triangle for
/// midpoints, a cross for intersections, a right angle for perpendicular
/// feet, a small circle on edges. Axis snaps draw their axis from `from`.
pub fn draw(painter: &Painter, snap: Snap, at: Pos2, from: Option<Pos2>) {
    let c = match snap.kind {
        SnapKind::Endpoint | SnapKind::Intersection => Color32::from_rgb(20, 150, 60),
        SnapKind::Midpoint => Color32::from_rgb(20, 120, 200),
        SnapKind::Perpendicular => Color32::from_rgb(160, 60, 180),
        SnapKind::OnEdge | SnapKind::Guide => Color32::from_rgb(200, 40, 40),
        SnapKind::AxisX => Color32::from_rgb(220, 50, 50),
        SnapKind::AxisY => Color32::from_rgb(40, 160, 60),
        SnapKind::Grid => return,
    };
    let s = Stroke::new(2.0, c);
    let r = 6.0;
    match snap.kind {
        SnapKind::Endpoint => {
            painter.rect_stroke(
                egui::Rect::from_center_size(at, egui::Vec2::splat(r * 2.0)),
                0.0,
                s,
                egui::StrokeKind::Middle,
            );
        }
        SnapKind::Midpoint => {
            painter.add(egui::Shape::closed_line(
                vec![
                    at + egui::vec2(0.0, -r),
                    at + egui::vec2(r, r * 0.8),
                    at + egui::vec2(-r, r * 0.8),
                ],
                s,
            ));
        }
        SnapKind::Intersection => {
            painter.line_segment([at + egui::vec2(-r, -r), at + egui::vec2(r, r)], s);
            painter.line_segment([at + egui::vec2(-r, r), at + egui::vec2(r, -r)], s);
        }
        SnapKind::Perpendicular => {
            painter.line_segment([at + egui::vec2(-r, r), at + egui::vec2(r, r)], s);
            painter.line_segment([at + egui::vec2(-r, r), at + egui::vec2(-r, -r)], s);
            painter.line_segment([at + egui::vec2(-r, 0.0), at + egui::vec2(0.0, 0.0)], s);
            painter.line_segment([at + egui::vec2(0.0, 0.0), at + egui::vec2(0.0, r)], s);
        }
        SnapKind::OnEdge | SnapKind::Guide => {
            painter.circle_stroke(at, r * 0.7, s);
        }
        SnapKind::AxisX | SnapKind::AxisY => {
            if let Some(f) = from {
                painter.add(egui::Shape::dashed_line(
                    &[f, at],
                    Stroke::new(1.5, c),
                    6.0,
                    4.0,
                ));
            }
            painter.circle_filled(at, 3.0, c);
        }
        SnapKind::Grid => {}
    }
    let label = snap.kind.label();
    if !label.is_empty() {
        painter.text(
            at + egui::vec2(10.0, 10.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(12.0),
            c,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(p: DVec3) -> Option<Pos2> {
        Some(Pos2::new((p.x * 100.0) as f32, (-p.y * 100.0) as f32))
    }

    #[test]
    fn corners_beat_edges_and_midpoints_are_found() {
        let mut s = Snapper::new();
        s.add_segment(DVec3::ZERO, DVec3::new(4.0, 0.0, 0.0), false);
        s.add_segment(DVec3::new(2.0, -1.0, 0.0), DVec3::new(2.0, 1.0, 0.0), false);
        s.add_intersections();
        let at = |x: f64, y: f64| flat(DVec3::new(x, y, 0.0)).unwrap();
        let hit = s.snap(at(0.03, 0.02), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::Endpoint);
        let hit = s.snap(at(2.02, 0.03), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::Intersection);
        let hit = s.snap(at(3.0, 0.04), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::OnEdge);
        assert!((hit.p.y).abs() < 1e-9);
    }

    #[test]
    fn a_line_snaps_square_to_the_axis_it_started_from() {
        let s = Snapper::new();
        let from = DVec3::new(1.0, 1.0, 0.0);
        let free = DVec3::new(3.013, 1.03, 0.0);
        let hit = s
            .snap(flat(free).unwrap(), &flat, Some(from), Some(free))
            .unwrap();
        assert_eq!(hit.kind, SnapKind::AxisX);
        assert!((hit.p.y - 1.0).abs() < 1e-9 && (hit.p.x - 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_cube_has_twelve_feature_edges() {
        let m = creabim_core::EditMesh::cuboid(DVec3::ONE);
        let mut tri = creabim_geom::mesh::TriMesh::default();
        for f in 0..m.faces.len() {
            for t in m.face_corners(f) {
                for i in t {
                    let p = m.vertices[m.faces[f][i] as usize].as_vec3();
                    tri.positions.push(p.to_array());
                    tri.normals.push([0.0; 3]);
                    tri.indices.push(tri.positions.len() as u32 - 1);
                }
            }
        }
        assert_eq!(feature_edges(&tri).len(), 12);
    }
}
