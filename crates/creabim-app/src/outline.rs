//! Clicking an outline, with the same snaps as drawing, in a plan or the 3D
//! view: a new slab of any shape (lines and arcs), or a new shape for an
//! existing slab or wall. A redrawn element keeps its identity (and its
//! doors, windows, tags and dimensions where they still fit); a wall
//! redrawn as a chain becomes the first segment plus new walls like it.

use creabim_core::{
    bulge_through, Curve, Document, Element, ElementId, ElementKind, Model, ModelError, Slab,
    Transaction,
};
use eframe::egui::{self, Color32, Pos2, Stroke};
use glam::{DVec2, DVec3};

/// Thickness of a newly drawn slab.
pub const NEW_SLAB_THICKNESS: f64 = 0.25;

#[derive(Clone, Debug)]
pub struct Outline {
    /// The slab or wall being redrawn; `None` draws a new slab.
    pub target: Option<ElementId>,
    pub points: Vec<DVec2>,
    /// How each segment drawn so far bends, as a bulge (0 is straight).
    pub bulges: Vec<f64>,
    /// An arc's end, clicked and waiting for a point it passes through.
    pub arc_end: Option<DVec2>,
    /// Height the outline is drawn at, for the 3D view.
    pub z: f64,
    /// The level it is drawn on.
    pub level: ElementId,
    wall: bool,
}

/// What a pane does after passing a frame's input to [`Outline::input`].
pub enum Next {
    /// Keep drawing; the hint is for the status bar.
    Drawing(String),
    /// Commit the outline, closed or not.
    Finish(bool),
    /// Drop the outline, with a message.
    Cancel(&'static str),
}

impl Outline {
    /// Start redrawing `id`, a slab or a wall.
    pub fn start(model: &Model, id: ElementId) -> Option<Outline> {
        let elevation = |l: ElementId| model.level(l).map_or(0.0, |l| l.elevation);
        let (level, z, wall) = match &model.get(id)?.kind {
            ElementKind::Slab(s) => (s.level, s.top(elevation(s.level)), false),
            ElementKind::Wall(w) => (w.level, elevation(w.level), true),
            _ => return None,
        };
        Some(Outline {
            target: Some(id),
            points: vec![],
            bulges: vec![],
            arc_end: None,
            z,
            level,
            wall,
        })
    }

    /// Start a new slab on `level` at its first corner.
    pub fn new_slab(model: &Model, level: ElementId, first: DVec2) -> Outline {
        Outline {
            target: None,
            points: vec![first],
            bulges: vec![],
            arc_end: None,
            z: model.level(level).map_or(0.0, |l| l.elevation),
            level,
            wall: false,
        }
    }

    pub fn is_wall(&self) -> bool {
        self.wall
    }

    pub fn hint(&self) -> String {
        if self.wall {
            return "Redrawing the wall: click each corner of its new path. Enter or right-click finishes, clicking the first corner closes the loop, Esc cancels.".into();
        }
        let what = if self.target.is_some() {
            "Redrawing the slab"
        } else {
            "Drawing a slab"
        };
        let next = match (self.arc_end, self.points.len()) {
            (Some(_), _) => "click a point the arc passes through",
            (None, 0) => "click its first corner",
            (None, 1 | 2) => "click the next corner",
            (None, _) => "click the next corner, or the first one (or Enter) to close",
        };
        format!("{what}: {next}. A draws arcs, L lines; Esc or right-click takes back a point.")
    }

    /// Whether clicking the first corner now would close the outline.
    fn can_close(&self, arc: bool) -> bool {
        let n = self.points.len();
        n >= 3 || (arc && n == 2 && self.arc_end.is_none())
    }

    /// Add a corner, or with `arc` an arc's end and then a point it passes
    /// through. `near_first` says the pointer is on the first corner, which
    /// closes the outline. Returns true when it closed.
    pub fn click(&mut self, p: DVec2, near_first: bool, arc: bool) -> bool {
        let arc = arc && !self.wall;
        if let Some(end) = self.arc_end {
            let Some(&last) = self.points.last() else {
                self.arc_end = None;
                return false;
            };
            let bulge = bulge_through(last, end, p);
            if bulge.abs() > 50.0 {
                return false; // too near an end to say which way it runs
            }
            self.arc_end = None;
            self.bulges
                .push(if bulge.abs() < 1e-6 { 0.0 } else { bulge });
            if end.distance(self.points[0]) < 1e-9 {
                return true;
            }
            self.points.push(end);
            return false;
        }
        if near_first && self.can_close(arc) {
            if arc {
                self.arc_end = Some(self.points[0]);
                return false;
            }
            self.bulges.push(0.0);
            return true;
        }
        if self.points.last().is_some_and(|q| q.distance(p) < 1e-6) {
            return false;
        }
        if arc && !self.points.is_empty() {
            self.arc_end = Some(p);
            return false;
        }
        if !self.points.is_empty() {
            self.bulges.push(0.0);
        }
        self.points.push(p);
        false
    }

    /// Take back the last click. Returns false when nothing is left.
    pub fn back_off(&mut self) -> bool {
        if self.arc_end.take().is_none() {
            self.points.pop();
            self.bulges.truncate(self.points.len().saturating_sub(1));
        }
        !self.points.is_empty()
    }

    /// Run a frame's keys and click: `click` is the snapped point and
    /// whether the pointer is on the first corner.
    pub fn input(
        &mut self,
        enter: bool,
        escape: bool,
        right_click: bool,
        click: Option<(DVec2, bool)>,
        arc: bool,
    ) -> Next {
        if self.wall {
            if escape {
                return Next::Cancel("Redraw cancelled.");
            }
            if enter || (right_click && !self.points.is_empty()) {
                return Next::Finish(false);
            }
        } else {
            if escape || right_click {
                if self.back_off() {
                    return Next::Drawing(self.hint());
                }
                return Next::Cancel(if self.target.is_some() {
                    "Redraw cancelled."
                } else {
                    "Slab cancelled. Right-click again to leave the Slab tool."
                });
            }
            if enter {
                return Next::Finish(true);
            }
        }
        if let Some((p, near)) = click {
            if self.click(p, near, arc) {
                return Next::Finish(true);
            }
        }
        Next::Drawing(self.hint())
    }

    /// Commit the shape. Returns a message and the new slab, if one was made.
    pub fn finish(
        self,
        doc: &mut Document,
        closed: bool,
    ) -> Result<(String, Option<ElementId>), ModelError> {
        let model = doc.model();
        if !self.wall {
            let mut bulges = self.bulges;
            bulges.resize(self.points.len(), 0.0);
            let arcs = bulges.iter().any(|b| b.abs() > 1e-9);
            if self.points.len() < 3 && !(arcs && self.points.len() == 2) {
                return Ok(("A slab needs at least three corners.".into(), None));
            }
            let mut slab = Slab::new(self.level, self.points, NEW_SLAB_THICKNESS);
            if arcs {
                slab.bulges = bulges;
            }
            slab.make_ccw();
            let Some(target) = self.target else {
                let e = Element::new("Slab", ElementKind::Slab(slab));
                let id = e.id;
                doc.commit(Transaction::new("Draw slab").insert(e))?;
                return Ok((
                    "Slab added. Its top is at the level; set its thickness and top offset in Properties. Ctrl+Z to undo.".into(),
                    Some(id),
                ));
            };
            let Some(before) = model.get(target).cloned() else {
                return Ok(("The element is gone.".into(), None));
            };
            let mut after = before.clone();
            if let ElementKind::Slab(s) = &mut after.kind {
                s.boundary = slab.boundary;
                s.bulges = slab.bulges;
            }
            doc.commit(Transaction::new("Redraw slab").replace(before, after))?;
            return Ok(("Slab redrawn. Ctrl+Z to undo.".into(), None));
        }
        let Some(before) = self.target.and_then(|t| model.get(t)).cloned() else {
            return Ok(("The element is gone.".into(), None));
        };
        let mut after = before.clone();
        let mut tx = Transaction::new("Redraw wall");
        let ElementKind::Wall(w) = &mut after.kind else {
            return Ok((String::new(), None));
        };
        let mut pts = self.points;
        if closed && pts.len() >= 3 {
            pts.push(pts[0]);
        }
        if pts.len() < 2 {
            return Ok(("A wall needs two points.".into(), None));
        }
        w.start = pts[0];
        w.end = pts[1];
        let len = w.length();
        let template = w.clone();
        // doors and windows stay on the first segment, pulled in to fit;
        // those that can't fit are removed
        for oid in model.dependents(before.id) {
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
        doc.commit(tx)?;
        Ok(("Wall redrawn. Ctrl+Z to undo.".into(), None))
    }

    /// The outline so far and the line or arc to the pointer.
    pub fn draw(
        &self,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<Pos2>,
        hover: Option<DVec2>,
    ) {
        let accent = Color32::from_rgb(230, 120, 30);
        let stroke = Stroke::new(2.0, accent);
        let at = |p: DVec2| project(p.extend(self.z));
        let line = |pts: &[DVec2], stroke: Stroke| {
            let scr: Option<Vec<Pos2>> = pts.iter().map(|p| at(*p)).collect();
            if let Some(scr) = scr {
                painter.add(egui::Shape::line(scr, stroke));
            }
        };
        let n = self.points.len();
        for i in 0..n.saturating_sub(1) {
            let b = self.bulges.get(i).copied().unwrap_or(0.0);
            line(
                &Curve::arc(self.points[i], self.points[i + 1], b).points(),
                stroke,
            );
        }
        let last = self.points.last().copied();
        let mut text = None;
        if let (Some(h), Some(last)) = (hover, last) {
            match self.arc_end {
                Some(end) => {
                    let c = Curve::arc(last, end, bulge_through(last, end, h).clamp(-50.0, 50.0));
                    line(&c.points(), stroke);
                    text = Some(match c {
                        Curve::Arc { radius, .. } => format!("R {radius:.2} m"),
                        Curve::Line(..) => format!("{:.2} m", last.distance(end)),
                    });
                }
                None => {
                    line(&[last, h], stroke);
                    text = Some(format!("{:.2} m", h.distance(last)));
                }
            }
        }
        // a slab closes back to its first corner
        let tail = self.arc_end.or(hover).or(last);
        if let (false, Some(tail), Some(&first)) = (self.wall, tail, self.points.first()) {
            if n >= 2 {
                if let (Some(a), Some(b)) = (at(tail), at(first)) {
                    painter.extend(egui::Shape::dashed_line(
                        &[a, b],
                        Stroke::new(1.0, accent),
                        6.0,
                        4.0,
                    ));
                }
            }
        }
        if let Some(e) = self.arc_end.and_then(at) {
            painter.circle_stroke(e, 4.0, stroke);
        }
        if let Some(first) = self.points.first().and_then(|p| at(*p)) {
            painter.circle_stroke(first, 6.0, stroke);
        }
        if let (Some(t), Some(s)) = (text, hover.and_then(at)) {
            painter.text(
                s + egui::vec2(12.0, -12.0),
                egui::Align2::LEFT_BOTTOM,
                t,
                egui::FontId::proportional(13.0),
                accent,
            );
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

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    fn level(m: &Model) -> ElementId {
        m.levels().next().unwrap().0.id
    }

    #[test]
    fn clicking_corners_and_the_first_again_draws_a_slab() {
        let mut doc = Document::from_model(sample_house());
        let l = level(doc.model());
        let mut o = Outline::new_slab(doc.model(), l, DVec2::ZERO);
        for p in [(4.0, 0.0), (4.0, 3.0), (2.0, 5.0), (0.0, 3.0)] {
            assert!(!o.click(DVec2::new(p.0, p.1), false, false));
        }
        assert!(o.click(DVec2::ZERO, true, false));
        let (_, id) = o.finish(&mut doc, true).unwrap();
        let Some(ElementKind::Slab(s)) = doc.model().get(id.unwrap()).map(|e| &e.kind) else {
            panic!("expected a slab");
        };
        assert_eq!(s.boundary.len(), 5);
        assert!((s.area() - 16.0).abs() < 1e-9);
    }

    #[test]
    fn an_arc_side_takes_its_end_then_a_point_on_it() {
        let mut doc = Document::from_model(sample_house());
        let l = level(doc.model());
        // a D: a straight side, then a half circle back to the start
        let mut o = Outline::new_slab(doc.model(), l, DVec2::new(0.0, 2.0));
        o.click(DVec2::new(0.0, -2.0), false, false);
        assert!(!o.click(DVec2::new(0.0, 2.0), true, true));
        assert!(o.arc_end.is_some());
        // take the arc's end back, then click it again
        assert!(o.back_off());
        assert!(o.arc_end.is_none() && o.points.len() == 2);
        o.click(DVec2::new(0.0, 2.0), true, true);
        assert!(o.click(DVec2::new(2.0, 0.0), false, true));
        let (_, id) = o.finish(&mut doc, true).unwrap();
        let Some(ElementKind::Slab(s)) = doc.model().get(id.unwrap()).map(|e| &e.kind) else {
            panic!("expected a slab");
        };
        assert!(s.has_arcs());
        assert!((s.area() - std::f64::consts::PI * 2.0).abs() < 0.05);
    }

    #[test]
    fn taking_back_every_point_leaves_nothing() {
        let m = sample_house();
        let mut o = Outline::new_slab(&m, level(&m), DVec2::ZERO);
        o.click(DVec2::new(1.0, 0.0), false, false);
        assert!(o.back_off());
        assert!(o.bulges.is_empty());
        assert!(!o.back_off());
    }
}
