//! Plan drawings generated from the model: walls cut at a height, openings
//! drawn with architectural symbols, everything below shown in projection.

use crate::wall::Cut;
use creabim_core::{Curve, ElementId, ElementKind, Model, OpeningKind};
use glam::{DVec2, DVec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pen {
    /// Material cut by the plan plane: heavy line, filled.
    Cut,
    /// Visible below the cut plane: light line.
    Projection,
    /// Symbols such as door swings: light line.
    Symbol,
    /// Grid lines: dashed.
    Grid,
    /// Construction guides: shown while working, never printed.
    Guide,
    /// Above the cut plane (beams, the stair past its break): dashed.
    Hidden,
    /// Another level traced under a plan: halftone, never printed, not
    /// picked, but snapped to.
    Underlay,
}

pub use creabim_core::Hatch;

/// The hatch for an element's cut faces: its material's, or its type's default.
pub fn hatch_of(model: &Model, id: ElementId) -> Hatch {
    let Some(e) = model.get(id) else {
        return Hatch::Solid;
    };
    if let Some((_, m)) = creabim_core::material_of(model, e) {
        return m.hatch;
    }
    match &e.kind {
        ElementKind::Wall(_) => Hatch::Diagonal,
        ElementKind::Slab(_) => Hatch::Cross,
        _ => Hatch::Solid,
    }
}

/// Hatch line segments clipped to `polygon`, with a base spacing of
/// `spacing` drawing units. Stipple dots come back as very short segments.
pub fn hatch_lines(hatch: Hatch, polygon: &[DVec2], spacing: f64) -> Vec<(DVec2, DVec2)> {
    let s = spacing;
    let deg = |d: f64| d.to_radians();
    match hatch {
        Hatch::Empty | Hatch::Solid => vec![],
        Hatch::Diagonal => scan_lines(polygon, deg(45.0), s, 0.0),
        Hatch::Cross => {
            let mut v = scan_lines(polygon, deg(45.0), s * 1.4, 0.0);
            v.extend(scan_lines(polygon, deg(-45.0), s * 1.4, 0.0));
            v
        }
        Hatch::Brick => {
            let mut v = scan_lines(polygon, deg(45.0), s * 2.0, 0.0);
            v.extend(scan_lines(polygon, deg(45.0), s * 2.0, s * 0.5));
            v
        }
        Hatch::Concrete => {
            let mut v = scan_lines(polygon, deg(45.0), s * 4.0, 0.0);
            v.extend(stipple(polygon, s * 1.3));
            v
        }
        Hatch::Timber => {
            // grain runs along the polygon's longest side
            let n = polygon.len();
            let angle = (0..n)
                .map(|i| polygon[(i + 1) % n] - polygon[i])
                .max_by(|a, b| a.length().total_cmp(&b.length()))
                .map_or(0.0, |d| d.to_angle());
            scan_lines(polygon, angle, s * 1.2, s * 0.3)
        }
    }
}

/// A staggered grid of dots inside a polygon.
fn stipple(polygon: &[DVec2], spacing: f64) -> Vec<(DVec2, DVec2)> {
    if polygon.len() < 3 || spacing <= 0.0 {
        return vec![];
    }
    let (lo, hi) = polygon
        .iter()
        .fold((DVec2::MAX, DVec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
    let dot = DVec2::new(spacing * 0.12, 0.0);
    let mut out = Vec::new();
    let mut j = (lo.y / spacing).floor() as i64;
    while (j as f64) * spacing <= hi.y {
        let y = j as f64 * spacing;
        // offset alternate rows, and jitter a little so it reads as stipple
        let shift = if j % 2 == 0 { 0.0 } else { 0.5 };
        let mut i = (lo.x / spacing).floor() as i64 - 1;
        while (i as f64) * spacing <= hi.x {
            let h = ((i * 73 + j * 151).rem_euclid(97)) as f64 / 97.0 - 0.5;
            let p = DVec2::new(
                (i as f64 + shift + h * 0.4) * spacing,
                y + h * 0.3 * spacing,
            );
            if point_in(p, polygon) {
                out.push((p, p + dot));
            }
            i += 1;
        }
        j += 1;
    }
    out
}

fn point_in(p: DVec2, poly: &[DVec2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

/// Parallel lines at `angle`, `spacing` apart and shifted by `phase`,
/// clipped to a polygon (even-odd).
fn scan_lines(polygon: &[DVec2], angle: f64, spacing: f64, phase: f64) -> Vec<(DVec2, DVec2)> {
    if polygon.len() < 3 || spacing <= 0.0 {
        return vec![];
    }
    // rotate so the lines run along x; lines sit at y = k * spacing, so
    // neighbouring polygons share one continuous pattern
    let (dir, nrm) = (DVec2::from_angle(angle), DVec2::from_angle(angle).perp());
    let local: Vec<DVec2> = polygon
        .iter()
        .map(|p| DVec2::new(p.dot(dir), p.dot(nrm) - phase))
        .collect();
    let (y0, y1) = local
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
    let mut out = Vec::new();
    let mut k = (y0 / spacing).ceil() as i64;
    while (k as f64) * spacing < y1 {
        let y = k as f64 * spacing;
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..local.len() {
            let (a, b) = (local[i], local[(i + 1) % local.len()]);
            if (a.y > y) != (b.y > y) {
                xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pair in xs.chunks(2) {
            if let [xa, xb] = pair {
                let y = y + phase;
                out.push((dir * *xa + nrm * y, dir * *xb + nrm * y));
            }
        }
        k += 1;
    }
    out
}

/// Which cut wins where two overlap: higher numbers cut through lower ones.
fn cut_priority(model: &Model, id: ElementId) -> u8 {
    match model.get(id).map(|e| &e.kind) {
        Some(ElementKind::Column(_)) => 3,
        Some(ElementKind::Wall(_)) => 2,
        Some(ElementKind::Slab(_)) => 1,
        _ => 0,
    }
}

/// Elements of the same material (or, without materials, the same type)
/// merge into one mass where they touch; different ones keep a line between.
fn material_key(model: &Model, id: ElementId) -> (Option<ElementId>, u8) {
    match model.get(id) {
        Some(e) => match creabim_core::material_of(model, e) {
            Some((m, _)) => (Some(m.id), 0),
            None => (None, 1 + cut_priority(model, id)),
        },
        None => (None, 0),
    }
}

/// A piece of cut material: an element's cut, or one layer's of a layered
/// element. [`clean_up_pieces`] turns them into fills and outlines.
#[derive(Clone, Debug, PartialEq)]
pub struct CutPiece {
    pub id: ElementId,
    pub points: Vec<DVec2>,
    pub layer: Option<crate::scene::LayerPart>,
}

impl CutPiece {
    /// Which cut wins where two overlap: a wall's core cuts through
    /// another wall's finishes, walls through slabs, columns through all.
    fn priority(&self, model: &Model) -> u8 {
        let core = self.layer.is_none_or(|l| l.core);
        cut_priority(model, self.id) * 2 + u8::from(core)
    }

    /// What it merges with: the same material, or for a layer without
    /// one, layers of the same function (air gaps run on into each other).
    fn key(&self, model: &Model) -> (Option<ElementId>, u8) {
        match self.layer {
            None => material_key(model, self.id),
            Some(l) => match l.material {
                Some(m) if model.get(m).is_some() => (Some(m), 0),
                _ if l.function == creabim_core::LayerFunction::AirGap => (None, 250),
                _ => (None, 1 + cut_priority(model, self.id)),
            },
        }
    }

    fn hatch(&self, model: &Model) -> Hatch {
        match self.layer {
            None => hatch_of(model, self.id),
            Some(l) => match l.material.and_then(|m| model.get(m)).map(|e| &e.kind) {
                Some(ElementKind::Material(m)) => m.hatch,
                _ if l.function == creabim_core::LayerFunction::AirGap => Hatch::Empty,
                _ => match model.get(self.id).map(|e| &e.kind) {
                    Some(ElementKind::Wall(_)) => Hatch::Diagonal,
                    Some(ElementKind::Slab(_)) => Hatch::Cross,
                    _ => Hatch::Solid,
                },
            },
        }
    }
}

fn is_convex(poly: &[DVec2]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0.0;
    for i in 0..n {
        let (a, b, c) = (poly[i], poly[(i + 1) % n], poly[(i + 2) % n]);
        let z = (b - a).perp_dot(c - b);
        if z.abs() < 1e-12 {
            continue;
        }
        if sign == 0.0 {
            sign = z.signum();
        } else if z.signum() != sign {
            return false;
        }
    }
    sign != 0.0
}

fn signed_area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| poly[i].perp_dot(poly[(i + 1) % n]))
        .sum::<f64>()
        / 2.0
}

/// Keep the part of `poly` on the side of a → b where `keep(side)` holds,
/// side being the signed distance (left positive).
fn clip_half(poly: &[DVec2], a: DVec2, b: DVec2, left: bool) -> Vec<DVec2> {
    let d = (b - a).normalize_or_zero();
    let side = |p: DVec2| {
        let s = d.perp_dot(p - a);
        if left {
            s
        } else {
            -s
        }
    };
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let (sp, sq) = (side(p), side(q));
        if sp >= 0.0 {
            out.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            out.push(p + (q - p) * (sp / (sp - sq)));
        }
    }
    out
}

/// `subject` minus the convex polygon `hole`, as convex-ish pieces.
pub fn subtract_convex(subject: &[DVec2], hole: &[DVec2]) -> Vec<Vec<DVec2>> {
    let ccw = signed_area(hole) > 0.0;
    let n = hole.len();
    let mut rest = subject.to_vec();
    let mut pieces = Vec::new();
    for i in 0..n {
        let (a, b) = (hole[i], hole[(i + 1) % n]);
        // outside this edge of the hole is kept; inside goes on to the next edge
        let outside = clip_half(&rest, a, b, !ccw);
        if outside.len() >= 3 && signed_area(&outside).abs() > 1e-9 {
            pieces.push(outside);
        }
        rest = clip_half(&rest, a, b, ccw);
        if rest.len() < 3 {
            break;
        }
    }
    pieces
}

/// Do two polygons' bounding boxes overlap by more than a sliver?
fn boxes_overlap(a: &[DVec2], b: &[DVec2]) -> bool {
    let bb = |p: &[DVec2]| {
        p.iter().fold((DVec2::MAX, DVec2::MIN), |(lo, hi), q| {
            (lo.min(*q), hi.max(*q))
        })
    };
    let ((alo, ahi), (blo, bhi)) = (bb(a), bb(b));
    let eps = 1e-6;
    alo.x < bhi.x - eps && blo.x < ahi.x - eps && alo.y < bhi.y - eps && blo.y < ahi.y - eps
}

/// Turn cut polygons into fills plus outlines, leaving out outline segments
/// shared with a neighbouring cut so joined elements read as one mass.
pub fn clean_up_cuts(model: &Model, items: &mut Vec<PlanItem>) {
    clean_up_pieces(model, items, vec![]);
}

/// [`clean_up_cuts`], with layer pieces of layered elements besides the
/// `Pen::Cut` polygons in `items`. Each layer is filled with its own
/// material's hatch; the line between two layers of one element is a thin
/// one, and layers of the same material merge across joined walls.
pub fn clean_up_pieces(model: &Model, items: &mut Vec<PlanItem>, layers: Vec<CutPiece>) {
    let mut cuts: Vec<CutPiece> = Vec::new();
    items.retain(|it| match it {
        PlanItem::Polygon {
            id,
            pen: Pen::Cut,
            points,
        } => {
            cuts.push(CutPiece {
                id: *id,
                points: points.clone(),
                layer: None,
            });
            false
        }
        _ => true,
    });
    cuts.extend(layers);
    // where cuts overlap, the higher-priority element cuts through the
    // other: a wall runs through the end of the slab that sits in it
    let mut polys: Vec<CutPiece> = Vec::new();
    for c in &cuts {
        let mut pieces = vec![c.points.clone()];
        let mine = c.priority(model);
        for other in &cuts {
            if other.priority(model) <= mine || other.id == c.id || !is_convex(&other.points) {
                continue;
            }
            let hole = &other.points;
            pieces = pieces
                .into_iter()
                .flat_map(|p| {
                    if boxes_overlap(&p, hole) {
                        subtract_convex(&p, hole)
                    } else {
                        vec![p]
                    }
                })
                .collect();
        }
        polys.extend(pieces.into_iter().map(|p| CutPiece {
            points: p,
            ..c.clone()
        }));
    }
    let keys: Vec<(Option<ElementId>, u8)> = polys.iter().map(|p| p.key(model)).collect();
    for p in &polys {
        items.push(PlanItem::Fill {
            id: p.id,
            points: p.points.clone(),
            hatch: p.hatch(model),
        });
    }
    let edges = |pts: &Vec<DVec2>| -> Vec<(DVec2, DVec2)> {
        (0..pts.len())
            .map(|i| (pts[i], pts[(i + 1) % pts.len()]))
            .collect()
    };
    let eps = 1e-4;
    for (i, piece) in polys.iter().enumerate() {
        for (a, b) in edges(&piece.points) {
            let len = a.distance(b);
            if len < eps {
                continue;
            }
            let d = (b - a) / len;
            let on_line = |p: DVec2| (p - a).perp_dot(d).abs() < eps;
            // parts of this edge covered by another cut's edge: merged away
            // where it is the same material, a thin line between two layers
            // of one element (drawn by the first of the two)
            let mut covered: Vec<(f64, f64)> = Vec::new();
            let mut thin: Vec<(f64, f64, bool)> = Vec::new();
            for (j, other) in polys.iter().enumerate() {
                let same = keys[i] == keys[j];
                if i == j || (!same && other.id != piece.id) {
                    continue;
                }
                for (p, q) in edges(&other.points) {
                    if on_line(p) && on_line(q) {
                        let (tp, tq) = ((p - a).dot(d), (q - a).dot(d));
                        let (lo, hi) = (tp.min(tq).max(0.0), tp.max(tq).min(len));
                        if hi > lo + eps {
                            if same {
                                covered.push((lo, hi));
                            } else {
                                thin.push((lo, hi, i < j));
                            }
                        }
                    }
                }
            }
            // split the edge where any of that changes, and draw each part
            let mut ts: Vec<f64> = vec![0.0, len];
            for (lo, hi) in &covered {
                ts.extend([*lo, *hi]);
            }
            for (lo, hi, _) in &thin {
                ts.extend([*lo, *hi]);
            }
            ts.sort_by(|x, y| x.partial_cmp(y).unwrap());
            ts.dedup_by(|x, y| (*x - *y).abs() < eps);
            let mut run: Option<(f64, f64, Pen)> = None;
            let flush = |run: &mut Option<(f64, f64, Pen)>, items: &mut Vec<PlanItem>| {
                if let Some((t0, t1, pen)) = run.take() {
                    items.push(PlanItem::Line {
                        id: piece.id,
                        pen,
                        a: a + d * t0,
                        b: a + d * t1,
                    });
                }
            };
            for w in ts.windows(2) {
                let (t0, t1) = (w[0], w[1]);
                let m = (t0 + t1) / 2.0;
                let inside = |lo: f64, hi: f64| lo <= m && m <= hi;
                let pen = if covered.iter().any(|(lo, hi)| inside(*lo, *hi)) {
                    None
                } else {
                    let mine: Vec<bool> = thin
                        .iter()
                        .filter(|(lo, hi, _)| inside(*lo, *hi))
                        .map(|t| t.2)
                        .collect();
                    if mine.is_empty() {
                        Some(Pen::Cut)
                    } else if mine.iter().any(|f| *f) {
                        Some(Pen::Projection)
                    } else {
                        None
                    }
                };
                match (pen, &mut run) {
                    (Some(p), Some((_, end, rp))) if *rp == p && (*end - t0).abs() < eps => {
                        *end = t1
                    }
                    (Some(p), _) => {
                        flush(&mut run, items);
                        run = Some((t0, t1, p));
                    }
                    (None, _) => flush(&mut run, items),
                }
            }
            flush(&mut run, items);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlanItem {
    Polygon {
        id: ElementId,
        pen: Pen,
        points: Vec<DVec2>,
    },
    /// Fill of cut material, with no outline (outlines are `Pen::Cut` lines).
    Fill {
        id: ElementId,
        points: Vec<DVec2>,
        hatch: Hatch,
    },
    Line {
        id: ElementId,
        pen: Pen,
        a: DVec2,
        b: DVec2,
    },
    /// Counter-clockwise arc from `start` to `end` angle, radians.
    Arc {
        id: ElementId,
        pen: Pen,
        center: DVec2,
        radius: f64,
        start: f64,
        end: f64,
    },
    Label {
        id: ElementId,
        at: DVec2,
        text: String,
    },
    /// Text at a fixed paper size, `size` mm tall.
    Text {
        id: ElementId,
        at: DVec2,
        text: String,
        size: f64,
        align: crate::paper::Anchor,
    },
    /// A dimension from `a` to `b`, its line `offset` to the left. A
    /// horizontal or vertical one measures from `a` to `b` square across,
    /// so `b` is where the true second point `witness[1]` lands on that
    /// line, and its witness lines start from `witness`.
    Dimension {
        id: ElementId,
        a: DVec2,
        b: DVec2,
        offset: f64,
        text: String,
        look: crate::annotation::TextLook,
        witness: Option<[DVec2; 2]>,
    },
    /// A tag: `text` in a bubble at `at`, with a leader back to `anchor`.
    Tag {
        id: ElementId,
        anchor: DVec2,
        at: DVec2,
        text: String,
        round: bool,
        look: crate::annotation::TextLook,
    },
    /// An angular, radial, diameter or arc-length dimension, or a note.
    Annotation {
        id: ElementId,
        item: Box<crate::annotation::Annotation>,
    },
    /// Shaded surfaces as an image spanning `lo`..`hi`, drawn only inside `clip`.
    Image {
        id: ElementId,
        raster: std::sync::Arc<crate::raster::Raster>,
        lo: DVec2,
        hi: DVec2,
        clip: Option<(DVec2, DVec2)>,
    },
}

/// Is a label short enough to sit in a bubble, as grid and section marks
/// do (level datums and longer text are plain)?
pub fn is_bubble(text: &str) -> bool {
    !text.contains(' ') && (1..=5).contains(&text.chars().count())
}

/// A bubble's radius for `text`, in units of the bubble for one or two
/// characters.
pub fn bubble_scale(text: &str) -> f64 {
    (text.chars().count() as f64 * 0.38).max(1.0)
}

impl PlanItem {
    pub fn id(&self) -> ElementId {
        match self {
            PlanItem::Polygon { id, .. }
            | PlanItem::Fill { id, .. }
            | PlanItem::Line { id, .. }
            | PlanItem::Arc { id, .. }
            | PlanItem::Label { id, .. }
            | PlanItem::Text { id, .. }
            | PlanItem::Dimension { id, .. }
            | PlanItem::Tag { id, .. }
            | PlanItem::Annotation { id, .. }
            | PlanItem::Image { id, .. } => *id,
        }
    }

    /// Corners of what the item covers, in drawing units (none for guides,
    /// which run on without end).
    pub fn extent(&self) -> Vec<DVec2> {
        let mut pts = Vec::new();
        match self {
            PlanItem::Polygon { points, .. } | PlanItem::Fill { points, .. } => {
                pts.extend(points.iter().copied())
            }
            PlanItem::Line {
                pen: Pen::Guide, ..
            } => {}
            PlanItem::Line { a, b, .. } => pts.extend([*a, *b]),
            PlanItem::Arc { center, radius, .. } => pts.extend([
                *center - DVec2::splat(*radius),
                *center + DVec2::splat(*radius),
            ]),
            PlanItem::Label { at, text, .. } => {
                // rough text extent at 1:100 (3 mm type)
                let half = DVec2::new(text.chars().count() as f64 * 0.09 + 0.2, 0.25);
                pts.extend([*at - half, *at + half]);
            }
            PlanItem::Text {
                at,
                text,
                size,
                align,
                ..
            } => {
                // text items are in paper units already (schedules)
                let w = crate::paper::text_width(text, *size);
                let x0 = match align {
                    crate::paper::Anchor::Start => at.x,
                    crate::paper::Anchor::Middle => at.x - w / 2.0,
                    crate::paper::Anchor::End => at.x - w,
                };
                pts.extend([
                    DVec2::new(x0, at.y - size / 2.0),
                    DVec2::new(x0 + w, at.y + size / 2.0),
                ]);
            }
            PlanItem::Dimension { a, b, offset, .. } => {
                let n = (*b - *a).normalize_or_zero().perp() * *offset;
                pts.extend([*a, *b, *a + n, *b + n]);
            }
            PlanItem::Tag { anchor, at, .. } => {
                pts.extend([*anchor, *at - DVec2::splat(0.3), *at + DVec2::splat(0.3)])
            }
            PlanItem::Annotation { item, .. } => pts.extend(item.extent()),
            PlanItem::Image { lo, hi, clip, .. } => match clip {
                Some((clo, chi)) => pts.extend([lo.max(*clo), hi.min(*chi)]),
                None => pts.extend([*lo, *hi]),
            },
        }
        pts
    }

    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let pts = self.extent();
        let first = *pts.first()?;
        Some(
            pts.iter()
                .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))),
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanView {
    pub items: Vec<PlanItem>,
}

impl PlanView {
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let pts: Vec<DVec2> = self.items.iter().flat_map(|it| it.extent()).collect();
        let first = *pts.first()?;
        Some(
            pts.iter()
                .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))),
        )
    }

    /// Render as SVG at a drawing scale, e.g. `100.0` for 1:100. Units are millimetres on paper.
    pub fn to_svg(&self, scale: f64) -> String {
        let (lo, hi) = self.bounds().unwrap_or((DVec2::ZERO, DVec2::ONE));
        let margin = DVec2::splat(1.0);
        let (lo, hi) = (lo - margin, hi + margin);
        let map = crate::paper::PaperMap {
            model: lo,
            paper: DVec2::ZERO,
            scale,
        };
        let mut svg = crate::paper::SvgCanvas::new((hi - lo) * map.k());
        crate::paper::draw(&mut svg, self, &map);
        svg.finish()
    }
}

/// Generate the plan of one level, cut `cut_height` metres above it.
/// A sketch's lines and arcs in a drawing. `project` takes a point in the
/// sketch's model coordinates (relative to its level) to drawing x and y,
/// plus depth. Arcs on a plane facing the drawing stay arcs; seen at an
/// angle they are drawn in short pieces.
pub fn sketch_items(
    id: ElementId,
    sk: &creabim_core::Sketch,
    project: &dyn Fn(DVec3) -> DVec3,
) -> Vec<PlanItem> {
    let at = |p: DVec2| project(sk.plane.to_model(p)).truncate();
    let o = at(DVec2::ZERO);
    let (x, y) = (at(DVec2::X) - o, at(DVec2::Y) - o);
    let facing =
        (x.length() - 1.0).abs() < 1e-6 && (y.length() - 1.0).abs() < 1e-6 && x.dot(y).abs() < 1e-6;
    let mut out = Vec::new();
    for c in sk.curves() {
        match c {
            Curve::Arc {
                center,
                radius,
                start,
                sweep,
            } if facing => {
                // seen from behind, the arc turns the other way
                let flip = x.perp_dot(y) < 0.0;
                let a0 = if flip {
                    x.to_angle() - start
                } else {
                    x.to_angle() + start
                };
                let sweep = if flip { -sweep } else { sweep };
                let (start, end) = if sweep < 0.0 {
                    (a0 + sweep, a0)
                } else {
                    (a0, a0 + sweep)
                };
                out.push(PlanItem::Arc {
                    id,
                    pen: Pen::Symbol,
                    center: at(center),
                    radius,
                    start,
                    end,
                });
            }
            _ => {
                for w in c.points().windows(2) {
                    let (a, b) = (at(w[0]), at(w[1]));
                    if a.distance(b) > 1e-6 {
                        out.push(PlanItem::Line {
                            id,
                            pen: Pen::Symbol,
                            a,
                            b,
                        });
                    }
                }
            }
        }
    }
    out
}

pub fn plan_view(model: &Model, level: ElementId, cut_height: f64) -> PlanView {
    let mut items = Vec::new();
    let joins = creabim_core::wall_joins(model);
    let cuts = crate::scene::cuts(model);
    let cutters: std::collections::HashSet<ElementId> = cuts.iter().map(|c| c.1).collect();
    let targets: std::collections::HashSet<ElementId> = cuts.iter().map(|c| c.0).collect();
    // elements whose plan comes from slicing their final 3D shape
    let mut sliced: Vec<ElementId> = Vec::new();
    // the layers of layered walls, cut
    let mut layers: Vec<CutPiece> = Vec::new();
    for e in model.iter() {
        if cutters.contains(&e.id) {
            continue;
        }
        let on_level = match &e.kind {
            ElementKind::Wall(w) => w.level == level,
            ElementKind::Slab(s) => s.level == level,
            ElementKind::FreeMesh(f) => f.level == Some(level),
            k => creabim_core::members::level_of(k) == Some(level),
        };
        if on_level && (targets.contains(&e.id) || matches!(e.kind, ElementKind::FreeMesh(_))) {
            sliced.push(e.id);
            if !matches!(e.kind, ElementKind::Wall(_)) {
                continue;
            }
        }
        match &e.kind {
            ElementKind::Grid(g) => {
                items.push(PlanItem::Line {
                    id: e.id,
                    pen: Pen::Grid,
                    a: g.start,
                    b: g.end,
                });
                let dir = (g.end - g.start).normalize_or_zero();
                items.push(PlanItem::Label {
                    id: e.id,
                    at: g.end + dir * 0.6,
                    text: creabim_core::grids::display_label(model, g),
                });
            }
            ElementKind::Wall(w) if w.level == level => {
                let d = w.direction();
                let n = DVec2::new(-d.y, d.x);
                let t = w.thickness / 2.0;
                let ends = joins.get(&e.id).cloned().unwrap_or_default();
                // wall-local point; the wall's own ends and the sides of the
                // gaps crossing walls leave follow its joins
                let at = |u: f64, s: f64| w.start + d * ends.place(w, u, s, false) + n * s;
                let cut_v = cut_height - w.base_offset;
                if cut_v <= 0.0 || cut_v >= w.height {
                    // wall entirely below or above the cut: show its outline in projection
                    if cut_v >= w.height {
                        items.push(PlanItem::Polygon {
                            id: e.id,
                            pen: Pen::Projection,
                            points: ends.footprint(w).to_vec(),
                        });
                    }
                    continue;
                }
                if let Some(g) = &w.curtain {
                    // mullions and panels, and the doors in them
                    let mut hosted: Vec<(ElementId, &creabim_core::Opening)> = vec![];
                    for oid in model.dependents(e.id) {
                        if let Some(ElementKind::Opening(o)) = model.get(oid).map(|x| &x.kind) {
                            hosted.push((oid, o));
                        }
                    }
                    let openings: Vec<&creabim_core::Opening> =
                        hosted.iter().map(|h| h.1).collect();
                    items.extend(crate::curtain::curtain_plan(e.id, w, g, &openings, cut_v));
                    let d = w.direction();
                    let n = DVec2::new(-d.y, d.x);
                    let plain = |u: f64, s: f64| w.start + d * u + n * s;
                    for (oid, o) in hosted {
                        if let Some(c) = Cut::of(o, w) {
                            if cut_v > c.v0 && cut_v < c.v1 {
                                items.extend(opening_symbol(oid, o, w, &c, cut_v, &plain));
                            }
                        }
                    }
                    continue;
                }
                let mut gaps: Vec<(f64, f64)> = Vec::new();
                for oid in model.dependents(e.id) {
                    let Some(oe) = model.get(oid) else { continue };
                    let ElementKind::Opening(o) = &oe.kind else {
                        continue;
                    };
                    let Some(c) = Cut::of(o, w) else { continue };
                    if cut_v > c.v0 && cut_v < c.v1 {
                        gaps.push((c.u0, c.u1));
                        items.extend(opening_symbol(oid, o, w, &c, cut_v, &at));
                    }
                }
                gaps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                // the stretches between crossing walls, less the openings
                let mut pieces = Vec::new();
                for (lo, hi) in ends.solid_spans(w) {
                    let mut u = lo;
                    for &(g0, g1) in &gaps {
                        if g1 <= u || g0 >= hi {
                            continue;
                        }
                        if g0 > u + 1e-9 {
                            pieces.push((u, g0));
                        }
                        u = u.max(g1);
                    }
                    if hi > u + 1e-9 {
                        pieces.push((u, hi));
                    }
                }
                if let Some((_, asm)) = creabim_core::assembly_of(model, e) {
                    // each layer between its own ends: a core runs on at a T
                    for band in creabim_core::assembly::wall_bands(asm, w) {
                        let at =
                            |u: f64, s: f64| w.start + d * ends.place(w, u, s, band.core) + n * s;
                        for &(a, b) in &pieces {
                            layers.push(CutPiece {
                                id: e.id,
                                points: vec![
                                    at(a, band.s0),
                                    at(b, band.s0),
                                    at(b, band.s1),
                                    at(a, band.s1),
                                ],
                                layer: Some(crate::scene::LayerPart {
                                    index: band.index,
                                    material: band.material,
                                    function: band.function,
                                    core: band.core,
                                }),
                            });
                        }
                    }
                    continue;
                }
                for (a, b) in pieces {
                    items.push(PlanItem::Polygon {
                        id: e.id,
                        pen: Pen::Cut,
                        points: vec![at(a, -t), at(b, -t), at(b, t), at(a, t)],
                    });
                }
            }
            ElementKind::Slab(s) if s.level == level => {
                // cut when the plan plane passes through it, else seen below
                let bottom = s.top_offset - s.thickness;
                let cut = bottom < cut_height && cut_height < s.top_offset;
                let holes = creabim_core::slab::holes_of(model, e);
                if cut {
                    // one outline with its holes bridged in, so the fill leaves them open
                    items.insert(
                        0,
                        PlanItem::Polygon {
                            id: e.id,
                            pen: Pen::Cut,
                            points: creabim_core::polygon::bridged(&s.outline(), &holes),
                        },
                    );
                } else {
                    items.insert(
                        0,
                        PlanItem::Polygon {
                            id: e.id,
                            pen: Pen::Projection,
                            points: s.outline(),
                        },
                    );
                    for h in &holes {
                        items.extend(hole_marks(e.id, h, Pen::Projection));
                    }
                }
            }
            ElementKind::Object(o) if o.level == level => {
                items.extend(crate::objects::object_plan(model, e, cut_height));
            }
            k if on_level && creabim_core::members::is_member(k) => {
                items.extend(crate::members::member_plan(model, e, cut_height));
            }
            ElementKind::Shaft(sh) => {
                // on every plan it passes through, crossed, to pick it by
                let z = model.level(level).map_or(0.0, |l| l.elevation);
                let (z0, z1) = sh.span(model);
                if z0 - 1e-6 <= z && z < z1 - 1e-6 {
                    items.extend(hole_marks(e.id, &sh.boundary, Pen::Symbol));
                }
            }
            ElementKind::Sketch(sk) if sk.level == Some(level) => {
                items.extend(sketch_items(e.id, sk, &|p| p));
            }
            // guides lying flat; one standing on a wall face shows in sections
            ElementKind::Guide(g)
                if g.level.is_none_or(|l| l == level) && (g.a.z - g.b.z).abs() < 1e-3 =>
            {
                if let Some((a, b)) = guide_span(g.a.truncate(), g.b.truncate()) {
                    items.push(PlanItem::Line {
                        id: e.id,
                        pen: Pen::Guide,
                        a,
                        b,
                    });
                }
            }
            ElementKind::View(v) => {
                if let creabim_core::ViewKind::Section {
                    start, end, cut, ..
                } = &v.kind
                {
                    section_marker(&mut items, e.id, &e.name, *start, *end, *cut);
                }
            }
            _ => {}
        }
    }
    // a cut wall keeps its opening symbols but takes its outline from the slice
    items.retain(|it| !sliced.contains(&it.id()) || matches!(it, PlanItem::Label { .. }));
    layers.retain(|p| !sliced.contains(&p.id));
    clean_up_pieces(model, &mut items, layers);
    if !sliced.is_empty() {
        let z = model.level(level).map_or(0.0, |l| l.elevation) + cut_height;
        let scene = crate::scene::build_scene(model);
        for item in scene.iter().filter(|i| sliced.contains(&i.id)) {
            sliced_plan(model, item, z, &mut items);
        }
    }
    PlanView { items }
}

/// The plan of `level` as an underlay: its linework (cut outlines,
/// projection, symbols) as halftone lines and arcs, without fills, text,
/// grids, guides or view marks.
pub fn underlay_items(model: &Model, level: ElementId, cut_height: f64) -> Vec<PlanItem> {
    let skip = |id: ElementId| {
        matches!(
            model.get(id).map(|e| &e.kind),
            Some(ElementKind::View(_) | ElementKind::Grid(_) | ElementKind::GridGroup(_))
        )
    };
    let mut out = Vec::new();
    for it in plan_view(model, level, cut_height).items {
        match it {
            PlanItem::Line { id, pen, a, b } if pen != Pen::Guide && pen != Pen::Grid => {
                if !skip(id) {
                    out.push(PlanItem::Line {
                        id,
                        pen: Pen::Underlay,
                        a,
                        b,
                    });
                }
            }
            PlanItem::Polygon { id, points, .. } if !skip(id) => {
                let n = points.len();
                for i in 0..n {
                    out.push(PlanItem::Line {
                        id,
                        pen: Pen::Underlay,
                        a: points[i],
                        b: points[(i + 1) % n],
                    });
                }
            }
            PlanItem::Arc {
                id,
                center,
                radius,
                start,
                end,
                ..
            } if !skip(id) => out.push(PlanItem::Arc {
                id,
                pen: Pen::Underlay,
                center,
                radius,
                start,
                end,
            }),
            _ => {}
        }
    }
    out
}

/// A hole through a slab, roof or shaft as plans draw it: its outline
/// and a cross corner to corner.
pub fn hole_marks(id: ElementId, hole: &[DVec2], pen: Pen) -> Vec<PlanItem> {
    let mut out = vec![PlanItem::Polygon {
        id,
        pen,
        points: hole.to_vec(),
    }];
    let n = hole.len();
    if n >= 4 {
        // the two diagonals of its box, kept inside it
        let lo = hole.iter().copied().reduce(DVec2::min).unwrap_or_default();
        let hi = hole.iter().copied().reduce(DVec2::max).unwrap_or_default();
        let far = |p: DVec2| {
            hole.iter()
                .copied()
                .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
                .unwrap_or(p)
        };
        for (a, b) in [(lo, hi), (DVec2::new(lo.x, hi.y), DVec2::new(hi.x, lo.y))] {
            out.push(PlanItem::Line {
                id,
                pen: Pen::Symbol,
                a: far(a),
                b: far(b),
            });
        }
    }
    out
}

/// Is this the linework of an underlay (drawn faintly, never picked)?
/// How far inside another element's cut a line must run to count as
/// hidden by it, so lines along its edge (a slab under a wall) stay as
/// they are.
const COVER_MARGIN: f64 = 0.005;

/// Lines of elements lying under another element's cut fill, drawn again
/// dashed on top: what a view showing hidden elements adds, e.g. a beam
/// inside a wall or a stud wall in a cavity. Doors and windows, which sit
/// in their walls' holes, are left as they are.
pub fn reveal_covered(model: &Model, items: &mut Vec<PlanItem>) {
    let fills: Vec<(ElementId, &[DVec2], DVec2, DVec2)> = items
        .iter()
        .filter_map(|it| match it {
            PlanItem::Fill { id, points, .. } if points.len() >= 3 => {
                let lo = points.iter().copied().reduce(DVec2::min)?;
                let hi = points.iter().copied().reduce(DVec2::max)?;
                Some((*id, points.as_slice(), lo, hi))
            }
            _ => None,
        })
        .collect();
    if fills.is_empty() {
        return;
    }
    let mut segments: Vec<(ElementId, DVec2, DVec2)> = Vec::new();
    for it in items.iter() {
        match it {
            PlanItem::Line {
                id,
                pen: Pen::Projection | Pen::Hidden,
                a,
                b,
            } => segments.push((*id, *a, *b)),
            PlanItem::Polygon {
                id,
                pen: Pen::Projection,
                points,
            } => {
                let n = points.len();
                segments.extend((0..n).map(|i| (*id, points[i], points[(i + 1) % n])));
            }
            _ => {}
        }
    }
    let opening = |id: ElementId| {
        matches!(
            model.get(id).map(|e| &e.kind),
            Some(ElementKind::Opening(_))
        )
    };
    let covered = |own: ElementId, p: DVec2| {
        fills.iter().any(|(id, poly, lo, hi)| {
            *id != own
                && p.cmpge(*lo).all()
                && p.cmple(*hi).all()
                && point_in(p, poly)
                && edge_distance(p, poly) > COVER_MARGIN
        })
    };
    let mut out = Vec::new();
    for (id, a, b) in segments {
        if opening(id) {
            continue;
        }
        let steps = (a.distance(b) / 0.01).ceil().clamp(4.0, 400.0) as usize;
        let mut start: Option<f64> = None;
        let mut last = 0.0;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let inside = covered(id, a + (b - a) * t);
            match (inside, start) {
                (true, None) => start = Some(t),
                (false, Some(s)) => {
                    out.push((id, s, last, a, b));
                    start = None;
                }
                _ => {}
            }
            if inside {
                last = t;
            }
        }
        if let Some(s) = start {
            out.push((id, s, 1.0, a, b));
        }
    }
    items.extend(
        out.into_iter()
            .filter(|(_, s, e, ..)| e > s)
            .map(|(id, s, e, a, b)| PlanItem::Line {
                id,
                pen: Pen::Hidden,
                a: a + (b - a) * s,
                b: a + (b - a) * e,
            }),
    );
}

/// How far `p` is from the nearest edge of `poly`.
fn edge_distance(p: DVec2, poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
            p.distance(a + ab * t)
        })
        .fold(f64::INFINITY, f64::min)
}

pub fn is_underlay(item: &PlanItem) -> bool {
    matches!(
        item,
        PlanItem::Line {
            pen: Pen::Underlay,
            ..
        } | PlanItem::Arc {
            pen: Pen::Underlay,
            ..
        }
    )
}

/// How far guides reach each way from their points: endless for any
/// building, short enough to stay well inside float precision.
pub const GUIDE_REACH: f64 = 200.0;

/// A guide through `a` and `b` as a long segment.
pub fn guide_span(a: DVec2, b: DVec2) -> Option<(DVec2, DVec2)> {
    let d = (b - a).try_normalize()?;
    let m = (a + b) / 2.0;
    Some((m - d * GUIDE_REACH, m + d * GUIDE_REACH))
}

/// The plan of a shape with no simple footprint: its section at `z`,
/// filled, and its edges below `z` in projection.
/// A door or window in plan, where the plan cuts the wall at `cut_v` above
/// its base: the frame members the cut passes through, and for a window
/// the sill member below, the wall faces and the glass line; for a door
/// its leaf drawn open and the swing it sweeps. `at(u, s)` is a point
/// `u` along the wall and `s` across it.
fn opening_symbol(
    id: ElementId,
    o: &creabim_core::Opening,
    w: &creabim_core::Wall,
    c: &Cut,
    cut_v: f64,
    at: &dyn Fn(f64, f64) -> DVec2,
) -> Vec<PlanItem> {
    use crate::opening::{opening_boxes, Fitted, Part};
    let mut out = Vec::new();
    let Some(f) = Fitted::of(o, w) else {
        return out;
    };
    let z = cut_v - c.v0;
    let line = |out: &mut Vec<PlanItem>, a: DVec2, b: DVec2| {
        out.push(PlanItem::Line {
            id,
            pen: Pen::Symbol,
            a,
            b,
        })
    };
    let rect = |out: &mut Vec<PlanItem>, u0: f64, u1: f64, s0: f64, s1: f64| {
        out.push(PlanItem::Polygon {
            id,
            pen: Pen::Symbol,
            points: vec![at(u0, s0), at(u1, s0), at(u1, s1), at(u0, s1)],
        })
    };
    let boxes = opening_boxes(o, w);
    for b in &boxes {
        let (u0, u1) = (c.u0 + b.lo.x, c.u0 + b.hi.x);
        let cut = b.lo.z < z && z < b.hi.z;
        match b.part {
            // the sill member shows below the cut
            Part::Frame if cut || (o.kind == OpeningKind::Window && b.lo.z <= 1e-9) => {
                rect(&mut out, u0, u1, b.lo.y, b.hi.y)
            }
            Part::Glass if cut => {
                let s = (b.lo.y + b.hi.y) / 2.0;
                line(&mut out, at(u0, s), at(u1, s));
            }
            _ => {}
        }
    }
    match o.kind {
        // a bare hole is just the gap in the wall
        OpeningKind::Hole => {}
        OpeningKind::Window => {
            // the wall faces carry on across, over the sill
            let t = w.thickness / 2.0;
            for s in [-t, t] {
                line(&mut out, at(c.u0, s), at(c.u1, s));
            }
        }
        OpeningKind::Door => {
            let Some(leaf) = boxes.iter().find(|b| b.part == Part::Leaf) else {
                return out;
            };
            // the leaf turned open about its hinge on the face of the frame
            // on the side it opens to, and the quarter circle it sweeps
            let left = o.faces_left(w);
            let side = if left { 1.0 } else { -1.0 };
            let s = if left { leaf.hi.y } else { leaf.lo.y };
            let r = leaf.hi.x - leaf.lo.x;
            let (u, towards) = if o.flip_hand {
                (c.u0 + leaf.hi.x, -1.0)
            } else {
                (c.u0 + leaf.lo.x, 1.0)
            };
            rect(&mut out, u, u + towards * f.leaf(), s, s + side * r);
            let hinge = at(u, s);
            let closed = at(u + towards, s) - hinge;
            let open = at(u, s + side) - hinge;
            // arcs run anticlockwise: from whichever of the two comes first
            let a0 = if closed.perp_dot(open) > 0.0 {
                closed.to_angle()
            } else {
                open.to_angle()
            };
            out.push(PlanItem::Arc {
                id,
                pen: Pen::Symbol,
                center: hinge,
                radius: r,
                start: a0,
                end: a0 + std::f64::consts::FRAC_PI_2,
            });
        }
    }
    out
}

fn sliced_plan(model: &Model, item: &crate::scene::SceneItem, z: f64, items: &mut Vec<PlanItem>) {
    let mut cut: Vec<(DVec2, DVec2)> = Vec::new();
    for lp in crate::slice::slice(&item.mesh, z) {
        let n = lp.len();
        for i in 0..n {
            cut.push((lp[i], lp[(i + 1) % n]));
            items.push(PlanItem::Line {
                id: item.id,
                pen: Pen::Cut,
                a: lp[i],
                b: lp[(i + 1) % n],
            });
        }
        let hatch = CutPiece {
            id: item.id,
            points: vec![],
            layer: item.layer,
        }
        .hatch(model);
        items.push(PlanItem::Fill {
            id: item.id,
            points: lp,
            hatch,
        });
    }
    for (a, b) in crate::section::feature_edges(item) {
        let (a, b) = if a.z <= b.z { (a, b) } else { (b, a) };
        if a.z >= z - 1e-6 {
            continue;
        }
        // keep the part below the cut
        let b = if b.z > z {
            a + (b - a) * ((z - a.z) / (b.z - a.z))
        } else {
            b
        };
        // edges seen end-on, and those lying in the cut, add nothing
        let (a, b) = (a.truncate(), b.truncate());
        if a.distance(b) < 1e-6 {
            continue;
        }
        // edges hidden under the heavy cut line
        let on = |p: DVec2, (s, e): (DVec2, DVec2)| {
            let d = e - s;
            let t = ((p - s).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
            p.distance(s + d * t) < 1e-4
        };
        if cut.iter().any(|&c| on(a, c) && on(b, c)) {
            continue;
        }
        items.push(PlanItem::Line {
            id: item.id,
            pen: Pen::Projection,
            a,
            b,
        });
    }
}

/// Section line with view arrows, or an elevation marker, as drawn in plan.
fn section_marker(
    items: &mut Vec<PlanItem>,
    id: ElementId,
    name: &str,
    a: DVec2,
    b: DVec2,
    cut: bool,
) {
    let along = (b - a).normalize_or_zero();
    let look = along.perp();
    let arrow = |at: DVec2| PlanItem::Polygon {
        id,
        pen: Pen::Symbol,
        points: vec![at - along * 0.35, at + along * 0.35, at + look * 0.45],
    };
    let tag = name.rsplit(' ').next().unwrap_or(name).to_string();
    if cut {
        items.push(PlanItem::Line {
            id,
            pen: Pen::Symbol,
            a,
            b,
        });
        for end in [a, b] {
            items.push(arrow(end));
        }
        items.push(PlanItem::Label {
            id,
            at: a - along * 0.7,
            text: tag.clone(),
        });
        items.push(PlanItem::Label {
            id,
            at: b + along * 0.7,
            text: tag,
        });
    } else {
        let mid = (a + b) / 2.0;
        items.push(arrow(mid));
        items.push(PlanItem::Label {
            id,
            at: mid - look * 0.6,
            text: tag,
        });
    }
}
