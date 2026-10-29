//! Drag handles on a selected element, defined once in model space and
//! shown in every pane that can use them: the 3D view, plans and sections.
//! Each handle slides along a line or across a plane; the pane turns the
//! pointer into a ray (a perspective ray in 3D, a straight-on one in 2D) and
//! the handle's constraint picks the point on it.
//!
//! Walls stretch by their ends (joined walls follow), grow taller by their
//! top and thicker by their face. Slabs move corners and edges, and get
//! thicker by their underside (the top stays) or by their top (the
//! underside stays). Doors and windows widen from either jamb and
//! change height and sill. A selected face of a mass pushes and pulls along
//! its normal. Shafts move their corners, base and top.
//!
//! Every outline (a slab's, a roof's, a shaft's, a sketch's, a railing's
//! path) has the same grips, from [`crate::editmode::outline_handles`]:
//! its corners and edges when selected, and in Edit mode its holes and
//! add-corner grips too, with corners to pick and remove (see
//! [`GripSel`]).

use crate::modal::LiveEdit;
use crate::snaps::{Snap, SnapKind};
use creabim_core::{
    transform, walls, Document, Element, ElementId, ElementKind, Model, OpeningKind, ViewKind,
    WallTop, Xform,
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

/// Which outline of an element an Edit-mode grip works on: its own, or
/// hole `k` of a slab or a roof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ring {
    Outline,
    Hole(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HandleKind {
    WallEnd {
        end: bool,
    },
    WallTop,
    WallFace,
    SlabUnderside,
    SlabTop,
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
    /// Grip `i` of a column, beam, roof, stair or railing; `free` ones
    /// move across the plan, others along a line.
    Member {
        i: usize,
        free: bool,
    },
    /// An end of a section or elevation line.
    ViewEnd {
        end: bool,
    },
    /// How far a section or elevation sees, from its line.
    ViewDepth,
    /// Face `i` of a crop box (see [`creabim_core::CropBox::face`]).
    CropFace(usize),
    /// A crop box as a whole, across the plan.
    CropMove,
    /// Corner `i` of an outline: a slab's, a roof's, a shaft's, a
    /// sketch's, a railing's path; in Edit mode a hole's too.
    Vertex {
        ring: Ring,
        i: usize,
    },
    /// Edge `i` of a closed outline, pushed out or pulled in square to
    /// itself.
    Edge {
        ring: Ring,
        i: usize,
    },
    /// In Edit mode, the middle of edge `i`: dragging it pulls a new
    /// corner out of the edge (a click adds nothing, so a double-click
    /// can't add a corner and take it away again).
    AddVertex {
        ring: Ring,
        i: usize,
    },
    /// In Edit mode, a whole outline moved: a hole dragged from inside.
    Ring {
        ring: Ring,
    },
    /// A shaft's base and its top, up and down.
    ShaftBase,
    ShaftTop,
    /// Turn an object, a column or a stair about its own point.
    Rotate,
    /// A rectangular column's width (along its own x) or depth; a round
    /// one's diameter.
    ColumnSize {
        depth: bool,
    },
    /// Slide a whole wall square to itself; the walls joined to it stretch.
    WallSlide,
    /// Slide a door or window along its wall at the same width.
    OpeningSlide,
    /// A beam's width, both sides at once.
    BeamWidth,
}

impl HandleKind {
    /// Round handles slide along a line; square ones move freely.
    pub fn round(self) -> bool {
        !matches!(
            self,
            HandleKind::WallEnd { .. }
                | HandleKind::GridEnd { .. }
                | HandleKind::Member { free: true, .. }
                | HandleKind::ViewEnd { .. }
                | HandleKind::CropMove
                | HandleKind::Vertex { .. }
                | HandleKind::AddVertex { .. }
        )
    }

    pub fn verb(self) -> &'static str {
        match self {
            HandleKind::WallTop
            | HandleKind::OpeningHead
            | HandleKind::ShaftBase
            | HandleKind::ShaftTop => "Change height of",
            HandleKind::Vertex { .. }
            | HandleKind::Edge { .. }
            | HandleKind::AddVertex { .. }
            | HandleKind::Ring { .. } => "Reshape",
            HandleKind::Rotate => "Rotate",
            HandleKind::ColumnSize { .. } => "Resize",
            HandleKind::WallSlide | HandleKind::OpeningSlide => "Slide",
            HandleKind::BeamWidth => "Change width of",
            HandleKind::WallFace | HandleKind::SlabUnderside | HandleKind::SlabTop => {
                "Change thickness of"
            }
            HandleKind::OpeningJamb { .. } => "Change width of",
            HandleKind::OpeningSill => "Change sill of",
            HandleKind::Face(_) => "Push/pull",
            HandleKind::ViewDepth => "Change depth of",
            HandleKind::CropFace(_) => "Resize",
            HandleKind::CropMove => "Move",
            _ => "Stretch",
        }
    }

    /// The undo label of a drag of this grip on `name`.
    pub fn label(self, name: &str) -> String {
        match self {
            HandleKind::Vertex { .. } => format!("Move a corner of {name}"),
            HandleKind::AddVertex { .. } => format!("Add a corner to {name}"),
            HandleKind::Edge { .. } => format!("Offset an edge of {name}"),
            HandleKind::Ring {
                ring: Ring::Hole(k),
            } => format!("Move hole {} of {name}", k + 1),
            HandleKind::Ring { .. } => format!("Move the outline of {name}"),
            k => format!("{} {name}", k.verb()),
        }
    }

    /// What the grip does, for the tooltip by the pointer.
    pub fn tip(self, edit: bool) -> &'static str {
        match self {
            HandleKind::Vertex { .. } if edit => {
                "Drag: move corner · Click: select (Shift adds) · Del: remove · Right-click: more"
            }
            HandleKind::Vertex { .. } => {
                "Drag: move corner · Double-click: edit the outline (add and remove corners)"
            }
            HandleKind::AddVertex { .. } => "Drag: pull out a new corner · Right-click: more",
            HandleKind::Edge { .. } if edit => "Drag: offset the edge · Right-click: more",
            HandleKind::Edge { .. } => "Drag: offset the edge",
            HandleKind::WallEnd { .. } => "Drag: move the end (Ctrl: free of its joins)",
            HandleKind::WallTop | HandleKind::ShaftTop => "Drag: change the top",
            HandleKind::ShaftBase => "Drag: change the base",
            HandleKind::WallFace => "Drag: change the thickness",
            HandleKind::SlabTop => "Drag: move the top (the underside stays)",
            HandleKind::SlabUnderside => "Drag: change the thickness (the top stays)",
            HandleKind::OpeningJamb { .. } => "Drag: change the width",
            HandleKind::OpeningHead => "Drag: change the height",
            HandleKind::OpeningSill => "Drag: change the sill",
            HandleKind::Face(_) => "Drag: push or pull the face",
            HandleKind::ViewDepth => "Drag: change how deep it sees",
            HandleKind::CropFace(_) => "Drag: move the crop face",
            HandleKind::Rotate => "Drag: turn it (steps of 5°) · Tab: type a length",
            HandleKind::ColumnSize { depth: false } => "Drag: change the width",
            HandleKind::ColumnSize { depth: true } => "Drag: change the depth",
            HandleKind::WallSlide => "Drag: slide the wall square to itself (joined walls follow)",
            HandleKind::OpeningSlide => "Drag: slide it along the wall · Tab: type how far",
            HandleKind::BeamWidth => "Drag: change the width",
            _ => "Drag to change it",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    pub kind: HandleKind,
    pub at: DVec3,
    pub constraint: Constraint,
    /// Screen pixels the grip is drawn out from `at` along its line, to
    /// keep it clear of another grip at the same point.
    pub nudge: f32,
}

/// The handles of element `id`, with `edit` those Edit mode adds (see
/// [`crate::editmode`]).
pub fn handles_in(model: &Model, id: ElementId, face: Option<usize>, edit: bool) -> Vec<Handle> {
    let mut out = handles(model, id, face);
    out.extend(crate::editmode::outline_handles(model, id, edit));
    out
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
        nudge: 0.0,
    };
    let flat = |kind, at: DVec3| Handle {
        kind,
        at,
        constraint: Constraint::Plane {
            point: at,
            normal: DVec3::Z,
        },
        nudge: 0.0,
    };
    match &e.kind {
        ElementKind::Wall(w) => {
            let z0 = elevation(model, Some(w.level)) + w.base_offset;
            let mid = (w.start + w.end) / 2.0;
            let n = w.direction().perp();
            // the ends are gripped on the wall's location line
            let (a, b) = walls::location_line(model, w);
            vec![
                flat(HandleKind::WallEnd { end: false }, a.extend(z0)),
                flat(HandleKind::WallEnd { end: true }, b.extend(z0)),
                line(HandleKind::WallTop, mid.extend(z0 + w.height), DVec3::Z),
                line(
                    HandleKind::WallFace,
                    (mid + n * w.thickness / 2.0).extend(z0 + w.height / 2.0),
                    n.extend(0.0),
                ),
                // off the other face, clear of the thickness grip
                line(
                    HandleKind::WallSlide,
                    (mid - n * (w.thickness / 2.0 + 0.3)).extend(z0 + w.height / 2.0),
                    n.extend(0.0),
                ),
            ]
        }
        ElementKind::Slab(s) => {
            // its outline's grips are every outline's (see handles_in)
            let z = s.top(elevation(model, Some(s.level)));
            let n = s.boundary.len();
            // the face a thickness handle sits on is the one that moves
            let c = s.boundary.iter().copied().sum::<DVec2>() / n.max(1) as f64;
            vec![
                line(HandleKind::SlabTop, c.extend(z), DVec3::Z),
                line(
                    HandleKind::SlabUnderside,
                    c.extend(z - s.thickness),
                    DVec3::Z,
                ),
            ]
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
            if matches!(o.kind, OpeningKind::Window | OpeningKind::Hole) {
                out.push(line(HandleKind::OpeningSill, at(mid, o.sill), DVec3::Z));
            }
            // in the middle, to slide it along its wall
            out.push(line(
                HandleKind::OpeningSlide,
                at(mid, o.sill + o.height / 2.0),
                u.extend(0.0),
            ));
            out
        }
        ElementKind::Shaft(s) => {
            let (z0, z1) = s.span(model);
            let n = s.boundary.len().max(1) as f64;
            let c = s.boundary.iter().copied().sum::<DVec2>() / n;
            vec![
                line(HandleKind::ShaftBase, c.extend(z0), DVec3::Z),
                line(HandleKind::ShaftTop, c.extend(z1), DVec3::Z),
            ]
        }
        ElementKind::View(v) => match v.kind {
            ViewKind::Section {
                start, end, depth, ..
            } => {
                let look = (end - start).normalize_or_zero().perp();
                vec![
                    flat(HandleKind::ViewEnd { end: false }, start.extend(0.0)),
                    flat(HandleKind::ViewEnd { end: true }, end.extend(0.0)),
                    line(
                        HandleKind::ViewDepth,
                        ((start + end) / 2.0 + look * depth).extend(0.0),
                        look.extend(0.0),
                    ),
                ]
            }
            _ => vec![],
        },
        ElementKind::CropBox(c) => {
            let mut out: Vec<Handle> = (0..6)
                .map(|i| {
                    let (at, n) = c.face(i);
                    line(HandleKind::CropFace(i), at, n)
                })
                .collect();
            out.push(flat(HandleKind::CropMove, c.min));
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
        ElementKind::Object(o) => {
            let z = creabim_core::objects::base_z(model, e).unwrap_or(0.0);
            let (lo, hi) = creabim_core::objects::type_of(model, e)
                .map(|t| t.bounds())
                .unwrap_or((DVec3::new(-0.3, -0.6, 0.0), DVec3::new(0.3, 0.0, 1.0)));
            let front = DVec2::new((lo.x + hi.x) / 2.0, lo.y - 0.35);
            vec![flat(HandleKind::Rotate, o.to_plan(front).extend(z))]
        }
        _ => creabim_core::members::grips(model, e)
            .into_iter()
            .enumerate()
            // a roof's corners and a railing's path points are outline
            // corners (see handles_in); the grips after them keep their
            // numbers
            .filter(|(i, _)| *i >= outline_grips(e))
            .map(|(i, g)| match g.how {
                creabim_core::members::GripMove::Flat => {
                    flat(HandleKind::Member { i, free: true }, g.at)
                }
                creabim_core::members::GripMove::Vertical => {
                    line(HandleKind::Member { i, free: false }, g.at, DVec3::Z)
                }
                creabim_core::members::GripMove::Along(d) => {
                    line(HandleKind::Member { i, free: false }, g.at, d.extend(0.0))
                }
            })
            .chain(member_extras(model, e))
            .collect(),
    }
}

/// The grips a column and a stair have besides their members' own: a
/// turning grip, and a column's width and depth.
fn member_extras(model: &Model, e: &Element) -> Vec<Handle> {
    let line = |kind, at: DVec3, dir: DVec2| Handle {
        kind,
        at,
        constraint: Constraint::Line {
            point: at,
            dir: dir.normalize().extend(0.0),
        },
        nudge: 0.0,
    };
    let flat = |kind, at: DVec3| Handle {
        kind,
        at,
        constraint: Constraint::Plane {
            point: at,
            normal: DVec3::Z,
        },
        nudge: 0.0,
    };
    match &e.kind {
        ElementKind::Column(c) => {
            let (z0, _) = c.span(model);
            let x = DVec2::from_angle(c.rotation.to_radians());
            let (w, d) = match c.profile {
                creabim_core::members::Profile::Rectangle { width, depth } => (width, depth),
                creabim_core::members::Profile::Round { diameter } => (diameter, diameter),
            };
            let mut out = vec![
                flat(
                    HandleKind::Rotate,
                    (c.position + x * (w / 2.0 + 0.35)).extend(z0),
                ),
                line(
                    HandleKind::ColumnSize { depth: false },
                    (c.position + x * w / 2.0).extend(z0),
                    x,
                ),
            ];
            if matches!(c.profile, creabim_core::members::Profile::Rectangle { .. }) {
                out.push(line(
                    HandleKind::ColumnSize { depth: true },
                    (c.position + x.perp() * d / 2.0).extend(z0),
                    x.perp(),
                ));
            }
            out
        }
        ElementKind::Beam(b) => {
            let n = (b.end - b.start).normalize_or_zero().perp();
            let mid = (b.start + b.end) / 2.0;
            vec![line(
                HandleKind::BeamWidth,
                (mid + n * b.width / 2.0).extend(b.top(model)),
                n,
            )]
        }
        ElementKind::Stair(s) => {
            let dir = DVec2::from_angle(s.direction.to_radians());
            vec![flat(
                HandleKind::Rotate,
                (s.start - dir * 0.6).extend(s.base(model)),
            )]
        }
        _ => vec![],
    }
}

/// The point a rotate grip turns its element about, in plan.
fn pivot_of(model: &Model, e: &Element) -> Option<DVec2> {
    match &e.kind {
        ElementKind::Object(o) => {
            let (lo, hi) = creabim_core::objects::type_of(model, e)?.bounds();
            Some(o.to_plan((lo.truncate() + hi.truncate()) / 2.0))
        }
        ElementKind::Column(c) => Some(c.position),
        ElementKind::Stair(s) => Some(s.start),
        _ => None,
    }
}

/// An object turned `by` degrees about the middle of its footprint.
pub fn turn_object(model: &Model, e: &mut Element, by: f64) {
    let pivot = pivot_of(model, e);
    if let ElementKind::Object(o) = &mut e.kind {
        let c = pivot.unwrap_or(o.position);
        let r = DVec2::from_angle(by.to_radians());
        o.position = c + r.rotate(o.position - c);
        o.rotation = (o.rotation + by).rem_euclid(360.0);
    }
}

/// How many of a member's grips are corners of its outline, given as
/// outline grips instead.
fn outline_grips(e: &Element) -> usize {
    match &e.kind {
        ElementKind::Roof(r) => r.boundary.len(),
        ElementKind::Railing(r) if r.stair.is_none() => r.path.len(),
        _ => 0,
    }
}

/// A handle that moves across the plan, seen in a section or elevation
/// (looking level along `look`): there it slides across the paper, along
/// the section line, so wall ends, slab corners, beam ends and the like
/// can be dragged there too.
pub fn in_section(h: Handle, look: DVec3) -> Handle {
    match h.constraint {
        Constraint::Plane { point, normal } if normal.z.abs() > 0.99 && look.z.abs() < 0.1 => {
            Handle {
                constraint: Constraint::Line {
                    point,
                    dir: DVec3::Z.cross(look).normalize_or(DVec3::X),
                },
                ..h
            }
        }
        _ => h,
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
        // an end that may not join drags alone
        let models = if w.disallow_join[usize::from(end)] {
            None
        } else {
            Some(model)
        };
        for other in models.into_iter().flat_map(|m| m.iter()) {
            if let ElementKind::Wall(o) = &other.kind {
                if other.id == id || o.level != w.level {
                    continue;
                }
                for (oe, q) in [(false, o.start), (true, o.end)] {
                    if q.distance(p) < 1e-3 && !o.disallow_join[usize::from(oe)] {
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
            // the grip is on the location line: move that, and the
            // centreline with it
            let p = now.truncate();
            let (mut a, mut b) = walls::location_line(model, w);
            if end {
                b = p;
            } else {
                a = p;
            }
            if a.distance(b) < 0.05 {
                return Err("a wall needs some length".into());
            }
            walls::set_location_line(model, w, a, b);
            moved_end = Some(if end { w.end } else { w.start });
        }
        (ElementKind::Wall(w), HandleKind::WallTop) => match &mut w.top {
            // tied to a level: the drag sets how far above it the top is
            WallTop::UpToLevel { offset, .. } => {
                let h = round(w.height + d.z, step);
                if h < 0.1 {
                    return Err("too low".into());
                }
                *offset += h - w.height;
                w.height = h;
            }
            WallTop::Unconnected => {
                w.height = round(w.height + d.z, step);
                if w.height < 0.1 {
                    return Err("too low".into());
                }
            }
        },
        (ElementKind::Wall(w), HandleKind::WallFace) => {
            let n = w.direction().perp().extend(0.0);
            w.thickness = round(w.thickness + 2.0 * d.dot(n), step / 5.0);
            if w.thickness < 0.02 {
                return Err("too thin".into());
            }
        }
        (ElementKind::Slab(s), HandleKind::SlabUnderside) => {
            s.set_thickness(round(s.thickness - d.z, step / 5.0), false);
            if s.thickness < 0.02 {
                return Err("too thin".into());
            }
        }
        (ElementKind::Slab(s), HandleKind::SlabTop) => {
            s.set_thickness(round(s.thickness + d.z, step / 5.0), true);
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
        (ElementKind::View(v), HandleKind::ViewEnd { end: which }) => {
            if let ViewKind::Section { start, end, .. } = &mut v.kind {
                let p = now.truncate();
                if which {
                    *end = p;
                } else {
                    *start = p;
                }
                if start.distance(*end) < 0.2 {
                    return Err("a view line needs some length".into());
                }
            }
        }
        (ElementKind::View(v), HandleKind::ViewDepth) => {
            if let ViewKind::Section {
                start, end, depth, ..
            } = &mut v.kind
            {
                let look = (*end - *start).normalize_or_zero().perp();
                let mid = (*start + *end) / 2.0;
                *depth = round((now.truncate() - mid).dot(look), step).max(0.5);
            }
        }
        (ElementKind::FreeMesh(m), HandleKind::Face(f)) => {
            let n = m.to_model(m.mesh.face_normal(f)).normalize_or_zero();
            let dist = round(d.dot(n), step / 5.0);
            let verts = m.mesh.faces[f].clone();
            let local = m.to_local(n * dist);
            m.mesh.translate_vertices(&verts, local);
        }
        (ElementKind::CropBox(c), HandleKind::CropFace(i)) => {
            let axis = i / 2;
            let (at, _) = c.face(i);
            c.set_face(i, round(at[axis] + d[axis], step));
        }
        (ElementKind::CropBox(c), HandleKind::CropMove) => {
            // the grabbed corner lands on a rounded spot
            let to = DVec3::new(
                round(c.min.x + d.x, step),
                round(c.min.y + d.y, step),
                c.min.z,
            );
            let by = to - c.min;
            c.min += by;
            c.max += by;
        }
        (ElementKind::Opening(o), HandleKind::OpeningSlide) => {
            let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
                return Err("its wall is gone".into());
            };
            let along = round(d.truncate().dot(w.direction()), step);
            o.offset = (o.offset + along).clamp(0.0, (w.length() - o.width).max(0.0));
        }
        (ElementKind::Beam(b), HandleKind::BeamWidth) => {
            let n = (b.end - b.start).normalize_or_zero().perp();
            b.width = round(b.width + 2.0 * d.truncate().dot(n), step / 5.0).max(0.05);
        }
        (ElementKind::Wall(w), HandleKind::WallSlide) => {
            let n = w.direction().perp();
            let by = n * round(d.truncate().dot(n), step);
            w.start += by;
            w.end += by;
        }
        (ElementKind::Column(c), HandleKind::ColumnSize { depth }) => {
            let x = DVec2::from_angle(c.rotation.to_radians());
            let axis = if depth { x.perp() } else { x };
            // both sides move: the column stays on its point
            let by = round(2.0 * d.truncate().dot(axis), step / 5.0);
            match &mut c.profile {
                creabim_core::members::Profile::Rectangle { width, depth: dp } => {
                    let v = if depth { dp } else { width };
                    *v = (*v + by).max(0.05);
                }
                creabim_core::members::Profile::Round { diameter } => {
                    *diameter = (*diameter + by).max(0.05);
                }
            }
        }
        (_, HandleKind::Rotate) => {
            let original = originals.first().ok_or("nothing to turn")?;
            let c = pivot_of(model, original).ok_or("this doesn't turn")?;
            let (a0, a1) = (grab.truncate() - c, now.truncate() - c);
            if a1.length() < 1e-6 {
                return Err("too close to its middle".into());
            }
            let by = round(a0.angle_to(a1).to_degrees(), 5.0);
            match &mut first.kind {
                ElementKind::Object(_) => turn_object(model, first, by),
                ElementKind::Column(col) => col.rotation = (col.rotation + by).rem_euclid(360.0),
                ElementKind::Stair(s) => s.direction = (s.direction + by).rem_euclid(360.0),
                _ => return Err("this doesn't turn".into()),
            }
        }
        (_, HandleKind::Member { i, .. }) => {
            creabim_core::members::drag_grip(model, first, i, grab, now, step)?
        }
        (
            _,
            HandleKind::Vertex { .. }
            | HandleKind::Edge { .. }
            | HandleKind::AddVertex { .. }
            | HandleKind::Ring { .. }
            | HandleKind::ShaftBase
            | HandleKind::ShaftTop,
        ) => crate::editmode::reshape(model, first, kind, grab, now, step)?,
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
            "length {}, height {}, thickness {}",
            crate::units::len(w.length()),
            crate::units::len(w.height),
            crate::units::len(w.thickness)
        ),
        Some(ElementKind::Opening(o)) => format!(
            "width {}, height {}, sill {}",
            crate::units::len(o.width),
            crate::units::len(o.height),
            crate::units::len(o.sill)
        ),
        Some(ElementKind::View(v)) => match v.kind {
            ViewKind::Section {
                start, end, depth, ..
            } => format!(
                "line {} long, sees {} deep",
                crate::units::len(start.distance(end)),
                crate::units::len(depth)
            ),
            _ => String::new(),
        },
        Some(ElementKind::Slab(s)) => format!(
            "area {}, thickness {}, top {}, underside {} from its level",
            crate::units::area(s.area()),
            crate::units::len(s.thickness),
            crate::units::elevation(s.top_offset),
            s.top_offset - s.thickness
        ),
        Some(ElementKind::Column(c)) => match c.height {
            Some(h) => format!(
                "{}, height {}",
                c.profile.label_in(&crate::units::get()),
                crate::units::len(h)
            ),
            None => c.profile.label_in(&crate::units::get()),
        },
        Some(ElementKind::Beam(b)) => format!(
            "length {}, {} × {}",
            crate::units::len(b.length()),
            crate::units::len(b.width),
            crate::units::len(b.depth)
        ),
        Some(ElementKind::Roof(r)) => format!(
            "{} roof, pitch {:.1}°, {} with overhang",
            r.effective_shape().label(),
            r.pitch,
            crate::units::area(r.plan_area())
        ),
        Some(ElementKind::Stair(s)) => {
            format!(
                "width {}, treads {}",
                crate::units::len(s.width),
                crate::units::len(s.tread)
            )
        }
        Some(ElementKind::Railing(r)) => format!("height {}", crate::units::len(r.height)),
        Some(ElementKind::Shaft(s)) => format!(
            "{}, {} corners",
            crate::units::area(creabim_core::polygon::signed_area2(&s.boundary).abs() / 2.0),
            s.boundary.len()
        ),
        Some(ElementKind::CropBox(c)) => {
            let s = c.size();
            format!(
                "crop box {} × {} × {}",
                crate::units::len(s.x),
                crate::units::len(s.y),
                crate::units::len(s.z)
            )
        }
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
    /// A corner, middle or centre of `id` itself near the pointer: a drag
    /// that starts there moves the element by that point, so it lands
    /// exactly on what it snaps to.
    pub grab_snap: &'a dyn Fn(Pos2, ElementId) -> Option<DVec3>,
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
    /// Where the press was on screen: nothing moves until the pointer
    /// leaves it.
    press: Pos2,
    /// Where the pointer last put the grip, in the model.
    last: DVec3,
    /// A length being typed (Tab, or a digit, starts it): the grip goes
    /// that far from where it was grabbed, the way the pointer went.
    typed: Option<String>,
}

/// The corners and holes picked in Edit mode, on the element being
/// edited, and what a pane is doing with them: a box over the grips, the
/// right-click menu of a corner, an edge or a hole. Shared by every pane.
#[derive(Debug, Default)]
pub struct GripSel {
    /// The element the picks are on.
    pub id: Option<ElementId>,
    pub corners: Vec<(Ring, usize)>,
    pub holes: Vec<usize>,
    /// The last click on a grip: which, when and where, so a double-click
    /// still counts when the pointer moved a little between the clicks.
    last_tap: Option<(HandleKind, f64, Pos2)>,
    /// A box being dragged over the grips: the pane and where it began.
    boxsel: Option<(egui::Id, Pos2)>,
    /// The right-click menu, open in one pane.
    menu: Option<GripMenu>,
    /// A length is being typed into a drag: keys are the drag's.
    pub typing: bool,
    /// What the app should do next with Edit mode.
    pub request: Option<GripRequest>,
}

/// Something only the app can do, asked for from a grip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GripRequest {
    /// Leave Edit mode.
    Finish,
    /// Open the selected element in Edit mode.
    Enter,
}

#[derive(Clone, Copy, Debug)]
struct GripMenu {
    pane: egui::Id,
    at: Pos2,
    on: MenuOn,
}

/// What a right-click in Edit mode was on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum MenuOn {
    Corner(Ring, usize),
    /// An edge, and the model point clicked on it.
    Edge(Ring, usize, DVec3),
    Hole(usize),
    Blank,
}

/// Two clicks on the same grip this close in time (seconds) and on
/// screen (pixels) are a double-click.
const DOUBLE_TAP: f64 = 0.5;
const DOUBLE_TAP_PX: f32 = 8.0;

impl GripSel {
    pub fn is_empty(&self) -> bool {
        self.corners.is_empty() && self.holes.is_empty()
    }

    /// Drop the picks, and any box or menu. The last click is kept, so a
    /// double-click still counts while this runs every frame.
    pub fn clear(&mut self) {
        self.corners.clear();
        self.holes.clear();
        self.boxsel = None;
        self.menu = None;
    }

    /// Start afresh on element `id` (or none).
    pub fn reset(&mut self, id: Option<ElementId>) {
        if self.id != id {
            self.clear();
            self.last_tap = None;
            self.id = id;
        }
    }

    pub fn picked(&self, ring: Ring, i: usize) -> bool {
        self.corners.contains(&(ring, i))
    }

    /// Pick corner `i` of `ring`: only it, or with `add` (Shift) in or out
    /// of the picks.
    pub fn pick(&mut self, ring: Ring, i: usize, add: bool) {
        if add {
            if let Some(k) = self.corners.iter().position(|c| *c == (ring, i)) {
                self.corners.remove(k);
            } else {
                self.corners.push((ring, i));
            }
        } else {
            self.corners = vec![(ring, i)];
            self.holes.clear();
        }
    }

    /// Pick hole `k`, as [`GripSel::pick`] does a corner.
    pub fn pick_hole(&mut self, k: usize, add: bool) {
        if add {
            if let Some(j) = self.holes.iter().position(|h| *h == k) {
                self.holes.remove(j);
            } else {
                self.holes.push(k);
            }
        } else {
            self.holes = vec![k];
            self.corners.clear();
        }
    }

    /// Is a box or a menu open?
    pub fn busy(&self) -> bool {
        self.boxsel.is_some() || self.menu.is_some()
    }

    /// A click on grip `kind` at `t` (seconds) and `at` (screen): is it
    /// the second of a double-click?
    fn tap(&mut self, kind: HandleKind, t: f64, at: Pos2) -> bool {
        let double = self.last_tap.is_some_and(|(k, t0, p)| {
            k == kind && t - t0 < DOUBLE_TAP && p.distance(at) < DOUBLE_TAP_PX
        });
        self.last_tap = if double { None } else { Some((kind, t, at)) };
        double
    }
}

/// Delete or Backspace in Edit mode: remove the picked holes, else the
/// picked corners, in one undo step. Never the element itself.
pub fn delete_picked(
    doc: &mut Document,
    id: ElementId,
    sel: &mut GripSel,
) -> Result<String, String> {
    if sel.id != Some(id) || sel.is_empty() {
        return Err(
            "Select a corner to remove it: click it (Shift+click or drag a box for more), or click inside a hole to remove the hole. Esc finishes editing."
                .into(),
        );
    }
    let r = if sel.holes.is_empty() {
        crate::editmode::remove_corners(doc, id, &sel.corners)
    } else {
        crate::editmode::remove_holes(doc, id, &sel.holes)
    };
    if r.is_ok() {
        sel.clear();
    }
    r
}

const ACCENT: Color32 = Color32::from_rgb(230, 120, 30);
/// The colour of a shape that would be broken.
const INVALID: Color32 = Color32::from_rgb(210, 40, 40);

/// A short note on a light plate beside the pointer: what a grip does,
/// a live length, a typed one.
fn note(painter: &Painter, at: Pos2, text: &str, color: Color32) {
    let font = egui::FontId::proportional(12.0);
    let galley = painter.layout_no_wrap(text.to_owned(), font, color);
    let mut pos = at + Vec2::new(16.0, 14.0);
    let clip = painter.clip_rect();
    if pos.x + galley.size().x + 8.0 > clip.right() {
        pos.x = at.x - galley.size().x - 16.0;
    }
    if pos.y + galley.size().y + 6.0 > clip.bottom() {
        pos.y = at.y - galley.size().y - 18.0;
    }
    let rect = Rect::from_min_size(pos, galley.size()).expand2(Vec2::new(5.0, 3.0));
    painter.rect(
        rect,
        3.0,
        Color32::from_rgba_unmultiplied(255, 255, 255, 235),
        Stroke::new(1.0, color.gamma_multiply(0.6)),
        egui::StrokeKind::Middle,
    );
    painter.galley(pos, galley, color);
}

/// The outlines of `e` on screen, each with its ring and corners.
fn screen_rings(
    model: &Model,
    e: &Element,
    input: &PaneInput,
) -> Vec<(crate::editmode::RingInfo, Vec<Pos2>)> {
    crate::editmode::rings(model, e)
        .into_iter()
        .filter_map(|r| {
            let pts: Option<Vec<Pos2>> = r.corners.iter().map(|p| (input.project)(*p)).collect();
            Some((r, pts?))
        })
        .collect()
}

fn in_screen_polygon(pts: &[Pos2], p: Pos2) -> bool {
    let poly: Vec<DVec2> = pts
        .iter()
        .map(|q| DVec2::new(q.x as f64, q.y as f64))
        .collect();
    pts.len() >= 3 && creabim_core::polygon::contains(&poly, DVec2::new(p.x as f64, p.y as f64))
}

/// How far `p` is from segment `a`–`b` on screen, and where along it (0 to 1).
fn to_segment(a: Pos2, b: Pos2, p: Pos2) -> (f32, f32) {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    ((a + ab * t).distance(p), t)
}

/// The hole of `e` whose inside `p` (on screen) is in.
fn hole_at(model: &Model, e: &Element, input: &PaneInput, p: Pos2) -> Option<usize> {
    screen_rings(model, e, input)
        .into_iter()
        .find_map(|(r, pts)| match r.ring {
            Ring::Hole(k) if in_screen_polygon(&pts, p) => Some(k),
            _ => None,
        })
}

/// The edge of `e`'s outlines under `p` (within a few pixels), and the
/// model point there.
fn edge_at(model: &Model, e: &Element, input: &PaneInput, p: Pos2) -> Option<(Ring, usize, DVec3)> {
    let mut best: Option<(f32, Ring, usize, DVec3)> = None;
    for (r, pts) in screen_rings(model, e, input) {
        let n = pts.len();
        let edges = if r.closed { n } else { n.saturating_sub(1) };
        for i in 0..edges {
            let (d, t) = to_segment(pts[i], pts[(i + 1) % n], p);
            if d < 6.0 && best.is_none_or(|b| d < b.0) {
                let at = r.corners[i].lerp(r.corners[(i + 1) % n], t as f64);
                best = Some((d, r.ring, i, at));
            }
        }
    }
    best.map(|(_, r, i, at)| (r, i, at))
}

/// In Edit mode, the outlines being edited, traced so a hole inside a
/// slab or a shaft inside walls reads clearly; picked holes filled. A
/// `broken` shape is drawn red over them.
fn draw_rings(
    model: &Model,
    e: &Element,
    painter: &Painter,
    input: &PaneInput,
    sel: &GripSel,
    color: Color32,
) {
    for (r, pts) in screen_rings(model, e, input) {
        if pts.len() < 2 {
            continue;
        }
        let n = pts.len();
        let picked = matches!(r.ring, Ring::Hole(k) if sel.holes.contains(&k));
        if picked && n >= 3 {
            // a hole may be concave: a fan from its middle is only a tint
            painter.add(egui::Shape::closed_line(
                pts.clone(),
                Stroke::new(4.0, color.gamma_multiply(0.35)),
            ));
        }
        let edges = if r.closed { n } else { n - 1 };
        let width = if picked { 2.6 } else { 1.6 };
        for i in 0..edges {
            // a slab's bent edge is traced along its arc
            let arc = match (&e.kind, r.ring) {
                (ElementKind::Slab(s), Ring::Outline) if s.bulge(i) != 0.0 => {
                    let c = s.edge(i);
                    let z = r.corners[i].z;
                    (0..=16)
                        .map(|k| (input.project)(c.at(k as f64 / 16.0).extend(z)))
                        .collect::<Option<Vec<Pos2>>>()
                }
                _ => None,
            };
            match arc {
                Some(line) => {
                    painter.add(egui::Shape::line(line, Stroke::new(width, color)));
                }
                None => {
                    painter.line_segment([pts[i], pts[(i + 1) % n]], Stroke::new(width, color));
                }
            }
        }
    }
}

/// The live lengths while a grip of an outline is dragged: the edges
/// either side of a corner, how far an edge or a hole moved.
fn live_lengths(model: &Model, e: &Element, kind: HandleKind, grab: DVec3, now: DVec3) -> String {
    let len = crate::units::len;
    match kind {
        HandleKind::Vertex { ring, i } | HandleKind::AddVertex { ring, i } => {
            let i = if matches!(kind, HandleKind::AddVertex { .. }) {
                i + 1
            } else {
                i
            };
            let Some(r) = crate::editmode::ring_of(model, e, ring) else {
                return String::new();
            };
            let n = r.corners.len();
            if i >= n {
                return String::new();
            }
            let c = r.corners[i];
            let mut parts = vec![];
            if i > 0 || r.closed {
                parts.push(len(c.distance(r.corners[(i + n - 1) % n])));
            }
            if i + 1 < n || r.closed {
                parts.push(len(c.distance(r.corners[(i + 1) % n])));
            }
            parts.join(" · ")
        }
        HandleKind::Edge { .. } => format!("offset {}", len((now - grab).length())),
        HandleKind::Ring { .. } => format!("moved {}", len((now - grab).length())),
        _ => len((now - grab).length()),
    }
}

/// The right-click menu of Edit mode, for one frame. Runs what is picked
/// and returns the status line; `None` while nothing was picked.
fn grip_menu(
    doc: &mut Document,
    id: ElementId,
    ui_ctx: &egui::Context,
    sel: &mut GripSel,
) -> Option<String> {
    let m = sel.menu?;
    let e = doc.model().get(id)?.clone();
    let model = doc.model();
    let reshapes = |ring| crate::editmode::ring_of(model, &e, ring).is_some_and(|r| r.reshapes);
    let slab_bulge = |ring: Ring, i: usize| match (&e.kind, ring) {
        (ElementKind::Slab(s), Ring::Outline) => Some(s.bulge(i)),
        _ => None,
    };
    #[derive(Clone, Copy, PartialEq)]
    enum Act {
        RemoveCorners,
        AddCorner(Ring, usize, DVec3),
        Arc(usize, bool),
        RemoveHole(usize),
        MoveHole(usize),
        AllCorners,
        Deselect,
        Finish,
    }
    let mut act = None;
    let area = egui::Area::new(egui::Id::new(("grip-menu", m.pane)))
        .order(egui::Order::Foreground)
        .fixed_pos(m.at)
        .show(ui_ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(170.0);
                let mut item = |ui: &mut egui::Ui, on: bool, text: &str, a: Act| {
                    if ui
                        .add_enabled(on, egui::Button::new(text).frame(false))
                        .clicked()
                    {
                        act = Some(a);
                    }
                };
                match m.on {
                    MenuOn::Corner(ring, _) => {
                        let n = sel.corners.len();
                        let text = if n > 1 {
                            format!("Remove {n} corners")
                        } else {
                            "Remove corner".into()
                        };
                        item(ui, reshapes(ring), &text, Act::RemoveCorners);
                        item(ui, true, "Select all corners", Act::AllCorners);
                    }
                    MenuOn::Edge(ring, i, at) => {
                        item(
                            ui,
                            reshapes(ring),
                            "Add corner here",
                            Act::AddCorner(ring, i, at),
                        );
                        match slab_bulge(ring, i) {
                            Some(b) if b != 0.0 => item(ui, true, "Straighten", Act::Arc(i, false)),
                            Some(_) => item(ui, true, "Make arc", Act::Arc(i, true)),
                            None => {
                                ui.add_enabled(
                                    false,
                                    egui::Button::new("Make arc (slab outlines)").frame(false),
                                );
                            }
                        }
                        if let Ring::Hole(k) = ring {
                            ui.separator();
                            item(ui, true, "Remove hole", Act::RemoveHole(k));
                        }
                    }
                    MenuOn::Hole(k) => {
                        item(ui, true, "Remove hole", Act::RemoveHole(k));
                        item(ui, true, "Move hole", Act::MoveHole(k));
                    }
                    MenuOn::Blank => {
                        item(ui, true, "Select all corners", Act::AllCorners);
                        item(ui, !sel.is_empty(), "Deselect corners", Act::Deselect);
                        ui.separator();
                        item(ui, true, "Finish editing   Esc", Act::Finish);
                    }
                }
            });
        });
    let pressed_out = ui_ctx.input(|i| {
        (i.pointer.any_pressed() || i.key_pressed(egui::Key::Escape))
            && i.pointer
                .interact_pos()
                .is_none_or(|p| !area.response.rect.contains(p))
    });
    let Some(a) = act else {
        if pressed_out {
            sel.menu = None;
        }
        return None;
    };
    sel.menu = None;
    let r = match a {
        Act::RemoveCorners => delete_picked(doc, id, sel),
        Act::RemoveHole(k) => {
            let r = crate::editmode::remove_holes(doc, id, &[k]);
            if r.is_ok() {
                sel.clear();
            }
            r
        }
        Act::AddCorner(ring, i, at) => crate::editmode::add_corner(doc, id, ring, i, Some(at)),
        Act::Arc(i, on) => crate::editmode::set_arc(doc, id, i, on),
        Act::MoveHole(k) => {
            sel.pick_hole(k, false);
            Ok("Drag inside the hole to move it. Esc cancels the drag.".into())
        }
        Act::AllCorners => {
            sel.holes.clear();
            sel.corners = crate::editmode::rings(doc.model(), &e)
                .iter()
                .flat_map(|r| (0..r.corners.len()).map(move |i| (r.ring, i)))
                .collect();
            Ok(format!(
                "{} corners selected: Delete removes them.",
                sel.corners.len()
            ))
        }
        Act::Deselect => {
            sel.clear();
            Ok("Nothing picked.".into())
        }
        Act::Finish => {
            sel.request = Some(GripRequest::Finish);
            Ok("Finished editing.".into())
        }
    };
    Some(r.unwrap_or_else(|e| e))
}

/// What a selected section or elevation sees: its line, and the band
/// as deep as it looks, so it can be found and its depth dragged.
/// Whether `snapped` shows close to `free` on screen: in 3D a point that
/// lies under the pointer can be far away along the line being slid on.
/// Where handle `h` shows on screen: at its point, or nudged out along
/// its line.
fn screen_at(h: &Handle, input: &PaneInput) -> Option<Pos2> {
    let c = (input.project)(h.at)?;
    let Constraint::Line { dir, .. } = h.constraint else {
        return Some(c);
    };
    if h.nudge == 0.0 {
        return Some(c);
    }
    let tip = (input.project)(h.at + dir * 0.3)?;
    let v = (tip - c).normalized();
    Some(if v.is_finite() { c + v * h.nudge } else { c })
}

fn near_on_screen(input: &PaneInput, snapped: DVec3, free: DVec3) -> bool {
    match ((input.project)(snapped), (input.project)(free)) {
        (Some(a), Some(b)) => a.distance(b) < 16.0,
        _ => false,
    }
}

fn draw_view_range(painter: &Painter, input: &PaneInput, a: DVec2, b: DVec2, depth: f64) {
    let look = (b - a).normalize_or_zero().perp();
    let corners = [a, b, b + look * depth, a + look * depth];
    let pts: Option<Vec<Pos2>> = corners
        .iter()
        .map(|p| (input.project)(p.extend(0.0)))
        .collect();
    let Some(pts) = pts else {
        return;
    };
    painter.add(egui::Shape::convex_polygon(
        pts.clone(),
        ACCENT.gamma_multiply(0.08),
        Stroke::NONE,
    ));
    painter.line_segment([pts[0], pts[1]], Stroke::new(2.5, ACCENT));
    for (p, q) in [(pts[1], pts[2]), (pts[2], pts[3]), (pts[3], pts[0])] {
        painter.add(egui::Shape::dashed_line(
            &[p, q],
            Stroke::new(1.2, ACCENT.gamma_multiply(0.8)),
            6.0,
            4.0,
        ));
    }
}

/// What a flip arrow turns round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flip {
    /// A wall's exterior and interior.
    Wall,
    /// A door's swing, or a window's inside and outside.
    Facing,
    /// A door's hinge side.
    Hand,
    /// The way a section or elevation looks.
    View,
    /// Turn an object round to face the other way.
    Object,
}

impl Flip {
    pub fn label(self) -> &'static str {
        match self {
            Flip::Wall => "Flip wall",
            Flip::Facing => "Flip facing",
            Flip::Hand => "Flip door hand",
            Flip::View => "Flip view direction",
            Flip::Object => "Turn it round",
        }
    }
}

/// A clickable flip control: where it sits and which way its arrows point.
#[derive(Clone, Copy, Debug)]
pub struct FlipArrow {
    pub flip: Flip,
    pub at: DVec3,
    pub dir: DVec2,
}

/// The flip arrows of a selected wall, door or window, in plan: a wall's
/// beside its exterior face, square to it; a door's or window's side by
/// side on the face away from where it opens or faces. A section's or
/// elevation's sits behind its line, pointing the way it looks.
pub fn flip_arrows(model: &Model, id: ElementId) -> Vec<FlipArrow> {
    match model.get(id).map(|e| &e.kind) {
        Some(ElementKind::View(v)) => match v.kind {
            ViewKind::Section { start, end, .. } if start.distance(end) > 0.2 => {
                let along = (end - start).normalize_or_zero();
                let look = along.perp();
                let at = start + along * (start.distance(end) * 0.25).min(1.5) - look * 0.6;
                vec![FlipArrow {
                    flip: Flip::View,
                    at: at.extend(0.0),
                    dir: look,
                }]
            }
            _ => vec![],
        },
        Some(ElementKind::Object(o)) => {
            let e = model.get(id).expect("just found");
            let Some((lo, hi)) = creabim_core::objects::type_of(model, e).map(|t| t.bounds())
            else {
                return vec![];
            };
            let z = creabim_core::objects::base_z(model, e).unwrap_or(0.0);
            let side = o.to_plan(DVec2::new(hi.x + 0.4, (lo.y + hi.y) / 2.0));
            vec![FlipArrow {
                flip: Flip::Object,
                at: side.extend(z),
                dir: o.facing().normalize_or_zero(),
            }]
        }
        Some(ElementKind::Wall(w)) if w.length() > 0.1 => {
            let z = walls::base_z(model, w);
            let n = w.direction().perp();
            let out = if w.flip { n } else { -n };
            let mid = (w.start + w.end) / 2.0;
            vec![FlipArrow {
                flip: Flip::Wall,
                at: (mid + out * (w.thickness / 2.0 + 0.5)).extend(z),
                dir: n,
            }]
        }
        Some(ElementKind::Opening(o)) => {
            let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
                return vec![];
            };
            let z = walls::base_z(model, w);
            let d = w.direction();
            let n = d.perp();
            let side = if o.faces_left(w) { n } else { -n };
            let mid = w.start + d * (o.offset + o.width / 2.0);
            let mut v = vec![FlipArrow {
                flip: Flip::Facing,
                at: (mid - d * 0.3 - side * (w.thickness / 2.0 + 0.5)).extend(z),
                dir: n,
            }];
            if o.kind == OpeningKind::Door {
                v.push(FlipArrow {
                    flip: Flip::Hand,
                    at: (mid + d * 0.3 - side * (w.thickness / 2.0 + 0.5)).extend(z),
                    dir: d,
                });
            }
            v
        }
        _ => vec![],
    }
}

/// Two arrowheads back to back across `c`, along screen direction `v`.
fn draw_flip_arrow(painter: &Painter, c: Pos2, v: Vec2, hot: bool) {
    let col = if hot {
        ACCENT
    } else {
        ACCENT.gamma_multiply(0.85)
    };
    painter.circle(
        c,
        11.0,
        Color32::WHITE.gamma_multiply(0.9),
        Stroke::new(1.0, col),
    );
    let p = v.rot90();
    let stroke = Stroke::new(if hot { 2.2 } else { 1.6 }, col);
    painter.line_segment([c - v * 7.5, c + v * 7.5], stroke);
    for s in [1.0, -1.0] {
        let tip = c + v * 7.5 * s;
        let back = tip - v * 4.0 * s;
        painter.line_segment([tip, back + p * 3.5], stroke);
        painter.line_segment([tip, back - p * 3.5], stroke);
    }
}

/// Draw the selected element's handles and run a drag on them (or on the
/// element itself). Returns `true` while a drag runs, so the pane leaves
/// the pointer alone, and puts a message for the status bar in `status`.
/// In Edit mode (`edit`) it also picks corners and holes into `grips`
/// (a click, Shift+click, a box), opens their right-click menu, and moves
/// a hole dragged from inside.
#[allow(clippy::too_many_arguments)]
pub fn interact(
    doc: &mut Document,
    painter: &Painter,
    response: &egui::Response,
    selection: Option<ElementId>,
    face: Option<usize>,
    edit: bool,
    drag: &mut Option<Drag>,
    grips: &mut GripSel,
    input: &PaneInput,
    status: &mut Option<String>,
) -> bool {
    let Some(id) = selection else {
        *drag = None;
        grips.reset(None);
        return false;
    };
    grips.reset(Some(id));
    if !edit {
        grips.clear();
    }
    let ctx = painter.ctx().clone();
    let shift = ctx.input(|i| i.modifiers.shift);
    let shown: Vec<Handle> = handles_in(doc.model(), id, face, edit)
        .into_iter()
        .map(|h| in_section(h, input.look))
        .filter(|h| h.constraint.usable(input.look))
        .collect();
    let screen = |h: &Handle| screen_at(h, input);
    let nearest = |p: Pos2| {
        shown
            .iter()
            .filter_map(|h| Some((h, screen(h)?.distance(p))))
            .filter(|(_, d)| *d < 10.0)
            // a corner wins over an add-corner grip or an edge's at the
            // same spot
            .min_by(|a, b| {
                let rank = |h: &Handle| match h.kind {
                    HandleKind::Vertex { .. } => 0.0,
                    _ => 1.5,
                };
                (a.1 + rank(a.0)).total_cmp(&(b.1 + rank(b.0)))
            })
            .map(|(h, _)| *h)
    };
    let element = doc.model().get(id).cloned();
    if let (true, Some(e)) = (edit, &element) {
        draw_rings(doc.model(), e, painter, input, grips, ACCENT);
    }
    if let Some(ElementKind::View(v)) = element.as_ref().map(|e| &e.kind) {
        if let ViewKind::Section {
            start, end, depth, ..
        } = v.kind
        {
            draw_view_range(painter, input, start, end, depth);
        }
    }
    let pointer = response.hover_pos();
    let hot = pointer
        .filter(|_| drag.is_none() && !grips.busy())
        .and_then(nearest)
        .map(|h| h.kind);
    for h in &shown {
        let Some(c) = screen(h) else {
            continue;
        };
        let is_hot = hot == Some(h.kind);
        if let HandleKind::AddVertex { .. } = h.kind {
            // a small diamond with a plus: a corner to pull out
            let r = if is_hot { 6.5 } else { 5.0 };
            let pts = vec![
                c + Vec2::new(0.0, -r),
                c + Vec2::new(r, 0.0),
                c + Vec2::new(0.0, r),
                c + Vec2::new(-r, 0.0),
            ];
            let fill = if is_hot {
                Color32::from_rgb(255, 228, 200)
            } else {
                Color32::WHITE
            };
            painter.add(egui::Shape::convex_polygon(
                pts,
                fill,
                Stroke::new(1.2, ACCENT),
            ));
            let s = Stroke::new(1.0, ACCENT);
            painter.line_segment([c - Vec2::X * 2.5, c + Vec2::X * 2.5], s);
            painter.line_segment([c - Vec2::Y * 2.5, c + Vec2::Y * 2.5], s);
            continue;
        }
        if let Constraint::Line { dir, .. } = h.constraint {
            // a short stem shows which way the handle slides
            if let Some(tip) = (input.project)(h.at + dir * 0.3) {
                let v = (tip - c).normalized() * 14.0;
                if v.is_finite() {
                    painter.line_segment([c - v, c + v], Stroke::new(1.5, ACCENT));
                }
            }
        }
        // a picked corner is filled; the one under the pointer grows
        let picked = matches!(h.kind, HandleKind::Vertex { ring, i } if grips.picked(ring, i));
        let fill = if picked {
            ACCENT
        } else if is_hot {
            Color32::from_rgb(255, 228, 200)
        } else {
            Color32::WHITE
        };
        let grow = if is_hot { 1.5 } else { 0.0 };
        if h.kind.round() {
            painter.circle(c, 4.5 + grow, fill, Stroke::new(1.5, ACCENT));
        } else {
            painter.rect(
                Rect::from_center_size(c, Vec2::splat(9.0 + 2.0 * grow)),
                0.0,
                fill,
                Stroke::new(1.5, ACCENT),
                egui::StrokeKind::Middle,
            );
        }
    }
    if let (Some(k), Some(p)) = (hot, pointer) {
        let fixed = matches!(k, HandleKind::Vertex { ring, .. } | HandleKind::AddVertex { ring, .. }
            if element.as_ref().and_then(|e| crate::editmode::ring_of(doc.model(), e, ring)).is_some_and(|r| !r.reshapes));
        let tip = if fixed && edit {
            "Drag: move corner · This sketch's arcs or constraints keep its corners: no adding or removing"
        } else {
            k.tip(edit)
        };
        note(painter, p, tip, Color32::from_gray(40));
    }

    // the right-click menu, open in this pane
    if grips.menu.is_some_and(|m| m.pane == response.id) {
        if let Some(s) = grip_menu(doc, id, &ctx, grips) {
            *status = Some(s);
        }
        return true;
    }
    if edit && drag.is_none() && response.secondary_clicked() {
        if let (Some(p), Some(e)) = (response.interact_pointer_pos(), &element) {
            let on = match nearest(p).map(|h| h.kind) {
                Some(HandleKind::Vertex { ring, i }) => {
                    if !grips.picked(ring, i) {
                        grips.pick(ring, i, false);
                    }
                    MenuOn::Corner(ring, i)
                }
                Some(HandleKind::AddVertex { ring, i } | HandleKind::Edge { ring, i }) => {
                    let at = edge_at(doc.model(), e, input, p)
                        .filter(|(r, j, _)| *r == ring && *j == i)
                        .map(|(_, _, at)| at)
                        .unwrap_or_else(|| {
                            shown
                                .iter()
                                .find(|h| h.kind == HandleKind::AddVertex { ring, i })
                                .map_or(DVec3::ZERO, |h| h.at)
                        });
                    MenuOn::Edge(ring, i, at)
                }
                _ => match edge_at(doc.model(), e, input, p) {
                    Some((ring, i, at)) => MenuOn::Edge(ring, i, at),
                    None => match hole_at(doc.model(), e, input, p) {
                        Some(k) => {
                            grips.pick_hole(k, false);
                            MenuOn::Hole(k)
                        }
                        None => MenuOn::Blank,
                    },
                },
            };
            grips.menu = Some(GripMenu {
                pane: response.id,
                at: p,
                on,
            });
            return true;
        }
    }

    // a box over the grips, in Edit mode
    if let Some((pane, from)) = grips.boxsel {
        if pane == response.id {
            let to = response.interact_pointer_pos().or(pointer).unwrap_or(from);
            let rect = Rect::from_two_pos(from, to);
            painter.rect(
                rect,
                0.0,
                ACCENT.gamma_multiply(0.08),
                Stroke::new(1.0, ACCENT),
                egui::StrokeKind::Middle,
            );
            if response.drag_stopped() || !response.dragged() {
                if !shift {
                    grips.corners.clear();
                    grips.holes.clear();
                }
                for h in &shown {
                    if let HandleKind::Vertex { ring, i } = h.kind {
                        if screen(h).is_some_and(|c| rect.contains(c)) && !grips.picked(ring, i) {
                            grips.corners.push((ring, i));
                        }
                    }
                }
                grips.boxsel = None;
                *status = Some(match grips.corners.len() {
                    0 => "No corners in the box.".into(),
                    n => {
                        format!("{n} corners selected: Delete removes them, right-click for more.")
                    }
                });
            }
            return true;
        }
    }

    // a selected wall's location line, and a cross at an end that may not join
    if let Some(ElementKind::Wall(w)) = element.as_ref().map(|e| &e.kind) {
        let z = walls::base_z(doc.model(), w);
        let (a, b) = walls::location_line(doc.model(), w);
        if let (Some(pa), Some(pb)) = ((input.project)(a.extend(z)), (input.project)(b.extend(z))) {
            painter.add(egui::Shape::dashed_line(
                &[pa, pb],
                Stroke::new(1.2, ACCENT),
                8.0,
                3.0,
            ));
            for (i, p) in [pa, pb].into_iter().enumerate() {
                if w.disallow_join[i] {
                    let o = Vec2::new(10.0, -10.0);
                    let s = Stroke::new(2.0, Color32::from_rgb(200, 40, 40));
                    let c = p + o;
                    painter.line_segment([c - Vec2::splat(4.0), c + Vec2::splat(4.0)], s);
                    painter.line_segment([c + Vec2::new(-4.0, 4.0), c + Vec2::new(4.0, -4.0)], s);
                }
            }
        }
    }

    // flip arrows, in plans: a click turns the wall, door or window round
    if drag.is_none() && input.look.z.abs() > 0.9 {
        let arrows = flip_arrows(doc.model(), id);
        let mut hit = None;
        for a in &arrows {
            let (Some(c), Some(tip)) = (
                (input.project)(a.at),
                (input.project)(a.at + a.dir.extend(0.0) * 0.3),
            ) else {
                continue;
            };
            let v = (tip - c).normalized();
            if !v.is_finite() {
                continue;
            }
            let hot = pointer.is_some_and(|p| p.distance(c) < 11.0);
            if hot {
                hit = Some(a.flip);
            }
            draw_flip_arrow(painter, c, v, hot);
        }
        if let (Some(Flip::View), true) = (hit, response.clicked()) {
            let r = match transform::flip_view(doc.model(), id) {
                Some(tx) => doc.commit(tx).map_err(|e| e.to_string()),
                None => Err("only sections and elevations flip".into()),
            };
            *status = Some(match r {
                Ok(()) => {
                    "Flipped: it looks the other way along the same line. Ctrl+Z to undo.".into()
                }
                Err(e) => format!("Couldn't do that: {e}"),
            });
            return true;
        }
        if let (Some(f), true) = (hit, response.clicked()) {
            let model = doc.model().clone();
            let r = doc.edit(id, f.label(), |e| match (&mut e.kind, f) {
                (ElementKind::Object(_), Flip::Object) => turn_object(&model, e, 180.0),
                (ElementKind::Wall(w), Flip::Wall) => walls::flip_wall(w),
                (ElementKind::Opening(o), Flip::Hand) => walls::flip_hand(o),
                (ElementKind::Opening(o), _) => walls::flip_facing(o),
                _ => {}
            });
            *status = Some(match r {
                Ok(()) => format!("{}. Ctrl+Z to undo.", f.label()),
                Err(e) => format!("Couldn't do that: {e}"),
            });
            return true;
        }
        if let (Some(f), Some(p)) = (hit, pointer) {
            note(painter, p, f.label(), Color32::from_gray(40));
        }
    }

    // a click on a grip keeps the selection rather than picking what lies
    // under it; in Edit mode it picks the corner (Shift adds), and a
    // second click on the same corner takes it away
    if drag.is_none() && response.clicked() {
        if let Some(p) = response.interact_pointer_pos() {
            let hit = nearest(p).map(|h| h.kind);
            let now = ctx.input(|i| i.time);
            if let Some(k) = hit {
                // when the frames are slow both clicks can come in one
                // frame: egui still counts them as a double-click
                if response.double_clicked() {
                    grips.last_tap = Some((k, now, p));
                }
                if let Some(s) = tap_grip(doc, id, k, edit, shift, now, p, grips) {
                    *status = Some(s);
                }
                return true;
            }
            if edit {
                if let Some(k) = element
                    .as_ref()
                    .and_then(|e| hole_at(doc.model(), e, input, p))
                {
                    grips.pick_hole(k, shift);
                    *status = Some(format!(
                        "Hole {} selected: Delete removes it, a drag inside moves it.",
                        k + 1
                    ));
                    return true;
                }
                if !shift && !grips.is_empty() {
                    grips.clear();
                }
            }
        }
    }

    if drag.is_none() && response.drag_started_by(egui::PointerButton::Primary) {
        let Some(press) = ctx
            .input(|i| i.pointer.press_origin())
            .or(response.interact_pointer_pos())
        else {
            return false;
        };
        let near = nearest(press);
        // in Edit mode a drag from inside a hole moves the hole
        let hole = (near.is_none() && edit)
            .then(|| {
                element
                    .as_ref()
                    .and_then(|e| hole_at(doc.model(), e, input, press))
            })
            .flatten()
            .and_then(|k| {
                let e = element.as_ref()?;
                let r = crate::editmode::ring_of(doc.model(), e, Ring::Hole(k))?;
                let at = (input.ray)(press)
                    .and_then(|ray| {
                        Constraint::Plane {
                            point: r.corners[0],
                            normal: r.normal,
                        }
                        .hit(ray)
                    })
                    .unwrap_or(r.corners[0]);
                let h = in_section(
                    Handle {
                        kind: HandleKind::Ring {
                            ring: Ring::Hole(k),
                        },
                        at,
                        constraint: Constraint::Plane {
                            point: at,
                            normal: r.normal,
                        },
                        nudge: 0.0,
                    },
                    input.look,
                );
                h.constraint.usable(input.look).then_some(h)
            });
        let start = match near.or(hole) {
            Some(h) => {
                let grab = (input.ray)(press)
                    .and_then(|r| h.constraint.hit(r))
                    .unwrap_or(h.at);
                Some((Some(h.kind), h.constraint, grab))
            }
            // in Edit mode a drag off the grips draws a box round some
            None if edit => {
                grips.boxsel = Some((response.id, press));
                grips.menu = None;
                return true;
            }
            None => {
                let movable = !matches!(
                    element.as_ref().map(|e| &e.kind),
                    Some(
                        ElementKind::View(_)
                            | ElementKind::Dimension(_)
                            | ElementKind::Tag(_)
                            | ElementKind::Note(_)
                            | ElementKind::TextStyle(_)
                            | ElementKind::SheetTemplate(_)
                            | ElementKind::Level(_)
                            | ElementKind::Sheet(_)
                            | ElementKind::Viewport(_)
                    ) | None
                );
                (input.grab_body)(press, id).filter(|_| movable).map(|p| {
                    let p = (input.grab_snap)(press, id).unwrap_or(p);
                    let c = match element.as_ref().map(|e| &e.kind) {
                        // a grid line slides square to itself
                        Some(ElementKind::Grid(g)) => Constraint::Line {
                            point: p,
                            dir: (g.end - g.start).normalize_or_zero().perp().extend(0.0),
                        },
                        _ => Constraint::Plane {
                            point: p,
                            normal: input.body_normal,
                        },
                    };
                    (None, c, p)
                })
            }
        };
        if let Some((kind, constraint, grab)) = start {
            let (mut originals, mut joined) = match kind {
                Some(k) => originals(doc.model(), id, k),
                None => (element.clone().into_iter().collect(), vec![]),
            };
            // Ctrl or Alt drags a wall end free of the walls joined to it
            let free = matches!(kind, Some(HandleKind::WallEnd { .. }))
                && ctx.input(|i| i.modifiers.command || i.modifiers.alt);
            if free {
                originals.truncate(1);
                joined.clear();
            }
            let name = originals.first().map_or(String::new(), |e| e.name.clone());
            let label = match kind {
                Some(k) if free => format!("{} {name} free", k.verb()),
                Some(k) => k.label(&name),
                None => format!("Move {name}"),
            };
            *drag = Some(Drag {
                kind,
                constraint,
                edit: LiveEdit::new(label, originals).free(free),
                joined,
                grab,
                press,
                last: grab,
                typed: None,
            });
        }
    }

    let Some(d) = drag.as_mut() else {
        grips.typing = false;
        return false;
    };
    // Tab, or a digit, starts typing a length
    let typed_now = ctx.input_mut(|i| {
        let tab = i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
        let digit = i.events.iter().find_map(|e| match e {
            egui::Event::Text(t) if t.chars().all(|c| c.is_ascii_digit() || c == '.') => {
                Some(t.clone())
            }
            _ => None,
        });
        (tab, digit)
    });
    if d.kind.is_some() && d.typed.is_none() {
        match typed_now {
            (true, _) => d.typed = Some(String::new()),
            (_, Some(t)) => d.typed = Some(t),
            _ => {}
        }
        if d.typed.is_some() {
            return true;
        }
    }
    if d.typed.is_some() {
        grips.typing = true;
        if typed_length(doc, painter, d, input, status) {
            *drag = None;
            grips.typing = false;
        }
        return true;
    }
    grips.typing = false;
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        d.edit.cancel(doc);
        *drag = None;
        *status = Some("Drag cancelled. Esc again finishes editing.".into());
        return true;
    }
    if response.drag_stopped() || !response.dragged() {
        let kind = d.kind;
        let applied = d.edit.applied();
        if applied {
            *status = doc.undo_label().map(|l| format!("{l}. Ctrl+Z to undo."));
        }
        let press = d.press;
        *drag = None;
        match kind {
            // a press on a grip that never moved is a click on it
            Some(k) if !applied => {
                let now = ctx.input(|i| i.time);
                if let Some(s) = tap_grip(doc, id, k, edit, shift, now, press, grips) {
                    *status = Some(s);
                }
            }
            // the corner dragged stays picked
            Some(HandleKind::Vertex { ring, i }) if edit => grips.pick(ring, i, false),
            Some(HandleKind::AddVertex { ring, i }) if edit => grips.pick(ring, i + 1, false),
            Some(HandleKind::Ring {
                ring: Ring::Hole(k),
            }) if edit => grips.pick_hole(k, false),
            _ => {}
        }
        return true;
    }
    let Some(pos) = response.interact_pointer_pos() else {
        return true;
    };
    // a click held a moment, or a shaky one, moves nothing
    if pos.distance(d.press) < crate::modal::STILL && !d.edit.applied() {
        return true;
    }
    let Some(free) = (input.ray)(pos).and_then(|r| d.constraint.hit(r)) else {
        return true;
    };
    let fixed = match (d.kind, d.edit.originals().first().map(|e| &e.kind)) {
        (Some(HandleKind::WallEnd { end }), Some(ElementKind::Wall(w))) => {
            let (a, b) = walls::location_line(doc.model(), w);
            Some(if end { a } else { b }.extend(free.z))
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
            ) && near_on_screen(input, point + dir * (p - point).dot(dir), free) =>
        {
            if let Some(at) = (input.project)(p) {
                crate::snaps::draw(painter, snap, at, None);
            }
            point + dir * (p - point).dot(dir)
        }
        _ => free,
    };
    d.last = now;
    let grid = snapped.is_none_or(|(_, s)| s.kind == SnapKind::Grid);
    follow(doc, painter, d, input, now, grid, Some(pos), status);
    painter.ctx().request_repaint();
    true
}

/// A click on grip `kind`: in Edit mode it picks a corner (Shift adds or
/// drops it), and a second click on the same corner takes it away; an
/// add-corner grip only says to drag it. Outside Edit mode a double-click
/// on a corner opens Edit mode. What the status bar says.
#[allow(clippy::too_many_arguments)]
fn tap_grip(
    doc: &mut Document,
    id: ElementId,
    kind: HandleKind,
    edit: bool,
    shift: bool,
    time: f64,
    at: Pos2,
    grips: &mut GripSel,
) -> Option<String> {
    let double = grips.tap(kind, time, at);
    match kind {
        HandleKind::Vertex { ring, i } if edit => {
            if double && !shift {
                let r = crate::editmode::remove_corners(doc, id, &[(ring, i)]);
                if r.is_ok() {
                    grips.clear();
                }
                return Some(r.unwrap_or_else(|e| e));
            }
            grips.pick(ring, i, shift);
            Some(match grips.corners.len() {
                0 => "Nothing picked.".into(),
                1 => "Corner selected: Delete removes it, a drag moves it, right-click for more."
                    .into(),
                n => format!("{n} corners selected: Delete removes them."),
            })
        }
        HandleKind::Vertex { .. } if double => {
            grips.request = Some(GripRequest::Enter);
            None
        }
        HandleKind::AddVertex { .. } => Some(
            "Drag the ◇ to pull out a new corner, or right-click the edge › Add corner here."
                .into(),
        ),
        _ => None,
    }
}

/// Remake the drag with the grip at `now`: the elements it changes shown
/// in place, or (an outline that would be broken) red, with the reason.
#[allow(clippy::too_many_arguments)]
fn follow(
    doc: &mut Document,
    painter: &Painter,
    d: &mut Drag,
    input: &PaneInput,
    now: DVec3,
    grid: bool,
    pointer: Option<Pos2>,
    status: &mut Option<String>,
) {
    let model = doc.model().clone();
    let result = match d.kind {
        Some(k) => apply(&model, d.edit.originals(), &d.joined, k, d.grab, now, 0.05),
        None => {
            let delta = now - d.grab;
            let delta = if grid {
                (delta / 0.05).round() * 0.05
            } else {
                delta
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
    let outline = matches!(
        d.kind,
        Some(
            HandleKind::Vertex { .. }
                | HandleKind::AddVertex { .. }
                | HandleKind::Edge { .. }
                | HandleKind::Ring { .. }
        )
    );
    match result {
        Ok(changed) => {
            let mut said = match d.kind {
                // a whole element moved: how far
                None => format!(
                    "{}: {}",
                    d.edit.label(),
                    crate::units::len((now - d.grab).length())
                ),
                Some(_) => format!("{}  {}", d.edit.label(), readout(&changed)),
            };
            if let Some(ElementKind::Shaft(s)) = changed.first().map(|e| &e.kind) {
                let missed: Vec<String> = creabim_core::slab::shaft_misses(&model, s)
                    .into_iter()
                    .filter_map(|m| model.get(m).map(|e| e.name.clone()))
                    .collect();
                if !missed.is_empty() {
                    said += &format!(
                        "  Warning: it doesn't cut {}, as it crosses the edge",
                        missed.join(", ")
                    );
                }
            }
            if let (true, Some(k), Some(p), Some(e)) = (outline, d.kind, pointer, changed.first()) {
                note(painter, p, &live_lengths(&model, e, k, d.grab, now), ACCENT);
            }
            *status = Some(said);
            if let Err(e) = d.edit.update(doc, changed) {
                *status = Some(format!("Can't go there: {e}"));
            }
        }
        Err(e) => {
            // the broken shape in red, never kept: the last good one stays
            if let (true, Some(k), Some(orig)) = (outline, d.kind, d.edit.originals().first()) {
                let mut broken = orig.clone();
                if crate::editmode::reshape_unchecked(&model, &mut broken, k, d.grab, now, 0.05)
                    .is_ok()
                {
                    draw_rings(
                        &model,
                        &broken,
                        painter,
                        input,
                        &GripSel::default(),
                        INVALID,
                    );
                }
                if let Some(p) = pointer {
                    note(painter, p, &format!("Not allowed: {e}"), INVALID);
                }
            }
            *status = Some(format!(
                "Can't go there: {e}. Release keeps the last good shape; Esc cancels."
            ));
        }
    }
}

/// A length typed into a drag: shown by the grip; Enter moves the grip
/// that far from where it was grabbed, the way the pointer went, and
/// finishes; Esc goes back to following the pointer. True when the drag
/// is done.
fn typed_length(
    doc: &mut Document,
    painter: &Painter,
    d: &mut Drag,
    input: &PaneInput,
    status: &mut Option<String>,
) -> bool {
    let ctx = painter.ctx().clone();
    let (text, back, enter, esc) = ctx.input_mut(|i| {
        let text: String = i
            .events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        (
            text,
            i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace),
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
        )
    });
    let Some(t) = d.typed.as_mut() else {
        return false;
    };
    for c in text.chars() {
        match c {
            '0'..='9' | '.' | '-' | '\'' | '"' | '/' | ' ' => t.push(c),
            ',' => t.push('.'),
            'm' | 'c' | 'f' | 't' | 'i' | 'n' if !t.is_empty() => t.push(c),
            _ => {}
        }
    }
    if back {
        t.pop();
    }
    if esc {
        d.typed = None;
        *status = Some("Typing cancelled: the grip follows the pointer again.".into());
        return false;
    }
    let dir = match d.constraint {
        Constraint::Line { dir, .. } => {
            let s = (d.last - d.grab).dot(dir);
            if s < 0.0 {
                -dir
            } else {
                dir
            }
        }
        Constraint::Plane { .. } => (d.last - d.grab).normalize_or_zero(),
    };
    let shown = t.clone();
    let at = (input.project)(d.grab + dir * crate::units::parse(&shown).unwrap_or(0.0));
    if let Some(p) = at.or((input.project)(d.grab)) {
        note(
            painter,
            p,
            &format!(
                "{shown}▏ {}  Enter: apply · Esc: back to the pointer",
                crate::units::symbol()
            ),
            ACCENT,
        );
    }
    *status = Some(format!(
        "Type a length ({}) and press Enter: the grip goes that far the way you dragged it.",
        crate::units::symbol()
    ));
    if !enter {
        return false;
    }
    let Some(v) = crate::units::parse(&shown) else {
        *status = Some(format!("“{shown}” isn't a length."));
        return false;
    };
    if dir == DVec3::ZERO {
        *status = Some("Drag the grip a little first, to show which way it goes.".into());
        return false;
    }
    let now = d.grab + dir * v;
    follow(doc, painter, d, input, now, false, None, status);
    if d.edit.applied() {
        *status = doc.undo_label().map(|l| format!("{l}. Ctrl+Z to undo."));
    }
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
    fn a_wall_end_is_gripped_on_its_location_line_and_may_be_left_unjoined() {
        let m = sample_house();
        let (id, mut w) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some((e.id, w.clone())),
                _ => None,
            })
            .unwrap();
        w.location = creabim_core::LocationLine::ExteriorFace;
        w.disallow_join[1] = true;
        let mut doc = Document::from_model(m);
        doc.edit(id, "edit", |e| e.kind = ElementKind::Wall(w.clone()))
            .unwrap();
        let m = doc.model().clone();
        let kind = HandleKind::WallEnd { end: true };
        let grip = handles(&m, id, None)
            .into_iter()
            .find(|h| h.kind == kind)
            .unwrap();
        let (_, b) = walls::location_line(&m, &w);
        assert!(
            grip.at.truncate().distance(b) < 1e-9,
            "on the exterior face"
        );
        assert!(b.distance(w.end) > 0.05);
        // the end may not join: nothing comes along
        let (orig, joined) = originals(&m, id, kind);
        assert!(joined.is_empty() && orig.len() == 1);
        let to = (b + w.direction() * 0.5).extend(0.0);
        let out = apply(&m, &orig, &joined, kind, grip.at, to, 0.05).unwrap();
        let ElementKind::Wall(n) = &out[0].kind else {
            panic!()
        };
        let (_, nb) = walls::location_line(&m, n);
        assert!(nb.distance(to.truncate()) < 1e-9);
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
        // the top stayed at the level
        assert!(s.top_offset.abs() < 1e-12);
    }

    #[test]
    fn pulling_a_slab_top_moves_the_top_and_keeps_the_underside() {
        let m = sample_house();
        let (id, t) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Slab(s) => Some((e.id, s.thickness)),
                _ => None,
            })
            .unwrap();
        let hs = handles(&m, id, None);
        let top = hs.iter().find(|h| h.kind == HandleKind::SlabTop).unwrap();
        let under = hs
            .iter()
            .find(|h| h.kind == HandleKind::SlabUnderside)
            .unwrap();
        assert!((top.at.z - under.at.z - t).abs() < 1e-9);
        let (orig, _) = originals(&m, id, top.kind);
        let out = apply(
            &m,
            &orig,
            &[],
            top.kind,
            top.at,
            top.at + DVec3::Z * 0.1,
            0.05,
        )
        .unwrap();
        let ElementKind::Slab(s) = &out[0].kind else {
            panic!()
        };
        assert!((s.thickness - (t + 0.1)).abs() < 1e-9);
        assert!((s.top_offset - 0.1).abs() < 1e-9);
        assert!((s.top_offset - s.thickness + t).abs() < 1e-9);
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
        use crate::editmode::{add_corner, remove_corners};
        add_corner(&mut doc, id, Ring::Outline, 0, None).unwrap();
        let corners = |doc: &Document| match &doc.model().get(id).unwrap().kind {
            ElementKind::Slab(s) => s.boundary.clone(),
            _ => panic!(),
        };
        let b = corners(&doc);
        assert_eq!(b.len(), n + 1);
        assert!(b[1].distance((b[0] + b[2]) / 2.0) < 1e-9);
        remove_corners(&mut doc, id, &[(Ring::Outline, 1)]).unwrap();
        assert_eq!(corners(&doc).len(), n);
        while corners(&doc).len() > 3 {
            remove_corners(&mut doc, id, &[(Ring::Outline, 0)]).unwrap();
        }
        assert!(remove_corners(&mut doc, id, &[(Ring::Outline, 0)]).is_err());
    }

    #[test]
    fn plan_handles_slide_along_a_section() {
        let m = sample_house();
        let (id, _) = wall_with_door(&m);
        // a section looking north: wall ends slide east and west
        let look = DVec3::Y;
        let end = handles(&m, id, None)
            .into_iter()
            .find(|h| matches!(h.kind, HandleKind::WallEnd { .. }))
            .unwrap();
        assert!(!end.constraint.usable(look));
        let h = in_section(end, look);
        assert!(h.constraint.usable(look));
        let Constraint::Line { dir, .. } = h.constraint else {
            panic!()
        };
        assert!(dir.dot(look).abs() < 1e-9 && dir.z.abs() < 1e-9);
        // a ray straight into the paper lands on the line, at the pointer
        let ray = Ray {
            o: DVec3::new(3.0, -50.0, end.at.z),
            d: look,
        };
        let p = h.constraint.hit(ray).unwrap();
        assert!((p.x - 3.0).abs() < 1e-9);
        // plans keep the handle as it was
        assert_eq!(in_section(end, -DVec3::Z), end);
    }

    fn a_slab(doc: &Document) -> (ElementId, Vec<DVec2>) {
        doc.model()
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Slab(s) => Some((e.id, s.boundary.clone())),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn a_double_click_on_an_add_corner_grip_changes_nothing() {
        let mut doc = Document::from_model(sample_house());
        let (id, before) = a_slab(&doc);
        let mut sel = GripSel::default();
        sel.reset(Some(id));
        let k = HandleKind::AddVertex {
            ring: Ring::Outline,
            i: 0,
        };
        let p = Pos2::new(100.0, 100.0);
        for t in [0.0, 0.2] {
            tap_grip(&mut doc, id, k, true, false, t, p, &mut sel);
        }
        assert_eq!(a_slab(&doc).1, before);
        assert!(!doc.can_undo(), "no undo step at all");
    }

    #[test]
    fn a_double_click_on_a_corner_takes_it_away_even_after_a_small_move() {
        let mut doc = Document::from_model(sample_house());
        let (id, _) = a_slab(&doc);
        doc.edit(id, "five", |e| {
            if let ElementKind::Slab(s) = &mut e.kind {
                s.split_edge(0);
            }
        })
        .unwrap();
        let n = a_slab(&doc).1.len();
        let mut sel = GripSel::default();
        sel.reset(Some(id));
        let k = HandleKind::Vertex {
            ring: Ring::Outline,
            i: 1,
        };
        // the first click picks it
        tap_grip(
            &mut doc,
            id,
            k,
            true,
            false,
            1.0,
            Pos2::new(50.0, 50.0),
            &mut sel,
        );
        assert!(sel.picked(Ring::Outline, 1));
        assert_eq!(a_slab(&doc).1.len(), n);
        // the second, three pixels off, takes it away
        tap_grip(
            &mut doc,
            id,
            k,
            true,
            false,
            1.3,
            Pos2::new(53.0, 50.0),
            &mut sel,
        );
        assert_eq!(a_slab(&doc).1.len(), n - 1);
        // clicks far apart in time only pick
        tap_grip(
            &mut doc,
            id,
            k,
            true,
            false,
            5.0,
            Pos2::new(50.0, 50.0),
            &mut sel,
        );
        tap_grip(
            &mut doc,
            id,
            k,
            true,
            false,
            6.0,
            Pos2::new(50.0, 50.0),
            &mut sel,
        );
        assert_eq!(a_slab(&doc).1.len(), n - 1);
        // Shift+click adds to the picks, and again takes away
        let other = HandleKind::Vertex {
            ring: Ring::Outline,
            i: 0,
        };
        tap_grip(
            &mut doc,
            id,
            other,
            true,
            true,
            8.0,
            Pos2::new(0.0, 0.0),
            &mut sel,
        );
        assert_eq!(sel.corners.len(), 2);
        tap_grip(
            &mut doc,
            id,
            other,
            true,
            true,
            9.0,
            Pos2::new(0.0, 0.0),
            &mut sel,
        );
        assert_eq!(sel.corners.len(), 1);
        // outside Edit mode a double-click on a corner asks for Edit mode
        let mut out = GripSel::default();
        tap_grip(&mut doc, id, k, false, false, 10.0, Pos2::ZERO, &mut out);
        // the picks are cleared every frame outside Edit mode; the click
        // must survive that
        out.clear();
        tap_grip(&mut doc, id, k, false, false, 10.2, Pos2::ZERO, &mut out);
        assert_eq!(out.request, Some(GripRequest::Enter));
        assert_eq!(a_slab(&doc).1.len(), n - 1);
    }

    #[test]
    fn delete_picked_removes_holes_or_corners_and_never_the_slab() {
        let mut doc = Document::from_model(sample_house());
        let (id, b) = a_slab(&doc);
        let c = b.iter().copied().sum::<DVec2>() / b.len() as f64;
        doc.edit(id, "hole", |e| {
            if let ElementKind::Slab(s) = &mut e.kind {
                s.holes = vec![creabim_core::polygon::rectangle(c, c + DVec2::ONE)];
            }
        })
        .unwrap();
        let mut sel = GripSel::default();
        sel.reset(Some(id));
        assert!(delete_picked(&mut doc, id, &mut sel).is_err());
        sel.pick_hole(0, false);
        delete_picked(&mut doc, id, &mut sel).unwrap();
        let ElementKind::Slab(s) = &doc.model().get(id).unwrap().kind else {
            panic!()
        };
        assert!(s.holes.is_empty());
        // a triangle keeps its corners
        sel.corners = (0..b.len()).map(|i| (Ring::Outline, i)).collect();
        assert!(delete_picked(&mut doc, id, &mut sel).is_err());
        assert!(doc.model().get(id).is_some());
    }

    fn drag_to(
        m: &Model,
        id: ElementId,
        kind: HandleKind,
        to: impl Fn(&Handle) -> DVec3,
    ) -> Element {
        let h = handles_in(m, id, None, false)
            .into_iter()
            .find(|h| h.kind == kind)
            .unwrap_or_else(|| panic!("no {kind:?}"));
        let (orig, joined) = originals(m, id, kind);
        apply(m, &orig, &joined, kind, h.at, to(&h), 0.05).unwrap()[0].clone()
    }

    #[test]
    fn a_wall_slides_square_to_itself() {
        let m = sample_house();
        let (id, w) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some((e.id, w.clone())),
                _ => None,
            })
            .unwrap();
        let n = w.direction().perp();
        let out = drag_to(&m, id, HandleKind::WallSlide, |h| {
            h.at + n.extend(0.0) * 0.5
        });
        let ElementKind::Wall(s) = &out.kind else {
            panic!()
        };
        assert!(s.start.distance(w.start + n * 0.5) < 1e-9);
        assert!(s.end.distance(w.end + n * 0.5) < 1e-9);
        assert!((s.thickness - w.thickness).abs() < 1e-12);
    }

    #[test]
    fn a_column_turns_and_changes_width_and_depth() {
        let m = creabim_core::sample::layered_house();
        let Some(id) = m
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Column(_)))
            .map(|e| e.id)
        else {
            return;
        };
        let ElementKind::Column(c) = &m.get(id).unwrap().kind else {
            panic!()
        };
        // a quarter turn about its point
        let out = drag_to(&m, id, HandleKind::Rotate, |h| {
            let r = h.at.truncate() - c.position;
            (c.position + r.perp()).extend(h.at.z)
        });
        let ElementKind::Column(t) = &out.kind else {
            panic!()
        };
        assert!(((t.rotation - c.rotation).rem_euclid(360.0) - 90.0).abs() < 1e-6);
        let creabim_core::members::Profile::Rectangle { width, .. } = c.profile else {
            return;
        };
        let x = DVec2::from_angle(c.rotation.to_radians());
        let out = drag_to(&m, id, HandleKind::ColumnSize { depth: false }, |h| {
            h.at + x.extend(0.0) * 0.05
        });
        let ElementKind::Column(t) = &out.kind else {
            panic!()
        };
        let creabim_core::members::Profile::Rectangle { width: w2, .. } = t.profile else {
            panic!()
        };
        assert!((w2 - (width + 0.1)).abs() < 1e-9);
        assert!(t.position.distance(c.position) < 1e-12);
    }

    #[test]
    fn an_object_turns_about_its_middle_and_turns_round() {
        let m = creabim_core::starter::starter_house();
        let Some(e) = m
            .iter()
            .find(|e| {
                matches!(e.kind, ElementKind::Object(_))
                    && creabim_core::objects::type_of(&m, e).is_some()
            })
            .cloned()
        else {
            return;
        };
        let mid = pivot_of(&m, &e).unwrap();
        let mut turned = e.clone();
        turn_object(&m, &mut turned, 180.0);
        // the footprint stays where it was, facing the other way
        assert!(pivot_of(&m, &turned).unwrap().distance(mid) < 1e-9);
        let (ElementKind::Object(a), ElementKind::Object(b)) = (&e.kind, &turned.kind) else {
            panic!()
        };
        assert!((a.facing() + b.facing()).length() < 1e-9);
        assert!(handles(&m, e.id, None)
            .iter()
            .any(|h| h.kind == HandleKind::Rotate));
        assert!(flip_arrows(&m, e.id).iter().any(|f| f.flip == Flip::Object));
    }

    #[test]
    fn a_door_slides_along_its_wall_at_the_same_width() {
        let m = sample_house();
        let (_, door) = wall_with_door(&m);
        let ElementKind::Opening(o) = &m.get(door).unwrap().kind else {
            panic!()
        };
        let h = handles(&m, door, None)
            .into_iter()
            .find(|h| h.kind == HandleKind::OpeningSlide)
            .unwrap();
        let Constraint::Line { dir, .. } = h.constraint else {
            panic!()
        };
        let out = drag_to(&m, door, HandleKind::OpeningSlide, |h| h.at - dir * 0.1);
        let ElementKind::Opening(n) = &out.kind else {
            panic!()
        };
        assert!((n.offset - (o.offset - 0.1)).abs() < 1e-9);
        assert!((n.width - o.width).abs() < 1e-12);
    }
}
