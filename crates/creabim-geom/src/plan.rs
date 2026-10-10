//! Plan drawings generated from the model: walls cut at a height, openings
//! drawn with architectural symbols, everything below shown in projection.

use crate::wall::Cut;
use creabim_core::{ElementId, ElementKind, Model, OpeningKind};
use glam::DVec2;
use std::fmt::Write;

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
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlanItem {
    Polygon {
        id: ElementId,
        pen: Pen,
        points: Vec<DVec2>,
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
}

impl PlanItem {
    pub fn id(&self) -> ElementId {
        match self {
            PlanItem::Polygon { id, .. }
            | PlanItem::Line { id, .. }
            | PlanItem::Arc { id, .. }
            | PlanItem::Label { id, .. } => *id,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PlanView {
    pub items: Vec<PlanItem>,
}

impl PlanView {
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let mut pts = Vec::new();
        for it in &self.items {
            match it {
                PlanItem::Polygon { points, .. } => pts.extend(points.iter().copied()),
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
        let margin = 1.0;
        let (lo, hi) = (lo - DVec2::splat(margin), hi + DVec2::splat(margin));
        let k = 1000.0 / scale; // metres in model -> mm on paper
        let size = (hi - lo) * k;
        let tx = |p: DVec2| DVec2::new((p.x - lo.x) * k, (hi.y - p.y) * k);
        let mut s = String::new();
        let _ = writeln!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{:.1}mm" height="{:.1}mm" viewBox="0 0 {:.3} {:.3}">"#,
            size.x, size.y, size.x, size.y
        );
        let _ = writeln!(s, r#"<rect width="100%" height="100%" fill="white"/>"#);
        let style = |pen: Pen| match pen {
            Pen::Cut => r##"stroke="#000" stroke-width="0.5" fill="#bbb""##,
            Pen::Projection => r##"stroke="#444" stroke-width="0.18" fill="none""##,
            Pen::Symbol => r##"stroke="#000" stroke-width="0.18" fill="none""##,
            Pen::Grid => {
                r##"stroke="#888" stroke-width="0.13" stroke-dasharray="4 1 1 1" fill="none""##
            }
        };
        for it in &self.items {
            match it {
                PlanItem::Polygon { pen, points, .. } => {
                    let pts: Vec<String> = points
                        .iter()
                        .map(|p| {
                            let q = tx(*p);
                            format!("{:.3},{:.3}", q.x, q.y)
                        })
                        .collect();
                    let _ = writeln!(
                        s,
                        r#"<polygon points="{}" {}/>"#,
                        pts.join(" "),
                        style(*pen)
                    );
                }
                PlanItem::Line { pen, a, b, .. } => {
                    let (a, b) = (tx(*a), tx(*b));
                    let _ = writeln!(
                        s,
                        r#"<line x1="{:.3}" y1="{:.3}" x2="{:.3}" y2="{:.3}" {}/>"#,
                        a.x,
                        a.y,
                        b.x,
                        b.y,
                        style(*pen)
                    );
                }
                PlanItem::Arc {
                    pen,
                    center,
                    radius,
                    start,
                    end,
                    ..
                } => {
                    let a = tx(*center + DVec2::from_angle(*start) * *radius);
                    let b = tx(*center + DVec2::from_angle(*end) * *radius);
                    let r = radius * k;
                    let large = if (end - start).abs() > std::f64::consts::PI {
                        1
                    } else {
                        0
                    };
                    // y is flipped on paper, so a CCW arc in the model is CW on paper (sweep 0)
                    let _ = writeln!(
                        s,
                        r#"<path d="M {:.3} {:.3} A {r:.3} {r:.3} 0 {large} 0 {:.3} {:.3}" {}/>"#,
                        a.x,
                        a.y,
                        b.x,
                        b.y,
                        style(*pen)
                    );
                }
                PlanItem::Label { at, text, .. } => {
                    let p = tx(*at);
                    let _ = writeln!(
                        s,
                        r#"<text x="{:.3}" y="{:.3}" font-family="sans-serif" font-size="3" text-anchor="middle" dominant-baseline="middle">{}</text>"#,
                        p.x,
                        p.y,
                        xml_escape(text)
                    );
                }
            }
        }
        s.push_str("</svg>\n");
        s
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Generate the plan of one level, cut `cut_height` metres above it.
pub fn plan_view(model: &Model, level: ElementId, cut_height: f64) -> PlanView {
    let mut items = Vec::new();
    for e in model.iter() {
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
                let at = |u: f64, s: f64| w.start + d * u + n * s;
                let cut_v = cut_height - w.base_offset;
                if cut_v <= 0.0 || cut_v >= w.height {
                    // wall entirely below or above the cut: show its outline in projection
                    if cut_v >= w.height {
                        items.push(PlanItem::Polygon {
                            id: e.id,
                            pen: Pen::Projection,
                            points: vec![
                                at(0.0, -t),
                                at(w.length(), -t),
                                at(w.length(), t),
                                at(0.0, t),
                            ],
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
            ElementKind::FreeMesh(f) if f.level == Some(level) => {
                let mut seen = std::collections::HashSet::new();
                for face in &f.mesh.faces {
                    for i in 0..face.len() {
                        let (a, b) = (face[i], face[(i + 1) % face.len()]);
                        if !seen.insert((a.min(b), a.max(b))) {
                            continue;
                        }
                        let pa = f.mesh.vertices[a as usize] + f.origin;
                        let pb = f.mesh.vertices[b as usize] + f.origin;
                        if pa.truncate().distance(pb.truncate()) < 1e-6 {
                            continue;
                        }
                        items.push(PlanItem::Line {
                            id: e.id,
                            pen: Pen::Projection,
                            a: pa.truncate(),
                            b: pb.truncate(),
                        });
                    }
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
    PlanView { items }
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
