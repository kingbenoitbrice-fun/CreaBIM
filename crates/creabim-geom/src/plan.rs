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
    let mut cuts: Vec<(ElementId, Vec<DVec2>)> = Vec::new();
    items.retain(|it| match it {
        PlanItem::Polygon {
            id,
            pen: Pen::Cut,
            points,
        } => {
            cuts.push((*id, points.clone()));
            false
        }
        _ => true,
    });
    // where cuts overlap, the higher-priority element cuts through the
    // other: a wall runs through the end of the slab that sits in it
    let mut polys: Vec<(ElementId, Vec<DVec2>)> = Vec::new();
    for (id, pts) in &cuts {
        let mut pieces = vec![pts.clone()];
        let mine = cut_priority(model, *id);
        for (other, hole) in &cuts {
            if cut_priority(model, *other) <= mine || !is_convex(hole) {
                continue;
            }
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
        polys.extend(pieces.into_iter().map(|p| (*id, p)));
    }
    for (id, pts) in &polys {
        items.push(PlanItem::Fill {
            id: *id,
            points: pts.clone(),
            hatch: hatch_of(model, *id),
        });
    }
    let edges = |pts: &Vec<DVec2>| -> Vec<(DVec2, DVec2)> {
        (0..pts.len())
            .map(|i| (pts[i], pts[(i + 1) % pts.len()]))
            .collect()
    };
    let eps = 1e-4;
    for (i, (id, pts)) in polys.iter().enumerate() {
        for (a, b) in edges(pts) {
            let len = a.distance(b);
            if len < eps {
                continue;
            }
            let d = (b - a) / len;
            let on_line = |p: DVec2| (p - a).perp_dot(d).abs() < eps;
            // parts of this edge covered by another cut's edge
            let mut covered: Vec<(f64, f64)> = Vec::new();
            for (j, (other_id, other)) in polys.iter().enumerate() {
                if i == j || material_key(model, *id) != material_key(model, *other_id) {
                    continue;
                }
                for (p, q) in edges(other) {
                    if on_line(p) && on_line(q) {
                        let (tp, tq) = ((p - a).dot(d), (q - a).dot(d));
                        let (lo, hi) = (tp.min(tq).max(0.0), tp.max(tq).min(len));
                        if hi > lo + eps {
                            covered.push((lo, hi));
                        }
                    }
                }
            }
            covered.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
            let mut t = 0.0;
            for (lo, hi) in covered.into_iter().chain([(len, len)]) {
                if lo > t + eps {
                    items.push(PlanItem::Line {
                        id: *id,
                        pen: Pen::Cut,
                        a: a + d * t,
                        b: a + d * lo,
                    });
                }
                t = t.max(hi);
            }
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
    /// An aligned dimension from `a` to `b`, its line `offset` to the left.
    Dimension {
        id: ElementId,
        a: DVec2,
        b: DVec2,
        offset: f64,
        text: String,
    },
    /// A tag: `text` in a bubble at `at`, with a leader back to `anchor`.
    Tag {
        id: ElementId,
        anchor: DVec2,
        at: DVec2,
        text: String,
        round: bool,
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
            | PlanItem::Image { id, .. } => *id,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanView {
    pub items: Vec<PlanItem>,
}

impl PlanView {
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let mut pts = Vec::new();
        for it in &self.items {
            match it {
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
                PlanItem::Image { lo, hi, clip, .. } => match clip {
                    Some((clo, chi)) => pts.extend([lo.max(*clo), hi.min(*chi)]),
                    None => pts.extend([*lo, *hi]),
                },
            }
        }
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
    for e in model.iter() {
        if cutters.contains(&e.id) {
            continue;
        }
        let on_level = match &e.kind {
            ElementKind::Wall(w) => w.level == level,
            ElementKind::Slab(s) => s.level == level,
            ElementKind::FreeMesh(f) => f.level == Some(level),
            _ => false,
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
                    text: g.label.clone(),
                });
            }
            ElementKind::Wall(w) if w.level == level => {
                let d = w.direction();
                let n = DVec2::new(-d.y, d.x);
                let t = w.thickness / 2.0;
                let ends = joins.get(&e.id).copied().unwrap_or_default();
                let len = w.length();
                // wall-local point; the wall's own ends follow its joins
                let at = |u: f64, s: f64| {
                    let u = if u <= 0.0 {
                        ends.start_u(s)
                    } else if u >= len {
                        ends.end_u(w, s)
                    } else {
                        u
                    };
                    w.start + d * u + n * s
                };
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
                let mut gaps: Vec<(f64, f64)> = Vec::new();
                for oid in model.dependents(e.id) {
                    let Some(oe) = model.get(oid) else { continue };
                    let ElementKind::Opening(o) = &oe.kind else {
                        continue;
                    };
                    let Some(c) = Cut::of(o, w) else { continue };
                    if cut_v > c.v0 && cut_v < c.v1 {
                        gaps.push((c.u0, c.u1));
                        match o.kind {
                            OpeningKind::Window => {
                                for s in [-t, 0.0, t] {
                                    items.push(PlanItem::Line {
                                        id: oid,
                                        pen: Pen::Symbol,
                                        a: at(c.u0, s),
                                        b: at(c.u1, s),
                                    });
                                }
                            }
                            OpeningKind::Door => {
                                let hinge = at(c.u0, t);
                                let r = c.u1 - c.u0;
                                let a0 = d.to_angle();
                                items.push(PlanItem::Line {
                                    id: oid,
                                    pen: Pen::Symbol,
                                    a: hinge,
                                    b: hinge + n * r,
                                });
                                items.push(PlanItem::Arc {
                                    id: oid,
                                    pen: Pen::Symbol,
                                    center: hinge,
                                    radius: r,
                                    start: a0,
                                    end: a0 + std::f64::consts::FRAC_PI_2,
                                });
                            }
                        }
                    }
                }
                gaps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                let mut u = 0.0;
                let mut pieces = Vec::new();
                for (g0, g1) in gaps {
                    if g0 > u + 1e-9 {
                        pieces.push((u, g0));
                    }
                    u = u.max(g1);
                }
                if w.length() > u + 1e-9 {
                    pieces.push((u, w.length()));
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
                items.insert(
                    0,
                    PlanItem::Polygon {
                        id: e.id,
                        pen: Pen::Projection,
                        points: s.boundary.clone(),
                    },
                );
            }
            ElementKind::Sketch(sk) if sk.level == Some(level) => {
                items.extend(sketch_items(e.id, sk, &|p| p));
            }
            ElementKind::Guide(g) if g.level.is_none_or(|l| l == level) => {
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
    clean_up_cuts(model, &mut items);
    if !sliced.is_empty() {
        let z = model.level(level).map_or(0.0, |l| l.elevation) + cut_height;
        let scene = crate::scene::build_scene(model);
        for item in scene.iter().filter(|i| sliced.contains(&i.id)) {
            sliced_plan(model, item, z, &mut items);
        }
    }
    PlanView { items }
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
        items.push(PlanItem::Fill {
            id: item.id,
            points: lp,
            hatch: hatch_of(model, item.id),
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
