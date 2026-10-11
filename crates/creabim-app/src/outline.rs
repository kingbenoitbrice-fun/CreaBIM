//! Redrawing a slab's outline or a wall's path, with the same snaps as
//! drawing, in a plan or the 3D view. The element keeps its identity (and
//! its doors, windows, tags and dimensions where they still fit); a wall
//! redrawn as a chain becomes the first segment plus new walls like it.

use creabim_core::{Document, Element, ElementId, ElementKind, Model, ModelError, Transaction};
use eframe::egui::{self, Color32, Pos2, Stroke};
use glam::{DVec2, DVec3};

#[derive(Clone, Debug)]
pub struct Outline {
    pub target: ElementId,
    pub points: Vec<DVec2>,
    /// Height the outline is drawn at, for the 3D view.
    pub z: f64,
    /// The level it is drawn on.
    pub level: ElementId,
    wall: bool,
}

impl Outline {
    /// Start redrawing `id`, a slab or a wall.
    pub fn start(model: &Model, id: ElementId) -> Option<Outline> {
        let (level, wall) = match &model.get(id)?.kind {
            ElementKind::Slab(s) => (s.level, false),
            ElementKind::Wall(w) => (w.level, true),
            _ => return None,
        };
        Some(Outline {
            target: id,
            points: vec![],
            z: model.level(level).map_or(0.0, |l| l.elevation),
            level,
            wall,
        })
    }

    pub fn hint(&self) -> &'static str {
        if self.wall {
            "Redrawing the wall: click each corner of its new path. Enter or right-click finishes, clicking the first corner closes the loop, Esc cancels."
        } else {
            "Redrawing the slab: click each corner of its new outline, then the first corner again (or Enter) to finish. Esc cancels."
        }
    }

    /// Add a corner. `near_first` says the pointer is on the first corner,
    /// which closes the outline. Returns true when it closed.
    pub fn click(&mut self, p: DVec2, near_first: bool) -> bool {
        if near_first && self.points.len() >= 3 {
            return true;
        }
        if self.points.last().is_none_or(|q| q.distance(p) > 1e-6) {
            self.points.push(p);
        }
        false
    }

    /// Commit the new shape.
    pub fn finish(self, doc: &mut Document, closed: bool) -> Result<String, ModelError> {
        let model = doc.model();
        let Some(before) = model.get(self.target).cloned() else {
            return Ok("The element is gone.".into());
        };
        let mut after = before.clone();
        let mut tx = Transaction::new(if self.wall {
            "Redraw wall"
        } else {
            "Redraw slab"
        });
        match &mut after.kind {
            ElementKind::Slab(s) => {
                if self.points.len() < 3 {
                    return Ok("A slab needs at least three corners.".into());
                }
                let mut pts = self.points;
                if creabim_core::polygon::signed_area2(&pts) < 0.0 {
                    pts.reverse();
                }
                s.boundary = pts;
                tx = tx.replace(before, after);
            }
            ElementKind::Wall(w) => {
                let mut pts = self.points;
                if closed && pts.len() >= 3 {
                    pts.push(pts[0]);
                }
                if pts.len() < 2 {
                    return Ok("A wall needs two points.".into());
                }
                w.start = pts[0];
                w.end = pts[1];
                let len = w.length();
                let template = w.clone();
                // doors and windows stay on the first segment, pulled in
                // to fit; those that can't fit are removed
                for oid in model.dependents(self.target) {
                    let Some(oe) = model.get(oid) else { continue };
                    let ElementKind::Opening(o) = &oe.kind else {
                        continue;
                    };
                    if o.width > len {
                        tx = tx.remove_cascade(model, oid);
                    } else if o.offset + o.width > len {
                        let mut moved = oe.clone();
                        if let ElementKind::Opening(m) = &mut moved.kind {
                            m.offset = len - o.width;
                        }
                        tx = tx.replace(oe.clone(), moved);
                    }
                }
                tx = tx.replace(before.clone(), after);
                for seg in pts.windows(2).skip(1) {
                    let mut w = template.clone();
                    w.start = seg[0];
                    w.end = seg[1];
                    tx = tx.insert(Element::new(before.name.clone(), ElementKind::Wall(w)));
                }
            }
            _ => return Ok(String::new()),
        }
        doc.commit(tx)?;
        Ok(if self.wall {
            "Wall redrawn. Ctrl+Z to undo.".into()
        } else {
            "Slab redrawn. Ctrl+Z to undo.".into()
        })
    }

    /// The outline so far and the line to the pointer.
    pub fn draw(
        &self,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<Pos2>,
        hover: Option<DVec2>,
    ) {
        let accent = Color32::from_rgb(230, 120, 30);
        let stroke = Stroke::new(2.0, accent);
        let at = |p: DVec2| project(p.extend(self.z));
        let mut pts = self.points.clone();
        pts.extend(hover);
        for w in pts.windows(2) {
            if let (Some(a), Some(b)) = (at(w[0]), at(w[1])) {
                painter.line_segment([a, b], stroke);
            }
        }
        // a slab closes back to its first corner
        if !self.wall && pts.len() >= 3 {
            if let (Some(a), Some(b)) = (at(pts[pts.len() - 1]), at(pts[0])) {
                painter.extend(egui::Shape::dashed_line(
                    &[a, b],
                    Stroke::new(1.0, accent),
                    6.0,
                    4.0,
                ));
            }
        }
        if let Some(first) = self.points.first().and_then(|p| at(*p)) {
            painter.circle_stroke(first, 6.0, stroke);
        }
        if let (Some(h), Some(last)) = (hover, self.points.last()) {
            if let Some(s) = at(h) {
                painter.text(
                    s + egui::vec2(12.0, -12.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!("{:.2} m", h.distance(*last)),
                    egui::FontId::proportional(13.0),
                    accent,
                );
            }
        }
    }

    /// Is the screen point `s` on the first corner?
    pub fn near_first(&self, s: Pos2, project: &dyn Fn(DVec3) -> Option<Pos2>) -> bool {
        self.points
            .first()
            .and_then(|p| project(p.extend(self.z)))
            .is_some_and(|f| f.distance(s) < 10.0)
    }
}
