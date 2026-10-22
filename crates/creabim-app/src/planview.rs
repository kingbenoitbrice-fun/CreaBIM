//! The 2D plan view: draws the generated plan of a level and hosts the
//! drawing tools. Edits made here are ordinary model edits, so the 3D view
//! and any other open view update at once.

use crate::boxsel;
use crate::commands::Command;
use crate::handles::{self, PaneInput};
use crate::hoverpick::{self, Pane};
use crate::modal::{self, Flow, Modal, Mode, Outcome, PaneMap, Space, Stage};
use crate::outline::Next;
use crate::selection::Selection;
use crate::sketch::{level_below, Click, SketchDraft};
use crate::snaps::{self, SnapKind, Snapper};
use creabim_core::Workplane;
use creabim_core::{
    Dimension, Document, EditMesh, Element, ElementId, ElementKind, FreeMesh, HostPoint, Opening,
    OpeningKind, Slab, Tag, Transaction, View, ViewKind, Wall,
};
use creabim_geom::annotate::{self, dimension_geometry, tag_geometry};
use creabim_geom::annotation::TextLook;
use creabim_geom::SceneItem;
use creabim_geom::{paper, plan::Pen, section::Drawing, triangulate::triangulate, PlanItem};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use glam::{DVec2, DVec3};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Wall,
    Door,
    Window,
    /// A bare opening in a wall: a hole with no door or window.
    Opening,
    Slab,
    /// A vertical hole through the slabs and roofs between two levels.
    Shaft,
    Mass,
    Section,
    Elevation,
    Dimension,
    Tag,
    Angle,
    Radius,
    Diameter,
    ArcLength,
    Note,
    Sketch,
    Guide,
    Grid,
    Column,
    Beam,
    Roof,
    Stair,
    Railing,
    Level,
    Align,
    /// A piece of furniture or a fixture from the catalogue.
    Object,
}

impl Tool {
    pub const ALL: [Tool; 28] = [
        Tool::Select,
        Tool::Wall,
        Tool::Door,
        Tool::Window,
        Tool::Opening,
        Tool::Slab,
        Tool::Shaft,
        Tool::Mass,
        Tool::Column,
        Tool::Beam,
        Tool::Roof,
        Tool::Stair,
        Tool::Railing,
        Tool::Object,
        Tool::Section,
        Tool::Elevation,
        Tool::Dimension,
        Tool::Tag,
        Tool::Angle,
        Tool::Radius,
        Tool::Diameter,
        Tool::ArcLength,
        Tool::Note,
        Tool::Sketch,
        Tool::Guide,
        Tool::Grid,
        Tool::Level,
        Tool::Align,
    ];

    /// Dimensions, tags and notes: tools that annotate a view.
    pub fn is_annotation(self) -> bool {
        matches!(self, Tool::Dimension | Tool::Tag) || crate::annotate::is_tool(self)
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Wall => "Wall",
            Tool::Door => "Door",
            Tool::Window => "Window",
            Tool::Opening => "Wall opening",
            Tool::Slab => "Slab",
            Tool::Shaft => "Shaft",
            Tool::Mass => "Mass",
            Tool::Section => "Section",
            Tool::Elevation => "Elevation",
            Tool::Dimension => "Dimension",
            Tool::Tag => "Tag",
            Tool::Angle => "Angle",
            Tool::Radius => "Radius",
            Tool::Diameter => "Diameter",
            Tool::ArcLength => "Arc length",
            Tool::Note => "Note",
            Tool::Sketch => "Sketch",
            Tool::Guide => "Guide",
            Tool::Grid => "Grid",
            Tool::Column => "Column",
            Tool::Beam => "Beam",
            Tool::Roof => "Roof",
            Tool::Stair => "Stair",
            Tool::Railing => "Railing",
            Tool::Level => "Level",
            Tool::Align => "Align",
            Tool::Object => "Object",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => "Click an element to select it. Drag a selected element to move it. M (Move) moves it by a base point; over a view, G moves it with the pointer, R rotates, S scales, Shift+D duplicates.",
            Tool::Wall => "Click to start, click again to place each wall. Esc or right-click ends the chain. Shift keeps it orthogonal.",
            Tool::Door => "Click on a wall to place a door.",
            Tool::Window => "Click on a wall to place a window.",
            Tool::Opening => "Click on a wall to cut a bare rectangular opening in it: no door or window, just the hole. Set its size and sill in Properties.",
            Tool::Shaft => "Draw the shaft's outline like a slab (1–4: Polygon, Rectangle, Fill room, From sketch). It runs from this level to the one above, cutting every slab and roof in between.",
            Tool::Slab => "Click the slab's corners, then the first again or Enter. A draws arcs, L lines; Esc or right-click takes back a point, right-click again leaves. 1–4: Polygon, Rectangle, Fill room, From sketch.",
            Tool::Mass => "Click to place a freeform mass you can then edit like a mesh.",
            Tool::Section => "Click two points for the section line. It looks to the left of the direction you draw.",
            Tool::Elevation => "Click two points for the elevation line, outside the building. It looks to the left of the direction you draw.",
            Tool::Dimension => "Click two points to measure, then click where the dimension line goes. Points snap to wall corners, door and window edges and levels, and stay attached to them.",
            Tool::Tag => "Click a door, window, wall, slab, roof, column, beam, stair or railing, then click where its tag goes.",
            Tool::Angle => "Click two lines (wall faces, edges) for the angle between them, or click the corner and a point on each arm. Then click where the arc goes.",
            Tool::Radius => "Click an arc or circle, or three points on one. Then click where the dimension line goes.",
            Tool::Diameter => "Click an arc or circle, or three points on one. Then click where the dimension line goes.",
            Tool::ArcLength => "Click an arc, or its start, a point on it and its end. Then click where the dimension line goes.",
            Tool::Note => "Click what the note points at, then where its text goes (turn Leader off to just click). Type its text in Properties. Works on sheets too.",
            Tool::Sketch => "Click the corners of a sketch on this level. Click the first corner to close it, Enter or right-click to leave it open, Esc to cancel. L, A and C draw lines, arcs and circles. Then E extrudes it.",
            Tool::Guide => "Click two points for a guide through them. Or click an edge, then click where its parallel guide goes. Everything snaps to guides.",
            Tool::Grid => "Click two points for a grid line. It goes in the active grid system (top bar) and takes its next label. Shift keeps it straight.",
            Tool::Column => "Click to place a column. It runs up to the next level; set its size in the top bar or Properties.",
            Tool::Beam => "Click the beam's start, then its end; keep clicking to chain beams. Its top sits at the top of the storey. Esc or right-click ends.",
            Tool::Roof => "Draw the roof's outline like a slab (1–4: Polygon, Rectangle, Fill room, From sketch). Pick Flat, Mono-pitch, Gable or Hip in the bar first.",
            Tool::Stair => "Click where the stair starts, then click the way it climbs. It rises to the next level in equal risers.",
            Tool::Railing => "Click inside a stair to rail both its sides, or click the points of a path; click the last point again to finish.",
            Tool::Level => "In a section or elevation, click where the level line starts (its height), then where it ends; its floor plan is made with it. In other views the Levels window opens to add, rename and move levels.",
            Tool::Object => "Click to place the object picked in the Catalogue window. Sinks, WCs, kitchen units, beds and sofas back onto the nearest wall; Space turns the next one a quarter.",
            Tool::Align => "Click a reference line (a wall face, slab edge, guide or grid line), then the edge of what to align: it turns parallel and moves flush. In Extend mode a wall, beam or grid line end stretches to the reference instead.",
        }
    }

    /// Tools that draw an outline the way the Slab tool does.
    pub fn draws_outline(self) -> bool {
        matches!(self, Tool::Slab | Tool::Roof | Tool::Shaft)
    }

    /// The hint for this tool in a section or elevation.
    fn section_hint(self) -> &'static str {
        match self {
            Tool::Select => "Click to select, Shift+click to add, drag on empty paper for a box. M moves by a base point, G with the pointer, R tilts, S scales, Shift+D duplicates.",
            Tool::Wall => "Click two points: the wall runs along the section line, as high as you draw it.",
            Tool::Slab => "Click two corners of the slab's edge. Its top snaps to the nearest level; it runs back as far as the walls.",
            Tool::Section => "Click where the new section cuts, then click on the side it should look towards.",
            Tool::Elevation => "Click where the new elevation stands, then click on the side it should look towards.",
            Tool::Dimension => "Click two points to measure, then click where the dimension line goes. Points snap to levels.",
            Tool::Guide => "Click two points for a guide on the section plane. It shows here, in 3D and in any section on the same plane.",
            Tool::Level => "Click where the new level line starts (it sets the height; it snaps to model points), then where it ends. Its floor plan is made with it.",
            Tool::Sketch => "Click the corners of a sketch on the section plane. Click the first corner to close it, Enter or right-click to leave it open. L, A and C draw lines, arcs and circles. Then E extrudes it.",
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
    /// The line new walls are drawn along, and which side their exterior
    /// is on (the right of the direction drawn, or flipped its left).
    pub wall_location: creabim_core::LocationLine,
    pub wall_flip: bool,
    /// Section line handle being dragged, and the line when the drag began.
    drag: Option<(Handle, DVec2, DVec2, DVec2)>,
    /// Dimension being drawn: its points so far, attached and as drawn.
    dim: Vec<(HostPoint, DVec2)>,
    /// Element picked for a tag, waiting for the tag's position.
    tag_host: Option<ElementId>,
    /// Tag or dimension being dragged: where the drag began and its offset then.
    anno_drag: Option<(ElementId, DVec2, DVec2)>,
    /// Angles, radii, arc lengths and notes being placed or dragged.
    pub anno: crate::annotate::Draft,
    /// A keyboard move, rotate or scale, in whichever pane started it.
    pub modal: Option<Modal>,
    /// A sketch being drawn, in whichever pane started it.
    pub sketch: Option<SketchDraft>,
    /// A handle or selected element being dragged in a plan or section.
    grip: Option<handles::Drag>,
    /// The element open in Edit mode (see [`crate::editmode`]): all its
    /// grips show, in every view.
    pub edit: Option<ElementId>,
    /// Snaps for each 2D view: built from its drawing without the listed
    /// elements, at a model revision.
    snaps: HashMap<ElementId, (u64, Vec<ElementId>, Snapper)>,
    /// A message for the status bar, taken by the app each frame.
    pub status: Option<String>,
    /// The edge a guide is drawn parallel to, picked by its first click.
    pub guide_edge: Option<(DVec2, DVec2)>,
    /// A slab outline being drawn, or a slab outline or wall path being
    /// redrawn.
    pub outline: Option<crate::outline::Outline>,
    /// How the Slab tool draws.
    pub slab_mode: crate::slabtool::SlabMode,
    /// The next side of a slab outline is an arc.
    pub slab_arc: bool,
    /// A new slab thickness keeps the underside where it is (the top
    /// moves) instead of the top.
    pub slab_keep_bottom: bool,
    /// Where a box selection began, on screen.
    pub boxsel: Option<Pos2>,
    /// What rotate and scale turn about.
    pub pivot: modal::Pivot,
    /// What the Move tool last worked on: whole elements or a face.
    pub move_target: modal::Target,
    /// The pivot cursor, placed with Shift+right-click.
    pub cursor: Option<DVec3>,
    /// A move, rotate or scale picked from a button or menu, started by the
    /// next view the pointer is over: (mode, copy first).
    pub armed: Option<(Mode, bool)>,
    /// Commands picked from a view's right-click menu, run by the app, with
    /// the model point the menu was opened at.
    pub commands: Vec<(Command, Option<DVec3>)>,
    /// Where the right-click menu was opened, in the model.
    pub menu_at: Option<DVec3>,
    /// Menus are drawn by the web interface: a right-click asks for one at
    /// this point on screen instead of opening egui's.
    pub web_menus: bool,
    pub menu_request: Option<egui::Pos2>,
    /// Which snaps are on, in every pane and the 3D view.
    pub snap: snaps::SnapSettings,
    /// The grid system the Grid tool puts new lines in.
    pub grid_group: Option<ElementId>,
    /// A level being dragged in a section or elevation, and how far above
    /// the pointer its line was when the drag began.
    level_drag: Option<(ElementId, f64)>,
    /// Sizes for new columns, beams, roofs, stairs and railings, and a
    /// railing path being clicked.
    pub members: crate::memberstool::MemberSettings,
    /// The Level tool was used where level lines can't be drawn: the app
    /// opens the Levels window instead.
    pub want_levels: bool,
    /// What the Object tool places.
    pub objects: crate::objecttool::ObjectSettings,
    /// The Object tool was used with nothing picked: the app opens the
    /// Catalogue window.
    pub want_catalogue: bool,
    /// The points clicked so far for a chain of walls (or beams, grid
    /// lines...), on a level, for alignment tracking.
    pub chain: (Option<ElementId>, Vec<DVec2>),
    /// Points the pointer rested on while drawing in a view, tracked like
    /// the shape's own points, and the point it is resting on (since when).
    rested: (Option<ElementId>, Vec<DVec2>),
    rest: Option<(DVec2, f64)>,
    /// Snap references in each view: points and edges the pointer rested
    /// on, that temporary guide lines run from. They stay while you work,
    /// until rested on again, cleared (Esc with nothing to cancel), or kept.
    pub refs: HashMap<ElementId, Vec<snaps::Reference>>,
    /// What the pointer is resting on for a reference: since when, and
    /// whether that rest has already picked (or dropped) it.
    ref_rest: Option<(snaps::Reference, f64, bool)>,
    /// Angled guides every this many degrees through references and the
    /// start point; 0 for none.
    pub guide_step: f64,
    /// The temporary guide under the pointer in the pane it is over, in
    /// that pane's drawing: what K keeps as a guide.
    pub hover_guide: Option<(ElementId, snaps::TempGuide)>,
    /// The Dimension tool's axis; none picks it from where the line goes.
    pub dim_axis: Option<creabim_core::DimAxis>,
    /// The axis a dimension is held to while Shift is down.
    dim_lock: Option<creabim_core::DimAxis>,
    /// The Align tool's picks so far.
    pub align: crate::aligntool::AlignState,
    /// What the pointer is over before a click, and what else is under it.
    pub hover: Option<crate::hoverpick::HoverPick>,
    /// The drawing views seen in X-ray while working: what holds others
    /// see-through, what is inside them drawn and picked first.
    pub xray: HashSet<ElementId>,
    /// Each view's model as its view rays pick from, with the revision it
    /// was built at.
    pick_scenes: HashMap<ElementId, (u64, Arc<Vec<SceneItem>>)>,
    /// The type each tool draws new elements as.
    pub types: HashMap<creabim_core::TypeCategory, ElementId>,
}

impl PlanState {
    /// `e`, made an instance of the type its tool draws (if there is one
    /// and it fits).
    pub fn typed(&self, model: &creabim_core::Model, mut e: Element) -> Element {
        let t = creabim_core::TypeCategory::of(&e.kind).and_then(|c| self.types.get(&c));
        if let Some(t) = t {
            creabim_core::types::stamp(model, &mut e, *t);
        }
        e
    }
}

impl Default for PlanState {
    fn default() -> Self {
        Self {
            cameras: HashMap::new(),
            tool: Tool::Select,
            pending: None,
            wall_thickness: 0.2,
            wall_location: Default::default(),
            wall_flip: false,
            drag: None,
            dim: vec![],
            tag_host: None,
            anno_drag: None,
            anno: Default::default(),
            modal: None,
            sketch: None,
            grip: None,
            edit: None,
            snaps: HashMap::new(),
            status: None,
            guide_edge: None,
            outline: None,
            slab_mode: Default::default(),
            slab_arc: false,
            slab_keep_bottom: false,
            boxsel: None,
            pivot: modal::Pivot::default(),
            move_target: modal::Target::default(),
            cursor: None,
            armed: None,
            commands: vec![],
            menu_at: None,
            web_menus: false,
            menu_request: None,
            snap: snaps::SnapSettings::default(),
            grid_group: None,
            level_drag: None,
            members: Default::default(),
            want_levels: false,
            objects: Default::default(),
            want_catalogue: false,
            chain: (None, vec![]),
            rested: (None, vec![]),
            rest: None,
            refs: HashMap::new(),
            ref_rest: None,
            guide_step: 45.0,
            hover_guide: None,
            dim_axis: None,
            dim_lock: None,
            align: Default::default(),
            hover: None,
            xray: HashSet::new(),
            pick_scenes: HashMap::new(),
            types: HashMap::new(),
        }
    }
}

impl PlanState {
    /// Drop any half-finished tool input.
    pub fn cancel(&mut self) {
        self.pending = None;
        self.dim.clear();
        self.anno.clear();
        self.tag_host = None;
        self.sketch = None;
        self.guide_edge = None;
        self.outline = None;
        self.members.path.clear();
        self.chain.1.clear();
        self.rested.1.clear();
        self.align.clear();
    }

    /// Is a tool part way through, with clicks to undo?
    pub fn has_input(&self) -> bool {
        self.pending.is_some()
            || !self.dim.is_empty()
            || self.anno.has_input()
            || self.tag_host.is_some()
            || self.sketch.is_some()
            || self.guide_edge.is_some()
            || self.outline.is_some()
            || self.armed.is_some()
            || !self.members.path.is_empty()
            || self.align.has_input()
    }

    /// Drop every snap reference, and the guides they make.
    pub fn clear_references(&mut self) -> usize {
        let n = self.refs.values().map(Vec::len).sum();
        self.refs.clear();
        self.ref_rest = None;
        n
    }

    /// Esc or right-click: drop what the tool has so far, or else leave the
    /// tool for Select. False when there was nothing to step back from.
    pub fn step_back(&mut self) -> bool {
        if self.has_input() {
            self.cancel();
            self.armed = None;
            self.status = Some("Cancelled. Right-click or Esc again to leave the tool.".into());
            true
        } else if self.clear_references() > 0 {
            self.status = Some("Guide lines cleared. Esc again leaves the tool.".into());
            true
        } else if self.edit.take().is_some() {
            self.status = Some("Finished editing.".into());
            true
        } else if self.tool != Tool::Select {
            self.tool = Tool::Select;
            self.status = Some("Back to Select.".into());
            true
        } else {
            false
        }
    }

    /// Is a tool waiting for more clicks?
    pub fn busy(&self) -> bool {
        self.pending.is_some()
            || !self.dim.is_empty()
            || self.anno.has_input()
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
        (PaneKind::Axonometric, Tool::Note) => Tool::Note,
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
    // the slabs of a plan's level: a light fill under the rest, stronger
    // where the pointer is, so they can be seen and clicked
    let slabs = match kind {
        PaneKind::Plan { level } => level_slabs(doc.model(), level, plan),
        _ => vec![],
    };
    // pre-selection: what a click here would take, and what else is
    // under the pointer (Tab cycles them)
    let pane = Pane::View(view);
    if !response.dragged() {
        let reading = matches!(tool, Tool::Select | Tool::Tag)
            && state.modal.is_none()
            && state.boxsel.is_none();
        let pointer = response.hover_pos().filter(|_| reading);
        let tol = 6.0 / xf.scale as f64;
        let rev = doc.model().revision();
        let mut hover = state.hover.take();
        let said = hoverpick::track(ui.ctx(), &mut hover, pane, pointer, rev, doc.model(), |p| {
            under_pointer(state, doc, view, kind, plan, xf.to_world(p), tol, mm)
        });
        state.hover = hover;
        if said.is_some() {
            state.status = said;
        }
    }
    let hovered = hoverpick::highlighted(&state.hover)
        .filter(|id| !selection.contains(*id) && !response.dragged());
    let hovered_slab = hovered.filter(|id| slabs.iter().any(|(s, _)| s == id));
    for (id, points) in &slabs {
        let fill = if selection.contains(*id) {
            SLAB_SELECTED
        } else if hovered_slab == Some(*id) {
            SLAB_HOVER
        } else {
            SLAB_FILL
        };
        fill_polygon(&painter, &xf, points, fill);
    }
    let xray = state.xray.contains(&view);
    for item in &plan.items {
        let selected = selection.contains(item.id());
        if xray {
            draw_item_xray(&painter, &xf, item, selected, mm, doc.model());
        } else {
            draw_item(&painter, &xf, item, selected, mm);
        }
    }
    if let Some(id) = hovered {
        draw_hover(&painter, &xf, &plan.items, id);
    }
    if let Some(id) = hoverpick::badge(ui.ctx(), &mut state.hover, pane, doc.model()) {
        if ui.input(|i| i.modifiers.shift) {
            selection.toggle(id);
        } else {
            selection.set(Some(id));
        }
        state.status = Some(format!(
            "{} selected.",
            hoverpick::describe(doc.model(), id)
        ));
    }
    if let Some((_, points)) = slabs.iter().find(|(id, _)| Some(*id) == hovered_slab) {
        // its edge over the walls that sit on it
        let pts: Vec<Pos2> = points.iter().map(|p| xf.to_screen(*p)).collect();
        painter.add(egui::Shape::closed_line(pts, Stroke::new(2.0, SLAB_EDGE)));
    }
    crate::annotate::draw_grips(&painter, &xf, &plan.items, selection);
    crate::sketch::draw_marks_in_pane(&painter, doc.model(), selection.active, kind, &xf);
    if tool == Tool::Align {
        let model = doc.model();
        let hover = response
            .hover_pos()
            .and_then(|p| {
                crate::aligntool::hover_edge(
                    model,
                    &state.align,
                    &plan.items,
                    xf.to_world(p),
                    6.0 / xf.scale as f64,
                )
            })
            .map(|(_, a, b)| (a, b));
        crate::aligntool::draw_picks(&painter, &xf, &state.align, view, hover);
    }

    let (shift, alt) = ui.input(|i| (i.modifiers.shift, i.modifiers.alt));
    // holding Alt picks free of snaps
    let snap_cfg = state.snap.live(alt);
    // snaps: the selection and what joins it are left out while selecting,
    // since dragging them must not snap to themselves
    let moving = state
        .modal
        .as_ref()
        .filter(|m| m.pane == Some(view))
        .and_then(|m| m.snap_exclude(doc));
    let exclude = match (moving, tool) {
        (Some(ids), _) => ids,
        (None, Tool::Select) => selection
            .active
            .map(|id| joined_with(doc, id))
            .unwrap_or_default(),
        _ => vec![],
    };
    if matches!(kind, PaneKind::Plan { .. } | PaneKind::Section { .. }) {
        refresh_snaps(state, view, doc.model().revision(), plan, &exclude);
    }
    let pane_project = |p: DVec3| Some(xf.to_screen(p.truncate()));
    // a line starts from the tool's first click, or the sketch's last corner
    let sketch_last = state
        .sketch
        .as_ref()
        .filter(|d| d.pane == Some(view))
        .and_then(|d| Some(d.plane.to_model(*d.points.last()?)));
    let from_pane = |m: DVec3| match kind {
        PaneKind::Section { start, end } => model_to_pane(Space::Section { start, end }, m),
        _ => m.truncate(),
    };
    let last = state
        .outline
        .as_ref()
        .and_then(|o| o.points.last().copied());
    let drawing_from = state
        .pending
        .or(sketch_last.map(from_pane))
        .or(last)
        .map(|a| a.extend(0.0));
    // temporary guide lines: from the points and edges rested on in the
    // drawing tools, and through where the line being drawn starts
    let guided = matches!(kind, PaneKind::Plan { .. } | PaneKind::Section { .. })
        && tool != Tool::Select
        && snap_cfg.allows(SnapKind::OnTempGuide);
    let guide_from = drawing_from.or(state.dim.first().map(|d| d.1.extend(0.0)));
    let guides = if guided {
        let refs = state.refs.get(&view).map_or(&[][..], |r| r.as_slice());
        snaps::temp_guides(refs, guide_from, state.guide_step)
    } else {
        vec![]
    };
    // the points of the shape being drawn here, for the next to line up with
    let track = {
        let shape = shape_points(state, view, kind, &from_pane);
        // (with guide lines on, points rested on make guides instead)
        let rested: Vec<DVec3> = match (&state.rested, drawing_from) {
            ((Some(v), pts), Some(_)) if *v == view && !guided => {
                pts.iter().map(|p| p.extend(0.0)).collect()
            }
            _ => vec![],
        };
        let shape: Vec<DVec3> = shape.iter().map(|p| p.extend(0.0)).collect();
        snaps::track_points(&shape, &rested)
    };
    let tracked = response.hover_pos().and_then(|p| {
        let snapper = &state.snaps.get(&view)?.2;
        snapper.snap_guided(
            &snap_cfg,
            p,
            &pane_project,
            drawing_from,
            &track,
            &guides,
            Some(xf.to_world(p).extend(0.0)),
        )
    });
    let tool_snap = tracked.as_ref().map(|t| t.snap);
    let tool_align = tracked.as_ref().and_then(|t| t.align);
    let now = ui.input(|i| i.time);
    if guided {
        // rest on a point or edge (or press Q over it) to pick it as a
        // reference, and again to drop it
        let under = response.hover_pos().and_then(|p| {
            let snapper = &state.snaps.get(&view)?.2;
            let s = snapper.snap(&snap_cfg, p, &pane_project, None, None)?;
            snapper.reference_at(&s)
        });
        let q = response.hovered()
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|i| i.key_pressed(egui::Key::Q) && !i.modifiers.command);
        if let Some(msg) = rest_reference(state, view, under, now, q) {
            state.status = Some(msg);
        }
    } else {
        // a point snapped to and rested on while drawing is tracked too
        rest_on(state, view, tool_snap, drawing_from.is_some(), now);
    }
    if guided {
        // the references, and their guides faintly; those a snap follows
        // are drawn again with it
        if let Some(refs) = state.refs.get(&view) {
            snaps::draw_references(&painter, &pane_project, refs);
        }
        snaps::draw_guides(&painter, &pane_project, &guides, &[]);
        if response.hovered() {
            let on = tracked.as_ref().and_then(|t| t.guides.first().copied());
            state.hover_guide = on.and_then(|i| guides.get(i)).map(|g| (view, *g));
        }
        // K keeps the guide line under the pointer
        if response.hovered()
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|i| i.key_pressed(egui::Key::K) && !i.modifiers.command)
        {
            let g = state.hover_guide.filter(|h| h.0 == view).map(|h| h.1);
            state.status = Some(match keep_guide(doc, kind, g) {
                Ok(msg) => msg,
                Err(e) => format!("Couldn't do that: {e}"),
            });
        }
    }
    let hover_world = response.hover_pos().map(|p| match kind {
        PaneKind::Plan { .. } | PaneKind::Section { .. } if !shift => {
            tool_snap.map_or(xf.to_world(p), |s| s.p.truncate())
        }
        PaneKind::Plan { level } => snap(
            doc,
            level,
            &xf,
            xf.to_world(p),
            state.pending,
            shift,
            &[],
            &snap_cfg,
        ),
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
            if snap_cfg.allows(SnapKind::Grid) {
                (p / 0.05).round() * 0.05
            } else {
                p
            }
        }
    });

    // the Slab tool's modes, and Line / Arc while clicking a slab outline
    if let PaneKind::Plan { level } = kind {
        let slab_outline = state
            .outline
            .as_ref()
            .is_some_and(|o| o.level == level && !o.is_wall());
        if tool.draws_outline() || slab_outline {
            let modes = tool.draws_outline();
            if let Some(msg) =
                crate::slabtool::controls(ui, rect, Some(view), modes, doc, state, selection)
            {
                state.status = Some(msg);
            }
        }
    }

    // drawing a slab, or redrawing a slab or wall, on this plan's level
    if let (Some(o), PaneKind::Plan { level }) = (&state.outline, kind) {
        if o.level == level {
            let o = o.clone();
            let project = |p: DVec3| Some(xf.to_screen(p.truncate()));
            if let (Some(t), false) = (&tracked, shift) {
                let from = o.points.last().map(|a| xf.to_screen(*a));
                snaps::draw_guides(&painter, &project, &guides, &t.guides);
                snaps::draw_guided(&painter, &project, t, &guides, from);
            }
            o.draw(&painter, &project, hover_world);
            // keys go to the pane under the pointer, as another pane may
            // show the same outline
            let here = response.hovered();
            let (enter, escape) = ui.input(|i| {
                (
                    here && i.key_pressed(egui::Key::Enter),
                    here && i.key_pressed(egui::Key::Escape),
                )
            });
            let click = match (response.clicked(), hover_world, response.hover_pos()) {
                (true, Some(p), Some(h)) => Some((p, o.near_first(h, &project))),
                _ => None,
            };
            let right = response.secondary_clicked() && !shift;
            let arc = state.slab_arc;
            let next = state
                .outline
                .as_mut()?
                .input(enter, escape, right, click, arc);
            match next {
                Next::Drawing(hint) => state.status = Some(hint),
                Next::Cancel(msg) => {
                    state.outline = None;
                    state.status = Some(msg.into());
                }
                Next::Finish(closed) => {
                    let o = state.outline.take()?;
                    return match o.finish(doc, closed) {
                        Ok((msg, made)) => {
                            if let Some(id) = made {
                                selection.only(id);
                            }
                            state.status = Some(msg);
                            None
                        }
                        Err(e) => Some(e.to_string()),
                    };
                }
            }
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
        // the pane's snaps, lent out while a move or rotate runs here
        let lent = std::mem::take(&mut state.snaps);
        let plane_z = match kind {
            PaneKind::Plan { level } => doc.model().level(level).map_or(0.0, |l| l.elevation),
            _ => 0.0,
        };
        let snap_fn = |pos: Pos2, from: Option<DVec3>| {
            let snapper = &lent.get(&view)?.2;
            let to_pane = |p: DVec3| model_to_pane(space, p).extend(0.0);
            let pane_project = |p: DVec3| Some(xf.to_screen(p.truncate()));
            let free = Some(xf.to_world(pos).extend(0.0));
            let s = snapper.snap(&snap_cfg, pos, &pane_project, from.map(to_pane), free)?;
            let p = pane_to_model(space, s.p.truncate(), from.map_or(plane_z, |f| f.z));
            Some((p, snaps::Snap { p, ..s }))
        };
        // Move on faces: the edge under the pointer stands for the face
        // through it, square to the paper
        let pushable: std::collections::HashSet<ElementId> = if state.modal.is_some() {
            let model = doc.model();
            plan.items
                .iter()
                .map(|i| i.id())
                .filter(|id| {
                    model
                        .get(*id)
                        .is_some_and(|e| creabim_core::pushpull::can_push(&e.kind))
                })
                .collect()
        } else {
            Default::default()
        };
        // the selection's own faces win over those lying on them
        let chosen = selection.all();
        let pick_face = |pos: Pos2| {
            let takes = |id: ElementId| {
                pushable
                    .contains(&id)
                    .then_some(u8::from(chosen.contains(&id)))
            };
            let px = 1.0 / xf.scale as f64;
            crate::paneface::face_at(
                &plan.items,
                space,
                xf.to_world(pos),
                6.0 * px,
                plane_z,
                5.0 * px,
                &takes,
            )
            .map(|f| f.pick)
        };
        let map = PaneMap {
            to_model: &to_model,
            project: &project,
            look: match space {
                Space::Section { start, end } => {
                    (end - start).normalize_or_zero().perp().extend(0.0)
                }
                _ => -DVec3::Z,
            },
            snap: &snap_fn,
            pick_face: Some(&pick_face),
        };
        let hover_now = state.hover.clone();
        let pick = |p: Pos2| {
            hover_now
                .as_ref()
                .and_then(|h| h.at(pane, p))
                .or_else(|| hit_test(&plan.items, xf.to_world(p), 6.0 / xf.scale as f64, mm))
        };
        let edits = keyboard_edits(
            ui, doc, view, space, &painter, &response, state, selection, &map, &pick,
        );
        state.snaps = lent;
        if let Some(r) = edits {
            return r;
        }
        if tool == Tool::Sketch {
            let level_z = match kind {
                PaneKind::Plan { level } => doc.model().level(level).map_or(0.0, |l| l.elevation),
                _ => 0.0,
            };
            let hover = hover_world.map(|w| pane_to_model(space, w, level_z));
            // a point snapped to something, or with increments off, is kept as is
            let exact = !snap_cfg.allows(SnapKind::Grid)
                || (!shift && tool_snap.is_some_and(|s| s.kind != SnapKind::Grid));
            if let Some(r) = sketch_tool(
                ui, doc, view, space, &xf, &painter, &response, state, selection, hover, shift,
                exact,
            ) {
                return r;
            }
        }
        // a level line or head in a section drags the level up and down
        if let (PaneKind::Section { .. }, Tool::Select) = (kind, tool) {
            let snapped = tool_snap.filter(|s| !shift && s.kind != SnapKind::Grid);
            if let Some(r) = drag_level(doc, plan, &xf, &response, state, selection, mm, snapped) {
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
                let s = snapper.snap(&snap_cfg, pos, &pane_project, from.map(to_pane), free)?;
                let z = from.map_or(level_z, |f| f.z);
                let p = pane_to_model(space, s.p.truncate(), z);
                Some((p, snaps::Snap { p, ..s }))
            };
            // what is under others (picked with Tab, or in X-ray) can be
            // dragged by its body too
            let hover_now = state.hover.clone();
            let grab_body = |pos: Pos2, id: ElementId| {
                let w = xf.to_world(pos);
                let on = hit_all(&plan.items, w, 6.0 / xf.scale as f64, mm).contains(&id)
                    || hover_now.as_ref().and_then(|h| h.at(pane, pos)) == Some(id);
                on.then(|| pane_to_model(space, w, level_z))
            };
            let grab_snap = |pos: Pos2, id: ElementId| {
                let only: Vec<PlanItem> = plan
                    .items
                    .iter()
                    .filter(|i| i.id() == id)
                    .cloned()
                    .collect();
                let own = Snapper::from_drawing(&only, &[]);
                let pane_project = |p: DVec3| Some(xf.to_screen(p.truncate()));
                let s = own.snap(&snap_cfg.points_only(), pos, &pane_project, None, None)?;
                Some(pane_to_model(space, s.p.truncate(), level_z))
            };
            let input = PaneInput {
                ray: &ray,
                project: &project,
                look,
                snap: &snap,
                grab_body: &grab_body,
                grab_snap: &grab_snap,
                body_normal: if matches!(space, Space::Section { .. }) {
                    look
                } else {
                    DVec3::Z
                },
            };
            let mut status = None;
            let edit = state.edit.is_some() && state.edit == selection.active;
            let busy = handles::interact(
                doc,
                &painter,
                &response,
                selection.active,
                None,
                edit,
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
    // a tag, dimension or note can be dragged into place
    if tool == Tool::Select {
        if let Some(r) = drag_annotation(doc, view, plan, &xf, &response, state, selection, mm) {
            return r;
        }
        let view_def = match doc.model().get(view).map(|e| &e.kind) {
            Some(ElementKind::View(v)) => Some(v.clone()),
            _ => None,
        };
        let leader_drag = matches!(state.anno.grab, Some(crate::annotate::Grab::Leader(_)));
        let snapped = match (&view_def, response.interact_pointer_pos(), leader_drag) {
            (Some(v), Some(p), true) => {
                let snapper = state.snaps.get(&view).map(|s| &s.2);
                let (hp, at, _) = dim_snap(doc, v, kind, snapper, &snap_cfg, &xf, p);
                Some((hp, at))
            }
            _ => None,
        };
        if let Some(r) = crate::annotate::drag(
            doc,
            view_def.as_ref(),
            &plan.items,
            &xf,
            &response,
            &mut state.anno,
            selection,
            mm,
            snapped,
        ) {
            return r;
        }
    }

    // drag on empty paper to select with a box
    if tool == Tool::Select && matches!(kind, PaneKind::Plan { .. } | PaneKind::Section { .. }) {
        if response.drag_started_by(egui::PointerButton::Primary) {
            let origin = ui.input(|i| i.pointer.press_origin());
            if let Some(p0) = origin {
                let on_item = hit_test(&plan.items, xf.to_world(p0), 6.0 / scale as f64, mm);
                if on_item.is_none() {
                    state.boxsel = Some(p0);
                }
            }
        }
        if let Some(p0) = state.boxsel {
            let now = ui.input(|i| i.pointer.latest_pos()).unwrap_or(p0);
            boxsel::draw(&painter, p0, now);
            if response.drag_stopped() || !response.dragged() {
                let parts = plan.items.iter().filter_map(|it| {
                    if creabim_geom::plan::is_underlay(it) {
                        return None;
                    }
                    let (lo, hi) = it.bounds()?;
                    Some((
                        it.id(),
                        Rect::from_two_pos(xf.to_screen(lo), xf.to_screen(hi)),
                    ))
                });
                let ids = boxsel::pick(p0, now, parts)
                    .into_iter()
                    .filter(|id| box_selectable(doc, *id));
                selection.extend(ids, shift);
                state.boxsel = None;
            }
            return None;
        }
    }

    // dimension and tag tools in progress
    let view_def = match doc.model().get(view).map(|e| &e.kind) {
        Some(ElementKind::View(v)) => Some(v.clone()),
        _ => None,
    };
    let dim_snapper = state.snaps.get(&view).map(|s| &s.2);
    let dim_guided = match (tool, &view_def, response.hover_pos()) {
        (t, Some(v), Some(p)) if t == Tool::Dimension || crate::annotate::is_tool(t) => Some(
            dim_snap_guided(doc, v, kind, dim_snapper, &snap_cfg, &xf, p, &guides),
        ),
        _ => None,
    };
    let dim_hit = dim_guided.as_ref().map(|(h, at, k, _)| (*h, *at, *k));
    let accent = Color32::from_rgb(230, 120, 30);
    if let Some((_, at, k, used)) = &dim_guided {
        let c = xf.to_screen(*at);
        match k {
            Some(k) if *k != SnapKind::Grid => {
                let from = state.dim.first().map(|d| xf.to_screen(d.1));
                let hit = snaps::Guided {
                    snap: snaps::Snap {
                        p: at.extend(0.0),
                        kind: *k,
                        host: None,
                    },
                    align: None,
                    guides: used.clone(),
                };
                snaps::draw_guides(&painter, &pane_project, &guides, used);
                snaps::draw_guided(&painter, &pane_project, &hit, &guides, from);
                if response.hovered() && !used.is_empty() {
                    state.hover_guide = guides.get(used[0]).map(|g| (view, *g));
                }
            }
            _ => {
                painter.circle_stroke(c, 5.0, Stroke::new(1.5, accent));
            }
        }
    }
    let dim_hover = dim_hit.map(|(h, at, _)| (h, at));
    // the way a linear dimension measures: as set, held with Shift, or
    // picked from where its line is going
    let dim_axis = match (state.dim.as_slice(), response.hover_pos()) {
        ([(_, a), (_, b)], Some(h)) => {
            let picked = state
                .dim_axis
                .unwrap_or_else(|| creabim_core::DimAxis::pick(*a, *b, xf.to_world(h)));
            if shift {
                *state.dim_lock.get_or_insert(picked)
            } else {
                state.dim_lock = None;
                picked
            }
        }
        _ => {
            state.dim_lock = None;
            state.dim_axis.unwrap_or_default()
        }
    };
    let dim_look = annotate::look_of(doc.model(), state.anno.dim_style, None);
    if crate::annotate::is_tool(tool) {
        let raw = response.hover_pos().map(|p| xf.to_world(p));
        crate::annotate::preview(
            &painter,
            &xf,
            tool,
            &state.anno,
            dim_hover.map(|h| h.1),
            raw,
            &plan.items,
            6.0 / scale as f64,
            mm,
        );
    }
    match (state.dim.as_slice(), dim_hover, response.hover_pos()) {
        ([(_, a)], Some((_, b)), _) if a.distance(b) > 1e-6 => {
            preview_dimension(&painter, &xf, *a, b, dim_axis, 0.0, &dim_look, mm, accent);
        }
        ([(_, a), (_, b)], _, Some(h)) => {
            let off = dim_offset(dim_axis, *a, *b, xf.to_world(h));
            preview_dimension(&painter, &xf, *a, *b, dim_axis, off, &dim_look, mm, accent);
            let what = if state.dim_lock.is_some() {
                format!("{} (held by Shift)", dim_axis.label())
            } else if state.dim_axis.is_some() {
                dim_axis.label().to_string()
            } else {
                format!("{} (Shift holds it)", dim_axis.label())
            };
            painter.text(
                h + Vec2::new(14.0, 16.0),
                egui::Align2::LEFT_TOP,
                what,
                egui::FontId::proportional(12.0),
                accent,
            );
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
                        look: annotate::look_of(doc.model(), None, None),
                    };
                    draw_item(&painter, &xf, &item, true, mm);
                }
            }
        }
    }

    // the object about to be placed, under the pointer
    if let (Tool::Object, PaneKind::Plan { level }, Some(h)) = (tool, kind, hover_world) {
        if ui.input(|i| i.key_pressed(egui::Key::Space)) && !ui.ctx().egui_wants_keyboard_input() {
            state.objects.rotation = (state.objects.rotation + 90.0).rem_euclid(360.0);
        }
        crate::objecttool::preview(
            &painter,
            &|p| Some(xf.to_screen(p.truncate())),
            doc.model(),
            level,
            h,
            state,
            false,
        );
    }
    // preview of the tool in progress
    if let (Some(a), Some(b)) = (state.pending, hover_world) {
        let accent = Stroke::new(1.5, Color32::from_rgb(230, 120, 30));
        match tool {
            Tool::Level => {
                let end = DVec2::new(b.x, a.y);
                painter.extend(egui::Shape::dashed_line(
                    &[xf.to_screen(a), xf.to_screen(end)],
                    accent,
                    10.0,
                    4.0,
                ));
                painter.text(
                    xf.to_screen(end) + Vec2::new(8.0, -6.0),
                    egui::Align2::LEFT_BOTTOM,
                    crate::units::elevation(a.y),
                    egui::FontId::proportional(13.0),
                    accent.color,
                );
            }
            Tool::Wall
            | Tool::Section
            | Tool::Elevation
            | Tool::Grid
            | Tool::Beam
            | Tool::Stair
            | Tool::Railing => {
                let level = match kind {
                    PaneKind::Plan { level } => Some(level),
                    _ => None,
                };
                crate::memberstool::preview(
                    &painter,
                    &|p| Some(xf.to_screen(p.truncate())),
                    doc.model(),
                    level,
                    tool,
                    state,
                    a.extend(0.0),
                    b.extend(0.0),
                );
                painter.line_segment([xf.to_screen(a), xf.to_screen(b)], accent);
                if tool == Tool::Wall && matches!(kind, PaneKind::Plan { .. }) {
                    // the wall's faces about the line it is drawn along
                    if let Some(faces) = wall_preview(doc.model(), state, a, b) {
                        let thin = Stroke::new(1.0, accent.color);
                        for (p, q) in faces {
                            painter.line_segment([xf.to_screen(p), xf.to_screen(q)], thin);
                        }
                    }
                }
                let len = a.distance(b);
                painter.text(
                    xf.to_screen((a + b) / 2.0) + Vec2::new(0.0, -12.0),
                    egui::Align2::CENTER_BOTTOM,
                    crate::units::len(len),
                    egui::FontId::proportional(13.0),
                    accent.color,
                );
            }
            Tool::Slab | Tool::Roof | Tool::Shaft => {
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
                        (b, d, crate::units::len(off))
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
    if let (Some(s), PaneKind::Plan { .. } | PaneKind::Section { .. }, false) =
        (tool_snap, kind, shift)
    {
        if tool != Tool::Select && !tool.is_annotation() {
            let from = state
                .pending
                .or(sketch_last.map(from_pane))
                .map(|a| xf.to_screen(a));
            let at = xf.to_screen(s.p.truncate());
            match &tracked {
                Some(t) if !t.guides.is_empty() => {
                    snaps::draw_guides(&painter, &pane_project, &guides, &t.guides);
                    snaps::draw_guided(&painter, &pane_project, t, &guides, from);
                }
                _ => snaps::draw_tracked(&painter, &pane_project, s, tool_align, at, from),
            }
        }
    }
    if let Some(p) = hover_world {
        if tool != Tool::Select && !tool.is_annotation() {
            painter.circle_stroke(
                xf.to_screen(p),
                4.0,
                Stroke::new(1.0, Color32::from_rgb(230, 120, 30)),
            );
        }
    }

    // Shift+right-click places the pivot cursor
    if let (Some(space), true, true, Some(p)) = (
        space,
        response.secondary_clicked(),
        shift,
        response.interact_pointer_pos(),
    ) {
        let level_z = match kind {
            PaneKind::Plan { level } => doc.model().level(level).map_or(0.0, |l| l.elevation),
            _ => 0.0,
        };
        let w = hover_world.unwrap_or_else(|| xf.to_world(p));
        let c = pane_to_model(space, w, level_z);
        state.cursor = Some(c);
        state.pivot = modal::Pivot::Cursor;
        state.status = Some(format!(
            "Pivot cursor at {:.2}, {:.2}, {:.2}. Rotate and scale now turn about it.",
            c.x, c.y, c.z
        ));
        return None;
    }
    if let (Some(space), Some(c)) = (space, state.cursor) {
        draw_cursor(&painter, xf.to_screen(model_to_pane(space, c)));
    }
    let right = response.secondary_clicked() && !shift;
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !ui.ctx().egui_wants_keyboard_input() {
        state.step_back();
    }
    let menu = right && !state.step_back() && tool == Tool::Select;
    if menu {
        // right-click on something not yet selected selects it first
        if let Some(p) = response.interact_pointer_pos() {
            let raw = xf.to_world(p);
            let hit = state
                .hover
                .as_ref()
                .and_then(|h| h.at(pane, p))
                .or_else(|| hit_test(&plan.items, raw, 6.0 / scale as f64, mm))
                .or_else(|| ray_pick(doc, view, kind, raw));
            if let Some(id) = hit.filter(|id| !selection.contains(*id)) {
                selection.only(id);
            }
            let level_z = match kind {
                PaneKind::Plan { level } => doc.model().level(level).map_or(0.0, |l| l.elevation),
                _ => 0.0,
            };
            let w = hover_world.unwrap_or(raw);
            state.menu_at = space.map(|space| pane_to_model(space, w, level_z));
        }
    }
    if tool == Tool::Select && state.modal.is_none() && state.web_menus {
        if menu {
            state.menu_request = response.interact_pointer_pos();
        }
    } else if tool == Tool::Select && state.modal.is_none() {
        let n = selection.len();
        let at = state.menu_at;
        response.context_menu(|ui| {
            if let Some(c) = crate::commands::context_menu(ui, n) {
                match c.modal() {
                    Some(m) => state.armed = Some(m),
                    None => state.commands.push((c, at)),
                }
            }
        });
    }
    if let (PaneKind::Section { .. }, Some(a), Some(b)) = (kind, state.pending, hover_world) {
        if matches!(tool, Tool::Section | Tool::Elevation) {
            preview_cross_section(&painter, &xf, a, b, accent);
        }
    }

    let mut error = None;
    if response.clicked() {
        let p = hover_world?;
        let at = response.interact_pointer_pos().unwrap_or_default();
        let raw = xf.to_world(at);
        // what is pre-selected there (Tab may have moved past the first);
        // else linework first (edges, symbols), then what the view ray hits
        let hover_pick = state.hover.as_mut().and_then(|h| {
            h.list = false;
            h.at(pane, at)
        });
        let picked = || {
            hover_pick
                .or_else(|| hit_test(&plan.items, raw, 6.0 / scale as f64, mm))
                .or_else(|| ray_pick(doc, view, kind, raw))
        };
        let result = match (kind, tool) {
            (_, Tool::Select) => {
                let under = picked();
                // a double-click on the selected element opens it in Edit mode
                if response.double_clicked()
                    && !shift
                    && under.is_some()
                    && under == selection.active
                {
                    if let Some(id) = under {
                        state.status =
                            Some(crate::editmode::enter(doc.model(), id, &mut state.edit));
                    }
                    return None;
                }
                match (under, shift) {
                    (Some(id), true) => selection.toggle(id),
                    (None, true) => {}
                    (id, false) => selection.set(id),
                }
                Ok(())
            }
            (_, Tool::Dimension) => match (dim_hover, &view_def) {
                (Some(hp), Some(_)) => {
                    dimension_tool(doc, view, hp, raw, dim_axis, state, selection)
                }
                _ => Ok(()),
            },
            (_, Tool::Align) => crate::aligntool::click(
                doc,
                view,
                kind,
                &plan.items,
                raw,
                tool_snap.filter(|_| !shift),
                6.0 / scale as f64,
                mm,
                state,
                selection,
            ),
            (_, Tool::Tag) => tag_tool(doc, view, &view_def, picked(), raw, state, selection),
            (_, t) if crate::annotate::is_tool(t) => match &view_def {
                Some(v) => crate::annotate::click(
                    doc,
                    view,
                    v,
                    t,
                    dim_hit,
                    raw,
                    &plan.items,
                    6.0 / scale as f64,
                    &mut state.anno,
                    selection,
                )
                .map(|msg| {
                    if !msg.is_empty() {
                        state.status = Some(msg);
                    }
                }),
                None => Ok(()),
            },
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
                    "Click to select, Shift+click to add, drag on empty paper for a box. Drag grips to stretch; double-click a slab edge grip to add a corner. M move, G grab, R rotate, S scale, Shift+D duplicate."
                }
                (PaneKind::Plan { .. }, t) => t.hint(),
                (PaneKind::Section { .. }, t) => t.section_hint(),
                (PaneKind::Axonometric, Tool::Tag) => Tool::Tag.hint(),
                (PaneKind::Axonometric, Tool::Note) => Tool::Note.hint(),
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

/// Underlays (another level traced under a plan) are drawn in this colour.
pub const UNDERLAY_COLOR: Color32 = Color32::from_rgb(178, 184, 196);

/// Drag a level's line or head in a section or elevation: the level (and
/// everything on it) follows the pointer up and down, onto model points it
/// snaps to, else to the nearest centimetre (half inch). Returns `None`
/// when the pointer isn't dragging a level.
#[allow(clippy::too_many_arguments)]
fn drag_level(
    doc: &mut Document,
    plan: &Drawing,
    xf: &Xf,
    response: &egui::Response,
    state: &mut PlanState,
    selection: &mut Selection,
    mm: f64,
    snapped: Option<snaps::Snap>,
) -> Option<Option<String>> {
    if response.drag_started_by(egui::PointerButton::Primary) {
        let press = response.ctx.input(|i| i.pointer.press_origin())?;
        let w = xf.to_world(press);
        let id = hit_test(&plan.items, w, 6.0 / xf.scale as f64, mm)?;
        let l = doc.model().level(id)?;
        state.level_drag = Some((id, l.elevation - w.y));
        selection.only(id);
    }
    let (id, off) = state.level_drag?;
    if response.drag_stopped() || !response.dragged() {
        state.level_drag = None;
        return Some(None);
    }
    let p = response.interact_pointer_pos()?;
    let step = if crate::units::get().is_imperial() {
        0.0127
    } else {
        0.01
    };
    let z = match snapped {
        Some(s) => s.p.y,
        None => ((xf.to_world(p).y + off) / step).round() * step,
    };
    let name = doc
        .model()
        .get(id)
        .map(|e| e.name.clone())
        .unwrap_or_default();
    let r = doc.edit_merged(id, "Move level", |e| {
        if let ElementKind::Level(l) = &mut e.kind {
            l.elevation = z;
        }
    });
    state.status = Some(format!(
        "{name} at {}. Everything on it moves with it.",
        crate::units::elevation(z)
    ));
    Some(r.err().map(|e| e.to_string()))
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
/// The points of a chain of clicks (walls, beams) are kept for alignment
/// tracking while it goes on.
pub fn plan_tool(
    doc: &mut Document,
    level: ElementId,
    tool: Tool,
    p: DVec2,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), creabim_core::ModelError> {
    let started = state.pending.is_none();
    let r = plan_click(doc, level, tool, p, state, selection);
    match state.pending {
        Some(q) => {
            if started || state.chain.0 != Some(level) {
                state.chain = (Some(level), vec![]);
            }
            if state.chain.1.last() != Some(&q) {
                state.chain.1.push(q);
            }
        }
        None => state.chain.1.clear(),
    }
    r
}

/// A wall as the Wall tool draws it between two clicks: along its location
/// line, the exterior to the right of the direction drawn unless flipped.
pub fn new_wall(
    model: &creabim_core::Model,
    level: ElementId,
    state: &PlanState,
    a: DVec2,
    b: DVec2,
) -> Wall {
    let height = model.level(level).map_or(3.0, |l| l.height);
    let mut w = Wall::new(level, a, b, state.wall_thickness, height);
    w.location = state.wall_location;
    // the wall type in use sets its thickness, location line and grid
    let typed = state.typed(model, Element::new("Wall", ElementKind::Wall(w.clone())));
    if let ElementKind::Wall(tw) = typed.kind {
        w = tw;
    }
    w.flip = state.wall_flip;
    creabim_core::walls::set_location_line(model, &mut w, a, b);
    w
}

/// The faces of the wall being drawn from `a` to `b`, and where its
/// exterior is, for the preview.
pub fn wall_preview(
    model: &creabim_core::Model,
    state: &PlanState,
    a: DVec2,
    b: DVec2,
) -> Option<[(DVec2, DVec2); 3]> {
    if a.distance(b) < 1e-6 {
        return None;
    }
    let w = new_wall(model, ElementId::new(), state, a, b);
    let n = w.direction().perp() * (w.thickness / 2.0);
    let out = if w.flip { n } else { -n };
    let mid = (w.start + w.end) / 2.0;
    Some([
        (w.start + n, w.end + n),
        (w.start - n, w.end - n),
        // a tick from the middle out of the exterior face
        (mid + out, mid + out * 2.5),
    ])
}

fn plan_click(
    doc: &mut Document,
    level: ElementId,
    tool: Tool,
    p: DVec2,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), creabim_core::ModelError> {
    match tool {
        Tool::Select | Tool::Align => Ok(()),
        Tool::Level => {
            state.status = Some(
                "Level lines are drawn in a section or elevation. Here, add, rename and move levels in the Levels window.".into(),
            );
            state.want_levels = true;
            Ok(())
        }
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
                let w = new_wall(doc.model(), level, state, a, p);
                let e = state.typed(doc.model(), Element::new("Wall", ElementKind::Wall(w)));
                selection.only(e.id);
                state.pending = Some(p);
                doc.commit(Transaction::new("Draw wall").insert(e))
            }
            Some(_) => Ok(()),
        },
        Tool::Door | Tool::Window | Tool::Opening => {
            place_opening(doc, level, p, tool, state, selection)
        }
        Tool::Dimension
        | Tool::Tag
        | Tool::Angle
        | Tool::Radius
        | Tool::Diameter
        | Tool::ArcLength
        | Tool::Note
        | Tool::Sketch => Ok(()),
        Tool::Slab | Tool::Roof | Tool::Shaft => {
            crate::slabtool::click(doc, level, p, state, selection)
        }
        Tool::Column | Tool::Beam | Tool::Stair | Tool::Railing => {
            crate::memberstool::click(doc, level, tool, p, state, selection)
        }
        Tool::Object => crate::objecttool::click(doc, level, p, state, selection),
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
        Tool::Grid => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) if a.distance(p) > 0.1 => {
                let e = creabim_core::grids::new_grid(doc.model(), state.grid_group, a, p);
                state.status = Some(format!("Added {}. Click again for the next line.", e.name));
                selection.only(e.id);
                doc.commit(Transaction::new("Add grid line").insert(e))
            }
            Some(a) => {
                state.pending = Some(a);
                Ok(())
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
    let scene = pick_scene(doc, view);
    ray_pick_all(doc.model(), &scene, view, kind, p, None)
        .first()
        .copied()
}

/// The model view `view` picks from: what it leaves out is not in it.
fn pick_scene(doc: &Document, view: ElementId) -> Vec<SceneItem> {
    // what the view leaves out can't be clicked in it, and isn't in the
    // model its drawing was made from
    let (hidden, axo) = match doc.model().get(view).map(|e| &e.kind) {
        Some(ElementKind::View(v)) => (
            v.hidden.hidden_set(doc.model()),
            matches!(v.kind, creabim_core::ViewKind::Axonometric { .. }),
        ),
        _ => Default::default(),
    };
    let model = creabim_core::visibility::without(doc.model(), &hidden);
    let mut scene = creabim_geom::build_scene(&model);
    scene.retain(|s| !hidden.contains(&s.id));
    // a 3D view's crop boxes cut its drawing, and what they cut away
    // can't be clicked either
    if axo {
        let boxes = creabim_geom::cropbox::active(doc.model(), Some(view));
        creabim_geom::cropbox::crop_scene(doc.model(), &mut scene, &boxes);
    }
    scene
}

/// [`pick_scene`], kept until the model changes.
fn cached_pick_scene(
    state: &mut PlanState,
    doc: &Document,
    view: ElementId,
) -> Arc<Vec<SceneItem>> {
    let rev = doc.model().revision();
    match state.pick_scenes.get(&view) {
        Some((r, s)) if *r == rev => s.clone(),
        _ => {
            let s = Arc::new(pick_scene(doc, view));
            state.pick_scenes.insert(view, (rev, s.clone()));
            s
        }
    }
}

/// Every element the viewing ray through drawing point `p` passes, nearest
/// first. In a plan the ray looks down through the level, and only what
/// the plan draws (`drawn`) counts.
fn ray_pick_all(
    model: &creabim_core::Model,
    scene: &[SceneItem],
    view: ElementId,
    kind: PaneKind,
    p: DVec2,
    drawn: Option<&HashSet<ElementId>>,
) -> Vec<ElementId> {
    let (o, d) = match kind {
        PaneKind::Plan { level } => {
            // a click inside a floor picks its slab
            let slabs: Vec<(ElementId, Vec<DVec2>)> = model
                .iter()
                .filter_map(|e| match &e.kind {
                    ElementKind::Slab(s) if s.level == level => Some((e.id, s.outline())),
                    _ => None,
                })
                .collect();
            let slab = slab_at(&slabs, p);
            let Some(drawn) = drawn else {
                return slab.into_iter().collect();
            };
            let z = model.level(level).map_or(0.0, |l| l.elevation);
            let o = DVec3::new(p.x, p.y, z + 100.0);
            let mut out: Vec<ElementId> =
                creabim_geom::pick::pick_all(scene, o.as_vec3(), -glam::Vec3::Z)
                    .into_iter()
                    .filter(|h| drawn.contains(&h.id) && o.z - h.t as f64 > z - 1.0)
                    .map(|h| h.id)
                    .collect();
            if let Some(s) = slab.filter(|s| !out.contains(s)) {
                out.push(s);
            }
            return out;
        }
        PaneKind::Schedule => return vec![],
        PaneKind::Section { start, end } => {
            let look = (end - start).normalize_or_zero().perp();
            let xy = PaneKind::section_point(start, end, p);
            (DVec3::new(xy.x, xy.y, p.y), look.extend(0.0))
        }
        PaneKind::Axonometric => {
            let Some(ElementKind::View(View {
                kind: ViewKind::Axonometric { azimuth, altitude },
                ..
            })) = model.get(view).map(|e| &e.kind)
            else {
                return vec![];
            };
            creabim_geom::section::AxoFrame::new(model, *azimuth, *altitude).ray(p)
        }
    };
    creabim_geom::pick::pick_all(scene, o.as_vec3(), d.as_vec3())
        .into_iter()
        .map(|h| h.id)
        .collect()
}

/// What holds others and turns see-through in X-ray: walls, slabs,
/// roofs and masses.
pub fn is_xray_host(kind: &ElementKind) -> bool {
    match kind {
        ElementKind::Wall(_) | ElementKind::Slab(_) | ElementKind::Roof(_) => true,
        ElementKind::FreeMesh(m) => !matches!(
            m.ifc_class.as_str(),
            "IfcBeam" | "IfcColumn" | "IfcMember" | "IfcStair" | "IfcRailing"
        ),
        _ => false,
    }
}

/// With X-ray on, what is inside comes before what holds it: the beam in
/// the wall is what a click takes first.
pub fn xray_order(model: &creabim_core::Model, under: &mut [ElementId]) {
    under.sort_by_key(|id| model.get(*id).is_some_and(|e| is_xray_host(&e.kind)));
}

/// Everything under drawing point `p` of a view, in the order a click
/// takes it: its linework first, then what the view ray passes.
#[allow(clippy::too_many_arguments)]
fn under_pointer(
    state: &mut PlanState,
    doc: &Document,
    view: ElementId,
    kind: PaneKind,
    plan: &Drawing,
    p: DVec2,
    tol: f64,
    mm: f64,
) -> Vec<ElementId> {
    let mut under = hit_all(&plan.items, p, tol, mm);
    let scene = cached_pick_scene(state, doc, view);
    let drawn: HashSet<ElementId> = match kind {
        PaneKind::Plan { .. } => plan.items.iter().map(|i| i.id()).collect(),
        _ => HashSet::new(),
    };
    for id in ray_pick_all(doc.model(), &scene, view, kind, p, Some(&drawn)) {
        if !under.contains(&id) {
            under.push(id);
        }
    }
    // underlays and guides drawn from elsewhere are not picked here
    under.retain(|id| doc.model().get(*id).is_some());
    if state.xray.contains(&view) {
        xray_order(doc.model(), &mut under);
    }
    under
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
        Tool::Level => match state.pending.take() {
            None => {
                state.pending = Some(p);
                state.status = Some(format!(
                    "New level at {}. Click where its line ends.",
                    crate::units::elevation(p.y)
                ));
                Ok(())
            }
            Some(a) => {
                let model = doc.model();
                let z = a.y;
                if model.levels().any(|(_, l)| (l.elevation - z).abs() < 1e-4) {
                    state.status = Some("There is a level at that height already.".into());
                    return Ok(());
                }
                // up to the next level, or as tall as the one below
                let list = creabim_core::levels::sorted(model);
                let height = list
                    .iter()
                    .find(|(_, _, l)| l.elevation > z + 1e-6)
                    .map(|(_, _, l)| l.elevation - z)
                    .or_else(|| {
                        list.iter()
                            .rev()
                            .find(|(_, _, l)| l.elevation < z)
                            .map(|(_, _, l)| l.height)
                    })
                    .unwrap_or(3.0)
                    .max(0.5);
                let name = creabim_core::levels::next_name(model);
                let (mut tx, id, _) = creabim_core::levels::add(model, &name, z, height, true);
                tx.label = "Draw level".into();
                selection.only(id);
                state.status = Some(format!(
                    "Added {name} at {} with its floor plan. Drag its line to move it.",
                    crate::units::elevation(z)
                ));
                doc.commit(tx)
            }
        },
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
                        assembly: None,
                        flip: false,
                        location: Default::default(),
                        top: Default::default(),
                        disallow_join: [false; 2],
                        attach_top: vec![],
                        curtain: None,
                    }),
                );
                // its type's thickness, but its drawn height
                let mut e = state.typed(doc.model(), e);
                if let ElementKind::Wall(w) = &mut e.kind {
                    w.height = height;
                }
                selection.only(e.id);
                doc.commit(Transaction::new("Draw wall in section").insert(e))
            }
        },
        Tool::Door | Tool::Window | Tool::Opening => {
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
            let (kind, name) = match tool {
                Tool::Door => (OpeningKind::Door, "Door"),
                Tool::Opening => (OpeningKind::Hole, "Wall opening"),
                _ => (OpeningKind::Window, "Window"),
            };
            let mut e = state.typed(
                doc.model(),
                Element::new(
                    name,
                    ElementKind::Opening(Opening {
                        host,
                        kind,
                        offset: 0.0,
                        sill: 0.0,
                        width: if kind == OpeningKind::Door { 0.9 } else { 1.2 },
                        height: match kind {
                            OpeningKind::Door => 2.1,
                            OpeningKind::Window => 1.3,
                            OpeningKind::Hole => 1.0,
                        },
                        material: None,
                        frame: Default::default(),
                        flip_hand: false,
                        flip_facing: false,
                    }),
                ),
            );
            let ElementKind::Opening(op) = &mut e.kind else {
                return Ok(());
            };
            if op.width > w.length() {
                return Ok(());
            }
            // where it was clicked: along the wall, and up it but for doors
            op.offset = (u - op.width / 2.0).clamp(0.0, w.length() - op.width);
            if kind != OpeningKind::Door {
                op.sill =
                    (p.y - base - op.height / 2.0).clamp(0.0, (w.height - op.height).max(0.0));
            }
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
                        top_offset: 0.0,
                        bulges: vec![],
                        assembly: None,
                        holes: vec![],
                    }),
                );
                // the slab type in use, unless its thickness was drawn
                let e = if dy > 0.05 {
                    e
                } else {
                    state.typed(doc.model(), e)
                };
                selection.only(e.id);
                doc.commit(Transaction::new("Draw slab in section").insert(e))
            }
        },
        Tool::Guide => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) if a.distance(p) > 0.01 => {
                let e = Element::new(
                    "Guide",
                    ElementKind::Guide(creabim_core::Guide {
                        level: None,
                        a: at(a).extend(a.y),
                        b: at(p).extend(p.y),
                    }),
                );
                doc.commit(Transaction::new("Add guide").insert(e))
            }
            Some(_) => Ok(()),
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

/// The Move and Rotate tools, and G, R and S (move, rotate and scale) and
/// Shift+D (copy) while the pointer is over this pane. Returns `Some` while
/// one runs here. `pick` finds the element under the pointer, for
/// Shift+click while the base point is picked.
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
    pick: &dyn Fn(Pos2) -> Option<ElementId>,
) -> Option<Option<String>> {
    let shift = ui.input(|i| i.modifiers.shift);
    if let Some(m) = state.modal.as_mut().filter(|m| m.pane == Some(view)) {
        // Shift+click while the base point is picked adds or takes away
        if m.stage() == Stage::Base && shift && response.clicked() {
            if let Some(id) = response.interact_pointer_pos().and_then(pick) {
                selection.toggle(id);
                let grab = response.hover_pos().unwrap_or_default();
                state.status = Some(restart_modal(
                    doc,
                    &mut state.modal,
                    &selection.all(),
                    grab,
                    state.pivot,
                    state.cursor,
                ));
            }
            return Some(None);
        }
        m.panel(ui.ctx(), response.rect);
        let click = response.clicked();
        let right = response.secondary_clicked() && !shift;
        match m.update(
            doc,
            ui.ctx(),
            painter,
            response.hover_pos(),
            click,
            right,
            map,
        ) {
            Outcome::Running(s) => state.status = Some(s),
            Outcome::Done(s) | Outcome::Cancelled(s) => {
                state.status = Some(s);
                state.modal = None;
            }
        }
        if let Some(m) = &state.modal {
            state.move_target = m.works_on;
        }
        return Some(None);
    }
    if state.modal.is_some() || !response.hovered() || ui.ctx().egui_wants_keyboard_input() {
        return None;
    }
    selection.active?;
    let keys = start_keys(ui.ctx());
    let flow = if keys.is_some() {
        Flow::Keys
    } else {
        Flow::Tool
    };
    let mode = keys.or(state.armed)?;
    let grab = response.hover_pos()?;
    state.armed = None;
    state.cancel();
    let ids = selection.all();
    match start_modal(
        doc,
        mode,
        &ids,
        None,
        space,
        Some(view),
        grab,
        state.pivot,
        state.cursor,
        flow,
        state.move_target,
    ) {
        Ok((m, copies)) => {
            if copies != ids {
                selection.extend(copies, false);
            }
            state.status = Some(start_hint(&m));
            state.modal = Some(m);
        }
        Err(e) => state.status = Some(e),
    }
    Some(None)
}

/// What to do first, as a running edit begins.
pub fn start_hint(m: &Modal) -> String {
    match (m.stage(), m.mode) {
        (Stage::Base, Mode::Rotate) => "Rotate: click the centre to turn about.".into(),
        (Stage::Base, _) => {
            "Move: click the base point, the point you move by. It snaps; Shift+click adds or takes away elements.".into()
        }
        (_, mode) => format!(
            "{mode:?}: move the pointer, or type a value. X, Y or Z keeps to an axis."
        ),
    }
}

/// Start the running Move or Rotate tool again on a new selection, as it
/// was changed with Shift+click before the base point. Returns a message.
pub fn restart_modal(
    doc: &mut Document,
    modal: &mut Option<Modal>,
    ids: &[ElementId],
    grab: Pos2,
    pivot: modal::Pivot,
    cursor: Option<DVec3>,
) -> String {
    let Some(old) = modal.take() else {
        return String::new();
    };
    let mut old = old;
    old.cancel(doc);
    if ids.is_empty() {
        return "Nothing selected: click what to move.".into();
    }
    match Modal::start(
        doc, old.mode, ids, None, old.space, old.pane, grab, pivot, cursor,
    ) {
        Ok(m) => {
            let n = ids.len();
            *modal = Some(m.with_flow(Flow::Tool).with_works_on(old.works_on));
            format!(
                "{n} selected. Shift+click adds or takes away more; click the base point to go on."
            )
        }
        Err(e) => e,
    }
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

/// Start a keyboard edit on the selection, copying it first for Shift+D.
/// Returns the edit and the elements it works on (the copies, if made).
#[allow(clippy::too_many_arguments)]
pub fn start_modal(
    doc: &mut Document,
    (mode, duplicate): (Mode, bool),
    ids: &[ElementId],
    face: Option<usize>,
    space: Space,
    pane: Option<ElementId>,
    grab: Pos2,
    pivot: modal::Pivot,
    cursor: Option<DVec3>,
    flow: Flow,
    works_on: modal::Target,
) -> Result<(Modal, Vec<ElementId>), String> {
    let targets = if duplicate {
        modal::duplicate(doc, ids)?
    } else {
        ids.to_vec()
    };
    // a selected mass face is what G moves; the Move tool picks its face
    // itself when it works on faces
    let face = face.filter(|_| !duplicate && flow == Flow::Keys);
    match Modal::start(doc, mode, &targets, face, space, pane, grab, pivot, cursor) {
        Ok(mut m) => {
            m.duplicated = duplicate;
            Ok((m.with_flow(flow).with_works_on(works_on), targets))
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
    exact: bool,
) -> Option<Option<String>> {
    let project = |p: DVec3| Some(xf.to_screen(model_to_pane(space, p)));
    // a sketch being drawn in another pane is left alone
    let mine = state.sketch.as_ref().is_some_and(|d| d.pane == Some(view));
    let draft = state.sketch.as_mut().filter(|d| d.pane == Some(view));
    if let Some(hint) = crate::sketch::shape_controls(ui, response.rect, Some(view), draft) {
        state.status = Some(hint);
    }
    let tol = 10.0 / xf.scale as f64;
    let on_plane = match (&state.sketch, hover) {
        (Some(d), Some(m)) if mine => Some(d.snap(d.plane.to_plane(m), tol, shift, !exact)),
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
    let finish = |doc: &mut Document, state: &mut PlanState, selection: &mut Selection, closed| {
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
            let draft = SketchDraft::new(plane, Some(view));
            let shape = crate::sketch::shape(ui.ctx());
            state.status = Some(crate::sketch::hint(shape, Some(&draft)));
            state.sketch = Some(draft);
            Some(None)
        }
        Some(d) => match d.click(on_plane?) {
            Some(Click::Closed) => finish(doc, state, selection, true),
            Some(Click::Added) => {
                state.status = Some(crate::sketch::hint(d.shape, Some(d)));
                Some(None)
            }
            None => Some(None),
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
    let dragging = state.grip.is_some()
        || state
            .modal
            .as_ref()
            .is_some_and(|m| m.pane == Some(view) && m.freezes_snaps());
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
                    let (w, c) = if selected {
                        (2.0, highlight)
                    } else {
                        (1.0, Color32::from_rgb(120, 140, 170))
                    };
                    painter.extend(egui::Shape::dashed_line(
                        &[a, b],
                        Stroke::new(w, c),
                        12.0,
                        4.0,
                    ));
                }
                Pen::Underlay => {
                    painter.line_segment([a, b], Stroke::new(1.0, UNDERLAY_COLOR));
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
                Pen::Hidden => {
                    let (w, c) = if selected {
                        (2.0, highlight)
                    } else {
                        (1.0, Color32::from_gray(110))
                    };
                    painter.extend(egui::Shape::dashed_line(
                        &[a, b],
                        Stroke::new(w, c),
                        6.0,
                        4.0,
                    ));
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
            pen,
            ..
        } => {
            let ink = if *pen == Pen::Underlay {
                UNDERLAY_COLOR
            } else {
                ink
            };
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
            if creabim_geom::plan::is_bubble(text) {
                let r = 10.0 * creabim_geom::plan::bubble_scale(text) as f32;
                painter.circle(p, r, Color32::WHITE, Stroke::new(1.0, blue));
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
            a,
            b,
            offset,
            text,
            look,
            witness,
            ..
        } => {
            let ink = crate::annotate::ink(look, selected, highlight);
            draw_dimension(painter, xf, *a, *b, *offset, *witness, text, look, mm, ink);
        }
        PlanItem::Annotation { item, .. } => {
            let ink = crate::annotate::ink(&item.look, selected, highlight);
            crate::annotate::paint(painter, xf, item, ink, mm);
        }
        PlanItem::Tag {
            anchor,
            at,
            text,
            round,
            look,
            ..
        } => {
            let g = tag_geometry(*anchor, *at, text, *round, look, mm);
            let pts: Vec<Pos2> = g.outline.iter().map(|p| xf.to_screen(*p)).collect();
            let ink = crate::annotate::ink(look, selected, highlight);
            if let Some((p, q)) = g.leader {
                painter.line_segment([xf.to_screen(p), xf.to_screen(q)], Stroke::new(1.0, ink));
            }
            if g.closed {
                painter.add(egui::Shape::convex_polygon(
                    pts,
                    Color32::WHITE,
                    Stroke::new(1.2, ink),
                ));
            } else if pts.len() > 1 {
                painter.add(egui::Shape::line(pts, Stroke::new(1.2, ink)));
            }
            let px = crate::annotate::text_px(look, mm, xf);
            crate::annotate::text(
                painter,
                xf.to_screen(*at),
                text,
                px,
                ink,
                0.0,
                paper::Anchor::Middle,
                look,
            );
        }
        PlanItem::Tint { points, rgb, .. } => {
            let c = if selected { [0.98, 0.70, 0.45] } else { *rgb };
            let a = (paper::TINT_ALPHA * 255.0) as u8;
            let fill = crate::annotate::color32(c).gamma_multiply_u8(a);
            fill_polygon(painter, xf, points, fill);
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

/// An item drawn in X-ray: what holds others (walls, slabs, roofs,
/// masses) faint and see-through, and what is hidden inside them drawn
/// solid instead of dashed, as if nothing were in front.
fn draw_item_xray(
    painter: &egui::Painter,
    xf: &Xf,
    item: &PlanItem,
    selected: bool,
    mm: f64,
    model: &creabim_core::Model,
) {
    let host = || model.get(item.id()).is_some_and(|e| is_xray_host(&e.kind));
    match item {
        PlanItem::Fill { points, .. } if !selected && host() => {
            fill_polygon(
                painter,
                xf,
                points,
                Color32::from_gray(150).gamma_multiply(0.18),
            );
        }
        PlanItem::Polygon {
            pen: Pen::Cut,
            points,
            ..
        } if !selected && host() => {
            let pts: Vec<Pos2> = points.iter().map(|p| xf.to_screen(*p)).collect();
            painter.add(egui::Shape::convex_polygon(
                pts,
                Color32::from_gray(150).gamma_multiply(0.18),
                Stroke::new(1.2, Color32::from_gray(90)),
            ));
        }
        PlanItem::Line {
            pen: Pen::Cut,
            a,
            b,
            ..
        } if !selected && host() => {
            painter.line_segment(
                [xf.to_screen(*a), xf.to_screen(*b)],
                Stroke::new(1.2, Color32::from_gray(90)),
            );
        }
        PlanItem::Line {
            pen: Pen::Hidden,
            a,
            b,
            ..
        } => {
            let c = if selected {
                Color32::from_rgb(230, 120, 30)
            } else {
                Color32::from_gray(25)
            };
            painter.line_segment([xf.to_screen(*a), xf.to_screen(*b)], Stroke::new(1.5, c));
        }
        _ => draw_item(painter, xf, item, selected, mm),
    }
}

/// The pre-selection highlight of element `id`: its linework traced and
/// its fills washed in the hover colour, over the drawing.
pub fn draw_hover(painter: &egui::Painter, xf: &Xf, items: &[PlanItem], id: ElementId) {
    let line = Stroke::new(2.5, hoverpick::HOVER.gamma_multiply(0.75));
    let wash = hoverpick::HOVER.gamma_multiply(0.16);
    for it in items.iter().filter(|it| it.id() == id) {
        match it {
            PlanItem::Fill { points, .. } | PlanItem::Tint { points, .. } => {
                fill_polygon(painter, xf, points, wash);
            }
            PlanItem::Polygon { points, .. } => {
                let pts: Vec<Pos2> = points.iter().map(|p| xf.to_screen(*p)).collect();
                painter.add(egui::Shape::closed_line(pts, line));
            }
            PlanItem::Line { pen, a, b, .. } => {
                let (a, b) = (xf.to_screen(*a), xf.to_screen(*b));
                // guides and grid lines run far: just the part in view
                if let Some((a, b)) = match pen {
                    Pen::Guide | Pen::Grid => clip_to_rect(a, b, painter.clip_rect()),
                    _ => Some((a, b)),
                } {
                    painter.line_segment([a, b], line);
                }
            }
            PlanItem::Arc {
                center,
                radius,
                start,
                end,
                ..
            } => {
                let pts: Vec<Pos2> = (0..=24)
                    .map(|i| {
                        let t = start + (end - start) * i as f64 / 24.0;
                        xf.to_screen(*center + DVec2::from_angle(t) * *radius)
                    })
                    .collect();
                painter.add(egui::Shape::line(pts, line));
            }
            PlanItem::Image { .. } => {}
            // labels, text and annotation: a soft box round them
            other => {
                if let Some((lo, hi)) = other.bounds() {
                    let r = Rect::from_two_pos(xf.to_screen(lo), xf.to_screen(hi)).expand(3.0);
                    painter.rect(r, 3.0, wash, line, egui::StrokeKind::Outside);
                }
            }
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
    witness: Option<[DVec2; 2]>,
    text: &str,
    look: &TextLook,
    mm: f64,
    ink: Color32,
) {
    let g = dimension_geometry(a, b, offset, witness, look, mm);
    for (p, q) in g.lines {
        painter.line_segment([xf.to_screen(p), xf.to_screen(q)], Stroke::new(1.0, ink));
    }
    crate::annotate::paint_ends(painter, xf, &g.ends, ink);
    let px = crate::annotate::text_px(look, mm, xf);
    crate::annotate::text(
        painter,
        xf.to_screen(g.text_at),
        text,
        px,
        ink,
        g.angle,
        paper::Anchor::Middle,
        look,
    );
}

/// A dimension being drawn, measuring along `axis`.
#[allow(clippy::too_many_arguments)]
fn preview_dimension(
    painter: &egui::Painter,
    xf: &Xf,
    a: DVec2,
    b: DVec2,
    axis: creabim_core::DimAxis,
    offset: f64,
    look: &TextLook,
    mm: f64,
    accent: Color32,
) {
    let Some((b2, witness)) = annotate::squared(axis, a, b) else {
        return;
    };
    let text = annotate::dimension_text(&crate::units::get(), a, b2);
    draw_dimension(painter, xf, a, b2, offset, witness, &text, look, mm, accent);
}

/// The point a dimension click lands on, what it attaches to, and the snap
/// that found it. Wall corners and door or window edges come first, since
/// those keep the most useful attachment; then any snap in the pane's
/// drawing: corners, edges and centres of masses, slabs, columns, beams,
/// sketches, and grid and level lines. A point on an element follows it.
fn dim_snap(
    doc: &Document,
    view: &View,
    kind: PaneKind,
    snapper: Option<&Snapper>,
    cfg: &snaps::SnapSettings,
    xf: &Xf,
    cursor: Pos2,
) -> (HostPoint, DVec2, Option<SnapKind>) {
    let (hp, at, k, _) = dim_snap_guided(doc, view, kind, snapper, cfg, xf, cursor, &[]);
    (hp, at, k)
}

/// The line offset of a dimension from `a` to `b` measuring along `axis`
/// whose line goes through `at`.
fn dim_offset(axis: creabim_core::DimAxis, a: DVec2, b: DVec2, at: DVec2) -> f64 {
    let u = axis.direction(a, b).unwrap_or((b - a).normalize_or_zero());
    (at - a).dot(u.perp())
}

/// [`dim_snap`] with temporary guides too: where they cross each other
/// and the drawing's edges. Also returns the guides the point is on.
#[allow(clippy::too_many_arguments)]
fn dim_snap_guided(
    doc: &Document,
    view: &View,
    kind: PaneKind,
    snapper: Option<&Snapper>,
    cfg: &snaps::SnapSettings,
    xf: &Xf,
    cursor: Pos2,
    guides: &[snaps::TempGuide],
) -> (HostPoint, DVec2, Option<SnapKind>, Vec<usize>) {
    let model = doc.model();
    let p = xf.to_world(cursor);
    let tol = 10.0 / xf.scale as f64;
    let mut best: Option<(f64, ElementId, DVec2)> = None;
    let mut consider = |host: ElementId, q: DVec2| {
        let d = q.distance(p);
        if d < tol && best.is_none_or(|(bd, ..)| d < bd) {
            best = Some((d, host, q));
        }
    };
    if let (PaneKind::Plan { level }, true) = (kind, cfg.allows(SnapKind::Endpoint)) {
        let joins = creabim_core::wall_joins(model);
        for e in model.iter() {
            let ElementKind::Wall(w) = &e.kind else {
                continue;
            };
            if w.level != level {
                continue;
            }
            let ends = joins.get(&e.id).cloned().unwrap_or_default();
            for q in ends.footprint(w) {
                consider(e.id, q);
            }
            for q in [w.start, w.end] {
                consider(e.id, q);
            }
            let (d, n, t) = (w.direction(), w.direction().perp(), w.thickness / 2.0);
            for oid in model.dependents(e.id) {
                let Some(ElementKind::Opening(o)) = model.get(oid).map(|e| &e.kind) else {
                    continue;
                };
                for u in [o.offset, o.offset + o.width] {
                    for s in [-t, t] {
                        consider(oid, w.start + d * u + n * s);
                    }
                }
            }
        }
    }
    let project = |q: DVec3| Some(xf.to_screen(q.truncate()));
    let hit = snapper.and_then(|s| {
        s.snap_guided(
            cfg,
            cursor,
            &project,
            None,
            &[],
            guides,
            Some(p.extend(0.0)),
        )
    });
    let (host, at, kind, used) = match (best, hit) {
        // a nearer corner of something else beats the wall's
        (Some((d, host, q)), h)
            if h.as_ref().is_none_or(|h| {
                !h.snap.kind.is_point() || h.snap.p.truncate().distance(p) >= d
            }) =>
        {
            (Some(host), q, Some(SnapKind::Endpoint), vec![])
        }
        (_, Some(h)) => (
            h.snap.host,
            h.snap.p.truncate(),
            Some(h.snap.kind),
            h.guides,
        ),
        (_, None) => (None, p, None, vec![]),
    };
    (annotate::attach(model, view, host, at), at, kind, used)
}

/// Keep a temporary guide line as a guide element: in a plan on its
/// level, in a section on the section's plane.
fn keep_guide(
    doc: &mut Document,
    kind: PaneKind,
    guide: Option<snaps::TempGuide>,
) -> Result<String, creabim_core::ModelError> {
    let Some(g) = guide else {
        return Ok("Point at a guide line first, then press K to keep it.".into());
    };
    let snaps::GuideShape::Line { p, d } = g.shape else {
        return Ok("Only straight guide lines can be kept; circles stay temporary.".into());
    };
    let (level, a, b) = match kind {
        PaneKind::Plan { level } => {
            let z = doc.model().level(level).map_or(0.0, |l| l.elevation);
            (Some(level), p.extend(z), (p + d).extend(z))
        }
        PaneKind::Section { start, end } => {
            let space = Space::Section { start, end };
            (
                None,
                pane_to_model(space, p, 0.0),
                pane_to_model(space, p + d, 0.0),
            )
        }
        _ => return Ok(String::new()),
    };
    let e = Element::new(
        format!("Guide ({})", g.kind.label().to_lowercase()),
        ElementKind::Guide(creabim_core::Guide { level, a, b }),
    );
    doc.commit(Transaction::new("Keep guide line").insert(e))?;
    Ok(format!(
        "Kept the {} guide line. It stays until you delete it.",
        g.kind.label().to_lowercase()
    ))
}

/// A click with the Dimension tool: first point, second point, then where the line goes.
fn dimension_tool(
    doc: &mut Document,
    view: ElementId,
    hit: (HostPoint, DVec2),
    raw: DVec2,
    axis: creabim_core::DimAxis,
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
            // a horizontal or vertical one needs its points apart that way
            let axis = if axis.direction(pa, pb).is_some() {
                axis
            } else {
                creabim_core::DimAxis::Aligned
            };
            let offset = dim_offset(axis, pa, pb, raw);
            let mut d = Dimension::linear(view, a, b, offset);
            d.axis = axis;
            let e = state
                .anno
                .styled(Element::new("Dimension", ElementKind::Dimension(d)));
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
            let taggable = picked.filter(|id| is_taggable(doc.model(), *id));
            if picked.is_some() && taggable.is_none() {
                state.status = Some(
                    "That can't take a tag: pick a door, window, wall, slab, roof, column, beam, stair or railing."
                        .into(),
                );
            }
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
            let e = state.anno.styled(Element::new(
                "Tag",
                ElementKind::Tag(Tag::new(view, host, raw - proj.project(c))),
            ));
            selection.only(e.id);
            doc.commit(Transaction::new("Add tag").insert(e))
        }
    }
}

/// Can the Tag tool tag element `id`? Building elements can; views,
/// annotation and the like can't.
pub fn is_taggable(model: &creabim_core::Model, id: ElementId) -> bool {
    matches!(
        model.get(id).map(|e| &e.kind),
        Some(
            ElementKind::Opening(_)
                | ElementKind::Wall(_)
                | ElementKind::Slab(_)
                | ElementKind::FreeMesh(_)
                | ElementKind::Roof(_)
                | ElementKind::Column(_)
                | ElementKind::Beam(_)
                | ElementKind::Stair(_)
                | ElementKind::Railing(_)
        )
    ) && annotate::centre(model, id).is_some()
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
            ElementKind::Tag(Tag::new(view, e.id, offset)),
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

/// Snap to wall endpoints within 10 px, else to a 5 cm grid, as far as
/// `cfg` allows. With `ortho`, keep the segment from `from` horizontal or
/// vertical.
#[allow(clippy::too_many_arguments)]
fn snap(
    doc: &Document,
    level: ElementId,
    xf: &Xf,
    p: DVec2,
    from: Option<DVec2>,
    ortho: bool,
    exclude: &[ElementId],
    cfg: &snaps::SnapSettings,
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
            if w.level != level || exclude.contains(&e.id) || !cfg.allows(SnapKind::Endpoint) {
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
    best.map(|(_, q)| q).unwrap_or_else(|| {
        if cfg.allows(SnapKind::Grid) {
            (p / 0.05).round() * 0.05
        } else {
            p
        }
    })
}

/// Fill a possibly concave polygon.
/// A slab's fill in its level's plan, under the pointer, and selected.
const SLAB_FILL: Color32 = Color32::from_rgba_unmultiplied_const(70, 110, 170, 30);
const SLAB_HOVER: Color32 = Color32::from_rgba_unmultiplied_const(60, 110, 200, 70);
const SLAB_SELECTED: Color32 = Color32::from_rgba_unmultiplied_const(240, 140, 50, 60);
/// The edge of the slab under the pointer.
const SLAB_EDGE: Color32 = Color32::from_rgb(60, 110, 200);

/// The slabs of `level` the plan's drawing shows in projection (not cut
/// through), with their outlines.
fn level_slabs(
    model: &creabim_core::Model,
    level: ElementId,
    plan: &Drawing,
) -> Vec<(ElementId, Vec<DVec2>)> {
    plan.items
        .iter()
        .filter_map(|it| match it {
            PlanItem::Polygon {
                id,
                pen: Pen::Projection,
                points,
            } if matches!(
                model.get(*id).map(|e| &e.kind),
                Some(ElementKind::Slab(s)) if s.level == level
            ) =>
            {
                Some((*id, points.clone()))
            }
            _ => None,
        })
        .collect()
}

/// The slab whose outline holds `p`; the smallest when they overlap.
fn slab_at(slabs: &[(ElementId, Vec<DVec2>)], p: DVec2) -> Option<ElementId> {
    slabs
        .iter()
        .filter(|(_, pts)| point_in_polygon(p, pts))
        .min_by(|a, b| signed_area(&a.1).abs().total_cmp(&signed_area(&b.1).abs()))
        .map(|(id, _)| *id)
}

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

/// The pivot cursor: a red and white ring with a cross, as in Blender.
pub fn draw_cursor(painter: &egui::Painter, at: Pos2) {
    let red = Color32::from_rgb(220, 50, 50);
    painter.circle_stroke(at, 7.0, Stroke::new(2.0, Color32::WHITE));
    painter.add(egui::Shape::dashed_line(
        &[
            at + Vec2::new(7.0, 0.0),
            at + Vec2::new(0.0, 7.0),
            at + Vec2::new(-7.0, 0.0),
            at + Vec2::new(0.0, -7.0),
            at + Vec2::new(7.0, 0.0),
        ],
        Stroke::new(2.0, red),
        4.0,
        4.0,
    ));
    for d in [Vec2::X, Vec2::Y, -Vec2::X, -Vec2::Y] {
        painter.line_segment(
            [at + d * 9.0, at + d * 14.0],
            Stroke::new(1.0, Color32::BLACK),
        );
    }
}

/// Can a box take this element? Views and levels are left to clicks, so a
/// box round the building doesn't pick up its section marks.
pub fn box_selectable(doc: &Document, id: ElementId) -> bool {
    doc.model().get(id).is_some_and(|e| {
        !matches!(
            e.kind,
            ElementKind::View(_)
                | ElementKind::Level(_)
                | ElementKind::Sheet(_)
                | ElementKind::Viewport(_)
        )
    })
}

pub fn hit_test(items: &[PlanItem], p: DVec2, tol: f64, mm: f64) -> Option<ElementId> {
    hit_all(items, p, tol, mm).first().copied()
}

/// Every element whose linework is at `p`, in the order a click takes
/// them: annotations, then symbols (doors and windows over the walls they
/// sit in), then fills (a tinted element over the wall it is inside),
/// then lines; each once.
pub fn hit_all(items: &[PlanItem], p: DVec2, tol: f64, mm: f64) -> Vec<ElementId> {
    let mut out: Vec<ElementId> = Vec::new();
    let mut found = |id: ElementId| {
        if !out.contains(&id) {
            out.push(id);
        }
    };
    // symbols (doors, windows) win over the walls they sit in
    let near_line = |a: DVec2, b: DVec2| segment_distance(p, a, b) < tol;
    // annotations sit on top of everything
    for it in items.iter().rev() {
        match it {
            PlanItem::Tag { id, at, .. } if p.distance(*at) < (3.0 * mm).max(tol) => found(*id),
            PlanItem::Annotation { id, item } if item.hit(p, tol, mm) => found(*id),
            PlanItem::Dimension {
                id, a, b, offset, ..
            } => {
                let n = (*b - *a).normalize_or_zero().perp() * *offset;
                let text = (*a + *b) / 2.0 + n;
                if near_line(*a + n, *b + n) || p.distance(text) < (3.0 * mm).max(tol) {
                    found(*id);
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
                    found(*id);
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
            } if near_line(*a, *b) => found(*id),
            PlanItem::Arc {
                id,
                center,
                radius,
                pen,
                ..
            } if *pen != Pen::Underlay && (p.distance(*center) - radius).abs() < tol => found(*id),
            // view marks and grid ends: their arrow or their bubble
            PlanItem::Polygon {
                id,
                pen: Pen::Symbol,
                points,
            } if point_in_polygon(p, points) => found(*id),
            PlanItem::Label { id, at, text }
                if text.chars().count() <= 3 && p.distance(*at) < tol * 1.2 =>
            {
                found(*id)
            }
            _ => {}
        }
    }
    for it in items.iter().rev() {
        match it {
            PlanItem::Fill { id, points, .. } | PlanItem::Tint { id, points, .. }
                if point_in_polygon(p, points) =>
            {
                found(*id)
            }
            _ => {}
        }
    }
    for it in items.iter().rev() {
        match it {
            PlanItem::Line { id, a, b, pen } if *pen != Pen::Underlay && near_line(*a, *b) => {
                found(*id)
            }
            PlanItem::Polygon { id, points, .. } => {
                let n = points.len();
                if (0..n).any(|i| near_line(points[i], points[(i + 1) % n])) {
                    found(*id);
                }
            }
            _ => {}
        }
    }
    out
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
    state: &PlanState,
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
        Tool::Opening => (OpeningKind::Hole, 1.0, 1.0, 1.0, "Wall opening"),
        _ => (OpeningKind::Window, 1.2, 1.3, 0.9, "Window"),
    };
    let mut e = state.typed(
        doc.model(),
        Element::new(
            name,
            ElementKind::Opening(Opening {
                host,
                kind,
                offset: 0.0,
                sill,
                width,
                height,
                material: None,
                frame: Default::default(),
                flip_hand: false,
                flip_facing: false,
            }),
        ),
    );
    // centred on the click, as wide as its type makes it
    if let ElementKind::Opening(o) = &mut e.kind {
        if o.width > len {
            return Ok(());
        }
        o.offset = (u - o.width / 2.0).clamp(0.0, len - o.width);
    }
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

/// The points of the shape being drawn in a view, in its drawing
/// coordinates: a slab outline, a sketch, a railing path or a chain of
/// walls. Alignment tracking lines the next point up with them.
fn shape_points(
    state: &PlanState,
    view: ElementId,
    kind: PaneKind,
    from_pane: &dyn Fn(DVec3) -> DVec2,
) -> Vec<DVec2> {
    let level = match kind {
        PaneKind::Plan { level } => Some(level),
        _ => None,
    };
    if let Some(o) = state.outline.as_ref().filter(|o| Some(o.level) == level) {
        return o.points.clone();
    }
    if let Some(d) = state.sketch.as_ref().filter(|d| d.pane == Some(view)) {
        return d
            .points
            .iter()
            .map(|p| from_pane(d.plane.to_model(*p)))
            .collect();
    }
    if level.is_some() && !state.members.path.is_empty() {
        return state.members.path.clone();
    }
    match (&state.chain, state.pending) {
        ((Some(l), pts), Some(_)) if Some(*l) == level => pts.clone(),
        _ => vec![],
    }
}

/// Rest on a point or edge for a moment (or press Q over it) and it becomes
/// a snap reference in this view, its guide lines showing; rest on one
/// again and it goes. Returns a status message when one is picked or
/// dropped.
fn rest_reference(
    state: &mut PlanState,
    view: ElementId,
    under: Option<snaps::Reference>,
    now: f64,
    key: bool,
) -> Option<String> {
    const DWELL: f64 = 0.6;
    let Some(r) = under else {
        state.ref_rest = None;
        return None;
    };
    let toggle = match state.ref_rest {
        Some((q, since, done)) if q.same(&r) => {
            let due = !done && (key || now - since > DWELL);
            state.ref_rest = Some((q, since, done || due));
            due
        }
        _ => {
            state.ref_rest = Some((r, now, key));
            key
        }
    };
    if !toggle {
        return None;
    }
    let refs = state.refs.entry(view).or_default();
    let what = match r {
        snaps::Reference::Point { .. } => "point",
        snaps::Reference::Edge { arc: Some(_), .. } => "arc",
        snaps::Reference::Edge { .. } => "edge",
    };
    if let Some(i) = refs.iter().position(|q| q.same(&r)) {
        refs.remove(i);
        return Some(format!("Dropped the {what}'s guide lines."));
    }
    refs.push(r);
    if refs.len() > snaps::MAX_REFERENCES {
        refs.remove(0);
    }
    Some(format!(
        "Guide lines from this {what}: where they cross snaps. Rest on it again to drop them, K keeps one, Esc clears them all."
    ))
}

/// While drawing, a point snapped to and rested on for a moment joins the
/// points tracked (the last few of them), as in SketchUp.
fn rest_on(
    state: &mut PlanState,
    view: ElementId,
    snap: Option<snaps::Snap>,
    drawing: bool,
    now: f64,
) {
    const DWELL: f64 = 0.4;
    const KEEP: usize = 4;
    if !drawing {
        state.rested.1.clear();
        state.rest = None;
        return;
    }
    if state.rested.0 != Some(view) {
        state.rested = (Some(view), vec![]);
    }
    let Some(p) = snap.filter(|s| s.kind.is_point()).map(|s| s.p.truncate()) else {
        state.rest = None;
        return;
    };
    match state.rest {
        Some((q, since)) if q.distance(p) < 1e-9 => {
            let pts = &mut state.rested.1;
            if now - since > DWELL && !pts.iter().any(|r| r.distance(p) < 1e-9) {
                pts.push(p);
                if pts.len() > KEEP {
                    pts.remove(0);
                }
            }
        }
        _ => state.rest = Some((p, now)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::{EditMesh, FreeMesh};

    #[test]
    fn tools_draw_new_elements_as_their_type() {
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let (tx, ids) = creabim_core::types::add_starters(doc.model());
        doc.commit(tx).unwrap();
        let level = doc.model().levels().next().unwrap().0.id;
        let storefront = ids[5];
        let window = ids[9];
        let mut state = PlanState::default();
        state
            .types
            .insert(creabim_core::TypeCategory::Wall, storefront);
        state
            .types
            .insert(creabim_core::TypeCategory::Window, window);
        let mut sel = Selection::default();
        let (a, b) = (DVec2::new(0.0, -3.0), DVec2::new(6.0, -3.0));
        plan_click(&mut doc, level, Tool::Wall, a, &mut state, &mut sel).unwrap();
        plan_click(&mut doc, level, Tool::Wall, b, &mut state, &mut sel).unwrap();
        let wall = sel.active.unwrap();
        let e = doc.model().get(wall).unwrap();
        assert_eq!(e.of_type.as_ref().map(|r| r.id), Some(storefront));
        let ElementKind::Wall(w) = &e.kind else {
            panic!()
        };
        assert!(w.curtain.is_some(), "drawn as a curtain wall");
        // a window clicked into a plain wall takes its type's size
        let mut sel = Selection::default();
        plan_click(
            &mut doc,
            level,
            Tool::Window,
            DVec2::new(5.0, 6.0),
            &mut state,
            &mut sel,
        )
        .unwrap();
        let e = doc.model().get(sel.active.unwrap()).unwrap();
        assert_eq!(e.of_type.as_ref().map(|r| r.id), Some(window));
        // and the opening tool cuts a bare hole, with no type
        let mut sel = Selection::default();
        plan_click(
            &mut doc,
            level,
            Tool::Opening,
            DVec2::new(10.0, 4.5),
            &mut state,
            &mut sel,
        )
        .unwrap();
        let e = doc.model().get(sel.active.unwrap()).unwrap();
        assert!(matches!(&e.kind, ElementKind::Opening(o) if o.kind == OpeningKind::Hole));
        assert!(e.of_type.is_none());
    }

    #[test]
    fn a_dimension_attaches_to_a_mass_corner_and_follows_it() {
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let (view, v) = doc
            .model()
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::View(v) if matches!(v.kind, ViewKind::Plan { .. }) => {
                    Some((e.id, v.clone()))
                }
                _ => None,
            })
            .unwrap();
        let ViewKind::Plan { level, .. } = v.kind else {
            unreachable!()
        };
        let mass = Element::new(
            "Mass",
            ElementKind::FreeMesh(FreeMesh::new(
                Some(level),
                DVec3::new(20.0, 5.0, 0.0),
                EditMesh::cuboid(DVec3::new(2.0, 4.0, 3.0)),
            )),
        );
        let id = mass.id;
        doc.commit(Transaction::new("Add mass").insert(mass))
            .unwrap();
        let drawing = creabim_geom::section::view_drawing(doc.model(), view).unwrap();
        let snapper = Snapper::from_drawing(&drawing.items, &[]);
        let xf = Xf {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(1000.0)),
            center: DVec2::new(20.0, 5.0),
            scale: 100.0,
        };
        let cursor = xf.to_screen(DVec2::new(21.03, 6.98));
        let cfg = snaps::SnapSettings::default();
        let (hp, at, kind) = dim_snap(&doc, &v, pane_kind(&v), Some(&snapper), &cfg, &xf, cursor);
        assert_eq!(kind, Some(SnapKind::Endpoint));
        assert!(at.distance(DVec2::new(21.0, 7.0)) < 1e-9);
        assert_eq!(hp.host, Some(id));
        // move the mass: the point goes with it
        doc.edit(id, "Move mass", |e| {
            if let ElementKind::FreeMesh(m) = &mut e.kind {
                m.origin += DVec3::new(1.5, -1.0, 0.0);
            }
        })
        .unwrap();
        let now = annotate::resolve(doc.model(), &v, &hp).unwrap();
        assert!(now.distance(DVec2::new(22.5, 6.0)) < 1e-9, "{now}");
        // with snapping off it is a free point
        let off = cfg.live(true);
        let (hp, ..) = dim_snap(&doc, &v, pane_kind(&v), Some(&snapper), &off, &xf, cursor);
        assert_eq!(hp.host, None);
    }

    #[test]
    fn the_beam_inside_a_wall_is_under_the_pointer_in_a_section_and_first_in_x_ray() {
        let (model, beam) = creabim_core::sample::sample_with_buried_beam();
        let doc = Document::from_model(model);
        let sec = doc
            .model()
            .iter()
            .find(|e| e.name == "Section A")
            .unwrap()
            .id;
        let wall = doc
            .model()
            .iter()
            .find(|e| e.name == "GF Wall 1")
            .unwrap()
            .id;
        let Some(ElementKind::View(v)) = doc.model().get(sec).map(|e| &e.kind) else {
            panic!("Section A is a view")
        };
        let kind = pane_kind(v);
        let drawing = creabim_geom::section::view_drawing(doc.model(), sec).unwrap();
        let mut state = PlanState::default();
        // Section A runs north from y = -5: the wall is 5 m along it; the
        // beam sits 2.3 to 2.6 m up inside it
        let p = DVec2::new(5.0, 2.45);
        let under = under_pointer(&mut state, &doc, sec, kind, &drawing, p, 0.02, 0.1);
        assert_eq!(under.first(), Some(&wall), "a plain click takes the wall");
        assert!(
            under.contains(&beam),
            "the beam is found under it: {under:?}"
        );
        // what a click takes without Tab is what it always took
        assert_eq!(
            hit_test(&drawing.items, p, 0.02, 0.1).or_else(|| ray_pick(&doc, sec, kind, p)),
            Some(wall)
        );
        state.xray.insert(sec);
        let xray = creabim_geom::section::view_drawing_with(doc.model(), sec, true).unwrap();
        let under = under_pointer(&mut state, &doc, sec, kind, &xray, p, 0.02, 0.1);
        assert_eq!(under.first(), Some(&beam), "in X-ray the beam comes first");
        assert!(under.contains(&wall));
    }

    #[test]
    fn what_a_3d_views_crop_box_cuts_away_cannot_be_picked_in_it() {
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let wall = doc
            .model()
            .iter()
            .find(|e| e.name == "GF Wall 1")
            .unwrap()
            .id;
        let axo = Element::new(
            "Axo",
            ElementKind::View(creabim_core::View::axonometric(45.0, 30.0)),
        );
        let view = axo.id;
        doc.commit(Transaction::new("view").insert(axo)).unwrap();
        assert!(pick_scene(&doc, view).iter().any(|s| s.id == wall));
        // a box over the north half of the house: the south wall (y = 0)
        // is cut away
        let b = creabim_core::CropBox::new(
            Some(view),
            DVec3::new(-1.0, 3.0, -1.0),
            DVec3::new(11.0, 7.0, 7.0),
        );
        doc.commit(Transaction::new("crop").insert(Element::new("Box", ElementKind::CropBox(b))))
            .unwrap();
        let scene = pick_scene(&doc, view);
        assert!(
            !scene.iter().any(|s| s.id == wall),
            "cut away, not pickable"
        );
        assert!(!scene.is_empty(), "what is inside the box still is");
    }

    #[test]
    fn in_an_elevation_the_slab_behind_a_wall_is_under_the_pointer_too() {
        let doc = Document::from_model(creabim_core::sample::sample_house());
        let named = |n: &str| doc.model().iter().find(|e| e.name == n).unwrap().id;
        let (elev, wall, slab) = (
            named("South Elevation S"),
            named("GF Wall 1"),
            named("FF Slab"),
        );
        let Some(ElementKind::View(v)) = doc.model().get(elev).map(|e| &e.kind) else {
            panic!("the elevation is a view")
        };
        let kind = pane_kind(v);
        let drawing = creabim_geom::section::view_drawing(doc.model(), elev).unwrap();
        let mut state = PlanState::default();
        // the elevation runs east from x = -2: 7 m along is the middle of
        // the south wall; the first floor slab is 2.75 to 3 m up behind it
        let p = DVec2::new(7.0, 2.9);
        let under = under_pointer(&mut state, &doc, elev, kind, &drawing, p, 0.02, 0.1);
        assert_eq!(under.first(), Some(&wall), "a plain click takes the wall");
        assert!(under.contains(&slab), "Tab reaches the slab: {under:?}");
        // and in X-ray the slab, though it holds others too, is still there
        state.xray.insert(elev);
        let xray = creabim_geom::section::view_drawing_with(doc.model(), elev, true).unwrap();
        let under = under_pointer(&mut state, &doc, elev, kind, &xray, p, 0.02, 0.1);
        assert!(under.contains(&slab) && under.contains(&wall));
    }
}
