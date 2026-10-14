//! Edits that follow the pointer: Blender-style move (G), rotate (R) and
//! scale (S) from the keyboard, and dragging grips and elements in plans.
//! The same code serves the 3D view, plans and sections; each pane only
//! says how its pointer maps onto the model.

use creabim_core::{
    transform::{self, Xform},
    Axis, Document, Element, ElementId, ElementKind, ModelError, Transaction,
};
use eframe::egui::{self, Color32, Pos2, Stroke};
use glam::{DVec2, DVec3};

/// An edit remade from the same starting elements every frame, so it can
/// follow the pointer and still undo, or cancel, as one step.
#[derive(Debug, Default)]
pub struct LiveEdit {
    originals: Vec<Element>,
    label: String,
    applied: bool,
}

impl LiveEdit {
    pub fn new(label: impl Into<String>, originals: Vec<Element>) -> Self {
        Self {
            originals,
            label: label.into(),
            applied: false,
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn originals(&self) -> &[Element] {
        &self.originals
    }

    /// Show `changed` (the originals edited, same ids) in place of the
    /// previous frame's version.
    pub fn update(&mut self, doc: &mut Document, changed: Vec<Element>) -> Result<(), ModelError> {
        self.cancel(doc);
        let mut tx = Transaction::new(self.label.clone());
        for (o, n) in self.originals.iter().zip(changed) {
            if *o != n {
                tx = tx.replace(o.clone(), n);
            }
        }
        if tx.ops.is_empty() {
            return Ok(());
        }
        doc.commit(tx)?;
        self.applied = true;
        Ok(())
    }

    /// Put everything back as it was.
    pub fn cancel(&mut self, doc: &mut Document) {
        if self.applied && doc.undo_label() == Some(self.label.as_str()) {
            doc.discard_last();
        }
        self.applied = false;
    }

    pub fn applied(&self) -> bool {
        self.applied
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Move,
    Rotate,
    Scale,
}

impl Mode {
    fn verb(self) -> &'static str {
        match self {
            Mode::Move => "Move",
            Mode::Rotate => "Rotate",
            Mode::Scale => "Scale",
        }
    }
}

/// What rotating and scaling turn about, as in Blender.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pivot {
    /// The middle of the selected elements' own pivots.
    #[default]
    Median,
    /// The centre of the box round them.
    Bounds,
    /// Each element about itself.
    Individual,
    /// The active element (the last one picked).
    Active,
    /// The pivot cursor, placed with Shift+right-click.
    Cursor,
}

impl Pivot {
    pub const ALL: [Pivot; 5] = [
        Pivot::Median,
        Pivot::Bounds,
        Pivot::Individual,
        Pivot::Active,
        Pivot::Cursor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Pivot::Median => "Median point",
            Pivot::Bounds => "Bounding box centre",
            Pivot::Individual => "Individual origins",
            Pivot::Active => "Active element",
            Pivot::Cursor => "Pivot cursor",
        }
    }
}

/// Where a pointer edit runs, which decides the plane things move in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Space {
    /// The 3D view: things slide on the level they stand on, or up and down
    /// with the Z axis.
    Model,
    /// A plan: things slide in plan.
    Plan,
    /// A section or elevation: things slide in the section plane.
    Section { start: DVec2, end: DVec2 },
}

/// How a pane maps its pointer onto the model, for one frame.
pub struct PaneMap<'a> {
    /// The pointer as a model point on the plane through `pivot` that
    /// things move in (vertical when constrained to Z).
    pub to_model: &'a dyn Fn(Pos2, Option<Axis>, DVec3) -> Option<DVec3>,
    /// A model point on screen.
    pub project: &'a dyn Fn(DVec3) -> Option<Pos2>,
    /// The direction the pane looks in, into the screen.
    pub look: DVec3,
}

/// A keyboard move, rotate or scale in progress.
#[derive(Debug)]
pub struct Modal {
    pub mode: Mode,
    /// A mesh face being edited instead of the whole element.
    pub face: Option<usize>,
    pub space: Space,
    /// The pane that runs it: a view, or `None` for the 3D view.
    pub pane: Option<ElementId>,
    /// The point things turn and scale about.
    pivot: DVec3,
    /// Each element's own pivot, for individual origins.
    own: Vec<DVec3>,
    individual: bool,
    grab: Pos2,
    pointer: Pos2,
    axis: Option<Axis>,
    typed: String,
    edit: LiveEdit,
    /// Started by Shift+D: cancelling also takes the copies away.
    pub duplicated: bool,
    shown: String,
    /// Elements left out because they can't take this edit.
    skipped: usize,
}

pub enum Outcome {
    Running(String),
    Done(String),
    Cancelled(String),
}

impl Modal {
    /// Start editing `ids` (the active one last), or one mesh face of a
    /// single mass, from pointer `grab`.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        doc: &Document,
        mode: Mode,
        ids: &[ElementId],
        face: Option<usize>,
        space: Space,
        pane: Option<ElementId>,
        grab: Pos2,
        pivot: Pivot,
        cursor: Option<DVec3>,
    ) -> Result<Modal, String> {
        let model = doc.model();
        let mut elements: Vec<&Element> = ids.iter().filter_map(|id| model.get(*id)).collect();
        // doors and windows go with their wall when it moves too
        let hosts: Vec<ElementId> = elements.iter().map(|e| e.id).collect();
        // so do cuts with what they cut
        elements.retain(|e| match &e.kind {
            ElementKind::Opening(o) => !hosts.contains(&o.host),
            ElementKind::Cut(_) => false,
            _ => true,
        });
        let first = *elements.last().ok_or("Select something first.")?;
        let face =
            face.filter(|_| elements.len() == 1 && matches!(first.kind, ElementKind::FreeMesh(_)));
        if let (Some(f), ElementKind::FreeMesh(m)) = (face, &first.kind) {
            if f < m.mesh.faces.len() {
                let z = m
                    .level
                    .and_then(|l| model.level(l))
                    .map_or(0.0, |l| l.elevation);
                let c = m.origin + m.to_model(m.mesh.face_center(f)) + DVec3::Z * z;
                let label = format!("{} face", mode.verb());
                return Ok(Modal {
                    mode,
                    face,
                    space,
                    pane,
                    pivot: c,
                    own: vec![c],
                    individual: false,
                    grab,
                    pointer: grab,
                    axis: None,
                    typed: String::new(),
                    edit: LiveEdit::new(label, vec![first.clone()]),
                    duplicated: false,
                    shown: String::new(),
                    skipped: 0,
                });
            }
        }
        // keep what can take this kind of edit
        let probe = |c: DVec3| match mode {
            Mode::Move => Xform::Move(DVec3::X),
            Mode::Rotate => match space {
                Space::Section { .. } => Xform::Tilt {
                    center: c,
                    axis: DVec3::X,
                    degrees: 1.0,
                },
                _ => Xform::Rotate {
                    center: c.truncate(),
                    degrees: 1.0,
                },
            },
            Mode::Scale => Xform::Scale {
                center: c,
                factor: 1.1,
            },
        };
        let mut kept: Vec<(Element, DVec3)> = Vec::new();
        let mut skipped = 0;
        for e in &elements {
            let Some(c) = transform::pivot(model, e.id) else {
                skipped += 1;
                continue;
            };
            if transform::transformed(model, e, &probe(c)).is_some() {
                kept.push(((*e).clone(), c));
            } else {
                skipped += 1;
            }
        }
        if kept.is_empty() {
            let name = &first.name;
            return Err(match (mode, elements.len(), space) {
                (Mode::Rotate, _, Space::Section { .. }) => {
                    "Only masses, sketches and guides turn in a section; walls and slabs stay upright.".into()
                }
                (Mode::Move, 1, _) => format!("{name} can't be moved this way."),
                (Mode::Rotate, 1, _) => format!("{name} can't be rotated on its own."),
                (Mode::Scale, 1, _) => format!("{name} can't be scaled."),
                (m, n, _) => format!("None of the {n} selected can take a {}.", m.verb().to_lowercase()),
            });
        }
        let own: Vec<DVec3> = kept.iter().map(|k| k.1).collect();
        let n = own.len() as f64;
        let median = own.iter().copied().sum::<DVec3>() / n;
        let lo = own.iter().copied().fold(DVec3::splat(f64::MAX), DVec3::min);
        let hi = own.iter().copied().fold(DVec3::splat(f64::MIN), DVec3::max);
        let center = match pivot {
            Pivot::Median | Pivot::Individual => median,
            Pivot::Bounds => (lo + hi) / 2.0,
            Pivot::Active => *own.last().expect("kept is not empty"),
            Pivot::Cursor => cursor.unwrap_or(median),
        };
        let what = if kept.len() == 1 {
            kept[0].0.name.clone()
        } else {
            format!("{} elements", kept.len())
        };
        Ok(Modal {
            mode,
            face: None,
            space,
            pane,
            pivot: center,
            own,
            individual: pivot == Pivot::Individual && kept.len() > 1,
            grab,
            pointer: grab,
            axis: None,
            typed: String::new(),
            edit: LiveEdit::new(
                format!("{} {what}", mode.verb()),
                kept.into_iter().map(|k| k.0).collect(),
            ),
            duplicated: false,
            shown: String::new(),
            skipped,
        })
    }

    /// Follow the pointer and keys for one frame, drawing the feedback.
    pub fn update(
        &mut self,
        doc: &mut Document,
        ctx: &egui::Context,
        painter: &egui::Painter,
        pointer: Option<Pos2>,
        map: &PaneMap,
    ) -> Outcome {
        let (events, enter, escape, backspace, confirm, cancel, ctrl) = ctx.input(|i| {
            (
                i.events.clone(),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::Backspace),
                i.pointer.primary_released(),
                i.pointer.secondary_released(),
                i.modifiers.command,
            )
        });
        if let Some(p) = pointer {
            self.pointer = p;
        }
        for e in &events {
            if let egui::Event::Text(t) = e {
                for c in t.chars() {
                    match c.to_ascii_lowercase() {
                        'x' => self.toggle(Axis::X),
                        'y' => self.toggle(Axis::Y),
                        'z' => self.toggle(Axis::Z),
                        c @ ('0'..='9' | '.' | '-') => self.typed.push(c),
                        _ => {}
                    }
                }
            }
        }
        if backspace {
            self.typed.pop();
        }
        if escape || cancel {
            self.cancel(doc);
            return Outcome::Cancelled(format!("{} cancelled.", self.mode.verb()));
        }

        let xform = self.xform(map, ctrl);
        let result = match xform {
            Some(t) => self.apply(doc, &t),
            None => Ok(()),
        };
        if let Err(e) = result {
            self.shown = format!("Can't go there: {e}");
        }
        self.draw(painter, map);
        let skipped = match self.skipped {
            0 => String::new(),
            1 => " (1 selected element can't take this and stays put)".into(),
            n => format!(" ({n} selected elements can't take this and stay put)"),
        };
        if enter || confirm {
            let done = format!("{}{skipped}. Ctrl+Z to undo.", self.shown);
            return Outcome::Done(done);
        }
        let keys = match self.mode {
            Mode::Move => "X, Y or Z keeps to an axis",
            Mode::Rotate => "X or Y tilts about that axis",
            Mode::Scale => "X, Y or Z scales along one axis",
        };
        Outcome::Running(format!(
            "{}{skipped}   ·   {keys}, type a value, click or Enter to finish, Esc to cancel{}",
            self.shown,
            if self.mode == Mode::Rotate {
                ", Ctrl snaps to 15°"
            } else {
                ", Ctrl snaps"
            }
        ))
    }

    fn toggle(&mut self, a: Axis) {
        if self.mode == Mode::Rotate && self.face.is_some() {
            return;
        }
        self.axis = if self.axis == Some(a) { None } else { Some(a) };
    }

    /// Undo whatever this edit has done.
    pub fn cancel(&mut self, doc: &mut Document) {
        self.edit.cancel(doc);
        if self.duplicated && doc.undo_label().is_some_and(|l| l.starts_with("Duplicate")) {
            doc.discard_last();
        }
    }

    /// The turn axis for a rotate: about the pane's view in plans and
    /// sections, about the vertical in 3D, or about a chosen axis.
    fn turn_axis(&self) -> Option<DVec3> {
        match (self.axis, self.space) {
            (Some(Axis::Z), _) | (None, Space::Model | Space::Plan) => None,
            (Some(a), _) => Some(a.unit()),
            (None, Space::Section { start, end }) => {
                Some(-(end - start).normalize_or_zero().perp().extend(0.0))
            }
        }
    }

    fn xform(&mut self, map: &PaneMap, ctrl: bool) -> Option<Xform> {
        let typed: Option<f64> = self.typed.parse().ok();
        let snap = |v: f64, step: f64| (v / step).round() * step;
        let ps = (map.project)(self.pivot).unwrap_or(self.grab);
        Some(match self.mode {
            Mode::Move => {
                let d = match typed {
                    Some(v) => {
                        let along = match (self.axis, self.space) {
                            (Some(a), _) => a.unit(),
                            (None, Space::Section { start, end }) => {
                                (end - start).normalize_or_zero().extend(0.0)
                            }
                            (None, _) => DVec3::X,
                        };
                        along * v
                    }
                    None => {
                        let a = (map.to_model)(self.grab, self.axis, self.pivot)?;
                        let b = (map.to_model)(self.pointer, self.axis, self.pivot)?;
                        let mut d = b - a;
                        if let Some(ax) = self.axis {
                            d = ax.unit() * d.dot(ax.unit());
                        }
                        let step = if ctrl { 0.25 } else { 0.01 };
                        DVec3::new(snap(d.x, step), snap(d.y, step), snap(d.z, step))
                    }
                };
                let axis = self
                    .axis
                    .map_or(String::new(), |a| format!(" along {}", a.label()));
                self.shown = format!("{}: {:.3} m{axis}", self.edit.label, d.length());
                Xform::Move(d)
            }
            Mode::Rotate => {
                let angle = |p: Pos2| (-(p.y - ps.y)).atan2(p.x - ps.x) as f64;
                let turn = self.turn_axis();
                let degrees = match typed {
                    Some(v) => v,
                    None => {
                        let d = (angle(self.pointer) - angle(self.grab)).to_degrees();
                        let d = (d + 180.0).rem_euclid(360.0) - 180.0;
                        // turning on screen anticlockwise turns about the
                        // axis that points at the viewer
                        let toward = match turn {
                            Some(a) if a.dot(-map.look) < 0.0 => -1.0,
                            _ => 1.0,
                        };
                        snap(d, if ctrl { 15.0 } else { 1.0 }) * toward
                    }
                };
                match turn {
                    Some(axis) => {
                        let about = self
                            .axis
                            .map_or(String::new(), |a| format!(" about {}", a.label()));
                        self.shown = format!("{}: {degrees:.1}°{about}", self.edit.label);
                        Xform::Tilt {
                            center: self.pivot,
                            axis,
                            degrees,
                        }
                    }
                    None => {
                        self.shown = format!("{}: {degrees:.1}°", self.edit.label);
                        Xform::Rotate {
                            center: self.pivot.truncate(),
                            degrees,
                        }
                    }
                }
            }
            Mode::Scale => {
                let factor = match typed {
                    Some(v) if v != 0.0 => v,
                    _ => {
                        let r0 = self.grab.distance(ps).max(4.0) as f64;
                        let r1 = self.pointer.distance(ps) as f64;
                        snap(r1 / r0, if ctrl { 0.1 } else { 0.01 }).max(0.01)
                    }
                };
                match self.axis.filter(|_| self.face.is_none()) {
                    Some(a) => {
                        self.shown =
                            format!("{}: × {factor:.2} along {}", self.edit.label, a.label());
                        let mut f = DVec3::ONE;
                        f[a.index()] = factor;
                        Xform::Stretch {
                            center: self.pivot,
                            factor: f,
                        }
                    }
                    None => {
                        self.shown = format!("{}: × {factor:.2}", self.edit.label);
                        Xform::Scale {
                            center: self.pivot,
                            factor,
                        }
                    }
                }
            }
        })
    }

    /// The same change about another centre, for individual origins.
    fn about(t: &Xform, c: DVec3) -> Xform {
        match *t {
            Xform::Move(d) => Xform::Move(d),
            Xform::Rotate { degrees, .. } => Xform::Rotate {
                center: c.truncate(),
                degrees,
            },
            Xform::Tilt { axis, degrees, .. } => Xform::Tilt {
                center: c,
                axis,
                degrees,
            },
            Xform::Scale { factor, .. } => Xform::Scale { center: c, factor },
            Xform::Stretch { factor, .. } => Xform::Stretch { center: c, factor },
        }
    }

    fn apply(&mut self, doc: &mut Document, t: &Xform) -> Result<(), String> {
        let originals = self.edit.originals().to_vec();
        let changed = match (self.face, &originals[0].kind) {
            (Some(f), ElementKind::FreeMesh(m)) => {
                let mut m = m.clone();
                let verts = m.mesh.faces[f].clone();
                match *t {
                    Xform::Move(d) => m.mesh.translate_vertices(&verts, m.to_local(d)),
                    Xform::Scale { factor, .. } => m.mesh.scale_face(f, factor),
                    Xform::Rotate { degrees, .. } => {
                        let c = m.mesh.face_center(f);
                        let r = DVec2::from_angle(degrees.to_radians());
                        let mut moved = verts.clone();
                        moved.sort_unstable();
                        moved.dedup();
                        for v in moved {
                            let p = &mut m.mesh.vertices[v as usize];
                            *p = (c.truncate() + r.rotate(p.truncate() - c.truncate())).extend(p.z);
                        }
                    }
                    Xform::Tilt { .. } | Xform::Stretch { .. } => {}
                }
                let mut e = originals[0].clone();
                e.kind = ElementKind::FreeMesh(m);
                vec![e]
            }
            _ => {
                let mut out = Vec::with_capacity(originals.len());
                for (i, o) in originals.iter().enumerate() {
                    let t = if self.individual {
                        Self::about(t, self.own[i])
                    } else {
                        *t
                    };
                    out.push(
                        transform::transformed(doc.model(), o, &t)
                            .ok_or_else(|| format!("{} can't take that change", o.name))?,
                    );
                }
                out
            }
        };
        self.edit.update(doc, changed).map_err(|e| e.to_string())
    }

    fn draw(&self, painter: &egui::Painter, map: &PaneMap) {
        let accent = Color32::from_rgb(230, 120, 30);
        let Some(ps) = (map.project)(self.pivot) else {
            return;
        };
        if let Some(a) = self.axis {
            let color = match a {
                Axis::X => Color32::from_rgb(210, 60, 60),
                Axis::Y => Color32::from_rgb(60, 160, 60),
                Axis::Z => Color32::from_rgb(60, 100, 220),
            };
            let far = |s: f64| (map.project)(self.pivot + a.unit() * s);
            if let (Some(p), Some(q)) = (far(-200.0), far(200.0)) {
                painter.line_segment([p, q], Stroke::new(1.5, color));
            }
        }
        if self.individual {
            for c in &self.own {
                if let Some(p) = (map.project)(*c) {
                    painter.circle_stroke(p, 3.0, Stroke::new(1.0, accent));
                }
            }
        } else {
            painter.circle_stroke(ps, 4.0, Stroke::new(1.5, accent));
        }
        if self.mode != Mode::Move {
            painter.add(egui::Shape::dashed_line(
                &[ps, self.pointer],
                Stroke::new(1.0, accent),
                6.0,
                4.0,
            ));
        }
    }
}

/// Copies of `ids` (with the doors in copied walls), inserted as one undo
/// step. Returns the copies to keep moving.
pub fn duplicate(doc: &mut Document, ids: &[ElementId]) -> Result<Vec<ElementId>, String> {
    let model = doc.model();
    let copies = if let [one] = ids {
        // one element: a lone door's copy goes beside it on its wall
        transform::duplicate(model, *one)
    } else {
        let set = transform::copy_set(model, ids);
        let elements: Vec<Element> = set
            .iter()
            .filter_map(|id| model.get(*id))
            .cloned()
            .collect();
        let (mut copies, _) = transform::fresh_copies(model, &elements);
        for (c, o) in copies.iter_mut().zip(&elements) {
            if ids.contains(&o.id) {
                c.name = format!("{} copy", c.name);
            }
        }
        copies
    };
    if copies.is_empty() {
        return Err("That can't be duplicated.".into());
    }
    let name = match copies.len() {
        1 => copies[0].name.clone(),
        n => format!("{n} elements"),
    };
    let ids: Vec<ElementId> = copies.iter().map(|c| c.id).collect();
    let mut tx = Transaction::new(format!("Duplicate {name}"));
    for c in copies {
        tx = tx.insert(c);
    }
    doc.commit(tx).map_err(|e| e.to_string())?;
    Ok(ids)
}
