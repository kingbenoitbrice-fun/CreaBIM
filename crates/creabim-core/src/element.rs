use crate::{editmesh::EditMesh, ElementId};
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
            ElementKind::Opening(o) => vec![o.host],
            ElementKind::FreeMesh(m) => m.level.into_iter().collect(),
            ElementKind::View(v) => match &v.kind {
                ViewKind::Plan { level, .. } => vec![*level],
                ViewKind::Section { .. } | ViewKind::Axonometric { .. } => vec![],
            },
            ElementKind::Sheet(_) => vec![],
            ElementKind::Viewport(v) => vec![v.sheet, v.view],
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
            ElementKind::FreeMesh(m) => m.mesh.validate(),
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
                }
            }
            ElementKind::Sheet(sh) => {
                positive(sh.width, "sheet width")?;
                positive(sh.height, "sheet height")
            }
            ElementKind::Viewport(v) => {
                if v.center.is_finite() && v.target.is_finite() {
                    Ok(())
                } else {
                    Err("viewport position must be finite".into())
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
}

/// A drawing of the model. Views live in the model like any element, so they
/// are saved, undone and (later) placed on sheets. Their drawings are always
/// generated from the current model, never stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub kind: ViewKind,
    /// Drawing scale denominator: 100 means 1:100.
    pub scale: f64,
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
}

impl View {
    pub fn plan(level: ElementId) -> Self {
        Self {
            kind: ViewKind::Plan {
                level,
                cut_height: 1.2,
            },
            scale: 100.0,
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
        }
    }
}

impl View {
    pub fn axonometric(azimuth: f64, altitude: f64) -> Self {
        Self {
            kind: ViewKind::Axonometric { azimuth, altitude },
            scale: 100.0,
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
