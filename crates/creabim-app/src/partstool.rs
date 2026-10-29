//! The Extrusion, Sweep and Void tools, and the site tools (Terrain, Hard
//! landscape, Planting): their settings for new elements, what a click or
//! a drawn outline makes, and the Properties panel for every element they
//! make (see `creabim_core::parts` and `creabim_core::site`).
//!
//! The outline tools draw with the Slab tool's modes (polygon, rectangle,
//! fill room, from sketch). Sizes, patterns and profiles are only starting
//! values: every one is the element's own afterwards.

use crate::planview::{PlanState, Tool};
use crate::selection::Selection;
use creabim_core::parts::{self, Form, Part, Path, WallEdge};
use creabim_core::site::{Pattern, PlantForm, Site, SiteForm};
use creabim_core::{Document, Element, ElementId, ElementKind, Model, ModelError, Transaction};
use eframe::egui;
use glam::{DVec2, DVec3};

/// A profile the Sweep tool starts from. The sizes are the tool bar's two
/// numbers: what they mean depends on the profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SweepProfile {
    /// Over a wall's top, proud of both faces.
    Coping,
    /// Along a wall's base, proud of its exterior face.
    Plinth,
    /// A plain rectangle: a kerb, a bar, a rail.
    Bar,
    /// A flight of steps, climbing to the path's left.
    Steps,
}

impl SweepProfile {
    pub const ALL: [SweepProfile; 4] = [
        SweepProfile::Coping,
        SweepProfile::Plinth,
        SweepProfile::Bar,
        SweepProfile::Steps,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SweepProfile::Coping => "Coping",
            SweepProfile::Plinth => "Plinth",
            SweepProfile::Bar => "Bar",
            SweepProfile::Steps => "Steps",
        }
    }

    /// The labels of the tool bar's two sizes.
    fn sizes(self) -> (&'static str, &'static str) {
        match self {
            SweepProfile::Coping => ("Overhang", "Depth"),
            SweepProfile::Plinth => ("Proud", "Height"),
            SweepProfile::Bar => ("Width", "Height"),
            SweepProfile::Steps => ("Going", "Rise"),
        }
    }

    /// Its usual sizes.
    fn defaults(self) -> (f64, f64) {
        match self {
            SweepProfile::Coping => (0.04, 0.06),
            SweepProfile::Plinth => (0.03, 0.3),
            SweepProfile::Bar => (0.15, 0.15),
            SweepProfile::Steps => (0.3, 0.17),
        }
    }

    /// The wall edge it follows when placed on a wall.
    fn edge(self) -> WallEdge {
        match self {
            SweepProfile::Plinth | SweepProfile::Steps => WallEdge::Base,
            SweepProfile::Coping | SweepProfile::Bar => WallEdge::Top,
        }
    }

    /// The profile, for a wall `wall` thick.
    fn profile(self, wall: f64, a: f64, b: f64, steps: u32) -> Vec<DVec2> {
        match self {
            SweepProfile::Coping => parts::coping(wall, a, b),
            SweepProfile::Plinth => parts::plinth(wall, a, b, false),
            SweepProfile::Bar => parts::rectangle(a, b),
            SweepProfile::Steps => parts::steps(steps, b, a),
        }
    }
}

/// A hard landscape area the tool starts from: its pattern, depth and
/// material from the library.
pub struct AreaPreset {
    pub label: &'static str,
    pub pattern: Pattern,
    pub thickness: f64,
    pub material: &'static str,
}

pub const AREAS: &[AreaPreset] = &[
    AreaPreset {
        label: "Paving",
        pattern: Pattern::Units {
            size: DVec2::new(0.6, 0.4),
            stagger: true,
            angle: 0.0,
        },
        thickness: 0.1,
        material: "Paving, concrete flags",
    },
    AreaPreset {
        label: "Driveway",
        pattern: Pattern::Units {
            size: DVec2::new(0.2, 0.1),
            stagger: true,
            angle: 90.0,
        },
        thickness: 0.15,
        material: "Block paving",
    },
    AreaPreset {
        label: "Gravel",
        pattern: Pattern::Stipple { spacing: 0.12 },
        thickness: 0.1,
        material: "Gravel",
    },
    AreaPreset {
        label: "Decking",
        pattern: Pattern::Lines {
            spacing: 0.145,
            angle: 0.0,
        },
        thickness: 0.05,
        material: "Timber decking",
    },
    AreaPreset {
        label: "Asphalt",
        pattern: Pattern::None,
        thickness: 0.1,
        material: "Asphalt",
    },
];

/// The IFC classes offered for an extrusion or a sweep.
pub const IFC_CLASSES: &[&str] = &[
    parts::PROXY,
    "IfcChimney",
    "IfcCovering",
    "IfcMember",
    "IfcPlate",
    "IfcSlab",
    "IfcStair",
    "IfcRamp",
    "IfcColumn",
    "IfcBeam",
    "IfcWall",
    "IfcFooting",
    "IfcShadingDevice",
    "IfcFurniture",
    "IfcCivilElement",
    "IfcBuildingElementPart",
];

/// Sizes for new parts and site elements, set in the tool bar.
#[derive(Clone, Debug)]
pub struct PartSettings {
    pub extrusion_base: f64,
    pub extrusion_height: f64,
    pub void_base: f64,
    pub void_height: f64,
    pub profile: SweepProfile,
    /// The profile's two sizes (see [`SweepProfile::sizes`]).
    pub size: (f64, f64),
    pub steps: u32,
    /// The sweep path clicked so far.
    pub path: Vec<DVec2>,
    pub terrain_thickness: f64,
    pub area: usize,
    pub plant: PlantForm,
    pub canopy: f64,
    pub plant_height: f64,
}

impl Default for PartSettings {
    fn default() -> Self {
        Self {
            extrusion_base: 0.0,
            extrusion_height: 1.0,
            void_base: 0.3,
            void_height: 0.6,
            profile: SweepProfile::Coping,
            size: SweepProfile::Coping.defaults(),
            steps: 3,
            path: vec![],
            terrain_thickness: 0.5,
            area: 0,
            plant: PlantForm::Tree,
            canopy: 4.0,
            plant_height: 6.0,
        }
    }
}

/// What an outline drawn with one of the outline tools makes.
#[derive(Clone, Debug)]
pub enum Make {
    Extrusion { base: f64, height: f64, void: bool },
    Terrain { thickness: f64 },
    Area(usize),
}

/// What an outline drawn now with `state`'s tool makes.
pub fn make_for(state: &PlanState) -> Option<Make> {
    let s = &state.parts;
    Some(match state.tool {
        Tool::Extrusion => Make::Extrusion {
            base: s.extrusion_base,
            height: s.extrusion_height,
            void: false,
        },
        Tool::Void => Make::Extrusion {
            base: s.void_base,
            height: s.void_height,
            void: true,
        },
        Tool::Terrain => Make::Terrain {
            thickness: s.terrain_thickness,
        },
        Tool::Paving => Make::Area(s.area),
        _ => return None,
    })
}

/// The new element (and any library material it needs, first) for an
/// outline on `level`, with the undo label and a message.
pub fn from_outline(
    model: &Model,
    level: ElementId,
    outline: Vec<DVec2>,
    make: &Make,
) -> (Vec<Element>, &'static str, String) {
    let mut out = vec![];
    let mut material = |name: &str| {
        let (id, new) = creabim_core::library::find_or_new(model, name)?;
        out.extend(new);
        Some(id)
    };
    let (e, label, msg) = match *make {
        Make::Extrusion { base, height, void } => {
            let p = Part::extrusion(level, outline, base, height);
            if void {
                (
                    Element::new("Void", ElementKind::Part(p.as_void())),
                    "Draw void",
                    "Void added: it cuts every wall, slab, roof and solid part it meets. Set its base and height in Properties. Ctrl+Z to undo.".to_string(),
                )
            } else {
                (
                    Element::new("Extrusion", ElementKind::Part(p)),
                    "Draw extrusion",
                    "Extrusion added. Set its base, height, material and IFC class in Properties; draw a Void to cut into it. Ctrl+Z to undo.".to_string(),
                )
            }
        }
        Make::Terrain { thickness } => {
            let mut s = Site::terrain(level, outline, 0.0, thickness);
            s.material = material("Grass");
            (
                Element::new("Terrain", ElementKind::Site(s)),
                "Draw terrain",
                "Terrain added, flat at the level. Add spot heights in Properties to shape it. Ctrl+Z to undo.".to_string(),
            )
        }
        Make::Area(i) => {
            let p = AREAS.get(i).unwrap_or(&AREAS[0]);
            let mut s = Site::area(level, outline, p.thickness, p.pattern);
            s.material = material(p.material);
            (
                Element::new(p.label, ElementKind::Site(s)),
                "Draw hard landscape",
                format!(
                    "{} added. Change its pattern and depth in Properties. Ctrl+Z to undo.",
                    p.label
                ),
            )
        }
    };
    out.push(e);
    (out, label, msg)
}

/// Commit an outline drawn with an outline tool. Returns a message and the
/// new element.
pub fn finish_outline(
    doc: &mut Document,
    level: ElementId,
    outline: Vec<DVec2>,
    make: &Make,
) -> Result<(String, Option<ElementId>), ModelError> {
    let (elements, label, msg) = from_outline(doc.model(), level, outline, make);
    let id = elements.last().map(|e| e.id);
    let mut tx = Transaction::new(label);
    for e in elements {
        tx = tx.insert(e);
    }
    doc.commit(tx)?;
    Ok((msg, id))
}

/// A click with the Sweep or Planting tool on `level`.
pub fn click(
    doc: &mut Document,
    level: ElementId,
    tool: Tool,
    p: DVec2,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), ModelError> {
    let s = state.parts.clone();
    let add = |doc: &mut Document, selection: &mut Selection, e: Element, label: &str| {
        selection.only(e.id);
        doc.commit(Transaction::new(label).insert(e))
    };
    match tool {
        Tool::Planting => {
            let plant = Site::plant(level, p, s.plant, s.canopy, s.plant_height);
            state.status = Some(format!(
                "{} placed, {} across. Click on to plant more.",
                s.plant.label(),
                crate::units::len(s.canopy)
            ));
            add(
                doc,
                selection,
                Element::new(s.plant.label(), ElementKind::Site(plant)),
                "Place plant",
            )
        }
        Tool::Sweep => {
            let (a, b) = s.size;
            if s.path.is_empty() && s.profile != SweepProfile::Steps {
                if let Some(wall) = crate::memberstool::wall_at(doc.model(), level, p) {
                    let Some(ElementKind::Wall(w)) = doc.model().get(wall).map(|e| &e.kind) else {
                        return Ok(());
                    };
                    let profile = s.profile.profile(w.thickness, a, b, s.steps);
                    let part = Part::sweep(
                        level,
                        profile,
                        Path::Wall {
                            wall,
                            edge: s.profile.edge(),
                            offset: 0.0,
                            extend: [0.0, 0.0],
                        },
                    );
                    state.status = Some(format!(
                        "{} added along the wall's {}: it follows the wall. Click another wall for the next one.",
                        s.profile.label(),
                        s.profile.edge().label().to_lowercase()
                    ));
                    return add(
                        doc,
                        selection,
                        Element::new(s.profile.label(), ElementKind::Part(part)),
                        "Sweep along wall",
                    );
                }
            }
            let path = &mut state.parts.path;
            if path.last().is_some_and(|q| q.distance(p) < 0.05) {
                // clicking the last point again finishes
                let pts = std::mem::take(path);
                state.pending = None;
                if pts.len() < 2 {
                    return Ok(());
                }
                let profile = s.profile.profile(0.2, a, b, s.steps);
                let part = Part::sweep(
                    level,
                    profile,
                    Path::Points {
                        points: pts.iter().map(|q| q.extend(0.0)).collect(),
                        closed: false,
                    },
                );
                state.status = Some(format!("{} added.", s.profile.label()));
                return add(
                    doc,
                    selection,
                    Element::new(s.profile.label(), ElementKind::Part(part)),
                    "Draw sweep",
                );
            }
            path.push(p);
            state.pending = Some(p);
            state.status = Some(
                "Click the path's next point; click the last one again to finish. Esc cancels."
                    .into(),
            );
            Ok(())
        }
        _ => Ok(()),
    }
}

/// The tool bar options of these tools.
pub fn tool_options(state: &PlanState) -> Vec<serde_json::Value> {
    use serde_json::json;
    let s = &state.parts;
    let num = |key: &str, label: &str, value: f64, hint: &str| {
        json!({
            "kind": "number", "key": format!("part.{key}"), "label": label, "value": value,
            "unit": "m", "step": 0.01, "min": -100.0, "max": 1000.0, "hint": hint,
        })
    };
    let seg = |items: Vec<serde_json::Value>| json!({"kind": "seg", "items": items});
    let opt = |id: String, label: &str, on: bool, hint: &str| json!({"id": id, "label": label, "on": on, "hint": hint});
    let mut out = match state.tool {
        Tool::Extrusion => vec![
            num(
                "extrusion_base",
                "Base",
                s.extrusion_base,
                "Height of new extrusions' undersides above the level",
            ),
            num(
                "extrusion_height",
                "Height",
                s.extrusion_height,
                "Height of new extrusions",
            ),
        ],
        Tool::Void => vec![
            num(
                "void_base",
                "Base",
                s.void_base,
                "Height of new voids' undersides above the level",
            ),
            num(
                "void_height",
                "Height",
                s.void_height,
                "Height of new voids",
            ),
        ],
        Tool::Sweep => {
            let (la, lb) = s.profile.sizes();
            let mut v = vec![seg(SweepProfile::ALL
                .iter()
                .map(|p| {
                    opt(
                        format!("part:profile.{}", p.label()),
                        p.label(),
                        s.profile == *p,
                        "The profile new sweeps start from",
                    )
                })
                .collect())];
            v.push(num("size_a", la, s.size.0, "A size of the profile"));
            v.push(num("size_b", lb, s.size.1, "A size of the profile"));
            if s.profile == SweepProfile::Steps {
                v.push(json!({
                    "kind": "number", "key": "part.steps", "label": "Steps", "value": s.steps,
                    "unit": "", "step": 1, "min": 1, "max": 40, "hint": "Number of steps",
                }));
            }
            v
        }
        Tool::Terrain => vec![num(
            "terrain_thickness",
            "Depth",
            s.terrain_thickness,
            "How deep new terrain is below its surface",
        )],
        Tool::Paving => vec![seg(AREAS
            .iter()
            .enumerate()
            .map(|(i, a)| {
                opt(
                    format!("part:area.{i}"),
                    a.label,
                    s.area == i,
                    "The pattern, depth and material new areas start with",
                )
            })
            .collect())],
        Tool::Planting => {
            let mut v = vec![seg(PlantForm::ALL
                .iter()
                .map(|f| {
                    opt(
                        format!("part:plant.{}", f.label()),
                        f.label(),
                        s.plant == *f,
                        "The form of new plants",
                    )
                })
                .collect())];
            v.push(num(
                "canopy",
                "Canopy",
                s.canopy,
                "Spread of new plants' crowns",
            ));
            v.push(num(
                "plant_height",
                "Height",
                s.plant_height,
                "Height of new plants",
            ));
            v
        }
        _ => vec![],
    };
    if !out.is_empty() {
        out.push(json!({"kind": "sep"}));
    }
    out
}

/// A number typed in the tool bar.
pub fn set_option(state: &mut PlanState, key: &str, v: f64) {
    let s = &mut state.parts;
    match key.trim_start_matches("part.") {
        "extrusion_base" => s.extrusion_base = v.clamp(-100.0, 1000.0),
        "extrusion_height" => s.extrusion_height = v.clamp(0.01, 1000.0),
        "void_base" => s.void_base = v.clamp(-100.0, 1000.0),
        "void_height" => s.void_height = v.clamp(0.01, 1000.0),
        "size_a" => s.size.0 = v.clamp(0.0, 20.0),
        "size_b" => s.size.1 = v.clamp(0.005, 20.0),
        "steps" => s.steps = v.round().clamp(1.0, 40.0) as u32,
        "terrain_thickness" => s.terrain_thickness = v.clamp(0.05, 100.0),
        "canopy" => s.canopy = v.clamp(0.2, 50.0),
        "plant_height" => s.plant_height = v.clamp(0.2, 100.0),
        _ => {}
    }
}

/// A choice clicked in the tool bar.
pub fn toggle_option(state: &mut PlanState, what: &str) {
    let s = &mut state.parts;
    if let Some(p) = what.strip_prefix("profile.") {
        if let Some(p) = SweepProfile::ALL.into_iter().find(|x| x.label() == p) {
            if p != s.profile {
                s.profile = p;
                s.size = p.defaults();
            }
        }
    } else if let Some(i) = what.strip_prefix("area.") {
        s.area = i.parse().unwrap_or(0).min(AREAS.len() - 1);
    } else if let Some(f) = what.strip_prefix("plant.") {
        if let Some(f) = PlantForm::ALL.into_iter().find(|x| x.label() == f) {
            s.plant = f;
            (s.canopy, s.plant_height) = match f {
                PlantForm::Tree => (4.0, 6.0),
                PlantForm::Conifer => (2.5, 7.0),
                PlantForm::Palm => (4.0, 7.0),
                PlantForm::Shrub => (1.2, 1.0),
            };
        }
    }
}

// ---- Properties ----

fn num(ui: &mut egui::Ui, text: &str, v: &mut f64, lo: f64, hi: f64) -> bool {
    ui.horizontal(|ui| {
        ui.label(text);
        ui.add(crate::units::drag(v).range(lo..=hi)).changed()
    })
    .inner
}

fn weak(ui: &mut egui::Ui, t: impl Into<String>) {
    ui.label(egui::RichText::new(t.into()).weak().small());
}

/// The Properties fields of a part or a site element. Returns the undo
/// label of a change, or "".
pub fn properties(ui: &mut egui::Ui, model: &Model, kind: &mut ElementKind) -> &'static str {
    match kind {
        ElementKind::Part(p) => part_fields(ui, model, p),
        ElementKind::Site(s) => site_fields(ui, model, s),
        _ => "",
    }
}

fn part_fields(ui: &mut egui::Ui, model: &Model, p: &mut Part) -> &'static str {
    let mut label = "";
    if ui
        .checkbox(&mut p.void, "Void: cuts what it meets")
        .on_hover_text("A void cuts every wall, slab, roof, site surface and solid part it meets, and is drawn only as a see-through outline in 3D")
        .changed()
    {
        label = "Void";
    }
    match &mut p.form {
        Form::Extrusion {
            outline,
            holes,
            base,
            height,
        } => {
            if num(ui, "Base", base, -100.0, 1000.0) {
                label = "Extrusion base";
            }
            if num(ui, "Height", height, 0.001, 1000.0) {
                label = "Extrusion height";
            }
            weak(
                ui,
                format!(
                    "An outline of {} corners{}, pulled up from {} above its level.",
                    outline.len(),
                    if holes.is_empty() {
                        String::new()
                    } else {
                        format!(" with {} holes", holes.len())
                    },
                    crate::units::len(*base)
                ),
            );
        }
        Form::Sweep { profile, path } => {
            // the profile's width and height, rescaling it
            let (lo, hi) = profile.iter().fold(
                (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
                |(a, b), q| (a.min(*q), b.max(*q)),
            );
            let size = hi - lo;
            let (mut w, mut h) = (size.x, size.y);
            if num(ui, "Profile width", &mut w, 0.005, 50.0) && size.x > 1e-9 {
                let k = w / size.x;
                for q in profile.iter_mut() {
                    q.x *= k;
                }
                label = "Profile width";
            }
            if num(ui, "Profile height", &mut h, 0.005, 50.0) && size.y > 1e-9 {
                let k = h / size.y;
                for q in profile.iter_mut() {
                    q.y = lo.y + (q.y - lo.y) * k;
                }
                label = "Profile height";
            }
            if ui
                .button("Mirror profile")
                .on_hover_text("Swap the profile to the path's other side")
                .clicked()
            {
                for q in profile.iter_mut() {
                    q.x = -q.x;
                }
                profile.reverse();
                label = "Mirror profile";
            }
            match path {
                Path::Wall {
                    wall,
                    edge,
                    offset,
                    extend,
                } => {
                    let name = model.get(*wall).map_or("?", |e| e.name.as_str());
                    weak(ui, format!("Follows wall {name}."));
                    ui.horizontal(|ui| {
                        ui.label("Along its");
                        for e in [WallEdge::Top, WallEdge::Base] {
                            if ui.selectable_value(edge, e, e.label()).changed() {
                                label = "Sweep edge";
                            }
                        }
                    });
                    if num(ui, "Lift", offset, -100.0, 100.0) {
                        label = "Sweep offset";
                    }
                    if num(ui, "Run past start", &mut extend[0], -50.0, 50.0)
                        | num(ui, "Run past end", &mut extend[1], -50.0, 50.0)
                    {
                        label = "Sweep ends";
                    }
                }
                Path::Points { points, closed } => {
                    if ui.checkbox(closed, "Closed path").changed() {
                        label = "Sweep path";
                    }
                    let mut z = points.first().map_or(0.0, |q| q.z);
                    if num(ui, "Height above level", &mut z, -100.0, 1000.0) {
                        for q in points.iter_mut() {
                            q.z = z;
                        }
                        label = "Sweep height";
                    }
                    weak(ui, format!("A path through {} points.", points.len()));
                }
            }
        }
    }
    if !p.void {
        egui::ComboBox::from_id_salt("part_ifc_class")
            .selected_text(p.ifc_class.clone())
            .width(190.0)
            .show_ui(ui, |ui| {
                for c in IFC_CLASSES {
                    if ui.selectable_label(p.ifc_class == *c, *c).clicked() {
                        p.ifc_class = c.to_string();
                        label = "IFC class";
                    }
                }
            })
            .response
            .on_hover_text("The IFC class it is exported as");
    }
    if let Some((lo, hi)) = p.bounds(model) {
        weak(
            ui,
            format!(
                "From {} to {}.",
                crate::units::elevation(lo.z),
                crate::units::elevation(hi.z)
            ),
        );
    }
    label
}

fn site_fields(ui: &mut egui::Ui, model: &Model, s: &mut Site) -> &'static str {
    let mut label = "";
    let base = creabim_core::members::elevation(model, s.level);
    match &mut s.form {
        SiteForm::Terrain {
            boundary,
            top,
            thickness,
            points,
            contour,
        } => {
            if num(ui, "Top", top, -100.0, 1000.0) {
                label = "Terrain top";
            }
            if num(ui, "Depth", thickness, 0.05, 100.0) {
                label = "Terrain depth";
            }
            if num(ui, "Contours every", contour, 0.0, 50.0) {
                label = "Contour interval";
            }
            ui.label("Spot heights");
            let mut remove = None;
            for (i, q) in points.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let mut changed = ui.add(crate::units::drag(&mut q.x)).changed();
                    changed |= ui.add(crate::units::drag(&mut q.y)).changed();
                    changed |= ui
                        .add(crate::units::drag(&mut q.z))
                        .on_hover_text("Height above the level")
                        .changed();
                    if changed {
                        label = "Spot height";
                    }
                    if ui.small_button("✕").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                points.remove(i);
                label = "Remove spot height";
            }
            if ui
                .button("Add spot height")
                .on_hover_text("A new spot height in the middle; type where it is and how high")
                .clicked()
            {
                let n = boundary.len().max(1) as f64;
                let c = boundary.iter().copied().sum::<DVec2>() / n;
                points.push(DVec3::new(c.x, c.y, *top + 0.5));
                label = "Add spot height";
            }
            weak(
                ui,
                if points.is_empty() {
                    format!("Flat, at {}.", crate::units::elevation(base + *top))
                } else {
                    "Its surface runs through the spot heights.".to_string()
                },
            );
        }
        SiteForm::Area {
            top,
            thickness,
            pattern,
            ..
        } => {
            if num(ui, "Top", top, -100.0, 1000.0) {
                label = "Area top";
            }
            if num(ui, "Depth", thickness, 0.005, 10.0) {
                label = "Area depth";
            }
            ui.horizontal(|ui| {
                ui.label("Pattern");
                let choices = [
                    Pattern::None,
                    Pattern::Units {
                        size: DVec2::new(0.6, 0.4),
                        stagger: true,
                        angle: 0.0,
                    },
                    Pattern::Lines {
                        spacing: 0.15,
                        angle: 0.0,
                    },
                    Pattern::Stipple { spacing: 0.12 },
                ];
                for c in choices {
                    let on = pattern.label() == c.label();
                    if ui.selectable_label(on, c.label()).clicked() && !on {
                        *pattern = c;
                        label = "Area pattern";
                    }
                }
            });
            let deg = |ui: &mut egui::Ui, v: &mut f64| {
                ui.horizontal(|ui| {
                    ui.label("Angle");
                    ui.add(egui::DragValue::new(v).speed(1.0).suffix("°"))
                        .changed()
                })
                .inner
            };
            match pattern {
                Pattern::None => {}
                Pattern::Units {
                    size,
                    stagger,
                    angle,
                } => {
                    if num(ui, "Unit length", &mut size.x, 0.01, 10.0)
                        | num(ui, "Unit width", &mut size.y, 0.01, 10.0)
                    {
                        label = "Pattern size";
                    }
                    if ui.checkbox(stagger, "Staggered courses").changed() {
                        label = "Pattern bond";
                    }
                    if deg(ui, angle) {
                        label = "Pattern angle";
                    }
                }
                Pattern::Lines { spacing, angle } => {
                    if num(ui, "Spacing", spacing, 0.01, 10.0) {
                        label = "Pattern spacing";
                    }
                    if deg(ui, angle) {
                        label = "Pattern angle";
                    }
                }
                Pattern::Stipple { spacing } => {
                    if num(ui, "Spacing", spacing, 0.01, 10.0) {
                        label = "Pattern spacing";
                    }
                }
            }
        }
        SiteForm::Plant {
            position,
            base: b,
            form,
            canopy,
            height,
        } => {
            ui.horizontal(|ui| {
                ui.label("Form");
                for f in PlantForm::ALL {
                    if ui.selectable_value(form, f, f.label()).changed() {
                        label = "Plant form";
                    }
                }
            });
            if num(ui, "Canopy", canopy, 0.1, 50.0) {
                label = "Canopy";
            }
            if num(ui, "Height", height, 0.1, 100.0) {
                label = "Plant height";
            }
            if num(ui, "Base", b, -100.0, 1000.0) {
                label = "Plant base";
            }
            if num(ui, "X", &mut position.x, -1e4, 1e4) | num(ui, "Y", &mut position.y, -1e4, 1e4) {
                label = "Move plant";
            }
        }
    }
    label
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

    #[test]
    fn a_coping_follows_the_wall_clicked() {
        let (mut doc, level, mut st, mut sel) = setup();
        let (wid, mid) = doc
            .model()
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) if w.level == level && w.curtain.is_none() => {
                    Some((e.id, (w.start + w.end) / 2.0))
                }
                _ => None,
            })
            .unwrap();
        click(&mut doc, level, Tool::Sweep, mid, &mut st, &mut sel).unwrap();
        let id = sel.all()[0];
        let e = doc.model().get(id).unwrap();
        let ElementKind::Part(p) = &e.kind else {
            panic!("not a part")
        };
        assert_eq!(p.host(), Some(wid));
        // undo takes it away again
        doc.undo();
        assert!(doc.model().get(id).is_none());
    }

    #[test]
    fn a_free_sweep_is_clicked_point_by_point() {
        let (mut doc, level, mut st, mut sel) = setup();
        st.parts.profile = SweepProfile::Steps;
        st.parts.size = SweepProfile::Steps.defaults();
        let far = DVec2::new(-40.0, -40.0);
        for p in [far, far + DVec2::X * 1.5, far + DVec2::X * 1.5] {
            click(&mut doc, level, Tool::Sweep, p, &mut st, &mut sel).unwrap();
        }
        let ElementKind::Part(p) = &doc.model().get(sel.all()[0]).unwrap().kind else {
            panic!("not a part")
        };
        assert!(
            matches!(&p.form, Form::Sweep { path: Path::Points { points, .. }, .. } if points.len() == 2)
        );
        assert!(st.parts.path.is_empty());
    }

    #[test]
    fn outlines_make_parts_and_site_with_their_materials() {
        let (mut doc, level, _, _) = setup();
        let sq = creabim_core::polygon::rectangle(DVec2::new(30.0, 30.0), DVec2::new(34.0, 36.0));
        for make in [
            Make::Extrusion {
                base: 0.0,
                height: 1.0,
                void: false,
            },
            Make::Terrain { thickness: 0.5 },
            Make::Area(1),
        ] {
            let (_, id) = finish_outline(&mut doc, level, sq.clone(), &make).unwrap();
            let e = doc.model().get(id.unwrap()).unwrap();
            match make {
                Make::Extrusion { .. } => assert!(matches!(e.kind, ElementKind::Part(_))),
                _ => assert!(creabim_core::material_of(doc.model(), e).is_some()),
            }
        }
        // the driveway's material came from the library, once
        let (_, id) = finish_outline(&mut doc, level, sq, &Make::Area(1)).unwrap();
        assert!(id.is_some());
        let n = doc
            .model()
            .iter()
            .filter(|e| e.name == "Block paving" && matches!(e.kind, ElementKind::Material(_)))
            .count();
        assert_eq!(n, 1);
    }

    #[test]
    fn planting_places_a_plant() {
        let (mut doc, level, mut st, mut sel) = setup();
        st.parts.plant = PlantForm::Palm;
        click(
            &mut doc,
            level,
            Tool::Planting,
            DVec2::new(20.0, 0.0),
            &mut st,
            &mut sel,
        )
        .unwrap();
        let e = doc.model().get(sel.all()[0]).unwrap();
        assert!(matches!(&e.kind, ElementKind::Site(s) if s.is_plant()));
        assert_eq!(e.name, "Palm");
    }
}
