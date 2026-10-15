//! Angular, radial, diameter and arc-length dimensions and text notes:
//! what they measure and the linework, arrows and text they are drawn
//! with. Everything is in drawing units; `mm` is drawing units per paper
//! millimetre, so arrows, gaps and text keep their size on paper.

use crate::paper::{text_width, Anchor};
use creabim_core::{DimensionKind, Font, TextStyle, Units};
use glam::DVec2;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// How an annotation's text and linework look: from its text style, with
/// the element's colour on top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLook {
    /// Letter height, paper mm.
    pub size: f64,
    /// sRGB 0..1; `None` is the view's ink (black on paper).
    pub rgb: Option<[f32; 3]>,
    pub font: Font,
    pub bold: bool,
    pub italic: bool,
}

impl Default for TextLook {
    fn default() -> Self {
        TextStyle::default().into()
    }
}

impl From<TextStyle> for TextLook {
    fn from(s: TextStyle) -> Self {
        Self {
            size: s.size,
            rgb: s.color,
            font: s.font,
            bold: s.bold,
            italic: s.italic,
        }
    }
}

impl TextLook {
    /// Rough width of `text` on paper, mm.
    pub fn width(&self, text: &str) -> f64 {
        let w = match self.font {
            Font::Mono => text.chars().count() as f64 * 0.6 * self.size,
            Font::Serif => text_width(text, self.size) * 0.95,
            Font::Sans => text_width(text, self.size),
        };
        if self.bold {
            w * 1.08
        } else {
            w
        }
    }

    /// The same look `factor` times as big, for drawings shown at another scale.
    pub fn scaled(self, factor: f64) -> Self {
        Self {
            size: self.size * factor,
            ..self
        }
    }
}

// ---- measuring --------------------------------------------------------------

/// The angle at `vertex` turning counter-clockwise from `a` to `b`, in
/// radians, in (0, 2π]. Zero-length arms measure 0.
pub fn sweep(vertex: DVec2, a: DVec2, b: DVec2) -> f64 {
    let (da, db) = (a - vertex, b - vertex);
    if da.length_squared() < 1e-18 || db.length_squared() < 1e-18 {
        return 0.0;
    }
    let s = (db.to_angle() - da.to_angle()).rem_euclid(TAU);
    if s < 1e-12 {
        TAU
    } else {
        s
    }
}

/// An angle as written: degrees, to a tenth where it is not whole.
pub fn angle_text(radians: f64) -> String {
    let d = (radians.to_degrees() * 10.0).round() / 10.0;
    if (d - d.round()).abs() < 1e-9 {
        format!("{d:.0}°")
    } else {
        format!("{d:.1}°")
    }
}

/// A length as dimensions write it in `units`.
pub fn length_text(units: &Units, metres: f64) -> String {
    units.dimension(metres)
}

/// Where the endless lines through `a0`,`a1` and `b0`,`b1` cross; none when
/// they are parallel.
pub fn line_intersection(a0: DVec2, a1: DVec2, b0: DVec2, b1: DVec2) -> Option<DVec2> {
    let (da, db) = (a1 - a0, b1 - b0);
    let den = da.perp_dot(db);
    if den.abs() < 1e-12 * da.length() * db.length() || den == 0.0 {
        return None;
    }
    let t = (b0 - a0).perp_dot(db) / den;
    Some(a0 + da * t)
}

/// The circle through three points, as (centre, radius); none when they
/// are in a line.
pub fn circle_through(p: DVec2, q: DVec2, r: DVec2) -> Option<(DVec2, f64)> {
    let (b, c) = (q - p, r - p);
    let d = 2.0 * b.perp_dot(c);
    if d.abs() < 1e-12 {
        return None;
    }
    let (bb, cc) = (b.length_squared(), c.length_squared());
    let o = DVec2::new(c.y * bb - b.y * cc, b.x * cc - c.x * bb) / d;
    Some((p + o, o.length()))
}

/// What a dimension of `kind` reads, from its resolved points.
pub fn measure(units: &Units, kind: DimensionKind, center: DVec2, a: DVec2, b: DVec2) -> String {
    let l = |m: f64| length_text(units, m);
    match kind {
        DimensionKind::Linear => l(a.distance(b)),
        DimensionKind::Angular => angle_text(sweep(center, a, b)),
        DimensionKind::Radius => format!("R{}", l(center.distance(a))),
        DimensionKind::Diameter => format!("Ø{}", l(2.0 * center.distance(a))),
        DimensionKind::ArcLength => l(center.distance(a) * sweep(center, a, b)),
    }
}

// ---- shapes -----------------------------------------------------------------

/// The geometry of an annotation, resolved in its view.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// An arc `radius` from `vertex`, from the arm through `a`
    /// counter-clockwise to the arm through `b`.
    Angle {
        vertex: DVec2,
        a: DVec2,
        b: DVec2,
        radius: f64,
    },
    /// A line from the centre (or, for a diameter, from across the circle)
    /// to `at` on it, running on `extra` past the circle.
    Radius {
        center: DVec2,
        at: DVec2,
        extra: f64,
        diameter: bool,
    },
    /// An arc `offset` outside the arc about `center` from `a`
    /// counter-clockwise to `b`.
    ArcLength {
        center: DVec2,
        a: DVec2,
        b: DVec2,
        offset: f64,
    },
    /// Lines of text from `at`, with a leader to `leader`.
    Note { at: DVec2, leader: Option<DVec2> },
    /// A level's head where its datum line ends at `at`, Revit style: a
    /// target symbol, the level's name (the annotation's text) above the
    /// line and its `elevation` below.
    Level { at: DVec2, elevation: String },
}

/// Radius of a level head's target symbol, paper mm.
pub const LEVEL_HEAD: f64 = 1.5;

/// Clear paper a level head keeps above and below its text, mm.
pub const LEVEL_ROOM: f64 = 6.0;

/// An annotation ready to draw: its shape, what it reads and how it looks.
#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub shape: Shape,
    /// The value of a dimension, or a note's text (lines split by `\n`).
    pub text: String,
    pub look: TextLook,
    /// Drawing units per paper mm at the view's own scale, for its extent.
    pub mm: f64,
}

/// A piece of text in [`Marks`].
#[derive(Clone, Debug, PartialEq)]
pub struct MarkText {
    pub at: DVec2,
    pub text: String,
    /// Radians counter-clockwise, kept readable.
    pub angle: f64,
    pub anchor: Anchor,
}

/// The linework of an annotation, in drawing units.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marks {
    /// Thin open polylines.
    pub lines: Vec<Vec<DVec2>>,
    /// Filled arrowheads.
    pub arrows: Vec<[DVec2; 3]>,
    pub texts: Vec<MarkText>,
}

/// Arrowhead length and half width, paper mm.
const ARROW: (f64, f64) = (2.5, 0.55);
/// Gap between text and its line, paper mm.
const TEXT_GAP: f64 = 1.0;

/// An arrowhead with its tip at `tip`, pointing along `dir`.
fn arrow(tip: DVec2, dir: DVec2, mm: f64) -> [DVec2; 3] {
    let d = dir.normalize_or(DVec2::X);
    let base = tip - d * ARROW.0 * mm;
    let n = d.perp() * ARROW.1 * mm;
    [tip, base + n, base - n]
}

/// A direction's angle turned so text along it reads left to right (or
/// bottom to top), and the side that is then "up".
pub fn upright(angle: f64) -> (f64, DVec2) {
    let mut a = angle.rem_euclid(TAU);
    if a > PI {
        a -= TAU;
    }
    if a > FRAC_PI_2 + 1e-6 || a <= -FRAC_PI_2 + 1e-6 {
        a += PI;
        if a > PI {
            a -= TAU;
        }
    }
    (a, DVec2::from_angle(a).perp())
}

/// Points along an arc about `c`, from angle `t0` turning `s`.
fn arc_points(c: DVec2, r: f64, t0: f64, s: f64) -> Vec<DVec2> {
    let n = ((s.abs() / TAU * 72.0).ceil() as usize).max(6);
    (0..=n)
        .map(|i| c + DVec2::from_angle(t0 + s * i as f64 / n as f64) * r)
        .collect()
}

/// An arc dimension line about `c` at radius `r` from `t0` turning `s`,
/// with arrows at both ends pointing out, and its text outside the middle.
fn arc_line(out: &mut Marks, c: DVec2, r: f64, t0: f64, s: f64, look: &TextLook, mm: f64) -> DVec2 {
    out.lines.push(arc_points(c, r, t0, s));
    let (p0, p1) = (DVec2::from_angle(t0), DVec2::from_angle(t0 + s));
    out.arrows.push(arrow(c + p0 * r, -p0.perp(), mm));
    out.arrows.push(arrow(c + p1 * r, p1.perp(), mm));
    let mid = DVec2::from_angle(t0 + s / 2.0);
    c + mid * (r + (TEXT_GAP + look.size / 2.0) * mm)
}

/// Text centred on `at`, reading along the direction `angle`.
fn text_along(at: DVec2, angle: f64, text: &str) -> MarkText {
    MarkText {
        at,
        text: text.to_string(),
        angle: upright(angle).0,
        anchor: Anchor::Middle,
    }
}

/// Lines and arrows from `from` out radially to `to`, with a gap at the
/// measured point, for an arc's witness lines.
fn witness(out: &mut Marks, c: DVec2, dir: DVec2, from: f64, to: f64, mm: f64) {
    let s = (to - from).signum();
    if (to - from).abs() > 2.0 * mm {
        out.lines.push(vec![
            c + dir * (from + s * mm),
            c + dir * (to + s * 1.5 * mm),
        ]);
    }
}

impl Annotation {
    /// Linework at `mm` drawing units per paper millimetre.
    pub fn marks(&self, mm: f64) -> Marks {
        let mut out = Marks::default();
        let look = &self.look;
        match self.shape {
            Shape::Angle {
                vertex,
                a,
                b,
                radius,
            } => {
                let s = sweep(vertex, a, b);
                if s <= 0.0 {
                    return out;
                }
                let r = radius.abs().max(3.0 * mm);
                let t0 = (a - vertex).to_angle();
                for (p, t) in [(a, t0), (b, t0 + s)] {
                    let d = DVec2::from_angle(t);
                    let reach = p.distance(vertex);
                    if r > reach {
                        witness(&mut out, vertex, d, reach, r, mm);
                    }
                }
                let at = arc_line(&mut out, vertex, r, t0, s, look, mm);
                out.texts
                    .push(text_along(at, t0 + s / 2.0 + FRAC_PI_2, &self.text));
            }
            Shape::Radius {
                center,
                at,
                extra,
                diameter,
            } => {
                let r = center.distance(at);
                let d = (at - center).normalize_or(DVec2::X);
                let end = at + d * extra.max(0.0);
                let start = if diameter { center - d * r } else { center };
                out.lines.push(vec![start, end]);
                out.arrows.push(arrow(at, d, mm));
                if diameter {
                    out.arrows.push(arrow(center - d * r, -d, mm));
                }
                // a small cross at the centre
                let c = 1.5 * mm;
                out.lines
                    .push(vec![center - DVec2::X * c, center + DVec2::X * c]);
                out.lines
                    .push(vec![center - DVec2::Y * c, center + DVec2::Y * c]);
                // text beside the line: past the circle when the line runs
                // on out, else halfway along the radius
                let along = if extra > look.width(&self.text) * mm {
                    at + d * (extra / 2.0)
                } else {
                    center + (at - center) * 0.55
                };
                let (angle, up) = upright(d.to_angle());
                out.texts.push(MarkText {
                    at: along + up * (TEXT_GAP + look.size / 2.0) * mm,
                    text: self.text.clone(),
                    angle,
                    anchor: Anchor::Middle,
                });
            }
            Shape::ArcLength {
                center,
                a,
                b,
                offset,
            } => {
                let s = sweep(center, a, b);
                let r = center.distance(a);
                if s <= 0.0 || r <= 0.0 {
                    return out;
                }
                let rr = if r + offset > 1e-9 { r + offset } else { r };
                let t0 = (a - center).to_angle();
                for t in [t0, t0 + s] {
                    witness(&mut out, center, DVec2::from_angle(t), r, rr, mm);
                }
                let at = arc_line(&mut out, center, rr, t0, s, look, mm);
                let mid = t0 + s / 2.0 + FRAC_PI_2;
                out.texts.push(text_along(at, mid, &self.text));
                // the arc-length sign: a small arc over the figures
                let (angle, up) = upright(mid);
                let along = DVec2::from_angle(angle);
                let half = look.width(&self.text) * 0.35 * mm;
                let lift = at + up * look.size * 0.75 * mm;
                let h = look.size * 0.35 * mm;
                out.lines.push(
                    (0..=12)
                        .map(|i| {
                            let t = PI * i as f64 / 12.0;
                            lift + along * (-half * t.cos()) + up * (h * t.sin())
                        })
                        .collect(),
                );
            }
            Shape::Level { at, ref elevation } => {
                let r = LEVEL_HEAD * mm;
                out.lines.push(arc_points(at, r, 0.0, TAU));
                // two opposite quarters filled in
                for q in [0.0, PI] {
                    for i in 0..4 {
                        let t = |k: usize| q + FRAC_PI_2 * k as f64 / 4.0;
                        out.arrows.push([
                            at,
                            at + DVec2::from_angle(t(i)) * r,
                            at + DVec2::from_angle(t(i + 1)) * r,
                        ]);
                    }
                }
                let x = at.x + r + TEXT_GAP * mm;
                let dy = (look.size * 0.5 + 0.8) * mm;
                for (y, text) in [(dy, self.text.clone()), (-dy, elevation.clone())] {
                    out.texts.push(MarkText {
                        at: DVec2::new(x, at.y + y),
                        text,
                        angle: 0.0,
                        anchor: Anchor::Start,
                    });
                }
            }
            Shape::Note { at, leader } => {
                let lines: Vec<&str> = self.text.split('\n').collect();
                let step = look.size * 1.5 * mm;
                let width = lines.iter().map(|l| look.width(l)).fold(0.0, f64::max) * mm;
                for (i, l) in lines.iter().enumerate() {
                    out.texts.push(MarkText {
                        at: at - DVec2::Y * step * i as f64,
                        text: l.to_string(),
                        angle: 0.0,
                        anchor: Anchor::Start,
                    });
                }
                if let Some(target) = leader {
                    out.lines.extend(note_leader(at, width, target, mm));
                    let from = out
                        .lines
                        .last()
                        .and_then(|l| l.iter().rev().nth(1).copied());
                    if let Some(from) = from {
                        out.arrows.push(arrow(target, target - from, mm));
                    }
                }
            }
        }
        out
    }

    /// Corners of what it covers, at its own view's scale.
    pub fn extent(&self) -> Vec<DVec2> {
        let mm = self.mm;
        let m = self.marks(mm);
        let mut pts: Vec<DVec2> = m.lines.iter().flatten().copied().collect();
        pts.extend(m.arrows.iter().flatten());
        for t in &m.texts {
            pts.extend(text_box(t, &self.look, mm));
        }
        // a level head keeps clear paper above and below it, so the top
        // and bottom levels of a section don't sit on the drawing's edge
        if let Shape::Level { at, .. } = self.shape {
            let room = (LEVEL_ROOM + self.look.size * 2.0) * mm;
            pts.extend([at + DVec2::Y * room, at - DVec2::Y * room]);
        }
        pts
    }

    /// Is `p` on its linework or text, within `tol`?
    pub fn hit(&self, p: DVec2, tol: f64, mm: f64) -> bool {
        let m = self.marks(mm);
        let near = m
            .lines
            .iter()
            .any(|l| l.windows(2).any(|w| segment_distance(p, w[0], w[1]) < tol));
        near || m.texts.iter().any(|t| {
            let w = self.look.width(&t.text) * mm;
            let h = self.look.size * mm;
            let (dir, up) = (
                DVec2::from_angle(t.angle),
                DVec2::from_angle(t.angle).perp(),
            );
            let x0 = match t.anchor {
                Anchor::Start => 0.0,
                Anchor::Middle => -w / 2.0,
                Anchor::End => -w,
            };
            let x = (p - t.at).dot(dir);
            let y = (p - t.at).dot(up);
            x >= x0 - tol && x <= x0 + w + tol && y.abs() <= h * 0.7 + tol
        })
    }

    /// The point a drag grabs it by: a note's first line, a dimension's text.
    pub fn grip(&self, mm: f64) -> Option<DVec2> {
        match self.shape {
            Shape::Note { at, .. } => Some(at),
            _ => self.marks(mm).texts.first().map(|t| t.at),
        }
    }
}

/// Corners of a piece of text's box.
fn text_box(t: &MarkText, look: &TextLook, mm: f64) -> [DVec2; 4] {
    let w = look.width(&t.text) * mm;
    let h = look.size * 0.6 * mm;
    let (dir, up) = (
        DVec2::from_angle(t.angle),
        DVec2::from_angle(t.angle).perp(),
    );
    let x0 = match t.anchor {
        Anchor::Start => 0.0,
        Anchor::Middle => -w / 2.0,
        Anchor::End => -w,
    };
    let o = t.at + dir * x0;
    [
        o - up * h,
        o + dir * w - up * h,
        o + dir * w + up * h,
        o + up * h,
    ]
}

/// A note's leader: from beside its first line (left or right, whichever
/// faces the target) along a short shoulder, then to the target.
pub fn note_leader(at: DVec2, width: f64, target: DVec2, mm: f64) -> Vec<Vec<DVec2>> {
    let left = target.x < at.x + width / 2.0;
    let (start, dir) = if left {
        (at - DVec2::X * mm, -1.0)
    } else {
        (at + DVec2::X * (width + mm), 1.0)
    };
    let shoulder = start + DVec2::X * dir * 2.5 * mm;
    // no shoulder when the target is right beside the text
    if (target.x - start.x) * dir < 3.0 * mm {
        return vec![vec![start, target]];
    }
    vec![vec![start, shoulder, target]]
}

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-18)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn angles_are_measured_counter_clockwise() {
        let o = DVec2::new(2.0, 1.0);
        let east = o + DVec2::X * 3.0;
        let north = o + DVec2::Y * 0.5;
        assert!(close(sweep(o, east, north), FRAC_PI_2));
        // the other way round is the reflex angle
        assert!(close(sweep(o, north, east), 3.0 * FRAC_PI_2));
        let diag = o + DVec2::new(1.0, 1.0);
        assert_eq!(angle_text(sweep(o, east, diag)), "45°");
        assert_eq!(angle_text(30.25f64.to_radians()), "30.3°");
        assert_eq!(
            measure(&Units::metric(), DimensionKind::Angular, o, east, north),
            "90°"
        );
    }

    #[test]
    fn radius_diameter_and_arc_length_read_in_millimetres() {
        let c = DVec2::new(1.0, 1.0);
        let a = c + DVec2::X * 1.5;
        let b = c + DVec2::Y * 1.5;
        assert_eq!(
            measure(&Units::metric(), DimensionKind::Radius, c, a, a),
            "R1500"
        );
        assert_eq!(
            measure(&Units::metric(), DimensionKind::Diameter, c, a, a),
            "Ø3000"
        );
        // a quarter of a 1.5 m circle
        assert_eq!(
            measure(&Units::metric(), DimensionKind::ArcLength, c, a, b),
            "2356"
        );
        assert_eq!(
            measure(&Units::metric(), DimensionKind::Linear, c, a, b),
            "2121"
        );
    }

    #[test]
    fn circles_and_lines_meet_where_expected() {
        let (c, r) = circle_through(
            DVec2::new(3.0, 0.0),
            DVec2::new(0.0, 3.0),
            DVec2::new(-3.0, 0.0),
        )
        .unwrap();
        assert!(c.length() < 1e-9 && close(r, 3.0));
        assert!(circle_through(DVec2::ZERO, DVec2::X, DVec2::X * 2.0).is_none());
        let x = line_intersection(
            DVec2::ZERO,
            DVec2::X,
            DVec2::new(2.0, 1.0),
            DVec2::new(2.0, 3.0),
        )
        .unwrap();
        assert!(x.distance(DVec2::new(2.0, 0.0)) < 1e-9);
        assert!(line_intersection(DVec2::ZERO, DVec2::X, DVec2::Y, DVec2::ONE).is_none());
    }

    fn note(text: &str, leader: Option<DVec2>) -> Annotation {
        Annotation {
            shape: Shape::Note {
                at: DVec2::new(10.0, 5.0),
                leader,
            },
            text: text.into(),
            look: TextLook::default(),
            mm: 0.1,
        }
    }

    #[test]
    fn notes_stack_their_lines_and_point_their_leader() {
        let n = note("First\nSecond\nThird", None);
        let m = n.marks(0.1);
        assert_eq!(m.texts.len(), 3);
        assert!(m.lines.is_empty() && m.arrows.is_empty());
        // lines run down the page, 1.5 letter heights apart
        assert!(close(m.texts[0].at.y - m.texts[1].at.y, 2.5 * 1.5 * 0.1));
        assert!(m.texts.iter().all(|t| t.anchor == Anchor::Start));
        // a target to the lower left: the leader leaves the left end with a shoulder
        let target = DVec2::new(7.0, 3.0);
        let m = note("Note", Some(target)).marks(0.1);
        let leader = &m.lines[0];
        assert_eq!(leader.len(), 3);
        assert!(leader[0].x < 10.0 && close(leader[0].y, 5.0));
        assert!(close(leader[1].y, 5.0) && leader[1].x < leader[0].x);
        assert_eq!(*leader.last().unwrap(), target);
        // its arrow's tip is on the target, pointing at it
        let [tip, b0, b1] = m.arrows[0];
        assert_eq!(tip, target);
        let back = ((b0 + b1) / 2.0 - tip).normalize();
        let along = (leader[1] - target).normalize();
        assert!(back.distance(along) < 1e-9);
        // to the right, it leaves the right end
        let m = note("Note", Some(DVec2::new(14.0, 8.0))).marks(0.1);
        assert!(m.lines[0][0].x > 10.0 + TextLook::default().width("Note") * 0.1);
        // it can be picked by its text or its leader
        let n = note("Note", Some(target));
        assert!(n.hit(DVec2::new(10.2, 5.0), 0.05, 0.1));
        assert!(n.hit((leader[1] + target) / 2.0, 0.05, 0.1));
        assert!(!n.hit(DVec2::new(20.0, 20.0), 0.05, 0.1));
    }

    #[test]
    fn angle_arcs_run_from_arm_to_arm() {
        let a = Annotation {
            shape: Shape::Angle {
                vertex: DVec2::ZERO,
                a: DVec2::X * 5.0,
                b: DVec2::Y * 5.0,
                radius: 2.0,
            },
            text: "90°".into(),
            look: TextLook::default(),
            mm: 0.1,
        };
        let m = a.marks(0.1);
        let arc = &m.lines[0];
        assert!(arc[0].distance(DVec2::X * 2.0) < 1e-9);
        assert!(arc.last().unwrap().distance(DVec2::Y * 2.0) < 1e-9);
        assert_eq!(m.arrows.len(), 2);
        // the text sits outside the arc, on the bisector, reading upright
        let t = &m.texts[0];
        assert!(t.at.length() > 2.0 && close(t.at.x, t.at.y));
        assert!(t.angle.abs() <= FRAC_PI_2 + 1e-9);
        // an arc further out than the arms gets witness lines
        let far = Annotation {
            shape: Shape::Angle {
                vertex: DVec2::ZERO,
                a: DVec2::X,
                b: DVec2::Y,
                radius: 3.0,
            },
            ..a
        };
        assert_eq!(far.marks(0.1).lines.len(), 3);
    }

    #[test]
    fn readable_text_never_hangs_upside_down() {
        for deg in (-360..=360).step_by(15) {
            let (a, up) = upright((deg as f64).to_radians());
            assert!(a > -FRAC_PI_2 - 1e-6 && a <= FRAC_PI_2 + 1e-6, "{deg}");
            assert!(up.y >= -1e-6, "{deg}");
        }
    }
}
