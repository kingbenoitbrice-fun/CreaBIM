//! Edit mode: an element opened for editing (Properties' Edit button, a
//! double-click on it, or Enter) shows every grip it has, in every view
//! it is in. Outlines get a grip on each corner, a push grip on each edge
//! and an add-corner grip in each edge's middle: a slab's outline and its
//! holes, a shaft's, a roof's and its holes, a sketch's corners. Shafts
//! get their base and top too. Double-clicking a corner takes it away.
//! Escape, or selecting something else, leaves Edit mode.

use crate::handles::{Constraint, Handle, HandleKind, Ring};
use creabim_core::{Document, Element, ElementId, ElementKind, Model};
use glam::{DVec2, DVec3};

/// How far an edge's push grip stands off the add-corner grip in the
/// edge's middle, in screen pixels.
pub const EDGE_NUDGE: f32 = 16.0;

/// The editable outlines of `e`, each as its corners in the model, whether
/// it closes and the plane its corners move on (a point and a normal).
pub struct RingInfo {
    pub ring: Ring,
    pub corners: Vec<DVec3>,
    pub closed: bool,
    pub normal: DVec3,
    /// Corners can be added and taken away (not on a sketch with arcs or
    /// constraints, whose corners those refer to).
    pub reshapes: bool,
}

fn elevation(model: &Model, level: Option<ElementId>) -> f64 {
    level
        .and_then(|l| model.level(l))
        .map_or(0.0, |l| l.elevation)
}

/// Every outline of `e` Edit mode works on.
pub fn rings(model: &Model, e: &Element) -> Vec<RingInfo> {
    let flat = |ring, pts: &[DVec2], z: f64| RingInfo {
        ring,
        corners: pts.iter().map(|p| p.extend(z)).collect(),
        closed: true,
        normal: DVec3::Z,
        reshapes: true,
    };
    let holes = |pts: &[Vec<DVec2>], z: f64| -> Vec<RingInfo> {
        pts.iter()
            .enumerate()
            .map(|(k, h)| flat(Ring::Hole(k), h, z))
            .collect()
    };
    match &e.kind {
        ElementKind::Slab(s) => {
            let z = s.top(elevation(model, Some(s.level)));
            let mut out = vec![flat(Ring::Outline, &s.boundary, z)];
            out.extend(holes(&s.holes, z));
            out
        }
        ElementKind::Shaft(s) => {
            let (z0, _) = s.span(model);
            vec![flat(Ring::Outline, &s.boundary, z0)]
        }
        ElementKind::Roof(r) => {
            let z = r.base(model);
            let mut out = vec![flat(Ring::Outline, &r.boundary, z)];
            out.extend(holes(&r.holes, z));
            out
        }
        ElementKind::Sketch(sk) if sk.circle.is_none() => {
            let lift = DVec3::Z * elevation(model, sk.level);
            let straight = (0..sk.segment_count())
                .all(|i| sk.kind(i) == creabim_core::sketch::SegmentKind::Line);
            vec![RingInfo {
                ring: Ring::Outline,
                corners: sk
                    .points
                    .iter()
                    .map(|p| sk.plane.to_model(*p) + lift)
                    .collect(),
                closed: sk.closed,
                normal: sk.plane.normal(),
                reshapes: straight && sk.constraints.is_empty(),
            }]
        }
        _ => vec![],
    }
}

/// The number of edges of an outline of `n` corners.
fn edge_count(n: usize, closed: bool) -> usize {
    if closed {
        n
    } else {
        n.saturating_sub(1)
    }
}

/// The grips Edit mode adds to those the element always has (see
/// [`crate::handles::handles`]): corners, edges and add-corner grips of
/// its outlines, less those the element's own grips already cover.
pub fn extra_handles(model: &Model, id: ElementId) -> Vec<Handle> {
    let Some(e) = model.get(id) else {
        return vec![];
    };
    let mut out = Vec::new();
    for r in rings(model, e) {
        let n = r.corners.len();
        let own_outline = r.ring == Ring::Outline;
        // a slab's outline corners and edges are its own grips, a shaft's
        // corners too
        let corners =
            !(own_outline && matches!(e.kind, ElementKind::Slab(_) | ElementKind::Shaft(_)));
        let edges = !(own_outline && matches!(e.kind, ElementKind::Slab(_)));
        if corners {
            for (i, c) in r.corners.iter().enumerate() {
                out.push(Handle {
                    kind: HandleKind::Vertex { ring: r.ring, i },
                    at: *c,
                    constraint: Constraint::Plane {
                        point: *c,
                        normal: r.normal,
                    },
                    nudge: 0.0,
                });
            }
        }
        let ccw = signed_area(&r.corners, r.normal) > 0.0;
        for i in 0..edge_count(n, r.closed) {
            let (a, b) = (r.corners[i], r.corners[(i + 1) % n]);
            let mid = (a + b) / 2.0;
            if r.reshapes {
                out.push(Handle {
                    kind: HandleKind::AddVertex { ring: r.ring, i },
                    at: mid,
                    constraint: Constraint::Plane {
                        point: mid,
                        normal: r.normal,
                    },
                    nudge: 0.0,
                });
            }
            // sketches keep their edges where their corners put them
            if edges && r.closed && !matches!(e.kind, ElementKind::Sketch(_)) {
                let along = (b - a).normalize_or_zero();
                let left = r.normal.cross(along);
                let out_dir = if ccw { -left } else { left };
                if out_dir != DVec3::ZERO {
                    out.push(Handle {
                        kind: HandleKind::Edge { ring: r.ring, i },
                        at: mid,
                        constraint: Constraint::Line {
                            point: mid,
                            dir: out_dir,
                        },
                        nudge: EDGE_NUDGE,
                    });
                }
            }
        }
    }
    out
}

/// Twice the area of `pts` seen along `normal`, positive counter-clockwise.
fn signed_area(pts: &[DVec3], normal: DVec3) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| pts[i].cross(pts[(i + 1) % n]).dot(normal))
        .sum()
}

/// The corners of outline `ring` of an element, in its own coordinates
/// (plan points, or a sketch's plane).
pub fn ring_mut(kind: &mut ElementKind, ring: Ring) -> Option<&mut Vec<DVec2>> {
    match (kind, ring) {
        (ElementKind::Slab(s), Ring::Outline) => Some(&mut s.boundary),
        (ElementKind::Slab(s), Ring::Hole(k)) => s.holes.get_mut(k),
        (ElementKind::Shaft(s), Ring::Outline) => Some(&mut s.boundary),
        (ElementKind::Roof(r), Ring::Outline) => Some(&mut r.boundary),
        (ElementKind::Roof(r), Ring::Hole(k)) => r.holes.get_mut(k),
        (ElementKind::Sketch(sk), Ring::Outline) => Some(&mut sk.points),
        _ => None,
    }
}

/// A model point as a corner of `e`'s outlines: in plan, or in a sketch's plane.
fn local(model: &Model, e: &Element, p: DVec3) -> DVec2 {
    match &e.kind {
        ElementKind::Sketch(sk) => sk.plane.to_plane(p - DVec3::Z * elevation(model, sk.level)),
        _ => p.truncate(),
    }
}

fn round(v: f64, step: f64) -> f64 {
    if step > 0.0 {
        (v / step).round() * step
    } else {
        v
    }
}

/// Insert a corner after corner `i` of `ring`, at `p`: on a slab's
/// outline its arcs split, a roof keeps its sloping edge, a sketch its
/// segment kinds.
fn insert(e: &mut Element, ring: Ring, i: usize, p: DVec2) {
    match (&mut e.kind, ring) {
        (ElementKind::Slab(s), Ring::Outline) => {
            s.split_edge(i);
            if let Some(c) = s.boundary.get_mut(i + 1) {
                *c = p;
            }
        }
        (ElementKind::Roof(r), Ring::Outline) => {
            r.boundary.insert(i + 1, p);
            if r.edge > i {
                r.edge += 1;
            }
        }
        (ElementKind::Sketch(sk), Ring::Outline) => {
            sk.points.insert(i + 1, p);
            if sk.kinds.len() > i {
                sk.kinds.insert(i + 1, Default::default());
            }
        }
        (kind, ring) => {
            if let Some(pts) = ring_mut(kind, ring) {
                pts.insert(i + 1, p);
            }
        }
    }
}

/// Take corner `i` of `ring` away.
fn remove(e: &mut Element, ring: Ring, i: usize) {
    match (&mut e.kind, ring) {
        (ElementKind::Slab(s), Ring::Outline) => s.remove_corner(i),
        (ElementKind::Roof(r), Ring::Outline) => {
            r.boundary.remove(i);
            if r.edge >= i && r.edge > 0 {
                r.edge -= 1;
            }
            r.edge = r.edge.min(r.boundary.len().saturating_sub(1));
        }
        (ElementKind::Sketch(sk), Ring::Outline) => {
            sk.points.remove(i);
            if i < sk.kinds.len() {
                sk.kinds.remove(i);
            }
            let count = sk.segment_count();
            sk.kinds.truncate(count);
        }
        (kind, ring) => {
            if let Some(pts) = ring_mut(kind, ring) {
                pts.remove(i);
            }
        }
    }
}

/// Grip `kind` of `e` dragged from `grab` to `now`: a corner moved, an
/// edge pushed square to itself (rounded to `step`), a corner pulled out
/// of an edge, a shaft's base or top raised or lowered.
pub fn reshape(
    model: &Model,
    e: &mut Element,
    kind: HandleKind,
    grab: DVec3,
    now: DVec3,
    step: f64,
) -> Result<(), String> {
    let d = now - grab;
    match kind {
        HandleKind::Vertex { ring, i } => {
            let p = local(model, e, now);
            let pts = ring_mut(&mut e.kind, ring).ok_or("that outline is gone")?;
            *pts.get_mut(i).ok_or("that corner is gone")? = p;
        }
        HandleKind::AddVertex { ring, i } => {
            let p = local(model, e, now);
            insert(e, ring, i, p);
        }
        HandleKind::Edge { ring, i } => {
            let info = rings(model, e)
                .into_iter()
                .find(|r| r.ring == ring)
                .ok_or("that outline is gone")?;
            let n = info.corners.len();
            let ccw = signed_area(&info.corners, info.normal) > 0.0;
            let (a, b) = (info.corners[i % n], info.corners[(i + 1) % n]);
            let left = info.normal.cross((b - a).normalize_or_zero());
            let out_dir = if ccw { -left } else { left };
            let by = out_dir * round(d.dot(out_dir), step);
            let (pa, pb) = (local(model, e, a + by), local(model, e, b + by));
            let pts = ring_mut(&mut e.kind, ring).ok_or("that outline is gone")?;
            pts[i % n] = pa;
            pts[(i + 1) % n] = pb;
        }
        HandleKind::ShaftBase | HandleKind::ShaftTop => {
            let ElementKind::Shaft(s) = &mut e.kind else {
                return Err("only shafts have that grip".into());
            };
            let (z0, z1) = s.span(model);
            let dz = round(d.z, step);
            if kind == HandleKind::ShaftBase {
                if z0 + dz > z1 - 0.1 {
                    return Err("its base would be above its top".into());
                }
                s.base_offset += dz;
            } else {
                if z1 + dz < z0 + 0.1 {
                    return Err("its top would be below its base".into());
                }
                s.top_offset += dz;
            }
        }
        _ => return Err("this handle doesn't fit this element".into()),
    }
    if let Some(r) = rings(model, e).into_iter().find(|r| {
        r.closed && r.corners.len() >= 3 && signed_area(&r.corners, r.normal).abs() < 1e-8
    }) {
        return Err(match r.ring {
            Ring::Outline => "the outline would have no area".into(),
            Ring::Hole(_) => "the hole would have no area".into(),
        });
    }
    Ok(())
}

/// A click on an add-corner grip, or a double-click on a corner: add a
/// corner in the middle of the edge, or take the corner away.
pub fn add_or_remove(
    doc: &mut Document,
    id: ElementId,
    kind: HandleKind,
) -> Result<String, String> {
    let e = doc.model().get(id).ok_or("it is gone")?.clone();
    let model = doc.model();
    let (ring, i, add) = match kind {
        HandleKind::AddVertex { ring, i } => (ring, i, true),
        HandleKind::Vertex { ring, i } => (ring, i, false),
        _ => return Err("That isn't a corner or an edge.".into()),
    };
    let info = rings(model, &e)
        .into_iter()
        .find(|r| r.ring == ring)
        .ok_or("That outline is gone.")?;
    if !info.reshapes {
        return Err(
            "This sketch has arcs or constraints, which keep its corners: drag them instead."
                .into(),
        );
    }
    let n = info.corners.len();
    let fewest = if info.closed { 3 } else { 2 };
    let what = match ring {
        Ring::Outline => "outline",
        Ring::Hole(_) => "hole",
    };
    let mut edited = e.clone();
    let (label, done) = if add {
        if i >= edge_count(n, info.closed) {
            return Err("That edge is gone.".into());
        }
        let mid = (info.corners[i] + info.corners[(i + 1) % n]) / 2.0;
        let p = local(model, &e, mid);
        // a slab's edge may be an arc: its middle is on the arc
        insert(&mut edited, ring, i, p);
        if let (ElementKind::Slab(s), Ring::Outline) = (&mut edited.kind, ring) {
            if let Some(c) = s.boundary.get_mut(i + 1) {
                *c = s_mid(&e, i).unwrap_or(p);
            }
        }
        ("Add corner", "Added a corner: drag it into place.")
    } else {
        if n <= fewest {
            return Err(format!("The {what} needs at least {fewest} corners."));
        }
        remove(&mut edited, ring, i);
        ("Remove corner", "Removed the corner.")
    };
    edited.validate()?;
    doc.edit(id, label, |el| *el = edited.clone())
        .map_err(|e| e.to_string())?;
    Ok(format!("{done} Ctrl+Z to undo."))
}

/// The middle of edge `i` of a slab's outline, on the arc if it is one.
fn s_mid(e: &Element, i: usize) -> Option<DVec2> {
    match &e.kind {
        ElementKind::Slab(s) if i < s.boundary.len() => Some(s.edge(i).at(0.5)),
        _ => None,
    }
}

/// What the status bar says while `e` is in Edit mode.
pub fn hint(model: &Model, e: &Element) -> String {
    let outlines = !rings(model, e).is_empty();
    let name = &e.name;
    if outlines {
        format!(
            "Editing {name}: drag a corner to move it, an edge's round grip to push it, a ◇ in an edge's middle to add a corner (or click it); double-click a corner to take it away. Esc or Enter finishes."
        )
    } else {
        format!("Editing {name}: drag its grips in any view. Esc or Enter finishes.")
    }
}

/// The short note Properties shows under the Edit button while editing.
pub fn panel_hint(kind: &ElementKind) -> &'static str {
    match kind {
        ElementKind::Slab(_) | ElementKind::Roof(_) => {
            "Drag corners and edges of the outline and of each hole; ◇ adds a corner, a double-click on a corner takes it away."
        }
        ElementKind::Shaft(_) => {
            "Drag its corners and edges in any plan, section or 3D view; ◇ adds a corner, a double-click on a corner takes it away. The grips in its middle move its base and top."
        }
        ElementKind::Sketch(_) => {
            "Drag its corners; ◇ adds one, a double-click on a corner takes it away."
        }
        _ => "Drag its grips in any view.",
    }
}

/// Open `id` in Edit mode, if it has grips; what the status bar says.
pub fn enter(model: &Model, id: ElementId, edit: &mut Option<ElementId>) -> String {
    match model.get(id) {
        Some(e) if editable(model, id) => {
            *edit = Some(id);
            hint(model, e)
        }
        Some(e) => format!("{} has no grips to edit: change it in Properties.", e.name),
        None => String::new(),
    }
}

/// Does `e` have grips to edit?
pub fn editable(model: &Model, id: ElementId) -> bool {
    !crate::handles::handles_in(model, id, None, true).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handles::{apply, handles_in, originals};
    use creabim_core::sample::sample_house;

    fn first(m: &Model, f: impl Fn(&ElementKind) -> bool) -> ElementId {
        m.iter().find(|e| f(&e.kind)).unwrap().id
    }

    fn drag(doc: &mut Document, id: ElementId, kind: HandleKind, to: DVec3) {
        let h = handles_in(doc.model(), id, None, true)
            .into_iter()
            .find(|h| h.kind == kind)
            .unwrap_or_else(|| panic!("no {kind:?} grip"));
        let (orig, joined) = originals(doc.model(), id, kind);
        let model = doc.model().clone();
        let out = apply(&model, &orig, &joined, kind, h.at, to, 0.0).unwrap();
        let new = out[0].clone();
        doc.edit(id, "drag", |e| *e = new.clone()).unwrap();
    }

    fn shaft(m: &Model, id: ElementId) -> creabim_core::Shaft {
        match &m.get(id).unwrap().kind {
            ElementKind::Shaft(s) => s.clone(),
            _ => panic!(),
        }
    }

    #[test]
    fn a_shaft_is_reshaped_and_resized_by_its_grips_in_edit_mode() {
        let m = creabim_core::sample::typed_house();
        let id = first(&m, |k| matches!(k, ElementKind::Shaft(_)));
        let mut doc = Document::from_model(m);
        let before = shaft(doc.model(), id);
        let n = before.boundary.len();
        // selected, a shaft has its corners and its base and top
        let plain = crate::handles::handles(doc.model(), id, None);
        assert!(plain.iter().any(|h| h.kind == HandleKind::ShaftTop));
        assert!(plain.iter().any(|h| matches!(
            h.kind,
            HandleKind::Vertex {
                ring: Ring::Outline,
                ..
            }
        )));
        assert!(!plain
            .iter()
            .any(|h| matches!(h.kind, HandleKind::AddVertex { .. })));
        // in Edit mode its edges and add-corner grips too
        let edit = handles_in(doc.model(), id, None, true);
        assert_eq!(
            edit.iter()
                .filter(|h| matches!(h.kind, HandleKind::AddVertex { .. }))
                .count(),
            n
        );
        // a corner moves
        let c = before.boundary[0];
        drag(
            &mut doc,
            id,
            HandleKind::Vertex {
                ring: Ring::Outline,
                i: 0,
            },
            (c + DVec2::new(-0.5, -0.25)).extend(0.0),
        );
        assert!(shaft(doc.model(), id).boundary[0].distance(c + DVec2::new(-0.5, -0.25)) < 1e-9);
        // an edge pushes out square to itself: the shaft gets bigger
        let area = |s: &creabim_core::Shaft| creabim_core::polygon::signed_area2(&s.boundary).abs();
        let a0 = area(&shaft(doc.model(), id));
        let h = handles_in(doc.model(), id, None, true)
            .into_iter()
            .find(|h| {
                h.kind
                    == HandleKind::Edge {
                        ring: Ring::Outline,
                        i: 1,
                    }
            })
            .unwrap();
        let Constraint::Line { dir, .. } = h.constraint else {
            panic!()
        };
        drag(
            &mut doc,
            id,
            HandleKind::Edge {
                ring: Ring::Outline,
                i: 1,
            },
            h.at + dir * 0.4,
        );
        assert!(area(&shaft(doc.model(), id)) > a0 + 0.1);
        // a corner pulled out of an edge, and taken away again
        let mid = handles_in(doc.model(), id, None, true)
            .into_iter()
            .find(|h| {
                h.kind
                    == HandleKind::AddVertex {
                        ring: Ring::Outline,
                        i: 2,
                    }
            })
            .unwrap()
            .at;
        drag(
            &mut doc,
            id,
            HandleKind::AddVertex {
                ring: Ring::Outline,
                i: 2,
            },
            mid + DVec3::new(0.3, 0.3, 0.0),
        );
        assert_eq!(shaft(doc.model(), id).boundary.len(), n + 1);
        add_or_remove(
            &mut doc,
            id,
            HandleKind::Vertex {
                ring: Ring::Outline,
                i: 3,
            },
        )
        .unwrap();
        assert_eq!(shaft(doc.model(), id).boundary.len(), n);
        // its top rises
        let (_, z1) = shaft(doc.model(), id).span(doc.model());
        let top = handles_in(doc.model(), id, None, true)
            .into_iter()
            .find(|h| h.kind == HandleKind::ShaftTop)
            .unwrap()
            .at;
        drag(&mut doc, id, HandleKind::ShaftTop, top + DVec3::Z * 0.5);
        let (_, z1b) = shaft(doc.model(), id).span(doc.model());
        assert!((z1b - z1 - 0.5).abs() < 1e-9);
        // the slabs it cuts follow
        assert!(!creabim_core::slab::cut_by_shaft(doc.model(), &shaft(doc.model(), id)).is_empty());
    }

    #[test]
    fn a_slab_hole_is_reshaped_in_edit_mode_and_keeps_three_corners() {
        let m = sample_house();
        let id = first(&m, |k| matches!(k, ElementKind::Slab(_)));
        let mut doc = Document::from_model(m);
        doc.edit(id, "hole", |e| {
            if let ElementKind::Slab(s) = &mut e.kind {
                let c = s.boundary.iter().copied().sum::<DVec2>() / s.boundary.len() as f64;
                s.holes = vec![vec![
                    c + DVec2::new(-0.5, -0.5),
                    c + DVec2::new(0.5, -0.5),
                    c + DVec2::new(0.5, 0.5),
                    c + DVec2::new(-0.5, 0.5),
                ]];
            }
        })
        .unwrap();
        let hole = |doc: &Document| match &doc.model().get(id).unwrap().kind {
            ElementKind::Slab(s) => s.holes[0].clone(),
            _ => panic!(),
        };
        // only in Edit mode
        assert!(!crate::handles::handles(doc.model(), id, None)
            .iter()
            .any(|h| matches!(
                h.kind,
                HandleKind::Vertex {
                    ring: Ring::Hole(0),
                    ..
                }
            )));
        let c0 = hole(&doc)[0];
        drag(
            &mut doc,
            id,
            HandleKind::Vertex {
                ring: Ring::Hole(0),
                i: 0,
            },
            (c0 - DVec2::splat(0.2)).extend(3.0),
        );
        assert!(hole(&doc)[0].distance(c0 - DVec2::splat(0.2)) < 1e-9);
        add_or_remove(
            &mut doc,
            id,
            HandleKind::AddVertex {
                ring: Ring::Hole(0),
                i: 0,
            },
        )
        .unwrap();
        assert_eq!(hole(&doc).len(), 5);
        for _ in 0..2 {
            add_or_remove(
                &mut doc,
                id,
                HandleKind::Vertex {
                    ring: Ring::Hole(0),
                    i: 0,
                },
            )
            .unwrap();
        }
        assert_eq!(hole(&doc).len(), 3);
        assert!(add_or_remove(
            &mut doc,
            id,
            HandleKind::Vertex {
                ring: Ring::Hole(0),
                i: 0,
            },
        )
        .is_err());
        // the slab's own outline gets add-corner grips, its edge grips
        // standing off them
        let edit = handles_in(doc.model(), id, None, true);
        assert!(edit.iter().any(|h| h.kind
            == HandleKind::AddVertex {
                ring: Ring::Outline,
                i: 0
            }));
        assert!(edit
            .iter()
            .filter(|h| matches!(h.kind, HandleKind::SlabEdge(_)))
            .all(|h| h.nudge > 0.0));
    }

    #[test]
    fn a_sketch_and_a_roof_take_corners_in_edit_mode() {
        let m = creabim_core::sample::layered_house();
        let level = m
            .levels()
            .min_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation))
            .unwrap()
            .0
            .id;
        let roof = first(&m, |k| matches!(k, ElementKind::Roof(_)));
        let mut doc = Document::from_model(m);
        let sk = creabim_core::Sketch::polyline(
            Some(level),
            creabim_core::Workplane::ground(0.0),
            vec![DVec2::ZERO, DVec2::new(2.0, 0.0), DVec2::new(2.0, 1.0)],
            false,
        );
        let e = Element::new("Sketch", ElementKind::Sketch(sk));
        let sid = e.id;
        doc.commit(creabim_core::Transaction::new("Sketch").insert(e))
            .unwrap();
        drag(
            &mut doc,
            sid,
            HandleKind::Vertex {
                ring: Ring::Outline,
                i: 2,
            },
            DVec3::new(3.0, 1.5, 0.0),
        );
        let pts = |doc: &Document| match &doc.model().get(sid).unwrap().kind {
            ElementKind::Sketch(s) => s.points.clone(),
            _ => panic!(),
        };
        assert!(pts(&doc)[2].distance(DVec2::new(3.0, 1.5)) < 1e-9);
        // an open sketch has an add-corner grip per segment, no edge grips
        let edit = handles_in(doc.model(), sid, None, true);
        assert_eq!(
            edit.iter()
                .filter(|h| matches!(h.kind, HandleKind::AddVertex { .. }))
                .count(),
            2
        );
        assert!(!edit
            .iter()
            .any(|h| matches!(h.kind, HandleKind::Edge { .. })));
        add_or_remove(
            &mut doc,
            sid,
            HandleKind::AddVertex {
                ring: Ring::Outline,
                i: 0,
            },
        )
        .unwrap();
        assert!(pts(&doc)[1].distance(DVec2::new(1.0, 0.0)) < 1e-9);
        // a roof's corner grips become outline corners in Edit mode
        let n = match &doc.model().get(roof).unwrap().kind {
            ElementKind::Roof(r) => r.boundary.len(),
            _ => panic!(),
        };
        let edit = handles_in(doc.model(), roof, None, true);
        assert_eq!(
            edit.iter()
                .filter(|h| matches!(
                    h.kind,
                    HandleKind::Vertex {
                        ring: Ring::Outline,
                        ..
                    }
                ))
                .count(),
            n
        );
        add_or_remove(
            &mut doc,
            roof,
            HandleKind::AddVertex {
                ring: Ring::Outline,
                i: 0,
            },
        )
        .unwrap();
        match &doc.model().get(roof).unwrap().kind {
            ElementKind::Roof(r) => assert_eq!(r.boundary.len(), n + 1),
            _ => panic!(),
        }
    }
}
