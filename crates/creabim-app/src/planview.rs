//! The 2D plan view: draws the generated plan of a level and hosts the
//! drawing tools. Edits made here are ordinary model edits, so the 3D view
//! and any other open view update at once.

use creabim_core::{
    Document, EditMesh, Element, ElementId, ElementKind, FreeMesh, Opening, OpeningKind, Slab,
    Transaction, View, ViewKind, Wall,
};
use creabim_geom::{paper, plan::Pen, section::Drawing, triangulate::triangulate, PlanItem};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use glam::{DVec2, DVec3};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Wall,
    Door,
    Window,
    Slab,
    Mass,
    Section,
    Elevation,
}

impl Tool {
    pub const ALL: [Tool; 8] = [
        Tool::Select,
        Tool::Wall,
        Tool::Door,
        Tool::Window,
        Tool::Slab,
        Tool::Mass,
        Tool::Section,
        Tool::Elevation,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Wall => "Wall",
            Tool::Door => "Door",
            Tool::Window => "Window",
            Tool::Slab => "Slab",
            Tool::Mass => "Mass",
            Tool::Section => "Section",
            Tool::Elevation => "Elevation",
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
            Tool::Section => "Click two points for the section line. It looks to the left of the direction you draw.",
            Tool::Elevation => "Click two points for the elevation line, outside the building. It looks to the left of the direction you draw.",
        }
    }
}

/// What kind of drawing the 2D pane is showing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PaneKind {
    /// A plan of a level: every drawing tool works here.
    Plan { level: ElementId },
    /// A section or elevation along `start` → `end`. Walls, doors, windows
    /// and masses can be drawn here too; drawing x runs along the line and
    /// y is height.
    Section { start: DVec2, end: DVec2 },
    /// A parallel 3D view: drag to orbit, click to select.
    Axonometric,
}

impl PaneKind {
    /// World plan position of a section drawing point on the section plane.
    fn section_point(start: DVec2, end: DVec2, p: DVec2) -> DVec2 {
        start + (end - start).normalize_or_zero() * p.x
    }
}

/// How a pane is shown: on its own, or inside a viewport on a sheet.
#[derive(Clone, Copy, Debug)]
pub struct PaneStyle {
    /// Background fill; `None` leaves what is behind (the sheet) visible.
    pub background: Option<Color32>,
    /// Metric grid and tool hint.
    pub decorations: bool,
    /// Fixed pan and zoom (centre, pixels per metre) instead of the view's own.
    pub camera: Option<(DVec2, f32)>,
    /// Drawing scale denominator, for hatch spacing.
    pub scale: f64,
}

/// Which part of a selected section line is being dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Handle {
    Start,
    End,
    Line,
}

pub struct PlanState {
    /// Pan and zoom per view: centre in drawing units, pixels per metre.
    pub cameras: HashMap<ElementId, (DVec2, f32)>,
    pub tool: Tool,
    /// First click of a two-click tool (wall start, slab corner).
    pub pending: Option<DVec2>,
    pub wall_thickness: f64,
    /// Section line handle being dragged, and the line when the drag began.
    drag: Option<(Handle, DVec2, DVec2, DVec2)>,
}

impl Default for PlanState {
    fn default() -> Self {
        Self {
            cameras: HashMap::new(),
            tool: Tool::Select,
            pending: None,
            wall_thickness: 0.2,
            drag: None,
        }
    }
}

pub struct Xf {
    pub rect: Rect,
    pub center: DVec2,
    pub scale: f32,
}

impl Xf {
    pub fn to_screen(&self, p: DVec2) -> Pos2 {
        let d = (p - self.center).as_vec2() * self.scale;
        self.rect.center() + Vec2::new(d.x, -d.y)
    }

    pub fn to_world(&self, p: Pos2) -> DVec2 {
        let d = (p - self.rect.center()) / self.scale;
        self.center + DVec2::new(d.x as f64, -d.y as f64)
    }
}

/// Show a view's drawing in `rect`. Returns a status message when an edit fails.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    doc: &mut Document,
    view: ElementId,
    kind: PaneKind,
    plan: &Drawing,
    state: &mut PlanState,
    selection: &mut Option<ElementId>,
    style: PaneStyle,
) -> Option<String> {
    let response = ui.interact(rect, ui.id().with(("view", view)), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    if let Some(bg) = style.background {
        painter.rect_filled(rect, 0.0, bg);
    }

    let (mut center, mut scale) = match style.camera {
        Some(c) => c,
        None => *state.cameras.entry(view).or_insert_with(|| fit(plan, rect)),
    };
    let tool = match (kind, state.tool) {
        (PaneKind::Section { .. }, Tool::Slab | Tool::Section | Tool::Elevation) => Tool::Select,
        (PaneKind::Axonometric, _) => Tool::Select,
        (_, t) => t,
    };

    // orbit a 3D view by dragging it
    if kind == PaneKind::Axonometric && response.dragged_by(egui::PointerButton::Primary) {
        let d = response.drag_delta();
        let r = doc.edit_merged(view, "Orbit 3D view", |e| {
            if let ElementKind::View(View {
                kind: ViewKind::Axonometric { azimuth, altitude },
                ..
            }) = &mut e.kind
            {
                *azimuth = (*azimuth - d.x as f64 * 0.4).rem_euclid(360.0);
                *altitude = (*altitude + d.y as f64 * 0.4).clamp(-89.0, 89.0);
            }
        });
        if let Err(e) = r {
            return Some(e.to_string());
        }
    }

    // pan and zoom
    if response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle)
    {
        let d = response.drag_delta() / scale;
        center -= DVec2::new(d.x as f64, -d.y as f64);
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            if let Some(hover) = response.hover_pos() {
                let xf = Xf {
                    rect,
                    center,
                    scale,
                };
                let before = xf.to_world(hover);
                scale = (scale * (1.0 + scroll * 0.002)).clamp(2.0, 2000.0);
                let xf = Xf {
                    rect,
                    center,
                    scale,
                };
                center += before - xf.to_world(hover);
            }
        }
    }
    if style.camera.is_none() {
        state.cameras.insert(view, (center, scale));
    }
    let xf = Xf {
        rect,
        center,
        scale,
    };

    if style.decorations {
        draw_metric_grid(&painter, &xf);
    }
    let hatch = paper::HATCH_SPACING * style.scale / 1000.0;
    for item in &plan.items {
        draw_item(&painter, &xf, item, *selection == Some(item.id()), hatch);
    }

    let shift = ui.input(|i| i.modifiers.shift);
    let hover_world = response.hover_pos().map(|p| match kind {
        PaneKind::Plan { level } => snap(doc, level, &xf, xf.to_world(p), state.pending, shift),
        PaneKind::Axonometric => xf.to_world(p),
        PaneKind::Section { .. } => {
            let mut p = xf.to_world(p);
            if let (true, Some(a)) = (shift, state.pending) {
                let d = p - a;
                p = if d.x.abs() >= d.y.abs() {
                    DVec2::new(p.x, a.y)
                } else {
                    DVec2::new(a.x, p.y)
                };
            }
            (p / 0.05).round() * 0.05
        }
    });

    // a selected section line in a plan can be dragged by its ends or middle
    if let (PaneKind::Plan { .. }, Tool::Select) = (kind, tool) {
        if let Some(r) = drag_section_line(doc, &painter, &xf, &response, state, *selection) {
            return r;
        }
    }

    // preview of the tool in progress
    if let (Some(a), Some(b)) = (state.pending, hover_world) {
        let accent = Stroke::new(1.5, Color32::from_rgb(230, 120, 30));
        match tool {
            Tool::Wall | Tool::Section | Tool::Elevation => {
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
        if tool != Tool::Select {
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
        let raw = xf.to_world(response.interact_pointer_pos().unwrap_or_default());
        // linework first (edges, symbols); then whatever the view ray hits
        let picked = || {
            hit_test(&plan.items, raw, 6.0 / scale as f64)
                .or_else(|| ray_pick(doc, view, kind, raw))
        };
        let result = match (kind, tool) {
            (_, Tool::Select) => {
                *selection = picked();
                Ok(())
            }
            (PaneKind::Section { start, end }, _) => {
                section_tool(doc, start, end, tool, p, picked(), state, selection)
            }
            (PaneKind::Plan { level }, tool) => plan_tool(doc, level, tool, p, state, selection),
            (PaneKind::Axonometric, _) => Ok(()),
        };
        if let Err(e) = result {
            error = Some(e.to_string());
        }
    }

    if style.decorations {
        painter.text(
            rect.left_bottom() + Vec2::new(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            match (kind, tool) {
                (PaneKind::Plan { .. }, t) => t.hint(),
                (PaneKind::Section { .. }, Tool::Select) => {
                    "Click to select. Wall, Door, Window and Mass also work here; Slab and Section tools work in plans."
                }
                (PaneKind::Section { .. }, Tool::Wall) => {
                    "Click two points: the wall runs along the section line, as high as you draw it."
                }
                (PaneKind::Section { .. }, t) => t.hint(),
                (PaneKind::Axonometric, _) => {
                    "Drag to orbit, click to select. Right-drag pans, the wheel zooms."
                }
            },
            egui::FontId::proportional(12.0),
            Color32::from_gray(90),
        );
    }
    error
}

/// A click with a drawing tool in a plan.
fn plan_tool(
    doc: &mut Document,
    level: ElementId,
    tool: Tool,
    p: DVec2,
    state: &mut PlanState,
    selection: &mut Option<ElementId>,
) -> Result<(), creabim_core::ModelError> {
    match tool {
        Tool::Select => Ok(()),
        Tool::Section | Tool::Elevation => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) if a.distance(p) > 0.2 => {
                let cut = tool == Tool::Section;
                let n = doc
                        .model()
                        .iter()
                        .filter(|e| matches!(&e.kind, ElementKind::View(v) if matches!(v.kind, ViewKind::Section { cut: c, .. } if c == cut)))
                        .count();
                let name = if cut {
                    format!("Section {}", section_letter(n))
                } else {
                    format!("Elevation {}", n + 1)
                };
                let e = Element::new(
                    name.clone(),
                    ElementKind::View(View::section(a, p, 30.0, cut)),
                );
                *selection = Some(e.id);
                doc.commit(Transaction::new(format!("Add {name}")).insert(e))
            }
            Some(_) => Ok(()),
        },
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
        Tool::Door | Tool::Window => place_opening(doc, level, p, tool, selection),
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
    }
}

/// The element a section or 3D view shows at drawing point `p`, found by
/// casting the viewing ray through the model.
fn ray_pick(doc: &Document, view: ElementId, kind: PaneKind, p: DVec2) -> Option<ElementId> {
    let (o, d) = match kind {
        PaneKind::Plan { .. } => return None,
        PaneKind::Section { start, end } => {
            let look = (end - start).normalize_or_zero().perp();
            let xy = PaneKind::section_point(start, end, p);
            (DVec3::new(xy.x, xy.y, p.y), look.extend(0.0))
        }
        PaneKind::Axonometric => {
            let Some(ElementKind::View(View {
                kind: ViewKind::Axonometric { azimuth, altitude },
                ..
            })) = doc.model().get(view).map(|e| &e.kind)
            else {
                return None;
            };
            creabim_geom::section::AxoFrame::new(doc.model(), *azimuth, *altitude).ray(p)
        }
    };
    let scene = creabim_geom::build_scene(doc.model());
    creabim_geom::pick::pick(&scene, o.as_vec3(), d.as_vec3()).map(|h| h.id)
}

/// The level a height belongs to: the highest one at or below it.
fn level_below(doc: &Document, z: f64) -> Option<(ElementId, f64)> {
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

/// A click with a drawing tool in a section or elevation. `p` is the
/// drawing point (along the line, height); `picked` is what is under it.
#[allow(clippy::too_many_arguments)]
fn section_tool(
    doc: &mut Document,
    start: DVec2,
    end: DVec2,
    tool: Tool,
    p: DVec2,
    picked: Option<ElementId>,
    state: &mut PlanState,
    selection: &mut Option<ElementId>,
) -> Result<(), creabim_core::ModelError> {
    let at = |p: DVec2| PaneKind::section_point(start, end, p);
    match tool {
        Tool::Wall => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) => {
                let (x0, x1) = (a.x.min(p.x), a.x.max(p.x));
                let (z0, z1) = (a.y.min(p.y), a.y.max(p.y));
                if x1 - x0 < 0.05 {
                    return Ok(());
                }
                let Some((level, elevation)) = level_below(doc, z0) else {
                    return Ok(());
                };
                let height = if z1 - z0 < 0.1 {
                    doc.model().level(level).map_or(3.0, |l| l.height)
                } else {
                    z1 - z0
                };
                // set just behind the section plane, so its face is what you drew
                let behind =
                    (end - start).normalize_or_zero().perp() * (state.wall_thickness / 2.0 + 0.01);
                let e = Element::new(
                    "Wall",
                    ElementKind::Wall(Wall {
                        level,
                        start: at(DVec2::new(x0, 0.0)) + behind,
                        end: at(DVec2::new(x1, 0.0)) + behind,
                        thickness: state.wall_thickness,
                        height,
                        base_offset: z0 - elevation,
                    }),
                );
                *selection = Some(e.id);
                doc.commit(Transaction::new("Draw wall in section").insert(e))
            }
        },
        Tool::Door | Tool::Window => {
            // the wall under the cursor hosts the opening (or the wall of
            // the door or window under it)
            let Some(mut host) = picked else {
                return Ok(());
            };
            if let Some(ElementKind::Opening(o)) = doc.model().get(host).map(|e| &e.kind) {
                host = o.host;
            }
            let Some(ElementKind::Wall(w)) = doc.model().get(host).map(|e| e.kind.clone()) else {
                return Ok(());
            };
            let look = (end - start).normalize_or_zero().perp();
            let d = w.direction();
            if d.perp_dot(look).abs() < 1e-6 {
                return Ok(()); // seen edge-on
            }
            // where the viewing ray through the click meets the wall's centreline
            let o = at(p);
            let t = (w.start - o).perp_dot(d) / look.perp_dot(d);
            let u = (o + look * t - w.start).dot(d);
            let base = doc.model().level(w.level).map_or(0.0, |l| l.elevation) + w.base_offset;
            let (kind, width, height, name) = match tool {
                Tool::Door => (OpeningKind::Door, 0.9, 2.1, "Door"),
                _ => (OpeningKind::Window, 1.2, 1.3, "Window"),
            };
            if width > w.length() {
                return Ok(());
            }
            let sill = match kind {
                OpeningKind::Door => 0.0,
                OpeningKind::Window => {
                    (p.y - base - height / 2.0).clamp(0.0, (w.height - height).max(0.0))
                }
            };
            let e = Element::new(
                name,
                ElementKind::Opening(Opening {
                    host,
                    kind,
                    offset: (u - width / 2.0).clamp(0.0, w.length() - width),
                    sill,
                    width,
                    height,
                }),
            );
            *selection = Some(e.id);
            doc.commit(
                Transaction::new(format!("Place {} in section", name.to_lowercase())).insert(e),
            )
        }
        Tool::Mass => {
            let Some((level, elevation)) = level_below(doc, p.y) else {
                return Ok(());
            };
            let xy = at(p);
            let e = Element::new(
                "Mass",
                ElementKind::FreeMesh(FreeMesh {
                    level: Some(level),
                    origin: DVec3::new(xy.x, xy.y, p.y - elevation),
                    mesh: EditMesh::cuboid(DVec3::new(2.0, 2.0, 3.0)),
                    ifc_class: "IfcBuildingElementProxy".into(),
                }),
            );
            *selection = Some(e.id);
            doc.commit(Transaction::new("Add mass").insert(e))
        }
        _ => Ok(()),
    }
}

/// Handles on a selected section or elevation line. Returns `Some` while a
/// handle is being dragged (the pane skips its other input then).
fn drag_section_line(
    doc: &mut Document,
    painter: &egui::Painter,
    xf: &Xf,
    response: &egui::Response,
    state: &mut PlanState,
    selection: Option<ElementId>,
) -> Option<Option<String>> {
    let id = selection?;
    let Some(ElementKind::View(View {
        kind: ViewKind::Section { start, end, .. },
        ..
    })) = doc.model().get(id).map(|e| &e.kind)
    else {
        state.drag = None;
        return None;
    };
    let (start, end) = (*start, *end);
    let handles = [
        (Handle::Start, start),
        (Handle::End, end),
        (Handle::Line, (start + end) / 2.0),
    ];
    let accent = Color32::from_rgb(230, 120, 30);
    for (h, p) in handles {
        let c = xf.to_screen(p);
        if h == Handle::Line {
            painter.circle(c, 5.0, Color32::WHITE, Stroke::new(1.5, accent));
        } else {
            painter.rect(
                Rect::from_center_size(c, Vec2::splat(9.0)),
                0.0,
                Color32::WHITE,
                Stroke::new(1.5, accent),
                egui::StrokeKind::Middle,
            );
        }
    }
    if response.drag_started_by(egui::PointerButton::Primary) {
        // where the button went down, not where the drag was recognised
        let p = painter
            .ctx()
            .input(|i| i.pointer.press_origin())
            .or(response.interact_pointer_pos())?;
        state.drag = handles
            .iter()
            .find(|(_, q)| xf.to_screen(*q).distance(p) < 10.0)
            .map(|(h, _)| (*h, xf.to_world(p), start, end));
    }
    let (h, grab, s0, e0) = state.drag?;
    if response.drag_stopped() || !response.dragged() {
        state.drag = None;
        return Some(None);
    }
    let p = xf.to_world(response.interact_pointer_pos()?);
    let snap = |v: DVec2| (v / 0.05).round() * 0.05;
    let (s1, e1) = match h {
        Handle::Start => (snap(p), e0),
        Handle::End => (s0, snap(p)),
        Handle::Line => {
            let d = snap(p - grab);
            (s0 + d, e0 + d)
        }
    };
    if s1.distance(e1) < 0.2 {
        return Some(None);
    }
    let r = doc.edit_merged(id, "Move section line", |e| {
        if let ElementKind::View(View {
            kind: ViewKind::Section { start, end, .. },
            ..
        }) = &mut e.kind
        {
            (*start, *end) = (s1, e1);
        }
    });
    Some(r.err().map(|e| e.to_string()))
}

/// Centre and zoom that fit a drawing in the pane.
fn fit(d: &Drawing, rect: Rect) -> (DVec2, f32) {
    match d.bounds() {
        Some((lo, hi)) => {
            let size = (hi - lo).max(DVec2::splat(1.0));
            let scale = (rect.width() as f64 / size.x).min(rect.height() as f64 / size.y) * 0.85;
            ((lo + hi) / 2.0, scale.clamp(2.0, 2000.0) as f32)
        }
        None => (DVec2::ZERO, 45.0),
    }
}

/// A, B, … Z, AA, AB, …
fn section_letter(n: usize) -> String {
    let mut n = n;
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break s;
        }
        n = n / 26 - 1;
    }
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

/// Draw one drawing item. `hatch` is the hatch spacing in drawing units.
pub fn draw_item(painter: &egui::Painter, xf: &Xf, item: &PlanItem, selected: bool, hatch: f64) {
    let highlight = Color32::from_rgb(230, 120, 30);
    let ink = if selected { highlight } else { Color32::BLACK };
    match item {
        PlanItem::Fill {
            points,
            hatch: pattern,
            ..
        } => {
            let gray = (paper::hatch_fill(*pattern) * 255.0) as u8;
            let fill = if selected {
                Color32::from_rgb(250, 205, 160)
            } else {
                Color32::from_gray(gray)
            };
            fill_polygon(painter, xf, points, fill);
            // skip hatching once it would be too dense to read
            if hatch * xf.scale as f64 > 3.0 {
                let stroke = Stroke::new(0.6, Color32::from_gray(70));
                for (a, b) in pattern.lines(points, hatch) {
                    painter.line_segment([xf.to_screen(a), xf.to_screen(b)], stroke);
                }
            }
        }
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
                Pen::Cut => {
                    painter.line_segment([a, b], Stroke::new(1.8, ink));
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
            // short tags (grids, section marks) sit in a bubble; longer
            // labels such as level datums are plain text
            if text.chars().count() <= 3 {
                painter.circle(p, 10.0, Color32::WHITE, Stroke::new(1.0, blue));
            }
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

/// Fill a possibly concave polygon.
fn fill_polygon(painter: &egui::Painter, xf: &Xf, points: &[DVec2], fill: Color32) {
    let mut mesh = egui::Mesh::default();
    for p in points {
        mesh.colored_vertex(xf.to_screen(*p), fill);
    }
    for [a, b, c] in triangulate(points) {
        mesh.add_triangle(a as u32, b as u32, c as u32);
    }
    painter.add(egui::Shape::mesh(mesh));
}

pub fn hit_test(items: &[PlanItem], p: DVec2, tol: f64) -> Option<ElementId> {
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
            PlanItem::Fill { id, points, .. } if point_in_polygon(p, points) => return Some(*id),
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

/// The pane behaviour for a view.
pub fn pane_kind(v: &View) -> PaneKind {
    match v.kind {
        ViewKind::Plan { level, .. } => PaneKind::Plan { level },
        ViewKind::Section { start, end, .. } => PaneKind::Section { start, end },
        ViewKind::Axonometric { .. } => PaneKind::Axonometric,
    }
}
