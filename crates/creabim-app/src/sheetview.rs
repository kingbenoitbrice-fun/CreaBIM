//! The sheet pane: a sheet drawn exactly as it prints, with live views.
//! Drag a view to move it on the sheet; double-click it to work on the model
//! through it, with the same tools as the view itself.

use crate::hoverpick::{self, Pane};
use crate::planview::{self, PaneStyle, PlanState};
use crate::selection::Selection;
use creabim_core::{Document, ElementId, ElementKind};
use creabim_geom::paper::{self, Anchor, Canvas, Stroke};
use creabim_geom::sheet::{self, PlacedView, SheetInfo};
use creabim_geom::triangulate::triangulate;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};
use glam::DVec2;
use std::collections::HashMap;

#[derive(Default)]
pub struct SheetState {
    /// Pan and zoom per sheet: centre in mm, pixels per mm.
    cameras: HashMap<ElementId, (DVec2, f32)>,
    /// The viewport being worked through, if any.
    pub active: Option<ElementId>,
    /// Viewport being moved: its centre and the grab point when the drag began.
    drag: Option<(ElementId, DVec2, DVec2)>,
    /// Crop edge being dragged: viewport, edge (left, right, bottom, top)
    /// and the crop when the drag began.
    crop_drag: Option<(ElementId, usize, [DVec2; 2])>,
}

/// Sheet millimetres (y up) to screen points.
#[derive(Clone, Copy)]
struct PaperXf {
    rect: Rect,
    center: DVec2,
    zoom: f32,
}

impl PaperXf {
    fn to_screen(self, p: DVec2) -> Pos2 {
        let d = (p - self.center).as_vec2() * self.zoom;
        self.rect.center() + Vec2::new(d.x, -d.y)
    }

    fn to_paper(self, p: Pos2) -> DVec2 {
        let d = (p - self.rect.center()) / self.zoom;
        self.center + DVec2::new(d.x as f64, -d.y as f64)
    }

    fn rect_of(self, lo: DVec2, hi: DVec2) -> Rect {
        Rect::from_two_pos(self.to_screen(lo), self.to_screen(hi))
    }
}

/// A [`Canvas`] that paints into egui, so the sheet looks as it prints.
struct EguiCanvas<'a> {
    painter: &'a egui::Painter,
    xf: PaperXf,
}

fn gray(g: f64) -> Color32 {
    Color32::from_gray((g.clamp(0.0, 1.0) * 255.0) as u8)
}

impl EguiCanvas<'_> {
    fn line_in(&mut self, pts: &[DVec2], s: Stroke, color: Color32) {
        let pts: Vec<Pos2> = pts.iter().map(|p| self.xf.to_screen(*p)).collect();
        let stroke = egui::Stroke::new((s.width as f32 * self.xf.zoom).max(0.5), color);
        match s.dash {
            Some([a, b]) => {
                let z = self.xf.zoom;
                self.painter.extend(egui::Shape::dashed_line(
                    &pts,
                    stroke,
                    (a as f32 * z).max(2.0),
                    (b as f32 * z).max(1.0),
                ));
            }
            None => {
                self.painter.add(egui::Shape::line(pts, stroke));
            }
        }
    }
}

impl Canvas for EguiCanvas<'_> {
    fn line(&mut self, pts: &[DVec2], s: Stroke) {
        self.line_in(pts, s, gray(s.gray));
    }

    fn line_rgb(&mut self, pts: &[DVec2], s: Stroke, rgb: [f32; 3]) {
        self.line_in(pts, s, crate::annotate::color32(rgb));
    }

    fn fill_rgb(&mut self, pts: &[DVec2], rgb: [f32; 3]) {
        let pts: Vec<Pos2> = pts.iter().map(|p| self.xf.to_screen(*p)).collect();
        let c = crate::annotate::color32(rgb);
        self.painter
            .add(egui::Shape::convex_polygon(pts, c, egui::Stroke::NONE));
    }

    fn styled_text(
        &mut self,
        at: DVec2,
        text: &str,
        anchor: Anchor,
        angle: f64,
        look: &creabim_geom::annotation::TextLook,
    ) {
        let px = look.size as f32 * self.xf.zoom;
        if px < 3.0 {
            return;
        }
        let color = look.rgb.map_or(Color32::BLACK, crate::annotate::color32);
        crate::annotate::text(
            self.painter,
            self.xf.to_screen(at),
            text,
            px,
            color,
            angle,
            anchor,
            look,
        );
    }

    fn fill(&mut self, pts: &[DVec2], g: f64) {
        let mut mesh = egui::Mesh::default();
        for p in pts {
            mesh.colored_vertex(self.xf.to_screen(*p), gray(g));
        }
        for [a, b, c] in triangulate(pts) {
            mesh.add_triangle(a as u32, b as u32, c as u32);
        }
        self.painter.add(egui::Shape::mesh(mesh));
    }

    fn text_angle(&mut self, at: DVec2, size: f64, text: &str, anchor: Anchor, angle: f64) {
        let px = size as f32 * self.xf.zoom;
        if px < 3.0 {
            return;
        }
        let galley = self.painter.layout_no_wrap(
            text.to_string(),
            egui::FontId::proportional(px),
            Color32::BLACK,
        );
        let w = galley.size().x;
        // centre of the text, along its baseline direction from the anchor
        let shift = match anchor {
            Anchor::Start => w / 2.0,
            Anchor::Middle => 0.0,
            Anchor::End => -w / 2.0,
        };
        let rot = egui::emath::Rot2::from_angle(-angle as f32);
        let centre = self.xf.to_screen(at) + rot * Vec2::new(shift, 0.0);
        let pos = centre - rot * (galley.size() / 2.0);
        self.painter.add(
            egui::epaint::TextShape::new(pos, galley, Color32::BLACK).with_angle(-angle as f32),
        );
    }

    fn image(
        &mut self,
        lo: DVec2,
        hi: DVec2,
        clip: Option<(DVec2, DVec2)>,
        raster: &std::sync::Arc<creabim_geom::raster::Raster>,
    ) {
        let rect = self.xf.rect_of(lo, hi);
        let clip = clip.map(|(a, b)| self.xf.rect_of(a, b));
        crate::textures::paint(self.painter, rect, clip, raster);
    }
}

const ACCENT: Color32 = Color32::from_rgb(230, 120, 30);

/// Draw `d` as it prints, as large as fits in `rect`: a type's preview.
/// Lines keep their printed weights at three pixels a millimetre.
pub fn draw_fit(painter: &egui::Painter, rect: Rect, d: &creabim_geom::PlanView) {
    let Some((lo, hi)) = d.bounds() else { return };
    let size = (hi - lo).max(DVec2::splat(0.1));
    let zoom = 3.0f32;
    // paper millimetres a model metre, for the drawing to fill the rect
    let k = (rect.width() as f64 / zoom as f64 / size.x)
        .min(rect.height() as f64 / zoom as f64 / size.y)
        * 0.86;
    let map = paper::PaperMap {
        model: (lo + hi) / 2.0,
        paper: DVec2::ZERO,
        scale: 1000.0 / k.max(1e-6),
    };
    let mut canvas = EguiCanvas {
        painter,
        xf: PaperXf {
            rect,
            center: DVec2::ZERO,
            zoom,
        },
    };
    paper::draw(&mut canvas, d, &map);
}

/// The scales a view is usually drawn at on a sheet, as denominators.
pub const SCALES: [f64; 10] = [1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 500.0];

/// Where a viewport's centre goes when its view's scale changes from
/// `old` to `new` (denominators), so that the drawing grows or shrinks
/// about `mid`, the middle of what it shows (drawing coordinates), and not
/// about the point it is pinned by.
pub fn rescaled_center(vp: &creabim_core::Viewport, mid: DVec2, old: f64, new: f64) -> DVec2 {
    let at = |scale: f64| (mid - vp.target) * (1000.0 / scale);
    vp.center + at(old) - at(new)
}

/// A dropdown of the usual scales and a box for any other: the new
/// denominator when one is picked or typed.
pub fn scale_picker(ui: &mut egui::Ui, salt: ElementId, scale: f64) -> Option<f64> {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.label("Scale");
        let standard = SCALES.iter().any(|s| (s - scale).abs() < 1e-6);
        egui::ComboBox::from_id_salt(("scale", salt))
            .selected_text(if standard {
                format!("1:{scale:.0}")
            } else {
                format!("1:{scale:.0} (custom)")
            })
            .show_ui(ui, |ui| {
                for s in SCALES {
                    if ui
                        .selectable_label((s - scale).abs() < 1e-6, format!("1:{s:.0}"))
                        .clicked()
                    {
                        out = Some(s);
                    }
                }
            });
        let mut v = scale;
        if ui
            .add(
                egui::DragValue::new(&mut v)
                    .speed(1.0)
                    .range(1.0..=5000.0)
                    .prefix("1:"),
            )
            .on_hover_text("Any other scale: type its denominator")
            .changed()
        {
            out = Some(v.round().max(1.0));
        }
    });
    out
}

fn contains(v: &PlacedView, p: DVec2) -> bool {
    let (lo, hi) = v.bounds;
    let pad = DVec2::splat(2.0);
    let title = v.title_at() - DVec2::new(0.0, 5.0);
    p.cmpge(lo.min(title) - pad).all() && p.cmple(hi + pad).all()
}

/// Show sheet `sheet` with its `views` in `rect`. Returns a status message when an edit fails.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    doc: &mut Document,
    sheet_id: ElementId,
    views: &[PlacedView],
    info: &SheetInfo,
    state: &mut SheetState,
    plan: &mut PlanState,
    selection: &mut Selection,
) -> Option<String> {
    let s = sheet::sheet_of(doc.model(), sheet_id)?.clone();
    let title = doc
        .model()
        .get(sheet_id)
        .map(|e| e.name.clone())
        .unwrap_or_default();
    let size = DVec2::new(s.width, s.height);
    if state
        .active
        .is_some_and(|a| !views.iter().any(|v| v.viewport == a))
    {
        state.active = None;
    }

    let response = ui.interact(
        rect,
        ui.id().with(("sheet", sheet_id)),
        Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_gray(205));

    let (mut center, mut zoom) = *state.cameras.entry(sheet_id).or_insert_with(|| {
        let z = (rect.width() / s.width as f32).min(rect.height() / s.height as f32) * 0.92;
        (size / 2.0, z)
    });
    if response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle)
    {
        let d = response.drag_delta() / zoom;
        center -= DVec2::new(d.x as f64, -d.y as f64);
    }
    if response.hovered() || state.active.is_some() && ui.rect_contains_pointer(rect) {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if let (true, Some(hover)) = (scroll != 0.0, ui.input(|i| i.pointer.hover_pos())) {
            let before = PaperXf { rect, center, zoom }.to_paper(hover);
            zoom = (zoom * (1.0 + scroll * 0.002)).clamp(0.2, 60.0);
            center += before - PaperXf { rect, center, zoom }.to_paper(hover);
        }
    }
    state.cameras.insert(sheet_id, (center, zoom));
    let xf = PaperXf { rect, center, zoom };

    // the paper, its frame and every view not being worked on
    let paper_rect = xf.rect_of(DVec2::ZERO, size);
    painter.rect_filled(
        paper_rect.translate(Vec2::splat(4.0)),
        0.0,
        Color32::from_gray(150),
    );
    painter.rect_filled(paper_rect, 0.0, Color32::WHITE);
    let mut canvas = EguiCanvas {
        painter: &painter,
        xf,
    };
    sheet::draw_frame(
        &mut canvas,
        doc.model(),
        &s,
        &title,
        &sheet::scale_text(doc.model(), views),
        info,
    );
    for v in views {
        if state.active != Some(v.viewport) {
            paper::draw_with(&mut canvas, &v.drawing, &v.map, &v.weights);
        }
        sheet::draw_view_title(&mut canvas, v);
        if selection.contains(v.viewport) && state.active.is_none() {
            let (lo, hi) = v.bounds;
            painter.rect_stroke(
                xf.rect_of(lo, hi).expand(4.0),
                2.0,
                egui::Stroke::new(1.5, ACCENT),
                egui::StrokeKind::Outside,
            );
        }
    }
    sheet::draw_sheet_notes(&mut canvas, doc.model(), sheet_id);

    // pre-selection: the view under the pointer, and the others under it
    // where views overlap
    let pane = Pane::Sheet(sheet_id);
    if state.active.is_none() && !response.dragged() {
        let pointer = response
            .hover_pos()
            .filter(|_| plan.tool == planview::Tool::Select && plan.modal.is_none());
        let said = hoverpick::track(
            ui.ctx(),
            &mut plan.hover,
            pane,
            pointer,
            doc.model().revision(),
            doc.model(),
            |p| {
                let p = xf.to_paper(p);
                views
                    .iter()
                    .rev()
                    .filter(|v| contains(v, p))
                    .map(|v| v.viewport)
                    .collect()
            },
        );
        if said.is_some() {
            plan.status = said;
        }
    }
    let lit = hoverpick::highlighted(&plan.hover).filter(|id| !selection.contains(*id));
    if let Some(v) = views.iter().find(|v| Some(v.viewport) == lit) {
        let (lo, hi) = v.bounds;
        painter.rect_stroke(
            xf.rect_of(lo, hi).expand(4.0),
            2.0,
            egui::Stroke::new(1.5, hoverpick::HOVER.gamma_multiply(0.7)),
            egui::StrokeKind::Outside,
        );
    }
    if state.active.is_none() {
        if let Some(id) = hoverpick::badge(ui.ctx(), &mut plan.hover, pane, doc.model()) {
            selection.set(Some(id));
        }
    }

    // crop handles on the selected viewport: drag an edge to crop the view
    let mut error = None;
    let pointer = response.interact_pointer_pos().or(response.hover_pos());
    let selected = views
        .iter()
        .find(|v| selection.contains(v.viewport) && state.active.is_none());
    if let Some(v) = selected {
        let crop = match doc.model().get(v.viewport).map(|e| &e.kind) {
            Some(ElementKind::Viewport(vp)) => vp.crop,
            _ => None,
        };
        let full = crop.or_else(|| v.drawing.bounds().map(|(a, b)| [a, b]));
        if let Some([lo, hi]) = full {
            let (plo, phi) = (v.map.to_paper(lo), v.map.to_paper(hi));
            let r = xf.rect_of(plo, phi);
            painter.extend(egui::Shape::dashed_line(
                &[
                    r.left_top(),
                    r.right_top(),
                    r.right_bottom(),
                    r.left_bottom(),
                    r.left_top(),
                ],
                egui::Stroke::new(1.0, ACCENT),
                6.0,
                4.0,
            ));
            let handles = [
                r.left_center(),
                r.right_center(),
                r.center_bottom(),
                r.center_top(),
            ];
            for h in handles {
                painter.rect(
                    Rect::from_center_size(h, Vec2::splat(8.0)),
                    0.0,
                    Color32::WHITE,
                    egui::Stroke::new(1.5, ACCENT),
                    egui::StrokeKind::Middle,
                );
            }
            if response.drag_started_by(egui::PointerButton::Primary) {
                let press = ui.input(|i| i.pointer.press_origin()).or(pointer);
                if let Some(p) = press {
                    if let Some(edge) = handles.iter().position(|h| h.distance(p) < 9.0) {
                        state.crop_drag = Some((v.viewport, edge, [lo, hi]));
                    }
                }
            }
            if let (Some((id, edge, [lo0, hi0])), Some(p)) = (state.crop_drag, pointer) {
                if id == v.viewport && response.dragged_by(egui::PointerButton::Primary) {
                    let m = v.map.to_model(xf.to_paper(p));
                    let snap = |x: f64| (x / 0.05).round() * 0.05;
                    let (mut lo, mut hi) = (lo0, hi0);
                    match edge {
                        0 => lo.x = snap(m.x).min(hi.x - 0.2),
                        1 => hi.x = snap(m.x).max(lo.x + 0.2),
                        2 => lo.y = snap(m.y).min(hi.y - 0.2),
                        _ => hi.y = snap(m.y).max(lo.y + 0.2),
                    }
                    let r = doc.edit_merged(id, "Crop view", |e| {
                        if let ElementKind::Viewport(vp) = &mut e.kind {
                            vp.crop = Some([lo, hi]);
                        }
                    });
                    error = r.err().map(|e| e.to_string());
                }
            }
        }
    }
    if response.drag_stopped() {
        state.crop_drag = None;
    }

    // notes written on the sheet itself
    if state.active.is_none() {
        let pxf = planview::Xf {
            rect,
            center,
            scale: zoom,
        };
        let took = crate::annotate::sheet_input(
            doc,
            sheet_id,
            &painter,
            &response,
            plan.tool,
            &mut plan.anno,
            selection,
            &pxf,
            &mut plan.status,
            &mut error,
        );
        if took {
            return error;
        }
    }

    // picking, moving and activating viewports
    let hit = pointer.and_then(|p| {
        let p = xf.to_paper(p);
        views.iter().rev().find(|v| contains(v, p))
    });
    if response.double_clicked() {
        state.active = hit.map(|v| v.viewport);
        if state.active.is_some() {
            selection.clear();
        }
    } else if response.clicked() {
        let chosen = pointer.and_then(|p| plan.hover.as_ref()?.at(pane, p));
        selection.set(chosen.or(hit.map(|v| v.viewport)));
        state.active = None;
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !plan.busy() {
        state.active = None;
    }
    // right-click a view for its menu: its scale, and opening it
    if response.secondary_clicked()
        && state.active.is_none()
        && plan.tool == planview::Tool::Select
        && plan.web_menus
    {
        if let Some(v) = hit {
            selection.only(v.viewport);
        }
        plan.menu_at = None;
        plan.menu_request = pointer;
    }
    if response.drag_started_by(egui::PointerButton::Primary) && state.crop_drag.is_none() {
        let press = ui.input(|i| i.pointer.press_origin()).or(pointer);
        let hit = press.and_then(|p| {
            let p = xf.to_paper(p);
            views.iter().rev().find(|v| contains(v, p))
        });
        if let (Some(v), Some(p)) = (hit, press) {
            let center = match doc.model().get(v.viewport).map(|e| &e.kind) {
                Some(ElementKind::Viewport(vp)) => vp.center,
                _ => v.map.paper,
            };
            state.drag = Some((v.viewport, center, xf.to_paper(p)));
            selection.only(v.viewport);
        }
    }
    if let (Some((id, c0, grab)), Some(p)) = (state.drag, pointer) {
        if response.dragged_by(egui::PointerButton::Primary) {
            let moved = (c0 + xf.to_paper(p) - grab).round();
            let r = doc.edit_merged(id, "Move view on sheet", |e| {
                if let ElementKind::Viewport(vp) = &mut e.kind {
                    vp.center = moved;
                }
            });
            error = r.err().map(|e| e.to_string());
        }
    }
    if response.drag_stopped() {
        state.drag = None;
    }

    // the active viewport is a live editor of the model
    if let Some(v) = state
        .active
        .and_then(|a| views.iter().find(|v| v.viewport == a))
    {
        let (lo, hi) = v.bounds;
        let area = xf.rect_of(lo, hi).expand(6.0).intersect(rect);
        let paper_mid = xf.to_paper(area.center());
        let camera = (v.map.to_model(paper_mid), zoom * v.map.k() as f32);
        let kind = doc.model().get(v.view).and_then(|e| match &e.kind {
            ElementKind::View(view) => Some(planview::pane_kind(view)),
            _ => None,
        });
        if let Some(kind) = kind {
            let style = PaneStyle {
                background: Some(Color32::from_rgb(255, 253, 248)),
                decorations: false,
                camera: Some(camera),
                scale: v.map.scale,
            };
            // work on the whole view, not just its cropped part
            let full;
            let drawing = if matches!(doc.model().get(v.viewport).map(|e| &e.kind), Some(ElementKind::Viewport(vp)) if vp.crop.is_some())
            {
                full = creabim_geom::section::view_drawing(doc.model(), v.view).unwrap_or_default();
                &full
            } else {
                &v.drawing
            };
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
            if let Some(e) = planview::show(
                &mut child, area, doc, v.view, kind, drawing, plan, selection, style,
            ) {
                error = Some(e);
            }
            painter.rect_stroke(
                area,
                2.0,
                egui::Stroke::new(2.0, ACCENT),
                egui::StrokeKind::Outside,
            );
            painter.text(
                area.left_top() + Vec2::new(0.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                format!("Working in {}. Esc or click outside to finish.", v.title),
                egui::FontId::proportional(12.0),
                ACCENT,
            );
        }
    }

    painter.text(
        rect.left_bottom() + Vec2::new(8.0, -8.0),
        egui::Align2::LEFT_BOTTOM,
        if state.active.is_some() {
            "Edits here change the model: every view, sheet and the 3D update at once."
        } else {
            "Drag a view to move it, or its orange handles to crop it. Double-click a view to work on the model through it. Right-drag pans, the wheel zooms."
        },
        egui::FontId::proportional(12.0),
        Color32::from_gray(60),
    );
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_scale_grows_the_view_about_its_middle() {
        // pinned by the drawing's corner (0, 0) at 100, 100 on the sheet;
        // its middle (10, 5) is then at 200, 150 at 1:100
        let vp = creabim_core::Viewport {
            sheet: ElementId::NIL,
            view: ElementId::NIL,
            center: DVec2::new(100.0, 100.0),
            target: DVec2::ZERO,
            crop: None,
        };
        let mid = DVec2::new(10.0, 5.0);
        let map = |c: DVec2, s: f64| c + (mid - vp.target) * (1000.0 / s);
        assert_eq!(map(vp.center, 100.0), DVec2::new(200.0, 150.0));
        for new in [50.0, 200.0, 20.0] {
            let c = rescaled_center(&vp, mid, 100.0, new);
            // the middle stays where it was on the sheet
            assert!(
                (map(c, new) - DVec2::new(200.0, 150.0)).length() < 1e-9,
                "1:{new}"
            );
        }
        // pinned by its middle: the pin doesn't move
        let pinned = creabim_core::Viewport { target: mid, ..vp };
        assert_eq!(rescaled_center(&pinned, mid, 100.0, 50.0), vp.center);
        assert!(SCALES.contains(&50.0) && SCALES.contains(&1.0) && SCALES.contains(&500.0));
    }
}
