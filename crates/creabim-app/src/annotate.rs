//! Angle, radius, diameter and arc-length dimensions and text notes in the
//! 2D panes and on sheets: drawing them on screen, the tools that place
//! them, dragging them, and their properties and text styles.

use crate::planview::{Tool, Xf};
use crate::selection::Selection;
use crate::snaps::SnapKind;
use creabim_core::{
    Dimension, DimensionKind, Document, Element, ElementId, ElementKind, Font, HostPoint, Model,
    ModelError, Note, StyleUse, TagShape, Terminator, TextStyle, Transaction, View,
};
use creabim_geom::annotate::attach;
use creabim_geom::annotation::{
    angle_text, circle_through, line_intersection, measure, sweep, Annotation, Ends, Shape,
    TextLook,
};
use creabim_geom::paper::Anchor;
use creabim_geom::PlanItem;
use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};
use glam::DVec2;

/// Annotations are drawn in this blue on screen unless they have a colour.
pub const INK: Color32 = Color32::from_rgb(30, 60, 120);
const ACCENT: Color32 = Color32::from_rgb(230, 120, 30);

pub fn color32([r, g, b]: [f32; 3]) -> Color32 {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(c(r), c(g), c(b))
}

/// The screen colour of an annotation.
pub fn ink(look: &TextLook, selected: bool, highlight: Color32) -> Color32 {
    match (selected, look.rgb) {
        (true, _) => highlight,
        (false, Some(rgb)) => color32(rgb),
        (false, None) => INK,
    }
}

/// Text height on screen, in pixels, for a look in a pane at `mm` drawing
/// units per paper mm.
pub fn text_px(look: &TextLook, mm: f64, xf: &Xf) -> f32 {
    ((look.size * mm * xf.scale as f64) as f32).max(7.0)
}

/// Text on screen in a look's family, weight and slant, turned `angle`
/// radians counter-clockwise about its anchor.
#[allow(clippy::too_many_arguments)]
pub fn text(
    painter: &egui::Painter,
    at: Pos2,
    text: &str,
    px: f32,
    color: Color32,
    angle: f64,
    anchor: Anchor,
    look: &TextLook,
) {
    let family = match look.font {
        Font::Mono => egui::FontFamily::Monospace,
        Font::Sans | Font::Serif => egui::FontFamily::Proportional,
    };
    let mut job = egui::text::LayoutJob::simple_singleline(
        text.to_string(),
        egui::FontId::new(px, family),
        color,
    );
    for s in &mut job.sections {
        s.format.italics = look.italic;
    }
    let galley = painter.layout_job(job);
    let w = galley.size().x;
    let shift = match anchor {
        Anchor::Start => w / 2.0,
        Anchor::Middle => 0.0,
        Anchor::End => -w / 2.0,
    };
    // the screen's y runs down, so a counter-clockwise turn is a negative angle
    let rot = egui::emath::Rot2::from_angle(-angle as f32);
    let centre = at + rot * Vec2::new(shift, 0.0);
    let pos = centre - rot * (galley.size() / 2.0);
    if look.bold {
        // no bold face on screen: the text again, a hair to the right
        let nudge = rot * Vec2::new((px * 0.05).max(0.6), 0.0);
        painter.add(
            egui::epaint::TextShape::new(pos + nudge, galley.clone(), color)
                .with_angle(-angle as f32),
        );
    }
    painter.add(egui::epaint::TextShape::new(pos, galley, color).with_angle(-angle as f32));
}

/// Draw an annotation in a pane.
pub fn paint(painter: &egui::Painter, xf: &Xf, item: &Annotation, color: Color32, mm: f64) {
    let m = item.marks(mm);
    for l in &m.lines {
        let pts: Vec<Pos2> = l.iter().map(|p| xf.to_screen(*p)).collect();
        painter.add(egui::Shape::line(pts, Stroke::new(1.0, color)));
    }
    for a in &m.arrows {
        let pts: Vec<Pos2> = a.iter().map(|p| xf.to_screen(*p)).collect();
        painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
    }
    paint_ends(painter, xf, &m.ends, color);
    let px = text_px(&item.look, mm, xf);
    for t in &m.texts {
        text(
            painter,
            xf.to_screen(t.at),
            &t.text,
            px,
            color,
            t.angle,
            t.anchor,
            &item.look,
        );
    }
}

/// Draw dimension and leader ends in a pane: thin and heavy strokes and
/// filled shapes.
pub fn paint_ends(painter: &egui::Painter, xf: &Xf, ends: &Ends, color: Color32) {
    let screen = |l: &Vec<DVec2>| -> Vec<Pos2> { l.iter().map(|p| xf.to_screen(*p)).collect() };
    for l in &ends.thin {
        painter.add(egui::Shape::line(screen(l), Stroke::new(1.0, color)));
    }
    for l in &ends.heavy {
        painter.add(egui::Shape::line(screen(l), Stroke::new(2.0, color)));
    }
    for f in &ends.fills {
        painter.add(egui::Shape::convex_polygon(screen(f), color, Stroke::NONE));
    }
}

// ---- tools ------------------------------------------------------------------

/// Is this one of the tools here?
pub fn is_tool(t: Tool) -> bool {
    matches!(
        t,
        Tool::Angle
            | Tool::Radius
            | Tool::Diameter
            | Tool::ArcLength
            | Tool::Note
            | Tool::SpotLevel
    )
}

type Picked = (HostPoint, DVec2);

/// What a tool has picked, waiting for the click that places it.
#[derive(Clone, Copy, Debug)]
enum Ready {
    Angle {
        vertex: Picked,
        a: Picked,
        b: Picked,
    },
    Circle {
        center: Picked,
        r: f64,
        host: Option<ElementId>,
        diameter: bool,
    },
    Arc {
        center: Picked,
        a: Picked,
        b: Picked,
    },
    Leader {
        target: Picked,
    },
    /// The point a spot level measures.
    Spot {
        point: Picked,
        plan: bool,
    },
}

/// What a drag in the Select tool holds.
#[derive(Clone, Copy, Debug)]
pub enum Grab {
    /// A note's text: where the drag began and the note's position then.
    Note(ElementId, DVec2, DVec2),
    /// A note's leader arrow.
    Leader(ElementId),
    /// A dimension's line or arc.
    Shape(ElementId),
    /// A note's leader elbow.
    Elbow(ElementId),
    /// A tag's leader end, and its host's centre in the drawing.
    TagEnd(ElementId, DVec2),
    /// A tag's leader elbow, and its host's centre in the drawing.
    TagElbow(ElementId, DVec2),
    /// A level's head in a section: the level, and its datum line's end.
    LevelHead(ElementId, DVec2),
}

/// Where the elbow grip of a leader running `from` → `to` sits: its
/// elbow, or halfway along when it has none.
fn elbow_grip(elbow: Option<DVec2>, from: DVec2, to: DVec2) -> DVec2 {
    elbow.unwrap_or((from + to) / 2.0)
}

/// The grips of a selected item: (where, what dragging it does).
fn grips(model: &Model, it: &PlanItem, mm: f64) -> Vec<(DVec2, Grab)> {
    match it {
        PlanItem::Annotation { id, item } => match &item.shape {
            Shape::Note {
                leader: Some(tip),
                elbow,
                ..
            } => {
                let m = item.marks(mm);
                let from = m
                    .lines
                    .last()
                    .and_then(|l| l.first().copied())
                    .unwrap_or(*tip);
                vec![
                    (*tip, Grab::Leader(*id)),
                    (elbow_grip(*elbow, from, *tip), Grab::Elbow(*id)),
                ]
            }
            // a level's head in a section or elevation, for this view only
            Shape::Level { at, line_end, .. } => {
                vec![(*at, Grab::LevelHead(*id, line_end.unwrap_or(*at)))]
            }
            _ => vec![],
        },
        PlanItem::Tag {
            id,
            anchor,
            at,
            elbow,
            ..
        } => {
            let Some(ElementKind::Tag(t)) = model.get(*id).map(|e| &e.kind) else {
                return vec![];
            };
            let centre = *anchor - t.end.unwrap_or(DVec2::ZERO);
            vec![
                (*anchor, Grab::TagEnd(*id, centre)),
                (
                    elbow_grip(*elbow, *at, *anchor),
                    Grab::TagElbow(*id, centre),
                ),
            ]
        }
        _ => vec![],
    }
}

/// The tools' clicks so far, and their options.
#[derive(Debug)]
pub struct Draft {
    points: Vec<Picked>,
    /// The first line picked for an angle: its element, ends, and the click.
    line: Option<(ElementId, DVec2, DVec2, DVec2)>,
    ready: Option<Ready>,
    /// The Note tool draws a leader first.
    pub leader: bool,
    /// A note was just placed: Properties should put the cursor in its text.
    pub focus_text: bool,
    pub grab: Option<Grab>,
    /// The styles new dimensions, notes and tags get (none: the default).
    pub dim_style: Option<ElementId>,
    pub note_style: Option<ElementId>,
    pub tag_style: Option<ElementId>,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            points: vec![],
            line: None,
            ready: None,
            leader: true,
            focus_text: false,
            grab: None,
            dim_style: None,
            note_style: None,
            tag_style: None,
        }
    }
}

impl Draft {
    pub fn clear(&mut self) {
        self.points.clear();
        self.line = None;
        self.ready = None;
    }

    /// The style a new element of this kind gets, if one is picked.
    pub fn style_for(&self, kind: &ElementKind) -> Option<ElementId> {
        match kind {
            ElementKind::Dimension(_) => self.dim_style,
            ElementKind::Note(_) => self.note_style,
            ElementKind::Tag(_) => self.tag_style,
            _ => None,
        }
    }

    /// Give a new dimension, note or tag the style picked for its kind.
    pub fn styled(&self, mut e: Element) -> Element {
        if let Some(id) = self.style_for(&e.kind) {
            set_style(&mut e.kind, id);
        }
        e
    }

    pub fn has_input(&self) -> bool {
        !self.points.is_empty() || self.line.is_some() || self.ready.is_some()
    }
}

/// The straight edge of the drawing nearest `p`, within `tol`.
pub fn segment_at(items: &[PlanItem], p: DVec2, tol: f64) -> Option<(ElementId, DVec2, DVec2)> {
    let mut best: Option<(f64, ElementId, DVec2, DVec2)> = None;
    let mut consider = |id: ElementId, a: DVec2, b: DVec2| {
        if a.distance(b) < 1e-9 {
            return;
        }
        let d = segment_distance(p, a, b);
        if d < tol && best.is_none_or(|(bd, ..)| d < bd) {
            best = Some((d, id, a, b));
        }
    };
    for it in items {
        match it {
            PlanItem::Line { id, a, b, .. } => consider(*id, *a, *b),
            PlanItem::Polygon { id, points, .. } => {
                for i in 0..points.len() {
                    consider(*id, points[i], points[(i + 1) % points.len()]);
                }
            }
            _ => {}
        }
    }
    best.map(|(_, id, a, b)| (id, a, b))
}

/// The arc or circle of the drawing nearest `p`, within `tol`: its element,
/// centre, radius and start and end angles.
fn arc_at(items: &[PlanItem], p: DVec2, tol: f64) -> Option<(ElementId, DVec2, f64, f64, f64)> {
    let mut best: Option<(f64, ElementId, DVec2, f64, f64, f64)> = None;
    for it in items {
        let PlanItem::Arc {
            id,
            center,
            radius,
            start,
            end,
            ..
        } = it
        else {
            continue;
        };
        let d = (p.distance(*center) - radius).abs();
        let span = (end - start).abs();
        let t = ((p - *center).to_angle() - start.min(*end)).rem_euclid(std::f64::consts::TAU);
        let on = span >= std::f64::consts::TAU - 1e-6 || t <= span + 1e-6;
        if on && d < tol && best.is_none_or(|b| d < b.0) {
            best = Some((d, *id, *center, *radius, *start, *end));
        }
    }
    best.map(|(_, id, c, r, s, e)| (id, c, r, s.min(e), s.max(e)))
}

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-18)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// The point on the endless line through `a`, `b` nearest `p`.
fn project(p: DVec2, a: DVec2, b: DVec2) -> DVec2 {
    let d = (b - a).normalize_or_zero();
    a + d * (p - a).dot(d)
}

/// A point on the arm from `vertex` through the line `a`,`b` on the side of
/// `click`, at least a little way out.
fn arm(vertex: DVec2, a: DVec2, b: DVec2, click: DVec2) -> DVec2 {
    let q = project(click, a, b);
    if q.distance(vertex) > 1e-3 {
        return q;
    }
    // a click right on the corner: along the line to its further end
    if a.distance(vertex) > b.distance(vertex) {
        a
    } else {
        b
    }
}

/// A click with an annotation tool in a plan or section. `hit` is the
/// snapped point under the pointer and what it attaches to; `raw` the
/// pointer itself. Returns a hint for the status bar.
#[allow(clippy::too_many_arguments)]
pub fn click(
    doc: &mut Document,
    view_id: ElementId,
    view: &View,
    tool: Tool,
    hit: Option<(HostPoint, DVec2, Option<SnapKind>)>,
    raw: DVec2,
    items: &[PlanItem],
    tol: f64,
    d: &mut Draft,
    selection: &mut Selection,
) -> Result<String, ModelError> {
    let model = doc.model();
    let (hp, p, snapped) = match hit {
        Some((hp, p, k)) => (hp, p, k.is_some_and(|k| k.is_point())),
        None => (HostPoint::free(raw), raw, false),
    };
    if let Some(ready) = d.ready.take() {
        d.clear();
        let mut e = d.styled(place(model, view_id, view, ready, tool, raw));
        if let ElementKind::Dimension(dim) = &mut e.kind {
            if dim.kind == DimensionKind::Spot {
                // a new spot level reads like the last one made
                dim.pattern = model
                    .iter()
                    .filter_map(|e| match &e.kind {
                        ElementKind::Dimension(d) if d.kind == DimensionKind::Spot => {
                            Some(d.pattern.clone())
                        }
                        _ => None,
                    })
                    .last()
                    .flatten();
            }
        }
        let id = e.id;
        let label = match &e.kind {
            ElementKind::Note(_) => "Add note",
            _ => "Add dimension",
        };
        doc.commit(Transaction::new(label).insert(e))?;
        selection.only(id);
        return Ok(added(tool, d));
    }
    match tool {
        Tool::Note if !d.leader => {
            let e = d.styled(Element::new(
                "Note",
                ElementKind::Note(Note::new(view_id, raw, "Note")),
            ));
            selection.only(e.id);
            doc.commit(Transaction::new("Add note").insert(e))?;
            Ok(added(tool, d))
        }
        Tool::SpotLevel => {
            let point = match view.kind {
                // in a plan, the top of the slab under the click
                creabim_core::ViewKind::Plan { level, .. } => {
                    (attach(model, view, slab_under(model, level, raw), raw), raw)
                }
                _ => (hp, p),
            };
            let plan = matches!(view.kind, creabim_core::ViewKind::Plan { .. });
            d.ready = Some(Ready::Spot { point, plan });
            Ok("Now click where its text goes.".into())
        }
        Tool::Note => {
            d.ready = Some(Ready::Leader { target: (hp, p) });
            Ok("Now click where the note's text goes.".into())
        }
        Tool::Angle => {
            if d.points.is_empty() && !snapped {
                if let Some((id, a, b)) = segment_at(items, raw, tol) {
                    match d.line.take() {
                        None => {
                            d.line = Some((id, a, b, raw));
                            return Ok("Now click the second line.".into());
                        }
                        Some((id0, a0, b0, c0)) => {
                            let Some(v) = line_intersection(a0, b0, a, b) else {
                                return Ok("Those lines are parallel: pick two that meet.".into());
                            };
                            let (pa, pb) = (arm(v, a0, b0, c0), arm(v, a, b, raw));
                            d.ready = Some(Ready::Angle {
                                vertex: (attach(model, view, Some(id0), v), v),
                                a: (attach(model, view, Some(id0), pa), pa),
                                b: (attach(model, view, Some(id), pb), pb),
                            });
                            return Ok("Now click where the arc goes.".into());
                        }
                    }
                }
            }
            if d.line.is_some() {
                return Ok("Click a second line, or Esc to start again.".into());
            }
            d.points.push((hp, p));
            match d.points.as_slice() {
                [vertex, a, b] => {
                    d.ready = Some(Ready::Angle {
                        vertex: *vertex,
                        a: *a,
                        b: *b,
                    });
                    d.points.clear();
                    Ok("Now click where the arc goes.".into())
                }
                [_] => Ok("Now click a point on the first arm.".into()),
                _ => Ok("Now click a point on the second arm.".into()),
            }
        }
        Tool::Radius | Tool::Diameter | Tool::ArcLength => {
            if d.points.is_empty() {
                if let Some((id, c, r, start, end)) = arc_at(items, raw, tol) {
                    let center = (attach(model, view, Some(id), c), c);
                    d.ready = Some(if tool == Tool::ArcLength {
                        let (a, b) = (
                            c + DVec2::from_angle(start) * r,
                            c + DVec2::from_angle(end) * r,
                        );
                        Ready::Arc {
                            center,
                            a: (attach(model, view, Some(id), a), a),
                            b: (attach(model, view, Some(id), b), b),
                        }
                    } else {
                        Ready::Circle {
                            center,
                            r,
                            host: Some(id),
                            diameter: tool == Tool::Diameter,
                        }
                    });
                    return Ok("Now click where the dimension line goes.".into());
                }
            }
            d.points.push((hp, p));
            if d.points.len() < 3 {
                return Ok(format!(
                    "Point {} of 3 on the {}. Or click an arc or circle in the drawing.",
                    d.points.len(),
                    if tool == Tool::ArcLength {
                        "arc: its start, a point on it, then its end"
                    } else {
                        "circle"
                    }
                ));
            }
            let pts = std::mem::take(&mut d.points);
            let Some((c, r)) = circle_through(pts[0].1, pts[1].1, pts[2].1) else {
                return Ok("Those points are in a line: pick three on a curve.".into());
            };
            // the centre is kept on what the first point is on, so it moves with it
            let host = pts[0].0.host;
            let center = (attach(model, view, host, c), c);
            d.ready = Some(if tool == Tool::ArcLength {
                let (mut a, mut b) = (pts[0], pts[2]);
                if sweep(c, a.1, pts[1].1) > sweep(c, a.1, b.1) {
                    std::mem::swap(&mut a, &mut b);
                }
                Ready::Arc { center, a, b }
            } else {
                Ready::Circle {
                    center,
                    r,
                    host,
                    diameter: tool == Tool::Diameter,
                }
            });
            Ok("Now click where the dimension line goes.".into())
        }
        _ => Ok(String::new()),
    }
}

/// The topmost slab on `level` whose outline holds `p`.
fn slab_under(model: &Model, level: ElementId, p: DVec2) -> Option<ElementId> {
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Slab(s)
                if s.level == level && creabim_core::polygon::contains(&s.boundary, p) =>
            {
                Some((e.id, s.top_offset))
            }
            _ => None,
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|s| s.0)
}

fn added(tool: Tool, d: &mut Draft) -> String {
    if tool == Tool::Note {
        d.focus_text = true;
        "Note added. Type its text in Properties; drag it to move it.".into()
    } else if tool == Tool::SpotLevel {
        "Spot level added. Its text pattern is in Properties; drag its text to move it.".into()
    } else {
        "Dimension added. Drag it to move its line; its style is in Properties.".into()
    }
}

/// The element a placing click at `at` makes.
fn place(
    model: &Model,
    view_id: ElementId,
    view: &View,
    ready: Ready,
    tool: Tool,
    at: DVec2,
) -> Element {
    match ready {
        Ready::Angle { vertex, a, b } => {
            let (a, b) = if sweep(vertex.1, a.1, at) > sweep(vertex.1, a.1, b.1) {
                (b, a)
            } else {
                (a, b)
            };
            let radius = at.distance(vertex.1);
            Element::new(
                "Angle",
                ElementKind::Dimension(Dimension::about(
                    DimensionKind::Angular,
                    view_id,
                    vertex.0,
                    a.0,
                    b.0,
                    radius,
                )),
            )
        }
        Ready::Circle {
            center,
            r,
            host,
            diameter,
        } => {
            let dir = (at - center.1).normalize_or(DVec2::X);
            let on = attach(model, view, host, center.1 + dir * r);
            let extra = (at.distance(center.1) - r).max(0.0);
            let (name, kind) = if diameter {
                ("Diameter", DimensionKind::Diameter)
            } else {
                ("Radius", DimensionKind::Radius)
            };
            Element::new(
                name,
                ElementKind::Dimension(Dimension::about(kind, view_id, center.0, on, on, extra)),
            )
        }
        Ready::Spot { point, .. } => Element::new(
            "Spot level",
            ElementKind::Dimension(Dimension::spot(
                view_id,
                point.0,
                attach(model, view, point.0.host, at),
                None,
            )),
        ),
        Ready::Arc { center, a, b } => {
            let offset = at.distance(center.1) - center.1.distance(a.1);
            Element::new(
                "Arc length",
                ElementKind::Dimension(Dimension::about(
                    DimensionKind::ArcLength,
                    view_id,
                    center.0,
                    a.0,
                    b.0,
                    offset,
                )),
            )
        }
        Ready::Leader { target } => {
            let _ = tool;
            let mut n = Note::new(view_id, at, "Note");
            n.leader = Some(target.0);
            Element::new("Note", ElementKind::Note(n))
        }
    }
}

/// The annotation a placing click at `at` would make, for the preview.
fn shape_of(ready: Ready, at: DVec2) -> (Shape, String) {
    match ready {
        Ready::Spot { point, plan } => (
            Shape::Spot {
                at: point.1,
                text_at: at,
                plan,
            },
            "Spot level".into(),
        ),
        Ready::Angle { vertex, a, b } => {
            let (a, b) = if sweep(vertex.1, a.1, at) > sweep(vertex.1, a.1, b.1) {
                (b.1, a.1)
            } else {
                (a.1, b.1)
            };
            (
                Shape::Angle {
                    vertex: vertex.1,
                    a,
                    b,
                    radius: at.distance(vertex.1),
                },
                angle_text(sweep(vertex.1, a, b)),
            )
        }
        Ready::Circle {
            center,
            r,
            diameter,
            ..
        } => {
            let c = center.1;
            let on = c + (at - c).normalize_or(DVec2::X) * r;
            let kind = if diameter {
                DimensionKind::Diameter
            } else {
                DimensionKind::Radius
            };
            (
                Shape::Radius {
                    center: c,
                    at: on,
                    extra: (at.distance(c) - r).max(0.0),
                    diameter,
                },
                measure(&crate::units::get(), kind, c, on, on),
            )
        }
        Ready::Arc { center, a, b } => (
            Shape::ArcLength {
                center: center.1,
                a: a.1,
                b: b.1,
                offset: at.distance(center.1) - center.1.distance(a.1),
            },
            measure(
                &crate::units::get(),
                DimensionKind::ArcLength,
                center.1,
                a.1,
                b.1,
            ),
        ),
        Ready::Leader { target } => (
            Shape::Note {
                at,
                leader: Some(target.1),
                elbow: None,
            },
            "Note".into(),
        ),
    }
}

/// What a tool will make, drawn in the accent colour as the pointer moves.
#[allow(clippy::too_many_arguments)]
pub fn preview(
    painter: &egui::Painter,
    xf: &Xf,
    tool: Tool,
    d: &Draft,
    hover: Option<DVec2>,
    raw: Option<DVec2>,
    items: &[PlanItem],
    tol: f64,
    mm: f64,
) {
    let accent = Stroke::new(1.5, ACCENT);
    if let (Some(ready), Some(at)) = (d.ready, raw) {
        let (shape, text) = shape_of(ready, at);
        let item = Annotation {
            shape,
            text,
            look: TextLook::default(),
            mm,
        };
        paint(painter, xf, &item, ACCENT, mm);
        return;
    }
    if let Some((_, a, b, _)) = d.line {
        painter.line_segment([xf.to_screen(a), xf.to_screen(b)], Stroke::new(3.0, ACCENT));
    }
    for (_, p) in &d.points {
        painter.circle_stroke(xf.to_screen(*p), 4.0, accent);
    }
    if let (Some(first), Some(h)) = (d.points.first(), hover) {
        let pts: Vec<Pos2> = d
            .points
            .iter()
            .map(|p| xf.to_screen(p.1))
            .chain([xf.to_screen(h)])
            .collect();
        if tool == Tool::Angle {
            // the arms from the vertex
            for p in &pts[1..] {
                painter.line_segment([xf.to_screen(first.1), *p], accent);
            }
        } else {
            painter.add(egui::Shape::dashed_line(&pts, accent, 6.0, 4.0));
        }
    }
    // what a click would pick
    if d.points.is_empty() {
        let Some(p) = raw else { return };
        match tool {
            Tool::Radius | Tool::Diameter | Tool::ArcLength => {
                if let Some((_, c, r, s, e)) = arc_at(items, p, tol) {
                    let n = 32;
                    let pts: Vec<Pos2> = (0..=n)
                        .map(|i| {
                            xf.to_screen(
                                c + DVec2::from_angle(s + (e - s) * i as f64 / n as f64) * r,
                            )
                        })
                        .collect();
                    painter.add(egui::Shape::line(pts, Stroke::new(3.0, ACCENT)));
                }
            }
            Tool::Angle => {
                if let Some((_, a, b)) = segment_at(items, p, tol) {
                    painter.line_segment(
                        [xf.to_screen(a), xf.to_screen(b)],
                        Stroke::new(2.0, ACCENT.gamma_multiply(0.6)),
                    );
                }
            }
            _ => {}
        }
    }
}

// ---- dragging ---------------------------------------------------------------

/// Drag a note (its text, or its leader's arrow when it is selected) or a
/// dimension's line in the Select tool. `snapped` is the point under the
/// pointer and what it attaches to, for a leader. Returns `Some` while a
/// drag is in progress.
#[allow(clippy::too_many_arguments)]
pub fn drag(
    doc: &mut Document,
    view_id: Option<ElementId>,
    view: Option<&View>,
    items: &[PlanItem],
    xf: &Xf,
    response: &egui::Response,
    d: &mut Draft,
    selection: &mut Selection,
    mm: f64,
    snapped: Option<(HostPoint, DVec2)>,
) -> Option<Option<String>> {
    let tol = 6.0 / xf.scale as f64;
    if response.drag_started_by(egui::PointerButton::Primary) {
        let press = response
            .ctx
            .input(|i| i.pointer.press_origin())
            .or(response.interact_pointer_pos())?;
        let w = xf.to_world(press);
        let model = doc.model();
        // the grips of what is selected come first
        let grip = items.iter().rev().find_map(|it| {
            if !selection.contains(it.id()) {
                return None;
            }
            grips(model, it, mm)
                .into_iter()
                .find(|(p, _)| xf.to_screen(*p).distance(press) < 9.0)
                .map(|g| g.1)
        });
        d.grab = grip.or_else(|| {
            items.iter().rev().find_map(|it| {
                let PlanItem::Annotation { id, item } = it else {
                    return None;
                };
                if !item.hit(w, tol, mm) {
                    return None;
                }
                Some(match item.shape {
                    Shape::Note { at, .. } => Grab::Note(*id, w, at),
                    _ => Grab::Shape(*id),
                })
            })
        });
        match d.grab {
            Some(
                Grab::Note(id, ..)
                | Grab::Leader(id)
                | Grab::Shape(id)
                | Grab::Elbow(id)
                | Grab::TagEnd(id, _)
                | Grab::TagElbow(id, _)
                | Grab::LevelHead(id, _),
            ) => selection.only(id),
            None => {}
        }
    }
    let grab = d.grab?;
    if response.drag_stopped() || !response.dragged() {
        d.grab = None;
        return Some(None);
    }
    let p = xf.to_world(response.interact_pointer_pos()?);
    let r = match grab {
        Grab::Note(id, from, start) => doc.edit_merged(id, "Move note", |e| {
            if let ElementKind::Note(n) = &mut e.kind {
                n.at = start + p - from;
            }
        }),
        Grab::Leader(id) => {
            let target = snapped.map_or(HostPoint::free(p), |s| s.0);
            doc.edit_merged(id, "Move leader", |e| {
                if let ElementKind::Note(n) = &mut e.kind {
                    n.leader = Some(target);
                }
            })
        }
        Grab::Shape(id) => {
            let shape = items.iter().find_map(|it| match it {
                PlanItem::Annotation { id: i, item } if *i == id => Some(item.shape.clone()),
                _ => None,
            });
            let (Some(shape), Some(view)) = (shape, view) else {
                return Some(None);
            };
            let model = doc.model().clone();
            doc.edit_merged(id, "Move dimension line", |e| {
                let ElementKind::Dimension(dim) = &mut e.kind else {
                    return;
                };
                match shape {
                    Shape::Angle { vertex, .. } => dim.offset = p.distance(vertex),
                    Shape::Radius { center, at, .. } => {
                        let r = at.distance(center);
                        let dir = (p - center).normalize_or(DVec2::X);
                        let on = attach(&model, view, dim.a.host, center + dir * r);
                        dim.a = on;
                        dim.b = on;
                        dim.offset = (p.distance(center) - r).max(0.0);
                    }
                    Shape::ArcLength { center, a, .. } => {
                        dim.offset = p.distance(center) - a.distance(center)
                    }
                    // a spot level's text moves, staying with what it is on
                    Shape::Spot { .. } => dim.b = attach(&model, view, dim.a.host, p),
                    Shape::Note { .. } | Shape::Level { .. } => {}
                }
            })
        }
        Grab::Elbow(id) => doc.edit_merged(id, "Move leader elbow", |e| {
            if let ElementKind::Note(n) = &mut e.kind {
                n.elbow = Some(p);
            }
        }),
        Grab::TagEnd(id, centre) => doc.edit_merged(id, "Move leader", |e| {
            if let ElementKind::Tag(t) = &mut e.kind {
                t.end = Some(p - centre);
            }
        }),
        Grab::TagElbow(id, centre) => doc.edit_merged(id, "Move leader elbow", |e| {
            if let ElementKind::Tag(t) = &mut e.kind {
                t.elbow = Some(p - centre);
            }
        }),
        Grab::LevelHead(level, end) => {
            let Some(view_id) = view_id else {
                return Some(None);
            };
            doc.edit_merged(view_id, "Move level head", |e| {
                if let ElementKind::View(v) = &mut e.kind {
                    v.level_heads.entry(level).or_default().offset = Some(p - end);
                }
            })
        }
    };
    Some(r.err().map(|e| e.to_string()))
}

/// The grips drawn on selected notes' and tags' leaders (their ends and
/// elbows) and on selected levels' heads.
pub fn draw_grips(
    painter: &egui::Painter,
    xf: &Xf,
    model: &Model,
    items: &[PlanItem],
    selection: &Selection,
    mm: f64,
) {
    for it in items {
        if !selection.contains(it.id()) {
            continue;
        }
        for (p, g) in grips(model, it, mm) {
            let elbow = matches!(g, Grab::Elbow(_) | Grab::TagElbow(..));
            let r = egui::Rect::from_center_size(xf.to_screen(p), Vec2::splat(7.0));
            if elbow {
                painter.circle(r.center(), 3.5, Color32::WHITE, Stroke::new(1.5, ACCENT));
            } else {
                painter.rect(
                    r,
                    0.0,
                    Color32::WHITE,
                    Stroke::new(1.5, ACCENT),
                    egui::StrokeKind::Middle,
                );
            }
        }
    }
}

// ---- sheets -----------------------------------------------------------------

/// Notes on a sheet itself: placing them with the Note tool, picking and
/// dragging them. `xf` maps sheet millimetres to the screen. Returns true
/// when it took the pointer.
#[allow(clippy::too_many_arguments)]
pub fn sheet_input(
    doc: &mut Document,
    sheet: ElementId,
    painter: &egui::Painter,
    response: &egui::Response,
    tool: Tool,
    d: &mut Draft,
    selection: &mut Selection,
    xf: &Xf,
    status: &mut Option<String>,
    error: &mut Option<String>,
) -> bool {
    let items = creabim_geom::annotate::sheet_notes(doc.model(), sheet);
    let to_paper = |p: Pos2| xf.to_world(p);
    let zoom = xf.scale;
    for it in &items {
        if let PlanItem::Annotation { id, item } = it {
            // the sheet draws the note itself; a selected one gets a frame
            let pts = item.extent();
            if let (true, Some(first)) = (selection.contains(*id), pts.first()) {
                let (lo, hi) = pts
                    .iter()
                    .fold((*first, *first), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
                painter.rect_stroke(
                    egui::Rect::from_two_pos(xf.to_screen(lo), xf.to_screen(hi)).expand(3.0),
                    2.0,
                    Stroke::new(1.5, ACCENT),
                    egui::StrokeKind::Outside,
                );
            }
        }
    }
    draw_grips(painter, xf, doc.model(), &items, selection, 1.0);
    let pointer = response.hover_pos().or(response.interact_pointer_pos());
    if tool == Tool::Note {
        let raw = pointer.map(to_paper);
        preview(painter, xf, tool, d, raw, raw, &[], 0.0, 1.0);
        if response.clicked() {
            if let Some(p) = raw {
                let mut ghost = Selection::default();
                let r = sheet_note_click(doc, sheet, p, d, &mut ghost);
                match r {
                    Ok(msg) => *status = Some(msg),
                    Err(e) => *error = Some(e.to_string()),
                }
                if let Some(id) = ghost.active {
                    selection.only(id);
                }
            }
        }
        return true;
    }
    if tool != Tool::Select {
        return false;
    }
    if let Some(r) = drag(
        doc, None, None, &items, xf, response, d, selection, 1.0, None,
    ) {
        if let Some(e) = r {
            *error = Some(e);
        }
        return true;
    }
    if response.clicked() {
        if let Some(p) = pointer.map(to_paper) {
            let hit = items.iter().rev().find_map(|it| match it {
                PlanItem::Annotation { id, item } if item.hit(p, 6.0 / zoom as f64, 1.0) => {
                    Some(*id)
                }
                _ => None,
            });
            if let Some(id) = hit {
                selection.only(id);
                return true;
            }
        }
    }
    false
}

/// A click with the Note tool on a sheet: the leader's point, then the text.
fn sheet_note_click(
    doc: &mut Document,
    sheet: ElementId,
    p: DVec2,
    d: &mut Draft,
    selection: &mut Selection,
) -> Result<String, ModelError> {
    if d.leader && d.ready.is_none() {
        d.ready = Some(Ready::Leader {
            target: (HostPoint::free(p), p),
        });
        return Ok("Now click where the note's text goes.".into());
    }
    let mut n = Note::new(sheet, p, "Note");
    if let Some(Ready::Leader { target }) = d.ready.take() {
        n.leader = Some(target.0);
    }
    d.clear();
    let e = d.styled(Element::new("Note", ElementKind::Note(n)));
    selection.only(e.id);
    doc.commit(Transaction::new("Add note").insert(e))?;
    Ok(added(Tool::Note, d))
}

// ---- properties -------------------------------------------------------------

/// What a style picker did.
pub enum LookEdit {
    None,
    Changed(&'static str),
    /// "New text style…" was picked.
    NewStyle,
}

/// The styles meant for `purpose` (and those for any annotation), by name.
pub fn styles_for(model: &Model, purpose: StyleUse) -> Vec<(ElementId, String)> {
    model
        .iter()
        .filter(|e| match &e.kind {
            ElementKind::TextStyle(s) => s.purpose == purpose || s.purpose == StyleUse::Any,
            _ => false,
        })
        .map(|e| (e.id, e.name.clone()))
        .collect()
}

/// The dimensions, tags and notes that use a style.
pub fn style_user_ids(model: &Model, id: ElementId) -> Vec<ElementId> {
    model
        .iter()
        .filter(|e| style_of(&e.kind) == Some(id))
        .map(|e| e.id)
        .collect()
}

/// The text styles in the model, by name.
pub fn styles(model: &Model) -> Vec<(ElementId, String)> {
    model
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::TextStyle(_)))
        .map(|e| (e.id, e.name.clone()))
        .collect()
}

/// A text style picker and a colour of its own, for a dimension, tag or note.
pub fn look_ui(
    ui: &mut egui::Ui,
    model: &Model,
    style: &mut Option<ElementId>,
    color: &mut Option<[f32; 3]>,
) -> LookEdit {
    let all = styles(model);
    let name = style
        .and_then(|id| all.iter().find(|s| s.0 == id))
        .map_or("Default", |s| s.1.as_str())
        .to_string();
    let mut out = LookEdit::None;
    crate::app::helpui::combo(ui, "Text style")
        .selected_text(name)
        .show_ui(ui, |ui| {
            if ui.selectable_value(style, None, "Default").changed() {
                out = LookEdit::Changed("Text style");
            }
            for (id, n) in &all {
                if ui.selectable_value(style, Some(*id), n).changed() {
                    out = LookEdit::Changed("Text style");
                }
            }
            ui.separator();
            if ui.button("New text style…").clicked() {
                out = LookEdit::NewStyle;
            }
        });
    ui.horizontal(|ui| {
        let mut own = color.is_some();
        if ui
            .checkbox(&mut own, "Own colour")
            .on_hover_text("Override the style's colour for this one")
            .changed()
        {
            *color = own.then_some([0.8, 0.1, 0.1]);
            out = LookEdit::Changed("Annotation colour");
        }
        if let Some(c) = color {
            if ui.color_edit_button_rgb(c).changed() {
                out = LookEdit::Changed("Annotation colour");
            }
        }
    });
    out
}

/// Add a text style, named after how many there are. Returns its id.
pub fn new_text_style(doc: &mut Document) -> Result<ElementId, ModelError> {
    let n = styles(doc.model()).len() + 1;
    let e = Element::new(
        format!("Text style {n}"),
        ElementKind::TextStyle(TextStyle::default()),
    );
    let id = e.id;
    doc.commit(Transaction::new("New text style").insert(e))?;
    Ok(id)
}

/// Point an annotation at a text style.
pub fn set_style(kind: &mut ElementKind, id: ElementId) {
    match kind {
        ElementKind::Dimension(d) => d.style = Some(id),
        ElementKind::Tag(t) => t.style = Some(id),
        ElementKind::Note(n) => n.style = Some(id),
        _ => {}
    }
}

/// A text style's font, size, colour, weight and slant.
pub fn style_ui(ui: &mut egui::Ui, s: &mut TextStyle) -> Option<&'static str> {
    let mut label = None;
    crate::app::helpui::combo(ui, "Font")
        .selected_text(s.font.label())
        .show_ui(ui, |ui| {
            for f in Font::ALL {
                if ui.selectable_value(&mut s.font, f, f.label()).changed() {
                    label = Some("Text font");
                }
            }
        });
    ui.horizontal(|ui| {
        ui.label("Size on paper");
        if ui
            .add(
                egui::DragValue::new(&mut s.size)
                    .speed(0.05)
                    .range(0.5..=50.0)
                    .suffix(" mm"),
            )
            .changed()
        {
            label = Some("Text size");
        }
    });
    ui.horizontal(|ui| {
        let mut own = s.color.is_some();
        if ui
            .checkbox(&mut own, "Colour")
            .on_hover_text("Off: black on paper, blue on screen")
            .changed()
        {
            s.color = own.then_some([0.0, 0.0, 0.0]);
            label = Some("Text colour");
        }
        if let Some(c) = &mut s.color {
            if ui.color_edit_button_rgb(c).changed() {
                label = Some("Text colour");
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.checkbox(&mut s.bold, "Bold").changed() {
            label = Some("Bold text");
        }
        if ui.checkbox(&mut s.italic, "Italic").changed() {
            label = Some("Italic text");
        }
    });
    if let Some(l) = ends_ui(ui, s) {
        label = Some(l);
    }
    label
}

/// A terminator picker, with "Usual" for none.
fn terminator_combo(
    ui: &mut egui::Ui,
    salt: &str,
    text: &str,
    usual: &str,
    t: &mut Option<Terminator>,
) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(salt)
        .selected_text(t.map_or(usual.to_string(), |t| t.label().to_string()))
        .show_ui(ui, |ui| {
            changed |= ui.selectable_value(t, None, usual).changed();
            for k in Terminator::ALL {
                changed |= ui.selectable_value(t, Some(k), k.label()).changed();
            }
        });
    ui.label(text);
    changed
}

/// The end of one tag's or note's leader, and its elbow: picked here or
/// left to its style. Returns the undo label of a change.
pub fn leader_ui(
    ui: &mut egui::Ui,
    end: &mut Option<Terminator>,
    usual: &str,
    elbow: &mut Option<DVec2>,
) -> Option<&'static str> {
    let mut label = None;
    ui.horizontal(|ui| {
        if terminator_combo(ui, "leader_end_own", "Leader end", usual, end) {
            label = Some("Leader end");
        }
    });
    if elbow.is_some()
        && ui
            .button("Straighten the leader")
            .on_hover_text("Take its elbow out; drag the round grip on the leader to bend it again")
            .clicked()
    {
        *elbow = None;
        label = Some("Straighten leader");
    }
    label
}

/// A style's line ends, tag shape and what it is for.
pub fn ends_ui(ui: &mut egui::Ui, s: &mut TextStyle) -> Option<&'static str> {
    let mut label = None;
    ui.horizontal(|ui| {
        if terminator_combo(
            ui,
            "style_end",
            "Dimension ends",
            "Usual (tick, arrow)",
            &mut s.terminator,
        ) {
            label = Some("Dimension ends");
        }
    });
    ui.horizontal(|ui| {
        if terminator_combo(
            ui,
            "style_leader",
            "Leader end",
            "Usual (arrow)",
            &mut s.leader_end,
        ) {
            label = Some("Leader end");
        }
    });
    ui.horizontal(|ui| {
        let mut size = s.end_size.unwrap_or(creabim_geom::annotation::END_SIZE);
        ui.label("End size");
        if ui
            .add(
                egui::DragValue::new(&mut size)
                    .speed(0.05)
                    .range(0.5..=10.0)
                    .suffix(" mm"),
            )
            .changed()
        {
            s.end_size = Some(size);
            label = Some("Terminator size");
        }
    });
    crate::app::helpui::combo(ui, "Tag shape")
        .selected_text(s.tag_shape.label())
        .show_ui(ui, |ui| {
            for k in TagShape::ALL {
                if ui
                    .selectable_value(&mut s.tag_shape, k, k.label())
                    .changed()
                {
                    label = Some("Tag shape");
                }
            }
        });
    if ui
        .checkbox(&mut s.break_at_tags, "Break dimension lines at tags")
        .on_hover_text(
            "Dimension lines in this style leave a gap where a tag or a note sits across them",
        )
        .changed()
    {
        label = Some("Dimension breaks");
    }
    crate::app::helpui::combo(ui, "Meant for")
        .selected_text(s.purpose.label())
        .show_ui(ui, |ui| {
            for k in StyleUse::ALL {
                if ui.selectable_value(&mut s.purpose, k, k.label()).changed() {
                    label = Some("Style use");
                }
            }
        });
    label
}

/// The text style a dimension, tag or note refers to.
pub fn style_of(kind: &ElementKind) -> Option<ElementId> {
    match kind {
        ElementKind::Dimension(d) => d.style,
        ElementKind::Tag(t) => t.style,
        ElementKind::Note(n) => n.style,
        _ => None,
    }
}

/// The style an annotation uses, editable in place under its picker, so a
/// change shows at once on everything using it. `open` unfolds it (for a
/// style just made). Returns a status message after an edit.
pub fn style_inline(
    ui: &mut egui::Ui,
    doc: &mut Document,
    kind: &ElementKind,
    open: bool,
) -> Option<String> {
    let id = style_of(kind)?;
    let e = doc.model().get(id)?;
    let ElementKind::TextStyle(mut s) = e.kind else {
        return None;
    };
    let users = style_users(doc.model(), id);
    let mut label = None;
    egui::CollapsingHeader::new(format!("Edit \u{201c}{}\u{201d}", e.name))
        .id_salt(("style", id))
        .open(open.then_some(true))
        .show(ui, |ui| {
            label = style_ui(ui, &mut s);
            ui.label(
                egui::RichText::new(format!("Changes every one of its {users} users.")).weak(),
            );
        });
    let label = label?;
    let r = doc.edit_merged(id, label, |e| e.kind = ElementKind::TextStyle(s));
    Some(match r {
        Ok(()) => format!("{label}."),
        Err(e) => format!("Couldn't do that: {e}"),
    })
}

/// How many dimensions, tags and notes use a style.
pub fn style_users(model: &Model, id: ElementId) -> usize {
    model
        .iter()
        .filter(|e| match &e.kind {
            ElementKind::Dimension(d) => d.style == Some(id),
            ElementKind::Tag(t) => t.style == Some(id),
            ElementKind::Note(n) => n.style == Some(id),
            _ => false,
        })
        .count()
}

/// What a dimension's offset means, for its kind.
pub fn offset_label(kind: DimensionKind) -> &'static str {
    match kind {
        DimensionKind::Linear => "Line offset",
        DimensionKind::Angular => "Arc radius",
        DimensionKind::Radius | DimensionKind::Diameter => "Line past circle",
        DimensionKind::ArcLength => "Line offset",
        DimensionKind::Spot => "Offset",
    }
}

/// A note's text and leader. `unit` is a drawing unit's worth of a
/// leader's default reach (larger on sheets, which are in mm).
pub fn note_ui(
    ui: &mut egui::Ui,
    n: &mut Note,
    focus: &mut bool,
    unit: f64,
) -> Option<&'static str> {
    let mut label = None;
    ui.label("Text");
    let r = ui.add(
        egui::TextEdit::multiline(&mut n.text)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    if std::mem::take(focus) {
        r.request_focus();
        // select it all, so typing replaces the placeholder
        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), r.id) {
            let all = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(n.text.chars().count()),
            );
            state.cursor.set_char_range(Some(all));
            state.store(ui.ctx(), r.id);
        }
    }
    if r.changed() {
        label = Some("Note text");
    }
    let mut leader = n.leader.is_some();
    if ui
        .checkbox(&mut leader, "Leader")
        .on_hover_text("An arrow from the note to what it is about. Drag its square to move it.")
        .changed()
    {
        n.leader = leader.then(|| HostPoint::free(n.at + DVec2::new(-1.5, -1.0) * unit));
        label = Some("Note leader");
    }
    if n.leader.is_some_and(|l| l.host.is_some()) {
        ui.label(
            egui::RichText::new("Its arrow is attached and follows what it points at.").weak(),
        );
    }
    label
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::ViewKind;
    use creabim_geom::plan::Pen;

    fn plan_of(doc: &Document) -> (ElementId, View) {
        doc.model()
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::View(v) if matches!(v.kind, ViewKind::Plan { .. }) => {
                    Some((e.id, v.clone()))
                }
                _ => None,
            })
            .unwrap()
    }

    /// The one element added since the model had `before`.
    fn added_since(doc: &Document, before: &[ElementId]) -> Element {
        let new: Vec<&Element> = doc
            .model()
            .iter()
            .filter(|e| !before.contains(&e.id))
            .collect();
        assert_eq!(new.len(), 1);
        new[0].clone()
    }

    fn ids(doc: &Document) -> Vec<ElementId> {
        doc.model().iter().map(|e| e.id).collect()
    }

    #[test]
    fn two_lines_make_an_angle_on_the_clicked_side() {
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let (vid, view) = plan_of(&doc);
        let line = |a: DVec2, b: DVec2| PlanItem::Line {
            id: ElementId::NIL,
            pen: Pen::Projection,
            a,
            b,
        };
        let items = [
            line(DVec2::new(0.0, 0.0), DVec2::new(4.0, 0.0)),
            line(DVec2::new(1.0, 1.0), DVec2::new(1.0, 5.0)),
        ];
        let (mut d, mut sel) = (Draft::default(), Selection::default());
        let before = ids(&doc);
        for at in [
            DVec2::new(3.0, 0.02),
            DVec2::new(1.03, 3.0),
            DVec2::new(2.0, 1.5),
        ] {
            click(
                &mut doc,
                vid,
                &view,
                Tool::Angle,
                None,
                at,
                &items,
                0.1,
                &mut d,
                &mut sel,
            )
            .unwrap();
        }
        let e = added_since(&doc, &before);
        let ElementKind::Dimension(dim) = &e.kind else {
            panic!("not a dimension")
        };
        assert_eq!(dim.kind, DimensionKind::Angular);
        assert!((dim.offset - DVec2::new(1.0, 1.5).length()).abs() < 1e-9);
        let drawing = creabim_geom::section::view_drawing(doc.model(), vid).unwrap();
        let text = drawing.items.iter().find_map(|i| match i {
            PlanItem::Annotation { id, item } if *id == e.id => Some(item.text.clone()),
            _ => None,
        });
        assert_eq!(text.as_deref(), Some("90°"));
        // undo takes it away
        doc.undo();
        assert_eq!(ids(&doc), before);
    }

    #[test]
    fn an_arc_gives_its_radius_and_a_note_its_leader() {
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let (vid, view) = plan_of(&doc);
        let items = [PlanItem::Arc {
            id: ElementId::NIL,
            pen: Pen::Symbol,
            center: DVec2::new(5.0, 5.0),
            radius: 2.0,
            start: 0.0,
            end: std::f64::consts::PI,
        }];
        let (mut d, mut sel) = (Draft::default(), Selection::default());
        let before = ids(&doc);
        let mut go = |doc: &mut Document, tool: Tool, at: DVec2, items: &[PlanItem]| {
            click(
                doc, vid, &view, tool, None, at, items, 0.1, &mut d, &mut sel,
            )
            .unwrap();
        };
        go(&mut doc, Tool::Radius, DVec2::new(5.0, 7.03), &items);
        go(&mut doc, Tool::Radius, DVec2::new(8.0, 5.0), &items);
        let e = added_since(&doc, &before);
        let ElementKind::Dimension(dim) = &e.kind else {
            panic!("not a dimension")
        };
        assert_eq!(dim.kind, DimensionKind::Radius);
        assert!(dim.a.local.distance(DVec2::new(7.0, 5.0)) < 1e-9);
        assert!((dim.offset - 1.0).abs() < 1e-9);
        doc.undo();

        // a note: its arrow's point, then its text
        go(&mut doc, Tool::Note, DVec2::new(1.0, 1.0), &[]);
        go(&mut doc, Tool::Note, DVec2::new(3.0, 2.0), &[]);
        let e = added_since(&doc, &before);
        let ElementKind::Note(note) = &e.kind else {
            panic!("not a note")
        };
        assert_eq!(note.at, DVec2::new(3.0, 2.0));
        assert_eq!(note.leader.unwrap().local, DVec2::new(1.0, 1.0));
        assert!(d.focus_text);
        assert_eq!(sel.active, Some(e.id));
    }
}
