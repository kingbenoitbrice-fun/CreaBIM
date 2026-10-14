//! Box selection, the same in plans, sections and 3D. Dragging left to
//! right takes what lies wholly inside the box; right to left also takes
//! what the box only touches, as in CAD.

use creabim_core::ElementId;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use std::collections::HashMap;

/// Does a box from `start` to `now` take what it touches?
pub fn crossing(start: Pos2, now: Pos2) -> bool {
    now.x < start.x
}

/// Draw the box: solid for inside, dashed for touching.
pub fn draw(painter: &egui::Painter, start: Pos2, now: Pos2) {
    let r = Rect::from_two_pos(start, now);
    let (fill, stroke) = if crossing(start, now) {
        (
            Color32::from_rgba_unmultiplied(60, 170, 90, 30),
            Stroke::new(1.0, Color32::from_rgb(40, 140, 70)),
        )
    } else {
        (
            Color32::from_rgba_unmultiplied(40, 110, 220, 30),
            Stroke::new(1.0, Color32::from_rgb(40, 110, 220)),
        )
    };
    painter.rect_filled(r, 0.0, fill);
    if crossing(start, now) {
        let c = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
        ];
        painter.extend(egui::Shape::dashed_line(&c, stroke, 6.0, 4.0));
    } else {
        painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
    }
}

/// The ids a box from `start` to `now` takes, given each element's parts
/// as screen rectangles (an element may have several).
pub fn pick(
    start: Pos2,
    now: Pos2,
    parts: impl IntoIterator<Item = (ElementId, Rect)>,
) -> Vec<ElementId> {
    let r = Rect::from_two_pos(start, now);
    let touch = crossing(start, now);
    // per element: (any part touched, every part inside)
    let mut seen: Vec<ElementId> = Vec::new();
    let mut state: HashMap<ElementId, (bool, bool)> = HashMap::new();
    for (id, p) in parts {
        if id == ElementId::NIL {
            continue;
        }
        let s = state.entry(id).or_insert_with(|| {
            seen.push(id);
            (false, true)
        });
        s.0 |= r.intersects(p);
        s.1 &= r.contains_rect(p);
    }
    seen.into_iter()
        .filter(|id| {
            let (any, all) = state[id];
            if touch {
                any
            } else {
                all
            }
        })
        .collect()
}

/// The ids a box takes in 3D, given each element's triangles on screen.
/// Testing triangles rather than bounding boxes keeps a crossing box from
/// picking a wall it only passes near.
pub fn pick_triangles(
    start: Pos2,
    now: Pos2,
    tris: impl IntoIterator<Item = (ElementId, [Pos2; 3])>,
) -> Vec<ElementId> {
    let r = Rect::from_two_pos(start, now);
    let touch = crossing(start, now);
    let mut seen: Vec<ElementId> = Vec::new();
    let mut state: HashMap<ElementId, (bool, bool)> = HashMap::new();
    for (id, t) in tris {
        if id == ElementId::NIL {
            continue;
        }
        let s = state.entry(id).or_insert_with(|| {
            seen.push(id);
            (false, true)
        });
        if !s.0 {
            s.0 = touches(r, t);
        }
        s.1 &= t.iter().all(|p| r.contains(*p));
    }
    seen.into_iter()
        .filter(|id| {
            let (any, all) = state[id];
            if touch {
                any
            } else {
                all
            }
        })
        .collect()
}

/// Does triangle `t` overlap rectangle `r`?
fn touches(r: Rect, t: [Pos2; 3]) -> bool {
    let tb = Rect::from_points(&t);
    if !r.intersects(tb) {
        return false;
    }
    if t.iter().any(|p| r.contains(*p)) {
        return true;
    }
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
    ];
    let cross = |a: Pos2, b: Pos2, p: Pos2| (b - a).x * (p - a).y - (b - a).y * (p - a).x;
    let inside = |p: Pos2| {
        let d = [
            cross(t[0], t[1], p),
            cross(t[1], t[2], p),
            cross(t[2], t[0], p),
        ];
        d.iter().all(|v| *v >= 0.0) || d.iter().all(|v| *v <= 0.0)
    };
    if corners.iter().any(|c| inside(*c)) {
        return true;
    }
    // an edge of the triangle crossing an edge of the rectangle
    let hit = |a: Pos2, b: Pos2, c: Pos2, d: Pos2| {
        let (d1, d2) = (cross(c, d, a), cross(c, d, b));
        let (d3, d4) = (cross(a, b, c), cross(a, b, d));
        d1 * d2 < 0.0 && d3 * d4 < 0.0
    };
    (0..3).any(|i| {
        let (a, b) = (t[i], t[(i + 1) % 3]);
        (0..4).any(|k| hit(a, b, corners[k], corners[(k + 1) % 4]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn left_to_right_takes_whole_elements_right_to_left_takes_touched_ones() {
        let (a, b) = (ElementId::new(), ElementId::new());
        let parts = vec![
            (a, Rect::from_min_max(pos2(10.0, 10.0), pos2(20.0, 20.0))),
            (b, Rect::from_min_max(pos2(15.0, 15.0), pos2(60.0, 20.0))),
        ];
        let window = pick(pos2(0.0, 0.0), pos2(30.0, 30.0), parts.clone());
        assert_eq!(window, vec![a]);
        let cross = pick(pos2(30.0, 30.0), pos2(0.0, 0.0), parts);
        assert_eq!(cross, vec![a, b]);
    }

    #[test]
    fn a_crossing_box_in_the_corner_of_a_triangles_bounds_misses_it() {
        let a = ElementId::new();
        // a thin diagonal triangle from (0,0) to (100,100)
        let t = [pos2(0.0, 0.0), pos2(100.0, 100.0), pos2(0.0, 5.0)];
        assert!(pick_triangles(pos2(100.0, 0.0), pos2(80.0, 20.0), [(a, t)]).is_empty());
        assert_eq!(
            pick_triangles(pos2(60.0, 40.0), pos2(40.0, 60.0), [(a, t)]),
            vec![a]
        );
    }
}
