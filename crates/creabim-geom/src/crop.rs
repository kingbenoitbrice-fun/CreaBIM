//! Cropping a drawing to a rectangle, for viewports that show only part of a view.

use crate::plan::{PlanItem, PlanView};
use glam::DVec2;

/// The part of segment a → b inside the box, if any (Liang–Barsky).
pub fn clip_segment(a: DVec2, b: DVec2, lo: DVec2, hi: DVec2) -> Option<(DVec2, DVec2)> {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-d.x, a.x - lo.x),
        (d.x, hi.x - a.x),
        (-d.y, a.y - lo.y),
        (d.y, hi.y - a.y),
    ] {
        if p.abs() < 1e-15 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 < t1).then(|| (a + d * t0, a + d * t1))
}

/// A polygon clipped to the box (Sutherland–Hodgman).
pub fn clip_polygon(poly: &[DVec2], lo: DVec2, hi: DVec2) -> Vec<DVec2> {
    let mut out = poly.to_vec();
    // each side of the box as (axis, limit, keep greater)
    for (axis, limit, greater) in [
        (0, lo.x, true),
        (0, hi.x, false),
        (1, lo.y, true),
        (1, hi.y, false),
    ] {
        let inside = |p: DVec2| {
            let v = if axis == 0 { p.x } else { p.y };
            if greater {
                v >= limit
            } else {
                v <= limit
            }
        };
        let input = std::mem::take(&mut out);
        for i in 0..input.len() {
            let (p, q) = (input[i], input[(i + 1) % input.len()]);
            if inside(p) {
                out.push(p);
            }
            if inside(p) != inside(q) {
                let (vp, vq) = if axis == 0 { (p.x, q.x) } else { (p.y, q.y) };
                out.push(p + (q - p) * ((limit - vp) / (vq - vp)));
            }
        }
        if out.len() < 3 {
            return vec![];
        }
    }
    out
}

fn inside(p: DVec2, lo: DVec2, hi: DVec2) -> bool {
    p.cmpge(lo).all() && p.cmple(hi).all()
}

/// The drawing with everything outside `lo`..`hi` cut away. Dimensions,
/// tags and labels are kept only when they fit inside whole.
pub fn crop(d: &PlanView, lo: DVec2, hi: DVec2) -> PlanView {
    let mut items = Vec::with_capacity(d.items.len());
    let lines = |items: &mut Vec<PlanItem>, id, pen, pts: &[DVec2], closed: bool| {
        let n = pts.len();
        let segs = if closed { n } else { n.saturating_sub(1) };
        for i in 0..segs {
            if let Some((a, b)) = clip_segment(pts[i], pts[(i + 1) % n], lo, hi) {
                items.push(PlanItem::Line { id, pen, a, b });
            }
        }
    };
    for it in &d.items {
        match it {
            PlanItem::Line { id, pen, a, b } => lines(&mut items, *id, *pen, &[*a, *b], false),
            PlanItem::Polygon { id, pen, points } => {
                if points.iter().all(|p| inside(*p, lo, hi)) {
                    items.push(it.clone());
                } else {
                    lines(&mut items, *id, *pen, points, true);
                }
            }
            PlanItem::Fill { id, points, hatch } => {
                let points = clip_polygon(points, lo, hi);
                if !points.is_empty() {
                    items.push(PlanItem::Fill {
                        id: *id,
                        points,
                        hatch: *hatch,
                    });
                }
            }
            PlanItem::Arc {
                id,
                pen,
                center,
                radius,
                start,
                end,
            } => {
                let pts: Vec<DVec2> = (0..=24)
                    .map(|i| {
                        *center
                            + DVec2::from_angle(start + (end - start) * i as f64 / 24.0) * *radius
                    })
                    .collect();
                if pts.iter().all(|p| inside(*p, lo, hi)) {
                    items.push(it.clone());
                } else {
                    lines(&mut items, *id, *pen, &pts, false);
                }
            }
            PlanItem::Label { at, .. } | PlanItem::Text { at, .. } => {
                if inside(*at, lo, hi) {
                    items.push(it.clone());
                }
            }
            PlanItem::Dimension { a, b, offset, .. } => {
                let n = (*b - *a).normalize_or_zero().perp() * *offset;
                if [*a, *b, *a + n, *b + n].iter().all(|p| inside(*p, lo, hi)) {
                    items.push(it.clone());
                }
            }
            PlanItem::Tag { anchor, at, .. } => {
                if inside(*anchor, lo, hi) && inside(*at, lo, hi) {
                    items.push(it.clone());
                }
            }
            PlanItem::Image {
                id,
                raster,
                lo: ilo,
                hi: ihi,
                clip,
            } => {
                let (clo, chi) = clip.unwrap_or((*ilo, *ihi));
                let (clo, chi) = (clo.max(lo), chi.min(hi));
                if clo.cmplt(chi).all() {
                    items.push(PlanItem::Image {
                        id: *id,
                        raster: raster.clone(),
                        lo: *ilo,
                        hi: *ihi,
                        clip: Some((clo, chi)),
                    });
                }
            }
        }
    }
    PlanView { items }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_and_polygons_are_clipped_to_the_box() {
        let (lo, hi) = (DVec2::ZERO, DVec2::splat(1.0));
        let (a, b) = clip_segment(DVec2::new(-1.0, 0.5), DVec2::new(2.0, 0.5), lo, hi).unwrap();
        assert!(
            a.distance(DVec2::new(0.0, 0.5)) < 1e-12 && b.distance(DVec2::new(1.0, 0.5)) < 1e-12
        );
        assert!(clip_segment(DVec2::new(2.0, 0.0), DVec2::new(3.0, 1.0), lo, hi).is_none());
        let sq = [
            DVec2::new(0.5, 0.5),
            DVec2::new(2.0, 0.5),
            DVec2::new(2.0, 2.0),
            DVec2::new(0.5, 2.0),
        ];
        let c = clip_polygon(&sq, lo, hi);
        assert!(c.iter().all(|p| inside(*p, lo, hi)));
        assert_eq!(c.len(), 4);
    }
}
