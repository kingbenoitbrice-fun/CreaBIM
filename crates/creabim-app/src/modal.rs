//! Edits that follow the pointer: the Move and Rotate tools, which take a
//! base point and then a destination, as in Revit or AutoCAD; Blender-style
//! move (G), rotate (R) and scale (S) from the keyboard; and dragging grips
//! and elements in plans. The same code serves the 3D view, plans and
//! sections; each pane only says how its pointer maps onto the model and
//! where it snaps.
//!
//! The Move tool goes: pick what to move (if nothing is selected), click a
//! base point (it snaps), then the selection follows the pointer with a
//! rubber band from the base point until a click places it. Nothing moves
//! before the base point is picked. Esc or right-click steps back a stage.

use crate::handles::SnapFn;
use crate::snaps::{Snap, SnapKind};
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
    /// The best snap near the pointer, as a model point. With a start
    /// point (the base point) it also tracks the axes through it; without
    /// one it lands on what is under the pointer (a face in 3D).
    pub snap: &'a SnapFn<'a>,
}

/// How the edit was started, which decides how it takes clicks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Flow {
    /// From a key over a view (G, R, S), Blender style: it follows the
    /// pointer at once, from the point under it.
    #[default]
    Keys,
    /// From the Move or Rotate tool (a button, a menu or M): click a base
    /// point first, then where it goes.
    Tool,
}

/// Where a pointer edit is in its clicks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Waiting for the base point (or a rotation's centre). Nothing has
    /// moved yet.
    Base,
    /// A rotation: centre picked, waiting for the direction to turn from.
    Reference,
    /// Following the pointer from the base point.
    Follow,
}

/// One frame's input for an edit, as read from the pane.
#[derive(Clone, Debug, Default)]
pub struct Events {
    /// The pointer over the pane, if it is there.
    pub pointer: Option<Pos2>,
    /// A left click (not the end of a drag) in the pane.
    pub click: bool,
    /// A right click (not the end of a pan) in the pane.
    pub right: bool,
    pub enter: bool,
    pub escape: bool,
    pub backspace: bool,
    /// Ctrl went down this frame.
    pub ctrl_pressed: bool,
    /// Ctrl is held.
    pub ctrl: bool,
    /// Shift is held: keep square to the pane's axes.
    pub shift: bool,
    /// Keys typed: x, y, z, digits.
    pub text: String,
}

impl Events {
    /// Read the frame's keys, with the pane's pointer and clicks.
    pub fn read(
        ctx: &egui::Context,
        pointer: Option<Pos2>,
        click: bool,
        right: bool,
        ctrl_before: &mut bool,
    ) -> Self {
        let typing = ctx.egui_wants_keyboard_input();
        let mut ev = ctx.input(|i| Events {
            pointer,
            click,
            right,
            enter: !typing && i.key_pressed(egui::Key::Enter),
            escape: i.key_pressed(egui::Key::Escape),
            backspace: !typing && i.key_pressed(egui::Key::Backspace),
            ctrl_pressed: false,
            ctrl: i.modifiers.ctrl || i.modifiers.mac_cmd,
            shift: i.modifiers.shift,
            text: if typing {
                String::new()
            } else {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect()
            },
        });
        ev.ctrl_pressed = ev.ctrl && !*ctrl_before;
        *ctrl_before = ev.ctrl;
        ev
    }
}

/// Copies following the pointer, for Move with Ctrl: each click leaves one
/// behind, as its own undo step.
#[derive(Debug)]
struct CopyPreview {
    label: String,
    /// Fresh copies, where the originals are.
    copies: Vec<Element>,
    /// Which copies move (a door in a copied wall goes with its wall).
    moves: Vec<bool>,
    applied: bool,
}

impl CopyPreview {
    fn new(doc: &Document, originals: &[Element]) -> Option<Self> {
        let model = doc.model();
        let ids: Vec<ElementId> = originals.iter().map(|e| e.id).collect();
        let set = transform::copy_set(model, &ids);
        let elements: Vec<Element> = set
            .iter()
            .filter_map(|id| model.get(*id))
            .cloned()
            .collect();
        let (mut copies, _) = transform::fresh_copies(model, &elements);
        if copies.len() != elements.len() || copies.is_empty() {
            return None;
        }
        let copied: Vec<ElementId> = copies.iter().map(|c| c.id).collect();
        let mut moves = Vec::with_capacity(copies.len());
        for (c, o) in copies.iter_mut().zip(&elements) {
            if ids.contains(&o.id) {
                c.name = format!("{} copy", c.name);
            }
            moves.push(match &c.kind {
                ElementKind::Opening(op) => !copied.contains(&op.host),
                ElementKind::Cut(_) => false,
                _ => true,
            });
        }
        let what = match ids.len() {
            1 => originals[0].name.clone(),
            n => format!("{n} elements"),
        };
        Some(Self {
            label: format!("Copy {what}"),
            copies,
            moves,
            applied: false,
        })
    }

    fn update(&mut self, doc: &mut Document, d: DVec3) -> Result<(), String> {
        self.cancel(doc);
        let mut tx = Transaction::new(self.label.clone());
        for (c, moves) in self.copies.iter().zip(&self.moves) {
            let c = if *moves {
                transform::transformed(doc.model(), c, &Xform::Move(d))
                    .ok_or_else(|| format!("{} can't be copied there", c.name))?
            } else {
                c.clone()
            };
            tx = tx.insert(c);
        }
        doc.commit(tx).map_err(|e| e.to_string())?;
        self.applied = true;
        Ok(())
    }

    fn cancel(&mut self, doc: &mut Document) {
        if self.applied && doc.undo_label() == Some(self.label.as_str()) {
            doc.discard_last();
        }
        self.applied = false;
    }
}

/// How far the pointer must go from where a move began before anything
/// moves, in points: a click or a hand's tremor nudges nothing.
pub const STILL: f32 = 4.0;

/// A move, rotate or scale in progress, from a key or a tool.
#[derive(Debug)]
pub struct Modal {
    pub mode: Mode,
    /// A mesh face being edited instead of the whole element.
    pub face: Option<usize>,
    pub space: Space,
    /// The pane that runs it: a view, or `None` for the 3D view.
    pub pane: Option<ElementId>,
    pub flow: Flow,
    stage: Stage,
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
    /// Finish or Cancel pressed on the on-screen panel.
    ask: Option<bool>,
    /// The base point things move by (or a rotation's centre), and where
    /// the pointer was when it was picked.
    base: Option<DVec3>,
    base_at: Option<Pos2>,
    /// A rotation's reference point: the direction it turns from.
    reference: Option<DVec3>,
    /// The snap under the pointer this frame, for its marker.
    snap: Option<Snap>,
    /// Where the base point goes this frame.
    target: Option<DVec3>,
    /// Shift held: square to the pane's axes.
    ortho: bool,
    /// Leave copies behind instead of moving (Ctrl).
    pub copy: bool,
    copies: Option<CopyPreview>,
    /// Copies placed so far.
    placed: usize,
    /// The last move applied, for the readout and tests.
    delta: DVec3,
    ctrl_before: bool,
    /// Copy switched on the on-screen panel.
    want_copy: Option<bool>,
}

pub enum Outcome {
    Running(String),
    Done(String),
    Cancelled(String),
}

/// Parse a typed length in metres: "2.4", "2400mm", "240cm", "2.4m".
pub fn parse_length(s: &str) -> Option<f64> {
    let s = s.trim();
    let (num, scale) = if let Some(n) = s.strip_suffix("mm") {
        (n, 0.001)
    } else if let Some(n) = s.strip_suffix("cm") {
        (n, 0.01)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 1.0)
    } else {
        (s, 1.0)
    };
    num.trim().parse::<f64>().ok().map(|v| v * scale)
}

/// The move from `base` to `target`, kept to `axis`, square to the pane
/// (`ortho`: the larger of the two axes), level unless `axis` is Z when
/// `level` (in 3D things slide on their level), and `typed` long when a
/// length was typed (along the way the pointer goes, or `fallback`).
pub fn move_delta(
    base: DVec3,
    target: DVec3,
    axis: Option<Axis>,
    ortho: Option<[DVec3; 2]>,
    typed: Option<f64>,
    fallback: DVec3,
    level: bool,
) -> DVec3 {
    let mut d = target - base;
    if level && axis != Some(Axis::Z) {
        d.z = 0.0;
    }
    if let Some(a) = axis {
        d = a.unit() * d.dot(a.unit());
    } else if let Some([u, v]) = ortho {
        d = if d.dot(u).abs() >= d.dot(v).abs() {
            u * d.dot(u)
        } else {
            v * d.dot(v)
        };
    }
    match typed {
        Some(len) => {
            let dir = match axis {
                Some(a) if d.length() < 1e-9 => a.unit(),
                _ => d.normalize_or(fallback),
            };
            dir * len
        }
        None => d,
    }
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
        let blank = |pivot: DVec3, own: Vec<DVec3>, edit: LiveEdit| Modal {
            mode,
            face: None,
            space,
            pane,
            flow: Flow::Keys,
            stage: Stage::Follow,
            pivot,
            own,
            individual: false,
            grab,
            pointer: grab,
            axis: None,
            typed: String::new(),
            edit,
            duplicated: false,
            shown: String::new(),
            skipped: 0,
            ask: None,
            base: None,
            base_at: None,
            reference: None,
            snap: None,
            target: None,
            ortho: false,
            copy: false,
            copies: None,
            placed: 0,
            delta: DVec3::ZERO,
            ctrl_before: false,
            want_copy: None,
        };
        if let (Some(f), ElementKind::FreeMesh(m)) = (face, &first.kind) {
            if f < m.mesh.faces.len() {
                let z = m
                    .level
                    .and_then(|l| model.level(l))
                    .map_or(0.0, |l| l.elevation);
                let c = m.origin + m.to_model(m.mesh.face_center(f)) + DVec3::Z * z;
                let label = format!("{} face", mode.verb());
                let mut m = blank(c, vec![c], LiveEdit::new(label, vec![first.clone()]));
                m.face = face;
                return Ok(m);
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
        let individual = pivot == Pivot::Individual && kept.len() > 1;
        let mut m = blank(
            center,
            own,
            LiveEdit::new(
                format!("{} {what}", mode.verb()),
                kept.into_iter().map(|k| k.0).collect(),
            ),
        );
        m.individual = individual;
        m.skipped = skipped;
        Ok(m)
    }

    /// Take clicks as the Move or Rotate tool does: a base point first.
    pub fn with_flow(mut self, flow: Flow) -> Self {
        self.flow = flow;
        if flow == Flow::Tool && self.uses_points() {
            self.stage = Stage::Base;
        }
        self
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// The elements being edited.
    pub fn ids(&self) -> Vec<ElementId> {
        self.edit.originals().iter().map(|e| e.id).collect()
    }

    /// The move applied this frame.
    #[cfg(test)]
    pub fn delta(&self) -> DVec3 {
        self.delta
    }

    /// Does this edit work from picked points (a base point, a target)
    /// rather than from how far the pointer went?
    fn uses_points(&self) -> bool {
        match self.mode {
            Mode::Move => true,
            Mode::Rotate => self.flow == Flow::Tool && self.face.is_none(),
            Mode::Scale => false,
        }
    }

    /// What the pane's snaps should leave out: the moving elements once
    /// they follow the pointer (else they would snap to themselves), and
    /// nothing while a base point is picked (it may be on them). `None`
    /// keeps the pane's own choice.
    pub fn snap_exclude(&self, doc: &Document) -> Option<Vec<ElementId>> {
        if !self.uses_points() {
            return None;
        }
        Some(match self.stage {
            Stage::Base | Stage::Reference => vec![],
            Stage::Follow => {
                let mut ids = self.ids();
                for id in self.ids() {
                    ids.extend(doc.model().dependents(id));
                }
                ids
            }
        })
    }

    /// Snaps stay as they were while things follow the pointer.
    pub fn freezes_snaps(&self) -> bool {
        self.stage == Stage::Follow
    }

    /// The pane's own axes, for Shift.
    fn pane_axes(&self) -> [DVec3; 2] {
        match self.space {
            Space::Section { start, end } => {
                [(end - start).normalize_or(DVec2::X).extend(0.0), DVec3::Z]
            }
            Space::Plan | Space::Model => [DVec3::X, DVec3::Y],
        }
    }

    /// Follow the pointer and keys for one frame, drawing the feedback.
    /// `click` and `right` are the pane's own clicks (not drags or pans).
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        doc: &mut Document,
        ctx: &egui::Context,
        painter: &egui::Painter,
        pointer: Option<Pos2>,
        click: bool,
        right: bool,
        map: &PaneMap,
    ) -> Outcome {
        let mut before = self.ctrl_before;
        let ev = Events::read(ctx, pointer, click, right, &mut before);
        self.ctrl_before = before;
        let out = self.step(doc, &ev, map);
        self.draw(painter, map);
        out
    }

    /// One frame of the edit, without drawing.
    pub fn step(&mut self, doc: &mut Document, ev: &Events, map: &PaneMap) -> Outcome {
        if let Some(p) = ev.pointer {
            self.pointer = p;
        }
        self.ortho = ev.shift;
        let follow = self.stage == Stage::Follow;
        for c in ev.text.chars() {
            match c.to_ascii_lowercase() {
                'x' => self.toggle(Axis::X),
                'y' => self.toggle(Axis::Y),
                'z' => self.toggle(Axis::Z),
                c @ ('0'..='9' | '.' | '-' | ',') if follow => {
                    self.typed.push(if c == ',' { '.' } else { c })
                }
                c @ ('m' | 'c') if follow && !self.typed.is_empty() => self.typed.push(c),
                _ => {}
            }
        }
        if ev.backspace {
            self.typed.pop();
        }
        // Ctrl leaves copies behind
        if ev.ctrl_pressed && self.mode == Mode::Move && self.face.is_none() && !self.duplicated {
            self.set_copy(doc, !self.copy);
        }
        if let Some(on) = self.want_copy.take() {
            self.set_copy(doc, on);
        }
        if ev.escape || ev.right || self.ask == Some(false) {
            self.ask = None;
            if ev.escape && !self.typed.is_empty() {
                self.typed.clear();
            } else if let Some(out) = self.back(doc) {
                return out;
            }
        }
        if !self.uses_points() {
            return self.step_relative(doc, ev, map);
        }
        let skipped = self.skipped_note();
        match self.stage {
            Stage::Base => {
                self.snap = ev.pointer.and_then(|p| (map.snap)(p, None)).map(|s| s.1);
                let picked = self
                    .snap
                    .map(|s| s.p)
                    .or_else(|| ev.pointer.and_then(|p| (map.to_model)(p, None, self.pivot)));
                if ev.click {
                    if let Some(b) = picked {
                        self.base = Some(b);
                        self.base_at = ev.pointer;
                        self.stage = if self.mode == Mode::Rotate {
                            Stage::Reference
                        } else {
                            Stage::Follow
                        };
                    }
                }
                let what = self.edit.label().to_string();
                Outcome::Running(match self.mode {
                    Mode::Rotate if self.stage == Stage::Reference => format!(
                        "{what}: click the direction to turn from (a reference point).{skipped}"
                    ),
                    Mode::Rotate => format!(
                        "{what}: click the centre to turn about. It snaps to ends, middles, crossings and grid lines. Esc cancels.{skipped}"
                    ),
                    _ if self.stage == Stage::Follow => {
                        format!("{what}: now move the pointer to where the base point goes.{skipped}")
                    }
                    _ => format!(
                        "{what}: click the base point, the point you move it by. It snaps to ends, middles, crossings, grid lines{}. Shift+click adds or takes away elements. Esc cancels.{skipped}",
                        if self.space == Space::Model { " and faces" } else { "" }
                    ),
                })
            }
            Stage::Reference => {
                let base = self.base.unwrap_or(self.pivot);
                self.snap = ev
                    .pointer
                    .and_then(|p| (map.snap)(p, Some(base)))
                    .map(|s| s.1);
                self.target = self.snap.map(|s| s.p);
                if ev.click {
                    if let Some(r) = self.target.filter(|r| self.flat(*r - base).length() > 1e-6) {
                        self.reference = Some(r);
                        self.base_at = ev.pointer;
                        self.stage = Stage::Follow;
                    }
                }
                Outcome::Running(format!(
                    "{}: click the direction to turn from (a reference point). Esc goes back to the centre.{skipped}",
                    self.edit.label()
                ))
            }
            Stage::Follow => self.step_follow(doc, ev, map),
        }
    }

    fn skipped_note(&self) -> String {
        match self.skipped {
            0 => String::new(),
            1 => " (1 selected element can't take this and stays put)".into(),
            n => format!(" ({n} selected elements can't take this and stay put)"),
        }
    }

    /// A vector in the plane things turn in: plan for plans and 3D, the
    /// section plane for sections, as (across, up).
    fn flat(&self, v: DVec3) -> DVec2 {
        match self.space {
            Space::Section { start, end } => {
                DVec2::new(v.truncate().dot((end - start).normalize_or(DVec2::X)), v.z)
            }
            Space::Plan | Space::Model => v.truncate(),
        }
    }

    /// Following the pointer from the base point.
    fn step_follow(&mut self, doc: &mut Document, ev: &Events, map: &PaneMap) -> Outcome {
        let skipped = self.skipped_note();
        // a key move starts from the point under the pointer, snapped
        if self.base.is_none() {
            let p = ev.pointer.unwrap_or(self.grab);
            let b = (map.snap)(p, None)
                .map(|s| s.0)
                .or_else(|| (map.to_model)(p, None, self.pivot))
                .unwrap_or(self.pivot);
            self.base = Some(b);
            self.base_at = Some(p);
        }
        let base = self.base.expect("just set");
        let still = self
            .base_at
            .is_some_and(|b| b.distance(self.pointer) < STILL);
        // where the base point goes: snapped, or up and down along Z
        let (target, snap) = match self.axis {
            Some(Axis::Z) if self.space == Space::Model => {
                ((map.to_model)(self.pointer, Some(Axis::Z), base), None)
            }
            _ => match (map.snap)(self.pointer, Some(base)) {
                Some((p, s)) => (Some(p), Some(s)),
                None => ((map.to_model)(self.pointer, self.axis, base), None),
            },
        };
        self.snap = snap;
        let typed = parse_length(&self.typed);
        let ortho = (self.ortho && self.axis.is_none()).then(|| self.pane_axes());
        let t = match self.mode {
            Mode::Move => {
                let fallback = self.pane_axes()[0];
                let d = match target {
                    _ if still && typed.is_none() => DVec3::ZERO,
                    Some(t) => {
                        let level = self.space == Space::Model;
                        move_delta(base, t, self.axis, ortho, typed, fallback, level)
                    }
                    None => self.delta,
                };
                self.target = Some(base + d);
                self.delta = d;
                let axis = match (self.axis, ortho.is_some()) {
                    (Some(a), _) => format!(" along {}", a.label()),
                    (None, true) => " square".into(),
                    _ => String::new(),
                };
                let label = match &self.copies {
                    Some(c) if self.copy => c.label.clone(),
                    _ => self.edit.label().to_string(),
                };
                self.shown = format!("{label}: {:.3} m{axis}", d.length());
                Xform::Move(d)
            }
            _ => {
                // a rotation from the reference direction to the pointer's
                let r = self.reference.unwrap_or(base + DVec3::X);
                let a0 = self.flat(r - base);
                let a1 = target.map_or(a0, |t| self.flat(t - base));
                let mut deg = a0.angle_to(a1).to_degrees();
                if !snap.is_some_and(|s| s.kind.is_point()) {
                    let step = if ev.ctrl { 15.0 } else { 1.0 };
                    deg = (deg / step).round() * step;
                }
                if still || !deg.is_finite() {
                    deg = 0.0;
                }
                if let Ok(v) = self.typed.trim().parse::<f64>() {
                    deg = v;
                }
                self.target = target;
                self.shown = format!("{}: {deg:.1}°", self.edit.label());
                match self.space {
                    Space::Section { start, end } => Xform::Tilt {
                        center: base,
                        axis: -(end - start).normalize_or_zero().perp().extend(0.0),
                        degrees: deg,
                    },
                    _ => Xform::Rotate {
                        center: base.truncate(),
                        degrees: deg,
                    },
                }
            }
        };
        if let Err(e) = self.apply(doc, &t) {
            self.shown = format!("Can't go there: {e}");
        }
        if ev.click || ev.enter || self.ask == Some(true) {
            self.ask = None;
            return self.place(doc);
        }
        let copy = if self.mode == Mode::Move && self.face.is_none() && !self.duplicated {
            if self.copy {
                ", Ctrl: copy is on"
            } else {
                ", Ctrl: copy"
            }
        } else {
            ""
        };
        let keys = match self.mode {
            Mode::Move => "type a distance + Enter, Shift keeps square, X, Y or Z keeps to an axis",
            _ => "type an angle + Enter, Ctrl snaps to 15°",
        };
        let snapped = match snap.map(|s| s.kind) {
            Some(k) if k != SnapKind::Grid => format!(" ({})", k.label().to_lowercase()),
            _ => String::new(),
        };
        let typed = if self.typed.is_empty() {
            String::new()
        } else {
            format!("  [typed {}]", self.typed)
        };
        Outcome::Running(format!(
            "{}{typed}{snapped}{skipped}   ·   click to place, {keys}{copy}, Esc steps back",
            self.shown
        ))
    }

    /// Leave things where they are now: the end of the edit, or one more
    /// copy placed.
    fn place(&mut self, doc: &mut Document) -> Outcome {
        if self.copy {
            if let Some(c) = &mut self.copies {
                if c.applied {
                    c.applied = false;
                    self.placed += 1;
                    // the next copy has ids of its own
                    self.copies = CopyPreview::new(doc, self.edit.originals());
                    self.typed.clear();
                    return Outcome::Running(format!(
                        "{} placed. Click to place another, Esc to finish. Ctrl+Z undoes one at a time.",
                        match self.placed {
                            1 => "1 copy".to_string(),
                            n => format!("{n} copies"),
                        }
                    ));
                }
            }
            return self.finish(doc);
        }
        let done = format!("{}{}. Ctrl+Z to undo.", self.shown, self.skipped_note());
        Outcome::Done(done)
    }

    /// Done with copies, or cancelled.
    fn finish(&mut self, doc: &mut Document) -> Outcome {
        if self.placed > 0 {
            self.edit.cancel(doc);
            if let Some(c) = &mut self.copies {
                c.cancel(doc);
            }
            Outcome::Done(format!(
                "Placed {} {}. Ctrl+Z undoes them one at a time.",
                self.placed,
                if self.placed == 1 { "copy" } else { "copies" }
            ))
        } else {
            self.cancel(doc);
            Outcome::Cancelled(format!("{} cancelled.", self.mode.verb()))
        }
    }

    /// Esc or right-click: take back the last click, or stop. `None` while
    /// the edit goes on.
    fn back(&mut self, doc: &mut Document) -> Option<Outcome> {
        if self.flow == Flow::Keys || !self.uses_points() {
            return Some(self.finish(doc));
        }
        match self.stage {
            Stage::Follow => {
                self.edit.cancel(doc);
                if let Some(c) = &mut self.copies {
                    c.cancel(doc);
                }
                self.typed.clear();
                self.delta = DVec3::ZERO;
                self.target = None;
                if self.mode == Mode::Rotate {
                    self.reference = None;
                    self.stage = Stage::Reference;
                } else {
                    self.base = None;
                    self.stage = Stage::Base;
                }
                None
            }
            Stage::Reference => {
                self.base = None;
                self.stage = Stage::Base;
                None
            }
            Stage::Base => Some(self.finish(doc)),
        }
    }

    /// Turn copying on or off, putting back what the other way showed.
    pub fn set_copy(&mut self, doc: &mut Document, on: bool) {
        if on == self.copy {
            return;
        }
        if on {
            self.edit.cancel(doc);
            if self.copies.is_none() {
                self.copies = CopyPreview::new(doc, self.edit.originals());
            }
            self.copy = self.copies.is_some();
        } else {
            if let Some(c) = &mut self.copies {
                c.cancel(doc);
            }
            self.copy = false;
        }
    }

    /// Keyboard rotate and scale: by how far the pointer went.
    fn step_relative(&mut self, doc: &mut Document, ev: &Events, map: &PaneMap) -> Outcome {
        let xform = self.xform(map, ev.ctrl);
        let result = match xform {
            Some(t) => self.apply(doc, &t),
            None => Ok(()),
        };
        if let Err(e) = result {
            self.shown = format!("Can't go there: {e}");
        }
        let skipped = self.skipped_note();
        if ev.enter || ev.click || self.ask == Some(true) {
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

    /// The on-screen panel while the edit runs, for working without the
    /// keyboard: axis buttons, a value box, Finish and Cancel.
    pub fn panel(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        egui::Area::new(egui::Id::new("modal_panel"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(rect.center_bottom() - egui::vec2(0.0, 28.0))
            .constrain_to(rect.shrink(4.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    // in a narrow view the panel wraps onto two rows
                    ui.set_max_width((rect.width() - 40.0).max(160.0));
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(self.mode.verb()).strong());
                        ui.separator();
                        let axes: &[Axis] = match (self.mode, self.face.is_some()) {
                            (Mode::Rotate, true) => &[],
                            _ => &Axis::ALL,
                        };
                        if !axes.is_empty() {
                            if ui.selectable_label(self.axis.is_none(), "Free").clicked() {
                                self.axis = None;
                            }
                            for a in axes {
                                let name = match (self.mode, a) {
                                    (Mode::Rotate, Axis::X) => "Tilt X",
                                    (Mode::Rotate, Axis::Y) => "Tilt Y",
                                    (Mode::Rotate, Axis::Z) => "Turn",
                                    (_, Axis::X) => "X",
                                    (_, Axis::Y) => "Y",
                                    (_, Axis::Z) => "Z",
                                };
                                if ui.selectable_label(self.axis == Some(*a), name).clicked() {
                                    self.toggle(*a);
                                }
                            }
                            ui.separator();
                        }
                        ui.label(match self.mode {
                            Mode::Move => "Distance",
                            Mode::Rotate => "Angle",
                            Mode::Scale => "Factor",
                        });
                        ui.add(
                            egui::TextEdit::singleline(&mut self.typed)
                                .desired_width(60.0)
                                .hint_text("type"),
                        );
                        if self.mode == Mode::Move && self.face.is_none() && !self.duplicated {
                            ui.separator();
                            if ui
                                .selectable_label(self.copy, "Copy (Ctrl)")
                                .on_hover_text("Leave copies behind: each click places one")
                                .clicked()
                            {
                                self.want_copy = Some(!self.copy);
                            }
                        }
                        ui.separator();
                        let ready = self.stage == Stage::Follow;
                        if ui
                            .add_enabled(ready, egui::Button::new("✔ Finish"))
                            .clicked()
                        {
                            self.ask = Some(true);
                        }
                        if ui.button("✖ Cancel").clicked() {
                            self.ask = Some(false);
                        }
                    });
                });
            });
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
        if let Some(c) = &mut self.copies {
            c.cancel(doc);
        }
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
                        // measure the pointer's travel at the pivot, not where
                        // the pointer happened to be when the move began: a
                        // move started from a button begins wherever the
                        // pointer comes into the view, often near the horizon
                        let (from, to) = match (map.project)(self.pivot) {
                            Some(p) => (p, p + (self.pointer - self.grab)),
                            None => (self.grab, self.pointer),
                        };
                        let a = (map.to_model)(from, self.axis, self.pivot)?;
                        let b = (map.to_model)(to, self.axis, self.pivot)?;
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
        if let (true, Some(c), Xform::Move(d)) = (self.copy, &mut self.copies, t) {
            return c.update(doc, *d);
        }
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
        if self.uses_points() {
            return self.draw_points(painter, map);
        }
        let Some(ps) = (map.project)(self.pivot) else {
            return;
        };
        if let Some(a) = self.axis {
            draw_axis(painter, map, self.pivot, a);
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
        painter.add(egui::Shape::dashed_line(
            &[ps, self.pointer],
            Stroke::new(1.0, accent),
            6.0,
            4.0,
        ));
    }

    /// The base point, the rubber band to where it goes, and the snap
    /// marker under the pointer.
    fn draw_points(&self, painter: &egui::Painter, map: &PaneMap) {
        let accent = Color32::from_rgb(230, 120, 30);
        let stroke = Stroke::new(1.5, accent);
        let marker = |s: Snap, from: Option<Pos2>| {
            if let Some(at) = (map.project)(s.p) {
                if s.kind == SnapKind::Grid {
                    painter.circle_stroke(at, 4.0, Stroke::new(1.5, accent));
                } else {
                    crate::snaps::draw(painter, s, at, from);
                }
            }
        };
        let base = self.base.and_then(|b| (map.project)(b));
        if let (Some(a), Some(b)) = (self.axis, self.base) {
            draw_axis(painter, map, b, a);
        }
        match self.stage {
            Stage::Base => {
                if let Some(s) = self.snap {
                    marker(s, None);
                }
            }
            Stage::Reference => {
                if let (Some(b), Some(t)) = (base, self.target.and_then(|t| (map.project)(t))) {
                    painter.add(egui::Shape::dashed_line(&[b, t], stroke, 6.0, 4.0));
                }
                if let Some(s) = self.snap {
                    marker(s, base);
                }
            }
            Stage::Follow => {
                let to = self.target.and_then(|t| (map.project)(t));
                if self.mode == Mode::Rotate {
                    // the reference ray, and the ray to the pointer
                    if let (Some(b), Some(r)) =
                        (base, self.reference.and_then(|r| (map.project)(r)))
                    {
                        painter.line_segment([b, r], Stroke::new(1.0, accent.gamma_multiply(0.6)));
                    }
                    if let (Some(b), Some(t)) = (base, to) {
                        painter.add(egui::Shape::dashed_line(&[b, t], stroke, 6.0, 4.0));
                    }
                } else if let (Some(b), Some(t)) = (base, to) {
                    // the rubber band, with its length at its middle
                    painter.add(egui::Shape::dashed_line(&[b, t], stroke, 6.0, 4.0));
                    if b.distance(t) > 30.0 {
                        let side = (t - b).normalized().rot90() * 14.0;
                        let side = if side.y > 0.0 { -side } else { side };
                        painter.text(
                            b + (t - b) / 2.0 + side,
                            egui::Align2::CENTER_CENTER,
                            format!("{:.3} m", self.delta.length()),
                            egui::FontId::proportional(13.0),
                            accent,
                        );
                    }
                    painter.circle_filled(t, 3.0, accent);
                }
                if let Some(s) = self.snap.filter(|s| s.kind != SnapKind::Grid) {
                    marker(s, base);
                }
            }
        }
        if let Some(b) = base {
            // the base point: a small cross in a circle
            painter.circle_stroke(b, 5.0, stroke);
            painter.line_segment([b - egui::vec2(8.0, 0.0), b + egui::vec2(8.0, 0.0)], stroke);
            painter.line_segment([b - egui::vec2(0.0, 8.0), b + egui::vec2(0.0, 8.0)], stroke);
        }
    }
}

/// The line through `at` along axis `a`, in the axis' colour.
fn draw_axis(painter: &egui::Painter, map: &PaneMap, at: DVec3, a: Axis) {
    let color = match a {
        Axis::X => Color32::from_rgb(210, 60, 60),
        Axis::Y => Color32::from_rgb(60, 160, 60),
        Axis::Z => Color32::from_rgb(60, 100, 220),
    };
    let far = |s: f64| (map.project)(at + a.unit() * s);
    if let (Some(p), Some(q)) = (far(-200.0), far(200.0)) {
        painter.line_segment([p, q], Stroke::new(1.5, color));
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

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    /// A plan-like pane: 10 points to the metre, y down, with endpoint
    /// snaps at `points` (within half a metre) and free points elsewhere.
    fn with_map<R>(points: &[DVec3], f: impl FnOnce(&PaneMap) -> R) -> R {
        let to_screen = |p: DVec3| Pos2::new((p.x * 10.0) as f32, (-p.y * 10.0) as f32);
        let to_world = |p: Pos2| DVec3::new(p.x as f64 / 10.0, -p.y as f64 / 10.0, 0.0);
        let to_model = |p: Pos2, _: Option<Axis>, pivot: DVec3| Some(to_world(p).with_z(pivot.z));
        let project = |p: DVec3| Some(to_screen(p));
        let pts = points.to_vec();
        let snap = move |p: Pos2, from: Option<DVec3>| {
            let w = to_world(p);
            let z = from.map_or(0.0, |f| f.z);
            if let Some(e) = pts
                .iter()
                .find(|e| e.truncate().distance(w.truncate()) < 0.5)
            {
                let s = Snap {
                    p: e.with_z(z),
                    kind: SnapKind::Endpoint,
                    host: None,
                };
                return Some((s.p, s));
            }
            let s = Snap {
                p: w.with_z(z),
                kind: SnapKind::Grid,
                host: None,
            };
            Some((s.p, s))
        };
        let map = PaneMap {
            to_model: &to_model,
            project: &project,
            look: -DVec3::Z,
            snap: &snap,
        };
        f(&map)
    }

    fn at(x: f64, y: f64) -> Pos2 {
        Pos2::new((x * 10.0) as f32, (-y * 10.0) as f32)
    }

    fn pointer(p: Pos2) -> Events {
        Events {
            pointer: Some(p),
            ..Default::default()
        }
    }

    fn click(p: Pos2) -> Events {
        Events {
            pointer: Some(p),
            click: true,
            ..Default::default()
        }
    }

    fn first_wall(doc: &Document) -> (ElementId, DVec2) {
        doc.model()
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some((e.id, w.start)),
                _ => None,
            })
            .unwrap()
    }

    fn wall_start(doc: &Document, id: ElementId) -> DVec2 {
        match &doc.model().get(id).unwrap().kind {
            ElementKind::Wall(w) => w.start,
            _ => unreachable!(),
        }
    }

    fn tool(doc: &Document, mode: Mode, id: ElementId) -> Modal {
        Modal::start(
            doc,
            mode,
            &[id],
            None,
            Space::Plan,
            None,
            Pos2::ZERO,
            Pivot::Median,
            None,
        )
        .unwrap()
        .with_flow(Flow::Tool)
    }

    #[test]
    fn nothing_moves_before_the_base_point() {
        let mut doc = Document::from_model(sample_house());
        let (id, start) = first_wall(&doc);
        let mut m = tool(&doc, Mode::Move, id);
        let steps = doc.undo_steps().len();
        with_map(&[], |map| {
            assert_eq!(m.stage(), Stage::Base);
            for p in [at(1.0, 1.0), at(5.0, -3.0), at(20.0, 9.0)] {
                assert!(matches!(
                    m.step(&mut doc, &pointer(p), map),
                    Outcome::Running(_)
                ));
                assert_eq!(wall_start(&doc, id), start);
            }
            assert_eq!(doc.undo_steps().len(), steps);
            // the base click itself moves nothing either
            m.step(&mut doc, &click(at(2.0, 2.0)), map);
            assert_eq!(m.stage(), Stage::Follow);
            assert_eq!(wall_start(&doc, id), start);
            // nor does a pointer that hasn't left the base point
            m.step(&mut doc, &pointer(at(2.0, 2.0) + egui::vec2(1.0, 1.0)), map);
            assert_eq!(wall_start(&doc, id), start);
        });
        assert_eq!(doc.undo_steps().len(), steps);
    }

    #[test]
    fn the_base_point_goes_to_the_snapped_destination() {
        let mut doc = Document::from_model(sample_house());
        let (id, start) = first_wall(&doc);
        let mut m = tool(&doc, Mode::Move, id);
        let base = DVec3::new(2.0, 2.0, 0.0);
        let target = DVec3::new(7.0, 4.0, 0.0);
        let steps = doc.undo_steps().len();
        with_map(&[base, target], |map| {
            // near the points, not on them: they snap
            m.step(&mut doc, &click(at(2.2, 1.9)), map);
            m.step(&mut doc, &pointer(at(6.8, 4.3)), map);
            assert_eq!(wall_start(&doc, id), start + DVec2::new(5.0, 2.0));
            assert!(matches!(
                m.step(&mut doc, &click(at(6.8, 4.3)), map),
                Outcome::Done(_)
            ));
        });
        assert_eq!(wall_start(&doc, id), start + DVec2::new(5.0, 2.0));
        // one move, one undo step
        assert_eq!(doc.undo_steps().len(), steps + 1);
        doc.undo();
        assert_eq!(wall_start(&doc, id), start);
    }

    #[test]
    fn a_typed_distance_goes_the_way_the_pointer_points() {
        let mut doc = Document::from_model(sample_house());
        let (id, start) = first_wall(&doc);
        let mut m = tool(&doc, Mode::Move, id);
        with_map(&[], |map| {
            m.step(&mut doc, &click(at(0.0, 0.0)), map);
            m.step(&mut doc, &pointer(at(0.0, 3.3)), map);
            let typed = Events {
                text: "2400mm".into(),
                ..pointer(at(0.0, 3.3))
            };
            m.step(&mut doc, &typed, map);
            assert!((m.delta() - DVec3::new(0.0, 2.4, 0.0)).length() < 1e-9);
            let enter = Events {
                enter: true,
                ..pointer(at(0.0, 3.3))
            };
            assert!(matches!(m.step(&mut doc, &enter, map), Outcome::Done(_)));
        });
        assert!((wall_start(&doc, id) - (start + DVec2::new(0.0, 2.4))).length() < 1e-9);
    }

    #[test]
    fn shift_keeps_square_and_an_axis_key_keeps_to_it() {
        let d = move_delta(
            DVec3::ZERO,
            DVec3::new(3.0, 1.0, 0.0),
            None,
            Some([DVec3::X, DVec3::Y]),
            None,
            DVec3::X,
            false,
        );
        assert_eq!(d, DVec3::new(3.0, 0.0, 0.0));
        let d = move_delta(
            DVec3::ZERO,
            DVec3::new(3.0, 1.0, 0.5),
            Some(Axis::Y),
            None,
            None,
            DVec3::X,
            false,
        );
        assert_eq!(d, DVec3::new(0.0, 1.0, 0.0));
        // in 3D things stay on their level unless kept to Z
        let d = move_delta(
            DVec3::ZERO,
            DVec3::new(3.0, 1.0, 2.0),
            None,
            None,
            None,
            DVec3::X,
            true,
        );
        assert_eq!(d, DVec3::new(3.0, 1.0, 0.0));
        assert_eq!(parse_length("120cm"), Some(1.2));
        assert_eq!(parse_length("3"), Some(3.0));
        assert_eq!(parse_length("-"), None);
    }

    #[test]
    fn ctrl_leaves_copies_behind_one_undo_step_each() {
        let mut doc = Document::from_model(sample_house());
        let (id, start) = first_wall(&doc);
        let walls = |doc: &Document| {
            doc.model()
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Wall(_)))
                .count()
        };
        let before = walls(&doc);
        let steps = doc.undo_steps().len();
        let mut m = tool(&doc, Mode::Move, id);
        with_map(&[], |map| {
            m.step(&mut doc, &click(at(0.0, 0.0)), map);
            let ctrl = Events {
                ctrl: true,
                ctrl_pressed: true,
                ..pointer(at(0.0, 0.0))
            };
            m.step(&mut doc, &ctrl, map);
            assert!(m.copy);
            for i in 1..=2 {
                let p = at(0.0, 3.0 * i as f64);
                m.step(&mut doc, &pointer(p), map);
                // the preview is a copy: the original stays
                assert_eq!(wall_start(&doc, id), start);
                assert_eq!(walls(&doc), before + i);
                assert!(matches!(
                    m.step(&mut doc, &click(p), map),
                    Outcome::Running(_)
                ));
            }
            // Esc leaves the follow stage, then the tool, keeping the copies
            let esc = Events {
                escape: true,
                ..pointer(at(5.0, 5.0))
            };
            assert!(matches!(m.step(&mut doc, &esc, map), Outcome::Running(_)));
            assert!(matches!(m.step(&mut doc, &esc, map), Outcome::Done(_)));
        });
        assert_eq!(walls(&doc), before + 2);
        assert_eq!(wall_start(&doc, id), start);
        assert_eq!(doc.undo_steps().len(), steps + 2);
        doc.undo();
        assert_eq!(walls(&doc), before + 1);
    }

    #[test]
    fn esc_steps_back_then_cancels_leaving_everything_as_it_was() {
        let mut doc = Document::from_model(sample_house());
        let (id, start) = first_wall(&doc);
        let steps = doc.undo_steps().len();
        let mut m = tool(&doc, Mode::Move, id);
        with_map(&[], |map| {
            m.step(&mut doc, &click(at(0.0, 0.0)), map);
            m.step(&mut doc, &pointer(at(4.0, 0.0)), map);
            assert_ne!(wall_start(&doc, id), start);
            let right = Events {
                right: true,
                ..pointer(at(4.0, 0.0))
            };
            // back to picking the base point, with the wall put back
            assert!(matches!(m.step(&mut doc, &right, map), Outcome::Running(_)));
            assert_eq!(m.stage(), Stage::Base);
            assert_eq!(wall_start(&doc, id), start);
            assert!(matches!(
                m.step(&mut doc, &right, map),
                Outcome::Cancelled(_)
            ));
        });
        assert_eq!(wall_start(&doc, id), start);
        assert_eq!(doc.undo_steps().len(), steps);
    }

    #[test]
    fn a_key_move_starts_at_the_pointer_without_a_jump() {
        let mut doc = Document::from_model(sample_house());
        let (id, start) = first_wall(&doc);
        let grab = at(3.0, 3.0);
        let mut m = Modal::start(
            &doc,
            Mode::Move,
            &[id],
            None,
            Space::Plan,
            None,
            grab,
            Pivot::Median,
            None,
        )
        .unwrap();
        // an endpoint near the pointer: the grabbed point snaps to it
        let corner = DVec3::new(3.3, 3.0, 0.0);
        with_map(&[corner], |map| {
            m.step(&mut doc, &pointer(grab), map);
            assert_eq!(wall_start(&doc, id), start);
            m.step(&mut doc, &pointer(at(8.0, 3.0)), map);
            let moved = start + DVec2::new(8.0 - 3.3, 0.0);
            assert!((wall_start(&doc, id) - moved).length() < 1e-6);
            assert!(matches!(
                m.step(&mut doc, &click(at(8.0, 3.0)), map),
                Outcome::Done(_)
            ));
        });
    }

    #[test]
    fn the_rotate_tool_turns_from_the_reference_to_the_pointer() {
        let mut doc = Document::from_model(sample_house());
        let (id, _) = first_wall(&doc);
        let dir = |doc: &Document| match &doc.model().get(id).unwrap().kind {
            ElementKind::Wall(w) => w.direction(),
            _ => unreachable!(),
        };
        let before = dir(&doc);
        let mut m = tool(&doc, Mode::Rotate, id);
        with_map(&[], |map| {
            m.step(&mut doc, &pointer(at(0.0, 0.0)), map);
            assert_eq!(dir(&doc), before);
            m.step(&mut doc, &click(at(0.0, 0.0)), map);
            assert_eq!(m.stage(), Stage::Reference);
            m.step(&mut doc, &click(at(5.0, 0.0)), map);
            assert_eq!(m.stage(), Stage::Follow);
            assert_eq!(dir(&doc), before);
            m.step(&mut doc, &pointer(at(0.0, 5.0)), map);
            assert!(matches!(
                m.step(&mut doc, &click(at(0.0, 5.0)), map),
                Outcome::Done(_)
            ));
        });
        let turned = DVec2::from_angle(90f64.to_radians()).rotate(before);
        assert!((dir(&doc) - turned).length() < 1e-6);
    }
}
