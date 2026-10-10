use crate::camera::OrbitCamera;
use crate::planview::{self, PlanState, Tool};
use crate::render3d::{Vertex, Viewport3d};
use creabim_core::{
    element::FREE_MESH_CLASSES, sample::sample_house, Document, Element, ElementId, ElementKind,
    Model, Transaction,
};
use creabim_geom::{build_scene, pick, plan::Pen, plan_view, PlanItem, SceneItem};
use eframe::egui::{self, Color32, Rect, Sense, Vec2};
use eframe::egui_wgpu;
use glam::{DVec2, Vec2 as GVec2};

/// What the 3D geometry was last built from; rebuild when any of it changes.
/// (model revision, selection, selected face, plan shown in 3D, active level)
type GeometryKey = (
    u64,
    Option<ElementId>,
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

pub struct CreaBimApp {
    doc: Document,
    selection: Option<ElementId>,
    /// Selected face when a free mesh is selected (mesh edit mode).
    face: Option<usize>,
    level: Option<ElementId>,
    camera: OrbitCamera,
    /// Aspect ratio of the 3D view as last drawn, for framing.
    aspect: f32,
    viewport: Option<Viewport3d>,
    geometry_key: Option<GeometryKey>,
    scene: Vec<SceneItem>,
    plan: PlanState,
    placement: PlanPlacement,
    plan_in_3d: bool,
    extrude_distance: f64,
    path: String,
    status: String,
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
            selection: None,
            face: None,
            level: None,
            camera: OrbitCamera::default(),
            aspect: 0.65,
            viewport,
            geometry_key: None,
            scene: vec![],
            plan: PlanState::default(),
            placement: PlanPlacement::Docked,
            plan_in_3d: false,
            extrude_distance: 1.0,
            path: "project.creabim.json".into(),
            status: "Loaded the sample house. Orbit with left drag, pan with right drag, zoom with the wheel.".into(),
        };
        app.load_model(sample_house());
        app
    }

    fn load_model(&mut self, model: Model) {
        self.doc = Document::from_model(model);
        self.selection = None;
        self.face = None;
        self.level = self.doc.model().levels().next().map(|(e, _)| e.id);
        self.geometry_key = None;
        self.frame_all();
    }

    fn frame_all(&mut self) {
        let scene = build_scene(self.doc.model());
        let bounds = scene
            .iter()
            .filter_map(|s| s.mesh.bounds())
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)));
        if let Some((lo, hi)) = bounds {
            self.camera.frame(lo, hi, self.aspect);
            self.plan.center =
                DVec2::new(((lo.x + hi.x) / 2.0) as f64, ((lo.y + hi.y) / 2.0) as f64);
        }
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
            self.extrude_face();
        }
        self.drop_stale_selection();
    }

    fn drop_stale_selection(&mut self) {
        if self
            .selection
            .is_some_and(|id| self.doc.model().get(id).is_none())
        {
            self.selection = None;
            self.face = None;
        }
        if self
            .level
            .is_none_or(|id| self.doc.model().level(id).is_none())
        {
            self.level = self.doc.model().levels().next().map(|(e, _)| e.id);
        }
    }

    fn delete_selected(&mut self) {
        let Some(id) = self.selection else { return };
        let name = self
            .doc
            .model()
            .get(id)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        let tx = Transaction::new(format!("Delete {name}")).remove_cascade(self.doc.model(), id);
        let r = self.doc.commit(tx);
        self.report(r, format!("Deleted {name} and anything hosted on it."));
        self.drop_stale_selection();
    }

    fn extrude_face(&mut self) {
        let (Some(id), Some(face)) = (self.selection, self.face) else {
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

    // ---- 3D -------------------------------------------------------------

    fn rebuild_geometry(&mut self, rs: &egui_wgpu::RenderState) {
        let key = (
            self.doc.model().revision(),
            self.selection,
            self.face,
            self.plan_in_3d,
            self.level,
        );
        if self.geometry_key == Some(key) {
            return;
        }
        self.geometry_key = Some(key);
        self.scene = build_scene(self.doc.model());

        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let highlight = [0.95, 0.55, 0.20];
        for item in &self.scene {
            let selected = Some(item.id) == self.selection;
            let base = vertices.len() as u32;
            let face_tris = match (
                selected,
                self.face,
                self.doc.model().get(item.id).map(|e| &e.kind),
            ) {
                (true, Some(f), Some(ElementKind::FreeMesh(m))) => Some(face_triangles(&m.mesh, f)),
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
                for t in tris.clone() {
                    for k in 0..3 {
                        colors[item.mesh.indices[t * 3 + k] as usize] = highlight;
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
            if let Some(level) = self.level {
                let z = self
                    .doc
                    .model()
                    .level(level)
                    .map_or(0.0, |l| l.elevation + self.plan.cut_height)
                    as f32;
                let plan = plan_view(self.doc.model(), level, self.plan.cut_height);
                let mut seg = |a: DVec2, b: DVec2, c: [f32; 3]| {
                    overlay.push(line_vertex([a.x as f32, a.y as f32, z], c));
                    overlay.push(line_vertex([b.x as f32, b.y as f32, z], c));
                };
                for it in &plan.items {
                    let c = match it {
                        PlanItem::Polygon { pen: Pen::Cut, .. } => [0.85, 0.15, 0.10],
                        _ => [0.15, 0.25, 0.55],
                    };
                    match it {
                        PlanItem::Polygon { points, .. } => {
                            for i in 0..points.len() {
                                seg(points[i], points[(i + 1) % points.len()], c);
                            }
                        }
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
                        PlanItem::Label { .. } => {}
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
        if response.dragged_by(egui::PointerButton::Primary) {
            let d = response.drag_delta();
            self.camera.orbit(GVec2::new(d.x, d.y));
        }
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
        if response.clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                let ndc = GVec2::new(
                    (p.x - rect.left()) / rect.width() * 2.0 - 1.0,
                    1.0 - (p.y - rect.top()) / rect.height() * 2.0,
                );
                let (o, d) = self.camera.ray(ndc, aspect);
                let hit = pick::pick(&self.scene, o, d);
                self.selection = hit.map(|h| h.id);
                self.face = hit.and_then(|h| match self.doc.model().get(h.id).map(|e| &e.kind) {
                    Some(ElementKind::FreeMesh(m)) => pick::face_of_triangle(&m.mesh, h.triangle),
                    _ => None,
                });
            }
        }

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
    }

    fn show_plan(&mut self, ui: &mut egui::Ui, rect: Rect, background: Color32) {
        let Some(level) = self.level else { return };
        let (before, revision) = (self.selection, self.doc.model().revision());
        if let Some(err) = planview::show(
            ui,
            rect,
            &mut self.doc,
            level,
            &mut self.plan,
            &mut self.selection,
            background,
        ) {
            self.status = format!("Couldn't do that: {err}");
        } else if self.doc.model().revision() != revision {
            if let Some(label) = self.doc.undo_label() {
                self.status = format!("{label}. Ctrl+Z to undo.");
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
            .get(level)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        ui.painter().text(
            rect.left_top() + Vec2::new(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            format!("Plan · {name}"),
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
                ui.separator();
                if ui.button("Open").clicked() {
                    let r = std::fs::read_to_string(&self.path)
                        .map_err(|e| e.to_string())
                        .and_then(|s| Model::from_json(&s));
                    match r {
                        Ok(m) => {
                            self.load_model(m);
                            self.status = format!("Opened {}", self.path);
                        }
                        Err(e) => self.status = format!("Couldn't open {}: {e}", self.path),
                    }
                }
                if ui.button("Save").clicked() {
                    let r = std::fs::write(&self.path, self.doc.model().to_json());
                    self.report(r, format!("Saved {}", self.path));
                }
                ui.separator();
                if ui.button("Export IFC4").clicked() {
                    let out = with_extension(&self.path, "ifc");
                    let opts = creabim_ifc::ExportOptions {
                        project_name: stem(&self.path),
                        ..Default::default()
                    };
                    let r = std::fs::write(&out, creabim_ifc::export_ifc4(self.doc.model(), &opts));
                    self.report(r, format!("Exported {out}"));
                }
                if ui.button("Export plan as SVG").clicked() {
                    if let Some(level) = self.level {
                        let name = self
                            .doc
                            .model()
                            .get(level)
                            .map(|e| e.name.replace(' ', "-"))
                            .unwrap_or_default();
                        let out = with_extension(&self.path, &format!("{name}.svg"));
                        let svg =
                            plan_view(self.doc.model(), level, self.plan.cut_height).to_svg(100.0);
                        let r = std::fs::write(&out, svg);
                        self.report(r, format!("Exported {out} at 1:100"));
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
                ui.label("Plan view");
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
                ui.checkbox(&mut self.plan_in_3d, "Show plan cut in 3D space");
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
                    self.plan.pending = None;
                }
            }
            ui.separator();
            ui.label("Level:");
            let current = self
                .level
                .and_then(|id| self.doc.model().get(id))
                .map(|e| e.name.clone())
                .unwrap_or_default();
            egui::ComboBox::from_id_salt("level")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    let levels: Vec<(ElementId, String)> = self
                        .doc
                        .model()
                        .levels()
                        .map(|(e, _)| (e.id, e.name.clone()))
                        .collect();
                    for (id, name) in levels {
                        ui.selectable_value(&mut self.level, Some(id), name);
                    }
                });
            ui.label("Cut height:");
            ui.add(
                egui::DragValue::new(&mut self.plan.cut_height)
                    .speed(0.05)
                    .range(0.1..=10.0)
                    .suffix(" m"),
            );
            ui.label("Wall thickness:");
            ui.add(
                egui::DragValue::new(&mut self.plan.wall_thickness)
                    .speed(0.01)
                    .range(0.05..=1.0)
                    .suffix(" m"),
            );
        });
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
                egui::CollapsingHeader::new(format!("{class} ({})", items.len()))
                    .default_open(class == "IfcBuildingStorey")
                    .show(ui, |ui| {
                        for (id, name) in items {
                            if ui
                                .selectable_label(self.selection == Some(id), name)
                                .clicked()
                            {
                                self.selection = Some(id);
                                self.face = None;
                            }
                        }
                    });
            }
        });
    }

    fn properties(&mut self, ui: &mut egui::Ui) {
        ui.heading("Properties");
        let Some(id) = self.selection else {
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
            }
            ElementKind::Slab(s) => {
                if num(ui, "Thickness", &mut s.thickness, 0.02, 3.0) {
                    label = "Slab thickness"
                }
                ui.label(format!("{} boundary points", s.boundary.len()));
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
            ElementKind::Grid(g) => {
                ui.horizontal(|ui| {
                    ui.label("Label");
                    if ui.text_edit_singleline(&mut g.label).changed() {
                        label = "Grid label"
                    }
                });
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
                if num(ui, "X", &mut m.origin.x, -1e4, 1e4)
                    | num(ui, "Y", &mut m.origin.y, -1e4, 1e4)
                    | num(ui, "Z", &mut m.origin.z, -1e3, 1e3)
                {
                    label = "Move mesh"
                }
                ui.label(format!(
                    "{} vertices, {} faces",
                    m.mesh.vertices.len(),
                    m.mesh.faces.len()
                ));
                ui.separator();
                ui.label(egui::RichText::new("Mesh edit").strong());
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
        }
        if !label.is_empty() {
            let r = self.doc.edit_merged(id, label, |e| *e = edited.clone());
            self.report(r, format!("{label}."));
        }

        ui.add_space(10.0);
        let deps = self.doc.model().dependents(id).len();
        let text = if deps > 0 {
            format!("Delete (and {deps} hosted)")
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
                    let mid = full.center().x;
                    let left = Rect::from_min_max(full.min, egui::pos2(mid - 2.0, full.max.y));
                    let right = Rect::from_min_max(egui::pos2(mid + 2.0, full.min.y), full.max);
                    self.show_3d(ui, left, frame);
                    self.show_plan(ui, right, plan_bg);
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
