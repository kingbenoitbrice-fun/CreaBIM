//! Building elements with a parametric shape: columns, beams, roofs, stairs
//! and railings. Each one hangs off a level, like walls and slabs, and is
//! described by a few numbers; their meshes are built in `creabim-geom`.
//!
//! This module holds the data and the plain geometry (stair risers and
//! flights, roof planes, railing paths) so it can be tested without meshes.

use crate::{Element, ElementId, ElementKind, Model, Xform};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// The cross-section of a column.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Profile {
    /// `width` along the column's own x axis, `depth` along its y.
    Rectangle {
        width: f64,
        depth: f64,
    },
    Round {
        diameter: f64,
    },
}

impl Profile {
    /// The outline in plan, counter-clockwise, about `center` and turned
    /// `rotation` degrees. Round profiles come back as 24 sides.
    pub fn outline(&self, center: DVec2, rotation: f64) -> Vec<DVec2> {
        let turn = DVec2::from_angle(rotation.to_radians());
        match *self {
            Profile::Rectangle { width, depth } => {
                let (x, y) = (width / 2.0, depth / 2.0);
                [(-x, -y), (x, -y), (x, y), (-x, y)]
                    .iter()
                    .map(|&(a, b)| center + turn.rotate(DVec2::new(a, b)))
                    .collect()
            }
            Profile::Round { diameter } => (0..ROUND_SIDES)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / ROUND_SIDES as f64;
                    center + DVec2::from_angle(a) * diameter / 2.0
                })
                .collect(),
        }
    }

    /// Cross-section area, exact for round profiles.
    pub fn area(&self) -> f64 {
        match *self {
            Profile::Rectangle { width, depth } => width * depth,
            Profile::Round { diameter } => std::f64::consts::PI * diameter * diameter / 4.0,
        }
    }

    /// "300 × 300" or "Ø 300", in millimetres.
    pub fn label(&self) -> String {
        self.label_in(&crate::Units::metric())
    }

    /// The label in a project's units: "300 × 300", "1'-0\" × 1'-0\"".
    pub fn label_in(&self, units: &crate::Units) -> String {
        match *self {
            Profile::Rectangle { width, depth } => {
                format!("{} × {}", units.dimension(width), units.dimension(depth))
            }
            Profile::Round { diameter } => format!("Ø {}", units.dimension(diameter)),
        }
    }

    fn check(&self) -> Result<(), String> {
        match *self {
            Profile::Rectangle { width, depth } => {
                positive(width, "column width")?;
                positive(depth, "column depth")
            }
            Profile::Round { diameter } => positive(diameter, "column diameter"),
        }
    }
}

/// Sides of the polygon a round column is drawn and meshed with.
pub const ROUND_SIDES: usize = 24;

/// A vertical column standing on a level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub level: ElementId,
    /// Centre of the profile in plan.
    pub position: DVec2,
    pub profile: Profile,
    /// Turn of the profile, degrees counter-clockwise.
    #[serde(default)]
    pub rotation: f64,
    /// Height of the base above the level.
    #[serde(default)]
    pub base_offset: f64,
    /// A fixed height; `None` runs up to the next level (or the level's own
    /// height when there is none above).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default)]
    pub material: Option<ElementId>,
}

impl Column {
    pub fn new(level: ElementId, position: DVec2, profile: Profile) -> Self {
        Self {
            level,
            position,
            profile,
            rotation: 0.0,
            base_offset: 0.0,
            height: None,
            material: None,
        }
    }

    /// The height as built: the fixed one, or up to the level above.
    pub fn height_in(&self, model: &Model) -> f64 {
        self.height
            .unwrap_or_else(|| storey_height(model, self.level) - self.base_offset)
            .max(0.01)
    }

    /// Absolute heights of the base and the top.
    pub fn span(&self, model: &Model) -> (f64, f64) {
        let z0 = elevation(model, self.level) + self.base_offset;
        (z0, z0 + self.height_in(model))
    }

    pub fn outline(&self) -> Vec<DVec2> {
        self.profile.outline(self.position, self.rotation)
    }
}

/// A horizontal beam between two points, hung from the top of its storey.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Beam {
    pub level: ElementId,
    pub start: DVec2,
    pub end: DVec2,
    pub width: f64,
    /// Height of the section.
    pub depth: f64,
    /// How far the top sits below the top of the storey (the next level,
    /// or the level's height).
    #[serde(default)]
    pub top_offset: f64,
    #[serde(default)]
    pub material: Option<ElementId>,
}

impl Beam {
    pub fn new(level: ElementId, start: DVec2, end: DVec2) -> Self {
        Self {
            level,
            start,
            end,
            width: 0.2,
            depth: 0.4,
            top_offset: 0.0,
            material: None,
        }
    }

    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    /// Absolute height of the top face.
    pub fn top(&self, model: &Model) -> f64 {
        elevation(model, self.level) + storey_height(model, self.level) - self.top_offset
    }

    /// The footprint in plan, counter-clockwise.
    pub fn outline(&self) -> Vec<DVec2> {
        let d = (self.end - self.start).normalize_or_zero();
        let n = d.perp() * self.width / 2.0;
        vec![self.start - n, self.end - n, self.end + n, self.start + n]
    }
}

/// How a roof slopes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoofShape {
    Flat,
    /// One slope, rising from the eave edge.
    Shed,
    /// Two slopes meeting at a ridge, with upright gable ends.
    Gable,
    /// A slope from every edge.
    #[default]
    Hip,
}

impl RoofShape {
    pub const ALL: [RoofShape; 4] = [
        RoofShape::Flat,
        RoofShape::Shed,
        RoofShape::Gable,
        RoofShape::Hip,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RoofShape::Flat => "Flat",
            RoofShape::Shed => "Mono-pitch",
            RoofShape::Gable => "Gable",
            RoofShape::Hip => "Hip",
        }
    }
}

/// A roof over an outline in plan. Its underside meets the outline at
/// `base_offset` above the level, and runs on past it by the overhang.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Roof {
    pub level: ElementId,
    /// The wall-plate outline (where the slopes start), counter-clockwise.
    pub boundary: Vec<DVec2>,
    pub shape: RoofShape,
    /// Slope in degrees from horizontal.
    pub pitch: f64,
    /// How far the roof reaches past the outline, in plan.
    pub overhang: f64,
    /// Thickness measured square to the slope.
    pub thickness: f64,
    #[serde(default)]
    pub base_offset: f64,
    /// The eave a mono-pitch roof rises from, and the edge a gable roof's
    /// slopes run along (the ridge is parallel to it).
    #[serde(default)]
    pub edge: usize,
    #[serde(default)]
    pub material: Option<ElementId>,
    /// The layered assembly the roof is built of, layers listed from the
    /// top down, each measured square to the slope. With one, the roof's
    /// thickness is the assembly's total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<ElementId>,
}

/// One flat piece of a roof's underside: its outline in plan and the plane
/// `z = c + g · p` (relative to the roof's base).
#[derive(Clone, Debug, PartialEq)]
pub struct RoofFace {
    pub outline: Vec<DVec2>,
    pub g: DVec2,
    pub c: f64,
}

impl RoofFace {
    pub fn z(&self, p: DVec2) -> f64 {
        self.c + self.g.dot(p)
    }
}

impl Roof {
    /// A roof of `shape` over `boundary`, made counter-clockwise, sloping
    /// along its longest edge.
    pub fn new(level: ElementId, boundary: Vec<DVec2>, shape: RoofShape) -> Self {
        let mut boundary = boundary;
        if crate::polygon::signed_area2(&boundary) < 0.0 {
            boundary.reverse();
        }
        let n = boundary.len();
        let edge = (0..n)
            .max_by(|&a, &b| {
                let len = |i: usize| boundary[i].distance(boundary[(i + 1) % n]);
                len(a).total_cmp(&len(b))
            })
            .unwrap_or(0);
        Self {
            level,
            boundary,
            shape,
            pitch: 30.0,
            overhang: 0.5,
            thickness: 0.25,
            base_offset: 0.0,
            edge,
            material: None,
            assembly: None,
        }
    }

    /// The shape actually built: gables and hips need a convex outline;
    /// others are built flat.
    pub fn effective_shape(&self) -> RoofShape {
        match self.shape {
            RoofShape::Gable | RoofShape::Hip if !is_convex(&self.boundary) => RoofShape::Flat,
            s => s,
        }
    }

    /// Absolute height where the underside meets the outline.
    pub fn base(&self, model: &Model) -> f64 {
        elevation(model, self.level) + self.base_offset
    }

    /// Thickness measured upright.
    pub fn vertical_thickness(&self) -> f64 {
        match self.effective_shape() {
            RoofShape::Flat => self.thickness,
            _ => self.thickness / self.pitch.to_radians().cos().max(0.05),
        }
    }

    /// The outline grown by the overhang: where the roof ends in plan.
    pub fn eave_outline(&self) -> Vec<DVec2> {
        offset_polygon(&self.boundary, self.overhang)
    }

    /// The edges a slope rises from.
    pub fn sloped_edges(&self) -> Vec<usize> {
        let n = self.boundary.len();
        let dir = |i: usize| (self.boundary[(i + 1) % n] - self.boundary[i]).normalize_or_zero();
        match self.effective_shape() {
            RoofShape::Flat => vec![],
            RoofShape::Shed => vec![self.edge % n],
            RoofShape::Hip => (0..n).collect(),
            RoofShape::Gable => {
                let r = dir(self.edge % n);
                let cos = 20f64.to_radians().cos();
                (0..n).filter(|&i| dir(i).dot(r).abs() > cos).collect()
            }
        }
    }

    /// The underside as flat pieces, together covering the eave outline.
    pub fn faces(&self) -> Vec<RoofFace> {
        let eaves = self.eave_outline();
        let n = self.boundary.len();
        if n < 3 {
            return vec![];
        }
        let slope = self.pitch.to_radians().tan();
        // the plane rising from edge i: height = slope × distance inside it
        let plane = |i: usize| {
            let a = self.boundary[i];
            let d = (self.boundary[(i + 1) % n] - a).normalize_or_zero();
            let inward = d.perp();
            (inward * slope, -inward.dot(a) * slope)
        };
        let edges = self.sloped_edges();
        if edges.is_empty() {
            return vec![RoofFace {
                outline: eaves,
                g: DVec2::ZERO,
                c: 0.0,
            }];
        }
        if edges.len() == 1 {
            let (g, c) = plane(edges[0]);
            return vec![RoofFace {
                outline: eaves,
                g,
                c,
            }];
        }
        // the underside is the lowest of the planes, each one where it is lowest
        let mut out = Vec::new();
        for (k, &i) in edges.iter().enumerate() {
            let (gi, ci) = plane(i);
            let mut poly = eaves.clone();
            for (m, &j) in edges.iter().enumerate() {
                if i == j {
                    continue;
                }
                let (gj, cj) = plane(j);
                // gi·p + ci <= gj·p + cj, ties to the earlier edge
                let tie = if m < k { -1e-9 } else { 1e-9 };
                poly = clip_half_plane(&poly, gi - gj, cj - ci + tie);
                if poly.len() < 3 {
                    break;
                }
            }
            if poly.len() >= 3 && crate::polygon::signed_area2(&poly).abs() > 1e-9 {
                out.push(RoofFace {
                    outline: poly,
                    g: gi,
                    c: ci,
                });
            }
        }
        out
    }

    /// Height of the underside above the base at `p` (inside the eaves).
    pub fn underside_at(&self, p: DVec2) -> f64 {
        let faces = self.faces();
        faces
            .iter()
            .map(|f| f.z(p))
            .fold(f64::INFINITY, f64::min)
            .min(if faces.is_empty() { 0.0 } else { f64::INFINITY })
    }

    /// Height of the top of the roof above its base.
    pub fn peak(&self) -> f64 {
        self.faces()
            .iter()
            .flat_map(|f| f.outline.iter().map(|p| f.z(*p)).collect::<Vec<_>>())
            .fold(0.0, f64::max)
            + self.vertical_thickness()
    }

    /// Plan area covered, overhang included.
    pub fn plan_area(&self) -> f64 {
        crate::polygon::signed_area2(&self.eave_outline()).abs() / 2.0
    }
}

/// A turn in an L-shaped stair: a square landing after `after` risers.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Landing {
    pub after: u32,
    /// The second flight turns to the left of the first (else right).
    pub left: bool,
}

/// A stair climbing from its level to the next.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stair {
    pub level: ElementId,
    /// Middle of the first riser, in plan.
    pub start: DVec2,
    /// Which way the first flight climbs, degrees counter-clockwise from x.
    pub direction: f64,
    pub width: f64,
    /// Tallest riser allowed; the risers share the height equally.
    pub max_riser: f64,
    /// Going: the depth of each tread.
    pub tread: f64,
    /// A fixed rise; `None` climbs to the next level (or the level's height).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default)]
    pub base_offset: f64,
    /// Thickness of the sloping slab under the steps.
    #[serde(default = "default_waist")]
    pub waist: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub landing: Option<Landing>,
    #[serde(default)]
    pub material: Option<ElementId>,
}

fn default_waist() -> f64 {
    0.15
}

/// One straight run of steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flight {
    /// Middle of the first riser, in plan.
    pub start: DVec2,
    /// Unit direction of travel.
    pub dir: DVec2,
    pub risers: u32,
    /// Height of the bottom of the flight above the stair's base.
    pub z0: f64,
}

impl Flight {
    /// Plan length from the first riser to the last.
    pub fn run(&self, tread: f64) -> f64 {
        self.risers.saturating_sub(1) as f64 * tread
    }

    pub fn end(&self, tread: f64) -> DVec2 {
        self.start + self.dir * self.run(tread)
    }
}

/// The parts of a stair, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct StairLayout {
    pub risers: u32,
    pub riser: f64,
    pub flights: Vec<Flight>,
    /// The landing's outline in plan and its height above the base.
    pub landing: Option<(Vec<DVec2>, f64)>,
}

impl Stair {
    pub fn new(level: ElementId, start: DVec2, direction: f64) -> Self {
        Self {
            level,
            start,
            direction,
            width: 1.0,
            max_riser: 0.18,
            tread: 0.28,
            height: None,
            base_offset: 0.0,
            waist: default_waist(),
            landing: None,
            material: None,
        }
    }

    pub fn dir(&self) -> DVec2 {
        DVec2::from_angle(self.direction.to_radians())
    }

    /// The total rise as built.
    pub fn rise(&self, model: &Model) -> f64 {
        self.height
            .unwrap_or_else(|| storey_height(model, self.level) - self.base_offset)
            .max(0.05)
    }

    /// Absolute height of the bottom.
    pub fn base(&self, model: &Model) -> f64 {
        elevation(model, self.level) + self.base_offset
    }

    /// The flights and landing for a rise of `rise`.
    pub fn layout(&self, rise: f64) -> StairLayout {
        let n = riser_count(rise, self.max_riser);
        let r = rise / n as f64;
        let d = self.dir();
        let first = Flight {
            start: self.start,
            dir: d,
            risers: n,
            z0: 0.0,
        };
        let Some(l) = self.landing.filter(|l| l.after >= 1 && l.after < n) else {
            return StairLayout {
                risers: n,
                riser: r,
                flights: vec![first],
                landing: None,
            };
        };
        let f1 = Flight {
            risers: l.after,
            ..first
        };
        let w = self.width;
        let edge = f1.end(self.tread);
        let side = if l.left { d.perp() } else { -d.perp() };
        let center = edge + d * (w / 2.0);
        let landing = vec![
            edge - d.perp() * w / 2.0,
            edge + d * w - d.perp() * w / 2.0,
            edge + d * w + d.perp() * w / 2.0,
            edge + d.perp() * w / 2.0,
        ];
        let f2 = Flight {
            start: center + side * (w / 2.0),
            dir: side,
            risers: n - l.after,
            z0: l.after as f64 * r,
        };
        StairLayout {
            risers: n,
            riser: r,
            flights: vec![f1, f2],
            landing: Some((landing, l.after as f64 * r)),
        }
    }

    /// The path a handrail follows along one side, over the nosings
    /// (heights above the stair's base), set `inset` in from the edge.
    pub fn side_path(&self, rise: f64, left: bool, inset: f64) -> Vec<DVec3> {
        let lay = self.layout(rise);
        let r = lay.riser;
        let off = self.width / 2.0 - inset;
        let mut out: Vec<DVec3> = Vec::new();
        for (k, f) in lay.flights.iter().enumerate() {
            let s = if left { f.dir.perp() } else { -f.dir.perp() } * off;
            let a = (f.start + s).extend(f.z0 + r);
            let b = (f.end(self.tread) + s).extend(f.z0 + f.risers as f64 * r);
            let outer = self.landing.is_some_and(|l| l.left != left);
            if let (1, true, Some(&last)) = (k, outer, out.last()) {
                // round the far corner of the landing, outside the turn
                let d1 = lay.flights[0].dir;
                out.push(last + (d1 * (self.width - inset)).extend(0.0));
                out.push(a.truncate().extend(f.z0));
            }
            out.push(a);
            out.push(b);
        }
        out.dedup_by(|a, b| a.distance(*b) < 1e-6);
        out
    }

    /// Plan footprint of each flight, counter-clockwise.
    pub fn flight_outlines(&self, rise: f64) -> Vec<Vec<DVec2>> {
        self.layout(rise)
            .flights
            .iter()
            .map(|f| {
                let n = f.dir.perp() * self.width / 2.0;
                let e = f.end(self.tread);
                vec![f.start - n, e - n, e + n, f.start + n]
            })
            .collect()
    }
}

/// How many equal risers climb `rise` with none taller than `max_riser`.
pub fn riser_count(rise: f64, max_riser: f64) -> u32 {
    if !(rise > 0.0 && max_riser > 0.0) {
        return 1;
    }
    ((rise / max_riser) - 1e-9).ceil().clamp(1.0, 500.0) as u32
}

/// Which side of a stair a railing runs along.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StairSide {
    pub stair: ElementId,
    pub left: bool,
}

/// A railing: posts and a top rail along a path, or along a stair's side.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Railing {
    pub level: ElementId,
    /// Points of the path, heights above the level. Ignored while the
    /// railing follows a stair.
    pub path: Vec<DVec3>,
    pub height: f64,
    pub post_spacing: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stair: Option<StairSide>,
    #[serde(default)]
    pub material: Option<ElementId>,
}

/// Rail and post section sizes.
pub const RAIL_SIZE: f64 = 0.05;
pub const POST_SIZE: f64 = 0.04;

impl Railing {
    pub fn new(level: ElementId, path: Vec<DVec3>) -> Self {
        Self {
            level,
            path,
            height: 1.0,
            post_spacing: 1.2,
            stair: None,
            material: None,
        }
    }

    /// A railing along one side of `stair`.
    pub fn along_stair(model: &Model, stair: ElementId, left: bool) -> Option<Self> {
        let Some(ElementKind::Stair(s)) = model.get(stair).map(|e| &e.kind) else {
            return None;
        };
        let mut r = Self::new(s.level, vec![]);
        r.stair = Some(StairSide { stair, left });
        Some(r)
    }

    /// The path as built, in model coordinates.
    pub fn path_in(&self, model: &Model) -> Vec<DVec3> {
        if let Some(side) = self.stair {
            if let Some(ElementKind::Stair(s)) = model.get(side.stair).map(|e| &e.kind) {
                let z = s.base(model);
                return s
                    .side_path(s.rise(model), side.left, RAIL_SIZE)
                    .into_iter()
                    .map(|p| p + DVec3::Z * z)
                    .collect();
            }
        }
        let z = elevation(model, self.level);
        self.path.iter().map(|p| *p + DVec3::Z * z).collect()
    }

    pub fn length(&self, model: &Model) -> f64 {
        self.path_in(model)
            .windows(2)
            .map(|w| w[0].truncate().distance(w[1].truncate()))
            .sum()
    }

    /// Where posts stand: every corner, and in between no further apart
    /// than the spacing.
    pub fn posts(&self, model: &Model) -> Vec<DVec3> {
        posts_along(&self.path_in(model), self.post_spacing)
    }
}

/// Post positions along `path`: each corner, and evenly between.
pub fn posts_along(path: &[DVec3], spacing: f64) -> Vec<DVec3> {
    let mut out = Vec::new();
    for w in path.windows(2) {
        let len = w[0].truncate().distance(w[1].truncate());
        let n = if spacing > 0.0 {
            (len / spacing - 1e-9).ceil().max(1.0) as usize
        } else {
            1
        };
        for i in 0..n {
            out.push(w[0].lerp(w[1], i as f64 / n as f64));
        }
    }
    if let Some(last) = path.last() {
        out.push(*last);
    }
    out
}

/// The elevation of a level, 0 when it is missing.
pub fn elevation(model: &Model, level: ElementId) -> f64 {
    model.level(level).map_or(0.0, |l| l.elevation)
}

/// Floor-to-floor height of a level: up to the next level above, else
/// the level's own height.
pub fn storey_height(model: &Model, level: ElementId) -> f64 {
    let Some(l) = model.level(level) else {
        return 3.0;
    };
    model
        .levels()
        .map(|(_, o)| o.elevation - l.elevation)
        .filter(|d| *d > 1e-6)
        .fold(None, |m: Option<f64>, d| Some(m.map_or(d, |m| m.min(d))))
        .unwrap_or(l.height)
}

fn positive(v: f64, what: &str) -> Result<(), String> {
    if v.is_finite() && v > 0.0 {
        Ok(())
    } else {
        Err(format!("{what} must be positive, got {v}"))
    }
}

fn finite(v: f64, what: &str) -> Result<(), String> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(format!("{what} must be a number"))
    }
}

/// Checks for the elements in this module.
pub fn validate(kind: &ElementKind) -> Result<(), String> {
    match kind {
        ElementKind::Column(c) => {
            c.profile.check()?;
            finite(c.base_offset, "column base offset")?;
            finite(c.rotation, "column rotation")?;
            if !c.position.is_finite() {
                return Err("column position must be finite".into());
            }
            match c.height {
                Some(h) => positive(h, "column height"),
                None => Ok(()),
            }
        }
        ElementKind::Beam(b) => {
            positive(b.width, "beam width")?;
            positive(b.depth, "beam depth")?;
            finite(b.top_offset, "beam top offset")?;
            positive(b.length(), "beam length")
        }
        ElementKind::Roof(r) => {
            positive(r.thickness, "roof thickness")?;
            finite(r.base_offset, "roof base offset")?;
            if !(r.overhang.is_finite() && r.overhang >= 0.0) {
                return Err("roof overhang must not be negative".into());
            }
            if r.boundary.len() < 3 || r.boundary.iter().any(|p| !p.is_finite()) {
                return Err("a roof outline needs at least 3 corners".into());
            }
            if r.shape != RoofShape::Flat && !(r.pitch > 0.0 && r.pitch < 80.0) {
                return Err(format!(
                    "roof pitch must be between 0° and 80°, got {}",
                    r.pitch
                ));
            }
            Ok(())
        }
        ElementKind::Stair(s) => {
            positive(s.width, "stair width")?;
            positive(s.tread, "tread depth")?;
            positive(s.max_riser, "riser height")?;
            positive(s.waist, "stair waist")?;
            finite(s.base_offset, "stair base offset")?;
            finite(s.direction, "stair direction")?;
            if let Some(h) = s.height {
                positive(h, "stair rise")?;
            }
            if !s.start.is_finite() {
                return Err("stair start must be finite".into());
            }
            Ok(())
        }
        ElementKind::Railing(r) => {
            positive(r.height, "railing height")?;
            positive(r.post_spacing, "post spacing")?;
            if r.stair.is_none() && r.path.len() < 2 {
                return Err("a railing path needs two points".into());
            }
            if r.path.iter().any(|p| !p.is_finite()) {
                return Err("railing points must be finite".into());
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// The level of a member, if `kind` is one.
pub fn level_of(kind: &ElementKind) -> Option<ElementId> {
    Some(match kind {
        ElementKind::Column(c) => c.level,
        ElementKind::Beam(b) => b.level,
        ElementKind::Roof(r) => r.level,
        ElementKind::Stair(s) => s.level,
        ElementKind::Railing(r) => r.level,
        _ => return None,
    })
}

/// The level reference of a member, to repoint when pasting.
pub fn level_mut(kind: &mut ElementKind) -> Option<&mut ElementId> {
    Some(match kind {
        ElementKind::Column(c) => &mut c.level,
        ElementKind::Beam(b) => &mut b.level,
        ElementKind::Roof(r) => &mut r.level,
        ElementKind::Stair(s) => &mut s.level,
        ElementKind::Railing(r) => &mut r.level,
        _ => return None,
    })
}

/// What a member is made of.
pub fn material_mut(kind: &mut ElementKind) -> Option<&mut Option<ElementId>> {
    Some(match kind {
        ElementKind::Column(c) => &mut c.material,
        ElementKind::Beam(b) => &mut b.material,
        ElementKind::Roof(r) => &mut r.material,
        ElementKind::Stair(s) => &mut s.material,
        ElementKind::Railing(r) => &mut r.material,
        _ => return None,
    })
}

pub fn material(kind: &ElementKind) -> Option<ElementId> {
    match kind {
        ElementKind::Column(c) => c.material,
        ElementKind::Beam(b) => b.material,
        ElementKind::Roof(r) => r.material,
        ElementKind::Stair(s) => s.material,
        ElementKind::Railing(r) => r.material,
        _ => None,
    }
}

/// Is this one of the elements in this module?
pub fn is_member(kind: &ElementKind) -> bool {
    level_of(kind).is_some()
}

/// Move, turn or scale a member. `None` when it can't take `t`.
pub fn transform(model: &Model, kind: &mut ElementKind, t: &Xform) -> Option<()> {
    if !t.keeps_upright() {
        return None;
    }
    let dz = match t {
        Xform::Move(d) => d.z,
        _ => 0.0,
    };
    let turn = match t {
        Xform::Rotate { degrees, .. } => *degrees,
        _ => 0.0,
    };
    match kind {
        ElementKind::Column(c) => {
            c.position = t.point2(c.position);
            c.rotation = (c.rotation + turn).rem_euclid(360.0);
            c.base_offset += dz;
            match t {
                Xform::Scale { factor, .. } => match &mut c.profile {
                    Profile::Rectangle { width, depth } => {
                        *width *= factor;
                        *depth *= factor;
                    }
                    Profile::Round { diameter } => *diameter *= factor,
                },
                Xform::Stretch { .. } => return None,
                _ => {}
            }
        }
        ElementKind::Beam(b) => {
            b.start = t.point2(b.start);
            b.end = t.point2(b.end);
            b.top_offset -= dz;
            if matches!(t, Xform::Stretch { .. }) && b.length() < 1e-6 {
                return None;
            }
        }
        ElementKind::Roof(r) => {
            for p in &mut r.boundary {
                *p = t.point2(*p);
            }
            r.base_offset += dz;
            if crate::polygon::signed_area2(&r.boundary) < 0.0 {
                // a mirror: run the outline the other way, keeping its eave
                let n = r.boundary.len();
                r.boundary.reverse();
                r.edge = (2 * n - 2 - r.edge % n) % n;
            }
        }
        ElementKind::Stair(s) => {
            let d = s.dir();
            s.start = t.point2(s.start);
            let nd = t.point2(d) - t.point2(DVec2::ZERO);
            s.direction = nd.to_angle().to_degrees().rem_euclid(360.0);
            s.base_offset += dz;
            if let (Xform::Stretch { factor, .. }, Some(l)) = (t, s.landing.as_mut()) {
                if factor.x * factor.y < 0.0 {
                    l.left = !l.left;
                }
            }
        }
        ElementKind::Railing(r) => {
            if r.stair.is_some() {
                // a railing on a stair goes where the stair goes
                return if matches!(t, Xform::Move(_)) {
                    Some(())
                } else {
                    None
                };
            }
            let z = elevation(model, r.level);
            for p in &mut r.path {
                *p = t.point3(*p + DVec3::Z * z) - DVec3::Z * z;
            }
        }
        _ => return None,
    }
    Some(())
}

/// Where a member is, for turning and scaling it about itself.
pub fn pivot(model: &Model, e: &Element) -> Option<DVec3> {
    Some(match &e.kind {
        ElementKind::Column(c) => c.position.extend(c.span(model).0),
        ElementKind::Beam(b) => ((b.start + b.end) / 2.0).extend(b.top(model)),
        ElementKind::Roof(r) => {
            let n = r.boundary.len().max(1) as f64;
            (r.boundary.iter().copied().sum::<DVec2>() / n).extend(r.base(model))
        }
        ElementKind::Stair(s) => s.start.extend(s.base(model)),
        ElementKind::Railing(r) => {
            let p = r.path_in(model);
            let n = p.len().max(1) as f64;
            p.iter().copied().sum::<DVec3>() / n
        }
        _ => return None,
    })
}

/// How a grip on a member moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GripMove {
    /// Anywhere on the horizontal plane through it.
    Flat,
    /// Up and down.
    Vertical,
    /// Along a direction in plan.
    Along(DVec2),
}

/// A drag handle on a member.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grip {
    pub at: DVec3,
    pub how: GripMove,
}

/// The grips of a member: ends, corners, heights.
pub fn grips(model: &Model, e: &Element) -> Vec<Grip> {
    let flat = |at| Grip {
        at,
        how: GripMove::Flat,
    };
    let up = |at| Grip {
        at,
        how: GripMove::Vertical,
    };
    match &e.kind {
        ElementKind::Column(c) => {
            let (z0, z1) = c.span(model);
            vec![flat(c.position.extend(z0)), up(c.position.extend(z1))]
        }
        ElementKind::Beam(b) => {
            let z = b.top(model);
            vec![
                flat(b.start.extend(z)),
                flat(b.end.extend(z)),
                up(((b.start + b.end) / 2.0).extend(z - b.depth)),
            ]
        }
        ElementKind::Roof(r) => {
            let z = r.base(model);
            let mut out: Vec<Grip> = r.boundary.iter().map(|p| flat(p.extend(z))).collect();
            let c = r.boundary.iter().copied().sum::<DVec2>() / r.boundary.len().max(1) as f64;
            out.push(up(c.extend(z + r.peak())));
            out
        }
        ElementKind::Stair(s) => {
            let rise = s.rise(model);
            let lay = s.layout(rise);
            let z = s.base(model);
            let f = lay.flights[0];
            let mut out = vec![flat(s.start.extend(z))];
            // the far side of the first flight sets its width
            out.push(Grip {
                at: (f.start + f.dir.perp() * s.width / 2.0).extend(z),
                how: GripMove::Along(f.dir.perp()),
            });
            // the end of the last flight sets the tread depth
            let last = lay.flights.last().copied().unwrap_or(f);
            out.push(Grip {
                at: last
                    .end(s.tread)
                    .extend(z + last.z0 + last.risers as f64 * lay.riser),
                how: GripMove::Along(last.dir),
            });
            out
        }
        ElementKind::Railing(r) if r.stair.is_none() => {
            let z = elevation(model, r.level);
            let mut out: Vec<Grip> = r.path.iter().map(|p| flat(*p + DVec3::Z * z)).collect();
            if let Some(p) = r.path.first() {
                out.push(up(*p + DVec3::Z * (z + r.height)));
            }
            out
        }
        ElementKind::Railing(r) => {
            let p = r.path_in(model);
            p.first()
                .map(|a| vec![up(*a + DVec3::Z * r.height)])
                .unwrap_or_default()
        }
        _ => vec![],
    }
}

/// Grip `i` of `e` dragged from `grab` to `now`; lengths round to `step`.
pub fn drag_grip(
    model: &Model,
    e: &mut Element,
    i: usize,
    grab: DVec3,
    now: DVec3,
    step: f64,
) -> Result<(), String> {
    let round = |v: f64, s: f64| if s > 0.0 { (v / s).round() * s } else { v };
    let d = now - grab;
    let grips = grips(model, e);
    let g = *grips.get(i).ok_or("no such handle")?;
    match &mut e.kind {
        ElementKind::Column(c) => match i {
            0 => c.position = now.truncate(),
            _ => {
                let h = c.height_in(model);
                c.height = Some(round(h + d.z, step).max(0.1));
            }
        },
        ElementKind::Beam(b) => match i {
            0 => b.start = now.truncate(),
            1 => b.end = now.truncate(),
            _ => b.depth = round(b.depth - d.z, step / 5.0).max(0.05),
        },
        ElementKind::Roof(r) => {
            let n = r.boundary.len();
            if i < n {
                r.boundary[i] = now.truncate();
            } else {
                // the peak sets the pitch
                let peak = g.at.z - r.base(model) - r.vertical_thickness();
                let want = (peak + d.z).max(0.05);
                if peak > 1e-6 && r.effective_shape() != RoofShape::Flat {
                    let slope = r.pitch.to_radians().tan() * want / peak;
                    r.pitch = round(slope.atan().to_degrees(), 0.5).clamp(1.0, 75.0);
                } else {
                    r.thickness = round(r.thickness + d.z, step / 5.0).max(0.02);
                }
            }
        }
        ElementKind::Stair(s) => match i {
            0 => s.start = now.truncate(),
            1 => {
                let GripMove::Along(dir) = g.how else {
                    return Ok(());
                };
                let w = s.width + d.truncate().dot(dir);
                s.width = round(w, step / 5.0).max(0.5);
            }
            _ => {
                let rise = s.rise(model);
                let lay = s.layout(rise);
                let last = lay.flights.last().copied().ok_or("no flight")?;
                let treads = last.risers.saturating_sub(1).max(1) as f64;
                let GripMove::Along(dir) = g.how else {
                    return Ok(());
                };
                let t = s.tread + d.truncate().dot(dir) / treads;
                s.tread = round(t, 0.005).clamp(0.15, 0.6);
            }
        },
        ElementKind::Railing(r) => {
            if r.stair.is_none() && i < r.path.len() {
                let z = r.path[i].z;
                r.path[i] = now.truncate().extend(z);
            } else {
                r.height = round(r.height + d.z, step / 5.0).max(0.2);
            }
        }
        _ => return Err("this handle doesn't fit this element".into()),
    }
    Ok(())
}

/// A short description of a member's size, for the status bar.
pub fn readout(model: &Model, e: &Element) -> Option<String> {
    Some(match &e.kind {
        ElementKind::Column(c) => format!(
            "{} mm, height {:.2} m",
            c.profile.label(),
            c.height_in(model)
        ),
        ElementKind::Beam(b) => format!(
            "length {:.2} m, {:.0} × {:.0} mm",
            b.length(),
            b.width * 1000.0,
            b.depth * 1000.0
        ),
        ElementKind::Roof(r) => format!(
            "{} roof, pitch {:.1}°, {:.2} m²",
            r.effective_shape().label(),
            r.pitch,
            r.plan_area()
        ),
        ElementKind::Stair(s) => {
            let rise = s.rise(model);
            let lay = s.layout(rise);
            format!(
                "{} risers of {:.0} mm, treads {:.0} mm, width {:.2} m",
                lay.risers,
                lay.riser * 1000.0,
                s.tread * 1000.0,
                s.width
            )
        }
        ElementKind::Railing(r) => {
            format!("length {:.2} m, height {:.2} m", r.length(model), r.height)
        }
        _ => return None,
    })
}

/// Is the polygon convex (either winding)?
pub fn is_convex(poly: &[DVec2]) -> bool {
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

/// A counter-clockwise polygon grown outwards by `by` (mitred corners).
pub fn offset_polygon(poly: &[DVec2], by: f64) -> Vec<DVec2> {
    let n = poly.len();
    if n < 3 || by.abs() < 1e-12 {
        return poly.to_vec();
    }
    let ccw = crate::polygon::signed_area2(poly) > 0.0;
    let out_n = |i: usize| {
        let d = (poly[(i + 1) % n] - poly[i]).normalize_or_zero();
        if ccw {
            -d.perp()
        } else {
            d.perp()
        }
    };
    (0..n)
        .map(|i| {
            let prev = (i + n - 1) % n;
            let (n0, n1) = (out_n(prev), out_n(i));
            let bis = n0 + n1;
            let cos = n0.dot(bis.normalize_or_zero());
            if bis.length() < 1e-9 || cos.abs() < 0.2 {
                poly[i] + n1 * by
            } else {
                poly[i] + bis.normalize() * (by / cos)
            }
        })
        .collect()
}

/// The part of convex polygon `poly` where `n · p <= c`.
pub fn clip_half_plane(poly: &[DVec2], n: DVec2, c: f64) -> Vec<DVec2> {
    let mut out = Vec::new();
    let len = poly.len();
    let side = |p: DVec2| n.dot(p) - c;
    for i in 0..len {
        let (a, b) = (poly[i], poly[(i + 1) % len]);
        let (sa, sb) = (side(a), side(b));
        if sa <= 0.0 {
            out.push(a);
        }
        if (sa < 0.0 && sb > 0.0) || (sa > 0.0 && sb < 0.0) {
            out.push(a + (b - a) * (sa / (sa - sb)));
        }
    }
    out.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if out.len() > 1 && out[0].distance(out[out.len() - 1]) < 1e-9 {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Level, Transaction};

    fn two_levels() -> (Model, ElementId, ElementId) {
        let mut m = Model::new();
        let a = Element::new(
            "Ground",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        );
        let b = Element::new(
            "First",
            ElementKind::Level(Level {
                elevation: 2.88,
                height: 3.0,
            }),
        );
        let (ia, ib) = (a.id, b.id);
        Transaction::new("levels")
            .insert(a)
            .insert(b)
            .apply(&mut m)
            .unwrap();
        (m, ia, ib)
    }

    fn rect(w: f64, d: f64) -> Vec<DVec2> {
        vec![
            DVec2::ZERO,
            DVec2::new(w, 0.0),
            DVec2::new(w, d),
            DVec2::new(0.0, d),
        ]
    }

    #[test]
    fn risers_never_exceed_the_maximum() {
        assert_eq!(riser_count(2.88, 0.18), 16);
        assert_eq!(riser_count(3.0, 0.18), 17);
        assert_eq!(riser_count(0.18, 0.18), 1);
        let (m, ground, _) = two_levels();
        let s = Stair::new(ground, DVec2::ZERO, 0.0);
        let rise = s.rise(&m);
        assert!((rise - 2.88).abs() < 1e-9, "climbs to the next level");
        let lay = s.layout(rise);
        assert_eq!(lay.risers, 16);
        assert!((lay.riser - 0.18).abs() < 1e-9);
        // 16 risers, 15 treads
        assert!((lay.flights[0].run(s.tread) - 15.0 * 0.28).abs() < 1e-9);
    }

    #[test]
    fn an_l_stair_turns_after_its_landing() {
        let (m, ground, _) = two_levels();
        let mut s = Stair::new(ground, DVec2::ZERO, 0.0);
        s.landing = Some(Landing {
            after: 8,
            left: true,
        });
        let lay = s.layout(s.rise(&m));
        assert_eq!(lay.flights.len(), 2);
        assert_eq!(lay.flights[0].risers + lay.flights[1].risers, 16);
        let f2 = lay.flights[1];
        assert!((f2.dir - DVec2::Y).length() < 1e-9);
        assert!((f2.z0 - 8.0 * 0.18).abs() < 1e-9);
        // the second flight starts at the landing's left side
        let edge = 7.0 * 0.28;
        assert!((f2.start - DVec2::new(edge + 0.5, 0.5)).length() < 1e-9);
        let path = s.side_path(2.88, false, 0.0);
        assert!(path.len() > 4, "the outer rail rounds the landing");
        assert!((path.last().unwrap().z - 2.88).abs() < 1e-9);
    }

    #[test]
    fn a_hip_roof_on_a_rectangle_has_four_faces_and_a_ridge() {
        let (_, ground, _) = two_levels();
        let mut r = Roof::new(ground, rect(10.0, 6.0), RoofShape::Hip);
        r.overhang = 0.0;
        r.pitch = 45.0;
        let faces = r.faces();
        assert_eq!(faces.len(), 4);
        // the ridge sits half the short side up at 45°
        let ridge = r.peak() - r.vertical_thickness();
        assert!((ridge - 3.0).abs() < 1e-9);
        let area: f64 = faces
            .iter()
            .map(|f| crate::polygon::signed_area2(&f.outline) / 2.0)
            .sum();
        assert!((area - 60.0).abs() < 1e-6);
        // the ridge line runs from (3,3) to (7,3)
        assert!((r.underside_at(DVec2::new(5.0, 3.0)) - 3.0).abs() < 1e-9);
        assert!((r.underside_at(DVec2::new(1.0, 3.0)) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_gable_roof_slopes_from_the_long_sides_only() {
        let (_, ground, _) = two_levels();
        let mut r = Roof::new(ground, rect(10.0, 6.0), RoofShape::Gable);
        r.overhang = 0.5;
        r.pitch = 30.0;
        assert_eq!(r.sloped_edges(), vec![0, 2]);
        assert_eq!(r.faces().len(), 2);
        let ridge = r.underside_at(DVec2::new(5.0, 3.0));
        assert!((ridge - 3.0 * 30f64.to_radians().tan()).abs() < 1e-9);
        // the overhang dips below the wall plate
        assert!(r.underside_at(DVec2::new(5.0, -0.5)) < 0.0);
        assert!((r.plan_area() - 11.0 * 7.0).abs() < 1e-9);
    }

    #[test]
    fn a_shed_roof_rises_from_its_eave() {
        let (_, ground, _) = two_levels();
        let mut r = Roof::new(ground, rect(4.0, 3.0), RoofShape::Shed);
        r.overhang = 0.0;
        r.edge = 0;
        r.pitch = 10.0;
        assert_eq!(r.faces().len(), 1);
        let top = r.underside_at(DVec2::new(2.0, 3.0));
        assert!((top - 3.0 * 10f64.to_radians().tan()).abs() < 1e-9);
    }

    #[test]
    fn hips_on_an_l_shape_fall_back_to_flat() {
        let (_, ground, _) = two_levels();
        let l = vec![
            DVec2::ZERO,
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 2.0),
            DVec2::new(2.0, 2.0),
            DVec2::new(2.0, 4.0),
            DVec2::new(0.0, 4.0),
        ];
        let r = Roof::new(ground, l, RoofShape::Hip);
        assert_eq!(r.effective_shape(), RoofShape::Flat);
        assert_eq!(r.faces().len(), 1);
    }

    #[test]
    fn columns_run_to_the_next_level_unless_given_a_height() {
        let (m, ground, first) = two_levels();
        let mut c = Column::new(
            ground,
            DVec2::new(1.0, 1.0),
            Profile::Rectangle {
                width: 0.3,
                depth: 0.3,
            },
        );
        assert!((c.height_in(&m) - 2.88).abs() < 1e-9);
        c.height = Some(2.0);
        assert_eq!(c.span(&m), (0.0, 2.0));
        // the top level has none above: its own height
        let top = Column::new(first, DVec2::ZERO, Profile::Round { diameter: 0.3 });
        assert!((top.height_in(&m) - 3.0).abs() < 1e-9);
        let b = Beam::new(ground, DVec2::ZERO, DVec2::new(5.0, 0.0));
        assert!((b.top(&m) - 2.88).abs() < 1e-9);
    }

    #[test]
    fn new_elements_round_trip_through_json() {
        let (mut m, ground, _) = two_levels();
        let s = Element::new(
            "Stair",
            ElementKind::Stair(Stair::new(ground, DVec2::ZERO, 90.0)),
        );
        let sid = s.id;
        let els = vec![
            Element::new(
                "Column",
                ElementKind::Column(Column::new(
                    ground,
                    DVec2::ZERO,
                    Profile::Round { diameter: 0.4 },
                )),
            ),
            Element::new(
                "Beam",
                ElementKind::Beam(Beam::new(ground, DVec2::ZERO, DVec2::X)),
            ),
            Element::new(
                "Roof",
                ElementKind::Roof(Roof::new(ground, rect(4.0, 4.0), RoofShape::Gable)),
            ),
            s,
        ];
        let mut tx = Transaction::new("add");
        for e in els {
            tx = tx.insert(e);
        }
        tx.apply(&mut m).unwrap();
        let rail = Railing::along_stair(&m, sid, true).unwrap();
        Transaction::new("rail")
            .insert(Element::new("Railing", ElementKind::Railing(rail)))
            .apply(&mut m)
            .unwrap();
        let back = Model::from_json(&m.to_json()).unwrap();
        assert_eq!(back.len(), m.len());
        for e in m.iter() {
            assert_eq!(back.get(e.id), Some(e));
        }
        // a stair's railing goes with it
        let tx = Transaction::new("delete").remove_cascade(&m, sid);
        assert_eq!(tx.ops.len(), 2);
    }

    #[test]
    fn older_files_without_optional_fields_still_load() {
        let json = r#"{"level":"00000000-0000-0000-0000-000000000001","start":[0.0,0.0],"direction":0.0,"width":1.0,"max_riser":0.18,"tread":0.28}"#;
        let s: Stair = serde_json::from_str(json).unwrap();
        assert_eq!(s.waist, 0.15);
        assert!(s.landing.is_none() && s.height.is_none());
    }

    #[test]
    fn posts_fall_at_corners_and_within_the_spacing() {
        let path = [
            DVec3::ZERO,
            DVec3::new(3.0, 0.0, 0.0),
            DVec3::new(3.0, 1.0, 0.0),
        ];
        let posts = posts_along(&path, 1.2);
        // 3 m in three bays, 1 m in one, plus the end
        assert_eq!(posts.len(), 5);
        assert!(posts.contains(&DVec3::new(3.0, 0.0, 0.0)));
    }
}
