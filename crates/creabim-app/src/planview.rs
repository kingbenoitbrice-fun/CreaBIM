//! The 2D plan view: draws the generated plan of a level and hosts the
//! drawing tools. Edits made here are ordinary model edits, so the 3D view
//! and any other open view update at once.

use crate::selection::Selection;
use crate::handles::{self, PaneInput};
use crate::modal::{self, Modal, Mode, Outcome, PaneMap, Space};
use crate::sketch::{level_below, Click, SketchDraft};
use crate::snaps::{self, Snapper};
use creabim_core::Workplane;
use creabim_core::{
    Dimension, Document, EditMesh, Element, ElementId, ElementKind, FreeMesh, HostPoint, Opening,
    OpeningKind, Slab, Tag, Transaction, View, ViewKind, Wall,
};
use creabim_geom::annotate::{self, dimension_geometry, tag_geometry};
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
    Dimension,
    Tag,
    Sketch,
    Guide,
}

impl Tool {
    pub const ALL: [Tool; 12] = [
        Tool::Select,
        Tool::Wall,
        Tool::Door,
        Tool::Window,
        Tool::Slab,
        Tool::Mass,
        Tool::Section,
        Tool::Elevation,
        Tool::Dimension,
        Tool::Tag,
        Tool::Sketch,
        Tool::Guide,
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
            Tool::Dimension => "Dimension",
            Tool::Tag => "Tag",
            Tool::Sketch => "Sketch",
            Tool::Guide => "Guide",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => "Click an element to select it. With the pointer over a view, G moves it, R rotates, S scales and Shift+D duplicates.",
            Tool::Wall => "Click to start, click again to place each wall. Esc or right-click ends the chain. Shift keeps it orthogonal.",
            Tool::Door => "Click on a wall to place a door.",
            Tool::Window => "Click on a wall to place a window.",
            Tool::Slab => "Click two opposite corners.",
            Tool::Mass => "Click to place a freeform mass you can then edit like a mesh.",
            Tool::Section => "Click two points for the section line. It looks to the left of the direction you draw.",
            Tool::Elevation => "Click two points for the elevation line, outside the building. It looks to the left of the direction you draw.",
            Tool::Dimension => "Click two points to measure, then click where the dimension line goes. Points snap to wall corners, door and window edges and levels, and stay attached to them.",
            Tool::Tag => "Click a door, window, wall or slab, then click where its tag goes.",
            Tool::Sketch => "Click the corners of a sketch on this level. Click the first corner to close it, Enter or right-click to leave it open, Esc to cancel. Then E extrudes it.",
            Tool::Guide => "Click two points for a guide through them. Or click an edge, then click where its parallel guide goes. Everything snaps to guides.",
        }
    }

    /// The hint for this tool in a section or elevation.
    fn section_hint(self) -> &'static str {
        match self {
            Tool::Select => "Click to select. Drag a selected element, tag or dimension to move it. G moves, S scales, Shift+D duplicates.",
            Tool::Wall => "Click two points: the wall runs along the section line, as high as you draw it.",
            Tool::Slab => "Click two corners of the slab's edge. Its top snaps to the nearest level; it runs back as far as the walls.",
            Tool::Section => "Click where the new section cuts, then click on the side it should look towards.",
            Tool::Elevation => "Click where the new elevation stands, then click on the side it should look towards.",
            Tool::Dimension => "Click two points to measure, then click where the dimension line goes. Points snap to levels.",
            Tool::Sketch => "Click the corners of a sketch on the section plane. Click the first corner to close it, Enter or right-click to leave it open. Then E extrudes it.",
            t => t.hint(),
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
    /// A schedule drawn as a table: click a row to select its element.
    Schedule,
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
    /// Drawing scale denominator, for hatch spacing and annotation sizes.
    pub scale: f64,
}

impl PaneStyle {
    /// Drawing units per paper millimetre.
    pub fn mm(&self) -> f64 {
        self.scale / 1000.0
    }
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
    /// Dimension being drawn: its points so far, attached and as drawn.
    dim: Vec<(HostPoint, DVec2)>,
    /// Element picked for a tag, waiting for the tag's position.
    tag_host: Option<ElementId>,
    /// Tag or dimension being dragged: where the drag began and its offset then.
    anno_drag: Option<(ElementId, DVec2, DVec2)>,
    /// A keyboard move, rotate or scale, in whichever pane started it.
    pub modal: Option<Modal>,
    /// A sketch being drawn, in whichever pane started it.
    pub sketch: Option<SketchDraft>,
    /// A handle or selected element being dragged in a plan or section.
    grip: Option<handles::Drag>,
    /// Snaps for each 2D view: built from its drawing without the listed
    /// elements, at a model revision.
    snaps: HashMap<ElementId, (u64, Vec<ElementId>, Snapper)>,
    /// A message for the status bar, taken by the app each frame.
    pub status: Option<String>,
    /// The edge a guide is drawn parallel to, picked by its first click.
    pub guide_edge: Option<(DVec2, DVec2)>,
    /// A slab outline or wall path being redrawn.
    pub outline: Option<crate::outline::Outline>,
}

impl Default for PlanState {
    fn default() -> Self {
        Self {
            cameras: HashMap::new(),
            tool: Tool::Select,
            pending: None,
            wall_thickness: 0.2,
            drag: None,
            dim: vec![],
            tag_host: None,
            anno_drag: None,
            modal: None,
            sketch: None,
            grip: None,
            snaps: HashMap::new(),
            status: None,
            guide_edge: None,
            outline: None,
        }
    }
}

impl PlanState {
    /// Drop any half-finished tool input.
    pub fn cancel(&mut self) {
        self.pending = None;
        self.dim.clear();
        self.tag_host = None;
        self.sketch = None;
        self.guide_edge = None;
        self.outline = None;
    }

    /// Is a tool waiting for more clicks?
    pub fn busy(&self) -> bool {
        self.pending.is_some()
            || !self.dim.is_empty()
            || self.tag_host.is_some()
            || self.sketch.is_some()
            || self.modal.is_some()
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
    selection: &mut Selection,
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
        (PaneKind::Axonometric, Tool::Tag) => Tool::Tag,
        (PaneKind::Axonometric | PaneKind::Schedule, _) => Tool::Select,
        (_, t) => t,
    };
    let mm = style.mm();

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

    if style.decorations && kind != PaneKind::Schedule {
        draw_metric_grid(&painter, &xf);
    }
    for item in &plan.items {
        draw_item(&painter, &xf, item, selection.contains(item.id()), mm);
    }

    let shift = ui.input(|i| i.modifiers.shift);
    // snaps: the selection and what joins it are left out while selecting,
    // since dragging them must not snap to themselves
    let exclude = match tool {
        Tool::Select => selection.active.map(|id| joined_with(doc, id)).unwrap_or_default(),
        _ => vec![],
    };
    if matches!(kind, PaneKind::Plan { .. } | PaneKind::Section { .. }) {
        refresh_snaps(state, view, doc.model().revision(), plan, &exclude);
    }
    let pane_project = |p: DVec3| Some(xf.to_screen(p.truncate()));
    let tool_snap = response.hover_pos().and_then(|p| {
        let snapper = &state.snaps.get(&view)?.2;
        let from = state.pending.map(|a| a.extend(0.0));
        let s = snapper.snap(p, &pane_project, from, Some(xf.to_world(p).extend(0.0)))?;
        Some(s)
    });
    let hover_world = response.hover_pos().map(|p| match kind {
        PaneKind::Plan { .. } if !shift => tool_snap.map_or(xf.to_world(p), |s| s.p.truncate()),
        PaneKind::Plan { level } => {
            snap(doc, level, &xf, xf.to_world(p), state.pending, shift, &[])
        }
        PaneKind::Axonometric | PaneKind::Schedule => xf.to_world(p),
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

    // redrawing a slab or wall on this plan's level
    if let (Some(o), PaneKind::Plan { level }) = (&state.outline, kind) {
        if o.level == level {
            let o = o.clone();
            let project = |p: DVec3| Some(xf.to_screen(p.truncate()));
            if let (Some(s), false) = (tool_snap, shift) {
                let from = o.points.last().map(|a| xf.to_screen(*a));
                snaps::draw(&painter, s, xf.to_screen(s.p.truncate()), from);
            }
            o.draw(&painter, &project, hover_world);
            let (enter, escape) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::Escape),
                )
            });
            if escape {
                state.outline = None;
                state.status = Some("Redraw cancelled.".into());
                return None;
            }
            let mut closed = false;
            if response.clicked() {
                if let (Some(p), Some(h)) = (hover_world, response.hover_pos()) {
                    let near = o.near_first(h, &project);
                    closed = state.outline.as_mut().is_some_and(|o| o.click(p, near));
                }
            }
            let finish = enter || (response.secondary_clicked() && !o.points.is_empty());
            if finish || closed {
                let o = state.outline.take()?;
                return match o.finish(doc, closed) {
                    Ok(msg) => {
                        state.status = Some(msg);
                        None
                    }
                    Err(e) => Some(e.to_string()),
                };
            }
            state.status = Some(o.hint().into());
            return None;
        }
    }

    // pointer edits: keyboard move, rotate and scale; sketches; grips
    let space = match kind {
        PaneKind::Plan { .. } => Some(Space::Plan),
        PaneKind::Section { start, end } => Some(Space::Section { start, end }),
        PaneKind::Axonometric | PaneKind::Schedule => None,
    };
    if let Some(space) = space {
        let to_model = |pos: Pos2, _axis: Option<creabim_core::Axis>, pivot: DVec3| {
            Some(pane_to_model(space, xf.to_world(pos), pivot.z))
        };
        let project = |p: DVec3| Some(xf.to_screen(model_to_pane(space, p)));
        let map = PaneMap {
            to_model: &to_model,
            project: &project,
        };
        if let Some(r) = keyboard_edits(
            ui, doc, view, space, &painter, &response, state, selection, &map,
        ) {
            return r;
        }
        if tool == Tool::Sketch {
            let level_z = match kind {
                PaneKind::Plan { level } => doc.model().level(level).map_or(0.0, |l| l.elevation),
                _ => 0.0,
            };
            let hover = hover_world.map(|w| pane_to_model(space, w, level_z));
            if let Some(r) = sketch_tool(
                ui, doc, view, space, &xf, &painter, &response, state, selection, hover, shift,
            ) {
                return r;
            }
        }
        if tool == Tool::Select {
            let level_z = match kind {
                PaneKind::Plan { level } => doc.model().level(level).map_or(0.0, |l| l.elevation),
                _ => 0.0,
            };
            let look = match space {
                Space::Section { start, end } => {
                    (end - start).normalize_or_zero().perp().extend(0.0)
                }
                _ => -DVec3::Z,
            };
            let snapper = &state.snaps[&view].2;
            let ray = |pos: Pos2| {
                let p = pane_to_model(space, xf.to_world(pos), level_z);
                Some(handles::Ray {
                    o: p - look * 1000.0,
                    d: look,
                })
            };
            let snap = |pos: Pos2, from: Option<DVec3>| {
                let to_pane = |p: DVec3| model_to_pane(space, p).extend(0.0);
                let pane_project = |p: DVec3| Some(xf.to_screen(p.truncate()));
                let free = Some(xf.to_world(pos).extend(0.0));
                let s = snapper.snap(pos, &pane_project, from.map(to_pane), free)?;
                let z = from.map_or(level_z, |f| f.z);
                let p = pane_to_model(space, s.p.truncate(), z);
                Some((p, snaps::Snap { p, kind: s.kind }))
            };
            let grab_body = |pos: Pos2, id: ElementId| {
                let w = xf.to_world(pos);
                (hit_test(&plan.items, w, 6.0 / xf.scale as f64, mm) == Some(id))
                    .then(|| pane_to_model(space, w, level_z))
            };
            let input = PaneInput {
                ray: &ray,
                project: &project,
                look,
                snap: &snap,
                grab_body: &grab_body,
                body_normal: if matches!(space, Space::Section { .. }) {
                    look
                } else {
                    DVec3::Z
                },
            };
            let mut status = None;
            let busy = handles::interact(
                doc,
                &painter,
                &response,
                selection.active,
                None,
                &mut state.grip,
                &input,
                &mut status,
            );
            if status.is_some() {
                state.status = status;
            }
            if busy {
                return None;
            }
        }
    }

    // a selected section line in a plan can be dragged by its ends or middle
    if let (PaneKind::Plan { .. }, Tool::Select) = (kind, tool) {
        if let Some(r) = drag_section_line(doc, &painter, &xf, &response, state, selection.active) {
            return r;
        }
    }
    // a tag or dimension can be dragged into place
    if tool == Tool::Select {
        if let Some(r) = drag_annotation(doc, view, plan, &xf, &response, state, selection, mm) {
            return r;
        }
    }

    // dimension and tag tools in progress
    let view_def = match doc.model().get(view).map(|e| &e.kind) {
        Some(ElementKind::View(v)) => Some(v.clone()),
        _ => None,
    };
    let dim_hover = match (tool, &view_def, response.hover_pos()) {
        (Tool::Dimension, Some(v), Some(p)) => {
            Some(dim_snap(doc, v, kind, plan, &xf, xf.to_world(p)))
        }
        _ => None,
    };
    let accent = Color32::from_rgb(230, 120, 30);
    if let Some((_, at)) = dim_hover {
        painter.circle_stroke(xf.to_screen(at), 5.0, Stroke::new(1.5, accent));
    }
    match (state.dim.as_slice(), dim_hover, response.hover_pos()) {
        ([(_, a)], Some((_, b)), _) if a.distance(b) > 1e-6 => {
            preview_dimension(&painter, &xf, *a, b, 0.0, mm, accent);
        }
        ([(_, a), (_, b)], _, Some(h)) => {
            let off = (xf.to_world(h) - *a).dot((*b - *a).normalize_or_zero().perp());
            preview_dimension(&painter, &xf, *a, *b, off, mm, accent);
        }
        _ => {}
    }
    if let (Tool::Tag, Some(host), Some(h)) = (tool, state.tag_host, response.hover_pos()) {
        if let Some(v) = &view_def {
            if let Some(proj) = annotate::Projector::of(doc.model(), v) {
                if let Some(c) = annotate::centre(doc.model(), host) {
                    let (text, round) = annotate::tag_text(doc.model(), host);
                    let item = PlanItem::Tag {
                        id: host,
                        anchor: proj.project(c),
                        at: xf.to_world(h),
                        text,
                        round,
                    };
                    draw_item(&painter, &xf, &item, true, mm);
                }
            }
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
            Tool::Guide => {
                let (through, dir, text) = match state.guide_edge {
                    Some((e0, e1)) => {
                        let d = (e1 - e0).normalize_or_zero();
                        let off = (b - e0).perp_dot(d).abs();
                        (b, d, format!("{off:.2} m"))
                    }
                    None => {
                        let d = (b - a).normalize_or_zero();
                        let deg = d.y.atan2(d.x).to_degrees().rem_euclid(180.0);
                        (a, d, format!("{deg:.1}°"))
                    }
                };
                let reach = rect.width().max(rect.height()) as f64 / xf.scale as f64;
                painter.extend(egui::Shape::dashed_line(
                    &[
                        xf.to_screen(through - dir * reach),
                        xf.to_screen(through + dir * reach),
                    ],
                    accent,
                    10.0,
                    5.0,
                ));
                painter.text(
                    xf.to_screen(b) + Vec2::new(12.0, -12.0),
                    egui::Align2::LEFT_BOTTOM,
                    text,
                    egui::FontId::proportional(13.0),
                    accent.color,
                );
            }
            _ => {}
        }
    }
    if let (Some(s), PaneKind::Plan { .. }, false) = (tool_snap, kind, shift) {
        if !matches!(
            tool,
            Tool::Select | Tool::Dimension | Tool::Tag | Tool::Sketch
        ) {
            let from = state.pending.map(|a| xf.to_screen(a));
            snaps::draw(&painter, s, xf.to_screen(s.p.truncate()), from);
        }
    }
    if let Some(p) = hover_world {
        if !matches!(tool, Tool::Select | Tool::Dimension | Tool::Tag) {
            painter.circle_stroke(
                xf.to_screen(p),
                4.0,
                Stroke::new(1.0, Color32::from_rgb(230, 120, 30)),
            );
        }
    }

    if ui.input(|i| i.key_pressed(egui::Key::Escape)) || response.secondary_clicked() {
        state.cancel();
    }
    if let (PaneKind::Section { .. }, Some(a), Some(b)) = (kind, state.pending, hover_world) {
        if matches!(tool, Tool::Section | Tool::Elevation) {
            preview_cross_section(&painter, &xf, a, b, accent);
        }
    }

    let mut error = None;
    if response.clicked() {
        let p = hover_world?;
        let raw = xf.to_world(response.interact_pointer_pos().unwrap_or_default());
        // linework first (edges, symbols); then whatever the view ray hits
        let picked = || {
            hit_test(&plan.items, raw, 6.0 / scale as f64, mm)
                .or_else(|| ray_pick(doc, view, kind, raw))
        };
        let result = match (kind, tool) {
            (_, Tool::Select) => {
                selection.set(picked());
                Ok(())
            }
            (_, Tool::Dimension) => match (dim_hover, &view_def) {
                (Some(hp), Some(_)) => dimension_tool(doc, view, hp, raw, state, selection),
                _ => Ok(()),
            },
            (_, Tool::Tag) => tag_tool(doc, view, &view_def, picked(), raw, state, selection),
            (PaneKind::Section { start, end }, _) => {
                section_tool(doc, start, end, tool, p, picked(), state, selection)
            }
            (PaneKind::Plan { level }, tool) => {
                if tool == Tool::Guide && state.pending.is_none() {
                    state.guide_edge = tool_snap
                        .filter(|s| {
                            !shift
                                && matches!(
                                    s.kind,
                                    snaps::SnapKind::OnEdge
                                        | snaps::SnapKind::Guide
                                        | snaps::SnapKind::Midpoint
                                )
                        })
                        .and_then(|s| state.snaps.get(&view)?.2.edge_at(s.p))
                        .map(|(a, b)| (a.truncate(), b.truncate()));
                }
                plan_tool(doc, level, tool, p, state, selection)
            }
            (PaneKind::Axonometric | PaneKind::Schedule, _) => Ok(()),
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
                (PaneKind::Plan { .. }, Tool::Select) => {
                    "Click to select. Drag a selected element to move it, or its square grips to stretch it. G move, R rotate, S scale, Shift+D duplicate."
                }
                (PaneKind::Plan { .. }, t) => t.hint(),
                (PaneKind::Section { .. }, t) => t.section_hint(),
                (PaneKind::Axonometric, Tool::Tag) => Tool::Tag.hint(),
                (PaneKind::Axonometric, _) => {
                    "Drag to orbit, click to select. The Tag tool works here too. Right-drag pans, the wheel zooms."
                }
                (PaneKind::Schedule, _) => {
                    "Click a row to select its element."
                }
            },
            egui::FontId::proportional(12.0),
            Color32::from_gray(90),
        );
    }
    error
}

/// Guides are drawn in this colour, in every pane.
pub const GUIDE_COLOR: Color32 = Color32::from_rgb(190, 60, 170);

/// The part of the screen segment ab inside `r`.
pub fn clip_to_rect(a: Pos2, b: Pos2, r: Rect) -> Option<(Pos2, Pos2)> {
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    let d = b - a;
    for (p, q) in [
        (-d.x, a.x - r.min.x),
        (d.x, r.max.x - a.x),
        (-d.y, a.y - r.min.y),
        (d.y, r.max.y - a.y),
    ] {
        if p.abs() < 1e-9 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 < t1).then(|| (a + d * t0, a + d * t1))
}

/// A click with a drawing tool in a plan (or on a level in the 3D view).
pub fn plan_tool(
    doc: &mut Document,
    level: ElementId,
    tool: Tool,
    p: DVec2,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), creabim_core::ModelError> {
    match tool {
        Tool::Select => Ok(()),
        Tool::Section | Tool::Elevation => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) if a.distance(p) > 0.2 => {
                add_section_view(doc, a, p, tool == Tool::Section, selection)
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
                        material: None,
                    }),
                );
                selection.only(e.id);
                state.pending = Some(p);
                doc.commit(Transaction::new("Draw wall").insert(e))
            }
            Some(_) => Ok(()),
        },
        Tool::Door | Tool::Window => place_opening(doc, level, p, tool, selection),
        Tool::Dimension | Tool::Tag | Tool::Sketch => Ok(()),
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
                        material: None,
                    }),
                );
                selection.only(e.id);
                doc.commit(Transaction::new("Draw slab").insert(e))
            }
        },
        Tool::Guide => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) => {
                let (ga, gb) = match state.guide_edge.take() {
                    Some((e0, e1)) => (p, p + (e1 - e0)),
                    None if a.distance(p) > 0.01 => (a, p),
                    None => return Ok(()),
                };
                let z = doc.model().level(level).map_or(0.0, |l| l.elevation);
                let e = Element::new(
                    "Guide",
                    ElementKind::Guide(creabim_core::Guide {
                        level: Some(level),
                        a: ga.extend(z),
                        b: gb.extend(z),
                    }),
                );
                doc.commit(Transaction::new("Add guide").insert(e))
            }
        },
        Tool::Mass => {
            let e = Element::new(
                "Mass",
                ElementKind::FreeMesh(FreeMesh::new(
                    Some(level),
                    DVec3::new(p.x, p.y, 0.0),
                    EditMesh::cuboid(DVec3::new(2.0, 2.0, 3.0)),
                )),
            );
            selection.only(e.id);
            doc.commit(Transaction::new("Add mass").insert(e))
        }
    }
}

/// The element a section or 3D view shows at drawing point `p`, found by
/// casting the viewing ray through the model.
fn ray_pick(doc: &Document, view: ElementId, kind: PaneKind, p: DVec2) -> Option<ElementId> {
    let (o, d) = match kind {
        PaneKind::Plan { .. } | PaneKind::Schedule => return None,
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
    selection: &mut Selection,
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
                        material: None,
                    }),
                );
                selection.only(e.id);
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
            selection.only(e.id);
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
                ElementKind::FreeMesh(FreeMesh::new(
                    Some(level),
                    DVec3::new(xy.x, xy.y, p.y - elevation),
                    EditMesh::cuboid(DVec3::new(2.0, 2.0, 3.0)),
                )),
            );
            selection.only(e.id);
            doc.commit(Transaction::new("Add mass").insert(e))
        }
        Tool::Slab => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) => {
                let (x0, x1) = (a.x.min(p.x), a.x.max(p.x));
                if x1 - x0 < 0.1 {
                    return Ok(());
                }
                // the top snaps to the nearest level; the drawn height is the thickness
                let top = a.y.max(p.y);
                let Some((level, _)) = doc
                    .model()
                    .levels()
                    .map(|(e, l)| (e.id, l.elevation))
                    .min_by(|x, y| (x.1 - top).abs().total_cmp(&(y.1 - top).abs()))
                else {
                    return Ok(());
                };
                let dy = (a.y - p.y).abs();
                let thickness = if dy > 0.05 { dy } else { 0.25 };
                let (d0, d1) = depth_of_walls(doc, level, start, end, x0, x1);
                let look = (end - start).normalize_or_zero().perp();
                let corner = |x: f64, d: f64| at(DVec2::new(x, 0.0)) + look * d;
                let mut boundary = vec![
                    corner(x0, d0),
                    corner(x1, d0),
                    corner(x1, d1),
                    corner(x0, d1),
                ];
                if signed_area(&boundary) < 0.0 {
                    boundary.reverse();
                }
                let e = Element::new(
                    "Slab",
                    ElementKind::Slab(Slab {
                        level,
                        boundary,
                        thickness,
                        material: None,
                    }),
                );
                selection.only(e.id);
                doc.commit(Transaction::new("Draw slab in section").insert(e))
            }
        },
        Tool::Section | Tool::Elevation => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) if (p.x - a.x).abs() > 0.05 => {
                // a new view square to this one, through the first click,
                // looking towards the side of the second
                let look = (end - start).normalize_or_zero().perp();
                let base = at(DVec2::new(a.x, 0.0));
                let (near, far) = model_depth(doc, start, end);
                let (s, e) = if p.x > a.x {
                    (base + look * far, base + look * near)
                } else {
                    (base + look * near, base + look * far)
                };
                add_section_view(doc, s, e, tool == Tool::Section, selection)
            }
            Some(_) => Ok(()),
        },
        _ => Ok(()),
    }
}

fn signed_area(p: &[DVec2]) -> f64 {
    (0..p.len())
        .map(|i| p[i].perp_dot(p[(i + 1) % p.len()]))
        .sum::<f64>()
        / 2.0
}

/// How far back from a section line the walls of `level` reach, within the
/// drawing range `x0..x1`: (nearest, furthest). A slab drawn in a section
/// runs back this far.
fn depth_of_walls(
    doc: &Document,
    level: ElementId,
    start: DVec2,
    end: DVec2,
    x0: f64,
    x1: f64,
) -> (f64, f64) {
    let along = (end - start).normalize_or_zero();
    let look = along.perp();
    let mut range: Option<(f64, f64)> = None;
    for e in doc.model().iter() {
        let ElementKind::Wall(w) = &e.kind else {
            continue;
        };
        if w.level != level {
            continue;
        }
        let pts = [w.start, w.end].map(|q| ((q - start).dot(along), (q - start).dot(look)));
        let (u0, u1) = (pts[0].0.min(pts[1].0), pts[0].0.max(pts[1].0));
        if u1 < x0 - 0.5 || u0 > x1 + 0.5 {
            continue;
        }
        for (_, d) in pts {
            range = Some(range.map_or((d, d), |(lo, hi)| (lo.min(d), hi.max(d))));
        }
    }
    match range {
        Some((lo, hi)) if hi - lo > 0.1 => (lo, hi),
        _ => (0.0, 5.0),
    }
}

/// The range of the model in front of a section line, with room to spare:
/// how long a section drawn across it should be.
fn model_depth(doc: &Document, start: DVec2, end: DVec2) -> (f64, f64) {
    let look = (end - start).normalize_or_zero().perp();
    let scene = creabim_geom::build_scene(doc.model());
    let (lo, hi) = scene
        .iter()
        .flat_map(|s| s.mesh.positions.iter())
        .map(|p| (DVec2::new(p[0] as f64, p[1] as f64) - start).dot(look))
        .fold((f64::MAX, f64::MIN), |(a, b), d| (a.min(d), b.max(d)));
    if lo > hi {
        (-2.0, 10.0)
    } else {
        (lo - 2.0, hi + 2.0)
    }
}

/// Add a section or elevation view along `a` → `b`, named in sequence.
fn add_section_view(
    doc: &mut Document,
    a: DVec2,
    b: DVec2,
    cut: bool,
    selection: &mut Selection,
) -> Result<(), creabim_core::ModelError> {
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
        ElementKind::View(View::section(a, b, 30.0, cut)),
    );
    selection.only(e.id);
    doc.commit(Transaction::new(format!("Add {name}")).insert(e))
}

/// Preview of a section or elevation being placed in a section: a vertical
/// line at the first click with an arrow to the side it will look.
fn preview_cross_section(painter: &egui::Painter, xf: &Xf, a: DVec2, b: DVec2, accent: Color32) {
    let lo = xf.to_world(xf.rect.left_bottom());
    let hi = xf.to_world(xf.rect.right_top());
    let stroke = Stroke::new(1.5, accent);
    painter.line_segment(
        [
            xf.to_screen(DVec2::new(a.x, lo.y)),
            xf.to_screen(DVec2::new(a.x, hi.y)),
        ],
        stroke,
    );
    let side = if b.x > a.x { 1.0 } else { -1.0 };
    let tip = xf.to_screen(DVec2::new(a.x, b.y)) + Vec2::new(side * 24.0, 0.0);
    let root = xf.to_screen(DVec2::new(a.x, b.y));
    painter.arrow(root, tip - root, stroke);
}

/// From a pane's drawing point to the model: plans are level at `z`;
/// sections stand on their line.
fn pane_to_model(space: Space, w: DVec2, z: f64) -> DVec3 {
    match space {
        Space::Section { start, end } => {
            (start + (end - start).normalize_or_zero() * w.x).extend(w.y)
        }
        Space::Plan | Space::Model => w.extend(z),
    }
}

fn model_to_pane(space: Space, p: DVec3) -> DVec2 {
    match space {
        Space::Section { start, end } => DVec2::new(
            (p.truncate() - start).dot((end - start).normalize_or_zero()),
            p.z,
        ),
        Space::Plan | Space::Model => p.truncate(),
    }
}

/// G, R and S move, rotate and scale the selection, and Shift+D copies it,
/// while the pointer is over this pane. Returns `Some` while one runs here.
#[allow(clippy::too_many_arguments)]
fn keyboard_edits(
    ui: &egui::Ui,
    doc: &mut Document,
    view: ElementId,
    space: Space,
    painter: &egui::Painter,
    response: &egui::Response,
    state: &mut PlanState,
    selection: &mut Selection,
    map: &PaneMap,
) -> Option<Option<String>> {
    if let Some(m) = state.modal.as_mut().filter(|m| m.pane == Some(view)) {
        match m.update(doc, ui.ctx(), painter, response.hover_pos(), map) {
            Outcome::Running(s) => state.status = Some(s),
            Outcome::Done(s) | Outcome::Cancelled(s) => {
                state.status = Some(s);
                state.modal = None;
            }
        }
        return Some(None);
    }
    if state.modal.is_some() || !response.hovered() || ui.ctx().egui_wants_keyboard_input() {
        return None;
    }
    let id = selection.active?;
    let mode = start_keys(ui.ctx())?;
    let grab = response.hover_pos()?;
    state.cancel();
    match start_modal(doc, mode, id, None, space, Some(view), grab) {
        Ok((m, copy)) => {
            selection.only(copy);
            state.status = Some(format!(
                "{:?}: move the pointer, or type a value. X, Y or Z keeps to an axis.",
                m.mode
            ));
            state.modal = Some(m);
        }
        Err(e) => state.status = Some(e),
    }
    Some(None)
}

/// Which keyboard edit the keys pressed this frame start, if any:
/// (mode, duplicate first).
pub fn start_keys(ctx: &egui::Context) -> Option<(Mode, bool)> {
    ctx.input(|i| {
        if i.modifiers.command || i.modifiers.alt {
            return None;
        }
        let shift = i.modifiers.shift;
        if shift && i.key_pressed(egui::Key::D) {
            Some((Mode::Move, true))
        } else if shift {
            None
        } else if i.key_pressed(egui::Key::G) {
            Some((Mode::Move, false))
        } else if i.key_pressed(egui::Key::R) {
            Some((Mode::Rotate, false))
        } else if i.key_pressed(egui::Key::S) {
            Some((Mode::Scale, false))
        } else {
            None
        }
    })
}

/// Start a keyboard edit, copying the element first for Shift+D. Returns
/// the edit and the element it works on (the copy, if one was made).
pub fn start_modal(
    doc: &mut Document,
    (mode, duplicate): (Mode, bool),
    id: ElementId,
    face: Option<usize>,
    space: Space,
    pane: Option<ElementId>,
    grab: Pos2,
) -> Result<(Modal, ElementId), String> {
    let target = if duplicate {
        modal::duplicate(doc, id)?
    } else {
        id
    };
    let face = face.filter(|_| !duplicate);
    match Modal::start(doc, mode, target, face, space, pane, grab) {
        Ok(mut m) => {
            m.duplicated = duplicate;
            Ok((m, target))
        }
        Err(e) => {
            if duplicate {
                doc.discard_last();
            }
            Err(e)
        }
    }
}

/// Drawing a sketch in a plan or section. Returns `Some` when it used the
/// frame's input.
#[allow(clippy::too_many_arguments)]
fn sketch_tool(
    ui: &egui::Ui,
    doc: &mut Document,
    view: ElementId,
    space: Space,
    xf: &Xf,
    painter: &egui::Painter,
    response: &egui::Response,
    state: &mut PlanState,
    selection: &mut Selection,
    hover: Option<DVec3>,
    shift: bool,
) -> Option<Option<String>> {
    let project = |p: DVec3| Some(xf.to_screen(model_to_pane(space, p)));
    // a sketch being drawn in another pane is left alone
    let mine = state.sketch.as_ref().is_some_and(|d| d.pane == Some(view));
    let tol = 10.0 / xf.scale as f64;
    let on_plane = match (&state.sketch, hover) {
        (Some(d), Some(m)) if mine => Some(d.snap(d.plane.to_plane(m), tol, shift)),
        _ => None,
    };
    if let (Some(d), true) = (&state.sketch, mine) {
        d.draw(painter, &project, on_plane);
    }
    if !mine && !response.clicked() {
        return None;
    }
    let (enter, escape) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
        )
    });
    let finish =
        |doc: &mut Document, state: &mut PlanState, selection: &mut Selection, closed| {
            let d = state.sketch.take()?;
            match d.finish(doc, closed) {
                Ok(Some(id)) => {
                    selection.only(id);
                    state.status = Some(
                        "Sketch added. Press E to extrude it into a mass, or G to move it.".into(),
                    );
                    Some(None)
                }
                Ok(None) => Some(None),
                Err(e) => Some(Some(e.to_string())),
            }
        };
    if escape && state.sketch.is_some() {
        state.sketch = None;
        return Some(None);
    }
    if enter || (response.secondary_clicked() && state.sketch.is_some()) {
        return finish(doc, state, selection, false);
    }
    if !response.clicked() {
        return None;
    }
    let m = hover?;
    if !mine {
        state.sketch = None;
    }
    match &mut state.sketch {
        None => {
            let plane = match space {
                Space::Section { start, end } => Workplane {
                    origin: m,
                    x: (end - start).normalize_or_zero().extend(0.0),
                    y: DVec3::Z,
                },
                Space::Plan | Space::Model => Workplane {
                    origin: m,
                    ..Workplane::ground(m.z)
                },
            };
            state.sketch = Some(SketchDraft::new(plane, Some(view)));
            Some(None)
        }
        Some(d) => match d.click(on_plane?) {
            Some(Click::Closed) => finish(doc, state, selection, true),
            _ => Some(None),
        },
    }
}

/// The element and the walls whose ends meet it: what a drag on it may
/// change, so it must not snap to itself.
pub fn joined_with(doc: &Document, id: ElementId) -> Vec<ElementId> {
    let mut out = vec![id];
    if let Some(ElementKind::Wall(w)) = doc.model().get(id).map(|e| &e.kind) {
        for e in doc.model().iter() {
            if let ElementKind::Wall(o) = &e.kind {
                let touch = [o.start, o.end]
                    .iter()
                    .any(|p| p.distance(w.start) < 1e-3 || p.distance(w.end) < 1e-3);
                if touch && e.id != id {
                    out.push(e.id);
                }
            }
        }
    }
    out
}

/// Rebuild a view's snaps when the model changed, but not during a drag,
/// which changes the model every frame.
fn refresh_snaps(
    state: &mut PlanState,
    view: ElementId,
    revision: u64,
    plan: &Drawing,
    exclude: &[ElementId],
) {
    let dragging = state.grip.is_some();
    let fresh = state
        .snaps
        .get(&view)
        .is_some_and(|(r, ex, _)| ex == exclude && (*r == revision || dragging));
    if !fresh {
        state.snaps.insert(
            view,
            (
                revision,
                exclude.to_vec(),
                Snapper::from_drawing(&plan.items, exclude),
            ),
        );
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

/// Draw one drawing item. `mm` is drawing units per paper millimetre, which
/// sizes hatching and annotations.
pub fn draw_item(painter: &egui::Painter, xf: &Xf, item: &PlanItem, selected: bool, mm: f64) {
    let highlight = Color32::from_rgb(230, 120, 30);
    let ink = if selected { highlight } else { Color32::BLACK };
    let hatch = paper::HATCH_SPACING * mm;
    // annotation text is a fixed size on paper, so it scales with zoom
    let text_px = |size: f64| (size * mm * xf.scale as f64) as f32;
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
                for (a, b) in creabim_geom::plan::hatch_lines(*pattern, points, hatch) {
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
                Pen::Guide => {
                    if let Some((a, b)) = clip_to_rect(a, b, painter.clip_rect()) {
                        let (w, c) = if selected {
                            (2.0, highlight)
                        } else {
                            (1.0, GUIDE_COLOR)
                        };
                        painter.extend(egui::Shape::dashed_line(
                            &[a, b],
                            Stroke::new(w, c),
                            6.0,
                            4.0,
                        ));
                    }
                }
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
        PlanItem::Text {
            at,
            text,
            size,
            align,
            ..
        } => {
            let px = text_px(*size);
            if px >= 4.0 {
                let align = match align {
                    paper::Anchor::Start => egui::Align2::LEFT_CENTER,
                    paper::Anchor::Middle => egui::Align2::CENTER_CENTER,
                    paper::Anchor::End => egui::Align2::RIGHT_CENTER,
                };
                painter.text(
                    xf.to_screen(*at),
                    align,
                    text,
                    egui::FontId::proportional(px),
                    ink,
                );
            }
        }
        PlanItem::Dimension {
            a, b, offset, text, ..
        } => {
            let ink = if selected {
                highlight
            } else {
                Color32::from_rgb(30, 60, 120)
            };
            draw_dimension(painter, xf, *a, *b, *offset, text, mm, ink);
        }
        PlanItem::Tag {
            anchor,
            at,
            text,
            round,
            ..
        } => {
            let g = tag_geometry(*anchor, *at, text, *round, mm);
            let pts: Vec<Pos2> = g.outline.iter().map(|p| xf.to_screen(*p)).collect();
            let ink = if selected {
                highlight
            } else {
                Color32::from_rgb(30, 60, 120)
            };
            if let Some((p, q)) = g.leader {
                painter.line_segment([xf.to_screen(p), xf.to_screen(q)], Stroke::new(1.0, ink));
            }
            painter.add(egui::Shape::convex_polygon(
                pts,
                Color32::WHITE,
                Stroke::new(1.2, ink),
            ));
            let px = text_px(annotate::TEXT).max(7.0);
            painter.text(
                xf.to_screen(*at),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(px),
                ink,
            );
        }
        PlanItem::Image {
            raster,
            lo,
            hi,
            clip,
            ..
        } => {
            let rect = Rect::from_two_pos(xf.to_screen(*lo), xf.to_screen(*hi));
            let clip = clip.map(|(a, b)| Rect::from_two_pos(xf.to_screen(a), xf.to_screen(b)));
            crate::textures::paint(painter, rect, clip, raster);
        }
    }
}

/// A dimension: witness lines, the line with its ticks, and its value.
#[allow(clippy::too_many_arguments)]
fn draw_dimension(
    painter: &egui::Painter,
    xf: &Xf,
    a: DVec2,
    b: DVec2,
    offset: f64,
    text: &str,
    mm: f64,
    ink: Color32,
) {
    let g = dimension_geometry(a, b, offset, mm);
    for (p, q) in g.lines {
        painter.line_segment([xf.to_screen(p), xf.to_screen(q)], Stroke::new(1.0, ink));
    }
    for (p, q) in g.ticks {
        painter.line_segment([xf.to_screen(p), xf.to_screen(q)], Stroke::new(2.0, ink));
    }
    let px = ((annotate::TEXT * mm * xf.scale as f64) as f32).max(7.0);
    rotated_text(painter, xf.to_screen(g.text_at), text, px, ink, g.angle);
}

/// Text centred on `at`, turned `angle` radians counter-clockwise.
pub fn rotated_text(
    painter: &egui::Painter,
    at: Pos2,
    text: &str,
    px: f32,
    color: Color32,
    angle: f64,
) {
    let galley = painter.layout_no_wrap(text.to_string(), egui::FontId::proportional(px), color);
    // the screen's y runs down, so a counter-clockwise turn is a negative angle
    let rot = egui::emath::Rot2::from_angle(-angle as f32);
    let pos = at - rot * (galley.size() / 2.0);
    painter.add(egui::epaint::TextShape::new(pos, galley, color).with_angle(-angle as f32));
}

/// A dimension being drawn.
#[allow(clippy::too_many_arguments)]
fn preview_dimension(
    painter: &egui::Painter,
    xf: &Xf,
    a: DVec2,
    b: DVec2,
    offset: f64,
    mm: f64,
    accent: Color32,
) {
    let text = annotate::dimension_text(a, b);
    draw_dimension(painter, xf, a, b, offset, &text, mm, accent);
}

/// The point a dimension click lands on, and what it attaches to: a wall
/// corner, a door or window edge, or a level; otherwise a free point.
fn dim_snap(
    doc: &Document,
    view: &View,
    kind: PaneKind,
    drawing: &Drawing,
    xf: &Xf,
    p: DVec2,
) -> (HostPoint, DVec2) {
    let model = doc.model();
    let tol = 10.0 / xf.scale as f64;
    let mut best: Option<(f64, Option<ElementId>, DVec2)> = None;
    let mut consider = |host: Option<ElementId>, q: DVec2| {
        let d = q.distance(p);
        if d < tol && best.is_none_or(|(bd, ..)| d < bd) {
            best = Some((d, host, q));
        }
    };
    match kind {
        PaneKind::Plan { level } => {
            let joins = creabim_core::wall_joins(model);
            for e in model.iter() {
                let ElementKind::Wall(w) = &e.kind else {
                    continue;
                };
                if w.level != level {
                    continue;
                }
                let ends = joins.get(&e.id).copied().unwrap_or_default();
                for q in ends.footprint(w) {
                    consider(Some(e.id), q);
                }
                for q in [w.start, w.end] {
                    consider(Some(e.id), q);
                }
                let (d, n, t) = (w.direction(), w.direction().perp(), w.thickness / 2.0);
                for oid in model.dependents(e.id) {
                    let Some(ElementKind::Opening(o)) = model.get(oid).map(|e| &e.kind) else {
                        continue;
                    };
                    for u in [o.offset, o.offset + o.width] {
                        for s in [-t, t] {
                            consider(Some(oid), w.start + d * u + n * s);
                        }
                    }
                }
            }
        }
        PaneKind::Section { .. } => {
            for (e, l) in model.levels() {
                if (p.y - l.elevation).abs() < tol {
                    consider(
                        Some(e.id),
                        DVec2::new((p.x / 0.05).round() * 0.05, l.elevation),
                    );
                }
            }
            // corners of what the section cuts
            for it in &drawing.items {
                if let PlanItem::Fill { points, .. } = it {
                    for q in points {
                        consider(None, *q);
                    }
                }
            }
        }
        _ => {}
    }
    let (host, at) = match best {
        Some((_, h, q)) => (h, q),
        None => (None, (p / 0.05).round() * 0.05),
    };
    (annotate::attach(model, view, host, at), at)
}

/// A click with the Dimension tool: first point, second point, then where the line goes.
fn dimension_tool(
    doc: &mut Document,
    view: ElementId,
    hit: (HostPoint, DVec2),
    raw: DVec2,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), creabim_core::ModelError> {
    match state.dim.len() {
        0 => {
            state.dim.push(hit);
            Ok(())
        }
        1 => {
            if hit.1.distance(state.dim[0].1) > 1e-3 {
                state.dim.push(hit);
            }
            Ok(())
        }
        _ => {
            let (a, pa) = state.dim[0];
            let (b, pb) = state.dim[1];
            state.dim.clear();
            let offset = (raw - pa).dot((pb - pa).normalize_or_zero().perp());
            let e = Element::new(
                "Dimension",
                ElementKind::Dimension(Dimension { view, a, b, offset }),
            );
            selection.only(e.id);
            doc.commit(Transaction::new("Add dimension").insert(e))
        }
    }
}

/// A click with the Tag tool: pick what to tag, then where the tag goes.
fn tag_tool(
    doc: &mut Document,
    view: ElementId,
    v: &Option<View>,
    picked: Option<ElementId>,
    raw: DVec2,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), creabim_core::ModelError> {
    let Some(v) = v else { return Ok(()) };
    match state.tag_host.take() {
        None => {
            let taggable = picked.filter(|id| {
                matches!(
                    doc.model().get(*id).map(|e| &e.kind),
                    Some(
                        ElementKind::Opening(_)
                            | ElementKind::Wall(_)
                            | ElementKind::Slab(_)
                            | ElementKind::FreeMesh(_)
                    )
                )
            });
            state.tag_host = taggable;
            Ok(())
        }
        Some(host) => {
            let (Some(proj), Some(c)) = (
                annotate::Projector::of(doc.model(), v),
                annotate::centre(doc.model(), host),
            ) else {
                return Ok(());
            };
            let e = Element::new(
                "Tag",
                ElementKind::Tag(Tag {
                    view,
                    host,
                    offset: raw - proj.project(c),
                }),
            );
            selection.only(e.id);
            doc.commit(Transaction::new("Add tag").insert(e))
        }
    }
}

/// Tag every door and window in a view that has no tag there yet, just
/// outside its wall. Returns how many were added.
pub fn tag_openings(
    doc: &mut Document,
    view: ElementId,
) -> Result<usize, creabim_core::ModelError> {
    let model = doc.model();
    let Some(ElementKind::View(v)) = model.get(view).map(|e| &e.kind) else {
        return Ok(0);
    };
    let level = match v.kind {
        ViewKind::Plan { level, .. } => Some(level),
        _ => None,
    };
    let Some(proj) = annotate::Projector::of(model, v) else {
        return Ok(0);
    };
    let tagged: Vec<ElementId> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Tag(t) if t.view == view => Some(t.host),
            _ => None,
        })
        .collect();
    // the drawing's middle, to tell outside from inside
    let middle = creabim_geom::section::view_drawing(model, view)
        .and_then(|d| d.bounds())
        .map_or(DVec2::ZERO, |(lo, hi)| (lo + hi) / 2.0);
    let scale = v.scale / 100.0;
    let mut tx = Transaction::new("Tag doors and windows");
    let mut n = 0;
    for e in model.iter() {
        let ElementKind::Opening(o) = &e.kind else {
            continue;
        };
        if tagged.contains(&e.id) {
            continue;
        }
        let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
            continue;
        };
        if level.is_some_and(|l| l != w.level) {
            continue;
        }
        let Some(c) = annotate::centre(model, e.id) else {
            continue;
        };
        let anchor = proj.project(c);
        let offset = match level {
            Some(_) => {
                let n = w.direction().perp();
                let out = if (anchor - middle).dot(n) > 0.0 {
                    n
                } else {
                    -n
                };
                out * 0.9 * scale
            }
            // in sections and 3D views, above the opening
            None => DVec2::new(0.0, (o.height / 2.0 + 0.5) * scale.max(0.5)),
        };
        tx = tx.insert(Element::new(
            "Tag",
            ElementKind::Tag(Tag {
                view,
                host: e.id,
                offset,
            }),
        ));
        n += 1;
    }
    doc.commit(tx)?;
    Ok(n)
}

/// Drag a tag (moves it) or a dimension (moves its line) in the Select
/// tool. Returns `Some` while a drag is in progress.
#[allow(clippy::too_many_arguments)]
fn drag_annotation(
    doc: &mut Document,
    view: ElementId,
    drawing: &Drawing,
    xf: &Xf,
    response: &egui::Response,
    state: &mut PlanState,
    selection: &mut Selection,
    mm: f64,
) -> Option<Option<String>> {
    if response.drag_started_by(egui::PointerButton::Primary) {
        let p = response
            .ctx
            .input(|i| i.pointer.press_origin())
            .or(response.interact_pointer_pos())?;
        let w = xf.to_world(p);
        let hit = drawing.items.iter().rev().find_map(|it| match it {
            PlanItem::Tag { id, at, .. }
                if at.distance(w) < 3.0 * mm.max(6.0 / xf.scale as f64 / 3.0) =>
            {
                Some(*id)
            }
            PlanItem::Dimension {
                id, a, b, offset, ..
            } => {
                let n = (*b - *a).normalize_or_zero().perp() * *offset;
                (segment_distance(w, *a + n, *b + n) < 6.0 / xf.scale as f64).then_some(*id)
            }
            _ => None,
        });
        state.anno_drag = hit.and_then(|id| {
            let start = match &doc.model().get(id)?.kind {
                ElementKind::Tag(t) if t.view == view => t.offset,
                ElementKind::Dimension(d) if d.view == view => DVec2::new(d.offset, 0.0),
                _ => return None,
            };
            Some((id, w, start))
        });
        if let Some((id, ..)) = state.anno_drag {
            selection.only(id);
        }
    }
    let (id, grab, start) = state.anno_drag?;
    if response.drag_stopped() || !response.dragged() {
        state.anno_drag = None;
        return Some(None);
    }
    let p = xf.to_world(response.interact_pointer_pos()?);
    let r = match doc.model().get(id).map(|e| e.kind.clone()) {
        Some(ElementKind::Tag(_)) => doc.edit_merged(id, "Move tag", |e| {
            if let ElementKind::Tag(t) = &mut e.kind {
                t.offset = start + p - grab;
            }
        }),
        Some(ElementKind::Dimension(_)) => {
            // move the line square to what it measures
            let item = drawing.items.iter().find_map(|it| match it {
                PlanItem::Dimension { id: i, a, b, .. } if *i == id => Some((*a, *b)),
                _ => None,
            });
            let Some((a, b)) = item else {
                return Some(None);
            };
            let n = (b - a).normalize_or_zero().perp();
            let offset = start.x + (p - grab).dot(n);
            doc.edit_merged(id, "Move dimension line", |e| {
                if let ElementKind::Dimension(d) = &mut e.kind {
                    d.offset = offset;
                }
            })
        }
        _ => Ok(()),
    };
    Some(r.err().map(|e| e.to_string()))
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
    exclude: &[ElementId],
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
            if w.level != level || exclude.contains(&e.id) {
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

pub fn hit_test(items: &[PlanItem], p: DVec2, tol: f64, mm: f64) -> Option<ElementId> {
    // symbols (doors, windows) win over the walls they sit in
    let near_line = |a: DVec2, b: DVec2| segment_distance(p, a, b) < tol;
    // annotations sit on top of everything
    for it in items.iter().rev() {
        match it {
            PlanItem::Tag { id, at, .. } if p.distance(*at) < (3.0 * mm).max(tol) => {
                return Some(*id)
            }
            PlanItem::Dimension {
                id, a, b, offset, ..
            } => {
                let n = (*b - *a).normalize_or_zero().perp() * *offset;
                let text = (*a + *b) / 2.0 + n;
                if near_line(*a + n, *b + n) || p.distance(text) < (3.0 * mm).max(tol) {
                    return Some(*id);
                }
            }
            PlanItem::Text {
                id,
                at,
                text,
                size,
                align,
            } if *id != ElementId::NIL => {
                // schedule rows: anywhere along the row's height
                let w = paper::text_width(text, *size) * mm;
                let x0 = match align {
                    paper::Anchor::Start => at.x,
                    paper::Anchor::Middle => at.x - w / 2.0,
                    paper::Anchor::End => at.x - w,
                };
                if (p.y - at.y).abs() < size * mm && p.x >= x0 - tol && p.x <= x0 + w + tol {
                    return Some(*id);
                }
            }
            _ => {}
        }
    }
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
    selection: &mut Selection,
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
    selection.only(e.id);
    doc.commit(Transaction::new(format!("Place {}", name.to_lowercase())).insert(e))
}

/// The pane behaviour for a view.
pub fn pane_kind(v: &View) -> PaneKind {
    match v.kind {
        ViewKind::Plan { level, .. } => PaneKind::Plan { level },
        ViewKind::Section { start, end, .. } => PaneKind::Section { start, end },
        ViewKind::Axonometric { .. } => PaneKind::Axonometric,
        ViewKind::Schedule { .. } => PaneKind::Schedule,
    }
}
