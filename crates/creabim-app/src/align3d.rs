//! The Align tool in 3D: in the 3D window and in 3D views. Click a
//! reference (a face, an edge or a point of another element, a grid line,
//! or empty space for a level's plane), then a face of what to align.
//! Stretch moves that face to the reference and the opposite face stays,
//! by the same per-kind rules as Move's face mode (`creabim_core::pushpull`);
//! Align moves the whole element flush, turning it first when both are
//! upright; Extend runs a wall's or beam's nearer end to it. What is under
//! the pointer is read as the pre-selection reads it, so Tab reaches a face
//! behind or inside another. The reference holds in every 3D pane.

use crate::aligntool::{stretch_tx, turn_to, AlignMode, AlignState, PARALLEL_TOL_DEG};
use crate::centres::{self, CentrePlane};
use crate::modal::FacePick;
use crate::selection::Selection;
use crate::snaps::{self, Snap, SnapKind};
use creabim_core::pushpull::FaceRef;
use creabim_core::transform::transformed;
use creabim_core::{Document, ElementId, ElementKind, Model, Transaction, Xform};
use creabim_geom::{pick, SceneItem};
use eframe::egui::{self, Color32, Pos2, Stroke};
use glam::{DVec2, DVec3};

/// Centre lines are drawn in this colour.
pub const CENTRE: Color32 = Color32::from_rgb(150, 60, 200);

/// What a reference is, in model coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A face, a grid line's upright plane or a level's: a point on it
    /// and its normal.
    Plane { point: DVec3, normal: DVec3 },
    /// An edge, through two points.
    Line { a: DVec3, b: DVec3 },
    /// A corner, a middle or a crossing.
    Point(DVec3),
}

/// The picked reference: its shape, the element it is part of, what to
/// call it, and a face's triangles to show it by.
#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    pub shape: Shape,
    pub id: Option<ElementId>,
    pub what: String,
    pub tris: Vec<[DVec3; 3]>,
}

/// What is under the pointer in a 3D pane, for the tool to take.
#[derive(Clone, Debug, Default)]
pub struct Under {
    /// The face under it: of what Tab moved the pre-selection to, else
    /// the nearest.
    pub face: Option<FacePick>,
    /// An object snap there (not the bare increment).
    pub snap: Option<Snap>,
    /// The edge that snap lies on.
    pub edge: Option<(DVec3, DVec3)>,
    /// The level whose plane a click on empty space takes.
    pub level: Option<(ElementId, f64)>,
    /// The centre planes of the face's element that cross the face, with
    /// where they cross it (see [`centres_under`]).
    pub centres: Vec<(CentrePlane, DVec3, DVec3)>,
    /// The one of them the pointer is on.
    pub centre: Option<usize>,
}

impl Under {
    /// What a click aligns: the centre plane the pointer is on, standing
    /// for its element as a face would, else the face.
    pub fn target(&self) -> Option<(FacePick, bool)> {
        let f = self.face.as_ref()?;
        Some(match self.centre.and_then(|i| self.centres.get(i)) {
            Some((c, ..)) => (
                FacePick {
                    id: f.id,
                    face: FaceRef {
                        point: c.point,
                        normal: c.normal,
                        mesh_face: None,
                    },
                    tris: vec![],
                },
                true,
            ),
            None => (f.clone(), false),
        })
    }
}

/// Does a click take a centre line under the pointer? Not as what to
/// stretch: Stretch moves faces, and a small face's centre line would
/// cover it.
pub fn takes_centres(state: &AlignState) -> bool {
    state.reference_3d.is_none() || state.mode != AlignMode::Stretch
}

/// The centre planes of `face`'s element that cross it, where they cross
/// it, and the one within a few pixels of `pointer`.
pub fn centres_under(
    model: &Model,
    scene: &[SceneItem],
    face: Option<&FacePick>,
    pointer: Pos2,
    project: &dyn Fn(DVec3) -> Option<Pos2>,
) -> (Vec<(CentrePlane, DVec3, DVec3)>, Option<usize>) {
    let Some(f) = face else {
        return (vec![], None);
    };
    let traces: Vec<(CentrePlane, DVec3, DVec3)> = centres::of(model, scene, f.id)
        .into_iter()
        .filter_map(|c| {
            let (a, b) = centres::trace_on(&c, f.face.point, f.face.normal)?;
            let (a, b) = centres::clip_to(a, b, f.face.normal, &f.tris)?;
            Some((c, a, b))
        })
        .collect();
    let to = |p: Pos2| DVec2::new(p.x as f64, p.y as f64);
    let hit = traces
        .iter()
        .enumerate()
        .filter_map(|(i, (_, a, b))| {
            let d = centres::segment_distance(to(pointer), to(project(*a)?), to(project(*b)?));
            (d < 5.0).then_some((i, d))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|h| h.0);
    (traces, hit)
}

/// The tool's hint for the status bar in 3D.
pub fn hint(state: &AlignState) -> String {
    match (&state.reference_3d, state.mode) {
        (None, _) => "Align in 3D: click the reference: a face, edge, corner or centre line (dashed purple) of another element, a grid line, or empty space for the working level's plane.".into(),
        (Some(r), AlignMode::Align) => format!("Reference: {}. Now click a face of what to move flush to it, or its centre line to centre it there. Tab picks what is behind. Esc lets go of the reference.", r.what),
        (Some(r), AlignMode::Extend) => format!("Reference: {}. Now click near the end of a wall or beam to run it to the reference; other things stretch the face clicked. Esc lets go of the reference.", r.what),
        (Some(r), AlignMode::Stretch) => format!("Reference: {}. Now click the face to stretch to it; the opposite face stays. Tab picks a face behind. Esc lets go of the reference.", r.what),
    }
}

/// The face of what `chosen` names under the ray `o`, `d` (else the
/// nearest face), with the mesh face it is for a mass.
pub fn face_pick(
    model: &Model,
    scene: &[SceneItem],
    o: DVec3,
    d: DVec3,
    chosen: Option<ElementId>,
) -> Option<FacePick> {
    let (o32, d32) = (o.as_vec3(), d.as_vec3());
    let f = chosen
        .and_then(|id| pick::pick_face_of(scene, o32, d32, id))
        .or_else(|| pick::pick_face(scene, o32, d32))?;
    let mesh_face = match model.get(f.hit.id).map(|e| &e.kind) {
        Some(ElementKind::FreeMesh(m)) => {
            let ev = m.evaluated();
            pick::face_of_triangle(&ev.mesh, f.hit.triangle)
                .and_then(|i| ev.source.get(i).copied().flatten())
        }
        _ => None,
    };
    Some(FacePick {
        id: f.hit.id,
        face: FaceRef {
            point: f.point,
            normal: f.normal,
            mesh_face,
        },
        tris: f.triangles,
    })
}

/// Which way a face looks, in words.
fn facing(n: DVec3) -> &'static str {
    if n.z > 0.9 {
        "top"
    } else if n.z < -0.9 {
        "underside"
    } else if n.z.abs() < 0.1 {
        "side"
    } else {
        "sloped face"
    }
}

fn name_of(model: &Model, id: ElementId) -> String {
    crate::hoverpick::describe(model, id)
}

/// The reference a click on `under` picks: a corner or middle snapped
/// to, a grid line, an edge, the face under the pointer, else the
/// level's plane.
pub fn reference_at(model: &Model, under: &Under) -> Option<Reference> {
    if let Some(s) = under.snap {
        // a face's centre stands for the face
        if s.kind.is_point() && s.kind != SnapKind::Center {
            return Some(Reference {
                shape: Shape::Point(s.p),
                id: s.host,
                what: match s.kind {
                    SnapKind::Endpoint => "an end point".into(),
                    SnapKind::Midpoint => "a midpoint".into(),
                    SnapKind::Intersection | SnapKind::GuideCross => "a crossing".into(),
                    _ => "a point".into(),
                },
                tris: vec![],
            });
        }
    }
    if let Some((c, ..)) = under.centre.and_then(|i| under.centres.get(i)) {
        let k = c.corners;
        return Some(Reference {
            shape: Shape::Plane {
                point: c.point,
                normal: c.normal,
            },
            id: Some(c.id),
            what: format!("the centre line of {}", name_of(model, c.id)),
            tris: vec![[k[0], k[1], k[2]], [k[0], k[2], k[3]]],
        });
    }
    if let Some(s) = under.snap {
        if let Some((id, ElementKind::Grid(g))) =
            s.host.and_then(|h| Some((h, &model.get(h)?.kind)))
        {
            let along = (g.end - g.start).normalize_or_zero();
            if along != DVec2::ZERO {
                return Some(Reference {
                    shape: Shape::Plane {
                        point: g.start.extend(s.p.z),
                        normal: along.perp().extend(0.0),
                    },
                    id: Some(id),
                    what: format!("grid line {}", g.label),
                    tris: vec![],
                });
            }
        }
        if let (SnapKind::OnEdge | SnapKind::Guide, Some((a, b))) = (s.kind, under.edge) {
            let what = match s.host {
                Some(h) if model.get(h).is_some() => format!("an edge of {}", name_of(model, h)),
                _ => "an edge".into(),
            };
            return Some(Reference {
                shape: Shape::Line { a, b },
                id: s.host,
                what,
                tris: vec![],
            });
        }
    }
    if let Some(f) = &under.face {
        return Some(Reference {
            shape: Shape::Plane {
                point: f.face.point,
                normal: f.face.normal,
            },
            id: Some(f.id),
            what: format!("the {} of {}", facing(f.face.normal), name_of(model, f.id)),
            tris: f.tris.clone(),
        });
    }
    let (id, z) = under.level?;
    let name = model.get(id).map_or("the level".into(), |e| e.name.clone());
    Some(Reference {
        shape: Shape::Plane {
            point: DVec3::new(0.0, 0.0, z),
            normal: DVec3::Z,
        },
        id: Some(id),
        what: format!("{name}'s plane"),
        tris: vec![],
    })
}

/// How far `face` moves out along its normal to meet the reference: to
/// a plane within [`PARALLEL_TOL_DEG`] of it, an edge running along it,
/// or a point. The point picked on the face is what meets it.
pub fn reach(shape: &Shape, face: &FaceRef) -> Result<f64, String> {
    let (p, n) = (face.point, face.normal);
    let tol = PARALLEL_TOL_DEG.to_radians();
    match *shape {
        Shape::Plane { point, normal } => {
            let c = n.dot(normal);
            if c.abs() < tol.cos() {
                let off = c.abs().clamp(0.0, 1.0).acos().to_degrees();
                return Err(format!("That face is {off:.0}° off the reference: pick a face within {PARALLEL_TOL_DEG:.0}° of parallel to it."));
            }
            Ok((point - p).dot(normal) / c)
        }
        Shape::Line { a, b } => {
            let e = (b - a).normalize_or_zero();
            if e.dot(n).abs() > tol.sin() {
                return Err("That edge runs into the face rather than along it: pick a face it lies parallel to.".into());
            }
            let q = a + e * (p - a).dot(e);
            Ok((q - p).dot(n))
        }
        Shape::Point(q) => Ok((q - p).dot(n)),
    }
}

/// The reference's direction across the level, when it is upright: what
/// Align turns an upright face parallel to.
fn plan_direction(shape: &Shape) -> Option<DVec2> {
    match *shape {
        Shape::Plane { normal, .. } if normal.z.abs() < 0.1 => {
            Some(normal.truncate().normalize_or_zero().perp())
        }
        Shape::Line { a, b } => {
            let d = b - a;
            (d.normalize_or_zero().z.abs() < 0.1 && d.truncate().length() > 1e-9)
                .then(|| d.truncate().normalize())
        }
        _ => None,
    }
}

/// Align: the element the face is of (a door's or window's wall) turned
/// parallel to the reference when both are upright, then moved square to
/// its face until that face is flush with it. By its `centre` line a door
/// or window itself slides along its wall instead.
fn align_tx(
    model: &Model,
    shape: &Shape,
    pick: &FacePick,
    centre: bool,
) -> Result<(Transaction, String), String> {
    let e = model.get(pick.id).ok_or("Click a face of what to align.")?;
    // a face of a hosted door or window is its wall's: the wall moves
    let e = match &e.kind {
        ElementKind::Opening(o) if !centre => model.get(o.host).unwrap_or(e),
        _ => e,
    };
    let fail = || format!("{} can't be aligned.", e.name);
    let (p, mut n) = (pick.face.point, pick.face.normal);
    let mut now = e.clone();
    if let (true, Some(r)) = (n.z.abs() < 0.1, plan_direction(shape)) {
        let deg = turn_to(n.truncate().perp(), r);
        if deg.abs() > 1e-9 {
            now = transformed(
                model,
                &now,
                &Xform::Rotate {
                    center: p.truncate(),
                    degrees: deg,
                },
            )
            .ok_or_else(fail)?;
            n = DVec2::from_angle(deg.to_radians())
                .rotate(n.truncate())
                .extend(n.z);
        }
    }
    let face = FaceRef {
        normal: n,
        ..pick.face
    };
    let s = reach(shape, &face)?;
    if s.abs() > 1e-9 {
        now = transformed(model, &now, &Xform::Move(n * s)).ok_or_else(fail)?;
    }
    if now == *e {
        return Err("Already aligned.".into());
    }
    let name = e.name.clone();
    Ok((
        Transaction::new("Align").replace(e.clone(), now),
        format!("Aligned {name} to the reference. Click more faces to align them to it; Esc lets it go."),
    ))
}

/// Extend: the end of a wall, beam or grid line nearer `at` run to where
/// its line meets the reference. `None` when it isn't one of those, or the
/// reference is level (nothing across the plan to run to).
fn extend_tx(
    model: &Model,
    shape: &Shape,
    pick: &FacePick,
    at: DVec3,
) -> Option<Result<(Transaction, String), String>> {
    let e = model.get(pick.id)?;
    let (near_start, hit) = match extend_end(&e.kind, shape, at)? {
        Ok(v) => v,
        Err(m) => return Some(Err(m)),
    };
    let mut now = e.clone();
    let (a, b) = match &mut now.kind {
        ElementKind::Wall(w) => (&mut w.start, &mut w.end),
        ElementKind::Beam(b) => (&mut b.start, &mut b.end),
        ElementKind::Grid(g) => (&mut g.start, &mut g.end),
        _ => return None,
    };
    *(if near_start { a } else { b }) = hit;
    if let ElementKind::Wall(w) = &now.kind {
        if w.length() < 0.01 {
            return Some(Err("That would leave the wall with no length.".into()));
        }
    }
    if now == *e {
        return Some(Err("Already there.".into()));
    }
    let name = e.name.clone();
    // walls joined at the end that moved follow it
    let mut changed = vec![now];
    let follow = creabim_core::joins::follow(model, &mut changed);
    let mut tx = Transaction::new("Extend to reference");
    for n in changed {
        if let Some(o) = model.get(n.id).filter(|o| **o != n) {
            tx = tx.replace(o.clone(), n);
        }
    }
    for (o, n) in follow {
        tx = tx.replace(o, n);
    }
    Some(Ok((
        tx,
        format!("Extended {name} to the reference. Click more to run them to it; Esc lets it go."),
    )))
}

/// Where Extend takes the linear element `pick` is on: the end nearer
/// `at` and where it goes, drawn before the click.
pub fn extend_preview(
    model: &Model,
    shape: &Shape,
    pick: &FacePick,
    at: DVec3,
) -> Option<(DVec3, DVec3)> {
    let kind = &model.get(pick.id)?.kind;
    let (near_start, hit) = extend_end(kind, shape, at)?.ok()?;
    let (a, b) = match kind {
        ElementKind::Wall(w) => (w.start, w.end),
        ElementKind::Beam(b) => (b.start, b.end),
        ElementKind::Grid(g) => (g.start, g.end),
        _ => return None,
    };
    let from = if near_start { a } else { b };
    Some((from.extend(at.z), hit.extend(at.z)))
}

/// The end of a wall's, beam's or grid line's run `kind` nearer `at` (true
/// for its start) and the plan point where its line meets the reference.
fn extend_end(
    kind: &ElementKind,
    shape: &Shape,
    at: DVec3,
) -> Option<Result<(bool, DVec2), String>> {
    let (a, b) = match kind {
        ElementKind::Wall(w) => (w.start, w.end),
        ElementKind::Beam(b) => (b.start, b.end),
        ElementKind::Grid(g) => (g.start, g.end),
        _ => return None,
    };
    let meet = |r0: DVec2, r1: DVec2| creabim_geom::annotation::line_intersection(a, b, r0, r1);
    let hit = match *shape {
        Shape::Plane { point, normal } => {
            let nxy = normal.truncate();
            if nxy.length() < 0.1 {
                return None;
            }
            // where the plane crosses the height clicked at
            let c = point.truncate() + nxy * ((point.z - at.z) * normal.z / nxy.length_squared());
            meet(c, c + nxy.perp())
        }
        Shape::Line { a: p, b: q } if (q - p).truncate().length() > 1e-6 => {
            meet(p.truncate(), q.truncate())
        }
        Shape::Line { a: p, .. } | Shape::Point(p) => {
            let d = (b - a).normalize_or_zero();
            (d != DVec2::ZERO).then(|| a + d * (p.truncate() - a).dot(d))
        }
    };
    let Some(hit) = hit else {
        return Some(Err(
            "It runs parallel to the reference, so it never meets it.".into(),
        ));
    };
    Some(Ok((
        at.truncate().distance(a) <= at.truncate().distance(b),
        hit,
    )))
}

/// The second click's edit: `pick`'s face (or element) to `reference` in
/// `mode`, as one undo step and the message for the status bar.
pub fn apply(
    model: &Model,
    mode: AlignMode,
    reference: &Reference,
    pick: &FacePick,
    centre: bool,
    at: DVec3,
) -> Result<(Transaction, String), String> {
    let same_plane = match reference.shape {
        Shape::Plane { point, normal } => {
            normal.dot(pick.face.normal).abs() > 0.9999
                && (pick.face.point - point).dot(normal).abs() < 1e-6
        }
        _ => false,
    };
    if reference.id == Some(pick.id) && same_plane {
        return Err("That is the reference itself: click a face of something else.".into());
    }
    if centre && mode == AlignMode::Stretch {
        return Err("Stretch moves a face: click a face, or use Align to bring a centre line onto the reference.".into());
    }
    let stretch = || {
        let s = reach(&reference.shape, &pick.face)?;
        stretch_tx(model, pick.id, &pick.face, s)
    };
    match mode {
        AlignMode::Stretch => stretch(),
        AlignMode::Align => align_tx(model, &reference.shape, pick, centre),
        // what has no ends to run stretches the face clicked instead
        AlignMode::Extend => extend_tx(model, &reference.shape, pick, at).unwrap_or_else(stretch),
    }
}

/// A click with the Align tool in a 3D pane: the reference first, then
/// what to align to it. Returns the status line.
pub fn click(
    doc: &mut Document,
    state: &mut AlignState,
    under: &Under,
    selection: &mut Selection,
) -> String {
    let Some(reference) = state.reference_3d.clone() else {
        return match reference_at(doc.model(), under) {
            Some(r) => {
                state.reference_3d = Some(r);
                hint(state)
            }
            None => "Click a face, edge or corner, or a grid line.".into(),
        };
    };
    let Some((pick, centre)) = under.target() else {
        return "Click a face of what to align to the reference.".into();
    };
    let at = under
        .face
        .as_ref()
        .map_or(pick.face.point, |f| f.face.point);
    match apply(doc.model(), state.mode, &reference, &pick, centre, at) {
        Ok((tx, msg)) => {
            let target = match doc.model().get(pick.id).map(|e| &e.kind) {
                Some(ElementKind::Opening(o)) if state.mode == AlignMode::Align && !centre => {
                    o.host
                }
                _ => pick.id,
            };
            match doc.commit(tx) {
                Ok(()) => {
                    selection.only(target);
                    msg
                }
                Err(e) => format!("Couldn't do that: {e}"),
            }
        }
        Err(e) => e,
    }
}

/// Draw the reference, what the pointer is over and, once a reference is
/// picked, where the face under it would go.
pub fn draw(
    painter: &egui::Painter,
    project: &dyn Fn(DVec3) -> Option<Pos2>,
    model: &Model,
    state: &AlignState,
    under: &Under,
) {
    let accent = Color32::from_rgb(230, 120, 30);
    let fill = |tris: &[[DVec3; 3]], c: Color32| {
        for t in tris {
            let pts: Option<Vec<Pos2>> = t.iter().map(|p| project(*p)).collect();
            if let Some(pts) = pts {
                painter.add(egui::Shape::convex_polygon(pts, c, Stroke::NONE));
            }
        }
    };
    let guide = Stroke::new(1.5, snaps::GUIDE);
    let line = |a: DVec3, b: DVec3, s: Stroke| {
        if let (Some(a), Some(b)) = (project(a), project(b)) {
            painter.add(egui::Shape::dashed_line(&[a, b], s, 8.0, 4.0));
        }
    };
    // a square of the plane about `c`, `r` each way
    let patch = |c: DVec3, n: DVec3, r: f64| {
        let u = if n.z.abs() > 0.9 {
            DVec3::X
        } else {
            DVec3::Z.cross(n).normalize_or(DVec3::X)
        };
        let v = n.cross(u).normalize_or(DVec3::Y);
        let pts: Option<Vec<Pos2>> = [
            (-1.0, -1.0),
            (1.0, -1.0),
            (1.0, 1.0),
            (-1.0, 1.0),
            (-1.0, -1.0),
        ]
        .iter()
        .map(|(x, y)| project(c + u * (x * r) + v * (y * r)))
        .collect();
        if let Some(pts) = pts {
            painter.add(egui::Shape::dashed_line(&pts, guide, 8.0, 4.0));
        }
    };
    // the centre lines on the face under the pointer, the one it is on
    // drawn stronger
    for (i, (_, a, b)) in under.centres.iter().enumerate() {
        if let (Some(a), Some(b)) = (project(*a), project(*b)) {
            let on = under.centre == Some(i);
            let w = if on { 2.5 } else { 1.0 };
            painter.add(egui::Shape::dashed_line(
                &[a, b],
                Stroke::new(w, CENTRE.gamma_multiply(if on { 1.0 } else { 0.6 })),
                10.0,
                3.0,
            ));
        }
    }
    match &state.reference_3d {
        Some(r) => {
            fill(&r.tris, snaps::GUIDE.gamma_multiply(0.35));
            match r.shape {
                Shape::Plane { point, normal } => {
                    // about where the pointer is, when it is over the plane
                    let near = under
                        .face
                        .as_ref()
                        .map(|f| f.face.point - normal * (f.face.point - point).dot(normal))
                        .unwrap_or(point);
                    patch(near, normal, 1.5);
                }
                Shape::Line { a, b } => {
                    let d = (b - a).normalize_or_zero();
                    line(a - d * 3.0, b + d * 3.0, Stroke::new(2.0, snaps::GUIDE));
                }
                Shape::Point(p) => {
                    if let Some(c) = project(p) {
                        painter.circle_stroke(c, 6.0, Stroke::new(2.0, snaps::GUIDE));
                    }
                }
            }
            // the face (or centre line) the next click takes, and where it
            // would go
            if let (Some(f), Some((t, centre))) = (&under.face, under.target()) {
                if !centre {
                    fill(&f.tris, accent.gamma_multiply(0.45));
                }
                let face = match state.mode {
                    AlignMode::Extend => None,
                    AlignMode::Stretch if centre => None,
                    _ => reach(&r.shape, &t.face).ok(),
                };
                // from the point clicked, or the nearest of the centre line
                let from = match under.centre.and_then(|i| under.centres.get(i)) {
                    Some((_, a, b)) => {
                        let d = (*b - *a).normalize_or_zero();
                        *a + d * (f.face.point - *a).dot(d).clamp(0.0, a.distance(*b))
                    }
                    None => f.face.point,
                };
                // Extend: the nearer end of a wall or beam, and where it runs
                let run = (state.mode == AlignMode::Extend && !centre)
                    .then(|| extend_preview(model, &r.shape, &t, f.face.point))
                    .flatten()
                    .filter(|(a, b)| a.distance(*b) > 1e-6);
                if let Some((a, b)) = run {
                    line(a, b, Stroke::new(1.5, accent));
                    if let Some(c) = project(b) {
                        painter.circle_filled(c, 3.5, accent);
                        painter.text(
                            c + egui::vec2(8.0, -8.0),
                            egui::Align2::LEFT_BOTTOM,
                            crate::units::len(a.distance(b)),
                            egui::FontId::proportional(12.0),
                            accent,
                        );
                    }
                }
                if let Some(s) = face.filter(|s| s.abs() > 1e-6) {
                    let to = from + t.face.normal * s;
                    line(from, to, Stroke::new(1.5, accent));
                    if let Some(c) = project(to) {
                        painter.circle_filled(c, 3.5, accent);
                        painter.text(
                            c + egui::vec2(8.0, -8.0),
                            egui::Align2::LEFT_BOTTOM,
                            crate::units::len(s.abs()),
                            egui::FontId::proportional(12.0),
                            accent,
                        );
                    }
                }
            }
        }
        None => {
            // what a click takes as the reference
            match under.snap {
                Some(s) if s.kind.is_point() && s.kind != SnapKind::Center => {
                    if let Some(c) = project(s.p) {
                        snaps::draw(painter, s, c, None);
                    }
                }
                Some(s)
                    if under.centre.is_none()
                        && under.edge.is_some()
                        && matches!(
                            s.kind,
                            SnapKind::OnEdge | SnapKind::Guide | SnapKind::OnGridLine
                        ) =>
                {
                    let (a, b) = under.edge.expect("checked");
                    if let (Some(a), Some(b)) = (project(a), project(b)) {
                        painter.line_segment([a, b], Stroke::new(3.0, snaps::GUIDE));
                    }
                }
                _ if under.centre.is_some() => {}
                _ => {
                    if let Some(f) = &under.face {
                        fill(&f.tris, snaps::GUIDE.gamma_multiply(0.3));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;
    use creabim_core::Element;

    fn level(doc: &Document) -> (ElementId, f64) {
        let (e, l) = doc.model().levels().next().unwrap();
        (e.id, l.elevation)
    }

    fn first<T: Clone>(doc: &Document, f: impl Fn(&ElementKind) -> Option<T>) -> (ElementId, T) {
        doc.model()
            .iter()
            .find_map(|e| Some((e.id, f(&e.kind)?)))
            .unwrap()
    }

    /// The face of `id` a ray from `o` along `d` hits, as the 3D window
    /// picks it.
    fn face(doc: &Document, id: ElementId, o: DVec3, d: DVec3) -> FacePick {
        let scene = creabim_geom::build_scene(doc.model());
        face_pick(doc.model(), &scene, o, d, Some(id)).expect("a face there")
    }

    fn plane(point: DVec3, normal: DVec3) -> Reference {
        Reference {
            shape: Shape::Plane { point, normal },
            id: None,
            what: "test".into(),
            tris: vec![],
        }
    }

    fn run(doc: &mut Document, mode: AlignMode, r: Reference, pick: FacePick) -> String {
        let mut state = AlignState {
            mode,
            reference_3d: Some(r),
            ..Default::default()
        };
        let mut sel = Selection::default();
        let under = Under {
            face: Some(pick),
            ..Default::default()
        };
        let msg = click(doc, &mut state, &under, &mut sel);
        assert!(state.reference_3d.is_some(), "the reference stays");
        msg
    }

    /// A lone wall along x from (20, 0) to (26, 0) on the first level.
    fn lone_wall(doc: &mut Document) -> ElementId {
        let (l, _) = level(doc);
        let (_, mut w) = first(doc, |k| match k {
            ElementKind::Wall(w) => Some(w.clone()),
            _ => None,
        });
        (w.level, w.start, w.end) = (l, DVec2::new(20.0, 0.0), DVec2::new(26.0, 0.0));
        let e = Element::new("Lone wall", ElementKind::Wall(w));
        let id = e.id;
        doc.commit(Transaction::new("Wall").insert(e)).unwrap();
        id
    }

    /// The box `id`'s 3D shape fills.
    fn bounds(doc: &Document, id: ElementId) -> (DVec3, DVec3) {
        creabim_geom::build_scene(doc.model())
            .iter()
            .filter(|s| s.id == id)
            .filter_map(|s| s.mesh.bounds())
            .map(|(a, b)| (a.as_dvec3(), b.as_dvec3()))
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
            .unwrap()
    }

    fn wall(doc: &Document, id: ElementId) -> creabim_core::Wall {
        match &doc.model().get(id).unwrap().kind {
            ElementKind::Wall(w) => w.clone(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn stretch_in_3d_raises_a_walls_top_to_a_face_and_keeps_its_bottom() {
        let mut doc = Document::from_model(sample_house());
        let id = lone_wall(&mut doc);
        let before = wall(&doc, id);
        let (lo, hi) = bounds(&doc, id);
        let top = face(&doc, id, DVec3::new(23.0, 0.0, 50.0), -DVec3::Z);
        assert!(top.face.normal.z > 0.9);
        // a face 0.7 above its top, its normal facing down at it
        let target = top.face.point.z + 0.7;
        let msg = run(
            &mut doc,
            AlignMode::Stretch,
            plane(DVec3::new(0.0, 0.0, target), -DVec3::Z),
            top,
        );
        assert!(msg.starts_with("Stretched"), "{msg}");
        let (lo2, hi2) = bounds(&doc, id);
        assert!((hi2.z - hi.z - 0.7).abs() < 1e-4, "{msg}");
        assert!((lo2.z - lo.z).abs() < 1e-6);
        assert_eq!(doc.undo_label(), Some("Stretch to reference"));
        doc.undo();
        assert_eq!(wall(&doc, id), before);
    }

    #[test]
    fn stretch_in_3d_takes_a_near_parallel_face_and_refuses_a_skew_one() {
        let mut doc = Document::from_model(sample_house());
        let id = lone_wall(&mut doc);
        let before = wall(&doc, id);
        let end = face(&doc, id, DVec3::new(40.0, 0.0, 1.0), -DVec3::X);
        assert!(end.face.normal.x > 0.9);
        // 3° off upright, through x = 28 at the height picked
        let n = DVec3::new(3f64.to_radians().cos(), 0.0, 3f64.to_radians().sin());
        let at = DVec3::new(28.0, 0.0, end.face.point.z);
        let msg = run(&mut doc, AlignMode::Stretch, plane(at, n), end.clone());
        assert!(msg.starts_with("Stretched"), "{msg}");
        let after = wall(&doc, id);
        assert!((after.end.x - 28.0).abs() < 1e-6, "{}", after.end);
        assert_eq!(after.start, before.start);
        // 20° off is too far
        doc.undo();
        let n = DVec3::new(20f64.to_radians().cos(), 20f64.to_radians().sin(), 0.0);
        let msg = run(&mut doc, AlignMode::Stretch, plane(at, n), end);
        assert!(msg.contains("20°"), "{msg}");
        assert_eq!(wall(&doc, id), before);
    }

    #[test]
    fn extend_in_3d_runs_a_walls_nearer_end_to_a_slanted_face() {
        let mut doc = Document::from_model(sample_house());
        let id = lone_wall(&mut doc);
        let side = face(&doc, id, DVec3::new(25.5, -20.0, 1.0), DVec3::Y);
        // an upright plane through (30, 0) at 45°
        let n = DVec3::new(1.0, 1.0, 0.0).normalize();
        // before the click, the preview runs the east end to (30, 0)
        let (from, to) = extend_preview(
            doc.model(),
            &Shape::Plane {
                point: DVec3::new(30.0, 0.0, 0.0),
                normal: n,
            },
            &side,
            side.face.point,
        )
        .expect("a wall's end runs");
        assert_eq!(from.truncate(), wall(&doc, id).end);
        assert!(
            (to.truncate() - DVec2::new(30.0, 0.0)).length() < 1e-6,
            "{to}"
        );
        let msg = run(
            &mut doc,
            AlignMode::Extend,
            plane(DVec3::new(30.0, 0.0, 0.0), n),
            side,
        );
        assert!(msg.starts_with("Extended"), "{msg}");
        let w = wall(&doc, id);
        assert!((w.end - DVec2::new(30.0, 0.0)).length() < 1e-6, "{}", w.end);
        assert_eq!(w.start, DVec2::new(20.0, 0.0));
        assert_eq!(doc.undo_label(), Some("Extend to reference"));
    }

    #[test]
    fn align_in_3d_turns_a_wall_parallel_to_a_grid_and_moves_it_flush() {
        let mut doc = Document::from_model(sample_house());
        let id = lone_wall(&mut doc);
        let before = wall(&doc, id);
        let side = face(&doc, id, DVec3::new(23.0, -20.0, 1.0), DVec3::Y);
        assert!(side.face.normal.y < -0.9);
        // an upright plane at 30° through (30, 5), as a grid line makes
        let along = DVec2::from_angle(30f64.to_radians());
        let r0 = DVec3::new(30.0, 5.0, 0.0);
        let msg = run(
            &mut doc,
            AlignMode::Align,
            plane(r0, along.perp().extend(0.0)),
            side,
        );
        assert!(msg.starts_with("Aligned"), "{msg}");
        let w = wall(&doc, id);
        // parallel, its south face on the reference, its length kept
        assert!((w.end - w.start).normalize().perp_dot(along).abs() < 1e-9);
        assert!((w.length() - before.length()).abs() < 1e-9);
        let half = w.thickness / 2.0;
        for p in [w.start, w.end] {
            let off = (p - r0.truncate()).perp_dot(along).abs();
            assert!((off - half).abs() < 1e-6, "{off}");
        }
    }

    #[test]
    fn stretch_in_3d_drops_a_slabs_underside_to_a_level_and_keeps_its_top() {
        let mut doc = Document::from_model(sample_house());
        let (id, s) = first(&doc, |k| match k {
            ElementKind::Slab(s) => Some(s.clone()),
            _ => None,
        });
        let z0 = doc.model().level(s.level).unwrap().elevation;
        let c = s.outline().iter().copied().sum::<DVec2>() / s.outline().len() as f64;
        // from below, past whatever is under it: the slab's own underside
        let under = face(&doc, id, c.extend(-50.0), DVec3::Z);
        assert!(under.face.normal.z < -0.9);
        let low = s.top(z0) - s.thickness - 0.25;
        let r = reference_at(
            doc.model(),
            &Under {
                level: Some((s.level, low)),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(r.what.ends_with("plane"), "{}", r.what);
        let msg = run(&mut doc, AlignMode::Stretch, r, under);
        assert!(msg.starts_with("Stretched"), "{msg}");
        let ElementKind::Slab(after) = &doc.model().get(id).unwrap().kind else {
            unreachable!()
        };
        // (picked on the scene, which is single precision)
        assert!((after.thickness - s.thickness - 0.25).abs() < 1e-4);
        assert!((after.top(z0) - s.top(z0)).abs() < 1e-9);
    }

    #[test]
    fn every_kind_stretches_in_3d_as_its_face_pushes() {
        // a column's top, a beam's end, a mass's side: each to a point
        // 0.3 out from the face picked
        let mut doc = Document::from_model(sample_house());
        let (l, _) = level(&doc);
        let col = Element::new(
            "Post",
            ElementKind::Column(creabim_core::members::Column::new(
                l,
                DVec2::new(40.0, 0.0),
                creabim_core::members::Profile::Round { diameter: 0.3 },
            )),
        );
        let beam = Element::new(
            "Beam",
            ElementKind::Beam(creabim_core::members::Beam::new(
                l,
                DVec2::new(40.0, 10.0),
                DVec2::new(46.0, 10.0),
            )),
        );
        let mass = Element::new(
            "Mass",
            ElementKind::FreeMesh(creabim_core::FreeMesh::new(
                Some(l),
                DVec3::new(40.0, 20.0, 0.0),
                creabim_core::EditMesh::cuboid(DVec3::new(2.0, 2.0, 3.0)),
            )),
        );
        let ids = [col.id, beam.id, mass.id];
        doc.commit(
            Transaction::new("Add")
                .insert(col)
                .insert(beam)
                .insert(mass),
        )
        .unwrap();
        let rays = [
            (DVec3::new(40.0, 0.0, 50.0), -DVec3::Z),
            (DVec3::new(60.0, 10.0, 0.0), -DVec3::X),
            (DVec3::new(60.0, 21.0, 1.0), -DVec3::X),
        ];
        for (id, (o, d)) in ids.into_iter().zip(rays) {
            let before = doc.model().get(id).unwrap().clone();
            let rays_at = match &before.kind {
                // the beam sits under the storey's top: find its height
                ElementKind::Beam(_) => {
                    let (lo, hi) = bounds(&doc, id);
                    DVec3::new(o.x, o.y, (lo.z + hi.z) / 2.0)
                }
                _ => o,
            };
            let f = face(&doc, id, rays_at, d);
            let to = f.face.point + f.face.normal * 0.3;
            let r = Reference {
                shape: Shape::Point(to),
                id: None,
                what: "a point".into(),
                tris: vec![],
            };
            let msg = run(&mut doc, AlignMode::Stretch, r, f.clone());
            assert!(msg.starts_with("Stretched"), "{}: {msg}", before.name);
            // the face moved: a fresh pick along the same ray lands 0.3 nearer
            let again = face(&doc, id, rays_at, d);
            let moved = (again.face.point - f.face.point).dot(f.face.normal);
            assert!((moved - 0.3).abs() < 1e-4, "{}: moved {moved}", before.name);
        }
    }

    #[test]
    fn a_reference_is_a_corner_an_edge_a_grid_a_face_or_a_level() {
        let doc = Document::from_model(sample_house());
        let model = doc.model();
        let (gid, g) = first(&doc, |k| match k {
            ElementKind::Grid(g) => Some(g.clone()),
            _ => None,
        });
        let snap = |kind, host| Snap {
            p: g.start.extend(0.0),
            kind,
            host,
        };
        let at = |under: Under| reference_at(model, &under).unwrap();
        let r = at(Under {
            snap: Some(snap(SnapKind::Endpoint, None)),
            ..Default::default()
        });
        assert!(matches!(r.shape, Shape::Point(_)));
        let r = at(Under {
            snap: Some(snap(SnapKind::OnGridLine, Some(gid))),
            ..Default::default()
        });
        let Shape::Plane { normal, .. } = r.shape else {
            panic!("{r:?}")
        };
        assert!(normal.z.abs() < 1e-9);
        assert!(normal.truncate().dot(g.end - g.start).abs() < 1e-9);
        let r = at(Under {
            snap: Some(snap(SnapKind::OnEdge, None)),
            edge: Some((DVec3::ZERO, DVec3::X)),
            ..Default::default()
        });
        assert!(matches!(r.shape, Shape::Line { .. }));
        let (l, z) = level(&doc);
        let r = at(Under {
            level: Some((l, z)),
            ..Default::default()
        });
        assert_eq!(
            r.shape,
            Shape::Plane {
                point: DVec3::new(0.0, 0.0, z),
                normal: DVec3::Z
            }
        );
        assert!(reference_at(model, &Under::default()).is_none());
    }
    /// A post at (40, 5) and a beam along y at x = 41, on the first level.
    fn post_and_beam(doc: &mut Document) -> (ElementId, ElementId) {
        let (l, _) = level(doc);
        let col = Element::new(
            "Post",
            ElementKind::Column(creabim_core::members::Column::new(
                l,
                DVec2::new(40.0, 5.0),
                creabim_core::members::Profile::Rectangle {
                    width: 0.3,
                    depth: 0.3,
                },
            )),
        );
        let beam = Element::new(
            "Beam",
            ElementKind::Beam(creabim_core::members::Beam::new(
                l,
                DVec2::new(41.0, 0.0),
                DVec2::new(41.0, 8.0),
            )),
        );
        let ids = (col.id, beam.id);
        doc.commit(Transaction::new("Add").insert(col).insert(beam))
            .unwrap();
        ids
    }

    #[test]
    fn align_in_3d_brings_a_beams_centre_line_onto_a_posts() {
        let mut doc = Document::from_model(sample_house());
        let (post, beam) = post_and_beam(&mut doc);
        // looking straight down, 100 pixels a metre
        let project = |p: DVec3| Some(Pos2::new(p.x as f32 * 100.0, p.y as f32 * 100.0));
        let under_at = |doc: &Document, id: ElementId, x: f64, y: f64| {
            let scene = creabim_geom::build_scene(doc.model());
            let f = face(doc, id, DVec3::new(x, y, 50.0), -DVec3::Z);
            let pointer = Pos2::new(x as f32 * 100.0, y as f32 * 100.0);
            let (centres, centre) = centres_under(doc.model(), &scene, Some(&f), pointer, &project);
            Under {
                face: Some(f),
                centres,
                centre,
                ..Default::default()
            }
        };
        // on the post's top, over its centre line along y
        let on_post = under_at(&doc, post, 40.0, 5.12);
        assert_eq!(on_post.centres.len(), 2, "both its centre lines");
        let r = reference_at(doc.model(), &on_post).unwrap();
        assert!(r.what.starts_with("the centre line of"), "{}", r.what);
        let Shape::Plane { normal, .. } = r.shape else {
            panic!()
        };
        assert!(normal.x.abs() > 0.99, "{normal}");
        let mut state = AlignState {
            mode: AlignMode::Align,
            reference_3d: Some(r),
            ..Default::default()
        };
        let mut sel = Selection::default();
        // the beam by its centre line
        let on_beam = under_at(&doc, beam, 41.0, 3.0);
        assert!(on_beam.centre.is_some());
        let msg = click(&mut doc, &mut state, &on_beam, &mut sel);
        assert!(msg.starts_with("Aligned"), "{msg}");
        let ElementKind::Beam(b) = &doc.model().get(beam).unwrap().kind else {
            unreachable!()
        };
        assert!((b.start.x - 40.0).abs() < 1e-4 && (b.end.x - 40.0).abs() < 1e-4);
        assert!((b.start.y).abs() < 1e-9 && (b.end.y - 8.0).abs() < 1e-9);
        // Stretch moves faces, not centre lines
        state.mode = AlignMode::Stretch;
        let on_beam = under_at(&doc, beam, 40.0, 3.0);
        let msg = click(&mut doc, &mut state, &on_beam, &mut sel);
        assert!(msg.starts_with("Stretch moves a face"), "{msg}");
    }

    #[test]
    fn a_window_by_its_centre_line_slides_along_its_wall() {
        let mut doc = Document::from_model(sample_house());
        let (id, o) = first(&doc, |k| match k {
            ElementKind::Opening(o) => Some(o.clone()),
            _ => None,
        });
        let ElementKind::Wall(host) = doc.model().get(o.host).unwrap().kind.clone() else {
            panic!("hosted by a wall")
        };
        let scene = creabim_geom::build_scene(doc.model());
        let across = crate::centres::of(doc.model(), &scene, id)
            .into_iter()
            .find(|c| c.normal.z.abs() < 0.1)
            .expect("a centre line across its wall");
        // a reference 300 mm along the wall from that centre line
        let along = (host.end - host.start).normalize().extend(0.0);
        let n = across.normal * across.normal.dot(along).signum();
        let r = plane(across.point + n * 0.3, n);
        let pick = FacePick {
            id,
            face: FaceRef {
                point: across.point,
                normal: across.normal,
                mesh_face: None,
            },
            tris: vec![],
        };
        let msg = apply(doc.model(), AlignMode::Align, &r, &pick, true, across.point)
            .map(|(tx, m)| {
                doc.commit(tx).expect("commits");
                m
            })
            .unwrap_or_else(|e| e);
        assert!(msg.starts_with("Aligned"), "{msg}");
        let ElementKind::Opening(now) = &doc.model().get(id).unwrap().kind else {
            unreachable!()
        };
        assert!(
            (now.offset - o.offset - 0.3).abs() < 1e-6,
            "{} → {}",
            o.offset,
            now.offset
        );
        // the wall stays put
        assert_eq!(
            doc.model().get(o.host).unwrap().kind,
            ElementKind::Wall(host)
        );
    }
}
