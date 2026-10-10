//! The 2D plan view: draws the generated plan of a level and hosts the
//! drawing tools. Edits made here are ordinary model edits, so the 3D view
//! and any other open view update at once.

use creabim_core::{
    Document, EditMesh, Element, ElementId, ElementKind, FreeMesh, Opening, OpeningKind, Slab,
    Transaction, Wall,
};
use creabim_geom::{plan::Pen, plan_view, PlanItem};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use glam::{DVec2, DVec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Wall,
    Door,
    Window,
    Slab,
    Mass,
}

impl Tool {
    pub const ALL: [Tool; 6] = [
        Tool::Select,
        Tool::Wall,
        Tool::Door,
        Tool::Window,
        Tool::Slab,
        Tool::Mass,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Wall => "Wall",
            Tool::Door => "Door",
            Tool::Window => "Window",
            Tool::Slab => "Slab",
            Tool::Mass => "Mass",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => "Click an element to select it.",
            Tool::Wall => "Click to start, click again to place each wall. Esc or right-click ends the chain. Shift keeps it orthogonal.",
            Tool::Door => "Click on a wall to place a door.",
            Tool::Window => "Click on a wall to place a window.",
            Tool::Slab => "Click two opposite corners.",
            Tool::Mass => "Click to place a freeform mass you can then edit like a mesh.",
        }
    }
}

pub struct PlanState {
    pub center: DVec2,
    /// Pixels per metre.
    pub scale: f32,
    pub tool: Tool,
    /// First click of a two-click tool (wall start, slab corner).
    pub pending: Option<DVec2>,
    pub cut_height: f64,
    pub wall_thickness: f64,
}

impl Default for PlanState {
    fn default() -> Self {
        Self {
            center: DVec2::new(5.0, 3.0),
            scale: 45.0,
            tool: Tool::Select,
            pending: None,
            cut_height: 1.2,
            wall_thickness: 0.2,
        }
    }
}

struct Xf {
    rect: Rect,
    center: DVec2,
    scale: f32,
}

impl Xf {
    fn to_screen(&self, p: DVec2) -> Pos2 {
        let d = (p - self.center).as_vec2() * self.scale;
        self.rect.center() + Vec2::new(d.x, -d.y)
    }

    fn to_world(&self, p: Pos2) -> DVec2 {
        let d = (p - self.rect.center()) / self.scale;
        self.center + DVec2::new(d.x as f64, -d.y as f64)
    }
}

/// Show the plan of `level` in `rect`. Returns a status message when an edit fails.
pub fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    doc: &mut Document,
    level: ElementId,
    state: &mut PlanState,
    selection: &mut Option<ElementId>,
    background: Color32,
) -> Option<String> {
    let response = ui.interact(rect, ui.id().with("plan"), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, background);

    // pan and zoom
    if response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle)
    {
        let d = response.drag_delta() / state.scale;
        state.center -= DVec2::new(d.x as f64, -d.y as f64);
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            if let Some(hover) = response.hover_pos() {
                let xf = Xf {
                    rect,
                    center: state.center,
                    scale: state.scale,
                };
                let before = xf.to_world(hover);
                state.scale = (state.scale * (1.0 + scroll * 0.002)).clamp(2.0, 2000.0);
                let xf = Xf {
                    rect,
                    center: state.center,
                    scale: state.scale,
                };
                state.center += before - xf.to_world(hover);
            }
        }
    }
    let xf = Xf {
        rect,
        center: state.center,
        scale: state.scale,
    };

    let plan = plan_view(doc.model(), level, state.cut_height);
    draw_metric_grid(&painter, &xf);
    for item in &plan.items {
        draw_item(&painter, &xf, item, *selection == Some(item.id()));
    }

    let shift = ui.input(|i| i.modifiers.shift);
    let hover_world = response
        .hover_pos()
        .map(|p| snap(doc, level, &xf, xf.to_world(p), state.pending, shift));

    // preview of the tool in progress
    if let (Some(a), Some(b)) = (state.pending, hover_world) {
        let accent = Stroke::new(1.5, Color32::from_rgb(230, 120, 30));
        match state.tool {
            Tool::Wall => {
                painter.line_segment([xf.to_screen(a), xf.to_screen(b)], accent);
                let len = a.distance(b);
                painter.text(
                    xf.to_screen((a + b) / 2.0) + Vec2::new(0.0, -12.0),
                    egui::Align2::CENTER_BOTTOM,
                    format!("{len:.2} m"),
                    egui::FontId::proportional(13.0),
                    accent.color,
                );
            }
            Tool::Slab => {
                painter.rect_stroke(
                    Rect::from_two_pos(xf.to_screen(a), xf.to_screen(b)),
                    0.0,
                    accent,
                    egui::StrokeKind::Middle,
                );
            }
            _ => {}
        }
    }
    if let Some(p) = hover_world {
        if state.tool != Tool::Select {
            painter.circle_stroke(
                xf.to_screen(p),
                4.0,
                Stroke::new(1.0, Color32::from_rgb(230, 120, 30)),
            );
        }
    }

    if ui.input(|i| i.key_pressed(egui::Key::Escape)) || response.secondary_clicked() {
        state.pending = None;
    }

    let mut error = None;
    if response.clicked() {
        let p = hover_world?;
        let result = match state.tool {
            Tool::Select => {
                *selection = hit_test(&plan.items, p, 6.0 / state.scale as f64);
                Ok(())
            }
            Tool::Wall => match state.pending {
                None => {
                    state.pending = Some(p);
                    Ok(())
                }
                Some(a) if a.distance(p) > 0.05 => {
                    let height = doc.model().level(level).map_or(3.0, |l| l.height);
                    let e = Element::new(
                        "Wall",
                        ElementKind::Wall(Wall {
                            level,
                            start: a,
                            end: p,
                            thickness: state.wall_thickness,
                            height,
                            base_offset: 0.0,
                        }),
                    );
                    *selection = Some(e.id);
                    state.pending = Some(p);
                    doc.commit(Transaction::new("Draw wall").insert(e))
                }
                Some(_) => Ok(()),
            },
            Tool::Door | Tool::Window => place_opening(doc, level, p, state.tool, selection),
            Tool::Slab => match state.pending.take() {
                None => {
                    state.pending = Some(p);
                    Ok(())
                }
                Some(a) => {
                    let (lo, hi) = (a.min(p), a.max(p));
                    let e = Element::new(
                        "Slab",
                        ElementKind::Slab(Slab {
                            level,
                            boundary: vec![lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)],
                            thickness: 0.25,
                        }),
                    );
                    *selection = Some(e.id);
                    doc.commit(Transaction::new("Draw slab").insert(e))
                }
            },
            Tool::Mass => {
                let e = Element::new(
                    "Mass",
                    ElementKind::FreeMesh(FreeMesh {
                        level: Some(level),
                        origin: DVec3::new(p.x, p.y, 0.0),
                        mesh: EditMesh::cuboid(DVec3::new(2.0, 2.0, 3.0)),
                        ifc_class: "IfcBuildingElementProxy".into(),
                    }),
                );
                *selection = Some(e.id);
                doc.commit(Transaction::new("Add mass").insert(e))
            }
        };
        if let Err(e) = result {
            error = Some(e.to_string());
        }
    }

    // scale bar and tool hint
    painter.text(
        rect.left_bottom() + Vec2::new(8.0, -8.0),
        egui::Align2::LEFT_BOTTOM,
        format!(
            "{}   ·   cut at {:.2} m",
            state.tool.hint(),
            state.cut_height
        ),
        egui::FontId::proportional(12.0),
        Color32::from_gray(90),
    );
    error
}

fn draw_metric_grid(painter: &egui::Painter, xf: &Xf) {
    let step = if xf.scale > 30.0 {
        1.0
    } else if xf.scale > 6.0 {
        5.0
    } else {
        25.0
    };
    let lo = xf.to_world(xf.rect.left_bottom());
    let hi = xf.to_world(xf.rect.right_top());
    let stroke = Stroke::new(1.0, Color32::from_gray(236));
    let mut x = (lo.x / step).floor() * step;
    while x <= hi.x {
        painter.line_segment(
            [
                xf.to_screen(DVec2::new(x, lo.y)),
                xf.to_screen(DVec2::new(x, hi.y)),
            ],
            stroke,
        );
        x += step;
    }
    let mut y = (lo.y / step).floor() * step;
    while y <= hi.y {
        painter.line_segment(
            [
                xf.to_screen(DVec2::new(lo.x, y)),
                xf.to_screen(DVec2::new(hi.x, y)),
            ],
            stroke,
        );
        y += step;
    }
}

fn draw_item(painter: &egui::Painter, xf: &Xf, item: &PlanItem, selected: bool) {
    let highlight = Color32::from_rgb(230, 120, 30);
    let ink = if selected { highlight } else { Color32::BLACK };
    match item {
        PlanItem::Polygon { pen, points, .. } => {
            let pts: Vec<Pos2> = points.iter().map(|p| xf.to_screen(*p)).collect();
            match pen {
                Pen::Cut => {
                    let fill = if selected {
                        Color32::from_rgb(250, 205, 160)
                    } else {
                        Color32::from_gray(150)
                    };
                    painter.add(egui::Shape::convex_polygon(
                        pts,
                        fill,
                        Stroke::new(1.6, ink),
                    ));
                }
                _ => {
                    let (w, c) = if selected {
                        (2.0, highlight)
                    } else {
                        (1.0, Color32::from_gray(120))
                    };
                    painter.add(egui::Shape::closed_line(pts, Stroke::new(w, c)));
                }
            }
        }
        PlanItem::Line { pen, a, b, .. } => {
            let (a, b) = (xf.to_screen(*a), xf.to_screen(*b));
            match pen {
                Pen::Grid => {
                    painter.extend(egui::Shape::dashed_line(
                        &[a, b],
                        Stroke::new(1.0, Color32::from_rgb(120, 140, 170)),
                        12.0,
                        4.0,
                    ));
                }
                Pen::Projection => {
                    let (w, c) = if selected {
                        (2.0, highlight)
                    } else {
                        (1.0, Color32::from_gray(120))
                    };
                    painter.line_segment([a, b], Stroke::new(w, c));
                }
                _ => {
                    painter.line_segment([a, b], Stroke::new(1.0, ink));
                }
            }
        }
        PlanItem::Arc {
            center,
            radius,
            start,
            end,
            ..
        } => {
            let n = 24;
            let pts: Vec<Pos2> = (0..=n)
                .map(|i| {
                    let t = start + (end - start) * i as f64 / n as f64;
                    xf.to_screen(*center + DVec2::from_angle(t) * *radius)
                })
                .collect();
            painter.add(egui::Shape::line(pts, Stroke::new(1.0, ink)));
        }
        PlanItem::Label { at, text, .. } => {
            let p = xf.to_screen(*at);
            let blue = Color32::from_rgb(120, 140, 170);
            painter.circle(p, 10.0, Color32::WHITE, Stroke::new(1.0, blue));
            painter.text(
                p,
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(12.0),
                blue,
            );
        }
    }
}

/// Snap to wall endpoints within 10 px, else to a 5 cm grid. With `ortho`,
/// keep the segment from `from` horizontal or vertical.
fn snap(
    doc: &Document,
    level: ElementId,
    xf: &Xf,
    p: DVec2,
    from: Option<DVec2>,
    ortho: bool,
) -> DVec2 {
    let mut p = p;
    if let (true, Some(a)) = (ortho, from) {
        let d = p - a;
        p = if d.x.abs() >= d.y.abs() {
            DVec2::new(p.x, a.y)
        } else {
            DVec2::new(a.x, p.y)
        };
    }
    let tol = 10.0 / xf.scale as f64;
    let mut best: Option<(f64, DVec2)> = None;
    for e in doc.model().iter() {
        if let ElementKind::Wall(w) = &e.kind {
            if w.level != level {
                continue;
            }
            for q in [w.start, w.end] {
                let d = q.distance(p);
                if d < tol && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, q));
                }
            }
        }
    }
    best.map(|(_, q)| q)
        .unwrap_or_else(|| (p / 0.05).round() * 0.05)
}

fn hit_test(items: &[PlanItem], p: DVec2, tol: f64) -> Option<ElementId> {
    // symbols (doors, windows) win over the walls they sit in
    let near_line = |a: DVec2, b: DVec2| segment_distance(p, a, b) < tol;
    for it in items.iter().rev() {
        match it {
            PlanItem::Line {
                id,
                pen: Pen::Symbol,
                a,
                b,
            } if near_line(*a, *b) => return Some(*id),
            PlanItem::Arc {
                id, center, radius, ..
            } if (p.distance(*center) - radius).abs() < tol => return Some(*id),
            _ => {}
        }
    }
    for it in items.iter().rev() {
        match it {
            PlanItem::Polygon {
                id,
                pen: Pen::Cut,
                points,
            } if point_in_polygon(p, points) => return Some(*id),
            _ => {}
        }
    }
    for it in items.iter().rev() {
        match it {
            PlanItem::Line { id, a, b, .. } if near_line(*a, *b) => return Some(*id),
            PlanItem::Polygon { id, points, .. } => {
                let n = points.len();
                if (0..n).any(|i| near_line(points[i], points[(i + 1) % n])) {
                    return Some(*id);
                }
            }
            _ => {}
        }
    }
    None
}

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

fn point_in_polygon(p: DVec2, poly: &[DVec2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

fn place_opening(
    doc: &mut Document,
    level: ElementId,
    p: DVec2,
    tool: Tool,
    selection: &mut Option<ElementId>,
) -> Result<(), creabim_core::ModelError> {
    let mut best: Option<(f64, ElementId, f64, f64)> = None;
    for e in doc.model().iter() {
        let ElementKind::Wall(w) = &e.kind else {
            continue;
        };
        if w.level != level {
            continue;
        }
        let d = segment_distance(p, w.start, w.end);
        if d <= w.thickness.max(0.3) && best.is_none_or(|b| d < b.0) {
            let u = (p - w.start).dot(w.direction());
            best = Some((d, e.id, u, w.length()));
        }
    }
    let Some((_, host, u, len)) = best else {
        return Ok(());
    };
    let (kind, width, height, sill, name) = match tool {
        Tool::Door => (OpeningKind::Door, 0.9, 2.1, 0.0, "Door"),
        _ => (OpeningKind::Window, 1.2, 1.3, 0.9, "Window"),
    };
    if width > len {
        return Ok(());
    }
    let e = Element::new(
        name,
        ElementKind::Opening(Opening {
            host,
            kind,
            offset: (u - width / 2.0).clamp(0.0, len - width),
            sill,
            width,
            height,
        }),
    );
    *selection = Some(e.id);
    doc.commit(Transaction::new(format!("Place {}", name.to_lowercase())).insert(e))
}
