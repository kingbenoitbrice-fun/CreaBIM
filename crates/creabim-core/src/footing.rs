//! Foundation footings: strip footings running under walls and pad
//! footings under columns (or placed on their own).
//!
//! A footing hangs off a level like everything else, its top `below` that
//! level. One placed under a wall or a column is hosted by it: a strip
//! follows its wall's centreline and sits `below` the wall's base, a pad
//! stays centred under its column and turns with it, so moving, turning or
//! re-levelling the host takes the footing along. Deleting the host
//! deletes its footing, as it would on site.
//!
//! Strip footings meet one another the way walls do: two strips whose
//! lines meet in an L are mitred, one ending on the middle of another (a
//! T) stops at its side, and crossing strips clean up (see
//! [`crate::joins`]). An end that meets nothing runs on past the wall by
//! as much as the footing is wider than it, so the footing stands proud of
//! the wall all round.

use crate::{
    joins::{plain_joins, WallEnds},
    Element, ElementId, ElementKind, Model, Transaction, Wall,
};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// What kind of footing it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FootingKind {
    /// A continuous strip under a wall (IFC's STRIP_FOOTING).
    #[default]
    Strip,
    /// A rectangular pad under a column (IFC's PAD_FOOTING).
    Pad,
}

impl FootingKind {
    pub fn label(self) -> &'static str {
        match self {
            FootingKind::Strip => "Strip footing",
            FootingKind::Pad => "Pad footing",
        }
    }

    /// The IFC predefined type it exports as.
    pub fn ifc_type(self) -> &'static str {
        match self {
            FootingKind::Strip => "STRIP_FOOTING",
            FootingKind::Pad => "PAD_FOOTING",
        }
    }
}

/// A footing: a strip along a line, or a pad about a point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Footing {
    pub level: ElementId,
    pub kind: FootingKind,
    /// A strip's line starts here; a pad is centred here. Under a host the
    /// host's position wins (this is kept for when it is let go).
    pub start: DVec2,
    /// Where a strip's line ends. Not used by a pad.
    #[serde(default)]
    pub end: DVec2,
    /// A pad's turn, degrees counter-clockwise (added to its column's).
    #[serde(default)]
    pub rotation: f64,
    /// Across a strip, or a pad's side along its own x axis.
    pub width: f64,
    /// A pad's side along its own y axis. Not used by a strip.
    #[serde(default)]
    pub length: f64,
    /// Top to underside.
    pub thickness: f64,
    /// How far the top sits below its level, or below the base of the
    /// wall or column it is under.
    #[serde(default)]
    pub below: f64,
    #[serde(default)]
    pub material: Option<ElementId>,
    /// The wall (for a strip) or column (for a pad) it is under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<ElementId>,
}

/// Default sizes: a strip 600 wide and 300 thick, a pad 1200 square and
/// 400 thick, both with their top on the level.
pub const STRIP_WIDTH: f64 = 0.6;
pub const STRIP_THICKNESS: f64 = 0.3;
pub const PAD_SIZE: f64 = 1.2;
pub const PAD_THICKNESS: f64 = 0.4;

impl Footing {
    /// A strip footing on `level` along `start` → `end`.
    pub fn strip(level: ElementId, start: DVec2, end: DVec2) -> Self {
        Self {
            level,
            kind: FootingKind::Strip,
            start,
            end,
            rotation: 0.0,
            width: STRIP_WIDTH,
            length: 0.0,
            thickness: STRIP_THICKNESS,
            below: 0.0,
            material: None,
            host: None,
        }
    }

    /// A pad footing on `level` centred on `center`.
    pub fn pad(level: ElementId, center: DVec2) -> Self {
        Self {
            level,
            kind: FootingKind::Pad,
            start: center,
            end: center,
            rotation: 0.0,
            width: PAD_SIZE,
            length: PAD_SIZE,
            thickness: PAD_THICKNESS,
            below: 0.0,
            material: None,
            host: None,
        }
    }

    /// A strip footing under wall `id`, on the wall's level.
    pub fn under_wall(id: ElementId, wall: &Wall) -> Self {
        let mut f = Self::strip(wall.level, wall.start, wall.end);
        f.host = Some(id);
        // at least as wide as the wall, standing proud of it either side
        f.width = f.width.max(wall.thickness + 0.3);
        f
    }

    /// A pad footing under column `id`, on the column's base level.
    pub fn under_column(id: ElementId, column: &crate::Column) -> Self {
        let mut f = Self::pad(column.level, column.position);
        f.host = Some(id);
        f
    }

    /// The wall it is under, when it is a strip under one that exists.
    pub fn host_wall<'a>(&self, model: &'a Model) -> Option<&'a Wall> {
        match (self.kind, model.get(self.host?).map(|e| &e.kind)) {
            (FootingKind::Strip, Some(ElementKind::Wall(w))) => Some(w),
            _ => None,
        }
    }

    /// The column it is under, when it is a pad under one that exists.
    pub fn host_column<'a>(&self, model: &'a Model) -> Option<&'a crate::Column> {
        match (self.kind, model.get(self.host?).map(|e| &e.kind)) {
            (FootingKind::Pad, Some(ElementKind::Column(c))) => Some(c),
            _ => None,
        }
    }

    /// The level whose plan it shows on: its host's, or its own.
    pub fn plan_level(&self, model: &Model) -> ElementId {
        if let Some(w) = self.host_wall(model) {
            return w.level;
        }
        if let Some(c) = self.host_column(model) {
            return c.level;
        }
        self.level
    }

    /// A strip's line as built: along its wall's centreline, or its own.
    pub fn line(&self, model: &Model) -> (DVec2, DVec2) {
        match self.host_wall(model) {
            Some(w) => (w.start, w.end),
            None => (self.start, self.end),
        }
    }

    /// A pad's centre and turn as built.
    pub fn place(&self, model: &Model) -> (DVec2, f64) {
        match self.host_column(model) {
            Some(c) => (c.position, c.rotation + self.rotation),
            None => (self.start, self.rotation),
        }
    }

    /// The absolute height of the top: below the host's base, or the level.
    pub fn top(&self, model: &Model) -> f64 {
        let base = if let Some(w) = self.host_wall(model) {
            crate::walls::base_z(model, w)
        } else if let Some(c) = self.host_column(model) {
            c.span(model).0
        } else {
            crate::members::elevation(model, self.level)
        };
        base - self.below
    }

    /// A pad's outline in plan, counter-clockwise.
    pub fn pad_outline(&self, model: &Model) -> Vec<DVec2> {
        let (c, turn) = self.place(model);
        let r = DVec2::from_angle(turn.to_radians());
        let (x, y) = (self.width / 2.0, self.length / 2.0);
        [(-x, -y), (x, -y), (x, y), (-x, y)]
            .iter()
            .map(|&(a, b)| c + r.rotate(DVec2::new(a, b)))
            .collect()
    }

    /// The plan length of a strip.
    pub fn strip_length(&self, model: &Model) -> f64 {
        let (a, b) = self.line(model);
        a.distance(b)
    }

    /// The point grips and readouts start from: a strip's middle or a
    /// pad's centre, at its top.
    pub fn anchor(&self, model: &Model) -> DVec3 {
        let p = match self.kind {
            FootingKind::Strip => {
                let (a, b) = self.line(model);
                (a + b) / 2.0
            }
            FootingKind::Pad => self.place(model).0,
        };
        p.extend(self.top(model))
    }

    pub fn validate(&self) -> Result<(), String> {
        let positive = |v: f64, what: &str| {
            if v.is_finite() && v > 0.0 {
                Ok(())
            } else {
                Err(format!("{what} must be positive, got {v}"))
            }
        };
        positive(self.width, "footing width")?;
        positive(self.thickness, "footing thickness")?;
        if !(self.below.is_finite() && self.rotation.is_finite()) {
            return Err("footing depth and turn must be numbers".into());
        }
        if !(self.start.is_finite() && self.end.is_finite()) {
            return Err("footing position must be finite".into());
        }
        match self.kind {
            FootingKind::Pad => positive(self.length, "footing length"),
            FootingKind::Strip if self.host.is_none() => {
                positive(self.start.distance(self.end), "footing length")
            }
            FootingKind::Strip => Ok(()),
        }
    }
}

/// The plan outlines of every strip footing in the model, each in one
/// piece or more (crossing strips cut each other into pieces), mitred and
/// trimmed where strips meet.
pub fn strip_outlines(model: &Model) -> HashMap<ElementId, Vec<Vec<DVec2>>> {
    // each strip as a wall as wide as it, on the level its plan is on
    let mut lines: Vec<(ElementId, Wall, f64)> = vec![];
    for e in model.iter() {
        let ElementKind::Footing(f) = &e.kind else {
            continue;
        };
        if f.kind != FootingKind::Strip {
            continue;
        }
        let (a, b) = f.line(model);
        let mut w = Wall::new(f.plan_level(model), a, b, f.width, 1.0);
        w.disallow_join = [false; 2];
        // a free end runs on as far as the strip stands proud of its wall
        let proud = f
            .host_wall(model)
            .map_or(0.0, |hw| ((f.width - hw.thickness) / 2.0).max(0.0));
        lines.push((e.id, w, proud));
    }
    let walls: Vec<(ElementId, Wall)> = lines.iter().map(|(id, w, _)| (*id, w.clone())).collect();
    let joins = plain_joins(&walls);
    let mut out = HashMap::new();
    for (id, w, proud) in &lines {
        if w.length() < 1e-6 {
            continue;
        }
        let mut ends: WallEnds = joins.get(id).cloned().unwrap_or_default();
        for (k, p) in [(0, w.start), (1, w.end)] {
            let free = !lines.iter().any(|(oid, o, _)| {
                oid != id
                    && o.level == w.level
                    && segment_distance(p, o.start, o.end) <= (o.thickness + w.thickness) / 2.0
            });
            if free && *proud > 0.0 {
                if k == 0 {
                    ends.start = [*proud; 2];
                } else {
                    ends.end = [*proud; 2];
                }
            }
        }
        let d = w.direction();
        let n = d.perp();
        let t = w.thickness / 2.0;
        let pieces: Vec<Vec<DVec2>> = ends
            .local_outlines(w, (-t, t))
            .into_iter()
            .map(|piece| {
                piece
                    .into_iter()
                    .map(|q| w.start + d * q.x + n * q.y)
                    .collect()
            })
            .filter(|p: &Vec<DVec2>| p.len() >= 3)
            .collect();
        out.insert(*id, pieces);
    }
    out
}

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
    p.distance(a + d * t)
}

/// The plan outlines of footing `e`: a pad's rectangle, or a strip's
/// pieces (given the model's [`strip_outlines`], to work them out once).
pub fn outlines(
    model: &Model,
    e: &Element,
    strips: Option<&HashMap<ElementId, Vec<Vec<DVec2>>>>,
) -> Vec<Vec<DVec2>> {
    let ElementKind::Footing(f) = &e.kind else {
        return vec![];
    };
    match f.kind {
        FootingKind::Pad => vec![f.pad_outline(model)],
        FootingKind::Strip => match strips {
            Some(s) => s.get(&e.id).cloned().unwrap_or_default(),
            None => strip_outlines(model).remove(&e.id).unwrap_or_default(),
        },
    }
}

/// The footings already under `host`.
pub fn footings_under(model: &Model, host: ElementId) -> Vec<ElementId> {
    model
        .iter()
        .filter(|e| matches!(&e.kind, ElementKind::Footing(f) if f.host == Some(host)))
        .map(|e| e.id)
        .collect()
}

/// New footings under each of `hosts` that has none yet: a strip under
/// each wall, a pad under each column; `stamp` makes each an instance of
/// a type (or leaves it be). One undo step. Returns the edit and the new
/// footings' ids.
pub fn under(
    model: &Model,
    hosts: &[ElementId],
    stamp: &dyn Fn(&mut Element),
) -> (Transaction, Vec<ElementId>) {
    let mut tx = Transaction::new("Footings under selection");
    let mut ids = vec![];
    for id in hosts {
        if !footings_under(model, *id).is_empty() {
            continue;
        }
        let Some(e) = model.get(*id) else { continue };
        let (name, f) = match &e.kind {
            ElementKind::Wall(w) if w.curtain.is_none() => {
                ("Strip footing", Footing::under_wall(*id, w))
            }
            ElementKind::Column(c) => ("Pad footing", Footing::under_column(*id, c)),
            _ => continue,
        };
        let mut fe = Element::new(name, ElementKind::Footing(f));
        stamp(&mut fe);
        ids.push(fe.id);
        tx = tx.insert(fe);
    }
    (tx, ids)
}

/// The footing let go of its host: where the host put it, its own.
pub fn detached(model: &Model, f: &Footing) -> Footing {
    let mut out = f.clone();
    match f.kind {
        FootingKind::Strip => {
            let (a, b) = f.line(model);
            out.start = a;
            out.end = b;
            if let Some(w) = f.host_wall(model) {
                out.level = w.level;
                out.below = f.below - w.base_offset;
            }
        }
        FootingKind::Pad => {
            let (c, turn) = f.place(model);
            out.start = c;
            out.end = c;
            out.rotation = turn;
            if let Some(col) = f.host_column(model) {
                out.level = col.level;
                out.below = f.below - col.base_offset;
            }
        }
    }
    out.host = None;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Level, Profile};

    fn model_with(walls: &[(DVec2, DVec2)]) -> (Model, ElementId, Vec<ElementId>) {
        let mut m = Model::new();
        let level = Element::new(
            "L0",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        );
        let lid = level.id;
        let mut tx = Transaction::new("t").insert(level);
        let mut ids = vec![];
        for (a, b) in walls {
            let w = Element::new("W", ElementKind::Wall(Wall::new(lid, *a, *b, 0.3, 3.0)));
            ids.push(w.id);
            tx = tx.insert(w);
        }
        tx.apply(&mut m).unwrap();
        (m, lid, ids)
    }

    fn area(p: &[DVec2]) -> f64 {
        crate::polygon::signed_area2(p).abs() / 2.0
    }

    #[test]
    fn strips_under_an_l_of_walls_mitre_at_the_corner() {
        let (mut m, _, walls) = model_with(&[
            (DVec2::ZERO, DVec2::new(5.0, 0.0)),
            (DVec2::new(5.0, 0.0), DVec2::new(5.0, 4.0)),
        ]);
        let (tx, ids) = under(&m, &walls, &|_| {});
        tx.apply(&mut m).unwrap();
        assert_eq!(ids.len(), 2);
        let outlines = strip_outlines(&m);
        let a = &outlines[&ids[0]][0];
        // 600 wide, mitred to the outer corner at x = 5.3, and proud of
        // the free end by (600 - 300) / 2
        let xmax = a.iter().map(|p| p.x).fold(f64::MIN, f64::max);
        let xmin = a.iter().map(|p| p.x).fold(f64::MAX, f64::min);
        assert!((xmax - 5.3).abs() < 1e-6, "{a:?}");
        assert!((xmin + 0.15).abs() < 1e-6, "{a:?}");
        // the two strips together cover the L with no overlap
        let b = &outlines[&ids[1]][0];
        let total = area(a) + area(b);
        // each as long as its faces on average: (5.45 + 4.85) / 2 and (4.45 + 3.85) / 2
        let expected = 5.15 * 0.6 + 4.15 * 0.6;
        assert!((total - expected).abs() < 1e-6, "{total} vs {expected}");
    }

    #[test]
    fn a_strip_follows_its_wall_and_a_pad_its_column() {
        let (mut m, lid, walls) = model_with(&[(DVec2::ZERO, DVec2::new(4.0, 0.0))]);
        let col = Element::new(
            "C",
            ElementKind::Column(crate::Column::new(
                lid,
                DVec2::new(2.0, 2.0),
                Profile::Rectangle {
                    width: 0.3,
                    depth: 0.3,
                },
            )),
        );
        let cid = col.id;
        Transaction::new("c").insert(col).apply(&mut m).unwrap();
        let (tx, ids) = under(&m, &[walls[0], cid], &|_| {});
        tx.apply(&mut m).unwrap();
        // a second time adds nothing
        assert!(under(&m, &[walls[0], cid], &|_| {}).1.is_empty());
        let mut w = m.get(walls[0]).unwrap().clone();
        if let ElementKind::Wall(w) = &mut w.kind {
            w.end = DVec2::new(6.0, 0.0);
            w.base_offset = 0.5;
        }
        let mut c = m.get(cid).unwrap().clone();
        if let ElementKind::Column(c) = &mut c.kind {
            c.position = DVec2::new(3.0, 3.0);
        }
        Transaction::new("move")
            .replace(m.get(walls[0]).unwrap().clone(), w)
            .replace(m.get(cid).unwrap().clone(), c)
            .apply(&mut m)
            .unwrap();
        let ElementKind::Footing(strip) = &m.get(ids[0]).unwrap().kind else {
            panic!()
        };
        assert_eq!(strip.line(&m).1, DVec2::new(6.0, 0.0));
        assert!((strip.top(&m) - 0.5).abs() < 1e-9);
        let ElementKind::Footing(pad) = &m.get(ids[1]).unwrap().kind else {
            panic!()
        };
        assert_eq!(pad.place(&m).0, DVec2::new(3.0, 3.0));
        // let go, it stays where it was
        let free = detached(&m, pad);
        assert_eq!(free.place(&m).0, DVec2::new(3.0, 3.0));
        assert!(free.host.is_none());
        // deleting the wall deletes its footing
        let tx = Transaction::new("del").remove_cascade(&m, walls[0]);
        tx.apply(&mut m).unwrap();
        assert!(m.get(ids[0]).is_none());
        assert!(m.get(ids[1]).is_some());
    }

    #[test]
    fn a_tee_stops_at_the_side_of_the_through_strip() {
        let (mut m, _, walls) = model_with(&[
            (DVec2::new(0.0, 0.0), DVec2::new(6.0, 0.0)),
            (DVec2::new(3.0, 0.0), DVec2::new(3.0, 4.0)),
        ]);
        let (tx, ids) = under(&m, &walls, &|_| {});
        tx.apply(&mut m).unwrap();
        let o = strip_outlines(&m);
        let stem = &o[&ids[1]][0];
        let ymin = stem.iter().map(|p| p.y).fold(f64::MAX, f64::min);
        assert!((ymin - 0.3).abs() < 1e-6, "{stem:?}");
    }

    #[test]
    fn footings_save_and_read_back_and_types_fit_their_kind() {
        let lid = ElementId::new();
        let f = Footing::pad(lid, DVec2::new(1.0, 2.0));
        let json = serde_json::to_string(&f).unwrap();
        assert!(!json.contains("host"));
        assert_eq!(serde_json::from_str::<Footing>(&json).unwrap(), f);
        // the fewest fields still read
        let bare = format!(
            r#"{{"level":"{}","kind":"Strip","start":[0.0,0.0],"width":0.6,"thickness":0.3}}"#,
            lid.0
        );
        let s: Footing = serde_json::from_str(&bare).unwrap();
        assert_eq!((s.below, s.length, s.host), (0.0, 0.0, None));
        let strip_type = crate::types::TypeParams::Footing(crate::types::FootingType {
            kind: FootingKind::Strip,
            width: 0.9,
            length: 0.0,
            thickness: 0.4,
            material: None,
        });
        assert!(!crate::types::fits(
            &strip_type,
            &ElementKind::Footing(f.clone())
        ));
        assert!(crate::types::fits(
            &strip_type,
            &ElementKind::Footing(s.clone())
        ));
        let mut e = Element::new("S", ElementKind::Footing(s));
        crate::types::apply(&Model::new(), &strip_type, &mut e, &[]);
        let ElementKind::Footing(s) = &e.kind else {
            panic!()
        };
        assert_eq!((s.width, s.thickness), (0.9, 0.4));
    }
}
