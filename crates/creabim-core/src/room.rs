//! Rooms (IFC's IfcSpace), the room separation lines that close them off
//! where there is no wall, and the ceilings hung in them (IfcCovering
//! CEILING).
//!
//! A room is placed by a point inside it. Its outline is never stored: it
//! is found again from the walls and room separators on its level each
//! time it is asked for, to the walls' inner faces, so moving a wall
//! changes the room's area, perimeter and tag at once. A room can also be
//! drawn with an outline of its own, for a space no wall closes off (a
//! terrace, a balcony); then the outline is kept and edited like a slab's.
//!
//! A ceiling hangs `height` above its level, `thickness` thick upwards,
//! either over a room (following the room's outline as walls move) or over
//! an outline of its own. Fixtures hung from the ceiling hang from it, and
//! walls can attach their tops to its underside (see [`crate::walls`]).

use crate::{Element, ElementId, ElementKind, Model};
use glam::DVec2;
use serde::{Deserialize, Serialize};

fn is_blank(s: &str) -> bool {
    s.is_empty()
}

/// A room: a space on a level, named by its element's name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Room {
    pub level: ElementId,
    /// A point inside it. With no outline of its own, the room is the
    /// space the walls and room separators of its level close off round
    /// this point; its tag sits here too.
    pub at: DVec2,
    /// An outline drawn for it, counter-clockwise; empty when the walls
    /// round `at` bound it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundary: Vec<DVec2>,
    /// Its number, as schedules and tags show it ("0.01").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub number: String,
    /// Floor, wall and ceiling finishes, as free text.
    #[serde(default, skip_serializing_if = "is_blank")]
    pub floor_finish: String,
    #[serde(default, skip_serializing_if = "is_blank")]
    pub wall_finish: String,
    #[serde(default, skip_serializing_if = "is_blank")]
    pub ceiling_finish: String,
    /// What it is used for ("Bedroom", "Office"), IFC's occupancy type.
    #[serde(default, skip_serializing_if = "is_blank")]
    pub occupancy: String,
    /// The department or dwelling it belongs to.
    #[serde(default, skip_serializing_if = "is_blank")]
    pub department: String,
    /// Height of its top above its level; none reaches up to the ceiling
    /// over it, or to the level's floor-to-floor height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
}

impl Room {
    /// A room round `at` on `level`, bounded by the walls there.
    pub fn new(level: ElementId, at: DVec2) -> Self {
        Self {
            level,
            at,
            ..Default::default()
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.at.is_finite() || self.boundary.iter().any(|p| !p.is_finite()) {
            return Err("a room's point and outline must be finite".into());
        }
        if !self.boundary.is_empty() && self.boundary.len() < 3 {
            return Err("a drawn room needs at least 3 corners".into());
        }
        if self.height.is_some_and(|h| !(h.is_finite() && h > 0.0)) {
            return Err("a room's height must be positive".into());
        }
        Ok(())
    }

    /// Is its outline drawn rather than found from the walls?
    pub fn is_drawn(&self) -> bool {
        !self.boundary.is_empty()
    }
}

/// A room separation line: closes rooms off where there is no wall, as
/// between a kitchen and the living room it opens into. Drawn in plans
/// only; it has no body.
/// One thing a room tag can show. A tag lists the fields it shows; one
/// that lists none shows the room's name, number and area.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoomField {
    Name,
    Number,
    Area,
    Perimeter,
    Height,
    Level,
    FloorFinish,
    WallFinish,
    CeilingFinish,
    Occupancy,
    Department,
}

impl RoomField {
    pub const ALL: [RoomField; 11] = [
        RoomField::Name,
        RoomField::Number,
        RoomField::Area,
        RoomField::Perimeter,
        RoomField::Height,
        RoomField::Level,
        RoomField::FloorFinish,
        RoomField::WallFinish,
        RoomField::CeilingFinish,
        RoomField::Occupancy,
        RoomField::Department,
    ];
    /// What a tag shows when it lists no fields.
    pub const DEFAULT: [RoomField; 3] = [RoomField::Name, RoomField::Number, RoomField::Area];

    pub fn label(self) -> &'static str {
        match self {
            RoomField::Name => "Name",
            RoomField::Number => "Number",
            RoomField::Area => "Area",
            RoomField::Perimeter => "Perimeter",
            RoomField::Height => "Height",
            RoomField::Level => "Level",
            RoomField::FloorFinish => "Floor finish",
            RoomField::WallFinish => "Wall finish",
            RoomField::CeilingFinish => "Ceiling finish",
            RoomField::Occupancy => "Occupancy",
            RoomField::Department => "Department",
        }
    }

    /// This field of a room as its tag writes it; empty when the room has
    /// none, and "Not enclosed" for the area or perimeter of a room its
    /// walls don't close off.
    pub fn value(self, model: &Model, room: ElementId) -> String {
        let Some(e) = model.get(room) else {
            return String::new();
        };
        let ElementKind::Room(r) = &e.kind else {
            return String::new();
        };
        let units = model.units();
        match self {
            RoomField::Name => e.name.clone(),
            RoomField::Number => r.number.clone(),
            RoomField::Area => shape_of(model, r).map_or("Not enclosed".into(), |s| {
                units.area(net_area(model, r, &s))
            }),
            RoomField::Perimeter => {
                shape_of(model, r).map_or("Not enclosed".into(), |s| units.dimension(s.perimeter()))
            }
            RoomField::Height => units.dimension(height(model, r)),
            RoomField::Level => model
                .get(r.level)
                .map(|l| l.name.clone())
                .unwrap_or_default(),
            RoomField::FloorFinish => r.floor_finish.clone(),
            RoomField::WallFinish => r.wall_finish.clone(),
            RoomField::CeilingFinish => r.ceiling_finish.clone(),
            RoomField::Occupancy => r.occupancy.clone(),
            RoomField::Department => r.department.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Separator {
    pub level: ElementId,
    pub a: DVec2,
    pub b: DVec2,
}

impl Separator {
    pub fn validate(&self) -> Result<(), String> {
        if self.a.is_finite() && self.b.is_finite() && self.a.distance(self.b) > 1e-6 {
            Ok(())
        } else {
            Err("a room separator needs two different points".into())
        }
    }
}

/// A ceiling: a flat layer hung `height` above its level, `thickness`
/// thick upwards from its underside.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ceiling {
    pub level: ElementId,
    /// Its outline, counter-clockwise; empty when it follows `room`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundary: Vec<DVec2>,
    /// The room whose outline it follows, when it has none of its own.
    /// Soft: when the room goes the ceiling keeps nothing to stand on and
    /// is left out until it is given an outline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<ElementId>,
    /// Height of its underside above the level.
    pub height: f64,
    pub thickness: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<ElementId>,
    /// The tile size of a suspended ceiling's grid, drawn in ceiling
    /// plans; 0 for a plain (plastered) one.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub grid: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// The height new ceilings hang at, and their thickness.
pub const CEILING_HEIGHT: f64 = 2.5;
pub const CEILING_THICKNESS: f64 = 0.0125;

impl Ceiling {
    /// A ceiling over `room`, following its outline.
    pub fn in_room(level: ElementId, room: ElementId, height: f64) -> Self {
        Self {
            level,
            boundary: vec![],
            room: Some(room),
            height,
            thickness: CEILING_THICKNESS,
            material: None,
            grid: 0.0,
        }
    }

    /// A ceiling with an outline of its own.
    pub fn drawn(level: ElementId, mut boundary: Vec<DVec2>, height: f64) -> Self {
        if crate::polygon::signed_area2(&boundary) < 0.0 {
            boundary.reverse();
        }
        Self {
            level,
            boundary,
            room: None,
            height,
            thickness: CEILING_THICKNESS,
            material: None,
            grid: 0.0,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(self.thickness.is_finite() && self.thickness > 0.0) {
            return Err(format!(
                "ceiling thickness must be positive, got {}",
                self.thickness
            ));
        }
        if !self.height.is_finite() {
            return Err("a ceiling's height must be a number".into());
        }
        if !(self.grid.is_finite() && (self.grid == 0.0 || self.grid >= 0.1)) {
            return Err("a ceiling's grid must be 0 (none) or at least 100 mm".into());
        }
        if self.boundary.iter().any(|p| !p.is_finite()) {
            return Err("a ceiling's outline must be finite".into());
        }
        if self.boundary.is_empty() && self.room.is_none() {
            return Err("a ceiling needs an outline or a room".into());
        }
        if !self.boundary.is_empty() && self.boundary.len() < 3 {
            return Err("a ceiling's outline needs at least 3 corners".into());
        }
        Ok(())
    }

    /// Absolute heights of its underside and its top.
    pub fn span(&self, model: &Model) -> (f64, f64) {
        let z = model.level(self.level).map_or(0.0, |l| l.elevation) + self.height;
        (z, z + self.thickness)
    }

    /// Its outline in plan: its own, or its room's as the walls stand now.
    pub fn outline(&self, model: &Model) -> Option<Vec<DVec2>> {
        if !self.boundary.is_empty() {
            return Some(self.boundary.clone());
        }
        outline(model, self.room?)
    }
}

// ------------------------------------------------------------ boundaries

/// Where a room's outline comes from, edge by edge.
#[derive(Clone, Debug, PartialEq)]
pub struct RoomShape {
    /// The outline, counter-clockwise.
    pub outline: Vec<DVec2>,
    /// The wall or room separator each edge (corner `i` to `i + 1`) runs
    /// along, if any.
    pub bounded_by: Vec<Option<ElementId>>,
}

impl RoomShape {
    pub fn area(&self) -> f64 {
        crate::polygon::signed_area2(&self.outline).abs() / 2.0
    }

    pub fn perimeter(&self) -> f64 {
        perimeter(&self.outline)
    }
}

/// The length round a closed outline.
pub fn perimeter(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].distance(poly[(i + 1) % n])).sum()
}

/// The lines that bound rooms on `level`: both faces of every wall, run on
/// at their ends far enough to meet the faces of the walls they stop
/// against, and every room separator, run on as far as the walls are so
/// that one drawn to a face, to a wall's centre line or a snap short of
/// the face still crosses it.
fn bounding_lines(model: &Model, level: ElementId) -> Vec<(DVec2, DVec2, ElementId)> {
    let walls: Vec<(ElementId, &crate::Wall)> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Wall(w) if w.level == level && w.length() > 1e-3 => Some((e.id, w)),
            _ => None,
        })
        .collect();
    let reach = walls
        .iter()
        .map(|w| w.1.thickness / 2.0)
        .fold(0.0, f64::max)
        + 0.01;
    let mut out = Vec::new();
    for (id, w) in &walls {
        let d = w.direction();
        let n = d.perp() * w.thickness / 2.0;
        let (a, b) = (w.start - d * reach, w.end + d * reach);
        out.push((a + n, b + n, *id));
        out.push((a - n, b - n, *id));
    }
    for e in model.iter() {
        if let ElementKind::RoomSeparator(s) = &e.kind {
            if s.level == level && s.a.distance(s.b) > 1e-6 {
                let d = (s.b - s.a).normalize();
                out.push((s.a - d * reach.max(0.1), s.b + d * reach.max(0.1), e.id));
            }
        }
    }
    out
}

/// The space the walls and room separators of `level` close off round
/// `p`, to the walls' faces; `None` when nothing closes it off.
pub fn enclosed(model: &Model, level: ElementId, p: DVec2) -> Option<RoomShape> {
    let lines = bounding_lines(model, level);
    let segs: Vec<(DVec2, DVec2)> = lines.iter().map(|l| (l.0, l.1)).collect();
    let outline = crate::slab::room_in_lines(&segs, p)?;
    // a point inside a wall finds the wall itself: that is no room
    let thin = crate::polygon::signed_area2(&outline).abs() / 2.0 < 0.05;
    if thin {
        return None;
    }
    let n = outline.len();
    let bounded_by = (0..n)
        .map(|i| {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            let mid = (a + b) / 2.0;
            lines
                .iter()
                .find(|l| point_on_segment(mid, l.0, l.1) && point_on_segment(a, l.0, l.1))
                .or_else(|| lines.iter().find(|l| point_on_segment(mid, l.0, l.1)))
                .map(|l| l.2)
        })
        .collect();
    Some(RoomShape {
        outline,
        bounded_by,
    })
}

fn point_on_segment(p: DVec2, a: DVec2, b: DVec2) -> bool {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < 1e-18 {
        return false;
    }
    let t = (p - a).dot(ab) / len2;
    (-1e-9..=1.0 + 1e-9).contains(&t) && (a + ab * t).distance(p) < 1e-6
}

/// A room's shape as the model stands: its drawn outline, or the space its
/// walls close off. `None` for a room that is not enclosed (or not a room).
pub fn shape(model: &Model, room: ElementId) -> Option<RoomShape> {
    let ElementKind::Room(r) = &model.get(room)?.kind else {
        return None;
    };
    shape_of(model, r)
}

/// [`shape`], for a room that need not be in the model yet.
pub fn shape_of(model: &Model, r: &Room) -> Option<RoomShape> {
    if r.is_drawn() {
        let mut outline = r.boundary.clone();
        if crate::polygon::signed_area2(&outline) < 0.0 {
            outline.reverse();
        }
        let n = outline.len();
        return Some(RoomShape {
            outline,
            bounded_by: vec![None; n],
        });
    }
    enclosed(model, r.level, r.at)
}

/// A room's outline, counter-clockwise.
pub fn outline(model: &Model, room: ElementId) -> Option<Vec<DVec2>> {
    shape(model, room).map(|s| s.outline)
}

/// A room's floor area, 0 when it is not enclosed: inside its outline,
/// less the shafts (a stair well, a lift) that open its floor to the
/// storey below.
pub fn area(model: &Model, room: ElementId) -> f64 {
    match model.get(room).map(|e| &e.kind) {
        Some(ElementKind::Room(r)) => shape_of(model, r).map_or(0.0, |s| net_area(model, r, &s)),
        _ => 0.0,
    }
}

/// The floor area of `r`, shaped `s`, less the shafts through its floor.
pub fn net_area(model: &Model, r: &Room, s: &RoomShape) -> f64 {
    let floor = model.level(r.level).map_or(0.0, |l| l.elevation);
    let voids: f64 = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Shaft(sh) => {
                let (z0, z1) = sh.span(model);
                let n = sh.boundary.len().max(1) as f64;
                let middle = sh.boundary.iter().copied().sum::<DVec2>() / n;
                (z0 < floor - 1e-6
                    && z1 >= floor - 1e-6
                    && crate::polygon::contains(&s.outline, middle))
                .then(|| crate::polygon::signed_area2(&sh.boundary).abs() / 2.0)
            }
            _ => None,
        })
        .sum();
    (s.area() - voids).max(0.0)
}

/// How tall a room is: its own height, else up to the underside of the
/// lowest ceiling over its point, else its level's floor-to-floor height.
pub fn height(model: &Model, r: &Room) -> f64 {
    if let Some(h) = r.height {
        return h;
    }
    let Some(l) = model.level(r.level) else {
        return 3.0;
    };
    ceiling_over(model, r.level, r.at, l.elevation + l.height + 1e-6)
        .map_or(l.height, |(_, z)| z - l.elevation)
}

/// The lowest ceiling over `p` among those on `level` whose underside is
/// below `below`: its id and the height of its underside.
pub fn ceiling_over(
    model: &Model,
    level: ElementId,
    p: DVec2,
    below: f64,
) -> Option<(ElementId, f64)> {
    let mut best: Option<(ElementId, f64)> = None;
    for e in model.iter() {
        let ElementKind::Ceiling(c) = &e.kind else {
            continue;
        };
        if c.level != level {
            continue;
        }
        let (z, _) = c.span(model);
        if z >= below || best.is_some_and(|b| b.1 <= z) {
            continue;
        }
        if c.outline(model)
            .is_some_and(|o| crate::polygon::contains(&o, p))
        {
            best = Some((e.id, z));
        }
    }
    best
}

/// The number for a new room on `level`: one more than the highest number
/// there ("0.03" after "0.02"), else the first on that storey ("1.01" on
/// the first storey above the ground).
pub fn next_number(model: &Model, level: ElementId) -> String {
    let mut best: Option<(String, u32, usize)> = None;
    for e in model.iter() {
        let ElementKind::Room(r) = &e.kind else {
            continue;
        };
        if r.level != level {
            continue;
        }
        let digits = r
            .number
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .count();
        if digits == 0 {
            continue;
        }
        let (head, tail) = r.number.split_at(r.number.len() - digits);
        let Ok(n) = tail.parse::<u32>() else { continue };
        if best.as_ref().is_none_or(|b| n > b.1) {
            best = Some((head.to_string(), n, digits));
        }
    }
    if let Some((head, n, width)) = best {
        return format!("{head}{:0width$}", n + 1);
    }
    let sorted = crate::levels::sorted(model);
    let ground = sorted
        .iter()
        .position(|l| l.2.elevation >= -1e-6)
        .unwrap_or(0) as i64;
    let here = sorted.iter().position(|l| l.0 == level).unwrap_or(0) as i64;
    format!("{}.01", here - ground)
}

/// Whether a note reads like a room label: a name, then its area on the
/// next line ("LIVING\n37.2 m²").
fn room_label(text: &str) -> Option<&str> {
    let mut lines = text.lines().map(str::trim);
    let name = lines.next().filter(|l| !l.is_empty())?;
    let area = lines.next()?;
    let number = area
        .strip_suffix("m²")
        .or_else(|| area.strip_suffix("m2"))?;
    number.trim().replace(',', ".").parse::<f64>().ok()?;
    lines.next().is_none().then_some(name)
}

/// "PRIMARY BEDROOM" as a name: "Primary Bedroom".
fn title_case(name: &str) -> String {
    if name.chars().any(char::is_lowercase) {
        return name.to_string();
    }
    let mut out = String::with_capacity(name.len());
    let mut start = true;
    for c in name.chars() {
        if start {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        start = !c.is_alphanumeric();
    }
    out
}

/// Turns the room labels written as notes in plans (a name over an area,
/// the way a project made before rooms existed wrote them) into rooms with
/// tags in their place, wherever the walls close the label off. Labels the
/// walls don't close off are left as notes and named in the second list.
pub fn from_notes(model: &Model) -> (crate::Transaction, Vec<String>) {
    let mut tx = crate::Transaction::new("Rooms from room notes");
    let mut skipped = Vec::new();
    let mut numbers: std::collections::HashMap<ElementId, String> = Default::default();
    let mut placed: Vec<(ElementId, Vec<DVec2>)> = Vec::new();
    for e in model.iter() {
        let ElementKind::Note(n) = &e.kind else {
            continue;
        };
        let Some(name) = room_label(&n.text) else {
            continue;
        };
        let Some(ElementKind::View(v)) = model.get(n.view).map(|v| &v.kind) else {
            continue;
        };
        let crate::ViewKind::Plan { level, .. } = v.kind else {
            continue;
        };
        // where the label's lines are centred: across its widest line, half
        // a line below its first
        let look = crate::text_style(model, n.style, n.color);
        let mm = v.scale / 1000.0;
        let widest = n.text.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
        let middle = n.at + DVec2::new(widest * 0.6 * look.size * mm / 2.0, -0.75 * look.size * mm);
        let Some((at, shape)) = [middle, n.at]
            .into_iter()
            .find_map(|p| enclosed(model, level, p).map(|s| (p, s)))
        else {
            skipped.push(name.to_string());
            continue;
        };
        // a room already there (or made from another label) keeps it
        let taken = model.iter().any(|x| match &x.kind {
            ElementKind::Room(r) => {
                r.level == level
                    && outline(model, x.id).is_some_and(|o| crate::polygon::contains(&o, at))
            }
            _ => false,
        }) || placed
            .iter()
            .any(|(l, o)| *l == level && crate::polygon::contains(o, at));
        if taken {
            skipped.push(name.to_string());
            continue;
        }
        let number = match numbers.get(&level) {
            Some(last) => bump(last),
            None => next_number(model, level),
        };
        numbers.insert(level, number.clone());
        let mut r = Room::new(level, at);
        r.number = number;
        let room = Element::new(title_case(name), ElementKind::Room(r));
        let mut tag = crate::Tag::new(n.view, room.id, middle - at);
        tag.style = n.style;
        tag.color = n.color;
        placed.push((level, shape.outline));
        tx = tx
            .insert(room.clone())
            .insert(Element::new("Room tag", ElementKind::Tag(tag)))
            .remove_cascade(model, e.id);
    }
    (tx, skipped)
}

/// The number after `number`: its trailing digits counted up.
fn bump(number: &str) -> String {
    let digits = number
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .count();
    let (head, tail) = number.split_at(number.len() - digits);
    match tail.parse::<u32>() {
        Ok(n) => format!("{head}{:0digits$}", n + 1),
        Err(_) => format!("{number}.1"),
    }
}

/// Rooms on `level` whose point another room's outline already holds:
/// two rooms placed in the same space.
pub fn overlapping(model: &Model) -> Vec<(ElementId, ElementId)> {
    let rooms: Vec<(ElementId, &Room, Option<RoomShape>)> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Room(r) => Some((e.id, r, shape_of(model, r))),
            _ => None,
        })
        .collect();
    let mut out = vec![];
    for (i, a) in rooms.iter().enumerate() {
        for b in &rooms[i + 1..] {
            if a.1.level != b.1.level {
                continue;
            }
            let holds = |s: &Option<RoomShape>, p: DVec2| {
                s.as_ref()
                    .is_some_and(|s| crate::polygon::contains(&s.outline, p))
            };
            if holds(&a.2, b.1.at) || holds(&b.2, a.1.at) {
                out.push((a.0, b.0));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Element, Level, Transaction, Wall};

    fn house() -> (Document, ElementId, Vec<ElementId>) {
        let lv = Element::new(
            "Ground",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        );
        let l = lv.id;
        let mut tx = Transaction::new("walls").insert(lv);
        let mut ids = vec![];
        for (a, b) in [
            ((0.0, 0.0), (6.0, 0.0)),
            ((6.0, 0.0), (6.0, 4.0)),
            ((6.0, 4.0), (0.0, 4.0)),
            ((0.0, 4.0), (0.0, 0.0)),
        ] {
            let w = Element::new(
                "Wall",
                ElementKind::Wall(Wall::new(
                    l,
                    DVec2::new(a.0, a.1),
                    DVec2::new(b.0, b.1),
                    0.2,
                    3.0,
                )),
            );
            ids.push(w.id);
            tx = tx.insert(w);
        }
        let mut doc = Document::new();
        doc.commit(tx).unwrap();
        (doc, l, ids)
    }

    #[test]
    fn a_room_fills_to_the_inner_faces_and_follows_its_walls() {
        let (mut doc, l, walls) = house();
        let room = Element::new(
            "Living",
            ElementKind::Room(Room::new(l, DVec2::new(2.0, 2.0))),
        );
        let rid = room.id;
        doc.commit(Transaction::new("room").insert(room)).unwrap();
        let s = shape(doc.model(), rid).expect("enclosed");
        // inner faces: 5.8 × 3.8
        assert!((s.area() - 5.8 * 3.8).abs() < 1e-9, "{}", s.area());
        assert!((s.perimeter() - 2.0 * (5.8 + 3.8)).abs() < 1e-9);
        // every edge runs along a wall
        assert!(s
            .bounded_by
            .iter()
            .all(|b| b.is_some_and(|b| walls.contains(&b))));
        // move the east wall out a metre (and stretch the two it meets)
        for (i, id) in walls.iter().enumerate() {
            doc.edit(*id, "Move east wall", |e| {
                if let ElementKind::Wall(w) = &mut e.kind {
                    for p in [&mut w.start, &mut w.end] {
                        if p.x > 5.0 {
                            p.x += 1.0;
                        }
                    }
                }
            })
            .unwrap_or_else(|_| panic!("wall {i}"));
        }
        assert!((area(doc.model(), rid) - 6.8 * 3.8).abs() < 1e-9);
        // a separator splits it
        let sep = Element::new(
            "Separator",
            ElementKind::RoomSeparator(Separator {
                level: l,
                a: DVec2::new(3.0, 0.1),
                b: DVec2::new(3.0, 3.9),
            }),
        );
        doc.commit(Transaction::new("sep").insert(sep)).unwrap();
        let s = shape(doc.model(), rid).unwrap();
        assert!((s.area() - 2.9 * 3.8).abs() < 1e-9, "{}", s.area());
        // a room outside the walls isn't enclosed
        let out = Room::new(l, DVec2::new(-3.0, 1.0));
        assert!(shape_of(doc.model(), &out).is_none());
    }

    #[test]
    fn a_separator_drawn_a_little_short_of_the_walls_still_splits_a_room() {
        let (mut doc, l, _) = house();
        let room = Element::new(
            "Room",
            ElementKind::Room(Room::new(l, DVec2::new(1.0, 2.0))),
        );
        let rid = room.id;
        doc.commit(Transaction::new("room").insert(room)).unwrap();
        let whole = area(doc.model(), rid);
        // a snap 50 mm short of each face
        let sep = Element::new(
            "Separator",
            ElementKind::RoomSeparator(Separator {
                level: l,
                a: DVec2::new(3.0, 0.15),
                b: DVec2::new(3.02, 3.85),
            }),
        );
        doc.commit(Transaction::new("sep").insert(sep)).unwrap();
        let split = area(doc.model(), rid);
        assert!(split < whole * 0.6, "{split} of {whole}");
    }

    #[test]
    fn a_room_is_as_tall_as_the_ceiling_over_it() {
        let (mut doc, l, _) = house();
        let room = Element::new(
            "Room",
            ElementKind::Room(Room::new(l, DVec2::new(2.0, 2.0))),
        );
        let rid = room.id;
        let r = match &room.kind {
            ElementKind::Room(r) => r.clone(),
            _ => unreachable!(),
        };
        doc.commit(Transaction::new("room").insert(room)).unwrap();
        assert_eq!(height(doc.model(), &r), 3.0);
        let c = Element::new(
            "Ceiling",
            ElementKind::Ceiling(Ceiling::in_room(l, rid, 2.4)),
        );
        doc.commit(Transaction::new("ceiling").insert(c)).unwrap();
        assert!((height(doc.model(), &r) - 2.4).abs() < 1e-12);
        let o = match &doc
            .model()
            .iter()
            .find(|e| e.name == "Ceiling")
            .unwrap()
            .kind
        {
            ElementKind::Ceiling(c) => c.outline(doc.model()).unwrap(),
            _ => unreachable!(),
        };
        assert!((crate::polygon::signed_area2(&o) / 2.0 - 5.8 * 3.8).abs() < 1e-9);
    }

    #[test]
    fn room_numbers_count_on() {
        let (mut doc, l, _) = house();
        assert_eq!(next_number(doc.model(), l), "0.01");
        let mut r = Room::new(l, DVec2::new(2.0, 2.0));
        r.number = "0.09".into();
        doc.commit(Transaction::new("room").insert(Element::new("A", ElementKind::Room(r))))
            .unwrap();
        assert_eq!(next_number(doc.model(), l), "0.10");
    }

    #[test]
    fn room_notes_become_rooms_with_tags() {
        let m = crate::starter::starter_house();
        let rooms: Vec<ElementId> = m
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Room(_)))
            .map(|e| e.id)
            .collect();
        let mut doc = crate::Document::from_model(m.clone());
        doc.commit(crate::Transaction::new("No rooms").remove_all_cascade(&m, &rooms))
            .unwrap();
        let plan = doc
            .model()
            .iter()
            .find(|e| e.name == "Ground Floor plan")
            .unwrap()
            .id;
        let note = |at: DVec2, text: &str| {
            Element::new("Note", ElementKind::Note(crate::Note::new(plan, at, text)))
        };
        doc.commit(
            crate::Transaction::new("Labels")
                .insert(note(DVec2::new(1.5, 3.5), "STUDY\n19.9 m²"))
                .insert(note(DVec2::new(1.2, 3.0), "SECOND STUDY\n19.9 m²"))
                .insert(note(DVec2::new(9.7, 1.3), "STORE\n4.0 m²"))
                .insert(note(DVec2::new(-20.0, -20.0), "GARDEN\n100 m²"))
                .insert(note(DVec2::new(5.0, 1.7), "just a note")),
        )
        .unwrap();
        let (tx, skipped) = from_notes(doc.model());
        doc.commit(tx).unwrap();
        let m = doc.model();
        let made: Vec<(&str, &Room)> = m
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Room(r) => Some((e.name.as_str(), r)),
                _ => None,
            })
            .collect();
        // the study once; a second label in it, and one outside, stay notes
        assert_eq!(made.len(), 2, "{made:?}");
        assert_eq!(made[0].0, "Study");
        assert_eq!(
            (made[0].1.number.as_str(), made[1].1.number.as_str()),
            ("0.01", "0.02")
        );
        assert_eq!(
            skipped,
            vec!["SECOND STUDY".to_string(), "GARDEN".to_string()]
        );
        let tags = m
            .iter()
            .filter(|e| matches!(&e.kind, ElementKind::Tag(t) if model_room(m, t.host)))
            .count();
        assert_eq!(tags, 2);
        assert!(m
            .iter()
            .any(|e| matches!(&e.kind, ElementKind::Note(n) if n.text == "just a note")));
        assert!(!m
            .iter()
            .any(|e| matches!(&e.kind, ElementKind::Note(n) if n.text.starts_with("STUDY"))));
    }

    fn model_room(m: &Model, id: ElementId) -> bool {
        matches!(m.get(id).map(|e| &e.kind), Some(ElementKind::Room(_)))
    }

    #[test]
    fn tags_from_before_room_fields_still_read() {
        let id = serde_json::to_string(&ElementId::new()).unwrap();
        let t: crate::Tag = serde_json::from_str(&format!(
            r#"{{"view":{id},"host":{id},"offset":[0.0,0.0]}}"#
        ))
        .unwrap();
        assert!(t.fields.is_empty());
        assert!(!serde_json::to_string(&t).unwrap().contains("fields"));
    }

    #[test]
    fn rooms_and_ceilings_read_with_only_what_they_need() {
        let level = serde_json::to_string(&ElementId::new()).unwrap();
        let r: Room =
            serde_json::from_str(&format!(r#"{{"level":{level},"at":[1.0,2.0]}}"#)).unwrap();
        assert!(r.number.is_empty() && r.height.is_none() && !r.is_drawn());
        let text = serde_json::to_string(&r).unwrap();
        assert!(!text.contains("finish") && !text.contains("boundary"));
        let c: Ceiling = serde_json::from_str(&format!(
            r#"{{"level":{level},"boundary":[[0,0],[1,0],[1,1]],"height":2.4,"thickness":0.02}}"#
        ))
        .unwrap();
        assert!(c.room.is_none() && c.material.is_none());
        c.validate().unwrap();
    }
}
