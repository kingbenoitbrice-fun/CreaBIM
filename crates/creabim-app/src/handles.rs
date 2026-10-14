//! Drag handles on a selected element, defined once in model space and
//! shown in every pane that can use them: the 3D view, plans and sections.
//! Each handle slides along a line or across a plane; the pane turns the
//! pointer into a ray (a perspective ray in 3D, a straight-on one in 2D) and
//! the handle's constraint picks the point on it.
//!
//! Walls stretch by their ends (joined walls follow), grow taller by their
//! top and thicker by their face. Slabs move corners and edges and get
//! thicker by their underside. Doors and windows widen from either jamb and
//! change height and sill. A selected face of a mass pushes and pulls along
//! its normal.

use crate::modal::LiveEdit;
use crate::snaps::{Snap, SnapKind};
use creabim_core::{
    transform, Document, Element, ElementId, ElementKind, Model, OpeningKind, Xform,
};
use eframe::egui::{self, Color32, Painter, Pos2, Rect, Stroke, Vec2};
use glam::{DVec2, DVec3};

/// A pointer ray in model space.
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub o: DVec3,
    pub d: DVec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Constraint {
    /// Anywhere on the plane through `point` with `normal`.
    Plane { point: DVec3, normal: DVec3 },
    /// Along the line through `point` in `dir` (a unit vector).
    Line { point: DVec3, dir: DVec3 },
}

impl Constraint {
    /// The point on the constraint the ray points at.
    pub fn hit(&self, r: Ray) -> Option<DVec3> {
        match *self {
            Constraint::Plane { point, normal } => {
                let den = r.d.dot(normal);
                if den.abs() < 1e-9 {
                    return None;
                }
                let t = (point - r.o).dot(normal) / den;
                Some(r.o + r.d * t)
            }
            Constraint::Line { point, dir } => {
                // closest point on the line to the ray
                let w = point - r.o;
                let (a, b, c) = (dir.dot(dir), dir.dot(r.d), r.d.dot(r.d));
                let (d, e) = (dir.dot(w), r.d.dot(w));
                let den = a * c - b * b;
                if den.abs() < 1e-12 {
                    return None;
                }
                let s = (b * e - c * d) / den;
                Some(point + dir * s)
            }
        }
    }

    /// Can this be dragged in a pane looking along `look`? A line seen end
    /// on can't.
    pub fn usable(&self, look: DVec3) -> bool {
        match *self {
            Constraint::Line { dir, .. } => dir.dot(look).abs() < 0.97,
            Constraint::Plane { normal, .. } => normal.dot(look).abs() > 0.2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HandleKind {
    WallEnd {
        end: bool,
    },
    WallTop,
    WallFace,
    SlabCorner(usize),
    SlabEdge(usize),
    SlabUnderside,
    OpeningJamb {
        right: bool,
    },
    OpeningHead,
    OpeningSill,
    GridEnd {
        end: bool,
    },
    /// Push or pull a face of a mass along its normal.
    Face(usize),
}

impl HandleKind {
    /// Round handles slide along a line; square ones move freely.
    pub fn round(self) -> bool {
        !matches!(
            self,
            HandleKind::WallEnd { .. } | HandleKind::SlabCorner(_) | HandleKind::GridEnd { .. }
        )
    }

    pub fn verb(self) -> &'static str {
        match self {
            HandleKind::WallTop | HandleKind::OpeningHead => "Change height of",
            HandleKind::WallFace | HandleKind::SlabUnderside => "Change thickness of",
            HandleKind::OpeningJamb { .. } => "Change width of",
            HandleKind::OpeningSill => "Change sill of",
            HandleKind::Face(_) => "Push/pull",
            _ => "Stretch",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    pub kind: HandleKind,
    pub at: DVec3,
    pub constraint: Constraint,
}

fn elevation(model: &Model, level: Option<ElementId>) -> f64 {
    level
        .and_then(|l| model.level(l))
        .map_or(0.0, |l| l.elevation)
}

/// The handles of element `id`. `face` is the selected face of a mass.
pub fn handles(model: &Model, id: ElementId, face: Option<usize>) -> Vec<Handle> {
    let Some(e) = model.get(id) else {
        return vec![];
    };
    let line = |kind, at: DVec3, dir: DVec3| Handle {
        kind,
        at,
        constraint: Constraint::Line {
            point: at,
            dir: dir.normalize(),
        },
    };
    let flat = |kind, at: DVec3| Handle {
        kind,
        at,
        constraint: Constraint::Plane {
            point: at,
            normal: DVec3::Z,
        },
    };
    match &e.kind {
        ElementKind::Wall(w) => {
            let z0 = elevation(model, Some(w.level)) + w.base_offset;
            let mid = (w.start + w.end) / 2.0;
            let n = w.direction().perp();
            vec![
                flat(HandleKind::WallEnd { end: false }, w.start.extend(z0)),
                flat(HandleKind::WallEnd { end: true }, w.end.extend(z0)),
                line(HandleKind::WallTop, mid.extend(z0 + w.height), DVec3::Z),
                line(
                    HandleKind::WallFace,
                    (mid + n * w.thickness / 2.0).extend(z0 + w.height / 2.0),
                    n.extend(0.0),
                ),
            ]
        }
        ElementKind::Slab(s) => {
            let z = elevation(model, Some(s.level));
            let n = s.boundary.len();
            let ccw = creabim_core::polygon::signed_area2(&s.boundary) > 0.0;
            let mut out: Vec<Handle> = (0..n)
                .map(|i| flat(HandleKind::SlabCorner(i), s.boundary[i].extend(z)))
                .collect();
            for i in 0..n {
                let (a, b) = (s.boundary[i], s.boundary[(i + 1) % n]);
                let d = (b - a).normalize_or_zero();
                let out_dir = if ccw { -d.perp() } else { d.perp() };
                out.push(line(
                    HandleKind::SlabEdge(i),
                    ((a + b) / 2.0).extend(z),
                    out_dir.extend(0.0),
                ));
            }
            let c = s.boundary.iter().copied().sum::<DVec2>() / n.max(1) as f64;
            out.push(line(
                HandleKind::SlabUnderside,
                c.extend(z - s.thickness),
                DVec3::Z,
            ));
            out
        }
        ElementKind::Opening(o) => {
            let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
                return vec![];
            };
            let z0 = elevation(model, Some(w.level)) + w.base_offset;
            let u = w.direction();
            let face = w.start + w.direction().perp() * (w.thickness / 2.0 + 0.01);
            let at = |along: f64, up: f64| (face + u * along).extend(z0 + up);
            let mid = o.offset + o.width / 2.0;
            let mut out = vec![
                line(
                    HandleKind::OpeningJamb { right: false },
                    at(o.offset, o.sill + o.height / 2.0),
                    u.extend(0.0),
                ),
                line(
                    HandleKind::OpeningJamb { right: true },
                    at(o.offset + o.width, o.sill + o.height / 2.0),
                    u.extend(0.0),
                ),
                line(
                    HandleKind::OpeningHead,
                    at(mid, o.sill + o.height),
                    DVec3::Z,
                ),
            ];
            if o.kind == OpeningKind::Window {
                out.push(line(HandleKind::OpeningSill, at(mid, o.sill), DVec3::Z));
            }
            out
        }
        ElementKind::Grid(g) => vec![
            flat(HandleKind::GridEnd { end: false }, g.start.extend(0.0)),
            flat(HandleKind::GridEnd { end: true }, g.end.extend(0.0)),
        ],
        ElementKind::FreeMesh(m) => {
            let Some(f) = face.filter(|f| *f < m.mesh.faces.len()) else {
                return vec![];
            };
            let base = m.origin + DVec3::Z * elevation(model, m.level);
            let c = base + m.to_model(m.mesh.face_center(f));
            let n = m.to_model(m.mesh.face_normal(f)).normalize_or_zero();
            if n == DVec3::ZERO {
                return vec![];
            }
            vec![line(HandleKind::Face(f), c, n)]
        }
        _ => vec![],
    }
}

/// The elements a drag of `kind` on `id` changes: the element first, then
/// for a wall end the walls whose ends sit on it (so corners stay closed).
/// The second list says which end of each joined wall moves.
pub fn originals(
    model: &Model,
    id: ElementId,
    kind: HandleKind,
) -> (Vec<Element>, Vec<(usize, bool)>) {
    let Some(e) = model.get(id) else {
        return (vec![], vec![]);
    };
    let mut out = vec![e.clone()];
    let mut joined = vec![];
    if let (HandleKind::WallEnd { end }, ElementKind::Wall(w)) = (kind, &e.kind) {
        let p = if end { w.end } else { w.start };
        for other in model.iter() {
            if let ElementKind::Wall(o) = &other.kind {
                if other.id == id || o.level != w.level {
                    continue;
                }
                for (oe, q) in [(false, o.start), (true, o.end)] {
                    if q.distance(p) < 1e-3 {
                        out.push(other.clone());
                        joined.push((out.len() - 1, oe));
                    }
                }
            }
        }
    }
    (out, joined)
}

fn round(v: f64, step: f64) -> f64 {
    if step > 0.0 {
        (v / step).round() * step
    } else {
        v
    }
}

/// The dragged elements with handle `kind` moved from `grab` to `now`
/// (model points on the handle's constraint). `snapped` is `now` already
/// snapped to something, for handles that move freely. Lengths round to
/// `step`. Returns an explanation when the drag would break the element.
pub fn apply(
    model: &Model,
    originals: &[Element],
    joined: &[(usize, bool)],
    kind: HandleKind,
    grab: DVec3,
    now: DVec3,
    step: f64,
) -> Result<Vec<Element>, String> {
    let mut out = originals.to_vec();
    let d = now - grab;
    let mut moved_end = None;
    let first = out.first_mut().ok_or("nothing to drag")?;
    match (&mut first.kind, kind) {
        (ElementKind::Wall(w), HandleKind::WallEnd { end }) => {
            let p = now.truncate();
            if end {
                w.end = p;
            } else {
                w.start = p;
            }
            if w.length() < 0.05 {
                return Err("a wall needs some length".into());
            }
            moved_end = Some(p);
        }
        (ElementKind::Wall(w), HandleKind::WallTop) => {
            w.height = round(w.height + d.z, step);
            if w.height < 0.1 {
                return Err("too low".into());
            }
        }
        (ElementKind::Wall(w), HandleKind::WallFace) => {
            let n = w.direction().perp().extend(0.0);
            w.thickness = round(w.thickness + 2.0 * d.dot(n), step / 5.0);
            if w.thickness < 0.02 {
                return Err("too thin".into());
            }
        }
        (ElementKind::Slab(s), HandleKind::SlabCorner(i)) => {
            s.boundary[i] = now.truncate();
        }
        (ElementKind::Slab(s), HandleKind::SlabEdge(i)) => {
            let n = s.boundary.len();
            let ccw = creabim_core::polygon::signed_area2(&s.boundary) > 0.0;
            let (a, b) = (s.boundary[i], s.boundary[(i + 1) % n]);
            let e = (b - a).normalize_or_zero();
            let out_dir = if ccw { -e.perp() } else { e.perp() };
            let off = out_dir * round(d.truncate().dot(out_dir), step);
            s.boundary[i] += off;
            s.boundary[(i + 1) % n] += off;
        }
        (ElementKind::Slab(s), HandleKind::SlabUnderside) => {
            s.thickness = round(s.thickness - d.z, step / 5.0);
            if s.thickness < 0.02 {
                return Err("too thin".into());
            }
        }
        (ElementKind::Opening(o), HandleKind::OpeningJamb { right }) => {
            let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
                return Err("its wall is gone".into());
            };
            let along = round(d.truncate().dot(w.direction()), step);
            if right {
                o.width = (o.width + along).min(w.length() - o.offset);
            } else {
                let along = along.max(-o.offset);
                o.offset += along;
                o.width -= along;
            }
            if o.width < 0.2 {
                return Err("too narrow".into());
            }
        }
        (ElementKind::Opening(o), HandleKind::OpeningHead) => {
            o.height = round(o.height + d.z, step);
        }
        (ElementKind::Opening(o), HandleKind::OpeningSill) => {
            let dz = round(d.z, step).clamp(-o.sill, o.height - 0.2);
            o.sill += dz;
            o.height -= dz;
        }
        (ElementKind::Grid(g), HandleKind::GridEnd { end }) => {
            if end {
                g.end = now.truncate();
            } else {
                g.start = now.truncate();
            }
        }
        (ElementKind::FreeMesh(m), HandleKind::Face(f)) => {
            let n = m.to_model(m.mesh.face_normal(f)).normalize_or_zero();
            let dist = round(d.dot(n), step / 5.0);
            let verts = m.mesh.faces[f].clone();
            let local = m.to_local(n * dist);
            m.mesh.translate_vertices(&verts, local);
        }
        _ => return Err("this handle doesn't fit this element".into()),
    }
    let opening_ok = match &first.kind {
        ElementKind::Opening(o) => match model.get(o.host).map(|h| &h.kind) {
            Some(ElementKind::Wall(w)) => o.sill + o.height <= w.height + 1e-9,
            _ => true,
        },
        _ => true,
    };
    if !opening_ok {
        return Err("taller than its wall".into());
    }
    if let Some(p) = moved_end {
        for &(i, oe) in joined {
            if let ElementKind::Wall(o) = &mut out[i].kind {
                if oe {
                    o.end = p;
                } else {
                    o.start = p;
                }
            }
        }
    }
    for e in &out {
        e.validate()?;
    }
    Ok(out)
}

/// How far a handle has moved, for the status bar.
pub fn readout(changed: &[Element]) -> String {
    match changed.first().map(|e| &e.kind) {
        Some(ElementKind::Wall(w)) => format!(
            "length {:.2} m, height {:.2} m, thickness {:.0} mm",
            w.length(),
            w.height,
            w.thickness * 1000.0
        ),
        Some(ElementKind::Opening(o)) => format!(
            "width {:.2} m, height {:.2} m, sill {:.2} m",
            o.width, o.height, o.sill
        ),
        Some(ElementKind::Slab(s)) => format!(
            "area {:.2} m², thickness {:.0} mm",
            creabim_core::polygon::signed_area2(&s.boundary).abs() / 2.0,
            s.thickness * 1000.0
        ),
        _ => String::new(),
    }
}

pub type SnapFn<'a> = dyn Fn(Pos2, Option<DVec3>) -> Option<(DVec3, Snap)> + 'a;

/// How a pane turns pointer positions into the model, for one frame.
pub struct PaneInput<'a> {
    /// The pointer as a ray into the model.
    pub ray: &'a dyn Fn(Pos2) -> Option<Ray>,
    /// A model point on screen.
    pub project: &'a dyn Fn(DVec3) -> Option<Pos2>,
    /// Which way the pane looks, to hide handles seen end on.
    pub look: DVec3,
    /// The best snap near the pointer as a model point, with the line's
    /// start for perpendicular and axis snaps. The pane leaves the dragged
    /// elements out.
    pub snap: &'a SnapFn<'a>,
    /// The point of `id` under the pointer, if it is there: grabbing the
    /// selected element anywhere but a handle moves it.
    pub grab_body: &'a dyn Fn(Pos2, ElementId) -> Option<DVec3>,
    /// The plane a grabbed element slides on: level in plans and 3D, the
    /// section plane in sections.
    pub body_normal: DVec3,
}

/// A handle or a whole element being dragged.
#[derive(Debug)]
pub struct Drag {
    /// `None` moves the whole element.
    kind: Option<HandleKind>,
    constraint: Constraint,
    edit: LiveEdit,
    joined: Vec<(usize, bool)>,
    grab: DVec3,
}

const ACCENT: Color32 = Color32::from_rgb(230, 120, 30);

/// Draw the selected element's handles and run a drag on them (or on the
/// element itself). Returns `true` while a drag runs, so the pane leaves
/// the pointer alone, and puts a message for the status bar in `status`.
/// Add a corner in the middle of a slab edge, or take a corner away.
pub fn edit_corners(doc: &mut Document, id: ElementId, kind: HandleKind) -> Result<String, String> {
    let Some(ElementKind::Slab(s)) = doc.model().get(id).map(|e| &e.kind) else {
        return Err("Only slab outlines have corners to add or remove.".into());
    };
    let n = s.boundary.len();
    let (label, done) = match kind {
        HandleKind::SlabEdge(i) if i < n => {
            ("Add slab corner", "Added a corner: drag it into place.")
        }
        HandleKind::SlabCorner(i) if i < n => {
            if n <= 3 {
                return Err("A slab needs at least three corners.".into());
            }
            ("Remove slab corner", "Removed the corner.")
        }
        _ => return Err("That isn't a corner or an edge.".into()),
    };
    doc.edit(id, label, |e| {
        if let ElementKind::Slab(s) = &mut e.kind {
            match kind {
                HandleKind::SlabEdge(i) => {
                    let (a, b) = (s.boundary[i], s.boundary[(i + 1) % n]);
                    s.boundary.insert(i + 1, (a + b) / 2.0);
                }
                HandleKind::SlabCorner(i) => {
                    s.boundary.remove(i);
                }
                _ => {}
            }
        }
    })
    .map_err(|e| e.to_string())?;
    Ok(format!("{done} Ctrl+Z to undo."))
}

#[allow(clippy::too_many_arguments)]
pub fn interact(
    doc: &mut Document,
    painter: &Painter,
    response: &egui::Response,
    selection: Option<ElementId>,
    face: Option<usize>,
    drag: &mut Option<Drag>,
    input: &PaneInput,
    status: &mut Option<String>,
) -> bool {
    let Some(id) = selection else {
        *drag = None;
        return false;
    };
    let shown: Vec<Handle> = handles(doc.model(), id, face)
        .into_iter()
        .filter(|h| h.constraint.usable(input.look))
        .collect();
    for h in &shown {
        let Some(c) = (input.project)(h.at) else {
            continue;
        };
        if let Constraint::Line { dir, .. } = h.constraint {
            // a short stem shows which way the handle slides
            if let Some(tip) = (input.project)(h.at + dir * 0.3) {
                let v = (tip - c).normalized() * 14.0;
                if v.is_finite() {
                    painter.line_segment([c - v, c + v], Stroke::new(1.5, ACCENT));
                }
            }
        }
        if h.kind.round() {
            painter.circle(c, 4.5, Color32::WHITE, Stroke::new(1.5, ACCENT));
        } else {
            painter.rect(
                Rect::from_center_size(c, Vec2::splat(9.0)),
                0.0,
                Color32::WHITE,
                Stroke::new(1.5, ACCENT),
                egui::StrokeKind::Middle,
            );
        }
    }

    // a slab's outline: double-click an edge's square to add a corner
    // there, or a corner to take it away
    if drag.is_none() && response.double_clicked() {
        if let Some(press) = response.interact_pointer_pos() {
            let near = shown
                .iter()
                .filter(|h| matches!(h.kind, HandleKind::SlabCorner(_) | HandleKind::SlabEdge(_)))
                .filter_map(|h| Some((h, (input.project)(h.at)?.distance(press))))
                .filter(|(_, d)| *d < 10.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(h, _)| h.kind);
            if let Some(k) = near {
                *status = Some(match edit_corners(doc, id, k) {
                    Ok(s) => s,
                    Err(e) => e,
                });
                return true;
            }
        }
    }

    if drag.is_none() && response.drag_started_by(egui::PointerButton::Primary) {
        let Some(press) = painter
            .ctx()
            .input(|i| i.pointer.press_origin())
            .or(response.interact_pointer_pos())
        else {
            return false;
        };
        let near = shown
            .iter()
            .filter_map(|h| Some((h, (input.project)(h.at)?.distance(press))))
            .filter(|(_, d)| *d < 10.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(h, _)| *h);
        let start = match near {
            Some(h) => {
                let grab = (input.ray)(press)
                    .and_then(|r| h.constraint.hit(r))
                    .unwrap_or(h.at);
                Some((Some(h.kind), h.constraint, grab))
            }
            None => {
                let movable = !matches!(
                    doc.model().get(id).map(|e| &e.kind),
                    Some(
                        ElementKind::View(_)
                            | ElementKind::Dimension(_)
                            | ElementKind::Tag(_)
                            | ElementKind::Level(_)
                            | ElementKind::Sheet(_)
                            | ElementKind::Viewport(_)
                    ) | None
                );
                (input.grab_body)(press, id).filter(|_| movable).map(|p| {
                    let normal = input.body_normal;
                    (None, Constraint::Plane { point: p, normal }, p)
                })
            }
        };
        if let Some((kind, constraint, grab)) = start {
            let (originals, joined) = match kind {
                Some(k) => originals(doc.model(), id, k),
                None => (doc.model().get(id).cloned().into_iter().collect(), vec![]),
            };
            let name = originals.first().map_or(String::new(), |e| e.name.clone());
            let label = match kind {
                Some(k) => format!("{} {name}", k.verb()),
                None => format!("Move {name}"),
            };
            *drag = Some(Drag {
                kind,
                constraint,
                edit: LiveEdit::new(label, originals),
                joined,
                grab,
            });
        }
    }

    let Some(d) = drag.as_mut() else {
        return false;
    };
    if painter.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        d.edit.cancel(doc);
        *drag = None;
        *status = Some("Drag cancelled.".into());
        return true;
    }
    if response.drag_stopped() || !response.dragged() {
        if d.edit.applied() {
            *status = doc.undo_label().map(|l| format!("{l}. Ctrl+Z to undo."));
        }
        *drag = None;
        return true;
    }
    let Some(pos) = response.interact_pointer_pos() else {
        return true;
    };
    let Some(free) = (input.ray)(pos).and_then(|r| d.constraint.hit(r)) else {
        return true;
    };
    let fixed = match (d.kind, d.edit.originals().first().map(|e| &e.kind)) {
        (Some(HandleKind::WallEnd { end }), Some(ElementKind::Wall(w))) => {
            Some(if end { w.start } else { w.end }.extend(free.z))
        }
        (None, _) => Some(d.grab),
        _ => None,
    };
    let snapped = (input.snap)(pos, fixed);
    let now = match (d.constraint, snapped) {
        (Constraint::Plane { point, normal }, Some((p, snap))) => {
            if snap.kind != SnapKind::Grid {
                if let Some(at) = (input.project)(p) {
                    crate::snaps::draw(painter, snap, at, fixed.and_then(|f| (input.project)(f)));
                }
            }
            // onto the plane, along its normal
            p - normal * (p - point).dot(normal)
        }
        (Constraint::Line { point, dir }, Some((p, snap)))
            if matches!(
                snap.kind,
                SnapKind::Endpoint | SnapKind::Midpoint | SnapKind::Center | SnapKind::Intersection
            ) =>
        {
            if let Some(at) = (input.project)(p) {
                crate::snaps::draw(painter, snap, at, None);
            }
            point + dir * (p - point).dot(dir)
        }
        _ => free,
    };
    let model = doc.model().clone();
    let result = match d.kind {
        Some(k) => apply(&model, d.edit.originals(), &d.joined, k, d.grab, now, 0.05),
        None => {
            let delta = now - d.grab;
            let delta = if snapped.is_some_and(|(_, s)| s.kind != SnapKind::Grid) {
                delta
            } else {
                (delta / 0.05).round() * 0.05
            };
            d.edit.originals()[..1]
                .iter()
                .map(|e| {
                    transform::transformed(&model, e, &Xform::Move(delta))
                        .ok_or_else(|| "this can't move that way".to_string())
                })
                .collect()
        }
    };
    match result {
        Ok(changed) => {
            *status = Some(format!("{}  {}", d.edit.label(), readout(&changed)));
            if let Err(e) = d.edit.update(doc, changed) {
                *status = Some(format!("Can't go there: {e}"));
            }
        }
        Err(e) => *status = Some(format!("Can't go there: {e}")),
    }
    painter.ctx().request_repaint();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    fn wall_with_door(m: &Model) -> (ElementId, ElementId) {
        m.iter()
            .find_map(|e| match &e.kind {
                ElementKind::Opening(o) if o.kind == OpeningKind::Door => Some((o.host, e.id)),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn a_line_constraint_takes_the_point_nearest_the_ray() {
        let c = Constraint::Line {
            point: DVec3::ZERO,
            dir: DVec3::Z,
        };
        let r = Ray {
            o: DVec3::new(5.0, 0.0, 2.0),
            d: -DVec3::X,
        };
        assert!(c.hit(r).unwrap().distance(DVec3::new(0.0, 0.0, 2.0)) < 1e-9);
        assert!(!c.usable(DVec3::Z));
    }

    #[test]
    fn dragging_a_door_jamb_widens_it_and_the_head_raises_it() {
        let m = sample_house();
        let (_, door) = wall_with_door(&m);
        let hs = handles(&m, door, None);
        let right = hs
            .iter()
            .find(|h| h.kind == HandleKind::OpeningJamb { right: true })
            .unwrap();
        let ElementKind::Opening(o) = &m.get(door).unwrap().kind else {
            panic!()
        };
        let Constraint::Line { dir, .. } = right.constraint else {
            panic!()
        };
        let (orig, joined) = originals(&m, door, right.kind);
        let out = apply(
            &m,
            &orig,
            &joined,
            right.kind,
            right.at,
            right.at + dir * 0.2,
            0.05,
        )
        .unwrap();
        let ElementKind::Opening(n) = &out[0].kind else {
            panic!()
        };
        assert!((n.width - (o.width + 0.2)).abs() < 1e-9);
        assert!((n.offset - o.offset).abs() < 1e-9);
        let head = hs
            .iter()
            .find(|h| h.kind == HandleKind::OpeningHead)
            .unwrap();
        let out = apply(
            &m,
            &orig,
            &[],
            head.kind,
            head.at,
            head.at + DVec3::Z * 0.1,
            0.05,
        )
        .unwrap();
        let ElementKind::Opening(n) = &out[0].kind else {
            panic!()
        };
        assert!((n.height - (o.height + 0.1)).abs() < 1e-9);
        // far too tall for the wall
        assert!(apply(
            &m,
            &orig,
            &[],
            head.kind,
            head.at,
            head.at + DVec3::Z * 5.0,
            0.05
        )
        .is_err());
    }

    #[test]
    fn stretching_a_wall_end_takes_the_joined_wall_along() {
        let m = sample_house();
        let (id, w) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some((e.id, w.clone())),
                _ => None,
            })
            .unwrap();
        let kind = HandleKind::WallEnd { end: true };
        let (orig, joined) = originals(&m, id, kind);
        assert!(!joined.is_empty(), "sample walls meet at corners");
        let to = (w.end + w.direction() * 0.5).extend(0.0);
        let out = apply(&m, &orig, &joined, kind, w.end.extend(0.0), to, 0.05).unwrap();
        for &(i, oe) in &joined {
            let ElementKind::Wall(o) = &out[i].kind else {
                panic!()
            };
            let p = if oe { o.end } else { o.start };
            assert!(p.distance(to.truncate()) < 1e-9);
        }
    }

    #[test]
    fn pulling_a_slab_underside_makes_it_thicker() {
        let m = sample_house();
        let (id, t) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Slab(s) => Some((e.id, s.thickness)),
                _ => None,
            })
            .unwrap();
        let h = handles(&m, id, None)
            .into_iter()
            .find(|h| h.kind == HandleKind::SlabUnderside)
            .unwrap();
        let (orig, _) = originals(&m, id, h.kind);
        let out = apply(&m, &orig, &[], h.kind, h.at, h.at - DVec3::Z * 0.05, 0.05).unwrap();
        let ElementKind::Slab(s) = &out[0].kind else {
            panic!()
        };
        assert!((s.thickness - (t + 0.05)).abs() < 1e-9);
    }

    #[test]
    fn slab_corners_can_be_added_on_an_edge_and_taken_away() {
        let m = creabim_core::sample::sample_house();
        let (id, n) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Slab(s) => Some((e.id, s.boundary.len())),
                _ => None,
            })
            .unwrap();
        let mut doc = Document::from_model(m);
        edit_corners(&mut doc, id, HandleKind::SlabEdge(0)).unwrap();
        let corners = |doc: &Document| match &doc.model().get(id).unwrap().kind {
            ElementKind::Slab(s) => s.boundary.clone(),
            _ => panic!(),
        };
        let b = corners(&doc);
        assert_eq!(b.len(), n + 1);
        assert!(b[1].distance((b[0] + b[2]) / 2.0) < 1e-9);
        edit_corners(&mut doc, id, HandleKind::SlabCorner(1)).unwrap();
        assert_eq!(corners(&doc).len(), n);
        while corners(&doc).len() > 3 {
            edit_corners(&mut doc, id, HandleKind::SlabCorner(0)).unwrap();
        }
        assert!(edit_corners(&mut doc, id, HandleKind::SlabCorner(0)).is_err());
    }
}
