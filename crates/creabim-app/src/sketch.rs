//! Drawing sketches: lines on a workplane, set on any face in 3D, on a
//! level in a plan, or on the section plane in a section. A closed sketch
//! extrudes into a mass you can keep editing.

use creabim_core::{
    Document, Element, ElementId, ElementKind, FreeMesh, ModelError, Sketch, Transaction, Workplane,
};
use eframe::egui::{self, Color32, Pos2, Stroke};
use glam::{DVec2, DVec3};

/// A sketch being drawn. The plane is in model coordinates (absolute
/// heights) until the sketch is finished and given a level.
#[derive(Clone, Debug)]
pub struct SketchDraft {
    pub plane: Workplane,
    pub points: Vec<DVec2>,
    /// The pane drawing it: a view, or `None` for the 3D view.
    pub pane: Option<ElementId>,
}

pub enum Click {
    Added,
    Closed,
}

impl SketchDraft {
    /// Start on `plane` with its first point at the plane's origin.
    pub fn new(plane: Workplane, pane: Option<ElementId>) -> Self {
        Self {
            plane,
            points: vec![DVec2::ZERO],
            pane,
        }
    }

    /// Where a pointer on the plane lands: on the first point when it is
    /// within `tol`, else on a 5 cm grid; Shift keeps the new line square
    /// to the plane's axes.
    pub fn snap(&self, p: DVec2, tol: f64, ortho: bool) -> DVec2 {
        if self.points.len() >= 3 && p.distance(self.points[0]) < tol {
            return self.points[0];
        }
        let mut p = (p / 0.05).round() * 0.05;
        if let (true, Some(a)) = (ortho, self.points.last()) {
            let d = p - *a;
            p = if d.x.abs() >= d.y.abs() {
                DVec2::new(p.x, a.y)
            } else {
                DVec2::new(a.x, p.y)
            };
        }
        p
    }

    /// Add a snapped point; clicking the first point again closes the sketch.
    pub fn click(&mut self, p: DVec2) -> Option<Click> {
        if self.points.len() >= 3 && p.distance(self.points[0]) < 1e-9 {
            return Some(Click::Closed);
        }
        if self.points.last().is_some_and(|q| q.distance(p) < 1e-6) {
            return None;
        }
        self.points.push(p);
        Some(Click::Added)
    }

    /// Commit the sketch, if it has enough points. Returns the new element.
    pub fn finish(self, doc: &mut Document, closed: bool) -> Result<Option<ElementId>, ModelError> {
        let closed = closed && self.points.len() >= 3;
        if self.points.len() < 2 {
            return Ok(None);
        }
        let (level, elevation) =
            level_below(doc, self.plane.origin.z).map_or((None, 0.0), |(l, z)| (Some(l), z));
        let mut plane = self.plane;
        plane.origin.z -= elevation;
        let e = Element::new(
            "Sketch",
            ElementKind::Sketch(Sketch {
                level,
                plane,
                points: self.points,
                closed,
            }),
        );
        let id = e.id;
        doc.commit(Transaction::new("Draw sketch").insert(e))?;
        Ok(Some(id))
    }

    /// The draft and the line to the pointer, drawn through `project`.
    pub fn draw(
        &self,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<Pos2>,
        hover: Option<DVec2>,
    ) {
        let accent = Color32::from_rgb(230, 120, 30);
        let stroke = Stroke::new(2.0, accent);
        let at = |p: DVec2| project(self.plane.to_model(p));
        let mut pts: Vec<DVec2> = self.points.clone();
        if let Some(h) = hover {
            pts.push(h);
        }
        for w in pts.windows(2) {
            if let (Some(a), Some(b)) = (at(w[0]), at(w[1])) {
                painter.line_segment([a, b], stroke);
            }
        }
        if let Some(first) = at(self.points[0]) {
            let closing = self.points.len() >= 3 && hover == Some(self.points[0]);
            painter.circle_stroke(first, if closing { 8.0 } else { 4.0 }, stroke);
        }
        if let (Some(h), Some(last)) = (hover, self.points.last()) {
            if let Some(s) = at(h) {
                painter.text(
                    s + egui::vec2(10.0, -10.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!("{:.2} m", h.distance(*last)),
                    egui::FontId::proportional(13.0),
                    accent,
                );
            }
        }
        draw_plane(painter, project, &self.plane, accent);
    }
}

/// A small square showing where a workplane lies.
pub fn draw_plane(
    painter: &egui::Painter,
    project: &dyn Fn(DVec3) -> Option<Pos2>,
    plane: &Workplane,
    color: Color32,
) {
    let r = 0.6;
    let corners = [(-r, -r), (r, -r), (r, r), (-r, r)];
    let pts: Option<Vec<Pos2>> = corners
        .iter()
        .map(|(x, y)| project(plane.to_model(DVec2::new(*x, *y))))
        .collect();
    if let Some(mut pts) = pts {
        pts.push(pts[0]);
        painter.add(egui::Shape::dashed_line(
            &pts,
            Stroke::new(1.0, color.gamma_multiply(0.7)),
            4.0,
            3.0,
        ));
    }
}

/// The level a height belongs to: the highest one at or below it.
pub fn level_below(doc: &Document, z: f64) -> Option<(ElementId, f64)> {
    let levels: Vec<(ElementId, f64)> = doc
        .model()
        .levels()
        .map(|(e, l)| (e.id, l.elevation))
        .collect();
    levels
        .iter()
        .filter(|(_, el)| *el <= z + 1e-6)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .or_else(|| levels.iter().min_by(|a, b| a.1.total_cmp(&b.1)))
        .copied()
}

/// Turn a sketch into a mass: its profile pushed `distance` off its plane.
/// The sketch is used up, as with SketchUp's push/pull. Returns the mass and,
/// for a closed sketch, its far cap (so E keeps pushing it).
pub fn extrude(
    doc: &mut Document,
    id: ElementId,
    distance: f64,
) -> Result<(ElementId, Option<usize>), String> {
    let Some(ElementKind::Sketch(s)) = doc.model().get(id).map(|e| e.kind.clone()) else {
        return Err("Select a sketch to extrude.".into());
    };
    if distance.abs() < 1e-6 {
        return Err("Set a distance to extrude by.".into());
    }
    let name = if s.closed { "Mass" } else { "Surface" };
    let mass = Element::new(
        name,
        ElementKind::FreeMesh(FreeMesh::new(s.level, s.plane.origin, s.extrude(distance))),
    );
    let mid = mass.id;
    let tx = Transaction::new("Extrude sketch")
        .remove_cascade(doc.model(), id)
        .insert(mass);
    doc.commit(tx).map_err(|e| e.to_string())?;
    Ok((mid, s.closed.then_some(1)))
}
