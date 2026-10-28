//! Rooms, room separators and ceilings in drawings: the rooms' floors (to
//! pick them by, and washed in colour when the view says so), separation
//! lines, ceiling plans, and rooms shaded in sections.

use crate::plan::{Pen, PlanItem};
use creabim_core::objects::{self, Mount};
use creabim_core::room::{self, Room};
use creabim_core::{ElementId, ElementKind, Model, OpeningKind, View};
use glam::DVec2;

/// Pale washes for rooms, in turn.
const PALETTE: [[f32; 3]; 8] = [
    [0.99, 0.85, 0.62],
    [0.74, 0.87, 0.97],
    [0.78, 0.92, 0.74],
    [0.96, 0.78, 0.80],
    [0.86, 0.80, 0.95],
    [0.97, 0.93, 0.65],
    [0.72, 0.90, 0.88],
    [0.93, 0.84, 0.74],
];

/// A room's wash: one colour for each department (or, without one, each
/// name), the same every time.
pub fn colour_of(name: &str, r: &Room) -> [f32; 3] {
    let key = if r.department.is_empty() {
        name
    } else {
        &r.department
    };
    let h = key
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    PALETTE[h as usize % PALETTE.len()]
}

/// What a plan of `level` draws of its rooms, room separators and (in a
/// ceiling plan) ceilings. Rooms go first, under everything.
pub fn plan_items(model: &Model, view: &View, level: ElementId, cut_height: f64) -> Vec<PlanItem> {
    let mut out = vec![];
    for e in model.iter() {
        match &e.kind {
            ElementKind::Room(r) if r.level == level => {
                if let Some(s) = room::shape_of(model, r) {
                    out.push(PlanItem::Room {
                        id: e.id,
                        points: s.outline,
                        rgb: view.room_colours.then(|| colour_of(&e.name, r)),
                    });
                }
            }
            ElementKind::RoomSeparator(s) if s.level == level => out.push(PlanItem::Line {
                id: e.id,
                pen: Pen::Symbol,
                a: s.a,
                b: s.b,
            }),
            ElementKind::Ceiling(c) if c.level == level && view.ceiling_plan => {
                if c.height < cut_height {
                    continue;
                }
                let Some(outline) = c.outline(model) else {
                    continue;
                };
                out.push(PlanItem::Polygon {
                    id: e.id,
                    pen: Pen::Projection,
                    points: outline.clone(),
                });
                if c.grid > 0.0 {
                    // a square grid, its lines from the outline's corner
                    for (a, b) in grid_lines(&outline, c.grid) {
                        out.push(PlanItem::Line {
                            id: e.id,
                            pen: Pen::Symbol,
                            a,
                            b,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// A square grid of `size` over `outline`, clipped to it.
fn grid_lines(outline: &[DVec2], size: f64) -> Vec<(DVec2, DVec2)> {
    let mut out = vec![];
    let (lo, hi) = outline.iter().fold(
        (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    if (hi - lo).max_element() / size > 400.0 {
        return out;
    }
    let mut x = lo.x + size;
    while x < hi.x - 1e-6 {
        out.extend(clip_line(
            outline,
            DVec2::new(x, lo.y - 1.0),
            DVec2::new(x, hi.y + 1.0),
        ));
        x += size;
    }
    let mut y = lo.y + size;
    while y < hi.y - 1e-6 {
        out.extend(clip_line(
            outline,
            DVec2::new(lo.x - 1.0, y),
            DVec2::new(hi.x + 1.0, y),
        ));
        y += size;
    }
    out
}

/// The pieces of segment `a` → `b` inside `poly`.
fn clip_line(poly: &[DVec2], a: DVec2, b: DVec2) -> Vec<(DVec2, DVec2)> {
    let d = b - a;
    let n = poly.len();
    let mut ts: Vec<f64> = vec![];
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let e = q - p;
        let den = d.perp_dot(e);
        if den.abs() < 1e-12 {
            continue;
        }
        let t = (p - a).perp_dot(e) / den;
        let s = (p - a).perp_dot(d) / den;
        if (0.0..=1.0).contains(&t) && (0.0..1.0).contains(&s) {
            ts.push(t);
        }
    }
    ts.sort_by(f64::total_cmp);
    ts.chunks(2)
        .filter(|c| c.len() == 2 && c[1] - c[0] > 1e-9)
        .map(|c| (a + d * c[0], a + d * c[1]))
        .collect()
}

/// A plan turned into a ceiling plan: what stands on the floor (furniture
/// and fixtures that don't hang from the ceiling, floors, stairs and
/// railings) and the swings of doors are left out; walls, windows, what
/// hangs from the ceiling and the ceilings themselves stay.
pub fn as_ceiling_plan(model: &Model, items: &mut Vec<PlanItem>) {
    items.retain(|it| {
        let id = it.id();
        match model.get(id).map(|e| &e.kind) {
            Some(ElementKind::Object(_)) => {
                let e = model.get(id).unwrap();
                objects::type_of(model, e).is_some_and(|t| t.mount == Mount::Ceiling)
            }
            Some(
                ElementKind::Slab(_)
                | ElementKind::Stair(_)
                | ElementKind::Railing(_)
                | ElementKind::Footing(_),
            ) => false,
            Some(ElementKind::Opening(o)) if o.kind == OpeningKind::Door => {
                let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
                    return true;
                };
                // keep the frame in the wall, not the leaf swung out of it
                let inside = |p: &DVec2| {
                    (*p - w.start).dot(w.direction().perp()).abs() <= w.thickness / 2.0 + 1e-6
                };
                match it {
                    PlanItem::Arc { .. } => false,
                    PlanItem::Polygon { points, .. } => points.iter().all(inside),
                    PlanItem::Line { a, b, .. } => inside(a) && inside(b),
                    _ => true,
                }
            }
            _ => true,
        }
    });
}

/// The rooms a section cuts through, washed in their colours from their
/// floor up to their top: one band for each stretch of the section line
/// inside a room. Drawing coordinates are (along the line, height).
pub fn section_items(model: &Model, start: DVec2, end: DVec2) -> Vec<PlanItem> {
    let mut out = vec![];
    let len = start.distance(end);
    if len < 1e-9 {
        return out;
    }
    for e in model.iter() {
        let ElementKind::Room(r) = &e.kind else {
            continue;
        };
        let Some(s) = room::shape_of(model, r) else {
            continue;
        };
        let floor = model.level(r.level).map_or(0.0, |l| l.elevation);
        let top = floor + room::height(model, r);
        for (a, b) in clip_line(&s.outline, start, end) {
            let (x0, x1) = (a.distance(start), b.distance(start));
            out.push(PlanItem::Tint {
                id: e.id,
                points: vec![
                    DVec2::new(x0, floor),
                    DVec2::new(x1, floor),
                    DVec2::new(x1, top),
                    DVec2::new(x0, top),
                ],
                rgb: colour_of(&e.name, r),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_and_lines_clip_to_the_outline() {
        let sq = creabim_core::polygon::rectangle(DVec2::ZERO, DVec2::new(3.0, 2.0));
        let lines = grid_lines(&sq, 0.6);
        // four lines across x (0.6..2.4) and three across y (0.6..1.8)
        assert_eq!(lines.len(), 4 + 3);
        for (a, b) in &lines {
            assert!(a.x >= -1e-9 && b.x <= 3.0 + 1e-9 && a.y >= -1e-9 && b.y <= 2.0 + 1e-9);
        }
        let l = clip_line(&sq, DVec2::new(-1.0, 1.0), DVec2::new(5.0, 1.0));
        assert_eq!(l.len(), 1);
        assert!((l[0].0.x).abs() < 1e-9 && (l[0].1.x - 3.0).abs() < 1e-9);
    }
}
