//! Dimensions and tags: where they sit in a view's drawing, what they
//! read, and the linework they are drawn with. Both follow their hosts,
//! because their positions are worked out from the model every time.

use crate::plan::PlanItem;
use crate::section::{AxoFrame, SectionFrame};
use creabim_core::{ElementId, ElementKind, HostPoint, Model, OpeningKind, View, ViewKind, Wall};
use glam::{DVec2, DVec3};

/// Maps world points into a view's drawing coordinates.
pub enum Projector {
    Plan,
    Section(SectionFrame),
    Axo(AxoFrame),
}

impl Projector {
    pub fn of(model: &Model, view: &View) -> Option<Self> {
        Some(match view.kind {
            ViewKind::Plan { .. } => Projector::Plan,
            ViewKind::Section {
                start, end, depth, ..
            } => Projector::Section(SectionFrame::new(start, end, depth)),
            ViewKind::Axonometric { azimuth, altitude } => {
                Projector::Axo(AxoFrame::new(model, azimuth, altitude))
            }
            ViewKind::Schedule { .. } => return None,
        })
    }

    pub fn project(&self, p: DVec3) -> DVec2 {
        match self {
            Projector::Plan => p.truncate(),
            Projector::Section(f) => f.project(p).truncate(),
            Projector::Axo(f) => f.project(p).truncate(),
        }
    }
}

/// A wall's frame: a point `u` along it and `s` to its left.
fn on_wall(w: &Wall, u: f64, s: f64) -> DVec2 {
    w.start + w.direction() * u + w.direction().perp() * s
}

fn host_wall(model: &Model, id: ElementId) -> Option<(&Wall, f64)> {
    match &model.get(id)?.kind {
        ElementKind::Wall(w) => Some((w, 0.0)),
        ElementKind::Opening(o) => match &model.get(o.host)?.kind {
            ElementKind::Wall(w) => Some((w, o.offset)),
            _ => None,
        },
        _ => None,
    }
}

/// Where a dimension point sits in a view's drawing, if it can be drawn there.
pub fn resolve(model: &Model, view: &View, p: &HostPoint) -> Option<DVec2> {
    let Some(host) = p.host else {
        return Some(p.local);
    };
    match (&model.get(host)?.kind, &view.kind) {
        (ElementKind::Level(l), ViewKind::Section { .. }) => {
            Some(DVec2::new(p.local.x, l.elevation + p.local.y))
        }
        (ElementKind::Wall(_) | ElementKind::Opening(_), ViewKind::Plan { .. }) => {
            let (w, base) = host_wall(model, host)?;
            let u = if p.from_end {
                w.length() + p.local.x
            } else {
                base + p.local.x
            };
            Some(on_wall(w, u, p.local.y))
        }
        _ => None,
    }
}

/// A dimension point attached to `host`, expressed in the host's frame so
/// it follows the host. `at` is a drawing point in `view`.
pub fn attach(model: &Model, view: &View, host: Option<ElementId>, at: DVec2) -> HostPoint {
    let attached = host.and_then(|h| match (&model.get(h)?.kind, &view.kind) {
        (ElementKind::Level(l), ViewKind::Section { .. }) => Some(HostPoint {
            host: Some(h),
            local: DVec2::new(at.x, at.y - l.elevation),
            from_end: false,
        }),
        (ElementKind::Wall(_) | ElementKind::Opening(_), ViewKind::Plan { .. }) => {
            let (w, base) = host_wall(model, h)?;
            let d = w.direction();
            let u = (at - w.start).dot(d);
            let s = (at - w.start).dot(d.perp());
            let is_wall = matches!(model.get(h)?.kind, ElementKind::Wall(_));
            // points nearer a wall's end are kept relative to the end, so
            // they follow it when the wall is lengthened
            let from_end = is_wall && u > w.length() / 2.0;
            Some(HostPoint {
                host: Some(h),
                local: DVec2::new(if from_end { u - w.length() } else { u - base }, s),
                from_end,
            })
        }
        _ => None,
    });
    attached.unwrap_or(HostPoint::free(at))
}

/// The centre of an element's body, in world coordinates.
pub fn centre(model: &Model, id: ElementId) -> Option<DVec3> {
    let e = model.get(id)?;
    let elevation = |level: ElementId| model.level(level).map_or(0.0, |l| l.elevation);
    Some(match &e.kind {
        ElementKind::Wall(w) => on_wall(w, w.length() / 2.0, 0.0)
            .extend(elevation(w.level) + w.base_offset + w.height / 2.0),
        ElementKind::Opening(o) => {
            let ElementKind::Wall(w) = &model.get(o.host)?.kind else {
                return None;
            };
            on_wall(w, o.offset + o.width / 2.0, 0.0)
                .extend(elevation(w.level) + w.base_offset + o.sill + o.height / 2.0)
        }
        ElementKind::Slab(s) => {
            let c = s.boundary.iter().copied().sum::<DVec2>() / s.boundary.len().max(1) as f64;
            c.extend(elevation(s.level) - s.thickness / 2.0)
        }
        ElementKind::FreeMesh(m) => {
            let n = m.mesh.vertices.len().max(1) as f64;
            let c = m.mesh.vertices.iter().copied().sum::<DVec3>() / n;
            c + m.origin + DVec3::Z * m.level.map_or(0.0, elevation)
        }
        _ => return None,
    })
}

/// What a tag on `id` reads, and whether it sits in a circle (doors) or a box.
pub fn tag_text(model: &Model, id: ElementId) -> (String, bool) {
    let Some(e) = model.get(id) else {
        return (String::new(), false);
    };
    let material = creabim_core::material_of(model, e).map(|(m, _)| m.name.clone());
    let with_material = |mm: f64| match &material {
        Some(m) => format!("{m} {mm:.0}"),
        None => format!("{mm:.0}"),
    };
    match &e.kind {
        ElementKind::Opening(o) => (
            creabim_core::marks(model)
                .remove(&id)
                .unwrap_or_else(|| e.name.clone()),
            o.kind == OpeningKind::Door,
        ),
        ElementKind::Wall(w) => (with_material(w.thickness * 1000.0), false),
        ElementKind::Slab(s) => (with_material(s.thickness * 1000.0), false),
        _ => (e.name.clone(), false),
    }
}

/// A dimension's value as it is written: millimetres.
pub fn dimension_text(a: DVec2, b: DVec2) -> String {
    format!("{:.0}", a.distance(b) * 1000.0)
}

/// The dimensions and tags placed in a view, as drawing items.
pub fn annotations(model: &Model, view_id: ElementId, view: &View) -> Vec<PlanItem> {
    let Some(project) = Projector::of(model, view) else {
        return vec![];
    };
    let mut out = Vec::new();
    for e in model.iter() {
        match &e.kind {
            ElementKind::Dimension(d) if d.view == view_id => {
                let (Some(a), Some(b)) = (resolve(model, view, &d.a), resolve(model, view, &d.b))
                else {
                    continue;
                };
                if a.distance(b) < 1e-6 {
                    continue;
                }
                out.push(PlanItem::Dimension {
                    id: e.id,
                    a,
                    b,
                    offset: d.offset,
                    text: dimension_text(a, b),
                });
            }
            ElementKind::Tag(t) if t.view == view_id => {
                let Some(c) = centre(model, t.host) else {
                    continue;
                };
                let anchor = project.project(c);
                let (text, round) = tag_text(model, t.host);
                out.push(PlanItem::Tag {
                    id: e.id,
                    anchor,
                    at: anchor + t.offset,
                    text,
                    round,
                });
            }
            _ => {}
        }
    }
    out
}

/// Linework of a dimension, in drawing units; `mm` is drawing units per
/// paper millimetre, so ticks and gaps are a fixed size on paper.
pub struct DimensionGeometry {
    pub lines: Vec<(DVec2, DVec2)>,
    /// Ticks, drawn heavier than the dimension line.
    pub ticks: Vec<(DVec2, DVec2)>,
    pub text_at: DVec2,
    /// Text angle in radians, kept readable (never upside down).
    pub angle: f64,
}

pub fn dimension_geometry(a: DVec2, b: DVec2, offset: f64, mm: f64) -> DimensionGeometry {
    let u = (b - a).normalize_or_zero();
    let n = u.perp();
    let off = n * offset;
    let (pa, pb) = (a + off, b + off);
    let mut lines = vec![(pa - u * 1.5 * mm, pb + u * 1.5 * mm)];
    if offset.abs() > 2.0 * mm {
        // witness lines from just off the measured points to just past the line
        let s = n * offset.signum();
        lines.push((a + s * 1.0 * mm, pa + s * 1.5 * mm));
        lines.push((b + s * 1.0 * mm, pb + s * 1.5 * mm));
    }
    let tick = (u + n).normalize() * 1.25 * mm;
    let ticks = vec![(pa - tick, pa + tick), (pb - tick, pb + tick)];
    let mut angle = u.to_angle();
    let mut up = n;
    if angle > std::f64::consts::FRAC_PI_2 + 1e-6 || angle <= -std::f64::consts::FRAC_PI_2 + 1e-6 {
        angle += std::f64::consts::PI;
        up = -n;
    }
    if angle > std::f64::consts::PI {
        angle -= std::f64::consts::TAU;
    }
    DimensionGeometry {
        lines,
        ticks,
        text_at: (pa + pb) / 2.0 + up * 1.8 * mm,
        angle,
    }
}

/// A tag's bubble and leader, in drawing units.
pub struct TagGeometry {
    pub outline: Vec<DVec2>,
    pub leader: Option<(DVec2, DVec2)>,
}

/// Text height of tags and dimensions, paper mm.
pub const TEXT: f64 = 2.5;

pub fn tag_geometry(anchor: DVec2, at: DVec2, text: &str, round: bool, mm: f64) -> TagGeometry {
    let half = DVec2::new(
        (crate::paper::text_width(text, TEXT) / 2.0 + 1.2).max(2.2),
        2.2,
    ) * mm;
    let outline: Vec<DVec2> = if round {
        let r = half.x.max(half.y);
        (0..24)
            .map(|i| at + DVec2::from_angle(i as f64 / 24.0 * std::f64::consts::TAU) * r)
            .collect()
    } else {
        vec![
            at - half,
            at + DVec2::new(half.x, -half.y),
            at + half,
            at + DVec2::new(-half.x, half.y),
        ]
    };
    // a leader back to the host, from the edge of the bubble
    let d = anchor - at;
    let leader = if round {
        let r = half.x.max(half.y);
        (d.length() > r + 1.0 * mm).then(|| (at + d.normalize() * r, anchor))
    } else {
        let t = (half.x / d.x.abs().max(1e-12)).min(half.y / d.y.abs().max(1e-12));
        (t < 1.0 && d.length() * (1.0 - t) > 1.0 * mm).then(|| (at + d * t, anchor))
    };
    TagGeometry { outline, leader }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::section::view_drawing;
    use creabim_core::sample::sample_house;

    fn named(m: &Model, name: &str) -> ElementId {
        m.iter().find(|e| e.name == name).unwrap().id
    }

    #[test]
    fn dimensions_follow_their_walls() {
        let mut doc = creabim_core::Document::from_model(sample_house());
        let plan = named(doc.model(), "Ground Floor plan");
        let overall = |m: &Model| {
            view_drawing(m, plan)
                .unwrap()
                .items
                .into_iter()
                .filter_map(|i| match i {
                    PlanItem::Dimension { text, .. } => text.parse::<f64>().ok(),
                    _ => None,
                })
                .fold(0.0, f64::max)
        };
        assert_eq!(overall(doc.model()), 10300.0);
        // stretch the house: move the east walls out by a metre
        for name in ["GF Wall 1", "GF Wall 2", "GF Wall 3"] {
            let id = named(doc.model(), name);
            doc.edit(id, "Stretch", |e| {
                if let ElementKind::Wall(w) = &mut e.kind {
                    for p in [&mut w.start, &mut w.end] {
                        if p.x > 9.0 {
                            p.x += 1.0;
                        }
                    }
                }
            })
            .unwrap();
        }
        assert_eq!(overall(doc.model()), 11300.0);
    }

    #[test]
    fn tags_read_marks_and_follow_doors() {
        let mut doc = creabim_core::Document::from_model(sample_house());
        let plan = named(doc.model(), "Ground Floor plan");
        let door = named(doc.model(), "Front Door");
        let tag_at = |m: &Model| {
            view_drawing(m, plan)
                .unwrap()
                .items
                .into_iter()
                .find_map(|i| match i {
                    PlanItem::Tag { at, text, .. } if text == "D01" => Some(at),
                    _ => None,
                })
                .expect("front door is tagged D01")
        };
        let before = tag_at(doc.model());
        doc.edit(door, "Move door", |e| {
            if let ElementKind::Opening(o) = &mut e.kind {
                o.offset += 2.0;
            }
        })
        .unwrap();
        let after = tag_at(doc.model());
        assert!((after.x - before.x - 2.0).abs() < 1e-9);
    }

    #[test]
    fn dimension_text_reads_upright() {
        let g = dimension_geometry(DVec2::new(0.0, 5.0), DVec2::ZERO, 1.0, 0.1);
        assert!(g.angle.abs() <= std::f64::consts::FRAC_PI_2 + 1e-9);
    }
}
