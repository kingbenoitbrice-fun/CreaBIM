//! The Align tool, as in Revit, with ArchiCAD's extend-to-a-reference:
//! click a reference line (a wall face, a slab edge, a guide, a grid line,
//! any edge of the drawing, or a temporary guide line), then the edge of
//! what to align. The element turns to lie parallel to the reference and
//! moves until its edge is flush with it. In Extend mode the end of a
//! wall, beam or grid line nearest the click stretches (or trims) to meet
//! the reference instead. In Stretch mode the face the picked edge stands
//! for moves to the reference and the rest of the element stays: a wall's
//! end or side, a slab's or roof's edge, a beam's end, a column's top or
//! bottom, a door's jamb (see `creabim_core::pushpull`). It works in plans,
//! sections and elevations. The reference stays picked, so several things
//! can be aligned to it; Esc lets it go. Every alignment is one undo step.

use crate::modal::Space;
use crate::paneface;
use crate::planview::{PaneKind, PlanState, Xf};
use crate::selection::Selection;
use crate::snaps;
use creabim_core::transform::transformed;
use creabim_core::{pushpull, Document, ElementId, ElementKind, ModelError, Transaction, Xform};
use creabim_geom::PlanItem;
use eframe::egui::{self, Color32, Stroke};
use glam::{DVec2, DVec3};

/// What the second click does to what it picks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlignMode {
    /// Turn it parallel to the reference and move it flush.
    #[default]
    Align,
    /// Stretch or trim its nearer end to the reference.
    Extend,
    /// Stretch the face its edge stands for to the reference; the rest of
    /// it stays.
    Stretch,
}

impl AlignMode {
    pub const ALL: [AlignMode; 3] = [AlignMode::Align, AlignMode::Stretch, AlignMode::Extend];

    pub fn label(self) -> &'static str {
        match self {
            AlignMode::Align => "Align",
            AlignMode::Extend => "Extend to it",
            AlignMode::Stretch => "Stretch to it",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            AlignMode::Align => "Turn the edge you pick parallel to the reference and move it flush",
            AlignMode::Extend => {
                "Stretch or trim the end of a wall, beam or grid line you pick to meet the reference"
            }
            AlignMode::Stretch => {
                "Stretch the edge or face you pick to the reference, the rest staying put: a wall end or side, a slab or roof edge, a beam end, a column top"
            }
        }
    }
}

/// The tool's picks.
#[derive(Clone, Debug, Default)]
pub struct AlignState {
    /// The reference line: the view it was picked in, two points on it
    /// (that view's drawing), and the element it belongs to.
    pub reference: Option<(ElementId, DVec2, DVec2, Option<ElementId>)>,
    pub mode: AlignMode,
}

impl AlignState {
    pub fn clear(&mut self) {
        self.reference = None;
    }

    pub fn has_input(&self) -> bool {
        self.reference.is_some()
    }
}

/// The tool's hint for the status bar.
pub fn hint(state: &AlignState) -> &'static str {
    match (state.reference.is_some(), state.mode) {
        (false, _) => "Align: click the reference line: a wall face, slab edge, guide, grid line or a temporary guide.",
        (true, AlignMode::Align) => "Now click the edge to align to it: the element turns parallel and moves flush. Esc lets go of the reference.",
        (true, AlignMode::Extend) => "Now click near the end of a wall, beam or grid line to stretch it to the reference. Esc lets go of the reference.",
        (true, AlignMode::Stretch) => "Now click the edge to stretch to it: a wall end or face, slab or roof edge, beam end, column top or bottom. The rest stays put. Esc lets go of the reference.",
    }
}

/// The straight edge of the drawing under `p`: its element and ends.
pub fn edge_at(items: &[PlanItem], p: DVec2, tol: f64) -> Option<(ElementId, DVec2, DVec2)> {
    crate::annotate::segment_at(items, p, tol)
}

/// The edge the next click takes: with a reference picked in Stretch
/// mode, one that stretches wins where edges lie on top of each other.
pub fn hover_edge(
    model: &creabim_core::Model,
    state: &AlignState,
    items: &[PlanItem],
    p: DVec2,
    tol: f64,
) -> Option<(ElementId, DVec2, DVec2)> {
    if state.reference.is_some() && state.mode == AlignMode::Stretch {
        let rank = |i: ElementId| {
            let push = model.get(i).is_some_and(|e| pushpull::can_push(&e.kind));
            Some(u8::from(push))
        };
        return paneface::edge_at(items, p, tol, &rank);
    }
    edge_at(items, p, tol)
}

/// The turn, in degrees, that makes direction `t` parallel to `r` by the
/// shorter way round (lines have no direction).
pub fn turn_to(t: DVec2, r: DVec2) -> f64 {
    let mut a = (r.to_angle() - t.to_angle()).to_degrees();
    a = a.rem_euclid(180.0);
    if a > 90.0 {
        a -= 180.0;
    }
    a
}

/// The moves that align the edge `t0`–`t1` with the line through `r0`
/// and `r1` in a plan: a turn about the point of the edge nearest the
/// click `at` (none when already parallel), then a move square to the
/// reference.
pub fn align_moves(t0: DVec2, t1: DVec2, r0: DVec2, r1: DVec2, at: DVec2) -> Vec<Xform> {
    let t = (t1 - t0).normalize_or_zero();
    let r = (r1 - r0).normalize_or_zero();
    let pivot = t0 + t * (at - t0).dot(t).clamp(0.0, t0.distance(t1));
    let mut out = vec![];
    let deg = turn_to(t, r);
    if deg.abs() > 1e-9 {
        out.push(Xform::Rotate {
            center: pivot,
            degrees: deg,
        });
    }
    let n = r.perp();
    let d = (r0 - pivot).dot(n);
    if d.abs() > 1e-9 {
        out.push(Xform::Move((n * d).extend(0.0)));
    }
    out
}

/// Where the line through `a` and `b` meets the line through `r0` and `r1`.
fn meet(a: DVec2, b: DVec2, r0: DVec2, r1: DVec2) -> Option<DVec2> {
    creabim_geom::annotation::line_intersection(a, b, r0, r1)
}

/// A click with the Align tool. `snap` is the snapped point under the
/// pointer, `raw` the pointer itself, in the pane's drawing.
#[allow(clippy::too_many_arguments)]
pub fn click(
    doc: &mut Document,
    view: ElementId,
    kind: PaneKind,
    items: &[PlanItem],
    raw: DVec2,
    snap: Option<snaps::Snap>,
    tol: f64,
    mm: f64,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), ModelError> {
    let _ = (snap, mm);
    let picked = edge_at(items, raw, tol);
    let reference = state.align.reference.filter(|r| r.0 == view);
    let Some((_, r0, r1, ref_id)) = reference else {
        // the reference: an edge of the drawing, else the guide line under
        // the pointer
        let line = picked.map(|(id, a, b)| (a, b, Some(id))).or_else(|| {
            match state.hover_guide.filter(|h| h.0 == view)?.1.shape {
                snaps::GuideShape::Line { p, d } => Some((p, p + d, None)),
                snaps::GuideShape::Circle { .. } => None,
            }
        });
        match line {
            Some((a, b, id)) => {
                state.align.reference = Some((view, a, b, id));
                state.status = Some(hint(&state.align).into());
            }
            None => state.status = Some("Click on a straight edge, guide or grid line.".into()),
        }
        return Ok(());
    };
    let Some((id, t0, t1)) = picked else {
        state.status = Some("Click on the edge of what to align.".into());
        return Ok(());
    };
    let same_line =
        (t1 - t0).perp_dot(r1 - r0).abs() < 1e-9 && (t0 - r0).perp_dot(r1 - r0).abs() < 1e-9;
    if Some(id) == ref_id && same_line {
        state.status =
            Some("That is the reference itself: click the edge of something else.".into());
        return Ok(());
    }
    let stretch = match (state.align.mode, kind) {
        (AlignMode::Stretch, _) => true,
        // a section has no ends at an angle to meet: Extend stretches there
        (AlignMode::Extend, PaneKind::Section { .. }) => true,
        _ => false,
    };
    let grid = matches!(
        doc.model().get(id).map(|e| &e.kind),
        Some(ElementKind::Grid(_))
    );
    // a grid line has no faces: in a plan its end extends instead
    if stretch && !(grid && matches!(kind, PaneKind::Plan { .. })) {
        // of edges on top of each other (a beam's end on a column's face),
        // the first that can stretch there goes
        let model = doc.model();
        let rank = |i: ElementId| {
            let push = model.get(i).is_some_and(|e| pushpull::can_push(&e.kind));
            Some(u8::from(push))
        };
        let mut tried = paneface::edges_at(items, raw, tol, &rank);
        if tried.is_empty() {
            tried.push((id, t0, t1));
        }
        let mut first_err = None;
        let mut done = None;
        for (id, t0, t1) in tried {
            match stretch_to(model, kind, items, id, (t0, t1), (r0, r1), raw) {
                Ok(r) => {
                    done = Some((id, r));
                    break;
                }
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        state.status = match done {
            Some((id, (tx, msg))) => Some(match doc.commit(tx) {
                Ok(()) => {
                    selection.only(id);
                    msg
                }
                Err(e) => format!("Couldn't do that: {e}"),
            }),
            None => first_err,
        };
        return Ok(());
    }
    let model = doc.model();
    let Some(e) = model.get(id).cloned() else {
        return Ok(());
    };
    // what an edge of a hosted door or window belongs to is the opening;
    // aligning moves its wall
    let e = match &e.kind {
        ElementKind::Opening(o) => model.get(o.host).cloned().unwrap_or(e),
        _ => e,
    };
    let (label, after) = match (state.align.mode, kind) {
        (AlignMode::Align, PaneKind::Plan { .. }) => {
            let mut now = e.clone();
            for t in align_moves(t0, t1, r0, r1, raw) {
                match transformed(model, &now, &t) {
                    Some(n) => now = n,
                    None => {
                        state.status = Some(format!("{} can't be aligned.", e.name));
                        return Ok(());
                    }
                }
            }
            ("Align", now)
        }
        (AlignMode::Align, PaneKind::Section { start, end }) => {
            // a section only moves things, across or up its plane
            let (t, r) = ((t1 - t0).normalize_or_zero(), (r1 - r0).normalize_or_zero());
            if turn_to(t, r).abs() > 0.05 {
                state.status = Some(
                    "In a section Align only moves: pick an edge parallel to the reference (turn it in a plan)."
                        .into(),
                );
                return Ok(());
            }
            let n = r.perp();
            let d = n * (r0 - t0).dot(n);
            let along = (end - start).normalize_or_zero().extend(0.0);
            let shift = along * d.x + DVec3::Z * d.y;
            match transformed(model, &e, &Xform::Move(shift)) {
                Some(n) => ("Align", n),
                None => {
                    state.status = Some(format!("{} can't be aligned.", e.name));
                    return Ok(());
                }
            }
        }
        (AlignMode::Extend | AlignMode::Stretch, PaneKind::Plan { .. }) => {
            let mut now = e.clone();
            let ends = match &mut now.kind {
                ElementKind::Wall(w) => Some((&mut w.start, &mut w.end)),
                ElementKind::Beam(b) => Some((&mut b.start, &mut b.end)),
                ElementKind::Grid(g) => Some((&mut g.start, &mut g.end)),
                _ => None,
            };
            let Some((a, b)) = ends else {
                state.status = Some(
                    "Extend works on walls, beams and grid lines; use Stretch for the edges of other things, Align to move them."
                        .into(),
                );
                return Ok(());
            };
            let Some(hit) = meet(*a, *b, r0, r1) else {
                state.status =
                    Some("It runs parallel to the reference, so it never meets it.".into());
                return Ok(());
            };
            // the end nearer the click goes
            let end = if raw.distance(*a) <= raw.distance(*b) {
                a
            } else {
                b
            };
            *end = hit;
            ("Extend to reference", now)
        }
        (AlignMode::Extend | AlignMode::Stretch, _) => {
            state.status = Some("Extend works in plans, sections and elevations.".into());
            return Ok(());
        }
        _ => return Ok(()),
    };
    if let (ElementKind::Wall(w), true) = (&after.kind, label == "Extend to reference") {
        if w.length() < 0.01 {
            state.status = Some("That would leave the wall with no length.".into());
            return Ok(());
        }
    }
    let target = after.id;
    let before = model.get(target).cloned().expect("aligned element exists");
    if before == after {
        state.status = Some("Already aligned.".into());
        return Ok(());
    }
    doc.commit(Transaction::new(label).replace(before, after))?;
    selection.only(target);
    state.status = Some(format!(
        "{label}. Click more edges to align them to the same reference; Esc lets it go."
    ));
    Ok(())
}

/// The stretch of the face the edge `t` of `id` stands for (picked at
/// `at`) to the reference line `r`, as one undo step; walls joined to a
/// stretched wall end follow it. With it the message for the status bar;
/// else why it can't.
fn stretch_to(
    model: &creabim_core::Model,
    kind: PaneKind,
    items: &[PlanItem],
    id: ElementId,
    (t0, t1): (DVec2, DVec2),
    (r0, r1): (DVec2, DVec2),
    at: DVec2,
) -> Result<(Transaction, String), String> {
    let Some(e) = model.get(id) else {
        return Err("Click on the edge of what to stretch.".into());
    };
    let name = e.name.clone();
    if !pushpull::can_push(&e.kind) {
        return Err(format!(
            "{name} has no faces to stretch; Align moves it instead."
        ));
    }
    let (t, r) = ((t1 - t0).normalize_or_zero(), (r1 - r0).normalize_or_zero());
    if turn_to(t, r).abs() > 0.05 {
        return Err("Stretch takes an edge parallel to the reference: turn it with Align first, or use Extend for a wall or beam end at an angle.".into());
    }
    let (space, z) = match kind {
        PaneKind::Plan { level } => (Space::Plan, model.level(level).map_or(0.0, |l| l.elevation)),
        PaneKind::Section { start, end } => (Space::Section { start, end }, 0.0),
        _ => return Err("Stretch works in plans, sections and elevations.".into()),
    };
    let f = paneface::face_of_edge(items, space, id, t0, t1, at, z, 0.0);
    let s = (r0 - t0).dot(f.out);
    if s.abs() < 1e-9 {
        return Err("Already there.".into());
    }
    let mut changed = pushpull::push_face(model, &pushpull::affected(model, id), &f.pick.face, s)
        .ok_or_else(|| {
            format!("{name} can't stretch that way that far: that face doesn't move, or the shape would turn inside out.")
        })?;
    let follow = creabim_core::joins::follow(model, &mut changed);
    let mut tx = Transaction::new("Stretch to reference");
    for n in changed {
        if let Some(o) = model.get(n.id).filter(|o| **o != n) {
            tx = tx.replace(o.clone(), n);
        }
    }
    for (o, n) in follow {
        tx = tx.replace(o, n);
    }
    let way = if s > 0.0 { "out" } else { "in" };
    Ok((tx, format!(
        "Stretched {name} {} {way} to the reference. Click more edges to stretch them to it; Esc lets it go.",
        crate::units::len(s.abs())
    )))
}

/// Draw the picked reference across the pane, and the edge the pointer is
/// over, so it is clear what the next click takes.
pub fn draw_picks(
    painter: &egui::Painter,
    xf: &Xf,
    state: &AlignState,
    view: ElementId,
    hover: Option<(DVec2, DVec2)>,
) {
    let accent = Color32::from_rgb(230, 120, 30);
    if let Some((v, a, b, _)) = state.reference {
        if v == view {
            let (sa, sb) = (xf.to_screen(a), xf.to_screen(b));
            let u = (sb - sa).normalized();
            if u.x.is_finite() {
                let reach = painter
                    .clip_rect()
                    .width()
                    .max(painter.clip_rect().height())
                    * 2.0;
                painter.add(egui::Shape::dashed_line(
                    &[sa - u * reach, sa + u * reach],
                    Stroke::new(1.5, snaps::GUIDE),
                    9.0,
                    4.0,
                ));
                painter.line_segment([sa, sb], Stroke::new(4.0, snaps::GUIDE.gamma_multiply(0.5)));
            }
        }
    }
    if let Some((a, b)) = hover {
        painter.line_segment([xf.to_screen(a), xf.to_screen(b)], Stroke::new(3.0, accent));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::Element;

    fn level(doc: &Document) -> ElementId {
        doc.model().levels().next().unwrap().0.id
    }

    /// A wall like the sample's first, from `a` to `b` on `level`.
    fn wall(doc: &Document, level: ElementId, a: DVec2, b: DVec2) -> Element {
        let mut w = doc
            .model()
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some(w.clone()),
                _ => None,
            })
            .unwrap();
        (w.level, w.start, w.end) = (level, a, b);
        Element::new("Wall", ElementKind::Wall(w))
    }

    #[test]
    fn turns_take_the_short_way_round() {
        assert!((turn_to(DVec2::X, DVec2::Y) - 90.0).abs() < 1e-9);
        assert!((turn_to(DVec2::X, DVec2::new(1.0, 1.0)) - 45.0).abs() < 1e-9);
        // lines have no direction: pointing back is parallel already
        assert!(turn_to(DVec2::X, -DVec2::X).abs() < 1e-9);
        assert!((turn_to(DVec2::X, DVec2::new(-1.0, 1.0)) + 45.0).abs() < 1e-9);
    }

    #[test]
    fn a_wall_turns_parallel_to_a_reference_and_moves_flush() {
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let l = level(&doc);
        let w = wall(&doc, l, DVec2::new(20.0, 0.0), DVec2::new(24.0, 1.0));
        let id = w.id;
        doc.commit(Transaction::new("Wall").insert(w)).unwrap();
        // the reference: a line at 30° through (30, 5)
        let r0 = DVec2::new(30.0, 5.0);
        let r1 = r0 + DVec2::from_angle(30f64.to_radians());
        let ElementKind::Wall(w) = &doc.model().get(id).unwrap().kind else {
            unreachable!()
        };
        let (t0, t1) = (w.start, w.end);
        let mut e = doc.model().get(id).unwrap().clone();
        for t in align_moves(t0, t1, r0, r1, DVec2::new(22.0, 0.5)) {
            e = transformed(doc.model(), &e, &t).unwrap();
        }
        let ElementKind::Wall(a) = &e.kind else {
            unreachable!()
        };
        let r = (r1 - r0).normalize();
        // parallel, and both ends on the reference line
        assert!((a.end - a.start).normalize().perp_dot(r).abs() < 1e-9);
        for p in [a.start, a.end] {
            assert!((p - r0).perp_dot(r).abs() < 1e-9, "{p}");
        }
        // its length is kept
        assert!((a.length() - w.length()).abs() < 1e-9);
    }

    #[test]
    fn extend_stretches_the_nearer_end_to_the_reference_and_undoes() {
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let l = level(&doc);
        let w = wall(&doc, l, DVec2::new(20.0, 0.0), DVec2::new(20.0, 3.0));
        let id = w.id;
        doc.commit(Transaction::new("Wall").insert(w)).unwrap();
        let view = doc
            .model()
            .iter()
            .find(|e| matches!(&e.kind, ElementKind::View(v) if matches!(v.kind, creabim_core::ViewKind::Plan { level, .. } if level == l)))
            .unwrap()
            .id;
        let mut state = PlanState::default();
        let mut sel = Selection::default();
        state.align.mode = AlignMode::Extend;
        // a slab edge along y = 5
        state.align.reference = Some((view, DVec2::new(15.0, 5.0), DVec2::new(25.0, 5.0), None));
        let items = vec![PlanItem::Line {
            id,
            pen: creabim_geom::plan::Pen::Cut,
            a: DVec2::new(20.0, 0.0),
            b: DVec2::new(20.0, 3.0),
        }];
        let kind = PaneKind::Plan { level: l };
        click(
            &mut doc,
            view,
            kind,
            &items,
            DVec2::new(20.0, 2.8),
            None,
            0.1,
            0.01,
            &mut state,
            &mut sel,
        )
        .unwrap();
        let ElementKind::Wall(w) = &doc.model().get(id).unwrap().kind else {
            unreachable!()
        };
        assert!((w.end - DVec2::new(20.0, 5.0)).length() < 1e-9, "{}", w.end);
        assert_eq!(w.start, DVec2::new(20.0, 0.0));
        assert_eq!(doc.undo_label(), Some("Extend to reference"));
        doc.undo();
        let ElementKind::Wall(w) = &doc.model().get(id).unwrap().kind else {
            unreachable!()
        };
        assert_eq!(w.end, DVec2::new(20.0, 3.0));
        // the reference stays for the next pick
        assert!(state.align.reference.is_some());
    }

    /// The rectangle `lo`–`hi` drawn as `id`.s cut outline.
    fn fill(id: ElementId, lo: DVec2, hi: DVec2) -> PlanItem {
        PlanItem::Polygon {
            id,
            pen: creabim_geom::plan::Pen::Cut,
            points: vec![lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)],
        }
    }

    #[test]
    fn stretch_moves_a_walls_side_to_the_reference_and_keeps_the_other() {
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let l = level(&doc);
        let w = wall(&doc, l, DVec2::new(20.0, 0.0), DVec2::new(26.0, 0.0));
        let (id, t) = (
            w.id,
            match &w.kind {
                ElementKind::Wall(w) => w.thickness,
                _ => unreachable!(),
            },
        );
        doc.commit(Transaction::new("Wall").insert(w)).unwrap();
        let mut state = PlanState::default();
        let mut sel = Selection::default();
        state.align.mode = AlignMode::Stretch;
        let view = ElementId::new();
        // a reference 0.5 north of the wall's north face
        let north = t / 2.0;
        state.align.reference = Some((
            view,
            DVec2::new(0.0, north + 0.5),
            DVec2::new(1.0, north + 0.5),
            None,
        ));
        let items = vec![fill(id, DVec2::new(20.0, -north), DVec2::new(26.0, north))];
        click(
            &mut doc,
            view,
            PaneKind::Plan { level: l },
            &items,
            DVec2::new(23.0, north),
            None,
            0.05,
            0.01,
            &mut state,
            &mut sel,
        )
        .unwrap();
        let ElementKind::Wall(after) = doc.model().get(id).unwrap().kind.clone() else {
            unreachable!()
        };
        // thicker by the gap, its south face where it was
        assert!((after.thickness - t - 0.5).abs() < 1e-9);
        assert!((after.start.y - after.thickness / 2.0 + north).abs() < 1e-9);
        assert_eq!(doc.undo_label(), Some("Stretch to reference"));
        // and its east end to a line at x = 28, in the same mode
        state.align.reference = Some((view, DVec2::new(28.0, 0.0), DVec2::new(28.0, 1.0), None));
        let n = after.thickness / 2.0 + after.start.y;
        let s = after.start.y - after.thickness / 2.0;
        let items = vec![fill(id, DVec2::new(20.0, s), DVec2::new(26.0, n))];
        click(
            &mut doc,
            view,
            PaneKind::Plan { level: l },
            &items,
            DVec2::new(26.0, after.start.y),
            None,
            0.05,
            0.01,
            &mut state,
            &mut sel,
        )
        .unwrap();
        let ElementKind::Wall(after) = &doc.model().get(id).unwrap().kind else {
            unreachable!()
        };
        assert!((after.end.x - 28.0).abs() < 1e-9);
        assert!((after.start.x - 20.0).abs() < 1e-9);
    }

    #[test]
    fn stretch_in_a_section_drops_a_slabs_underside_to_a_line() {
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let (id, s) = doc
            .model()
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Slab(s) => Some((e.id, s.clone())),
                _ => None,
            })
            .unwrap();
        let z0 = doc.model().level(s.level).unwrap().elevation;
        let (top, bottom) = (s.top(z0), s.top(z0) - s.thickness);
        // a section along x through the slab: it is cut from x = 0 to 10
        let kind = PaneKind::Section {
            start: DVec2::new(0.0, 3.0),
            end: DVec2::new(10.0, 3.0),
        };
        let items = vec![fill(id, DVec2::new(0.0, bottom), DVec2::new(10.0, top))];
        let mut state = PlanState::default();
        let mut sel = Selection::default();
        state.align.mode = AlignMode::Stretch;
        let view = ElementId::new();
        state.align.reference = Some((
            view,
            DVec2::new(-5.0, bottom - 0.3),
            DVec2::new(15.0, bottom - 0.3),
            None,
        ));
        click(
            &mut doc,
            view,
            kind,
            &items,
            DVec2::new(5.0, bottom),
            None,
            0.05,
            0.01,
            &mut state,
            &mut sel,
        )
        .unwrap();
        let ElementKind::Slab(after) = &doc.model().get(id).unwrap().kind else {
            unreachable!()
        };
        assert!((after.thickness - s.thickness - 0.3).abs() < 1e-9);
        assert!((after.top(z0) - top).abs() < 1e-9);
        // and its east edge out to x = 12 in the same section
        state.align.reference = Some((view, DVec2::new(12.0, 0.0), DVec2::new(12.0, 1.0), None));
        let items = vec![fill(
            id,
            DVec2::new(0.0, bottom - 0.3),
            DVec2::new(10.0, top),
        )];
        click(
            &mut doc,
            view,
            kind,
            &items,
            DVec2::new(10.0, top - 0.1),
            None,
            0.05,
            0.01,
            &mut state,
            &mut sel,
        )
        .unwrap();
        let ElementKind::Slab(after) = &doc.model().get(id).unwrap().kind else {
            unreachable!()
        };
        let east = after.boundary.iter().map(|p| p.x).fold(f64::MIN, f64::max);
        assert!((east - 12.0).abs() < 1e-9, "{:?}", after.boundary);
    }
}
