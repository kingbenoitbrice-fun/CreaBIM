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
