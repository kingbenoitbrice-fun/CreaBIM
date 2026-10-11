use crate::{
    editmesh::EditMesh,
    modifiers::{Evaluated, Modifier},
    ElementId,
};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// One object in the building model. Lengths are in metres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub id: ElementId,
    pub name: String,
    pub kind: ElementKind,
}

impl Element {
    pub fn new(name: impl Into<String>, kind: ElementKind) -> Self {
        Self {
            id: ElementId::new(),
            name: name.into(),
            kind,
        }
    }

    /// Elements this element depends on. The model refuses edits that would
    /// leave any of these missing.
    pub fn references(&self) -> Vec<ElementId> {
        match &self.kind {
            ElementKind::Level(_) | ElementKind::Grid(_) => vec![],
            ElementKind::Wall(w) => vec![w.level],
            ElementKind::Slab(s) => vec![s.level],
            // a missing material falls back to the element type's default,
            // so materials are not hard references
            ElementKind::Material(_) => vec![],
            ElementKind::Opening(o) => vec![o.host],
            ElementKind::FreeMesh(m) => m.level.into_iter().collect(),
            ElementKind::View(v) => match &v.kind {
                ViewKind::Plan { level, .. } => vec![*level],
                ViewKind::Section { .. }
                | ViewKind::Axonometric { .. }
                | ViewKind::Schedule { .. } => vec![],
            },
            ElementKind::Sheet(_) => vec![],
            ElementKind::Viewport(v) => vec![v.sheet, v.view],
            ElementKind::Dimension(d) => std::iter::once(d.view)
                .chain(d.a.host)
                .chain(d.b.host)
                .collect(),
            ElementKind::Tag(t) => vec![t.view, t.host],
            ElementKind::Sketch(s) => s.level.into_iter().collect(),
            ElementKind::Cut(c) => vec![c.target, c.cutter],
            ElementKind::Guide(g) => g.level.into_iter().collect(),
        }
    }

    /// The IFC entity this element exports as.
    pub fn ifc_class(&self) -> &str {
        match &self.kind {
            ElementKind::Level(_) => "IfcBuildingStorey",
            ElementKind::Grid(_) => "IfcGrid",
            ElementKind::Wall(_) => "IfcWall",
            ElementKind::Slab(_) => "IfcSlab",
            ElementKind::Opening(o) => match o.kind {
                OpeningKind::Door => "IfcDoor",
                OpeningKind::Window => "IfcWindow",
            },
            ElementKind::FreeMesh(m) => &m.ifc_class,
            // Views are documentation, not building elements; they are not exported to IFC.
            ElementKind::View(_) => "View",
            ElementKind::Sheet(_) => "Sheet",
            ElementKind::Viewport(_) => "Viewport",
            ElementKind::Material(_) => "IfcMaterial",
            ElementKind::Dimension(_) => "Dimension",
            ElementKind::Tag(_) => "Tag",
            ElementKind::Sketch(_) => "Sketch",
            ElementKind::Cut(_) => "Cut",
            ElementKind::Guide(_) => "Guide",
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let positive = |v: f64, what: &str| {
            if v.is_finite() && v > 0.0 {
                Ok(())
            } else {
                Err(format!("{what} must be positive, got {v}"))
            }
        };
        match &self.kind {
            ElementKind::Level(l) => positive(l.height, "level height"),
            ElementKind::Grid(g) => {
                if g.start.distance(g.end) > 1e-6 {
                    Ok(())
                } else {
                    Err("grid line has zero length".into())
                }
            }
            ElementKind::Wall(w) => {
                positive(w.thickness, "wall thickness")?;
                positive(w.height, "wall height")?;
                positive(w.length(), "wall length")
            }
            ElementKind::Slab(s) => {
                positive(s.thickness, "slab thickness")?;
                if s.boundary.len() < 3 {
                    return Err("slab boundary needs at least 3 points".into());
                }
                Ok(())
            }
            ElementKind::Opening(o) => {
                positive(o.width, "opening width")?;
                positive(o.height, "opening height")?;
                if o.sill < 0.0 || o.offset < 0.0 {
                    return Err("opening sill and offset must not be negative".into());
                }
                Ok(())
            }
            ElementKind::FreeMesh(m) => {
                m.mesh.validate()?;
                if !(m.origin.is_finite() && m.rotation.is_finite()) {
                    return Err("mass position must be finite".into());
                }
                if !m.scale.is_finite() || m.scale.abs() < 1e-6 {
                    return Err("mass scale must not be zero".into());
                }
                m.modifiers.iter().try_for_each(Modifier::validate)
            }
            ElementKind::View(v) => {
                positive(v.scale, "view scale")?;
                match &v.kind {
                    ViewKind::Plan { cut_height, .. } => positive(*cut_height, "cut height"),
                    ViewKind::Section {
                        start, end, depth, ..
                    } => {
                        positive(*depth, "view depth")?;
                        positive(start.distance(*end), "section line length")
                    }
                    ViewKind::Axonometric { altitude, .. } => {
                        if altitude.is_finite() && altitude.abs() <= 90.0 {
                            Ok(())
                        } else {
                            Err(format!("view altitude must be within ±90°, got {altitude}"))
                        }
                    }
                    ViewKind::Schedule { .. } => Ok(()),
                }
            }
            ElementKind::Sheet(sh) => {
                positive(sh.width, "sheet width")?;
                positive(sh.height, "sheet height")
            }
            ElementKind::Viewport(v) => {
                if !v.center.is_finite() || !v.target.is_finite() {
                    return Err("viewport position must be finite".into());
                }
                match v.crop {
                    Some([lo, hi]) if !(lo.is_finite() && hi.is_finite() && lo.cmplt(hi).all()) => {
                        Err("viewport crop must have a positive size".into())
                    }
                    _ => Ok(()),
                }
            }
            ElementKind::Material(m) => {
                if m.color.iter().all(|c| (0.0..=1.0).contains(c)) {
                    Ok(())
                } else {
                    Err("material colour components must be within 0..1".into())
                }
            }
            ElementKind::Dimension(d) => {
                if d.offset.is_finite() && d.a.local.is_finite() && d.b.local.is_finite() {
                    Ok(())
                } else {
                    Err("dimension points must be finite".into())
                }
            }
            ElementKind::Tag(t) => {
                if t.offset.is_finite() {
                    Ok(())
                } else {
                    Err("tag position must be finite".into())
                }
            }
            ElementKind::Sketch(s) => {
                let p = &s.plane;
                let square = (p.x.length() - 1.0).abs() < 1e-6
                    && (p.y.length() - 1.0).abs() < 1e-6
                    && p.x.dot(p.y).abs() < 1e-6;
                if !(square && p.origin.is_finite()) {
                    return Err("sketch plane needs perpendicular unit axes".into());
                }
                if s.points.iter().any(|q| !q.is_finite()) {
                    return Err("sketch points must be finite".into());
                }
                match (s.points.len(), s.closed) {
                    (0 | 1, _) => Err("a sketch needs at least 2 points".into()),
                    (2, true) => Err("a closed sketch needs at least 3 points".into()),
                    _ => Ok(()),
                }
            }
            ElementKind::Cut(c) => {
                if c.target == c.cutter {
                    Err("an element can't cut itself".into())
                } else {
                    Ok(())
                }
            }
            ElementKind::Guide(g) => {
                if g.a.is_finite() && g.b.is_finite() && g.a.distance(g.b) > 1e-6 {
                    Ok(())
                } else {
                    Err("a guide needs two different points".into())
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ElementKind {
    Level(Level),
    Grid(Grid),
    Wall(Wall),
    Slab(Slab),
    Opening(Opening),
    FreeMesh(FreeMesh),
    View(View),
    Sheet(Sheet),
    Viewport(Viewport),
    Material(Material),
    Dimension(Dimension),
    Tag(Tag),
    Sketch(Sketch),
    Cut(Cut),
    Guide(Guide),
}

impl ElementKind {
    /// The variant name, as it appears in saved files (`"Wall"`, `"Level"`...).
    pub fn name(&self) -> &'static str {
        match self {
            ElementKind::Level(_) => "Level",
            ElementKind::Grid(_) => "Grid",
            ElementKind::Wall(_) => "Wall",
            ElementKind::Slab(_) => "Slab",
            ElementKind::Opening(_) => "Opening",
            ElementKind::FreeMesh(_) => "FreeMesh",
            ElementKind::View(_) => "View",
            ElementKind::Sheet(_) => "Sheet",
            ElementKind::Viewport(_) => "Viewport",
            ElementKind::Material(_) => "Material",
            ElementKind::Dimension(_) => "Dimension",
            ElementKind::Tag(_) => "Tag",
            ElementKind::Sketch(_) => "Sketch",
            ElementKind::Cut(_) => "Cut",
            ElementKind::Guide(_) => "Guide",
        }
    }
}

/// Pattern drawn on cut faces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hatch {
    /// Outline only.
    Empty,
    /// Plain grey fill (poché).
    #[default]
    Solid,
    /// 45° lines.
    Diagonal,
    /// Crossed 45° lines.
    Cross,
    /// Pairs of 45° lines, the usual masonry hatch.
    Brick,
    /// Stipple with sparse 45° lines.
    Concrete,
    /// Lines along the cut, like grain.
    Timber,
}

impl Hatch {
    pub const ALL: [Hatch; 7] = [
        Hatch::Empty,
        Hatch::Solid,
        Hatch::Diagonal,
        Hatch::Cross,
        Hatch::Brick,
        Hatch::Concrete,
        Hatch::Timber,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Hatch::Empty => "Empty",
            Hatch::Solid => "Solid",
            Hatch::Diagonal => "Diagonal",
            Hatch::Cross => "Cross",
            Hatch::Brick => "Brick",
            Hatch::Concrete => "Concrete",
            Hatch::Timber => "Timber",
        }
    }
}

/// What an element is made of: how it looks in 3D and how its cut faces are hatched.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub hatch: Hatch,
    /// sRGB colour, 0..1, as picked in a colour chooser.
    pub color: [f32; 3],
}

/// One end of a dimension. With a host, `local` is in the host's own
/// frame, so the dimension follows the host when it moves:
/// - wall: (distance along the wall, offset across it), measured from the
///   wall's end instead of its start when `from_end` is set;
/// - door or window: (distance from its left edge, offset across the wall);
/// - level: (drawing x, height above the level).
///
/// Without a host, `local` is a point in the view's drawing.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostPoint {
    pub host: Option<ElementId>,
    pub local: DVec2,
    #[serde(default)]
    pub from_end: bool,
}

impl HostPoint {
    pub fn free(p: DVec2) -> Self {
        Self {
            host: None,
            local: p,
            from_end: false,
        }
    }
}

/// A boolean between two elements, like IFC's IfcRelVoidsElement: `target`
/// is shown as `target op cutter` and the cutter itself is hidden (drawn as
/// a ghost in 3D, left out of drawings and exports). Deleting either
/// element deletes the cut.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cut {
    pub target: ElementId,
    pub cutter: ElementId,
    pub op: crate::csg::BoolOp,
}

/// A construction line, endless through `a` and `b`, to snap to. Guides are
/// drawn dashed in plans (on their level) and in 3D, never exported.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    pub level: Option<ElementId>,
    pub a: DVec3,
    pub b: DVec3,
}

/// An aligned dimension in one view, measuring from `a` to `b`. Its line
/// sits `offset` drawing units to the left of a → b.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dimension {
    pub view: ElementId,
    pub a: HostPoint,
    pub b: HostPoint,
    pub offset: f64,
}

/// A label in one view showing a property of `host`, such as a door's mark.
/// It sits at `offset` from the host's centre, so it moves with the host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub view: ElementId,
    pub host: ElementId,
    pub offset: DVec2,
}

/// A storey. Walls and slabs hang off a level, so moving the level moves them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub elevation: f64,
    /// Default floor-to-floor height, used for new walls on this level.
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    pub label: String,
    pub start: DVec2,
    pub end: DVec2,
}

/// A straight wall defined by its centreline on a level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub level: ElementId,
    pub start: DVec2,
    pub end: DVec2,
    pub thickness: f64,
    pub height: f64,
    /// Offset of the wall base above its level.
    pub base_offset: f64,
    #[serde(default)]
    pub material: Option<ElementId>,
}

impl Wall {
    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    /// Unit vector along the wall, from start to end.
    pub fn direction(&self) -> DVec2 {
        (self.end - self.start).normalize_or_zero()
    }
}

/// A horizontal slab whose top face sits at its level's elevation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slab {
    pub level: ElementId,
    /// Outline in plan, counter-clockwise.
    pub boundary: Vec<DVec2>,
    pub thickness: f64,
    #[serde(default)]
    pub material: Option<ElementId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpeningKind {
    Door,
    Window,
}

/// A door or window cut into a host wall.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Opening {
    pub host: ElementId,
    pub kind: OpeningKind,
    /// Distance from the wall start to the opening's left edge.
    pub offset: f64,
    /// Height of the opening's bottom edge above the wall base.
    pub sill: f64,
    pub width: f64,
    pub height: f64,
}

/// A freely modelled object, edited Blender-style, that still carries BIM
/// meaning through its IFC class.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FreeMesh {
    pub level: Option<ElementId>,
    pub origin: DVec3,
    pub mesh: EditMesh,
    pub ifc_class: String,
    #[serde(default)]
    pub material: Option<ElementId>,
    /// Non-destructive operations applied to `mesh`, top to bottom.
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
    /// Turn about the vertical axis through `origin`, in degrees.
    #[serde(default)]
    pub rotation: f64,
    /// Uniform size factor about `origin`.
    #[serde(default = "one")]
    pub scale: f64,
}

fn one() -> f64 {
    1.0
}

impl FreeMesh {
    /// A mesh as modelled, with no modifiers, turn or scaling.
    pub fn new(level: Option<ElementId>, origin: DVec3, mesh: EditMesh) -> Self {
        Self {
            level,
            origin,
            mesh,
            ifc_class: "IfcBuildingElementProxy".into(),
            material: None,
            modifiers: vec![],
            rotation: 0.0,
            scale: 1.0,
        }
    }

    /// From the mesh's own axes to the model's, about `origin`.
    pub fn to_model(&self, v: DVec3) -> DVec3 {
        let (s, c) = self.rotation.to_radians().sin_cos();
        let v = v * self.scale;
        DVec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
    }

    /// A model-axes offset in the mesh's own axes (for moving vertices).
    pub fn to_local(&self, d: DVec3) -> DVec3 {
        let (s, c) = (-self.rotation).to_radians().sin_cos();
        DVec3::new(d.x * c - d.y * s, d.x * s + d.y * c, d.z) / self.scale
    }

    /// The shape as shown: modifiers applied, turned and scaled, relative to
    /// `origin`. `source` gives each face's face in `mesh`, for editing.
    pub fn evaluated(&self) -> Evaluated {
        let mut e = crate::modifiers::evaluate(&self.mesh, &self.modifiers);
        if self.rotation != 0.0 || self.scale != 1.0 {
            for v in &mut e.mesh.vertices {
                *v = self.to_model(*v);
            }
            if self.scale < 0.0 {
                for f in &mut e.mesh.faces {
                    f.reverse();
                }
            }
        }
        e
    }
}

/// A plane anywhere in the model to sketch on: an origin and two
/// perpendicular unit axes. Its normal is `x × y`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workplane {
    pub origin: DVec3,
    pub x: DVec3,
    pub y: DVec3,
}

impl Workplane {
    /// A horizontal plane at height `z`.
    pub fn ground(z: f64) -> Self {
        Self {
            origin: DVec3::new(0.0, 0.0, z),
            x: DVec3::X,
            y: DVec3::Y,
        }
    }

    /// The plane through `origin` facing `normal`. Its x axis is level
    /// (horizontal) whenever the plane is not, so sketches on walls stay square.
    pub fn facing(origin: DVec3, normal: DVec3) -> Self {
        let n = normal.normalize_or(DVec3::Z);
        let x = if n.z.abs() > 0.999 {
            DVec3::X * n.z.signum()
        } else {
            DVec3::Z.cross(n).normalize()
        };
        Self {
            origin,
            x,
            y: n.cross(x),
        }
    }

    pub fn normal(&self) -> DVec3 {
        self.x.cross(self.y)
    }

    pub fn to_model(&self, p: DVec2) -> DVec3 {
        self.origin + self.x * p.x + self.y * p.y
    }

    pub fn to_plane(&self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.x), d.dot(self.y))
    }

    /// Where a ray meets the plane, if it does in front of its origin.
    pub fn intersect(&self, origin: DVec3, dir: DVec3) -> Option<DVec3> {
        let n = self.normal();
        let denom = dir.dot(n);
        if denom.abs() < 1e-9 {
            return None;
        }
        let t = (self.origin - origin).dot(n) / denom;
        (t > 0.0).then(|| origin + dir * t)
    }
}

/// Lines drawn on a workplane: a profile to extrude into a mass, or
/// construction lines. The plane's origin is relative to the level's
/// elevation, like a mass's, so sketches move with their level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sketch {
    pub level: Option<ElementId>,
    pub plane: Workplane,
    /// Corners in the plane's axes.
    pub points: Vec<DVec2>,
    /// The last point joins the first.
    pub closed: bool,
}

impl Sketch {
    /// The corners in model coordinates, relative to the level's elevation.
    pub fn model_points(&self) -> Vec<DVec3> {
        self.points
            .iter()
            .map(|p| self.plane.to_model(*p))
            .collect()
    }

    /// Each line of the sketch as a pair of corners in model coordinates.
    pub fn segments(&self) -> Vec<(DVec3, DVec3)> {
        let pts = self.model_points();
        let n = pts.len();
        let count = if self.closed { n } else { n.saturating_sub(1) };
        (0..count).map(|i| (pts[i], pts[(i + 1) % n])).collect()
    }

    /// The profile pushed `distance` along the plane's normal: a closed solid
    /// for a closed sketch, a surface (to solidify) for an open one. Vertices
    /// are relative to the plane's origin, ready for a mass placed there.
    pub fn extrude(&self, distance: f64) -> EditMesh {
        let mut pts = self.points.clone();
        if self.closed && crate::polygon::signed_area2(&pts) < 0.0 {
            pts.reverse();
        }
        let n = self.plane.normal() * distance;
        let flat = |p: DVec2| self.plane.x * p.x + self.plane.y * p.y;
        let m = pts.len() as u32;
        let mut mesh = EditMesh::default();
        mesh.vertices.extend(pts.iter().map(|p| flat(*p)));
        mesh.vertices.extend(pts.iter().map(|p| flat(*p) + n));
        let edges = if self.closed { m } else { m - 1 };
        for i in 0..edges {
            let j = (i + 1) % m;
            mesh.faces.push(vec![i, j, j + m, i + m]);
        }
        if self.closed {
            mesh.faces.insert(0, (0..m).rev().collect());
            mesh.faces.insert(1, (m..2 * m).collect());
        }
        if distance < 0.0 {
            for f in &mut mesh.faces {
                f.reverse();
            }
        }
        mesh
    }
}

/// A drawing of the model. Views live in the model like any element, so they
/// are saved, undone and (later) placed on sheets. Their drawings are always
/// generated from the current model, never stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub kind: ViewKind,
    /// Drawing scale denominator: 100 means 1:100.
    pub scale: f64,
    #[serde(default)]
    pub style: ViewStyle,
}

/// How a section, elevation or 3D view draws what is beyond the cut.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewStyle {
    /// Visible edges only.
    #[default]
    HiddenLine,
    /// Visible edges over surfaces shaded in their material colours.
    Shaded,
}

/// What a schedule lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleCategory {
    Doors,
    Windows,
    Walls,
    Slabs,
}

impl ScheduleCategory {
    pub const ALL: [ScheduleCategory; 4] = [
        ScheduleCategory::Doors,
        ScheduleCategory::Windows,
        ScheduleCategory::Walls,
        ScheduleCategory::Slabs,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ScheduleCategory::Doors => "Doors",
            ScheduleCategory::Windows => "Windows",
            ScheduleCategory::Walls => "Walls",
            ScheduleCategory::Slabs => "Slabs",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ViewKind {
    /// A floor plan of one level, cut horizontally at `cut_height` above it.
    Plan { level: ElementId, cut_height: f64 },
    /// A vertical section along the line `start` → `end`, looking to the left
    /// of that direction (a line drawn west to east looks north) for `depth`
    /// metres. An elevation is the same view with `cut` off: nothing is cut,
    /// everything in front of the viewer is drawn in projection.
    Section {
        start: DVec2,
        end: DVec2,
        depth: f64,
        cut: bool,
    },
    /// A parallel-projection 3D view of the whole model, seen from compass
    /// bearing `azimuth` (degrees, 0 = from the south, counter-clockwise)
    /// and `altitude` degrees above the horizon.
    Axonometric { azimuth: f64, altitude: f64 },
    /// A table of every element in a category, read live from the model.
    /// It is drawn at paper size, whatever the scale.
    Schedule { category: ScheduleCategory },
}

impl View {
    pub fn plan(level: ElementId) -> Self {
        Self {
            kind: ViewKind::Plan {
                level,
                cut_height: 1.2,
            },
            scale: 100.0,
            style: ViewStyle::HiddenLine,
        }
    }

    pub fn section(start: DVec2, end: DVec2, depth: f64, cut: bool) -> Self {
        Self {
            kind: ViewKind::Section {
                start,
                end,
                depth,
                cut,
            },
            scale: 100.0,
            style: ViewStyle::HiddenLine,
        }
    }
}

impl View {
    pub fn axonometric(azimuth: f64, altitude: f64) -> Self {
        Self {
            kind: ViewKind::Axonometric { azimuth, altitude },
            scale: 100.0,
            style: ViewStyle::Shaded,
        }
    }

    pub fn schedule(category: ScheduleCategory) -> Self {
        Self {
            kind: ViewKind::Schedule { category },
            scale: 1000.0,
            style: ViewStyle::HiddenLine,
        }
    }

    /// The scale its drawing is placed on paper at. Schedules are drawn in
    /// paper millimetres, so they are placed 1:1000 (a drawing metre is a millimetre).
    pub fn paper_scale(&self) -> f64 {
        match self.kind {
            ViewKind::Schedule { .. } => 1000.0,
            _ => self.scale,
        }
    }
}

/// A drawing sheet. Its name is the sheet title; sizes are in millimetres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    /// Sheet number, e.g. "A101".
    pub number: String,
    pub width: f64,
    pub height: f64,
}

impl Sheet {
    /// ISO A-series sizes, landscape.
    pub const SIZES: &[(&str, f64, f64)] = &[
        ("A0", 1189.0, 841.0),
        ("A1", 841.0, 594.0),
        ("A2", 594.0, 420.0),
        ("A3", 420.0, 297.0),
        ("A4", 297.0, 210.0),
    ];

    pub fn a(number: impl Into<String>, size: &str) -> Self {
        let (_, width, height) = Self::SIZES
            .iter()
            .find(|(n, ..)| *n == size)
            .copied()
            .unwrap_or(Self::SIZES[3]);
        Self {
            number: number.into(),
            width,
            height,
        }
    }

    /// The paper size's name, if it is a standard one.
    pub fn size_name(&self) -> Option<&'static str> {
        Self::SIZES
            .iter()
            .find(|(_, w, h)| (w - self.width).abs() < 0.5 && (h - self.height).abs() < 0.5)
            .map(|(n, ..)| *n)
    }
}

/// A view placed on a sheet. The view's drawing is generated live, so the
/// sheet always shows the current model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub sheet: ElementId,
    pub view: ElementId,
    /// Where `target` sits on the sheet, mm from the sheet's lower-left corner.
    pub center: DVec2,
    /// The drawing point (model metres) pinned to `center`, so the drawing
    /// stays put on the sheet as the model changes.
    pub target: DVec2,
    /// Only the part of the drawing inside this rectangle (drawing
    /// coordinates, lower-left and upper-right) is shown.
    #[serde(default)]
    pub crop: Option<[DVec2; 2]>,
}

/// IFC classes a free mesh may be classified as.
pub const FREE_MESH_CLASSES: &[&str] = &[
    "IfcBuildingElementProxy",
    "IfcWall",
    "IfcSlab",
    "IfcRoof",
    "IfcColumn",
    "IfcBeam",
    "IfcCovering",
    "IfcFurniture",
];

#[cfg(test)]
mod sketch_tests {
    use super::*;

    fn l_shape(plane: Workplane, closed: bool) -> Sketch {
        Sketch {
            level: None,
            plane,
            points: vec![
                DVec2::new(0.0, 0.0),
                DVec2::new(4.0, 0.0),
                DVec2::new(4.0, 2.0),
                DVec2::new(2.0, 2.0),
                DVec2::new(2.0, 4.0),
                DVec2::new(0.0, 4.0),
            ],
            closed,
        }
    }

    #[test]
    fn workplanes_on_walls_keep_their_x_axis_level() {
        let p = Workplane::facing(DVec3::new(1.0, 2.0, 3.0), DVec3::new(0.0, -1.0, 0.0));
        assert!(p.x.z.abs() < 1e-12 && p.normal().distance(-DVec3::Y) < 1e-12);
        let q = DVec3::new(4.0, 2.0, 7.0);
        assert!(p.to_model(p.to_plane(q)).distance(q) < 1e-12);
        let hit = p.intersect(DVec3::new(0.0, -5.0, 0.0), DVec3::Y).unwrap();
        assert!((hit.y - 2.0).abs() < 1e-12);
    }

    #[test]
    fn a_closed_sketch_extrudes_into_a_solid_on_any_plane() {
        for plane in [
            Workplane::ground(0.0),
            Workplane::facing(DVec3::ZERO, DVec3::new(1.0, 1.0, 0.5)),
        ] {
            for d in [2.0, -2.0] {
                let m = l_shape(plane, true).extrude(d);
                m.validate().unwrap();
                assert!((m.volume() - 24.0).abs() < 1e-9, "{d}");
                // the concave caps triangulate with n - 2 triangles each
                let caps: Vec<_> = (0..2).map(|f| m.face_triangles(f)).collect();
                assert!(caps.iter().all(|t| t.len() == 4));
            }
        }
    }

    #[test]
    fn an_open_sketch_extrudes_into_a_surface() {
        let m = l_shape(Workplane::ground(0.0), false).extrude(3.0);
        assert_eq!(m.faces.len(), 5);
        assert_eq!(m.vertices.len(), 12);
    }
}
