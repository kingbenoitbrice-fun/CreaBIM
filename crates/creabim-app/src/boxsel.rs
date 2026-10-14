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
}
