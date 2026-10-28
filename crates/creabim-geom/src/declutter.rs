//! Keeping a plan's text readable: room names, notes, tags and plain
//! labels are nudged off the lines and other text they would sit on, to
//! the nearest clear spot, with a leader back when they move far.

use crate::annotate::{dimension_geometry, tag_geometry};
use crate::annotation::Shape;
use crate::plan::{is_bubble, Pen, PlanItem};
use glam::DVec2;

/// A box on the drawing, in drawing units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextBox {
    pub lo: DVec2,
    pub hi: DVec2,
}

impl TextBox {
    fn around(pts: impl IntoIterator<Item = DVec2>) -> Option<Self> {
        let mut it = pts.into_iter();
        let first = it.next()?;
        let (lo, hi) = it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p)));
        Some(Self { lo, hi })
    }

    fn moved(self, d: DVec2) -> Self {
        Self {
            lo: self.lo + d,
            hi: self.hi + d,
        }
    }

    fn shrunk(self, by: f64) -> Self {
        let c = (self.lo + self.hi) / 2.0;
        Self {
            lo: (self.lo + DVec2::splat(by)).min(c),
            hi: (self.hi - DVec2::splat(by)).max(c),
        }
    }

    pub fn overlaps(&self, o: &TextBox) -> bool {
        self.lo.x < o.hi.x && o.lo.x < self.hi.x && self.lo.y < o.hi.y && o.lo.y < self.hi.y
    }

    fn centre(&self) -> DVec2 {
        (self.lo + self.hi) / 2.0
    }

    /// Does segment `a` → `b` pass through the box?
    pub fn crossed_by(&self, a: DVec2, b: DVec2) -> bool {
        // Liang-Barsky clipping
        let d = b - a;
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        for (p, q) in [
            (-d.x, a.x - self.lo.x),
            (d.x, self.hi.x - a.x),
            (-d.y, a.y - self.lo.y),
            (d.y, self.hi.y - a.y),
        ] {
            if p.abs() < 1e-15 {
                if q < 0.0 {
                    return false;
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    t0 = t0.max(r);
                } else {
                    t1 = t1.min(r);
                }
            }
        }
        t0 < t1
    }
}

/// Size of plain labels' text, paper mm (as [`crate::paper::draw`] draws them).
const LABEL_SIZE: f64 = 2.5;

/// The text box of a label, tag or note, and whether the pass may move it.
pub fn text_box(item: &PlanItem, mm: f64) -> Option<(TextBox, bool)> {
    match item {
        PlanItem::Label { at, text, .. } => {
            if is_bubble(text) {
                let r = 2.5 * crate::plan::bubble_scale(text) * mm;
                return Some((
                    TextBox {
                        lo: *at - DVec2::splat(r),
                        hi: *at + DVec2::splat(r),
                    },
                    false,
                ));
            }
            let half = DVec2::new(
                crate::paper::text_width(text, LABEL_SIZE) / 2.0,
                LABEL_SIZE * 0.6,
            ) * mm;
            Some((
                TextBox {
                    lo: *at - half,
                    hi: *at + half,
                },
                true,
            ))
        }
        PlanItem::Tag {
            anchor,
            at,
            text,
            round,
            look,
            ..
        } => {
            let g = tag_geometry(*anchor, *at, text, *round, look, mm);
            let b = TextBox::around(g.outline.iter().copied()).or_else(|| {
                let half = DVec2::new(look.width(text) / 2.0, look.size * 0.6) * mm;
                Some(TextBox {
                    lo: *at - half,
                    hi: *at + half,
                })
            })?;
            Some((b, true))
        }
        PlanItem::Annotation { item: a, .. } => {
            let m = a.marks(mm);
            let pts = m.texts.iter().flat_map(|t| {
                let w = a.look.width(&t.text) * mm;
                let h = a.look.size * 0.6 * mm;
                let (dir, up) = (
                    DVec2::from_angle(t.angle),
                    DVec2::from_angle(t.angle).perp(),
                );
                let x0 = match t.anchor {
                    crate::paper::Anchor::Start => 0.0,
                    crate::paper::Anchor::Middle => -w / 2.0,
                    crate::paper::Anchor::End => -w,
                };
                let o = t.at + dir * x0;
                [
                    o - up * h,
                    o + dir * w + up * h,
                    o + dir * w - up * h,
                    o + up * h,
                ]
            });
            let b = TextBox::around(pts)?;
            Some((b, matches!(a.shape, Shape::Note { .. })))
        }
        PlanItem::Dimension {
            a,
            b,
            offset,
            text,
            look,
            witness,
            ..
        } => {
            let g = dimension_geometry(*a, *b, *offset, *witness, look, mm);
            let w = look.width(text) * mm;
            let h = look.size * 0.6 * mm;
            let (dir, up) = (
                DVec2::from_angle(g.angle),
                DVec2::from_angle(g.angle).perp(),
            );
            let c = g.text_at;
            let b = TextBox::around([
                c - dir * w / 2.0 - up * h,
                c + dir * w / 2.0 - up * h,
                c + dir * w / 2.0 + up * h,
                c - dir * w / 2.0 + up * h,
            ])?;
            Some((b, false))
        }
        _ => None,
    }
}

/// The lines a label should keep off, with how much sitting on each
/// costs: cut lines most, then the rest.
fn obstacles(items: &[PlanItem], mm: f64) -> Vec<(DVec2, DVec2, f64, usize)> {
    let mut out = Vec::new();
    for (k, it) in items.iter().enumerate() {
        match it {
            PlanItem::Line { pen, a, b, .. } => {
                let w = match pen {
                    Pen::Guide | Pen::Underlay => continue,
                    Pen::Cut => 3.0,
                    Pen::Grid => 2.0,
                    _ => 1.0,
                };
                out.push((*a, *b, w, k));
            }
            PlanItem::Polygon { pen, points, .. } => {
                if *pen == Pen::Guide || *pen == Pen::Underlay {
                    continue;
                }
                let w = if *pen == Pen::Cut { 3.0 } else { 1.0 };
                let n = points.len();
                for i in 0..n {
                    out.push((points[i], points[(i + 1) % n], w, k));
                }
            }
            PlanItem::Arc {
                pen,
                center,
                radius,
                start,
                end,
                ..
            } if *pen != Pen::Underlay => {
                let p = |i: usize| {
                    *center + DVec2::from_angle(start + (end - start) * i as f64 / 8.0) * *radius
                };
                for i in 0..8 {
                    out.push((p(i), p(i + 1), 1.0, k));
                }
            }
            PlanItem::Dimension {
                a,
                b,
                offset,
                look,
                witness,
                ..
            } => {
                for (p, q) in dimension_geometry(*a, *b, *offset, *witness, look, mm).lines {
                    out.push((p, q, 1.0, k));
                }
            }
            PlanItem::Tag {
                anchor,
                at,
                text,
                round,
                look,
                ..
            } => {
                if let Some((p, q)) = tag_geometry(*anchor, *at, text, *round, look, mm).leader {
                    out.push((p, q, 1.0, k));
                }
            }
            PlanItem::Annotation { item, .. } => {
                for l in item.marks(mm).lines {
                    for w in l.windows(2) {
                        out.push((w[0], w[1], 1.0, k));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// How far a label may move to find a clear spot, paper mm.
const REACH: f64 = 14.0;
/// A label moved further than this, paper mm, gets a leader back.
const LEADER_PAST: f64 = 9.0;

/// Move labels, notes and tags off lines and each other. `mm` is drawing
/// units per paper mm. Grid and section bubbles and dimension text stay
/// where they are and are kept clear of.
pub fn place_labels(items: &mut Vec<PlanItem>, mm: f64) {
    let segs = obstacles(items, mm);
    // what is placed so far: fixed text first
    let mut taken: Vec<(TextBox, usize)> = Vec::new();
    let mut movable: Vec<(usize, TextBox)> = Vec::new();
    for (k, it) in items.iter().enumerate() {
        match text_box(it, mm) {
            Some((b, false)) => taken.push((b, k)),
            Some((b, true)) => movable.push((k, b)),
            None => {}
        }
    }
    // the bigger texts first (room names, notes), tags after
    movable.sort_by(|a, b| {
        let area = |t: &TextBox| (t.hi - t.lo).x * (t.hi - t.lo).y;
        area(&b.1).total_cmp(&area(&a.1))
    });
    let pad = 0.4 * mm;
    let mut leaders: Vec<PlanItem> = Vec::new();
    for (k, b) in movable {
        let own = items[k].id();
        // only what is near matters
        let reach = REACH * mm + (b.hi - b.lo).length();
        let near = TextBox {
            lo: b.lo - DVec2::splat(reach),
            hi: b.hi + DVec2::splat(reach),
        };
        let local: Vec<(DVec2, DVec2, f64)> = segs
            .iter()
            .filter(|(p, q, _, i)| {
                *i != k
                    && !(own == items[*i].id() && is_own_mark(&items[k]))
                    && near.crossed_by(*p, *q)
            })
            .map(|(p, q, w, _)| (*p, *q, *w))
            .collect();
        let cost = |c: &TextBox| -> f64 {
            let c2 = c.shrunk(-pad);
            let lines: f64 = local
                .iter()
                .filter(|(p, q, _)| c2.crossed_by(*p, *q))
                .map(|s| s.2)
                .sum();
            let texts = taken.iter().filter(|(t, _)| t.overlaps(&c2)).count() as f64;
            lines + texts * 10.0
        };
        let here = cost(&b);
        if here == 0.0 {
            taken.push((b, k));
            continue;
        }
        let h = (b.hi.y - b.lo.y).max(1.0 * mm);
        let step = h * 0.5;
        let mut best = (here, DVec2::ZERO);
        let rings = (REACH * mm / step).ceil() as i32;
        'search: for r in 1..=rings {
            let dist = r as f64 * step;
            let around = (8 * r).min(48);
            for i in 0..around {
                let d = DVec2::from_angle(i as f64 / around as f64 * std::f64::consts::TAU) * dist;
                // a slight preference for staying close
                let c = cost(&b.moved(d)) + dist / (REACH * mm) * 0.5;
                if c < best.0 - 1e-9 {
                    best = (c, d);
                    if c < 0.5 {
                        break 'search;
                    }
                }
            }
        }
        let d = best.1;
        if d == DVec2::ZERO {
            taken.push((b, k));
            continue;
        }
        let nb = b.moved(d);
        let far = d.length() > LEADER_PAST * mm;
        shift(&mut items[k], d, b.centre(), far, &mut leaders, nb);
        taken.push((nb, k));
    }
    items.extend(leaders);
}

/// Is the item a tag or note, whose own leader and outline it may sit on?
fn is_own_mark(item: &PlanItem) -> bool {
    matches!(item, PlanItem::Tag { .. } | PlanItem::Annotation { .. })
}

/// Move an item's text by `d`. Moved `far`, a note gets a leader to
/// where its text was and a plain label a line back to it; a tag keeps
/// its own leader to what it tags.
fn shift(
    item: &mut PlanItem,
    d: DVec2,
    was: DVec2,
    far: bool,
    leaders: &mut Vec<PlanItem>,
    now: TextBox,
) {
    match item {
        PlanItem::Label { id, at, .. } => {
            *at += d;
            if far {
                leaders.push(PlanItem::Line {
                    id: *id,
                    pen: Pen::Fine,
                    a: edge_toward(&now, was),
                    b: was,
                });
            }
        }
        PlanItem::Tag { at, .. } => *at += d,
        PlanItem::Annotation { item, .. } => {
            if let Shape::Note { at, leader } = &mut item.shape {
                *at += d;
                if far && leader.is_none() {
                    *leader = Some(was);
                }
            }
        }
        _ => {}
    }
}

/// Where a line from the box's centre toward `p` leaves the box.
fn edge_toward(b: &TextBox, p: DVec2) -> DVec2 {
    let c = b.centre();
    let v = p - c;
    let half = (b.hi - b.lo) / 2.0;
    let k = [
        if v.x.abs() > 1e-12 {
            half.x / v.x.abs()
        } else {
            f64::MAX
        },
        if v.y.abs() > 1e-12 {
            half.y / v.y.abs()
        } else {
            f64::MAX
        },
    ]
    .into_iter()
    .fold(f64::MAX, f64::min)
    .min(1.0);
    c + v * k
}

/// What a label, tag, note or dimension reads, for reports.
pub fn label_text(item: &PlanItem) -> String {
    match item {
        PlanItem::Label { text, .. }
        | PlanItem::Tag { text, .. }
        | PlanItem::Dimension { text, .. } => text.clone(),
        PlanItem::Annotation { item, .. } => item.text.replace('\n', " "),
        _ => String::new(),
    }
}

/// Pairs of label boxes that overlap: none after [`place_labels`], where
/// there was room.
pub fn overlapping_labels(items: &[PlanItem], mm: f64) -> Vec<(usize, usize)> {
    let boxes: Vec<(usize, TextBox)> = items
        .iter()
        .enumerate()
        .filter_map(|(k, it)| text_box(it, mm).map(|(b, _)| (k, b.shrunk(0.2 * mm))))
        .collect();
    let mut out = Vec::new();
    for i in 0..boxes.len() {
        for j in i + 1..boxes.len() {
            if boxes[i].1.overlaps(&boxes[j].1) {
                out.push((boxes[i].0, boxes[j].0));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::{ElementId, ElementKind, Model};

    #[test]
    fn a_label_on_a_line_moves_off_it() {
        let id = ElementId::NIL;
        let mut items = vec![
            PlanItem::Line {
                id,
                pen: Pen::Grid,
                a: DVec2::new(-10.0, 0.0),
                b: DVec2::new(10.0, 0.0),
            },
            PlanItem::Label {
                id,
                at: DVec2::ZERO,
                text: "LOGGIA".into(),
            },
        ];
        let mm = 0.05;
        place_labels(&mut items, mm);
        let (b, _) = text_box(&items[1], mm).unwrap();
        assert!(!b.crossed_by(DVec2::new(-10.0, 0.0), DVec2::new(10.0, 0.0)));
        // close by, so no leader
        assert_eq!(items.len(), 2);
    }

    fn plans_on_sheets(m: &Model) -> Vec<ElementId> {
        let mut out = vec![];
        for e in m.iter() {
            if let ElementKind::Viewport(vp) = &e.kind {
                if matches!(
                    m.get(vp.view).map(|v| &v.kind),
                    Some(ElementKind::View(v)) if matches!(v.kind, creabim_core::ViewKind::Plan { .. })
                ) && !out.contains(&vp.view)
                {
                    out.push(vp.view);
                }
            }
        }
        out
    }

    #[test]
    fn no_two_labels_overlap_on_the_starter_house_sheets() {
        let m = creabim_core::starter::starter_house();
        let plans = plans_on_sheets(&m);
        assert_eq!(plans.len(), 4);
        for view in plans {
            let ElementKind::View(v) = &m.get(view).unwrap().kind else {
                unreachable!()
            };
            let mm = v.scale / 1000.0;
            let items = crate::section::view_drawing(&m, view).unwrap().items;
            let clash = overlapping_labels(&items, mm);
            let name = &m.get(view).unwrap().name;
            assert!(
                clash.is_empty(),
                "{name}: {:?}",
                clash
                    .iter()
                    .map(|(a, b)| (format!("{:?}", items[*a]), format!("{:?}", items[*b])))
                    .collect::<Vec<_>>()
            );
            // room names and notes keep off grid and section lines
            for it in &items {
                let PlanItem::Annotation { item, .. } = it else {
                    continue;
                };
                if !matches!(item.shape, Shape::Note { .. }) {
                    continue;
                }
                let (b, _) = text_box(it, mm).unwrap();
                for other in &items {
                    if let PlanItem::Line {
                        id,
                        pen: Pen::Grid | Pen::Symbol,
                        a,
                        b: e,
                    } = other
                    {
                        let line = matches!(
                            m.get(*id).map(|x| &x.kind),
                            Some(ElementKind::Grid(_) | ElementKind::View(_))
                        );
                        assert!(
                            !line || !b.shrunk(0.3 * mm).crossed_by(*a, *e),
                            "{name}: {:?} sits on a grid or section line",
                            item.text
                        );
                    }
                }
            }
        }
    }
}
