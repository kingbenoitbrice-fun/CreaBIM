use crate::selection::Selection;
use crate::camera::OrbitCamera;
use crate::handles::{self, PaneInput, Ray};
use crate::icons::{self, Icon};
use crate::modal::{Outcome, PaneMap, Space};
use crate::planview::{self, PaneStyle, PlanState, Tool};
use crate::render3d::{Vertex, Viewport3d};
use crate::sheetview::{self, SheetState};
use crate::sketch::{Click, SketchDraft};
use crate::snaps::{self, SnapKind, Snapper};
use creabim_core::{
    element::FREE_MESH_CLASSES, sample::sample_house, Axis, BoolOp, Cut, Document, Element,
    ElementId, ElementKind, Hatch, Material, Model, Modifier, ModifierKind, ScheduleCategory,
    Sheet, Transaction, View, ViewKind, ViewStyle, Workplane,
};
use creabim_geom::sheet::{self as sheets, PlacedView, SheetInfo};
use creabim_geom::{
    build_scene, pick, plan::Pen, section::view_drawing, section::Drawing, PlanItem, SceneItem,
};
use eframe::egui::{self, Color32, Rect, Sense, Vec2};
use eframe::egui_wgpu;
use glam::{DVec2, DVec3, Vec2 as GVec2};
use std::collections::HashMap;

/// What the 3D geometry was last built from; rebuild when any of it changes.
/// (model revision, selection, selected face, drawing shown in 3D, active view)
type GeometryKey = (
    u64,
    Selection,
    Option<usize>,
    bool,
    Option<ElementId>,
);

/// Where the 2D plan is shown relative to the 3D view.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlanPlacement {
    /// Side by side with the 3D view.
    Docked,
    /// In its own OS window, e.g. on a second monitor.
    Window,
    /// Inset inside the 3D workspace.
    Overlay,
    Hidden,
}

/// How the 3D view and the 2D pane share the window when docked. The
/// divider between them drags like Blender's area borders.
#[derive(Clone, Copy)]
struct PaneLayout {
    /// Share of the width (or height, when stacked) given to the first pane.
    split: f32,
    /// One above the other instead of side by side.
    stacked: bool,
    /// The 2D pane first (left or top) instead of the 3D view.
    swapped: bool,
}

impl Default for PaneLayout {
    fn default() -> Self {
        Self {
            split: 0.5,
            stacked: false,
            swapped: false,
        }
    }
}

pub struct CreaBimApp {
    doc: Document,
    selection: Selection,
    /// Selected face when a free mesh is selected (mesh edit mode).
    face: Option<usize>,
    /// What the 2D pane shows: a view (plan, section, elevation, 3D) or a sheet.
    active_view: Option<ElementId>,
    sheet: SheetState,
    /// Generated drawings by view, with the model revision they were made at.
    drawings: HashMap<ElementId, (u64, Drawing)>,
    camera: OrbitCamera,
    /// Aspect ratio of the 3D view as last drawn, for framing.
    aspect: f32,
    viewport: Option<Viewport3d>,
    geometry_key: Option<GeometryKey>,
    scene: Vec<SceneItem>,
    plan: PlanState,
    placement: PlanPlacement,
    plan_in_3d: bool,
    layout: PaneLayout,
    /// A handle or element being dragged in the 3D view.
    handle_drag: Option<handles::Drag>,
    /// Snaps for the 3D view: built from the scene without the listed
    /// elements, at a model revision.
    snaps_3d: Option<(u64, Vec<ElementId>, Snapper)>,
    /// First click of a two-click tool in the 3D view, on a level.
    pending_3d: Option<(DVec2, ElementId)>,
    extrude_distance: f64,
    path: String,
    status: String,
    /// Linked files (Blender...) and when they were last read.
    watch: crate::files::Watch,
    /// Reload linked files as soon as they are saved.
    live_reload: bool,
}

impl CreaBimApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let viewport = cc
            .wgpu_render_state
            .as_ref()
            .map(|rs| Viewport3d::new(&rs.device));
        let mut app = Self {
            doc: Document::new(),
            selection: Selection::default(),
            face: None,
            active_view: None,
            sheet: SheetState::default(),
            drawings: HashMap::new(),
            camera: OrbitCamera::default(),
            aspect: 0.65,
            viewport,
            geometry_key: None,
            scene: vec![],
            plan: PlanState::default(),
            placement: PlanPlacement::Docked,
            plan_in_3d: false,
            layout: PaneLayout::default(),
            handle_drag: None,
            snaps_3d: None,
            pending_3d: None,
            extrude_distance: 1.0,
            path: "project.creabim".into(),
            watch: Default::default(),
            live_reload: true,
            status: "Loaded the sample house. Orbit with left drag, pan with right drag, zoom with the wheel.".into(),
        };
        app.load_model(sample_house());
        app
    }

    fn load_model(&mut self, model: Model) {
        self.doc = Document::from_model(with_plan_views(model));
        self.selection.clear();
        self.face = None;
        self.active_view = None;
        self.sheet = SheetState::default();
        self.drawings.clear();
        self.geometry_key = None;
        self.drop_stale_selection();
        self.frame_all();
    }

    /// Open a project file, or import and link any other file.
    fn open_path(&mut self, path: &str) {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".ifc") || lower.ends_with(".ifcx") {
            let r = std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|t| {
                    if lower.ends_with(".ifcx") {
                        creabim_ifc::import_ifcx(&t)
                    } else {
                        creabim_ifc::import_ifc(&t)
                    }
                    .map_err(|e| e.to_string())
                });
            match r {
                Ok(imported) => {
                    let n = imported.model.len();
                    let warnings = imported.warnings.len();
                    self.load_model(imported.model);
                    self.watch = Default::default();
                    self.path = with_extension(path, "creabim");
                    self.status = format!(
                        "Opened {} ({}): {n} elements{}. Save keeps it as {}.",
                        file_label(path),
                        imported.schema,
                        if warnings > 0 {
                            format!(", {warnings} notes (see the terminal)")
                        } else {
                            String::new()
                        },
                        file_label(&self.path)
                    );
                    for w in &imported.warnings {
                        eprintln!("IFC import: {w}");
                    }
                }
                Err(e) => self.status = format!("Couldn't open {path}: {e}"),
            }
            return;
        }
        if !crate::files::is_project(path) {
            self.link_file(path);
            return;
        }
        match creabim_store::load(path) {
            Ok(doc) => {
                let model = doc.model().clone();
                self.load_model(model.clone());
                // keep the saved undo history when no views had to be added
                if self.doc.model() == &model {
                    self.doc = doc;
                }
                self.watch = Default::default();
                self.status = format!("Opened {path}");
            }
            Err(e) => self.status = format!("Couldn't open {path}: {e}"),
        }
    }

    fn save(&mut self) {
        if self.path.to_ascii_lowercase().ends_with(".json") {
            let r = std::fs::write(&self.path, self.doc.model().to_json());
            self.report(r, format!("Saved {}", self.path));
            return;
        }
        if !self.path.to_ascii_lowercase().ends_with(".creabim") {
            self.path = with_extension(&self.path, "creabim");
        }
        let r = creabim_store::save(&self.path, &self.doc);
        self.report(r, format!("Saved {}", self.path));
    }

    /// Bring a file's objects in as linked elements.
    fn link_file(&mut self, path: &str) {
        let full = std::fs::canonicalize(path)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.to_string());
        if crate::files::linked_files(self.doc.model()).contains(&full) {
            self.reload(&full);
            return;
        }
        match crate::files::read_incoming(&full) {
            Ok(incoming) => {
                let (tx, report) = creabim_exchange::sync(self.doc.model(), &full, &incoming);
                let r = self.doc.commit(tx);
                self.watch.mark(&full);
                self.drawings.clear();
                self.frame_all();
                self.report(
                    r,
                    format!(
                        "Linked {}: {}. Pick an object and use Assign as to make it a wall, slab, door or window.",
                        file_label(&full),
                        report.summary()
                    ),
                );
            }
            Err(e) => self.status = format!("Couldn't read {path}: {e}"),
        }
    }

    /// Read a linked file again and update the elements it made.
    fn reload(&mut self, file: &str) {
        match crate::files::read_incoming(file) {
            Ok(incoming) => {
                let (tx, report) = creabim_exchange::sync(self.doc.model(), file, &incoming);
                self.watch.mark(file);
                if tx.ops.is_empty() {
                    self.status = format!("{} is up to date.", file_label(file));
                    return;
                }
                let r = self.doc.commit(tx);
                self.drop_stale_selection();
                self.report(
                    r,
                    format!("Reloaded {}: {}.", file_label(file), report.summary()),
                );
            }
            Err(e) => self.status = format!("Couldn't reload {file}: {e}"),
        }
    }

    /// "Assign as": give a mesh a building meaning, or change its class.
    fn assign_panel(&mut self, ui: &mut egui::Ui, id: ElementId) {
        let Some(e) = self.doc.model().get(id) else {
            return;
        };
        if !matches!(
            e.kind,
            ElementKind::FreeMesh(_)
                | ElementKind::Wall(_)
                | ElementKind::Slab(_)
                | ElementKind::Opening(_)
        ) {
            return;
        }
        if let Some(s) = &e.source {
            ui.label(
                egui::RichText::new(format!("From {}: {}", file_label(&s.file), s.object)).weak(),
            );
        }
        let is_mesh = matches!(e.kind, ElementKind::FreeMesh(_));
        let mut pick = None;
        egui::ComboBox::from_id_salt("assign_as")
            .selected_text("Assign as…")
            .show_ui(ui, |ui| {
                for a in creabim_exchange::Assign::menu() {
                    if matches!(a, creabim_exchange::Assign::Class(_)) && !is_mesh {
                        continue;
                    }
                    if ui.selectable_label(false, a.label()).clicked() {
                        pick = Some(a);
                    }
                }
            });
        if is_mesh {
            ui.label(
                egui::RichText::new(
                    "Make this shape a wall, slab, door or window sized to fit it, or give it a class.",
                )
                .weak(),
            );
        }
        if let Some(a) = pick {
            let before = e.clone();
            match creabim_exchange::assign(self.doc.model(), id, &a) {
                Ok(after) => {
                    let r = self.doc.commit(
                        Transaction::new(format!("Assign as {}", a.label())).replace(before, after),
                    );
                    self.report(
                        r,
                        format!("Now a {}. Its handles resize it.", a.label().to_lowercase()),
                    );
                }
                Err(why) => self.status = why,
            }
        }
    }

    /// Sheets in number order, as (id, "A101 Floor Plans").
    fn sheet_list(&self) -> Vec<(ElementId, String)> {
        sheets::sheets(self.doc.model())
            .into_iter()
            .map(|(id, s)| {
                let name = self.doc.model().get(id).map_or("", |e| e.name.as_str());
                (id, format!("{} {name}", s.number))
            })
            .collect()
    }

    fn is_sheet(&self, id: ElementId) -> bool {
        sheets::sheet_of(self.doc.model(), id).is_some()
    }

    fn sheet_info(&self) -> SheetInfo {
        SheetInfo {
            project: stem(&self.path),
            date: sheets::today(),
        }
    }

    /// A sheet's views with their (cached) drawings.
    fn placed_views(&mut self, sheet: ElementId) -> Vec<PlacedView> {
        let ids: Vec<ElementId> = self
            .doc
            .model()
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Viewport(vp) if vp.sheet == sheet => Some(vp.view),
                _ => None,
            })
            .collect();
        for id in ids {
            self.drawing(id);
        }
        let cache = &self.drawings;
        sheets::placed_views(self.doc.model(), sheet, &mut |v| {
            cache.get(&v).map(|(_, d)| d.clone())
        })
    }

    /// Every view in the model, plans first, in model order.
    fn views(&self) -> Vec<(ElementId, String, bool)> {
        let mut v: Vec<_> = self
            .doc
            .model()
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::View(v) => Some((
                    e.id,
                    e.name.clone(),
                    matches!(v.kind, ViewKind::Plan { .. }),
                )),
                _ => None,
            })
            .collect();
        v.sort_by_key(|(_, _, plan)| !plan);
        v
    }

    fn view(&self, id: ElementId) -> Option<&View> {
        match self.doc.model().get(id).map(|e| &e.kind) {
            Some(ElementKind::View(v)) => Some(v),
            _ => None,
        }
    }

    /// The drawing of a view, regenerated only when the model has changed.
    fn drawing(&mut self, id: ElementId) -> Option<Drawing> {
        let revision = self.doc.model().revision();
        if let Some((r, d)) = self.drawings.get(&id) {
            if *r == revision {
                return Some(d.clone());
            }
        }
        let d = view_drawing(self.doc.model(), id)?;
        self.drawings.insert(id, (revision, d.clone()));
        Some(d)
    }

    fn frame_all(&mut self) {
        let scene = build_scene(self.doc.model());
        let bounds = scene
            .iter()
            .filter_map(|s| s.mesh.bounds())
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)));
        if let Some((lo, hi)) = bounds {
            self.camera.frame(lo, hi, self.aspect);
        }
        self.plan.cameras.clear();
    }

    fn report(&mut self, r: Result<(), impl ToString>, ok: impl Into<String>) {
        self.status = match r {
            Ok(()) => ok.into(),
            Err(e) => format!("Couldn't do that: {}", e.to_string()),
        };
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (undo, redo, delete, extrude) = ctx.input(|i| {
            let cmd = i.modifiers.command;
            (
                cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
                cmd && (i.key_pressed(egui::Key::Y)
                    || (i.modifiers.shift && i.key_pressed(egui::Key::Z))),
                i.key_pressed(egui::Key::Delete),
                !cmd && i.key_pressed(egui::Key::E),
            )
        });
        if undo {
            let label = self.doc.undo_label().map(str::to_owned);
            if self.doc.undo() {
                self.status = format!("Undid: {}", label.unwrap_or_default());
            }
        }
        if redo {
            let label = self.doc.redo_label().map(str::to_owned);
            if self.doc.redo() {
                self.status = format!("Redid: {}", label.unwrap_or_default());
            }
        }
        if delete {
            self.delete_selected();
        }
        if extrude {
            let sketch = self
                .selection
                .active
                .and_then(|id| self.doc.model().get(id))
                .is_some_and(|e| matches!(e.kind, ElementKind::Sketch(_)));
            if sketch {
                self.extrude_sketch();
            } else {
                self.extrude_face();
            }
        }
        self.drop_stale_selection();
    }

    fn drop_stale_selection(&mut self) {
        if self
            .selection
            .active
            .is_some_and(|id| self.doc.model().get(id).is_none())
        {
            self.selection.clear();
            self.face = None;
        }
        if self
            .active_view
            .is_none_or(|id| self.view(id).is_none() && !self.is_sheet(id))
        {
            self.active_view = self.views().first().map(|v| v.0);
        }
    }

    fn delete_selected(&mut self) {
        let Some(id) = self.selection.active else { return };
        let name = self
            .doc
            .model()
            .get(id)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        let tx = Transaction::new(format!("Delete {name}")).remove_cascade(self.doc.model(), id);
        let r = self.doc.commit(tx);
        self.report(r, format!("Deleted {name} and anything attached to it."));
        self.drop_stale_selection();
    }

    fn extrude_face(&mut self) {
        let (Some(id), Some(face)) = (self.selection.active, self.face) else {
            return;
        };
        let d = self.extrude_distance;
        let r = self.doc.edit(id, "Extrude face", |e| {
            if let ElementKind::FreeMesh(m) = &mut e.kind {
                if face < m.mesh.faces.len() {
                    m.mesh.extrude_face(face, d);
                }
            }
        });
        self.report(
            r,
            format!("Extruded face {face} by {d:.2} m. Press E again to keep going."),
        );
    }

    fn extrude_sketch(&mut self) {
        let Some(id) = self.selection.active else { return };
        let d = self.extrude_distance;
        match crate::sketch::extrude(&mut self.doc, id, d) {
            Ok((mass, cap)) => {
                self.selection.only(mass);
                self.face = cap;
                self.status = match cap {
                    Some(_) => format!(
                        "Extruded the sketch {d:.2} m into a mass. Its far face is selected: press E to push it further."
                    ),
                    None => format!(
                        "Extruded the open sketch {d:.2} m into a surface. Add a Solidify modifier to give it thickness."
                    ),
                };
            }
            Err(e) => self.status = e,
        }
    }

    // ---- 3D -------------------------------------------------------------

    fn rebuild_geometry(&mut self, rs: &egui_wgpu::RenderState) {
        let key = (
            self.doc.model().revision(),
            self.selection.clone(),
            self.face,
            self.plan_in_3d,
            self.active_view,
        );
        if self.geometry_key.as_ref() == Some(&key) {
            return;
        }
        self.geometry_key = Some(key);
        self.scene = build_scene(self.doc.model());

        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let highlight = [0.95, 0.55, 0.20];
        let mut ghost_lines: Vec<(DVec3, DVec3, [f32; 3])> = Vec::new();
        for item in &self.scene {
            let selected = self.selection.contains(item.id);
            if item.ghost {
                // cutters show as their outline only
                let c = if selected {
                    highlight
                } else {
                    [0.35, 0.45, 0.75]
                };
                for (a, b) in snaps::feature_edges(&item.mesh) {
                    ghost_lines.push((a, b, c));
                }
                continue;
            }
            let base = vertices.len() as u32;
            let face_tris = match (
                selected,
                self.face,
                self.doc.model().get(item.id).map(|e| &e.kind),
            ) {
                (true, Some(f), Some(ElementKind::FreeMesh(m))) => {
                    // the face and its copies made by modifiers
                    let ev = m.evaluated();
                    let tris: Vec<usize> = (0..ev.mesh.faces.len())
                        .filter(|&i| ev.source[i] == Some(f))
                        .flat_map(|i| face_triangles(&ev.mesh, i))
                        .collect();
                    Some(tris)
                }
                _ => None,
            };
            let mut colors = vec![
                if selected && face_tris.is_none() {
                    highlight
                } else {
                    item.color
                };
                item.mesh.positions.len()
            ];
            if let Some(tris) = &face_tris {
                for &t in tris {
                    for k in 0..3 {
                        // a cut mesh no longer matches its faces one to one
                        if let Some(&v) = item.mesh.indices.get(t * 3 + k) {
                            colors[v as usize] = highlight;
                        }
                    }
                }
            }
            for (i, p) in item.mesh.positions.iter().enumerate() {
                vertices.push(Vertex {
                    pos: *p,
                    normal: item.mesh.normals[i],
                    color: colors[i],
                });
            }
            indices.extend(item.mesh.indices.iter().map(|i| i + base));
        }

        // ground grid, 1 m spacing
        let lo = self
            .scene
            .iter()
            .filter_map(|s| s.mesh.bounds())
            .map(|b| b.0.z)
            .fold(0.0f32, f32::min)
            - 0.01;
        let mut lines = Vec::new();
        for (a, b, c) in &ghost_lines {
            lines.push(line_vertex(a.as_vec3().to_array(), *c));
            lines.push(line_vertex(b.as_vec3().to_array(), *c));
        }
        let grid = [0.80, 0.80, 0.82];
        for i in -20..=30 {
            let v = i as f32;
            for (a, b) in [([v, -20.0], [v, 30.0]), ([-20.0, v], [30.0, v])] {
                lines.push(line_vertex([a[0], a[1], lo], grid));
                lines.push(line_vertex([b[0], b[1], lo], grid));
            }
        }

        // 2D drawing shown in 3D space, on its level
        let mut overlay = Vec::new();
        if self.plan_in_3d {
            let placed = self.active_view.and_then(|id| {
                let kind = self.view(id)?.kind.clone();
                Some((kind, self.drawing(id)?))
            });
            if let Some((kind, plan)) = placed {
                // drawing coordinates → world: plans lie flat at their cut
                // height, sections stand on their section line
                let to_world: Option<Box<dyn Fn(DVec2) -> [f32; 3]>> = match kind {
                    ViewKind::Plan { level, cut_height } => {
                        let z = self
                            .doc
                            .model()
                            .level(level)
                            .map_or(0.0, |l| l.elevation + cut_height)
                            as f32;
                        Some(Box::new(move |p| [p.x as f32, p.y as f32, z]))
                    }
                    ViewKind::Section { start, end, .. } => {
                        let along = (end - start).normalize_or_zero();
                        Some(Box::new(move |p| {
                            let w = start + along * p.x;
                            [w.x as f32, w.y as f32, p.y as f32]
                        }))
                    }
                    // 3D views and schedules have no place of their own in the model
                    ViewKind::Axonometric { .. } | ViewKind::Schedule { .. } => None,
                };
                let to_world = to_world.unwrap_or_else(|| Box::new(|_| [0.0; 3]));
                let plan = if matches!(
                    kind,
                    ViewKind::Axonometric { .. } | ViewKind::Schedule { .. }
                ) {
                    Drawing::default()
                } else {
                    plan
                };
                let mut seg = |a: DVec2, b: DVec2, c: [f32; 3]| {
                    overlay.push(line_vertex(to_world(a), c));
                    overlay.push(line_vertex(to_world(b), c));
                };
                for it in &plan.items {
                    let c = match it {
                        PlanItem::Polygon { pen: Pen::Cut, .. }
                        | PlanItem::Line { pen: Pen::Cut, .. } => [0.85, 0.15, 0.10],
                        _ => [0.15, 0.25, 0.55],
                    };
                    match it {
                        PlanItem::Polygon { points, .. } => {
                            for i in 0..points.len() {
                                seg(points[i], points[(i + 1) % points.len()], c);
                            }
                        }
                        // guides show as their own lines in 3D
                        PlanItem::Line {
                            pen: Pen::Guide, ..
                        } => {}
                        PlanItem::Line { a, b, .. } => seg(*a, *b, c),
                        PlanItem::Arc {
                            center,
                            radius,
                            start,
                            end,
                            ..
                        } => {
                            let n = 16;
                            for i in 0..n {
                                let t0 = start + (end - start) * i as f64 / n as f64;
                                let t1 = start + (end - start) * (i + 1) as f64 / n as f64;
                                seg(
                                    *center + DVec2::from_angle(t0) * *radius,
                                    *center + DVec2::from_angle(t1) * *radius,
                                    c,
                                );
                            }
                        }
                        PlanItem::Dimension { a, b, offset, .. } => {
                            let n = (*b - *a).normalize_or_zero().perp() * *offset;
                            seg(*a + n, *b + n, [0.12, 0.24, 0.47]);
                        }
                        PlanItem::Label { .. }
                        | PlanItem::Fill { .. }
                        | PlanItem::Text { .. }
                        | PlanItem::Tag { .. }
                        | PlanItem::Image { .. } => {}
                    }
                }
            }
        }

        if let Some(vp) = &mut self.viewport {
            vp.set_geometry(&rs.device, &vertices, &indices, &lines, &overlay);
        }
    }

    fn show_3d(&mut self, ui: &mut egui::Ui, rect: Rect, frame: &eframe::Frame) {
        let response = ui.interact(rect, ui.id().with("3d"), Sense::click_and_drag());
        let Some(rs) = frame.wgpu_render_state() else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "3D needs the wgpu renderer",
                egui::FontId::proportional(16.0),
                Color32::RED,
            );
            return;
        };
        let modal_here = self.plan.modal.as_ref().is_some_and(|m| m.pane.is_none());
        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            let d = response.drag_delta();
            self.camera.pan(GVec2::new(d.x, d.y), rect.height());
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.camera.zoom(scroll / 50.0);
            }
        }
        let aspect = rect.width() / rect.height().max(1.0);
        self.aspect = aspect;

        self.rebuild_geometry(rs);
        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
        if let Some(vp) = &mut self.viewport {
            let tex = vp.render(rs, size, self.camera.view_proj(aspect), [0.93, 0.94, 0.96]);
            ui.painter().image(
                tex,
                rect,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        ui.painter().text(
            rect.left_top() + Vec2::new(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            "3D",
            egui::FontId::proportional(13.0),
            Color32::from_gray(80),
        );

        // overlays and pointer input, drawn over the rendered image
        let painter = ui.painter_at(rect);
        let cam = self.camera.clone();
        let view_proj = cam.view_proj(aspect);
        let project = move |p: DVec3| -> Option<egui::Pos2> {
            let c = view_proj * p.as_vec3().extend(1.0);
            if c.w <= 1e-4 {
                return None;
            }
            let n = c.truncate() / c.w;
            Some(egui::pos2(
                rect.left() + (n.x + 1.0) / 2.0 * rect.width(),
                rect.top() + (1.0 - n.y) / 2.0 * rect.height(),
            ))
        };
        let ray = |p: egui::Pos2| {
            let ndc = GVec2::new(
                (p.x - rect.left()) / rect.width() * 2.0 - 1.0,
                1.0 - (p.y - rect.top()) / rect.height() * 2.0,
            );
            let (o, d) = cam.ray(ndc, aspect);
            (o.as_dvec3(), d.as_dvec3())
        };
        let hover = response.hover_pos();
        self.draw_sketches(&painter, &project);

        // a keyboard move, rotate or scale running here
        if modal_here {
            let eye = cam.eye().as_dvec3();
            let to_model = |pos: egui::Pos2, axis: Option<Axis>, pivot: DVec3| {
                let (o, d) = ray(pos);
                let plane = if axis == Some(Axis::Z) {
                    // stand a plane up through the pivot, facing the camera
                    let f = (pivot - eye).truncate().normalize_or(DVec2::X);
                    Workplane::facing(pivot, -f.extend(0.0))
                } else {
                    Workplane::ground(pivot.z)
                };
                plane.intersect(o, d)
            };
            let map = PaneMap {
                to_model: &to_model,
                project: &project,
            };
            let ctx = ui.ctx().clone();
            if let Some(m) = self.plan.modal.as_mut() {
                match m.update(&mut self.doc, &ctx, &painter, hover, &map) {
                    Outcome::Running(s) => self.status = s,
                    Outcome::Done(s) | Outcome::Cancelled(s) => {
                        self.status = s;
                        self.plan.modal = None;
                    }
                }
            }
            ui.ctx().request_repaint();
            return;
        }
        if response.hovered() && self.plan.modal.is_none() && !ui.ctx().egui_wants_keyboard_input()
        {
            if let (Some(keys), Some(id), Some(grab)) =
                (planview::start_keys(ui.ctx()), self.selection.active, hover)
            {
                self.plan.cancel();
                match planview::start_modal(
                    &mut self.doc,
                    keys,
                    id,
                    self.face,
                    Space::Model,
                    None,
                    grab,
                ) {
                    Ok((m, target)) => {
                        if target != id {
                            self.selection.only(target);
                            self.face = None;
                        }
                        self.status = format!(
                            "{:?}: move the pointer, or type a value. X, Y or Z keeps to an axis.",
                            m.mode
                        );
                        self.plan.modal = Some(m);
                    }
                    Err(e) => self.status = e,
                }
                return;
            }
        }

        if self.plan.tool == Tool::Sketch {
            if response.dragged_by(egui::PointerButton::Primary) {
                let d = response.drag_delta();
                self.camera.orbit(GVec2::new(d.x, d.y));
            }
            self.sketch_3d(ui, &response, &painter, &project, &ray, &cam);
            return;
        }

        // handles on the selection, and dragging the selection itself
        let exclude = self
            .selection
            .active
            .map(|id| planview::joined_with(&self.doc, id))
            .unwrap_or_default();
        self.refresh_snaps_3d(&exclude);
        let dragging = {
            let snapper = &self.snaps_3d.as_ref().expect("just built").2;
            let scene = &self.scene;
            let ray_fn = |p: egui::Pos2| {
                let (o, d) = ray(p);
                Some(Ray { o, d })
            };
            let snap = |p: egui::Pos2, from: Option<DVec3>| {
                let (o, d) = ray(p);
                let free = Workplane::ground(from.map_or(0.0, |f| f.z)).intersect(o, d);
                let s = snapper.snap(p, &project, from, free)?;
                Some((s.p, s))
            };
            let grab_body = |p: egui::Pos2, id: ElementId| {
                let (o, d) = ray(p);
                let h = pick::pick(scene, o.as_vec3(), d.as_vec3())?;
                (h.id == id).then(|| o + d * h.t as f64)
            };
            let (_, look) = ray(rect.center());
            let input = PaneInput {
                ray: &ray_fn,
                project: &project,
                look,
                snap: &snap,
                grab_body: &grab_body,
                body_normal: DVec3::Z,
            };
            let mut status = None;
            let busy = self.plan.tool == Tool::Select
                && handles::interact(
                    &mut self.doc,
                    &painter,
                    &response,
                    self.selection.active,
                    self.face,
                    &mut self.handle_drag,
                    &input,
                    &mut status,
                );
            if let Some(s) = status {
                self.status = s;
            }
            busy
        };
        if dragging {
            return;
        }
        if response.dragged_by(egui::PointerButton::Primary) {
            let d = response.drag_delta();
            self.camera.orbit(GVec2::new(d.x, d.y));
        }

        if self.plan.outline.is_some() {
            self.outline_3d(ui, &response, &painter, &project, &ray);
            return;
        }
        if self.plan.tool != Tool::Select {
            self.tools_3d(ui, &response, &painter, &project, &ray);
            return;
        }

        if response.clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                // sketch lines first: they have no surface to hit
                let near_sketch = creabim_geom::scene::sketch_lines(self.doc.model())
                    .into_iter()
                    .filter_map(|(id, a, b)| {
                        let (a, b) = (project(a)?, project(b)?);
                        let d = egui::Rect::from_two_pos(a, b)
                            .expand(6.0)
                            .contains(p)
                            .then(|| {
                                let ab = b - a;
                                let t =
                                    ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                                (a + ab * t).distance(p)
                            })?;
                        (d < 6.0).then_some((id, d))
                    })
                    .min_by(|x, y| x.1.total_cmp(&y.1));
                if let Some((id, _)) = near_sketch {
                    self.selection.only(id);
                    self.face = None;
                    return;
                }
                let (o, d) = ray(p);
                let hit = pick::pick(&self.scene, o.as_vec3(), d.as_vec3());
                // guides cross everything, so they only take clicks that miss
                if hit.is_none() {
                    if let Some(id) = self.guide_near(p, &project) {
                        self.selection.only(id);
                        self.face = None;
                        return;
                    }
                }
                self.selection.set(hit.map(|h| h.id));
                self.face = hit.and_then(|h| match self.doc.model().get(h.id).map(|e| &e.kind) {
                    Some(ElementKind::FreeMesh(m)) => {
                        let ev = m.evaluated();
                        let f = pick::face_of_triangle(&ev.mesh, h.triangle)?;
                        ev.source.get(f).copied().flatten()
                    }
                    _ => None,
                });
            }
        }
    }

    /// Rebuild the 3D snaps when the model changed, but not during a drag,
    /// which changes the model every frame.
    fn refresh_snaps_3d(&mut self, exclude: &[ElementId]) {
        let rev = self.doc.model().revision();
        let fresh = self
            .snaps_3d
            .as_ref()
            .is_some_and(|(r, ex, _)| ex == exclude && (*r == rev || self.handle_drag.is_some()));
        if !fresh {
            let mut snapper = Snapper::from_scene(&self.scene, exclude);
            snapper.add_guides(self.doc.model(), GUIDE_REACH_3D);
            self.snaps_3d = Some((rev, exclude.to_vec(), snapper));
        }
    }

    /// The level the 3D drawing tools draw on: the active plan's, else the
    /// selected element's, else the lowest.
    fn work_level(&self) -> Option<(ElementId, f64)> {
        let m = self.doc.model();
        let plan = self
            .active_view
            .and_then(|v| match self.view(v).map(|v| &v.kind) {
                Some(ViewKind::Plan { level, .. }) => Some(*level),
                _ => None,
            });
        let of_selection = self
            .selection
            .active
            .and_then(|id| match m.get(id).map(|e| &e.kind) {
                Some(ElementKind::Wall(w)) => Some(w.level),
                Some(ElementKind::Slab(s)) => Some(s.level),
                Some(ElementKind::FreeMesh(f)) => f.level,
                Some(ElementKind::Level(_)) => Some(id),
                _ => None,
            });
        let lowest = m
            .levels()
            .min_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation))
            .map(|(e, _)| e.id);
        let id = plan.or(of_selection).or(lowest)?;
        Some((id, m.level(id).map_or(0.0, |l| l.elevation)))
    }

    /// Walls, slabs, doors, windows, masses and section lines drawn right in
    /// the 3D view, on the working level, with the same snaps as dragging.
    fn tools_3d(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
    ) {
        let tool = self.plan.tool;
        if matches!(tool, Tool::Dimension | Tool::Tag) {
            if response.clicked() {
                self.status = format!(
                    "The {} tool works in plans and sections for now.",
                    tool.label()
                );
            }
            return;
        }
        let Some((level, z)) = self.work_level() else {
            return;
        };
        let accent = Color32::from_rgb(230, 120, 30);
        let stroke = egui::Stroke::new(2.0, accent);
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) || response.secondary_clicked() {
            self.pending_3d = None;
        }
        let Some(hover) = response.hover_pos() else {
            return;
        };
        let (o, d) = ray(hover);
        let hit = pick::pick(&self.scene, o.as_vec3(), d.as_vec3());
        let level_name = self
            .doc
            .model()
            .get(level)
            .map_or(String::new(), |e| e.name.clone());

        // doors and windows go into the wall under the pointer
        if matches!(tool, Tool::Door | Tool::Window) {
            let wall = hit.and_then(|h| match self.doc.model().get(h.id).map(|e| &e.kind) {
                Some(ElementKind::Wall(w)) => Some((h, w.level)),
                Some(ElementKind::Opening(op)) => {
                    match self.doc.model().get(op.host).map(|e| &e.kind) {
                        Some(ElementKind::Wall(w)) => Some((h, w.level)),
                        _ => None,
                    }
                }
                _ => None,
            });
            if let Some((h, wl)) = wall {
                let at = o + d * h.t as f64;
                if let Some(c) = project(at) {
                    painter.circle_stroke(c, 6.0, stroke);
                }
                if response.clicked() {
                    let mut sel = self.selection.clone();
                    let mut state = std::mem::take(&mut self.plan);
                    let r = planview::plan_tool(
                        &mut self.doc,
                        wl,
                        tool,
                        at.truncate(),
                        &mut state,
                        &mut sel,
                    );
                    self.plan = state;
                    self.selection = sel;
                    self.report(
                        r,
                        format!(
                            "{} placed. Drag its round handles to resize it.",
                            tool.label()
                        ),
                    );
                }
            } else if response.clicked() {
                self.status = format!(
                    "Click a wall to put the {} in it.",
                    tool.label().to_lowercase()
                );
            }
            return;
        }

        // everything else is drawn on the working level
        let from = self
            .pending_3d
            .filter(|(_, l)| *l == level)
            .map(|(p, _)| p.extend(z));
        let free = Workplane::ground(z).intersect(o, d);
        let exclude: Vec<ElementId> = vec![];
        self.refresh_snaps_3d(&exclude);
        let snapper = &self.snaps_3d.as_ref().expect("just built").2;
        let Some(snap) = snapper.snap(hover, project, from, free) else {
            return;
        };
        let p = snap.p.truncate();
        let at = p.extend(z);
        if let Some(c) = project(at) {
            if snap.kind == SnapKind::Grid {
                painter.circle_stroke(c, 4.0, egui::Stroke::new(1.0, accent));
            } else {
                snaps::draw(painter, snap, c, from.and_then(project));
            }
        }
        if let Some(a) = from {
            match tool {
                Tool::Guide => {
                    let (through, dir) = match self.plan.guide_edge {
                        Some((e0, e1)) => (at, (e1 - e0).extend(0.0)),
                        None => (a, at - a),
                    };
                    draw_guide_3d(painter, project, through, through + dir, stroke);
                    if let Some(sb) = project(at) {
                        let text = match self.plan.guide_edge {
                            Some((e0, e1)) => {
                                let d = (e1 - e0).normalize_or_zero();
                                format!("{:.2} m", (p - e0).perp_dot(d).abs())
                            }
                            None => format!("{:.2} m", a.truncate().distance(p)),
                        };
                        painter.text(
                            sb + Vec2::new(10.0, -12.0),
                            egui::Align2::LEFT_BOTTOM,
                            text,
                            egui::FontId::proportional(13.0),
                            accent,
                        );
                    }
                }
                Tool::Slab => {
                    let (lo, hi) = (a.truncate().min(p), a.truncate().max(p));
                    let pts = [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y), lo];
                    let scr: Vec<egui::Pos2> =
                        pts.iter().filter_map(|q| project(q.extend(z))).collect();
                    painter.add(egui::Shape::line(scr, stroke));
                }
                _ => {
                    if let (Some(sa), Some(sb)) = (project(a), project(at)) {
                        painter.line_segment([sa, sb], stroke);
                        painter.text(
                            sb + Vec2::new(10.0, -12.0),
                            egui::Align2::LEFT_BOTTOM,
                            format!("{:.2} m", a.truncate().distance(p)),
                            egui::FontId::proportional(13.0),
                            accent,
                        );
                    }
                    if tool == Tool::Wall {
                        // the wall's height, so it reads as a wall
                        let h = self.doc.model().level(level).map_or(3.0, |l| l.height);
                        let up = DVec3::Z * h;
                        let quad = [a, at, at + up, a + up, a];
                        let scr: Vec<egui::Pos2> =
                            quad.iter().filter_map(|q| project(*q)).collect();
                        painter.add(egui::Shape::line(scr, egui::Stroke::new(1.0, accent)));
                    }
                }
            }
        }
        if response.clicked() && tool == Tool::Guide && from.is_none() {
            // clicking an edge first draws the guide parallel to it
            self.plan.guide_edge = matches!(
                snap.kind,
                SnapKind::OnEdge | SnapKind::Guide | SnapKind::Midpoint
            )
            .then(|| snapper.edge_at(snap.p))
            .flatten()
            .filter(|(a, b)| a.truncate().distance(b.truncate()) > 1e-3)
            .map(|(a, b)| (a.truncate(), b.truncate()));
        }
        if response.clicked() {
            let mut sel = self.selection.clone();
            let mut state = std::mem::take(&mut self.plan);
            state.pending = self.pending_3d.filter(|(_, l)| *l == level).map(|(p, _)| p);
            let r = planview::plan_tool(&mut self.doc, level, tool, p, &mut state, &mut sel);
            self.pending_3d = state.pending.take().map(|p| (p, level));
            self.plan = state;
            self.selection = sel;
            if let Err(e) = r {
                self.status = format!("Couldn't do that: {e}");
            } else {
                self.status = match (tool, self.pending_3d) {
                    (Tool::Wall, Some(_)) => format!("Drawing walls on {level_name}. Click the next corner; Esc or right-click ends."),
                    (Tool::Slab | Tool::Section | Tool::Elevation, Some(_)) => "Click the second corner.".into(),
                    (Tool::Guide, Some(_)) if self.plan.guide_edge.is_some() => "Click where the parallel guide goes.".into(),
                    (Tool::Guide, Some(_)) => "Click a second point the guide runs through.".into(),
                    _ => format!("Added on {level_name}."),
                };
            }
        } else if from.is_none() && response.hovered() {
            self.status = format!(
                "{} on {level_name} (the open plan's level). {}",
                tool.label(),
                tool.hint()
            );
        }
    }

    /// Redrawing a slab or wall in the 3D view, on its level, with snaps.
    fn outline_3d(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
    ) {
        let Some(outline) = self.plan.outline.clone() else {
            return;
        };
        let (enter, escape) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if escape {
            self.plan.outline = None;
            self.status = "Redraw cancelled.".into();
            return;
        }
        let exclude = [outline.target];
        self.refresh_snaps_3d(&exclude);
        let hover = response.hover_pos().and_then(|h| {
            let (o, d) = ray(h);
            let free = Workplane::ground(outline.z).intersect(o, d);
            let from = outline.points.last().map(|p| p.extend(outline.z));
            let snapper = &self.snaps_3d.as_ref()?.2;
            let s = snapper.snap(h, project, from, free)?;
            if let Some(c) = project(s.p.truncate().extend(outline.z)) {
                if s.kind != SnapKind::Grid {
                    snaps::draw(painter, s, c, from.and_then(project));
                }
            }
            Some((h, s.p.truncate()))
        });
        outline.draw(painter, project, hover.map(|h| h.1));
        let finish = enter || (response.secondary_clicked() && !outline.points.is_empty());
        let mut closed = false;
        if response.clicked() {
            if let Some((h, p)) = hover {
                let near = outline.near_first(h, project);
                let o = self.plan.outline.as_mut().expect("checked");
                closed = o.click(p, near);
            }
        }
        if finish || closed {
            if let Some(o) = self.plan.outline.take() {
                let r = o.finish(&mut self.doc, closed);
                match r {
                    Ok(msg) => self.status = msg,
                    Err(e) => self.status = format!("Couldn't redraw: {e}"),
                }
            }
        } else {
            self.status = outline.hint().into();
        }
    }

    /// The guide within a few pixels of `p` in the 3D view.
    fn guide_near(
        &self,
        p: egui::Pos2,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) -> Option<ElementId> {
        let mut best: Option<(ElementId, f32)> = None;
        for e in self.doc.model().iter() {
            let ElementKind::Guide(g) = &e.kind else {
                continue;
            };
            let Some(dir) = (g.b - g.a).try_normalize() else {
                continue;
            };
            let m = (g.a + g.b) / 2.0;
            let steps = 240;
            let pts: Vec<Option<egui::Pos2>> = (0..=steps)
                .map(|i| {
                    let t = -GUIDE_REACH_3D + 2.0 * GUIDE_REACH_3D * i as f64 / steps as f64;
                    project(m + dir * t)
                })
                .collect();
            for w in pts.windows(2) {
                let (Some(a), Some(b)) = (w[0], w[1]) else {
                    continue;
                };
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                let d = (a + ab * t).distance(p);
                if d < 5.0 && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((e.id, d));
                }
            }
        }
        best.map(|(id, _)| id)
    }

    /// Every sketch as lines over the 3D view, and every guide dashed; the
    /// selected one stands out.
    fn draw_sketches(
        &self,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) {
        for e in self.doc.model().iter() {
            let ElementKind::Guide(g) = &e.kind else {
                continue;
            };
            let stroke = if self.selection.contains(e.id) {
                egui::Stroke::new(2.0, Color32::from_rgb(242, 140, 51))
            } else {
                egui::Stroke::new(1.0, planview::GUIDE_COLOR)
            };
            draw_guide_3d(painter, project, g.a, g.b, stroke);
        }
        for (id, a, b) in creabim_geom::scene::sketch_lines(self.doc.model()) {
            let selected = self.selection.contains(id);
            let stroke = if selected {
                egui::Stroke::new(2.5, Color32::from_rgb(242, 140, 51))
            } else {
                egui::Stroke::new(1.5, Color32::from_rgb(40, 70, 140))
            };
            if let (Some(a), Some(b)) = (project(a), project(b)) {
                painter.line_segment([a, b], stroke);
            }
        }
    }

    /// The Sketch tool in the 3D view: the first click sets the workplane on
    /// the face under the pointer (or the ground), later clicks add corners.
    fn sketch_3d(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
        cam: &OrbitCamera,
    ) {
        // a sketch being drawn in a 2D pane is left alone until clicking here
        let mine = self.plan.sketch.as_ref().is_some_and(|d| d.pane.is_none());
        if !mine && response.clicked() {
            self.plan.sketch = None;
        }
        let (enter, escape, shift) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
                i.modifiers.shift,
            )
        });
        let hover = response.hover_pos();
        // the plane a first click would start on
        let candidate = |p: egui::Pos2| -> Option<Workplane> {
            let (o, d) = ray(p);
            match pick::pick(&self.scene, o.as_vec3(), d.as_vec3()) {
                Some(h) => {
                    let item = self.scene.iter().find(|s| s.id == h.id)?;
                    let idx = &item.mesh.indices[h.triangle * 3..h.triangle * 3 + 3];
                    let v = |k: usize| glam::Vec3::from(item.mesh.positions[idx[k] as usize]);
                    let mut n = (v(1) - v(0))
                        .cross(v(2) - v(0))
                        .normalize_or_zero()
                        .as_dvec3();
                    if n.dot(d) > 0.0 {
                        n = -n;
                    }
                    let at = o + d * h.t as f64;
                    Some(Workplane::facing(at, n))
                }
                None => {
                    let at = Workplane::ground(0.0).intersect(o, d)?;
                    Some(Workplane {
                        origin: at,
                        ..Workplane::ground(0.0)
                    })
                }
            }
        };
        let on_plane = match (&self.plan.sketch, hover) {
            (Some(d), Some(p)) if mine => {
                let (o, dir) = ray(p);
                d.plane.intersect(o, dir).map(|m| {
                    // 10 px at that distance, for closing on the first corner
                    let dist = m.distance(cam.eye().as_dvec3());
                    let tol = dist * 2.0 * (cam.fov_y as f64 / 2.0).tan() * 10.0
                        / response.rect.height() as f64;
                    d.snap(d.plane.to_plane(m), tol, shift)
                })
            }
            _ => None,
        };
        let accent = Color32::from_rgb(230, 120, 30);
        match self.plan.sketch.as_ref().filter(|_| mine) {
            Some(d) => d.draw(painter, project, on_plane),
            None => {
                if let Some(plane) = hover.and_then(candidate) {
                    crate::sketch::draw_plane(painter, project, &plane, accent);
                }
            }
        }
        let finish = |app: &mut Self, closed: bool| {
            let Some(d) = app.plan.sketch.take() else {
                return;
            };
            match d.finish(&mut app.doc, closed) {
                Ok(Some(id)) => {
                    app.selection.only(id);
                    app.face = None;
                    app.status =
                        "Sketch added. Press E to extrude it into a mass, or G to move it.".into();
                }
                Ok(None) => {}
                Err(e) => app.status = format!("Couldn't do that: {e}"),
            }
        };
        if !mine && !response.clicked() {
            return;
        }
        if escape {
            self.plan.sketch = None;
            return;
        }
        if enter || (response.secondary_clicked() && self.plan.sketch.is_some()) {
            finish(self, false);
            return;
        }
        if !response.clicked() {
            return;
        }
        match &mut self.plan.sketch {
            None => {
                if let Some(plane) = hover.and_then(candidate) {
                    self.plan.sketch = Some(SketchDraft::new(plane, None));
                    self.status = "Workplane set. Click the corners; click the first one to close the sketch, Enter to leave it open.".into();
                }
            }
            Some(d) => {
                if let Some(p) = on_plane {
                    if let Some(Click::Closed) = d.click(p) {
                        finish(self, true);
                    }
                }
            }
        }
    }

    fn show_plan(&mut self, ui: &mut egui::Ui, rect: Rect, background: Color32) {
        let Some(view) = self.active_view else { return };
        let (before, revision) = (self.selection.clone(), self.doc.model().revision());
        let schedule = match self.view(view).map(|v| &v.kind) {
            Some(ViewKind::Schedule { category }) => Some(*category),
            _ => None,
        };
        let result = if let Some(category) = schedule {
            let csv = with_extension(
                &self.path,
                &format!("{}.csv", category.label().to_lowercase()),
            );
            let msg = crate::schedulepane::show(
                ui,
                rect,
                &mut self.doc,
                view,
                category,
                &mut self.selection,
                &csv,
            );
            if let Some(m) = msg {
                self.status = m;
            }
            None
        } else if self.is_sheet(view) {
            let views = self.placed_views(view);
            let info = self.sheet_info();
            sheetview::show(
                ui,
                rect,
                &mut self.doc,
                view,
                &views,
                &info,
                &mut self.sheet,
                &mut self.plan,
                &mut self.selection,
            )
        } else {
            let Some(kind) = self.view(view).map(planview::pane_kind) else {
                return;
            };
            let Some(drawing) = self.drawing(view) else {
                return;
            };
            let style = PaneStyle {
                background: Some(background),
                decorations: true,
                camera: None,
                scale: self.view(view).map_or(100.0, |v| v.scale),
            };
            planview::show(
                ui,
                rect,
                &mut self.doc,
                view,
                kind,
                &drawing,
                &mut self.plan,
                &mut self.selection,
                style,
            )
        };
        if let Some(err) = result {
            self.status = format!("Couldn't do that: {err}");
        } else if self.doc.model().revision() != revision {
            if let Some(label) = self.doc.undo_label() {
                self.status = format!("{label}. Ctrl+Z to undo.");
                if label.starts_with("Add Section") || label.starts_with("Add Elevation") {
                    self.status
                        .push_str(" Open it from the View list or the Views group.");
                }
            }
        }
        if self.selection != before {
            self.face = None;
        }
        if self.selection != before || self.doc.model().revision() != revision {
            // other views drew this frame before the edit; draw them again
            ui.ctx().request_repaint();
        }
        let name = self
            .doc
            .model()
            .get(view)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        let label = match (self.view(view), sheets::sheet_of(self.doc.model(), view)) {
            (Some(_), _) if schedule.is_some() => format!("{name}  ·  schedule"),
            (Some(v), _) => format!("{name}  ·  1:{:.0}", v.scale),
            (_, Some(s)) => format!(
                "Sheet {} {name}  ·  {}",
                s.number,
                s.size_name().unwrap_or("custom size")
            ),
            _ => name,
        };
        ui.painter().text(
            rect.left_top() + Vec2::new(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(13.0),
            Color32::from_gray(80),
        );
    }

    // ---- panels ---------------------------------------------------------

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New sample house").clicked() {
                    self.load_model(sample_house());
                    self.status = "Loaded the sample house.".into();
                }
                if ui.button("New empty project").clicked() {
                    let mut m = Document::new();
                    let l = Element::new(
                        "Level 0",
                        ElementKind::Level(creabim_core::Level {
                            elevation: 0.0,
                            height: 3.0,
                        }),
                    );
                    let _ = m.commit(Transaction::new("New project").insert(l));
                    self.load_model(m.model().clone());
                    self.status =
                        "New project with one level. Pick the Wall tool to start drawing.".into();
                }
                if ui.button("New sheet").clicked() {
                    self.new_sheet();
                }
                ui.menu_button("New schedule", |ui| {
                    for c in ScheduleCategory::ALL {
                        if ui.button(c.label()).clicked() {
                            self.new_schedule(c);
                        }
                    }
                });
                ui.separator();
                if ui
                    .button("Open")
                    .on_hover_text("Open the .creabim, .ifc or .ifcx file named in the File box. Other files (.blend, .FCStd) are linked into this project.")
                    .clicked()
                {
                    let path = self.path.clone();
                    self.open_path(&path);
                }
                if ui
                    .button("Save")
                    .on_hover_text("Save as a .creabim project file, with its undo history.")
                    .clicked()
                {
                    self.save();
                }
                ui.separator();
                if ui
                    .button("Import and link…")
                    .on_hover_text("Bring in the objects of the .blend or .FCStd file named in the File box. Saving it in Blender or FreeCAD updates them here.")
                    .clicked()
                {
                    let path = self.path.clone();
                    self.link_file(&path);
                }
                ui.checkbox(&mut self.live_reload, "Reload linked files when saved");
                let linked = crate::files::linked_files(self.doc.model());
                if !linked.is_empty() {
                    ui.menu_button("Linked files", |ui| {
                        for f in &linked {
                            if ui.button(format!("Reload {}", file_label(f))).clicked() {
                                self.reload(f);
                            }
                        }
                    });
                }
                ui.separator();
                if ui
                    .button("Export glTF (.glb) for Blender")
                    .on_hover_text("Every element as a mesh with its CreaBIM id, so edits saved in Blender come back to the same elements.")
                    .clicked()
                {
                    let out = with_extension(&self.path, "glb");
                    let r = std::fs::write(&out, creabim_exchange::gltf::export_glb(self.doc.model()));
                    self.report(r, format!("Exported {out}. In Blender: File > Import > glTF, or use the CreaBIM add-on."));
                }
                if ui
                    .button("Export FreeCAD (.FCStd)")
                    .on_hover_text("Every element as a solid with its BIM type and CreaBIM id; open it in FreeCAD, edit, save, and link it back.")
                    .clicked()
                {
                    let out = with_extension(&self.path, "FCStd");
                    let r = creabim_exchange::freecad::export_fcstd(self.doc.model(), &stem(&self.path))
                        .map_err(|e| e.to_string())
                        .and_then(|b| std::fs::write(&out, b).map_err(|e| e.to_string()));
                    self.report(r, format!("Exported {out}"));
                }
                if ui
                    .button("Export IFC5 (.ifcx, alpha)")
                    .on_hover_text("The new JSON-based IFC. Its schema is still an alpha, so other tools may not read it yet.")
                    .clicked()
                {
                    let out = with_extension(&self.path, "ifcx");
                    let r = std::fs::write(&out, creabim_ifc::export_ifcx(self.doc.model()));
                    self.report(r, format!("Exported {out}"));
                }
                if ui.button("Export IFC4").clicked() {
                    let out = with_extension(&self.path, "ifc");
                    let opts = creabim_ifc::ExportOptions {
                        project_name: stem(&self.path),
                        ..Default::default()
                    };
                    let r = std::fs::write(&out, creabim_ifc::export_ifc4(self.doc.model(), &opts));
                    self.report(r, format!("Exported {out}"));
                }
                if ui.button("Export sheets as PDF").clicked() {
                    let out = with_extension(&self.path, "pdf");
                    if sheets::sheets(self.doc.model()).is_empty() {
                        self.status = "There are no sheets to export yet.".into();
                    } else {
                        let pdf = sheets::sheets_pdf(self.doc.model(), &self.sheet_info());
                        let r = std::fs::write(&out, pdf);
                        let n = sheets::sheets(self.doc.model()).len();
                        self.report(r, format!("Exported {n} sheets to {out}"));
                    }
                }
                if ui.button("Export view or sheet as SVG").clicked() {
                    if let Some(sheet) = self.active_view.filter(|id| self.is_sheet(*id)) {
                        let number = sheets::sheet_of(self.doc.model(), sheet)
                            .map(|s| s.number.clone())
                            .unwrap_or_default();
                        let out = with_extension(&self.path, &format!("{number}.svg"));
                        if let Some(svg) =
                            sheets::sheet_svg(self.doc.model(), sheet, &self.sheet_info())
                        {
                            let r = std::fs::write(&out, svg);
                            self.report(r, format!("Exported {out}"));
                        }
                    } else if let Some(view) = self.active_view {
                        let name = self
                            .doc
                            .model()
                            .get(view)
                            .map(|e| e.name.replace(' ', "-"))
                            .unwrap_or_default();
                        let scale = self.view(view).map_or(100.0, |v| v.scale);
                        let out = with_extension(&self.path, &format!("{name}.svg"));
                        if let Some(d) = self.drawing(view) {
                            let r = std::fs::write(&out, d.to_svg(scale));
                            self.report(r, format!("Exported {out} at 1:{scale:.0}"));
                        }
                    }
                }
            });
            ui.menu_button("Edit", |ui| {
                let undo = self
                    .doc
                    .undo_label()
                    .map(|l| format!("Undo {l}"))
                    .unwrap_or("Undo".into());
                if ui
                    .add_enabled(self.doc.can_undo(), egui::Button::new(undo))
                    .clicked()
                {
                    self.doc.undo();
                    self.drop_stale_selection();
                }
                let redo = self
                    .doc
                    .redo_label()
                    .map(|l| format!("Redo {l}"))
                    .unwrap_or("Redo".into());
                if ui
                    .add_enabled(self.doc.can_redo(), egui::Button::new(redo))
                    .clicked()
                {
                    self.doc.redo();
                    self.drop_stale_selection();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.selection.is_some(),
                        egui::Button::new("Delete selected"),
                    )
                    .clicked()
                {
                    self.delete_selected();
                }
            });
            ui.menu_button("View", |ui| {
                ui.label("2D view");
                ui.radio_value(
                    &mut self.placement,
                    PlanPlacement::Docked,
                    "Docked beside 3D",
                );
                ui.radio_value(
                    &mut self.placement,
                    PlanPlacement::Window,
                    "In its own window",
                );
                ui.radio_value(
                    &mut self.placement,
                    PlanPlacement::Overlay,
                    "Overlaid in the 3D workspace",
                );
                ui.radio_value(&mut self.placement, PlanPlacement::Hidden, "Hidden");
                ui.separator();
                ui.label("Docked layout (drag the divider to resize)");
                ui.radio_value(&mut self.layout.stacked, false, "Side by side");
                ui.radio_value(&mut self.layout.stacked, true, "One above the other");
                ui.checkbox(&mut self.layout.swapped, "2D view first");
                ui.separator();
                ui.checkbox(&mut self.plan_in_3d, "Show the 2D view in 3D space");
                if ui.button("Frame all").clicked() {
                    self.frame_all();
                }
            });
            ui.separator();
            ui.label("File:");
            ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(220.0));
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for t in Tool::ALL {
                if ui
                    .selectable_label(self.plan.tool == t, t.label())
                    .on_hover_text(t.hint())
                    .clicked()
                {
                    self.plan.tool = t;
                    self.plan.cancel();
                }
            }
            ui.separator();
            ui.label("View:");
            let current = self
                .active_view
                .and_then(|id| self.doc.model().get(id))
                .map(|e| e.name.clone())
                .unwrap_or_default();
            let views = self.views();
            let sheet_list = self.sheet_list();
            let current = match self
                .active_view
                .and_then(|id| sheets::sheet_of(self.doc.model(), id))
            {
                Some(s) => format!("{} {current}", s.number),
                None => current,
            };
            egui::ComboBox::from_id_salt("view")
                .selected_text(current)
                .width(190.0)
                .show_ui(ui, |ui| {
                    for (id, name, _) in views {
                        ui.selectable_value(&mut self.active_view, Some(id), name);
                    }
                    if !sheet_list.is_empty() {
                        ui.separator();
                        ui.label(egui::RichText::new("Sheets").weak());
                    }
                    for (id, name) in sheet_list {
                        ui.selectable_value(&mut self.active_view, Some(id), name);
                    }
                });
            if let Some(id) = self.active_view.filter(|id| self.is_sheet(*id)) {
                self.place_view_combo(ui, id);
            }
            if let Some(id) = self.active_view {
                if let Some(mut v) = self.view(id).cloned() {
                    let shadable = matches!(
                        v.kind,
                        ViewKind::Section { .. } | ViewKind::Axonometric { .. }
                    );
                    let (text, value, unit) = match &mut v.kind {
                        ViewKind::Plan { cut_height, .. } => ("Cut height:", cut_height, " m"),
                        ViewKind::Section { depth, .. } => ("Depth:", depth, " m"),
                        ViewKind::Axonometric { altitude, .. } => ("Altitude:", altitude, "°"),
                        ViewKind::Schedule { .. } => return,
                    };
                    ui.label(text);
                    let changed = ui
                        .add(
                            egui::DragValue::new(value)
                                .speed(0.05)
                                .range(if unit == "°" {
                                    -89.0..=89.0
                                } else {
                                    0.1..=200.0
                                })
                                .suffix(unit),
                        )
                        .changed();
                    let mut shaded = v.style == ViewStyle::Shaded;
                    let restyled = shadable && ui.checkbox(&mut shaded, "Shaded").changed();
                    if restyled {
                        v.style = if shaded {
                            ViewStyle::Shaded
                        } else {
                            ViewStyle::HiddenLine
                        };
                    }
                    if changed || restyled {
                        let r = self.doc.edit_merged(id, "Change view", |e| {
                            e.kind = ElementKind::View(v.clone())
                        });
                        self.report(r, "View changed.");
                    }
                    if ui
                        .button("Tag doors and windows")
                        .on_hover_text("Tag every door and window in this view that has no tag yet")
                        .clicked()
                    {
                        let r = planview::tag_openings(&mut self.doc, id);
                        self.status = match r {
                            Ok(0) => "Every door and window here is tagged already.".into(),
                            Ok(n) => {
                                format!("Tagged {n} doors and windows. Drag a tag to move it.")
                            }
                            Err(e) => format!("Couldn't do that: {e}"),
                        };
                    }
                }
            }
            ui.label("Wall thickness:");
            ui.add(
                egui::DragValue::new(&mut self.plan.wall_thickness)
                    .speed(0.01)
                    .range(0.05..=1.0)
                    .suffix(" m"),
            );
        });
    }

    /// "Place view" picker shown while a sheet is open.
    fn place_view_combo(&mut self, ui: &mut egui::Ui, sheet: ElementId) {
        let placed: Vec<ElementId> = self
            .doc
            .model()
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Viewport(vp) if vp.sheet == sheet => Some(vp.view),
                _ => None,
            })
            .collect();
        let mut chosen = None;
        egui::ComboBox::from_id_salt("place_view")
            .selected_text("Place view…")
            .width(150.0)
            .show_ui(ui, |ui| {
                for (id, name, _) in self.views() {
                    let label = if placed.contains(&id) {
                        format!("{name} (placed)")
                    } else {
                        name
                    };
                    if ui.selectable_label(false, label).clicked() {
                        chosen = Some(id);
                    }
                }
            });
        let Some(view) = chosen else { return };
        let at = sheets::free_spot(self.doc.model(), sheet, view);
        let vp = sheets::new_viewport(self.doc.model(), sheet, view, at);
        let e = Element::new("Viewport", ElementKind::Viewport(vp));
        let id = e.id;
        let r = self
            .doc
            .commit(Transaction::new("Place view on sheet").insert(e));
        self.selection.only(id);
        self.report(
            r,
            "Placed the view. Drag it to where it belongs on the sheet.",
        );
    }

    fn new_sheet(&mut self) {
        let used: Vec<String> = sheets::sheets(self.doc.model())
            .into_iter()
            .map(|(_, s)| s.number.clone())
            .collect();
        let number = (301..)
            .map(|n| format!("A{n}"))
            .find(|n| !used.contains(n))
            .expect("unbounded");
        let e = Element::new("New Sheet", ElementKind::Sheet(Sheet::a(number, "A3")));
        let id = e.id;
        let r = self.doc.commit(Transaction::new("New sheet").insert(e));
        self.active_view = Some(id);
        self.selection.only(id);
        self.report(
            r,
            "New A3 sheet. Use Place view… in the toolbar to put views on it.",
        );
    }

    fn new_schedule(&mut self, category: ScheduleCategory) {
        let label = category.label();
        let e = Element::new(
            format!("{} Schedule", &label[..label.len() - 1]),
            ElementKind::View(View::schedule(category)),
        );
        let id = e.id;
        let r = self
            .doc
            .commit(Transaction::new(format!("New {} schedule", label.to_lowercase())).insert(e));
        self.active_view = Some(id);
        self.report(
            r,
            "New schedule. Edit cells to change the model, or place it on a sheet with Place view….",
        );
    }

    fn materials(&self) -> Vec<(ElementId, String)> {
        self.doc
            .model()
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Material(_)))
            .map(|e| (e.id, e.name.clone()))
            .collect()
    }

    /// A material picker; "New material" adds one and picks it.
    fn material_combo(&mut self, ui: &mut egui::Ui, current: &mut Option<ElementId>) -> bool {
        let materials = self.materials();
        let name = current
            .and_then(|id| materials.iter().find(|m| m.0 == id))
            .map_or("By type", |m| m.1.as_str())
            .to_string();
        let mut changed = false;
        let mut create = false;
        egui::ComboBox::from_label("Material")
            .selected_text(name)
            .show_ui(ui, |ui| {
                changed |= ui.selectable_value(current, None, "By type").changed();
                for (id, n) in &materials {
                    changed |= ui.selectable_value(current, Some(*id), n).changed();
                }
                ui.separator();
                create = ui.button("New material…").clicked();
            });
        if create {
            let e = Element::new(
                format!("Material {}", materials.len() + 1),
                ElementKind::Material(Material {
                    hatch: Hatch::Diagonal,
                    color: [0.8, 0.8, 0.8],
                }),
            );
            *current = Some(e.id);
            let r = self.doc.commit(Transaction::new("New material").insert(e));
            self.report(
                r,
                "New material. Select it in the Model list to set its hatch and colour.",
            );
            changed = true;
        }
        changed
    }

    fn outliner(&mut self, ui: &mut egui::Ui) {
        ui.heading("Model");
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut groups: Vec<(String, Vec<(ElementId, String)>)> = Vec::new();
            for e in self.doc.model().iter() {
                let class = e.ifc_class().to_string();
                match groups.iter_mut().find(|(c, _)| *c == class) {
                    Some((_, v)) => v.push((e.id, e.name.clone())),
                    None => groups.push((class, vec![(e.id, e.name.clone())])),
                }
            }
            for (class, items) in groups {
                let opens = class == "View" || class == "Sheet";
                let title = match class.as_str() {
                    "View" => format!("Views ({}), click to open", items.len()),
                    "Sheet" => format!("Sheets ({}), click to open", items.len()),
                    _ => format!("{class} ({})", items.len()),
                };
                let id = ui.make_persistent_id(("group", &class));
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    class == "IfcBuildingStorey" || opens,
                )
                .show_header(ui, |ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                    Icon::of_class(&class).paint(ui.painter(), r);
                    ui.label(title);
                })
                .body(|ui| {
                    for (id, name) in items {
                        let icon = self.doc.model().get(id).map_or(Icon::Mass, Icon::of);
                        if icons::row(ui, icon, self.selection.contains(id), &name).clicked() {
                            self.selection.only(id);
                            self.face = None;
                            if opens {
                                self.active_view = Some(id);
                            }
                        }
                    }
                });
            }
        });
    }

    /// Booleans on a wall, slab or mass: the cuts it has, and a menu to cut
    /// it with another element (which then turns into a see-through ghost).
    fn cuts_panel(&mut self, ui: &mut egui::Ui, id: ElementId) {
        let cuttable = |k: &ElementKind| {
            matches!(
                k,
                ElementKind::Wall(_) | ElementKind::Slab(_) | ElementKind::FreeMesh(_)
            )
        };
        let model = self.doc.model();
        if !model.get(id).is_some_and(|e| cuttable(&e.kind)) {
            return;
        }
        let existing: Vec<(ElementId, Cut, String)> = model
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Cut(c) if c.target == id => Some((
                    e.id,
                    *c,
                    model
                        .get(c.cutter)
                        .map_or(String::new(), |x| x.name.clone()),
                )),
                _ => None,
            })
            .collect();
        // masses first: they are what you usually cut with
        let mut candidates: Vec<(bool, ElementId, String)> = model
            .iter()
            .filter(|e| e.id != id && cuttable(&e.kind))
            .filter(|e| !existing.iter().any(|(_, c, _)| c.cutter == e.id))
            .map(|e| {
                (
                    !matches!(e.kind, ElementKind::FreeMesh(_)),
                    e.id,
                    e.name.clone(),
                )
            })
            .collect();
        candidates.sort_by_key(|c| c.0);
        ui.separator();
        ui.label("Booleans");
        let mut remove = None;
        let mut select = None;
        for (cid, c, name) in &existing {
            ui.horizontal(|ui| {
                ui.label(format!("{} {name}", c.op.label()));
                if ui.small_button("Select").clicked() {
                    select = Some(*cid);
                }
                if ui.small_button("Remove").clicked() {
                    remove = Some(*cid);
                }
            });
        }
        let mut add = None;
        egui::ComboBox::from_id_salt("cut_with")
            .selected_text("Subtract…")
            .show_ui(ui, |ui| {
                for (_, cid, name) in &candidates {
                    if ui.selectable_label(false, name).clicked() {
                        add = Some(*cid);
                    }
                }
            });
        ui.label(
            egui::RichText::new(
                "Pick an element to cut away. It turns into a see-through ghost you can still select and move.",
            )
            .weak(),
        );
        if let Some(c) = remove {
            let r = self
                .doc
                .commit(Transaction::new("Remove boolean").remove_cascade(self.doc.model(), c));
            self.report(r, "Boolean removed.");
        }
        if let Some(cutter) = add {
            let e = Element::new(
                "Cut",
                ElementKind::Cut(Cut {
                    target: id,
                    cutter,
                    op: BoolOp::Difference,
                }),
            );
            let r = self.doc.commit(Transaction::new("Subtract").insert(e));
            self.report(r, "Subtracted. Move the ghost to change the cut.");
        }
        if let Some(c) = select {
            self.selection.only(c);
        }
    }

    fn properties(&mut self, ui: &mut egui::Ui) {
        ui.heading("Properties");
        let Some(id) = self.selection.active else {
            ui.label(
                "Nothing selected. Click an element in the plan, the 3D view or the model list.",
            );
            return;
        };
        let Some(e) = self.doc.model().get(id).cloned() else {
            return;
        };
        ui.label(egui::RichText::new(e.ifc_class()).weak());
        let mut name = e.name.clone();
        if ui.text_edit_singleline(&mut name).changed() {
            let r = self
                .doc
                .edit_merged(id, "Rename", |e| e.name = name.clone());
            self.report(r, "Renamed.");
        }
        ui.add_space(6.0);

        let mut edited = e.clone();
        let mut label = "";
        let mut extrude_sketch = false;
        let num = |ui: &mut egui::Ui, text: &str, v: &mut f64, lo: f64, hi: f64| -> bool {
            ui.horizontal(|ui| {
                ui.label(text);
                ui.add(
                    egui::DragValue::new(v)
                        .speed(0.01)
                        .range(lo..=hi)
                        .suffix(" m"),
                )
                .changed()
            })
            .inner
        };
        let mut redraw = false;
        match &mut edited.kind {
            ElementKind::Level(l) => {
                if num(ui, "Elevation", &mut l.elevation, -100.0, 1000.0) {
                    label = "Level elevation"
                }
                if num(ui, "Height", &mut l.height, 0.5, 50.0) {
                    label = "Level height"
                }
                ui.label(egui::RichText::new("Moving a level moves everything on it.").weak());
            }
            ElementKind::Wall(w) => {
                if num(ui, "Thickness", &mut w.thickness, 0.02, 2.0) {
                    label = "Wall thickness"
                }
                if num(ui, "Height", &mut w.height, 0.1, 50.0) {
                    label = "Wall height"
                }
                if num(ui, "Base offset", &mut w.base_offset, -10.0, 50.0) {
                    label = "Wall base offset"
                }
                if self.material_combo(ui, &mut w.material) {
                    label = "Wall material"
                }
                ui.label(format!("Length {:.2} m", w.length()));
                if num(ui, "Start X", &mut w.start.x, -1e4, 1e4)
                    | num(ui, "Start Y", &mut w.start.y, -1e4, 1e4)
                {
                    label = "Move wall start"
                }
                if num(ui, "End X", &mut w.end.x, -1e4, 1e4)
                    | num(ui, "End Y", &mut w.end.y, -1e4, 1e4)
                {
                    label = "Move wall end"
                }
                if ui
                    .button("Redraw path")
                    .on_hover_text("Click a new path for this wall, in a plan or the 3D view. More corners make more walls like it.")
                    .clicked()
                {
                    redraw = true;
                }
            }
            ElementKind::Slab(s) => {
                if num(ui, "Thickness", &mut s.thickness, 0.02, 3.0) {
                    label = "Slab thickness"
                }
                if self.material_combo(ui, &mut s.material) {
                    label = "Slab material"
                }
                ui.label(format!("{} boundary points", s.boundary.len()));
                if ui
                    .button("Redraw outline")
                    .on_hover_text("Click a new outline for this slab, in a plan or the 3D view.")
                    .clicked()
                {
                    redraw = true;
                }
            }
            ElementKind::Opening(o) => {
                if num(ui, "Width", &mut o.width, 0.1, 20.0) {
                    label = "Opening width"
                }
                if num(ui, "Height", &mut o.height, 0.1, 20.0) {
                    label = "Opening height"
                }
                if num(ui, "Sill", &mut o.sill, 0.0, 20.0) {
                    label = "Opening sill"
                }
                if num(ui, "Offset along wall", &mut o.offset, 0.0, 1e4) {
                    label = "Move opening"
                }
            }
            ElementKind::View(v) if matches!(v.kind, ViewKind::Schedule { .. }) => {
                if let ViewKind::Schedule { category } = &mut v.kind {
                    egui::ComboBox::from_label("Lists")
                        .selected_text(category.label())
                        .show_ui(ui, |ui| {
                            for c in ScheduleCategory::ALL {
                                if ui.selectable_value(category, c, c.label()).changed() {
                                    label = "Schedule category"
                                }
                            }
                        });
                }
                if self.active_view != Some(id) && ui.button("Open this schedule").clicked() {
                    self.active_view = Some(id);
                }
            }
            ElementKind::View(v) => {
                ui.horizontal(|ui| {
                    ui.label("Scale 1:");
                    if ui
                        .add(
                            egui::DragValue::new(&mut v.scale)
                                .speed(1.0)
                                .range(1.0..=5000.0),
                        )
                        .changed()
                    {
                        label = "View scale"
                    }
                });
                match &mut v.kind {
                    ViewKind::Plan { cut_height, .. } => {
                        if num(ui, "Cut height", cut_height, 0.1, 50.0) {
                            label = "View cut height"
                        }
                    }
                    ViewKind::Section {
                        start,
                        end,
                        depth,
                        cut,
                    } => {
                        if ui
                            .checkbox(cut, "Cut through the model (section)")
                            .changed()
                        {
                            label = "Section or elevation"
                        }
                        if num(ui, "Depth", depth, 0.1, 500.0) {
                            label = "View depth"
                        }
                        if num(ui, "Start X", &mut start.x, -1e4, 1e4)
                            | num(ui, "Start Y", &mut start.y, -1e4, 1e4)
                            | num(ui, "End X", &mut end.x, -1e4, 1e4)
                            | num(ui, "End Y", &mut end.y, -1e4, 1e4)
                        {
                            label = "Move section line"
                        }
                    }
                    ViewKind::Axonometric { azimuth, altitude } => {
                        let deg = |ui: &mut egui::Ui, text: &str, v: &mut f64, lo: f64, hi: f64| {
                            ui.horizontal(|ui| {
                                ui.label(text);
                                ui.add(
                                    egui::DragValue::new(v)
                                        .speed(0.5)
                                        .range(lo..=hi)
                                        .suffix("°"),
                                )
                                .changed()
                            })
                            .inner
                        };
                        if deg(ui, "Azimuth", azimuth, 0.0, 360.0)
                            | deg(ui, "Altitude", altitude, -89.0, 89.0)
                        {
                            label = "Orbit 3D view"
                        }
                        ui.label(egui::RichText::new("Or drag in the view to orbit it.").weak());
                    }
                    ViewKind::Schedule { .. } => {}
                }
                if matches!(
                    v.kind,
                    ViewKind::Section { .. } | ViewKind::Axonometric { .. }
                ) {
                    let mut shaded = v.style == ViewStyle::Shaded;
                    if ui.checkbox(&mut shaded, "Shaded surfaces").changed() {
                        v.style = if shaded {
                            ViewStyle::Shaded
                        } else {
                            ViewStyle::HiddenLine
                        };
                        label = "View style"
                    }
                }
                if self.active_view != Some(id) && ui.button("Open this view").clicked() {
                    self.active_view = Some(id);
                }
            }
            ElementKind::Sheet(sh) => {
                ui.horizontal(|ui| {
                    ui.label("Number");
                    if ui.text_edit_singleline(&mut sh.number).changed() {
                        label = "Sheet number"
                    }
                });
                let current = sh.size_name().unwrap_or("Custom");
                egui::ComboBox::from_label("Paper")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (name, w, h) in Sheet::SIZES {
                            if ui.selectable_label(current == *name, *name).clicked() {
                                (sh.width, sh.height) = (*w, *h);
                                label = "Sheet size"
                            }
                        }
                    });
                if self.active_view != Some(id) && ui.button("Open this sheet").clicked() {
                    self.active_view = Some(id);
                }
            }
            ElementKind::Viewport(vp) => {
                let view_name = self
                    .doc
                    .model()
                    .get(vp.view)
                    .map(|e| e.name.clone())
                    .unwrap_or_default();
                ui.label(format!("Shows {view_name}"));
                let mm = |ui: &mut egui::Ui, text: &str, v: &mut f64| {
                    ui.horizontal(|ui| {
                        ui.label(text);
                        ui.add(egui::DragValue::new(v).speed(1.0).suffix(" mm"))
                            .changed()
                    })
                    .inner
                };
                if mm(ui, "Centre X", &mut vp.center.x) | mm(ui, "Centre Y", &mut vp.center.y) {
                    label = "Move view on sheet"
                }
                let mut cropped = vp.crop.is_some();
                if ui.checkbox(&mut cropped, "Crop the view").changed() {
                    vp.crop = if cropped {
                        let view = vp.view;
                        self.drawing(view)
                            .and_then(|d| d.bounds())
                            .map(|(lo, hi)| [lo, hi])
                    } else {
                        None
                    };
                    label = "Crop view"
                }
                if let Some([lo, hi]) = &mut vp.crop {
                    if num(ui, "Crop left", &mut lo.x, -1e4, hi.x - 0.1)
                        | num(ui, "Crop right", &mut hi.x, lo.x + 0.1, 1e4)
                        | num(ui, "Crop bottom", &mut lo.y, -1e4, hi.y - 0.1)
                        | num(ui, "Crop top", &mut hi.y, lo.y + 0.1, 1e4)
                    {
                        label = "Crop view"
                    }
                }
                ui.label(
                    egui::RichText::new(
                        "Drag the view on the sheet to move it; double-click it to edit the model through it.",
                    )
                    .weak(),
                );
                if ui.button("Open the view itself").clicked() {
                    self.active_view = Some(vp.view);
                }
            }
            ElementKind::Material(m) => {
                egui::ComboBox::from_label("Cut hatch")
                    .selected_text(m.hatch.label())
                    .show_ui(ui, |ui| {
                        for h in Hatch::ALL {
                            if ui.selectable_value(&mut m.hatch, h, h.label()).changed() {
                                label = "Material hatch"
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    ui.label("Colour");
                    if ui.color_edit_button_rgb(&mut m.color).changed() {
                        label = "Material colour"
                    }
                });
                let users = self
                    .doc
                    .model()
                    .iter()
                    .filter(|u| {
                        creabim_core::material_of(self.doc.model(), u)
                            .is_some_and(|(x, _)| x.id == id)
                    })
                    .count();
                ui.label(egui::RichText::new(format!("Used by {users} elements.")).weak());
            }
            ElementKind::Dimension(d) => {
                let measured = self
                    .drawing(d.view)
                    .and_then(|dr| {
                        dr.items.into_iter().find_map(|i| match i {
                            PlanItem::Dimension { id: i, text, .. } if i == id => Some(text),
                            _ => None,
                        })
                    })
                    .unwrap_or_else(|| "-".into());
                ui.label(format!("Measures {measured} mm"));
                let attached = [d.a.host, d.b.host].iter().flatten().count();
                ui.label(
                    egui::RichText::new(match attached {
                        0 => "Not attached: it stays where it was drawn.".to_string(),
                        n => format!("Attached at {n} end(s): it follows what it measures."),
                    })
                    .weak(),
                );
                if num(ui, "Line offset", &mut d.offset, -100.0, 100.0) {
                    label = "Move dimension line"
                }
            }
            ElementKind::Tag(t) => {
                let host = self
                    .doc
                    .model()
                    .get(t.host)
                    .map(|e| e.name.clone())
                    .unwrap_or_default();
                ui.label(format!("Tags {host}"));
                if num(ui, "Offset X", &mut t.offset.x, -100.0, 100.0)
                    | num(ui, "Offset Y", &mut t.offset.y, -100.0, 100.0)
                {
                    label = "Move tag"
                }
                if ui.button("Select the tagged element").clicked() {
                    self.selection.only(t.host);
                }
            }
            ElementKind::Grid(g) => {
                ui.horizontal(|ui| {
                    ui.label("Label");
                    if ui.text_edit_singleline(&mut g.label).changed() {
                        label = "Grid label"
                    }
                });
            }
            ElementKind::Sketch(sk) => {
                ui.label(format!(
                    "{} corners, {}",
                    sk.points.len(),
                    if sk.closed { "closed" } else { "open" }
                ));
                if sk.points.len() >= 3 && ui.checkbox(&mut sk.closed, "Closed").changed() {
                    label = "Close sketch"
                }
                if num(ui, "Plane X", &mut sk.plane.origin.x, -1e4, 1e4)
                    | num(ui, "Plane Y", &mut sk.plane.origin.y, -1e4, 1e4)
                    | num(ui, "Plane Z", &mut sk.plane.origin.z, -1e3, 1e3)
                {
                    label = "Move sketch"
                }
                let n = sk.plane.normal();
                ui.label(
                    egui::RichText::new(format!(
                        "Its plane faces ({:.2}, {:.2}, {:.2}).",
                        n.x, n.y, n.z
                    ))
                    .weak(),
                );
                ui.horizontal(|ui| {
                    ui.label("Extrude by");
                    ui.add(
                        egui::DragValue::new(&mut self.extrude_distance)
                            .speed(0.05)
                            .suffix(" m"),
                    );
                });
                if ui
                    .button(if sk.closed {
                        "Extrude into a mass (E)"
                    } else {
                        "Extrude into a surface (E)"
                    })
                    .clicked()
                {
                    extrude_sketch = true;
                }
            }
            ElementKind::FreeMesh(m) => {
                egui::ComboBox::from_label("IFC class")
                    .selected_text(m.ifc_class.clone())
                    .show_ui(ui, |ui| {
                        for c in FREE_MESH_CLASSES {
                            if ui
                                .selectable_value(&mut m.ifc_class, c.to_string(), *c)
                                .changed()
                            {
                                label = "Classify mesh"
                            }
                        }
                    });
                if self.material_combo(ui, &mut m.material) {
                    label = "Mesh material"
                }
                if num(ui, "X", &mut m.origin.x, -1e4, 1e4)
                    | num(ui, "Y", &mut m.origin.y, -1e4, 1e4)
                    | num(ui, "Z", &mut m.origin.z, -1e3, 1e3)
                {
                    label = "Move mesh"
                }
                ui.horizontal(|ui| {
                    ui.label("Rotation");
                    if ui
                        .add(egui::DragValue::new(&mut m.rotation).speed(1.0).suffix("°"))
                        .changed()
                    {
                        label = "Rotate mesh"
                    }
                    ui.label("Scale");
                    if ui
                        .add(
                            egui::DragValue::new(&mut m.scale)
                                .speed(0.01)
                                .range(0.01..=100.0),
                        )
                        .changed()
                    {
                        label = "Scale mesh"
                    }
                });
                ui.label(
                    egui::RichText::new(
                        "With the pointer over a view: G moves, R rotates, S scales, Shift+D duplicates.",
                    )
                    .weak(),
                );

                ui.separator();
                ui.label(egui::RichText::new("Modifiers").strong());
                if m.modifiers.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "None yet. They change the shape without touching the mesh.",
                        )
                        .weak(),
                    );
                }
                let count = m.modifiers.len();
                let mut action: Option<(usize, i32)> = None; // (index, -1 up, 1 down, 0 remove, 2 apply)
                for (i, md) in m.modifiers.iter_mut().enumerate() {
                    ui.push_id(("modifier", i), |ui| {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui
                                    .checkbox(&mut md.enabled, md.kind.label())
                                    .on_hover_text("Switch it off to see the mesh without it")
                                    .changed()
                                {
                                    label = "Switch modifier"
                                }
                                if ui.small_button("Remove").clicked() {
                                    action = Some((i, 0));
                                }
                                if i + 1 < count && ui.small_button("Down").clicked() {
                                    action = Some((i, 1));
                                }
                                if i > 0 && ui.small_button("Up").clicked() {
                                    action = Some((i, -1));
                                }
                                if i == 0
                                    && ui
                                        .small_button("Apply")
                                        .on_hover_text(
                                            "Make it part of the mesh, so you can edit the result",
                                        )
                                        .clicked()
                                {
                                    action = Some((i, 2));
                                }
                            });
                            match &mut md.kind {
                                ModifierKind::Array { count, offset } => {
                                    ui.horizontal(|ui| {
                                        ui.label("Count");
                                        if ui
                                            .add(
                                                egui::DragValue::new(count)
                                                    .speed(0.1)
                                                    .range(1..=200),
                                            )
                                            .changed()
                                        {
                                            label = "Array count"
                                        }
                                    });
                                    if num(ui, "Offset X", &mut offset.x, -1e3, 1e3)
                                        | num(ui, "Offset Y", &mut offset.y, -1e3, 1e3)
                                        | num(ui, "Offset Z", &mut offset.z, -1e3, 1e3)
                                    {
                                        label = "Array offset"
                                    }
                                }
                                ModifierKind::Mirror { axis, plane } => {
                                    ui.horizontal(|ui| {
                                        ui.label("Across");
                                        for a in Axis::ALL {
                                            if ui.radio_value(axis, a, a.label()).changed() {
                                                label = "Mirror axis"
                                            }
                                        }
                                    });
                                    if num(ui, "Mirror plane at", plane, -1e3, 1e3) {
                                        label = "Mirror plane"
                                    }
                                }
                                ModifierKind::Solidify { thickness } => {
                                    if num(ui, "Thickness", thickness, -10.0, 10.0) {
                                        label = "Solidify thickness"
                                    }
                                }
                                ModifierKind::Subdivision { levels } => {
                                    ui.horizontal(|ui| {
                                        ui.label("Levels");
                                        if ui
                                            .add(
                                                egui::DragValue::new(levels)
                                                    .speed(0.05)
                                                    .range(0..=4),
                                            )
                                            .changed()
                                        {
                                            label = "Subdivision levels"
                                        }
                                    });
                                }
                            }
                        });
                    });
                }
                match action {
                    Some((i, 0)) => {
                        m.modifiers.remove(i);
                        label = "Remove modifier";
                    }
                    Some((i, 1)) => {
                        m.modifiers.swap(i, i + 1);
                        label = "Reorder modifiers";
                    }
                    Some((i, -1)) => {
                        m.modifiers.swap(i, i - 1);
                        label = "Reorder modifiers";
                    }
                    Some((_, _)) => {
                        let first = m.modifiers.remove(0);
                        m.mesh = creabim_core::modifiers::evaluate(&m.mesh, &[first]).mesh;
                        self.face = None;
                        label = "Apply modifier";
                    }
                    None => {}
                }
                egui::ComboBox::from_id_salt("add_modifier")
                    .selected_text("Add modifier…")
                    .show_ui(ui, |ui| {
                        for k in ModifierKind::presets() {
                            if ui.selectable_label(false, k.label()).clicked() {
                                m.modifiers.push(Modifier::new(k));
                                label = "Add modifier";
                            }
                        }
                    });
                ui.label(format!(
                    "{} vertices, {} faces",
                    m.mesh.vertices.len(),
                    m.mesh.faces.len()
                ));
                ui.separator();
                ui.label(egui::RichText::new("Mesh edit").strong());
                if !m.modifiers.is_empty() {
                    ui.label(
                        egui::RichText::new("Edits change the mesh under the modifiers.").weak(),
                    );
                }
                match self.face {
                    Some(f) => ui.label(format!("Face {f} selected (click a face in 3D)")),
                    None => ui.label("Click a face in the 3D view to edit it."),
                };
                ui.horizontal(|ui| {
                    ui.label("Distance");
                    ui.add(
                        egui::DragValue::new(&mut self.extrude_distance)
                            .speed(0.05)
                            .suffix(" m"),
                    );
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.face.is_some(), egui::Button::new("Extrude (E)"))
                        .clicked()
                    {
                        if let Some(f) = self.face {
                            m.mesh.extrude_face(f, self.extrude_distance);
                            label = "Extrude face";
                        }
                    }
                    if ui
                        .add_enabled(self.face.is_some(), egui::Button::new("Taper"))
                        .on_hover_text("Scale the face to 80%")
                        .clicked()
                    {
                        if let Some(f) = self.face {
                            m.mesh.scale_face(f, 0.8);
                            label = "Taper face";
                        }
                    }
                    if ui
                        .add_enabled(self.face.is_some(), egui::Button::new("Move"))
                        .on_hover_text("Move the face along its normal by the distance")
                        .clicked()
                    {
                        if let Some(f) = self.face {
                            let n = m.mesh.face_normal(f) * self.extrude_distance;
                            let verts = m.mesh.faces[f].clone();
                            m.mesh.translate_vertices(&verts, n);
                            label = "Move face";
                        }
                    }
                });
                if ui.button("Subdivide (Catmull-Clark)").clicked() {
                    m.mesh = m.mesh.subdivide();
                    self.face = None;
                    label = "Subdivide mesh";
                }
            }
            ElementKind::Cut(c) => {
                let name = |id| {
                    self.doc
                        .model()
                        .get(id)
                        .map_or("(missing)".to_string(), |e| e.name.clone())
                };
                ui.label(format!(
                    "{} {} with {}",
                    c.op.label(),
                    name(c.target),
                    name(c.cutter)
                ));
                for op in BoolOp::ALL {
                    if ui.radio_value(&mut c.op, op, op.label()).changed() {
                        label = "Change boolean";
                    }
                }
            }
            ElementKind::Guide(g) => {
                ui.label(
                    egui::RichText::new(
                        "A construction line to snap to. It is never printed or exported.",
                    )
                    .weak(),
                );
                if num(ui, "From X", &mut g.a.x, -1e4, 1e4)
                    | num(ui, "From Y", &mut g.a.y, -1e4, 1e4)
                    | num(ui, "To X", &mut g.b.x, -1e4, 1e4)
                    | num(ui, "To Y", &mut g.b.y, -1e4, 1e4)
                {
                    label = "Move guide";
                }
            }
        }
        if !label.is_empty() {
            let r = self.doc.edit_merged(id, label, |e| *e = edited.clone());
            self.report(r, format!("{label}."));
        }
        if extrude_sketch {
            self.extrude_sketch();
            return;
        }
        if redraw {
            self.plan.cancel();
            self.plan.tool = Tool::Select;
            self.plan.outline = crate::outline::Outline::start(self.doc.model(), id);
            if let Some(o) = &self.plan.outline {
                self.status = o.hint().into();
            }
        }

        self.assign_panel(ui, id);
        self.cuts_panel(ui, id);

        ui.add_space(10.0);
        let deps = self.doc.model().dependents(id).len();
        let text = if deps > 0 {
            format!("Delete (and {deps} attached)")
        } else {
            "Delete".into()
        };
        if ui.button(text).clicked() {
            self.delete_selected();
        }
    }
}

impl eframe::App for CreaBimApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        let dropped: Vec<String> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().display().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        });
        for path in dropped {
            self.path = path.clone();
            self.open_path(&path);
        }
        if self.live_reload {
            for f in self.watch.changed(self.doc.model()) {
                self.reload(&f);
            }
            if !crate::files::linked_files(self.doc.model()).is_empty() {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
        }

        egui::Panel::top("menu").show(ui, |ui| {
            self.menu(ui);
            self.toolbar(ui);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{} elements", self.doc.model().len()));
                });
            });
        });
        egui::Panel::left("outliner")
            .default_size(220.0)
            .show(ui, |ui| self.outliner(ui));
        egui::Panel::right("properties")
            .default_size(260.0)
            .show(ui, |ui| self.properties(ui));

        egui::CentralPanel::default().show(ui, |ui| {
            let full = ui.available_rect_before_wrap();
            let plan_bg = Color32::from_rgb(252, 252, 250);
            match self.placement {
                PlanPlacement::Docked => {
                    let (first, second) = split_panes(&mut self.layout, ui, full);
                    let (a, b) = if self.layout.swapped {
                        (second, first)
                    } else {
                        (first, second)
                    };
                    self.show_3d(ui, a, frame);
                    self.show_plan(ui, b, plan_bg);
                }
                PlanPlacement::Overlay => {
                    self.show_3d(ui, full, frame);
                    let inset = Rect::from_min_size(
                        full.right_top() + Vec2::new(-full.width() * 0.42 - 12.0, 12.0),
                        Vec2::new(full.width() * 0.42, full.height() * 0.45),
                    );
                    self.show_plan(
                        ui,
                        inset,
                        Color32::from_rgba_unmultiplied(252, 252, 250, 225),
                    );
                    ui.painter().rect_stroke(
                        inset,
                        4.0,
                        egui::Stroke::new(1.0, Color32::from_gray(160)),
                        egui::StrokeKind::Outside,
                    );
                }
                PlanPlacement::Window | PlanPlacement::Hidden => self.show_3d(ui, full, frame),
            }
            ui.allocate_rect(full, Sense::hover());
        });

        if let Some(s) = self.plan.status.take() {
            self.status = s;
        }

        if self.placement == PlanPlacement::Window {
            let mut close = false;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("creabim_plan"),
                egui::ViewportBuilder::default()
                    .with_title("CreaBIM · Plan")
                    .with_inner_size([760.0, 560.0]),
                |ui, _class| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        let rect = ui.available_rect_before_wrap();
                        self.show_plan(ui, rect, plan_bg_color());
                        ui.allocate_rect(rect, Sense::hover());
                    });
                    if ui.input(|i| i.viewport().close_requested()) {
                        close = true;
                    }
                },
            );
            if close {
                self.placement = PlanPlacement::Docked;
            }
        }
    }
}

/// Split `full` in two at the divider and let the divider be dragged.
/// Double-clicking it evens the two halves out again.
fn split_panes(l: &mut PaneLayout, ui: &mut egui::Ui, full: Rect) -> (Rect, Rect) {
    let gap = 3.0;
    let along = |r: Rect| if l.stacked { r.height() } else { r.width() };
    let size = along(full);
    let at = |f: f32| {
        if l.stacked {
            full.min.y + f * size
        } else {
            full.min.x + f * size
        }
    };
    let cut = at(l.split);
    let handle = if l.stacked {
        Rect::from_min_max(
            egui::pos2(full.min.x, cut - gap),
            egui::pos2(full.max.x, cut + gap),
        )
    } else {
        Rect::from_min_max(
            egui::pos2(cut - gap, full.min.y),
            egui::pos2(cut + gap, full.max.y),
        )
    };
    let r = ui.interact(
        handle,
        ui.id().with("pane_divider"),
        Sense::click_and_drag(),
    );
    if r.hovered() || r.dragged() {
        ui.ctx().set_cursor_icon(if l.stacked {
            egui::CursorIcon::ResizeVertical
        } else {
            egui::CursorIcon::ResizeHorizontal
        });
    }
    if r.dragged() {
        let d = if l.stacked {
            r.drag_delta().y
        } else {
            r.drag_delta().x
        };
        l.split = (l.split + d / size).clamp(0.15, 0.85);
    }
    if r.double_clicked() {
        l.split = 0.5;
    }
    let cut = at(l.split);
    let color = if r.hovered() || r.dragged() {
        Color32::from_rgb(90, 140, 220)
    } else {
        Color32::from_gray(200)
    };
    let (a, b) = if l.stacked {
        (
            Rect::from_min_max(full.min, egui::pos2(full.max.x, cut - gap)),
            Rect::from_min_max(egui::pos2(full.min.x, cut + gap), full.max),
        )
    } else {
        (
            Rect::from_min_max(full.min, egui::pos2(cut - gap, full.max.y)),
            Rect::from_min_max(egui::pos2(cut + gap, full.min.y), full.max),
        )
    };
    let line = if l.stacked {
        [egui::pos2(full.min.x, cut), egui::pos2(full.max.x, cut)]
    } else {
        [egui::pos2(cut, full.min.y), egui::pos2(cut, full.max.y)]
    };
    ui.painter()
        .line_segment(line, egui::Stroke::new(2.0, color));
    (a, b)
}

fn plan_bg_color() -> Color32 {
    Color32::from_rgb(252, 252, 250)
}

fn line_vertex(pos: [f32; 3], color: [f32; 3]) -> Vertex {
    Vertex {
        pos,
        normal: [0.0, 0.0, 1.0],
        color,
    }
}

/// Triangle indices (into the flattened mesh) that make up polygon face `face`.
fn face_triangles(mesh: &creabim_core::EditMesh, face: usize) -> std::ops::Range<usize> {
    let start: usize = mesh.faces[..face.min(mesh.faces.len())]
        .iter()
        .map(|f| f.len() - 2)
        .sum();
    let len = mesh.faces.get(face).map_or(0, |f| f.len() - 2);
    start..start + len
}

fn stem(path: &str) -> String {
    let name = std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.split('.').next().unwrap_or("project").to_string()
}

fn with_extension(path: &str, ext: &str) -> String {
    let p = std::path::Path::new(path);
    let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    dir.join(format!("{}.{ext}", stem(path)))
        .to_string_lossy()
        .into_owned()
}

/// Give every level a plan view so older files open with something to show.
fn with_plan_views(model: Model) -> Model {
    let planned: Vec<ElementId> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::View(View {
                kind: ViewKind::Plan { level, .. },
                ..
            }) => Some(*level),
            _ => None,
        })
        .collect();
    let mut tx = Transaction::new("Add plan views");
    let mut any = false;
    for (e, _) in model.levels() {
        if !planned.contains(&e.id) {
            tx = tx.insert(Element::new(
                format!("{} plan", e.name),
                ElementKind::View(View::plan(e.id)),
            ));
            any = true;
        }
    }
    if !any {
        return model;
    }
    let mut doc = Document::from_model(model);
    // a failed insert leaves the model as it was, which still opens
    let _ = doc.commit(tx);
    doc.model().clone()
}

/// A file's name without its folders.
fn file_label(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or(path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// How far guides reach each way in the 3D view.
const GUIDE_REACH_3D: f64 = 60.0;

/// A guide through `a` and `b`, dashed across the 3D view. It is sampled
/// along its length so the parts behind the camera drop out cleanly.
fn draw_guide_3d(
    painter: &egui::Painter,
    project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    a: DVec3,
    b: DVec3,
    stroke: egui::Stroke,
) {
    let Some(d) = (b - a).try_normalize() else {
        return;
    };
    let m = (a + b) / 2.0;
    let mut run: Vec<egui::Pos2> = Vec::new();
    let steps = 240;
    for i in 0..=steps {
        let t = -GUIDE_REACH_3D + 2.0 * GUIDE_REACH_3D * i as f64 / steps as f64;
        match project(m + d * t).filter(|p| painter.clip_rect().expand(200.0).contains(*p)) {
            Some(p) => run.push(p),
            None => {
                if run.len() > 1 {
                    painter.extend(egui::Shape::dashed_line(&run, stroke, 6.0, 4.0));
                }
                run.clear();
            }
        }
    }
    if run.len() > 1 {
        painter.extend(egui::Shape::dashed_line(&run, stroke, 6.0, 4.0));
    }
}
