//! The Column, Beam, Stair and Railing tools (the Roof tool draws its
//! outline with the Slab tool's modes), their settings for new elements,
//! and the Properties panel for every element they make.

use crate::planview::{PlanState, Tool};
use crate::selection::Selection;
use creabim_core::members::{self, Landing, Profile, RoofShape};
use creabim_core::{
    Beam, Column, Document, Element, ElementId, ElementKind, Model, ModelError, Railing, Roof,
    Slab, Stair, Transaction,
};
use eframe::egui::{self, Color32, Pos2, Stroke};
use glam::{DVec2, DVec3};

/// Sizes and shapes for new elements, set in the tool bar.
#[derive(Clone, Debug)]
pub struct MemberSettings {
    pub column: Profile,
    pub beam_width: f64,
    pub beam_depth: f64,
    pub roof_shape: RoofShape,
    pub roof_pitch: f64,
    pub stair_width: f64,
    /// New stairs turn on a landing halfway up.
    pub stair_turn: bool,
    pub railing_height: f64,
    /// The railing path clicked so far.
    pub path: Vec<DVec2>,
}

impl Default for MemberSettings {
    fn default() -> Self {
        Self {
            column: Profile::Rectangle {
                width: 0.3,
                depth: 0.3,
            },
            beam_width: 0.2,
            beam_depth: 0.4,
            roof_shape: RoofShape::Hip,
            roof_pitch: 30.0,
            stair_width: 1.0,
            stair_turn: false,
            railing_height: 1.0,
            path: vec![],
        }
    }
}

impl MemberSettings {
    /// Width (or diameter) of new columns.
    pub fn column_size(&self) -> f64 {
        match self.column {
            Profile::Rectangle { width, .. } => width,
            Profile::Round { diameter } => diameter,
        }
    }

    pub fn set_column_size(&mut self, v: f64) {
        self.column = match self.column {
            Profile::Rectangle { .. } => Profile::Rectangle { width: v, depth: v },
            Profile::Round { .. } => Profile::Round { diameter: v },
        };
    }

    pub fn round_columns(&self) -> bool {
        matches!(self.column, Profile::Round { .. })
    }

    pub fn set_round(&mut self, round: bool) {
        let v = self.column_size();
        self.column = if round {
            Profile::Round { diameter: v }
        } else {
            Profile::Rectangle { width: v, depth: v }
        };
    }
}

/// A roof from an outline drawn with the Slab tool's modes.
pub fn roof_from_slab(slab: &Slab, shape: RoofShape, pitch: f64) -> Element {
    let mut r = Roof::new(slab.level, slab.outline(), shape);
    r.pitch = pitch;
    r.base_offset = slab.top_offset;
    Element::new("Roof", ElementKind::Roof(r))
}

/// What the Slab tool's modes make: a slab, or with the Roof tool a roof.
pub fn outline_element(
    model: &creabim_core::Model,
    state: &PlanState,
    slab: Slab,
) -> (Element, &'static str) {
    if state.tool == Tool::Roof {
        let m = &state.members;
        (
            state.typed(model, roof_from_slab(&slab, m.roof_shape, m.roof_pitch)),
            "Draw roof",
        )
    } else if state.tool == Tool::Shaft {
        (
            crate::outline::shaft_from_outline(model, slab.level, slab.outline()),
            "Draw shaft",
        )
    } else {
        (
            state.typed(model, Element::new("Slab", ElementKind::Slab(slab))),
            "Draw slab",
        )
    }
}

/// A click with the Column, Beam, Stair or Railing tool on `level`.
pub fn click(
    doc: &mut Document,
    level: ElementId,
    tool: Tool,
    p: DVec2,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), ModelError> {
    let set = state.members.clone();
    let add = |doc: &mut Document, selection: &mut Selection, e: Element, label: &str| {
        selection.only(e.id);
        doc.commit(Transaction::new(label).insert(e))
    };
    match tool {
        Tool::Column => {
            let c = Column::new(level, p, set.column);
            state.status = Some(format!(
                "Column {} placed. Its height runs to the next level; drag its top handle or set a height in Properties.",
                set.column.label()
            ));
            let e = state.typed(doc.model(), Element::new("Column", ElementKind::Column(c)));
            add(doc, selection, e, "Place column")
        }
        Tool::Beam => match state.pending {
            None => {
                state.pending = Some(p);
                state.status = Some("Click the beam's other end.".into());
                Ok(())
            }
            Some(a) if a.distance(p) > 0.05 => {
                let mut b = Beam::new(level, a, p);
                b.width = set.beam_width;
                b.depth = set.beam_depth;
                state.pending = Some(p);
                state.status =
                    Some("Beam added. Click on to chain another; Esc or right-click ends.".into());
                let e = state.typed(doc.model(), Element::new("Beam", ElementKind::Beam(b)));
                add(doc, selection, e, "Draw beam")
            }
            Some(_) => Ok(()),
        },
        Tool::Stair => match state.pending.take() {
            None => {
                state.pending = Some(p);
                state.status = Some("Click the way the stair climbs.".into());
                Ok(())
            }
            Some(a) if a.distance(p) > 0.05 => {
                let s = new_stair(doc.model(), level, a, p, &set);
                let lay = s.layout(s.rise(doc.model()));
                state.status = Some(format!(
                    "Stair added: {} risers of {}. Railing tool: click inside it to add its railings.",
                    lay.risers,
                    crate::units::len(lay.riser)
                ));
                add(
                    doc,
                    selection,
                    Element::new("Stair", ElementKind::Stair(s)),
                    "Draw stair",
                )
            }
            Some(_) => Ok(()),
        },
        Tool::Railing => {
            if state.members.path.is_empty() {
                if let Some(stair) = stair_at(doc.model(), level, p) {
                    state.status = Some("Railings added along both sides of the stair.".into());
                    return add_stair_railings(doc, stair, set.railing_height, selection);
                }
            }
            let path = &mut state.members.path;
            if path.last().is_some_and(|q| q.distance(p) < 0.05) {
                // clicking the last point again finishes
                let pts = std::mem::take(path);
                state.pending = None;
                if pts.len() < 2 {
                    return Ok(());
                }
                let mut r = Railing::new(level, pts.iter().map(|q| q.extend(0.0)).collect());
                r.height = set.railing_height;
                state.status = Some("Railing added.".into());
                return add(
                    doc,
                    selection,
                    Element::new("Railing", ElementKind::Railing(r)),
                    "Draw railing",
                );
            }
            path.push(p);
            state.pending = Some(p);
            state.status = Some(
                "Click the next point; click the last one again to finish. Esc cancels.".into(),
            );
            Ok(())
        }
        _ => Ok(()),
    }
}

/// A stair from `a` climbing towards `b`, sized from the tool's settings.
pub fn new_stair(
    model: &Model,
    level: ElementId,
    a: DVec2,
    b: DVec2,
    set: &MemberSettings,
) -> Stair {
    let mut s = Stair::new(level, a, (b - a).to_angle().to_degrees());
    s.width = set.stair_width;
    if set.stair_turn {
        let n = members::riser_count(s.rise(model), s.max_riser);
        s.landing = Some(Landing {
            after: n / 2,
            left: true,
        });
    }
    s
}

/// The stair on `level` whose footprint holds `p`.
fn stair_at(model: &Model, level: ElementId, p: DVec2) -> Option<ElementId> {
    model.iter().find_map(|e| match &e.kind {
        ElementKind::Stair(s) if s.level == level => {
            let rise = s.rise(model);
            let mut shapes = s.flight_outlines(rise);
            if let Some((l, _)) = s.layout(rise).landing {
                shapes.push(l);
            }
            shapes.iter().any(|o| inside(o, p)).then_some(e.id)
        }
        _ => None,
    })
}

fn inside(poly: &[DVec2], p: DVec2) -> bool {
    let n = poly.len();
    let mut c = false;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            c = !c;
        }
    }
    c
}

/// Railings on both sides of `stair`, as one undo step.
pub fn add_stair_railings(
    doc: &mut Document,
    stair: ElementId,
    height: f64,
    selection: &mut Selection,
) -> Result<(), ModelError> {
    let mut tx = Transaction::new("Add stair railings");
    let mut first = None;
    for left in [true, false] {
        if let Some(mut r) = Railing::along_stair(doc.model(), stair, left) {
            r.height = height;
            let e = Element::new("Railing", ElementKind::Railing(r));
            first.get_or_insert(e.id);
            tx = tx.insert(e);
        }
    }
    if let Some(id) = first {
        selection.only(id);
    }
    doc.commit(tx)
}

/// What the tool would make, drawn while its second point is chosen:
/// a beam's width, a stair's flights, a railing's path so far.
#[allow(clippy::too_many_arguments)]
pub fn preview(
    painter: &egui::Painter,
    project: &dyn Fn(DVec3) -> Option<Pos2>,
    model: &Model,
    level: Option<ElementId>,
    tool: Tool,
    state: &PlanState,
    a: DVec3,
    b: DVec3,
) {
    let stroke = Stroke::new(1.0, Color32::from_rgb(230, 120, 30));
    let z = a.z;
    let ring = |pts: &[DVec2]| {
        let scr: Option<Vec<Pos2>> = pts
            .iter()
            .chain(pts.first())
            .map(|p| project(p.extend(z)))
            .collect();
        if let Some(scr) = scr {
            painter.add(egui::Shape::line(scr, stroke));
        }
    };
    let set = &state.members;
    match (tool, level) {
        (Tool::Beam, Some(level)) if a.distance(b) > 1e-3 => {
            let mut bm = Beam::new(level, a.truncate(), b.truncate());
            bm.width = set.beam_width;
            ring(&bm.outline());
        }
        (Tool::Stair, Some(level)) if a.distance(b) > 1e-3 => {
            let s = new_stair(model, level, a.truncate(), b.truncate(), set);
            let rise = s.rise(model);
            for o in s.flight_outlines(rise) {
                ring(&o);
            }
            if let Some((l, _)) = s.layout(rise).landing {
                ring(&l);
            }
        }
        (Tool::Railing, _) => {
            let scr: Option<Vec<Pos2>> = set.path.iter().map(|p| project(p.extend(z))).collect();
            if let Some(scr) = scr {
                painter.add(egui::Shape::line(scr, Stroke::new(2.0, stroke.color)));
            }
        }
        _ => {}
    }
}

/// The Properties panel rows for a column, beam, roof, stair or railing.
/// Returns the undo label when something changed.
pub fn properties(ui: &mut egui::Ui, model: &Model, kind: &mut ElementKind) -> &'static str {
    let mut label = "";
    let num = |ui: &mut egui::Ui, text: &str, v: &mut f64, lo: f64, hi: f64| -> bool {
        ui.horizontal(|ui| {
            ui.label(text);
            ui.add(crate::units::drag(v).range(lo..=hi)).changed()
        })
        .inner
    };
    let deg = |ui: &mut egui::Ui, text: &str, v: &mut f64, lo: f64, hi: f64| -> bool {
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
    let weak = |ui: &mut egui::Ui, t: String| {
        ui.label(egui::RichText::new(t).weak());
    };
    match kind {
        ElementKind::Column(c) => {
            let mut round = matches!(c.profile, Profile::Round { .. });
            ui.horizontal(|ui| {
                ui.label("Profile");
                let a = ui.selectable_value(&mut round, false, "Rectangular");
                let b = ui.selectable_value(&mut round, true, "Round");
                if a.changed() || b.changed() {
                    label = "Column profile";
                }
            });
            if label == "Column profile" {
                c.profile = match (round, c.profile) {
                    (true, Profile::Rectangle { width, .. }) => Profile::Round { diameter: width },
                    (false, Profile::Round { diameter }) => Profile::Rectangle {
                        width: diameter,
                        depth: diameter,
                    },
                    (_, p) => p,
                };
            }
            match &mut c.profile {
                Profile::Rectangle { width, depth } => {
                    if num(ui, "Width", width, 0.05, 5.0) | num(ui, "Depth", depth, 0.05, 5.0) {
                        label = "Column size"
                    }
                    if deg(ui, "Rotation", &mut c.rotation, -360.0, 360.0) {
                        label = "Turn column"
                    }
                }
                Profile::Round { diameter } => {
                    if num(ui, "Diameter", diameter, 0.05, 5.0) {
                        label = "Column size"
                    }
                }
            }
            let mut fixed = c.height.is_some();
            if ui
                .checkbox(&mut fixed, "Fixed height (else up to the next level)")
                .changed()
            {
                c.height = fixed.then(|| c.height_in(model));
                label = "Column height";
            }
            if let Some(h) = &mut c.height {
                if num(ui, "Height", h, 0.1, 100.0) {
                    label = "Column height"
                }
            }
            if num(ui, "Base offset", &mut c.base_offset, -10.0, 50.0) {
                label = "Column base offset"
            }
            if num(ui, "X", &mut c.position.x, -1e4, 1e4)
                | num(ui, "Y", &mut c.position.y, -1e4, 1e4)
            {
                label = "Move column"
            }
            let (z0, z1) = c.span(model);
            weak(
                ui,
                format!(
                    "Base at {}, top at {}.",
                    crate::units::elevation(z0),
                    crate::units::elevation(z1)
                ),
            );
        }
        ElementKind::Beam(b) => {
            if num(ui, "Width", &mut b.width, 0.02, 3.0) {
                label = "Beam width"
            }
            if num(ui, "Depth", &mut b.depth, 0.02, 5.0) {
                label = "Beam depth"
            }
            if num(ui, "Below storey top", &mut b.top_offset, -10.0, 50.0) {
                label = "Beam top offset"
            }
            if num(ui, "Start X", &mut b.start.x, -1e4, 1e4)
                | num(ui, "Start Y", &mut b.start.y, -1e4, 1e4)
                | num(ui, "End X", &mut b.end.x, -1e4, 1e4)
                | num(ui, "End Y", &mut b.end.y, -1e4, 1e4)
            {
                label = "Move beam"
            }
            weak(
                ui,
                format!(
                    "Length {}, top at {}. Plans show it dashed above the cut.",
                    crate::units::len(b.length()),
                    crate::units::elevation(b.top(model))
                ),
            );
        }
        ElementKind::Roof(r) => {
            ui.horizontal_wrapped(|ui| {
                ui.label("Shape");
                for s in RoofShape::ALL {
                    if ui.selectable_value(&mut r.shape, s, s.label()).changed() {
                        label = "Roof shape"
                    }
                }
            });
            if r.shape != RoofShape::Flat && deg(ui, "Pitch", &mut r.pitch, 1.0, 75.0) {
                label = "Roof pitch"
            }
            if num(ui, "Overhang", &mut r.overhang, 0.0, 5.0) {
                label = "Roof overhang"
            }
            if num(ui, "Thickness", &mut r.thickness, 0.02, 2.0) {
                label = "Roof thickness"
            }
            if num(ui, "Base offset", &mut r.base_offset, -10.0, 100.0) {
                label = "Roof base offset"
            }
            let n = r.boundary.len();
            if matches!(r.shape, RoofShape::Shed | RoofShape::Gable) && n > 0 {
                ui.horizontal(|ui| {
                    ui.label(if r.shape == RoofShape::Shed {
                        "Low edge"
                    } else {
                        "Eave edge"
                    });
                    if ui.button("Next edge").clicked() {
                        r.edge = (r.edge + 1) % n;
                        label = "Roof edge";
                    }
                    ui.label(format!("{} of {n}", r.edge % n + 1));
                });
            }
            if r.effective_shape() != r.shape {
                weak(
                    ui,
                    "Gable and hip roofs need a convex outline; this one is built flat.".into(),
                );
            }
            weak(
                ui,
                format!(
                    "{} corners, {} with the overhang, {} high.",
                    n,
                    crate::units::area(r.plan_area()),
                    crate::units::len(r.peak())
                ),
            );
        }
        ElementKind::Stair(s) => {
            if num(ui, "Width", &mut s.width, 0.5, 5.0) {
                label = "Stair width"
            }
            if num(ui, "Max riser", &mut s.max_riser, 0.1, 0.3) {
                label = "Stair riser"
            }
            if num(ui, "Tread", &mut s.tread, 0.15, 0.6) {
                label = "Stair tread"
            }
            if num(ui, "Waist", &mut s.waist, 0.05, 0.6) {
                label = "Stair waist"
            }
            if deg(ui, "Direction", &mut s.direction, -360.0, 360.0) {
                label = "Turn stair"
            }
            let mut fixed = s.height.is_some();
            if ui
                .checkbox(&mut fixed, "Fixed rise (else up to the next level)")
                .changed()
            {
                s.height = fixed.then(|| s.rise(model));
                label = "Stair rise";
            }
            if let Some(h) = &mut s.height {
                if num(ui, "Rise", h, 0.1, 20.0) {
                    label = "Stair rise"
                }
            }
            let rise = s.rise(model);
            let n = members::riser_count(rise, s.max_riser);
            let mut turn = s.landing.is_some();
            if ui.checkbox(&mut turn, "L-shaped, with a landing").changed() {
                s.landing = turn.then_some(Landing {
                    after: n / 2,
                    left: true,
                });
                label = "Stair landing";
            }
            if let Some(l) = &mut s.landing {
                ui.horizontal(|ui| {
                    ui.label("Landing after");
                    if ui
                        .add(
                            egui::DragValue::new(&mut l.after)
                                .range(1..=n.saturating_sub(1).max(1))
                                .suffix(" risers"),
                        )
                        .changed()
                    {
                        label = "Stair landing";
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Turns");
                    let a = ui.selectable_value(&mut l.left, true, "Left");
                    let b = ui.selectable_value(&mut l.left, false, "Right");
                    if a.changed() || b.changed() {
                        label = "Stair turn";
                    }
                });
            }
            let lay = s.layout(rise);
            let going: f64 = lay.flights.iter().map(|f| f.run(s.tread)).sum();
            let r2g = 2.0 * lay.riser + s.tread;
            weak(
                ui,
                format!(
                    "{} risers of {}, rise {}, going {}. 2R + G = {}{}.",
                    lay.risers,
                    crate::units::len(lay.riser),
                    crate::units::len(rise),
                    crate::units::len(going),
                    crate::units::len(r2g),
                    if (0.55..=0.70).contains(&r2g) {
                        String::new()
                    } else {
                        format!(
                            " (outside the usual {}–{})",
                            crate::units::len(0.55),
                            crate::units::len(0.70)
                        )
                    }
                ),
            );
        }
        ElementKind::Railing(r) => {
            if num(ui, "Height", &mut r.height, 0.2, 3.0) {
                label = "Railing height"
            }
            if num(ui, "Post spacing", &mut r.post_spacing, 0.1, 5.0) {
                label = "Railing posts"
            }
            if let Some(side) = &mut r.stair {
                ui.horizontal(|ui| {
                    ui.label("Side");
                    let a = ui.selectable_value(&mut side.left, true, "Left");
                    let b = ui.selectable_value(&mut side.left, false, "Right");
                    if a.changed() || b.changed() {
                        label = "Railing side";
                    }
                });
                weak(ui, "Follows its stair.".into());
            }
            weak(
                ui,
                format!(
                    "Length {}, {} posts.",
                    crate::units::len(r.length(model)),
                    r.posts(model).len()
                ),
            );
        }
        _ => {}
    }
    label
}

/// The tool bar's settings for the active tool: sizes as numbers, shapes
/// as toggles (`member:<what>` actions).
pub fn tool_options(state: &PlanState) -> Vec<serde_json::Value> {
    use serde_json::json;
    let m = &state.members;
    let num = |key: &str,
               label: &str,
               value: f64,
               unit: &str,
               step: f64,
               min: f64,
               max: f64,
               hint: &str| {
        json!({
            "kind": "number", "key": format!("member.{key}"), "label": label, "value": value,
            "unit": unit, "step": step, "min": min, "max": max, "hint": hint,
        })
    };
    let toggle = |id: &str, label: &str, on: bool, icon: &str, hint: &str| {
        json!({
            "kind": "toggle", "id": format!("member:{id}"), "label": label, "on": on,
            "icon": icon, "hint": hint,
        })
    };
    let mut out = match state.tool {
        Tool::Column => vec![
            toggle(
                "square",
                "Square",
                !m.round_columns(),
                "square-dashed",
                "Rectangular columns",
            ),
            toggle(
                "round",
                "Round",
                m.round_columns(),
                "circle-dot",
                "Round columns",
            ),
            num(
                "column",
                if m.round_columns() {
                    "Diameter"
                } else {
                    "Size"
                },
                m.column_size(),
                "m",
                0.01,
                0.05,
                3.0,
                "Size of new columns",
            ),
        ],
        Tool::Beam => vec![
            num(
                "beam_width",
                "Width",
                m.beam_width,
                "m",
                0.01,
                0.05,
                2.0,
                "Width of new beams",
            ),
            num(
                "beam_depth",
                "Depth",
                m.beam_depth,
                "m",
                0.01,
                0.05,
                3.0,
                "Depth of new beams",
            ),
        ],
        Tool::Roof => {
            let mut v: Vec<serde_json::Value> = RoofShape::ALL
                .iter()
                .map(|s| {
                    toggle(
                        &format!("roof.{}", s.label()),
                        s.label(),
                        m.roof_shape == *s,
                        "tent",
                        "Shape of new roofs",
                    )
                })
                .collect();
            if m.roof_shape != RoofShape::Flat {
                v.push(num(
                    "pitch",
                    "Pitch",
                    m.roof_pitch,
                    "°",
                    1.0,
                    1.0,
                    75.0,
                    "Slope of new roofs",
                ));
            }
            v
        }
        Tool::Stair => vec![
            num(
                "stair_width",
                "Width",
                m.stair_width,
                "m",
                0.05,
                0.5,
                5.0,
                "Width of new stairs",
            ),
            toggle(
                "lshape",
                "L-shaped",
                m.stair_turn,
                "corner-down-right",
                "New stairs turn left on a landing halfway up",
            ),
        ],
        Tool::Railing => vec![num(
            "railing_height",
            "Height",
            m.railing_height,
            "m",
            0.05,
            0.3,
            3.0,
            "Height of new railings",
        )],
        _ => vec![],
    };
    if !out.is_empty() {
        out.push(json!({"kind": "sep"}));
    }
    out
}

/// A number typed in the tool bar.
pub fn set_option(state: &mut PlanState, key: &str, v: f64) {
    let m = &mut state.members;
    match key.trim_start_matches("member.") {
        "column" => m.set_column_size(v.clamp(0.05, 3.0)),
        "beam_width" => m.beam_width = v.clamp(0.05, 2.0),
        "beam_depth" => m.beam_depth = v.clamp(0.05, 3.0),
        "pitch" => m.roof_pitch = v.clamp(1.0, 75.0),
        "stair_width" => m.stair_width = v.clamp(0.5, 5.0),
        "railing_height" => m.railing_height = v.clamp(0.3, 3.0),
        _ => {}
    }
}

/// A toggle clicked in the tool bar.
pub fn toggle_option(state: &mut PlanState, what: &str) {
    let m = &mut state.members;
    match what {
        "square" => m.set_round(false),
        "round" => m.set_round(true),
        "lshape" => m.stair_turn = !m.stair_turn,
        w => {
            if let Some(s) = RoofShape::ALL
                .into_iter()
                .find(|s| w == format!("roof.{}", s.label()))
            {
                m.roof_shape = s;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    fn setup() -> (Document, ElementId, PlanState, Selection) {
        let doc = Document::from_model(sample_house());
        let level = doc
            .model()
            .levels()
            .min_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation))
            .unwrap()
            .0
            .id;
        (doc, level, PlanState::default(), Selection::default())
    }

    fn count(doc: &Document, f: fn(&ElementKind) -> bool) -> usize {
        doc.model().iter().filter(|e| f(&e.kind)).count()
    }

    fn active<'a>(doc: &'a Document, sel: &Selection) -> Option<&'a ElementKind> {
        sel.active
            .and_then(|id| doc.model().get(id))
            .map(|e| &e.kind)
    }

    #[test]
    fn tools_place_each_member_and_undo() {
        let (mut doc, level, mut st, mut sel) = setup();
        let mut go = |doc: &mut Document, st: &mut PlanState, tool, x, y| {
            click(doc, level, tool, DVec2::new(x, y), st, &mut sel).unwrap();
        };
        go(&mut doc, &mut st, Tool::Column, 1.0, 1.0);
        assert_eq!(count(&doc, |k| matches!(k, ElementKind::Column(_))), 1);
        for (x, y) in [(0.0, 0.0), (4.0, 0.0), (4.0, 3.0)] {
            go(&mut doc, &mut st, Tool::Beam, x, y);
        }
        assert_eq!(count(&doc, |k| matches!(k, ElementKind::Beam(_))), 2);
        st.cancel();
        st.members.stair_turn = true;
        go(&mut doc, &mut st, Tool::Stair, 20.0, 0.0);
        go(&mut doc, &mut st, Tool::Stair, 21.0, 0.0);
        assert_eq!(
            count(
                &doc,
                |k| matches!(k, ElementKind::Stair(s) if s.landing.is_some())
            ),
            1
        );
        // a click on the stair rails both its sides
        go(&mut doc, &mut st, Tool::Railing, 20.5, 0.0);
        assert_eq!(count(&doc, |k| matches!(k, ElementKind::Railing(_))), 2);
        // a free path: click the last point again to finish
        for (x, y) in [(30.0, 0.0), (33.0, 0.0), (33.0, 0.0)] {
            go(&mut doc, &mut st, Tool::Railing, x, y);
        }
        assert_eq!(count(&doc, |k| matches!(k, ElementKind::Railing(_))), 3);
        assert!(st.members.path.is_empty());
        doc.undo();
        assert_eq!(count(&doc, |k| matches!(k, ElementKind::Railing(_))), 2);
    }

    #[test]
    fn the_roof_tool_draws_rectangles_and_outlines() {
        let (mut doc, level, mut st, mut sel) = setup();
        st.tool = Tool::Roof;
        st.members.roof_shape = RoofShape::Gable;
        st.slab_mode = crate::slabtool::SlabMode::Rectangle;
        crate::slabtool::click(&mut doc, level, DVec2::ZERO, &mut st, &mut sel).unwrap();
        crate::slabtool::click(&mut doc, level, DVec2::new(8.0, 5.0), &mut st, &mut sel).unwrap();
        assert!(matches!(
            active(&doc, &sel),
            Some(ElementKind::Roof(r)) if r.shape == RoofShape::Gable
        ));
        st.slab_mode = crate::slabtool::SlabMode::Polygon;
        crate::slabtool::click(&mut doc, level, DVec2::new(20.0, 0.0), &mut st, &mut sel).unwrap();
        let mut o = st.outline.take().unwrap();
        assert!(o.roof.is_some());
        for (x, y) in [(24.0, 0.0), (24.0, 4.0), (20.0, 4.0)] {
            o.click(DVec2::new(x, y), false, false);
        }
        o.click(DVec2::new(20.0, 0.0), true, false);
        let (_, id) = o.finish(&mut doc, true).unwrap();
        assert!(matches!(
            doc.model().get(id.unwrap()).map(|e| &e.kind),
            Some(ElementKind::Roof(_))
        ));
    }
}
