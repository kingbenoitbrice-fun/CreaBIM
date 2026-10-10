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
    pivot: DVec3,
    grab: Pos2,
    pointer: Pos2,
    axis: Option<Axis>,
    typed: String,
    edit: LiveEdit,
    /// Started by Shift+D: cancelling also takes the copy away.
    pub duplicated: bool,
    shown: String,
}

pub enum Outcome {
    Running(String),
    Done(String),
    Cancelled(String),
}

impl Modal {
    /// Start editing `id` (or one of its mesh faces) from pointer `grab`.
    pub fn start(
        doc: &Document,
        mode: Mode,
        id: ElementId,
        face: Option<usize>,
        space: Space,
        pane: Option<ElementId>,
        grab: Pos2,
    ) -> Result<Modal, String> {
        let model = doc.model();
        let e = model.get(id).ok_or("Select something first.")?;
        if matches!(space, Space::Section { .. }) && mode == Mode::Rotate {
            return Err("Rotate in a plan or the 3D view: things turn about the vertical.".into());
        }
        let pivot = match (face, &e.kind) {
            (Some(f), ElementKind::FreeMesh(m)) if f < m.mesh.faces.len() => {
                let z = m
                    .level
                    .and_then(|l| model.level(l))
                    .map_or(0.0, |l| l.elevation);
                m.origin + m.to_model(m.mesh.face_center(f)) + DVec3::Z * z
            }
            _ => {
                transform::pivot(model, id).ok_or(format!("{} can't be moved this way.", e.name))?
            }
        };
        let face = face.filter(|_| matches!(e.kind, ElementKind::FreeMesh(_)));
        if face.is_none() {
            let probe = match mode {
                Mode::Move => Xform::Move(DVec3::X),
                Mode::Rotate => Xform::Rotate {
                    center: pivot.truncate(),
                    degrees: 1.0,
                },
                Mode::Scale => Xform::Scale {
                    center: pivot,
                    factor: 1.1,
                },
            };
            if transform::transformed(model, e, &probe).is_none() {
                return Err(match mode {
                    Mode::Move => format!("{} can't be moved this way.", e.name),
                    Mode::Rotate => format!("{} can't be rotated on its own.", e.name),
                    Mode::Scale => format!("{} can't be scaled.", e.name),
                });
            }
        }
        let what = match face {
            Some(_) => "face".to_string(),
            None => e.name.clone(),
        };
        Ok(Modal {
            mode,
            face,
            space,
            pane,
            pivot,
            grab,
            pointer: grab,
            axis: None,
            typed: String::new(),
            edit: LiveEdit::new(format!("{} {what}", mode.verb()), vec![e.clone()]),
            duplicated: false,
            shown: String::new(),
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
        if enter || confirm {
            let done = format!("{}. Ctrl+Z to undo.", self.shown);
            return Outcome::Done(done);
        }
        Outcome::Running(format!(
            "{}   ·   X, Y or Z keeps to an axis, type a value, click or Enter to finish, Esc to cancel{}",
            self.shown,
            if self.mode == Mode::Rotate {
                ", Ctrl snaps to 15°"
            } else {
                ", Ctrl snaps"
            }
        ))
    }

    fn toggle(&mut self, a: Axis) {
        if self.mode == Mode::Move {
            self.axis = if self.axis == Some(a) { None } else { Some(a) };
        }
    }

    /// Undo whatever this edit has done.
    pub fn cancel(&mut self, doc: &mut Document) {
        self.edit.cancel(doc);
        if self.duplicated && doc.undo_label().is_some_and(|l| l.starts_with("Duplicate")) {
            doc.discard_last();
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
                let degrees = match typed {
                    Some(v) => v,
                    None => {
                        let d = (angle(self.pointer) - angle(self.grab)).to_degrees();
                        let d = (d + 180.0).rem_euclid(360.0) - 180.0;
                        snap(d, if ctrl { 15.0 } else { 1.0 })
                    }
                };
                self.shown = format!("{}: {degrees:.1}°", self.edit.label);
                Xform::Rotate {
                    center: self.pivot.truncate(),
                    degrees,
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
                self.shown = format!("{}: × {factor:.2}", self.edit.label);
                Xform::Scale {
                    center: self.pivot,
                    factor,
                }
            }
        })
    }

    fn apply(&mut self, doc: &mut Document, t: &Xform) -> Result<(), String> {
        let original = self.edit.originals()[0].clone();
        let changed = match (self.face, &original.kind) {
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
                }
                let mut e = original.clone();
                e.kind = ElementKind::FreeMesh(m);
                e
            }
            _ => transform::transformed(doc.model(), &original, t)
                .ok_or("this element can't take that change")?,
        };
        self.edit
            .update(doc, vec![changed])
            .map_err(|e| e.to_string())
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
        painter.circle_stroke(ps, 4.0, Stroke::new(1.5, accent));
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

/// The copies a Shift+D makes, inserted as one undo step. Returns the copy
/// to keep moving.
pub fn duplicate(doc: &mut Document, id: ElementId) -> Result<ElementId, String> {
    let copies = transform::duplicate(doc.model(), id);
    let first = copies
        .first()
        .map(|c| c.id)
        .ok_or("That can't be duplicated.")?;
    let name = doc
        .model()
        .get(id)
        .map(|e| e.name.clone())
        .unwrap_or_default();
    let mut tx = Transaction::new(format!("Duplicate {name}"));
    for c in copies {
        tx = tx.insert(c);
    }
    doc.commit(tx).map_err(|e| e.to_string())?;
    Ok(first)
}
