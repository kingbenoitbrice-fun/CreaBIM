//! The Align tool, as in Revit, with ArchiCAD's extend-to-a-reference:
//! click a reference line (a wall face, a slab edge, a guide, a grid line,
//! any edge of the drawing, or a temporary guide line), then the edge of
//! what to align. The element turns to lie parallel to the reference and
//! moves until its edge is flush with it. In Extend mode the end of a
//! wall, beam or grid line nearest the click stretches (or trims) to meet
//! the reference instead. The reference stays picked, so several things
//! can be aligned to it; Esc lets it go. Every alignment is one undo step.

use crate::planview::{PaneKind, PlanState, Xf};
use crate::selection::Selection;
use crate::snaps;
use creabim_core::transform::transformed;
use creabim_core::{Document, ElementId, ElementKind, ModelError, Transaction, Xform};
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
}

impl AlignMode {
    pub const ALL: [AlignMode; 2] = [AlignMode::Align, AlignMode::Extend];

    pub fn label(self) -> &'static str {
        match self {
            AlignMode::Align => "Align",
            AlignMode::Extend => "Extend to it",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            AlignMode::Align => "Turn the edge you pick parallel to the reference and move it flush",
            AlignMode::Extend => {
                "Stretch or trim the end of a wall, beam or grid line you pick to meet the reference"
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
    }
}

/// The straight edge of the drawing under `p`: its element and ends.
pub fn edge_at(items: &[PlanItem], p: DVec2, tol: f64) -> Option<(ElementId, DVec2, DVec2)> {
    crate::annotate::segment_at(items, p, tol)
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
        (AlignMode::Extend, PaneKind::Plan { .. }) => {
            let mut now = e.clone();
            let ends = match &mut now.kind {
                ElementKind::Wall(w) => Some((&mut w.start, &mut w.end)),
                ElementKind::Beam(b) => Some((&mut b.start, &mut b.end)),
                ElementKind::Grid(g) => Some((&mut g.start, &mut g.end)),
                _ => None,
            };
            let Some((a, b)) = ends else {
                state.status = Some(
                    "Extend works on walls, beams and grid lines; use Align to move other things."
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
        (AlignMode::Extend, _) => {
            state.status = Some("Extend works in plans.".into());
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
}
