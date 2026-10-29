//! The Slab tool's ways of drawing a slab, picked in the tool options of
//! the top bar (or, with the classic panels, a small bar at a pane's
//! top-left corner): click a polygon (with arcs), drag out a rectangle,
//! fill the room under the pointer, or turn a flat sketch into a slab.

use crate::outline::NEW_SLAB_THICKNESS;
use crate::planview::PlanState;
use crate::selection::Selection;
use creabim_core::{Document, Element, ElementId, ElementKind, Slab, Transaction};
use eframe::egui;
use glam::DVec2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SlabMode {
    /// Click each corner; A and L switch between arcs and lines.
    #[default]
    Polygon,
    /// Two opposite corners.
    Rectangle,
    /// Click inside walls: the slab runs to their centre lines.
    FillRoom,
    /// Click a closed flat sketch; it is used up.
    FromSketch,
}

impl SlabMode {
    pub const ALL: [SlabMode; 4] = [
        SlabMode::Polygon,
        SlabMode::Rectangle,
        SlabMode::FillRoom,
        SlabMode::FromSketch,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SlabMode::Polygon => "Polygon (1)",
            SlabMode::Rectangle => "Rectangle (2)",
            SlabMode::FillRoom => "Fill room (3)",
            SlabMode::FromSketch => "From sketch (4)",
        }
    }

    fn key(self) -> egui::Key {
        match self {
            SlabMode::Polygon => egui::Key::Num1,
            SlabMode::Rectangle => egui::Key::Num2,
            SlabMode::FillRoom => egui::Key::Num3,
            SlabMode::FromSketch => egui::Key::Num4,
        }
    }

    /// [`SlabMode::hint`] for the tool drawing: a room or a ceiling is
    /// placed to the walls' faces, not their centre lines.
    pub fn hint_for(self, tool: crate::planview::Tool) -> &'static str {
        use crate::planview::Tool;
        match (tool, self) {
            (Tool::Room, SlabMode::FillRoom) => "Click inside walls: the room fills the space to their inner faces and follows them when they move.",
            (Tool::Room, SlabMode::FromSketch) => "Click inside a closed sketch lying flat: it becomes a room with its outline, and the sketch is used up.",
            (Tool::Ceiling, SlabMode::FillRoom) => "Click inside a room: the ceiling goes over it and follows it.",
            (Tool::Ceiling, SlabMode::FromSketch) => "Click inside a closed sketch lying flat: it becomes a ceiling with its outline, and the sketch is used up.",
            _ => self.hint(),
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            SlabMode::Polygon => "Click the slab's corners; click the first one again, or press Enter, to close it. A draws arcs (click the end, then a point on the arc), L lines. Esc or right-click takes back a point.",
            SlabMode::Rectangle => "Click two opposite corners.",
            SlabMode::FillRoom => "Click inside a room: the slab fills it to the centre lines of the walls around it.",
            SlabMode::FromSketch => "Click inside a closed sketch lying flat: it becomes a slab with its outline, and the sketch is used up.",
        }
    }
}

fn top_bar_id() -> egui::Id {
    egui::Id::new("tool options in the top bar")
}

fn pick_id() -> egui::Id {
    egui::Id::new("slab mode picked")
}

/// Tool options are shown in the top bar's own row (the web interface)
/// rather than over the views; set each frame by the app.
pub fn set_options_in_top_bar(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(top_bar_id(), on));
}

pub fn options_in_top_bar(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp(top_bar_id())).unwrap_or(false)
}

/// A mode, or Line / Arc, picked in the top bar, for the next frame.
pub fn pick(ctx: &egui::Context, mode: Option<SlabMode>, arc: Option<bool>) {
    ctx.data_mut(|d| d.insert_temp(pick_id(), (mode, arc)));
}

/// The mode bar (and Line / Arc while clicking a polygon) in a pane's
/// top-left corner, and the 1–4, A and L keys. `modes` is false where only
/// Line / Arc apply (redrawing a slab outline). Choosing From sketch with a
/// flat closed sketch selected makes the slab at once. Returns a message
/// for the status bar when something changed.
pub fn controls(
    ui: &egui::Ui,
    rect: egui::Rect,
    pane: Option<ElementId>,
    modes: bool,
    doc: &mut Document,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Option<String> {
    let ctx = ui.ctx().clone();
    let (mut mode, mut arc) = (state.slab_mode, state.slab_arc);
    let picked: Option<(Option<SlabMode>, Option<bool>)> =
        ctx.data_mut(|d| d.remove_temp(pick_id()));
    if let Some((m, a)) = picked {
        mode = m.filter(|_| modes).unwrap_or(mode);
        arc = a.unwrap_or(arc);
    }
    let lines = !modes || mode == SlabMode::Polygon;
    if !ctx.egui_wants_keyboard_input() && ui.rect_contains_pointer(rect) {
        ctx.input(|i| {
            if i.modifiers.command || i.modifiers.alt {
                return;
            }
            for m in SlabMode::ALL {
                if modes && i.key_pressed(m.key()) {
                    mode = m;
                }
            }
            if lines && i.key_pressed(egui::Key::A) {
                arc = true;
            }
            if lines && i.key_pressed(egui::Key::L) {
                arc = false;
            }
        });
    }
    if !options_in_top_bar(&ctx) {
        egui::Area::new(egui::Id::new(("slab modes", pane)))
            .fixed_pos(rect.left_top() + egui::vec2(8.0, 8.0))
            .order(egui::Order::Foreground)
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if modes {
                            for m in SlabMode::ALL {
                                if ui.selectable_label(mode == m, m.label()).clicked() {
                                    mode = m;
                                }
                            }
                        }
                        if mode == SlabMode::Polygon || !modes {
                            if modes {
                                ui.separator();
                            }
                            if ui.selectable_label(!arc, "Line (L)").clicked() {
                                arc = false;
                            }
                            if ui.selectable_label(arc, "Arc (A)").clicked() {
                                arc = true;
                            }
                        }
                    });
                });
            });
    }
    let mut msg = None;
    if arc != state.slab_arc {
        state.slab_arc = arc;
        if let Some(o) = state.outline.as_mut() {
            o.arc_end = None;
        }
        msg = Some(if arc {
            "Arc: click its end, then a point it passes through.".to_string()
        } else {
            "Line: click the next corner.".to_string()
        });
    }
    if mode != state.slab_mode {
        state.slab_mode = mode;
        // a half-drawn slab of the old kind is dropped
        state.pending = None;
        if state.outline.as_ref().is_some_and(|o| o.target.is_none()) {
            state.outline = None;
        }
        msg = Some(mode.hint_for(state.tool).to_string());
        if mode == SlabMode::FromSketch {
            if let Some(id) = selection.active {
                if matches!(
                    doc.model().get(id).map(|e| &e.kind),
                    Some(ElementKind::Sketch(_))
                ) {
                    let roof = (state.tool == crate::planview::Tool::Roof)
                        .then_some((state.members.roof_shape, state.members.roof_pitch));
                    msg = Some(match from_sketch(doc, id, selection, roof) {
                        Ok(m) | Err(m) => m,
                    });
                }
            }
        }
    }
    msg
}

/// Make a slab of the closed flat sketch `id`, which is used up.
pub fn from_sketch(
    doc: &mut Document,
    id: ElementId,
    selection: &mut Selection,
    roof: Option<(creabim_core::RoofShape, f64)>,
) -> Result<String, String> {
    let Some(ElementKind::Sketch(sk)) = doc.model().get(id).map(|e| &e.kind) else {
        return Err("That isn't a sketch.".into());
    };
    let slab = creabim_core::slab_from_sketch(doc.model(), sk, NEW_SLAB_THICKNESS)?;
    let e = match roof {
        Some((shape, pitch)) => crate::memberstool::roof_from_slab(&slab, shape, pitch),
        None => Element::new("Slab", ElementKind::Slab(slab)),
    };
    let what = if roof.is_some() { "roof" } else { "slab" };
    let sid = e.id;
    let tx = Transaction::new(format!("{} from sketch", e.name))
        .remove_cascade(doc.model(), id)
        .insert(e);
    doc.commit(tx).map_err(|e| e.to_string())?;
    selection.only(sid);
    Ok(format!(
        "The sketch is now a {what}, on the sketch's plane. Ctrl+Z to undo."
    ))
}

/// The closed flat sketch whose outline is around plan point `p`: one on
/// `level` first, else the nearest in height to it.
fn sketch_around(doc: &Document, level: ElementId, p: DVec2) -> Option<ElementId> {
    let model = doc.model();
    let elevation =
        |l: Option<ElementId>| l.and_then(|l| model.level(l)).map_or(0.0, |l| l.elevation);
    let z = elevation(Some(level));
    model
        .iter()
        .filter_map(|e| {
            let ElementKind::Sketch(sk) = &e.kind else {
                return None;
            };
            if !sk.closed || sk.plane.normal().z.abs() < 0.999 {
                return None;
            }
            let outline: Vec<DVec2> = sk.model_points().iter().map(|q| q.truncate()).collect();
            if !inside(&outline, p) {
                return None;
            }
            let off = (sk.plane.origin.z + elevation(sk.level) - z).abs();
            Some((e.id, sk.level != Some(level), off))
        })
        .min_by(|a, b| {
            (a.1, a.2)
                .partial_cmp(&(b.1, b.2))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(id, _, _)| id)
}

fn inside(poly: &[DVec2], p: DVec2) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

/// A click with the Slab tool on `level` at plan point `p`, in any mode.
pub fn click(
    doc: &mut Document,
    level: ElementId,
    p: DVec2,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), creabim_core::ModelError> {
    match state.slab_mode {
        SlabMode::Polygon => {
            let mut o = crate::outline::Outline::new_slab(doc.model(), level, p);
            if state.tool == crate::planview::Tool::Roof {
                o.roof = Some((state.members.roof_shape, state.members.roof_pitch));
            }
            o.shaft = state.tool == crate::planview::Tool::Shaft;
            o.space = crate::roomtool::Space::of(state.tool);
            let cat = if o.roof.is_some() {
                creabim_core::TypeCategory::Roof
            } else {
                creabim_core::TypeCategory::Slab
            };
            o.make = crate::partstool::make_for(state);
            o.of_type = state
                .types
                .get(&cat)
                .copied()
                .filter(|_| !o.shaft && o.make.is_none());
            state.outline = Some(o);
            state.status = state.outline.as_ref().map(|o| o.hint());
            Ok(())
        }
        SlabMode::Rectangle => match state.pending.take() {
            None => {
                state.pending = Some(p);
                state.status = Some("Click the opposite corner.".into());
                Ok(())
            }
            Some(a) => {
                let (lo, hi) = (a.min(p), a.max(p));
                if (hi - lo).min_element() < 0.01 {
                    return Ok(());
                }
                if let Some(make) = crate::partstool::make_for(state) {
                    let pts = creabim_core::polygon::rectangle(lo, hi);
                    let (msg, id) = crate::partstool::finish_outline(doc, level, pts, &make)?;
                    selection.set(id);
                    state.status = Some(msg);
                    return Ok(());
                }
                let rect = vec![lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)];
                if let Some(space) = crate::roomtool::Space::of(state.tool) {
                    state.status =
                        Some(crate::roomtool::drawn(doc, space, level, rect, selection)?);
                    return Ok(());
                }
                let (e, label) = crate::memberstool::outline_element(
                    doc.model(),
                    state,
                    Slab::new(
                        level,
                        vec![lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)],
                        NEW_SLAB_THICKNESS,
                    ),
                );
                selection.only(e.id);
                state.status = Some(format!("{} added.", e.name));
                doc.commit(Transaction::new(label).insert(e))
            }
        },
        SlabMode::FillRoom => {
            if let Some(space) = crate::roomtool::Space::of(state.tool) {
                state.status = Some(crate::roomtool::fill(doc, space, level, p, selection)?);
                return Ok(());
            }
            let Some(outline) = creabim_core::room_outline(doc.model(), level, p) else {
                state.status = Some(
                    "That spot isn't closed in by walls on this level, so there's no room to fill. Click inside a room, or draw a polygon (1).".into(),
                );
                return Ok(());
            };
            if let Some(make) = crate::partstool::make_for(state) {
                let (msg, id) = crate::partstool::finish_outline(doc, level, outline, &make)?;
                selection.set(id);
                state.status = Some(msg);
                return Ok(());
            }
            let (e, label) = crate::memberstool::outline_element(
                doc.model(),
                state,
                Slab::new(level, outline, NEW_SLAB_THICKNESS),
            );
            selection.only(e.id);
            state.status = Some(format!(
                "{} added over the room, to the walls' centre lines.",
                e.name
            ));
            doc.commit(Transaction::new(label).insert(e))
        }
        SlabMode::FromSketch => {
            let Some(id) = sketch_around(doc, level, p) else {
                state.status = Some(
                    "Click inside a closed sketch lying flat (draw one with the Sketch tool)."
                        .into(),
                );
                return Ok(());
            };
            if let Some(make) = crate::partstool::make_for(state) {
                let Some(ElementKind::Sketch(sk)) = doc.model().get(id).map(|e| &e.kind) else {
                    return Ok(());
                };
                let slab = match creabim_core::slab_from_sketch(doc.model(), sk, NEW_SLAB_THICKNESS)
                {
                    Ok(s) => s,
                    Err(m) => {
                        state.status = Some(m);
                        return Ok(());
                    }
                };
                let (elements, label, msg) =
                    crate::partstool::from_outline(doc.model(), slab.level, slab.outline(), &make);
                selection.set(elements.last().map(|e| e.id));
                state.status = Some(msg);
                let mut tx = Transaction::new(label).remove_cascade(doc.model(), id);
                for e in elements {
                    tx = tx.insert(e);
                }
                return doc.commit(tx);
            }
            if let Some(space) = crate::roomtool::Space::of(state.tool) {
                let Some(ElementKind::Sketch(sk)) = doc.model().get(id).map(|e| &e.kind) else {
                    return Ok(());
                };
                match creabim_core::slab_from_sketch(doc.model(), sk, NEW_SLAB_THICKNESS) {
                    Ok(slab) => {
                        state.status = Some(crate::roomtool::drawn(
                            doc,
                            space,
                            slab.level,
                            slab.outline(),
                            selection,
                        )?);
                    }
                    Err(m) => state.status = Some(m),
                }
                return Ok(());
            }
            if state.tool == crate::planview::Tool::Shaft {
                let Some(ElementKind::Sketch(sk)) = doc.model().get(id).map(|e| &e.kind) else {
                    return Ok(());
                };
                let slab = match creabim_core::slab_from_sketch(doc.model(), sk, NEW_SLAB_THICKNESS)
                {
                    Ok(s) => s,
                    Err(m) => {
                        state.status = Some(m);
                        return Ok(());
                    }
                };
                let e = crate::outline::shaft_from_outline(doc.model(), slab.level, slab.outline());
                selection.only(e.id);
                state.status = Some("The sketch is now a shaft. Ctrl+Z to undo.".into());
                let tx = Transaction::new("Shaft from sketch")
                    .remove_cascade(doc.model(), id)
                    .insert(e);
                return doc.commit(tx);
            }
            let roof = (state.tool == crate::planview::Tool::Roof)
                .then_some((state.members.roof_shape, state.members.roof_pitch));
            state.status = Some(match from_sketch(doc, id, selection, roof) {
                Ok(m) | Err(m) => m,
            });
            Ok(())
        }
    }
}
