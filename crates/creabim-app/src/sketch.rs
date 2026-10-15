//! Drawing sketches: lines, arcs and circles on a workplane, set on any face
//! in 3D, on a level in a plan, or on the section plane in a section. A
//! closed sketch extrudes into a mass you can keep editing. Constraints
//! (lengths, radii, horizontal, vertical, perpendicular, parallel, equal)
//! are added in the Properties panel and shown on the sketch.

use crate::planview::{PaneKind, Xf};
use creabim_core::{
    bulge_through, Constraint, Curve, Document, Element, ElementId, ElementKind, FreeMesh, Model,
    ModelError, SegmentKind, Sketch, Transaction, Workplane,
};
use eframe::egui::{self, Color32, Pos2, Stroke};
use glam::{DVec2, DVec3};

/// What the next click draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shape {
    #[default]
    Line,
    Arc,
    Circle,
}

impl Shape {
    pub const ALL: [Shape; 3] = [Shape::Line, Shape::Arc, Shape::Circle];

    pub fn label(self) -> &'static str {
        match self {
            Shape::Line => "Line (L)",
            Shape::Arc => "Arc (A)",
            Shape::Circle => "Circle (C)",
        }
    }

    fn key(self) -> egui::Key {
        match self {
            Shape::Line => egui::Key::L,
            Shape::Arc => egui::Key::A,
            Shape::Circle => egui::Key::C,
        }
    }
}

fn shape_id() -> egui::Id {
    egui::Id::new("sketch shape")
}

fn pick_id() -> egui::Id {
    egui::Id::new("sketch shape picked")
}

/// Pick the shape from the top bar's tool options, for the next frame.
pub fn pick_shape(ctx: &egui::Context, s: Shape) {
    ctx.data_mut(|d| d.insert_temp(pick_id(), s));
}

/// The shape the Sketch tool draws, shared by every pane.
pub fn shape(ctx: &egui::Context) -> Shape {
    ctx.data(|d| d.get_temp(shape_id())).unwrap_or_default()
}

/// The Line / Arc / Circle row in a pane's corner and the L, A and C keys.
/// Keeps `draft` drawing the chosen shape; returns a hint when it changes.
pub fn shape_controls(
    ui: &egui::Ui,
    rect: egui::Rect,
    pane: Option<ElementId>,
    draft: Option<&mut SketchDraft>,
) -> Option<String> {
    let ctx = ui.ctx().clone();
    let mut chosen = shape(&ctx);
    // picked in the top bar's tool options
    if let Some(s) = ctx.data_mut(|d| d.remove_temp::<Shape>(pick_id())) {
        chosen = s;
    }
    if !ctx.egui_wants_keyboard_input() && ui.rect_contains_pointer(rect) {
        ctx.input(|i| {
            for s in Shape::ALL {
                if !i.modifiers.command && i.key_pressed(s.key()) {
                    chosen = s;
                }
            }
        });
    }
    if !crate::slabtool::options_in_top_bar(&ctx) {
        egui::Area::new(egui::Id::new(("sketch shapes", pane)))
            .fixed_pos(rect.left_top() + egui::vec2(8.0, 8.0))
            .order(egui::Order::Foreground)
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for s in Shape::ALL {
                            if ui.selectable_label(chosen == s, s.label()).clicked() {
                                chosen = s;
                            }
                        }
                    });
                });
            });
    }
    let before = shape(&ctx);
    let mut msg = None;
    // a circle is a sketch of its own
    if chosen == Shape::Circle && draft.as_ref().is_some_and(|d| d.points.len() > 1) {
        if before != Shape::Circle {
            msg = Some("A circle is a sketch of its own: finish this one first (Enter).".into());
        }
        chosen = before;
    }
    if chosen != before {
        ctx.data_mut(|d| d.insert_temp(shape_id(), chosen));
        msg = Some(hint(chosen, draft.as_deref()));
    }
    if let Some(d) = draft {
        if d.shape != chosen {
            d.arc_end = None;
        }
        d.shape = chosen;
    }
    msg
}

/// What to click next, for the status bar.
pub fn hint(shape: Shape, draft: Option<&SketchDraft>) -> String {
    let started = draft.is_some();
    match (shape, draft.and_then(|d| d.arc_end)) {
        (Shape::Line, _) if started => {
            "Click the corners; click the first one to close the sketch, Enter to leave it open."
        }
        (Shape::Line, _) => {
            "Click where the sketch starts, then its corners. A and C draw arcs and circles."
        }
        (Shape::Arc, Some(_)) => "Click a point the arc passes through.",
        (Shape::Arc, None) if started => "Click the arc's end, then a point it passes through.",
        (Shape::Arc, None) => {
            "Click where the arc starts, then its end, then a point it passes through."
        }
        (Shape::Circle, _) if started => "Click a point on the circle.",
        (Shape::Circle, _) => "Click the circle's centre, then a point on it.",
    }
    .into()
}

/// A sketch being drawn. The plane is in model coordinates (absolute
/// heights) until the sketch is finished and given a level.
#[derive(Clone, Debug)]
pub struct SketchDraft {
    pub plane: Workplane,
    pub points: Vec<DVec2>,
    /// How each segment drawn so far runs.
    pub kinds: Vec<SegmentKind>,
    /// The pane drawing it: a view, or `None` for the 3D view.
    pub pane: Option<ElementId>,
    /// What the next click draws.
    pub shape: Shape,
    /// An arc's end, clicked and waiting for a point it passes through.
    pub arc_end: Option<DVec2>,
    /// A circle's radius, once its second click is in.
    radius: Option<f64>,
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
            kinds: vec![],
            pane,
            shape: Shape::Line,
            arc_end: None,
            radius: None,
        }
    }

    /// Whether a click on the first point would close the outline.
    fn closes_at_first(&self) -> bool {
        match self.shape {
            Shape::Line => self.points.len() >= 3,
            Shape::Arc => self.points.len() >= 2 && self.arc_end.is_none(),
            Shape::Circle => false,
        }
    }

    /// Where a pointer on the plane lands: on the first point when it is
    /// within `tol`, else on a 5 cm grid when `round` (a point already
    /// snapped to something is kept); Shift keeps the new line square to the
    /// plane's axes.
    pub fn snap(&self, p: DVec2, tol: f64, ortho: bool, round: bool) -> DVec2 {
        if self.closes_at_first() && p.distance(self.points[0]) < tol {
            return self.points[0];
        }
        let mut p = if round { (p / 0.05).round() * 0.05 } else { p };
        if let (true, None, Some(a)) = (ortho, self.arc_end, self.points.last()) {
            let d = p - *a;
            p = if d.x.abs() >= d.y.abs() {
                DVec2::new(p.x, a.y)
            } else {
                DVec2::new(a.x, p.y)
            };
        }
        p
    }

    /// Add a snapped point; clicking the first point again closes the
    /// sketch, and a circle's second click finishes it.
    pub fn click(&mut self, p: DVec2) -> Option<Click> {
        let last = *self.points.last()?;
        let first = self.points[0];
        match self.shape {
            Shape::Circle => {
                let r = p.distance(first);
                if self.points.len() != 1 || r < 1e-6 {
                    return None;
                }
                self.radius = Some(r);
                Some(Click::Closed)
            }
            Shape::Arc => match self.arc_end {
                None => {
                    if last.distance(p) < 1e-6 {
                        return None;
                    }
                    self.arc_end = Some(p);
                    Some(Click::Added)
                }
                Some(end) => {
                    let bulge = bulge_through(last, end, p);
                    if bulge.abs() > 50.0 {
                        // too near an end to say which way the arc runs
                        return None;
                    }
                    self.arc_end = None;
                    self.kinds.push(if bulge.abs() < 1e-6 {
                        SegmentKind::Line
                    } else {
                        SegmentKind::Arc { bulge }
                    });
                    if self.points.len() >= 2 && end.distance(first) < 1e-9 {
                        return Some(Click::Closed);
                    }
                    self.points.push(end);
                    Some(Click::Added)
                }
            },
            Shape::Line => {
                if self.points.len() >= 3 && p.distance(first) < 1e-9 {
                    self.kinds.push(SegmentKind::Line);
                    return Some(Click::Closed);
                }
                if last.distance(p) < 1e-6 {
                    return None;
                }
                self.points.push(p);
                self.kinds.push(SegmentKind::Line);
                Some(Click::Added)
            }
        }
    }

    /// The sketch as drawn so far, on its model-coordinate plane.
    fn sketch(&self, closed: bool) -> Sketch {
        let mut s = match self.radius {
            Some(r) => Sketch::circle(None, self.plane, self.points[0], r),
            None => Sketch::polyline(None, self.plane, self.points.clone(), false),
        };
        if self.radius.is_none() {
            s.kinds = self.kinds.clone();
            s.set_closed(closed);
        }
        s
    }

    /// Commit the sketch, if it has enough points. Returns the new element.
    pub fn finish(self, doc: &mut Document, closed: bool) -> Result<Option<ElementId>, ModelError> {
        if self.points.len() < 2 && self.radius.is_none() {
            return Ok(None);
        }
        let (level, elevation) =
            level_below(doc, self.plane.origin.z).map_or((None, 0.0), |(l, z)| (Some(l), z));
        let mut sketch = self.sketch(closed);
        sketch.level = level;
        sketch.plane.origin.z -= elevation;
        let e = Element::new("Sketch", ElementKind::Sketch(sketch));
        let id = e.id;
        doc.commit(Transaction::new("Draw sketch").insert(e))?;
        Ok(Some(id))
    }

    /// The draft and the line, arc or circle to the pointer, drawn through
    /// `project`.
    pub fn draw(
        &self,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<Pos2>,
        hover: Option<DVec2>,
    ) {
        let accent = Color32::from_rgb(230, 120, 30);
        let stroke = Stroke::new(2.0, accent);
        let at = |p: DVec2| project(self.plane.to_model(p));
        let line = |pts: &[DVec2]| {
            let scr: Option<Vec<Pos2>> = pts.iter().map(|p| at(*p)).collect();
            if let Some(scr) = scr {
                painter.add(egui::Shape::line(scr, stroke));
            }
        };
        let drawn = self.sketch(false);
        if drawn.points.len() >= 2 {
            for c in drawn.curves() {
                line(&c.points());
            }
        }
        let last = *self.points.last().unwrap_or(&DVec2::ZERO);
        // what the next click would add, and its size
        let ghost = hover.map(|h| match (self.shape, self.arc_end) {
            (Shape::Circle, _) if self.points.len() == 1 => {
                let r = h.distance(self.points[0]);
                let c = Curve::Arc {
                    center: self.points[0],
                    radius: r,
                    start: 0.0,
                    sweep: std::f64::consts::TAU,
                };
                (c, format!("R {r:.2} m"))
            }
            (Shape::Arc, Some(end)) => {
                let c = Curve::arc(last, end, bulge_through(last, end, h).clamp(-50.0, 50.0));
                let r = match c {
                    Curve::Arc { radius, .. } => format!("R {radius:.2} m"),
                    Curve::Line(..) => format!("{:.2} m", last.distance(end)),
                };
                (c, r)
            }
            _ => (Curve::Line(last, h), format!("{:.2} m", h.distance(last))),
        });
        if let Some((c, _)) = &ghost {
            line(&c.points());
        }
        if let Some(e) = self.arc_end.and_then(at) {
            painter.circle_stroke(e, 4.0, stroke);
        }
        if let Some(first) = at(self.points[0]) {
            let closing = self.closes_at_first() && hover == Some(self.points[0]);
            painter.circle_stroke(first, if closing { 8.0 } else { 4.0 }, stroke);
        }
        if let (Some((_, text)), Some(s)) = (ghost, hover.and_then(at)) {
            painter.text(
                s + egui::vec2(10.0, -10.0),
                egui::Align2::LEFT_BOTTOM,
                text,
                egui::FontId::proportional(13.0),
                accent,
            );
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

// ---- constraints in the Properties panel ---------------------------------

/// What the Properties panel did to a sketch.
pub enum Edit {
    None,
    /// Changed it; commit under this label.
    Done(&'static str),
    /// Refused a change, and why. The sketch is as it was.
    Refused(String),
}

/// Kinds of constraint the Add row offers.
const KINDS: [&str; 8] = [
    "Length",
    "Horizontal",
    "Vertical",
    "Perpendicular",
    "Parallel",
    "Equal length",
    "Radius",
    "Corners together",
];

/// The Add row's choices, kept between frames.
#[derive(Clone, Copy, Default)]
struct AddForm {
    kind: usize,
    a: usize,
    b: usize,
}

/// A segment's name in pickers: its number, shape and size.
fn segment_label(sk: &Sketch, i: usize) -> String {
    match (sk.circle, sk.radius(i)) {
        (Some(c), _) => format!("Circle, R {:.2} m", c.radius),
        (None, Some(r)) => format!("{} · arc, R {r:.2} m", i + 1),
        (None, None) => format!("{} · {:.2} m", i + 1, sk.chord(i)),
    }
}

/// The sketch's shape, its constraints and the row to add one.
pub fn properties(ui: &mut egui::Ui, sk: &mut Sketch) -> Edit {
    let mut edit = Edit::None;
    let arcs = (0..sk.segment_count())
        .filter(|i| matches!(sk.kind(*i), SegmentKind::Arc { .. }))
        .count();
    match sk.circle {
        Some(_) => {
            ui.label("A circle");
        }
        None => {
            let n = sk.segment_count();
            ui.label(format!(
                "{n} segment{}{}, {}",
                if n == 1 { "" } else { "s" },
                match arcs {
                    0 => String::new(),
                    1 => " (1 arc)".into(),
                    k => format!(" ({k} arcs)"),
                },
                if sk.closed { "closed" } else { "open" }
            ));
            let mut closed = sk.closed;
            if sk.can_close() && ui.checkbox(&mut closed, "Closed").changed() {
                sk.set_closed(closed);
                edit = Edit::Done("Close sketch");
            }
        }
    }
    if let Some(c) = sk.circle {
        let mut r = c.radius;
        let changed = ui
            .horizontal(|ui| {
                ui.label("Radius");
                ui.add(
                    egui::DragValue::new(&mut r)
                        .speed(0.01)
                        .range(0.001..=1e4)
                        .suffix(" m"),
                )
                .changed()
            })
            .inner;
        if changed {
            let fixed = sk
                .constraints
                .iter()
                .position(|c| matches!(c, Constraint::Radius { .. }));
            match fixed {
                Some(i) => match sk.set_dimension(i, r) {
                    Ok(()) => edit = Edit::Done("Dimension"),
                    Err(e) => edit = Edit::Refused(format!("Couldn't change that: {e}")),
                },
                None => {
                    if let Some(c) = &mut sk.circle {
                        c.radius = r;
                    }
                    edit = Edit::Done("Circle radius");
                }
            }
        }
    }

    ui.add_space(4.0);
    ui.label(egui::RichText::new("Constraints").strong());
    if sk.constraints.is_empty() {
        ui.label(
            egui::RichText::new("None yet. Add one below; the sketch moves to meet it.").weak(),
        );
    }
    let mut remove = None;
    for i in 0..sk.constraints.len() {
        let c = sk.constraints[i];
        ui.horizontal(|ui| {
            let mark = match c {
                Constraint::Length { .. } => "↔",
                c => c.glyph(),
            };
            ui.label(egui::RichText::new(mark).monospace().strong());
            match c.value() {
                Some(v) => {
                    let what = match c {
                        Constraint::Radius { .. } if sk.circle.is_some() => "Radius".to_string(),
                        Constraint::Radius { segment, .. } => format!("Radius of {}", segment + 1),
                        _ => format!("Length of {}", c.segments()[0] + 1),
                    };
                    ui.label(what);
                    let mut value = v;
                    let r = ui.add(
                        egui::DragValue::new(&mut value)
                            .speed(0.01)
                            .range(0.001..=1e4)
                            .suffix(" m"),
                    );
                    if r.changed() {
                        match sk.set_dimension(i, value) {
                            Ok(()) => edit = Edit::Done("Dimension"),
                            Err(e) => edit = Edit::Refused(format!("Couldn't change that: {e}")),
                        }
                    }
                }
                None => {
                    ui.label(c.describe());
                }
            }
            if ui.small_button("✕").on_hover_text("Remove").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        sk.unconstrain(i);
        edit = Edit::Done("Remove constraint");
    }

    // the Add row
    let id = ui.id().with("sketch constraint form");
    let mut form: AddForm = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let count = sk.segment_count();
    let corners = sk.points.len();
    if sk.circle.is_some() {
        form.kind = 6;
    }
    let two = matches!(form.kind, 3..=5);
    ui.horizontal_wrapped(|ui| {
        if sk.circle.is_none() {
            egui::ComboBox::from_id_salt("constraint_kind")
                .selected_text(KINDS[form.kind])
                .show_ui(ui, |ui| {
                    for (k, name) in KINDS.iter().enumerate() {
                        ui.selectable_value(&mut form.kind, k, *name);
                    }
                });
        }
        let picker = |ui: &mut egui::Ui, salt: &str, v: &mut usize, corners_only: bool| {
            let n = if corners_only { corners } else { count };
            *v = (*v).min(n.saturating_sub(1));
            let name = |i: usize| {
                if corners_only {
                    format!("Corner {}", i + 1)
                } else {
                    segment_label(sk, i)
                }
            };
            egui::ComboBox::from_id_salt(salt)
                .selected_text(name(*v))
                .show_ui(ui, |ui| {
                    for i in 0..n {
                        ui.selectable_value(v, i, name(i));
                    }
                });
        };
        if sk.circle.is_none() {
            let corner = form.kind == 7;
            picker(ui, "constraint_a", &mut form.a, corner);
            if two || corner {
                ui.label("and");
                picker(ui, "constraint_b", &mut form.b, corner);
            }
        }
        if ui.button("Add").clicked() {
            let (a, b) = (form.a, form.b);
            let mut c = match form.kind {
                0 => Constraint::Length {
                    segment: a,
                    value: 0.0,
                },
                1 => Constraint::Horizontal { segment: a },
                2 => Constraint::Vertical { segment: a },
                3 => Constraint::Perpendicular { a, b },
                4 => Constraint::Parallel { a, b },
                5 => Constraint::Equal { a, b },
                6 => Constraint::Radius {
                    segment: if sk.circle.is_some() { 0 } else { a },
                    value: 0.0,
                },
                _ => Constraint::Coincident { a, b },
            };
            // a new dimension starts at the size the sketch has now
            if let (Some(m), Some(v)) = (c.measure(sk), c.value_mut()) {
                *v = m;
            }
            match sk.constrain(c) {
                Ok(()) => edit = Edit::Done("Add constraint"),
                Err(e) => edit = Edit::Refused(format!("Couldn't add that: {e}")),
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(id, form));
    if !sk.constraints.is_empty() {
        ui.label(
            egui::RichText::new(
                "Drag a value or type it; the sketch follows. Its first corner stays put.",
            )
            .weak(),
        );
    }
    edit
}

// ---- constraint marks on the sketch ----------------------------------------

const MARK_COLOR: Color32 = Color32::from_rgb(40, 70, 140);
const SELECTED_COLOR: Color32 = Color32::from_rgb(242, 140, 51);

/// A small label with a pale backing, centred on `at`.
fn tag(painter: &egui::Painter, at: Pos2, text: &str, color: Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(11.0), color);
    let r = egui::Rect::from_center_size(at, galley.size()).expand2(egui::vec2(3.0, 1.0));
    painter.rect_filled(r, 3.0, Color32::from_white_alpha(210));
    painter.rect_stroke(
        r,
        3.0,
        Stroke::new(1.0, color.gamma_multiply(0.6)),
        egui::StrokeKind::Inside,
    );
    painter.galley(r.min + egui::vec2(3.0, 1.0), galley, color);
}

/// Constraint marks (H, V, ⟂, ∥, =) and dimension values beside the
/// segments they hold, for every sketch `show` accepts, drawn through
/// `project` (model coordinates).
pub fn draw_marks(
    painter: &egui::Painter,
    model: &Model,
    selection: Option<ElementId>,
    project: &dyn Fn(DVec3) -> Option<Pos2>,
    show: &dyn Fn(&Sketch) -> bool,
) {
    for e in model.iter() {
        let ElementKind::Sketch(sk) = &e.kind else {
            continue;
        };
        if sk.constraints.is_empty() || !show(sk) {
            continue;
        }
        let z = sk
            .level
            .and_then(|l| model.level(l))
            .map_or(0.0, |l| l.elevation);
        let at = |p: DVec2| project(sk.plane.to_model(p) + DVec3::Z * z);
        let color = if selection == Some(e.id) {
            SELECTED_COLOR
        } else {
            MARK_COLOR
        };
        // marks stack outwards from each segment
        let mut stacked = vec![0usize; sk.segment_count()];
        for c in &sk.constraints {
            let text = match *c {
                Constraint::Length { value, .. } => format!("{value:.2}"),
                Constraint::Radius { value, .. } => format!("R {value:.2}"),
                c => c.glyph().to_owned(),
            };
            for s in c.segments() {
                let curve = sk.curve(s);
                let (Some(m), Some(p), Some(q)) =
                    (at(curve.at(0.5)), at(curve.at(0.45)), at(curve.at(0.55)))
                else {
                    continue;
                };
                let along = (q - p).normalized();
                let mut out = egui::vec2(along.y, -along.x);
                if out == egui::Vec2::ZERO {
                    out = egui::vec2(0.0, -1.0);
                }
                // a circle's mark sits outside it, above
                if sk.circle.is_some() {
                    out = egui::vec2(0.0, -1.0);
                }
                let k = stacked.get(s).copied().unwrap_or(0);
                if let Some(n) = stacked.get_mut(s) {
                    *n += 1;
                }
                tag(painter, m + out * (12.0 + 17.0 * k as f32), &text, color);
            }
            for corner in c.corners() {
                if let Some(p) = sk.points.get(corner).and_then(|p| at(*p)) {
                    painter.circle_filled(p, 3.5, color);
                }
            }
        }
    }
}

/// The marks in a plan or section pane, and the selected sketch's lines
/// picked out (plans draw sketch lines plainly).
pub fn draw_marks_in_pane(
    painter: &egui::Painter,
    model: &Model,
    selection: Option<ElementId>,
    kind: PaneKind,
    xf: &Xf,
) {
    let (to_pane, facing): (Box<dyn Fn(DVec3) -> DVec2>, DVec3) = match kind {
        PaneKind::Plan { .. } => (Box::new(|p: DVec3| p.truncate()), DVec3::Z),
        PaneKind::Section { start, end } => {
            let d = (end - start).normalize_or_zero();
            (
                Box::new(move |p: DVec3| DVec2::new((p.truncate() - start).dot(d), p.z)),
                d.perp().extend(0.0),
            )
        }
        PaneKind::Axonometric | PaneKind::Schedule => return,
    };
    let project = |p: DVec3| Some(xf.to_screen(to_pane(p)));
    let show = |sk: &Sketch| match kind {
        PaneKind::Plan { level } => sk.level == Some(level),
        _ => sk.plane.normal().dot(facing).abs() > 0.999,
    };
    if let Some(ElementKind::Sketch(sk)) = selection.and_then(|id| model.get(id)).map(|e| &e.kind) {
        if show(sk) {
            let z = sk
                .level
                .and_then(|l| model.level(l))
                .map_or(0.0, |l| l.elevation);
            for (a, b) in sk.segments() {
                let (a, b) = (project(a + DVec3::Z * z), project(b + DVec3::Z * z));
                if let (Some(a), Some(b)) = (a, b) {
                    painter.line_segment([a, b], Stroke::new(2.0, SELECTED_COLOR));
                }
            }
        }
    }
    draw_marks(painter, model, selection, &project, &show);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> SketchDraft {
        SketchDraft::new(Workplane::ground(0.0), None)
    }

    #[test]
    fn an_arc_takes_its_end_then_a_point_on_it() {
        let mut d = draft();
        d.click(DVec2::new(2.0, 0.0));
        d.shape = Shape::Arc;
        assert!(matches!(d.click(DVec2::new(2.0, 2.0)), Some(Click::Added)));
        assert_eq!(d.points.len(), 2);
        d.click(DVec2::new(3.0, 1.0));
        assert_eq!(d.points.len(), 3);
        let s = d.sketch(false);
        let SegmentKind::Arc { bulge } = s.kind(1) else {
            panic!("expected an arc");
        };
        assert!((bulge - 1.0).abs() < 1e-9);
        // an arc back to the start closes it
        d.click(DVec2::ZERO);
        assert!(matches!(d.click(DVec2::new(1.0, 1.5)), Some(Click::Closed)));
        let s = d.sketch(true);
        assert!(s.closed && s.check().is_ok());
    }

    #[test]
    fn a_circle_is_its_centre_and_a_point_on_it() {
        let mut d = draft();
        d.shape = Shape::Circle;
        assert!(matches!(d.click(DVec2::new(0.0, 1.5)), Some(Click::Closed)));
        let s = d.sketch(true);
        assert_eq!(s.circle.map(|c| c.radius), Some(1.5));
        s.check().unwrap();
    }
}
