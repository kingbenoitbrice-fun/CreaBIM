//! Element types: named sets of shared parameters that walls, doors,
//! windows, slabs, roofs, columns and beams are instances of, like Revit's
//! family types or ArchiCAD's favourites.
//!
//! A type is an element of its own ([`ElementKind::ElementType`]), so it
//! saves with the project, has a name and undoes like anything else. It
//! holds what its instances share: a wall type its assembly (or thickness
//! and material), location line, default height and curtain grid; a door
//! or window type its size, sill and frame; and so on. An instance keeps
//! its own placement, and names (in its [`TypeRef`]) the parameters it
//! sets for itself: its overrides.
//!
//! The type's values are written into each instance, so geometry, drawings
//! and exports that only read the instance keep working. Editing a type
//! rewrites every instance (but not the parameters they override) in one
//! transaction; swapping the type of many elements at once is
//! [`assign`]. Overrides are found, not declared: after every edit,
//! [`settle`] marks each parameter that differs from the type, so a door
//! widened by its grip keeps its width when its type changes.
//!
//! Layers stay in [`crate::assembly`]: a wall, slab or roof type names an
//! assembly rather than holding layers of its own.

use crate::curtain::{CurtainGrid, GridRule, PanelKind};
use crate::{
    assembly, Element, ElementId, ElementKind, LocationLine, Model, Op, OpeningFrame, OpeningKind,
    Profile, Transaction,
};
use serde::{Deserialize, Serialize};

/// A parameter a type holds and an instance may override.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Param {
    Thickness,
    Assembly,
    Material,
    Location,
    Curtain,
    Width,
    Height,
    Sill,
    Frame,
    Profile,
    Depth,
    /// A pad footing's side along its own y axis.
    Length,
}

impl Param {
    pub fn label(self) -> &'static str {
        match self {
            Param::Thickness => "Thickness",
            Param::Assembly => "Assembly",
            Param::Material => "Material",
            Param::Location => "Location line",
            Param::Curtain => "Curtain grid",
            Param::Width => "Width",
            Param::Height => "Height",
            Param::Sill => "Sill",
            Param::Frame => "Frame and panes",
            Param::Profile => "Profile",
            Param::Depth => "Depth",
            Param::Length => "Length",
        }
    }
}

/// An instance's type, and the parameters it overrides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypeRef {
    pub id: ElementId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<Param>,
}

impl TypeRef {
    pub fn new(id: ElementId) -> Self {
        Self {
            id,
            overrides: vec![],
        }
    }
}

/// What kind of element a type is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeCategory {
    Wall,
    Door,
    Window,
    Slab,
    Roof,
    Column,
    Beam,
    /// Strip and pad footings.
    Footing,
    /// Furniture and fixtures, placed from the catalogue (not drawn with a
    /// tool, so not in [`TypeCategory::ALL`]).
    Object,
}

impl TypeCategory {
    pub const ALL: [TypeCategory; 8] = [
        TypeCategory::Wall,
        TypeCategory::Door,
        TypeCategory::Window,
        TypeCategory::Slab,
        TypeCategory::Roof,
        TypeCategory::Column,
        TypeCategory::Beam,
        TypeCategory::Footing,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TypeCategory::Wall => "Walls",
            TypeCategory::Door => "Doors",
            TypeCategory::Window => "Windows",
            TypeCategory::Slab => "Slabs",
            TypeCategory::Roof => "Roofs",
            TypeCategory::Column => "Columns",
            TypeCategory::Beam => "Beams",
            TypeCategory::Footing => "Footings",
            TypeCategory::Object => "Objects",
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            TypeCategory::Wall => "wall",
            TypeCategory::Door => "door",
            TypeCategory::Window => "window",
            TypeCategory::Slab => "slab",
            TypeCategory::Roof => "roof",
            TypeCategory::Column => "column",
            TypeCategory::Beam => "beam",
            TypeCategory::Footing => "footing",
            TypeCategory::Object => "object",
        }
    }

    /// The category an element's type would be in; none for kinds without
    /// types (and bare holes).
    pub fn of(kind: &ElementKind) -> Option<TypeCategory> {
        Some(match kind {
            ElementKind::Wall(_) => TypeCategory::Wall,
            ElementKind::Opening(o) => match o.kind {
                OpeningKind::Door => TypeCategory::Door,
                OpeningKind::Window => TypeCategory::Window,
                OpeningKind::Hole => return None,
            },
            ElementKind::Slab(_) => TypeCategory::Slab,
            ElementKind::Roof(_) => TypeCategory::Roof,
            ElementKind::Column(_) => TypeCategory::Column,
            ElementKind::Beam(_) => TypeCategory::Beam,
            ElementKind::Footing(_) => TypeCategory::Footing,
            ElementKind::Object(_) => TypeCategory::Object,
            _ => return None,
        })
    }

    /// The parameters a type of this category holds, in the order shown.
    pub fn params(self) -> &'static [Param] {
        match self {
            TypeCategory::Wall => &[
                Param::Curtain,
                Param::Assembly,
                Param::Thickness,
                Param::Material,
                Param::Location,
            ],
            TypeCategory::Door | TypeCategory::Window => &[
                Param::Width,
                Param::Height,
                Param::Sill,
                Param::Frame,
                Param::Material,
            ],
            TypeCategory::Slab | TypeCategory::Roof => {
                &[Param::Assembly, Param::Thickness, Param::Material]
            }
            TypeCategory::Column => &[Param::Profile, Param::Material],
            TypeCategory::Beam => &[Param::Width, Param::Depth, Param::Material],
            TypeCategory::Footing => &[
                Param::Width,
                Param::Length,
                Param::Thickness,
                Param::Material,
            ],
            // an object's shape is its type's alone
            TypeCategory::Object => &[],
        }
    }
}

/// What walls of a type share.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WallType {
    pub thickness: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<ElementId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<ElementId>,
    #[serde(default, skip_serializing_if = "LocationLine::is_centre")]
    pub location: LocationLine,
    /// The height new walls of the type are drawn at; none takes their
    /// level's height. Only new walls take it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    /// A curtain wall type's grid, mullions and panels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curtain: Option<CurtainGrid>,
}

/// What doors or windows of a type share.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OpeningType {
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub sill: f64,
    #[serde(default)]
    pub frame: OpeningFrame,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<ElementId>,
}

/// What slabs or roofs of a type share.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlabType {
    pub thickness: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<ElementId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<ElementId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColumnType {
    pub profile: Profile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<ElementId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeamType {
    pub width: f64,
    pub depth: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<ElementId>,
}

/// What footings of a type share. A strip type's `length` is not used.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FootingType {
    pub kind: crate::FootingKind,
    pub width: f64,
    #[serde(default)]
    pub length: f64,
    pub thickness: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<ElementId>,
}

/// A type's parameters, by category.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TypeParams {
    Wall(WallType),
    Door(OpeningType),
    Window(OpeningType),
    Slab(SlabType),
    Roof(SlabType),
    Column(ColumnType),
    Beam(BeamType),
    Object(crate::objects::ObjectType),
    Footing(FootingType),
}

/// An element type. The element's name is the type's name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ElementType {
    pub params: TypeParams,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl ElementType {
    pub fn new(params: TypeParams) -> Self {
        Self {
            params,
            description: String::new(),
        }
    }

    pub fn category(&self) -> TypeCategory {
        match &self.params {
            TypeParams::Wall(_) => TypeCategory::Wall,
            TypeParams::Door(_) => TypeCategory::Door,
            TypeParams::Window(_) => TypeCategory::Window,
            TypeParams::Slab(_) => TypeCategory::Slab,
            TypeParams::Roof(_) => TypeCategory::Roof,
            TypeParams::Column(_) => TypeCategory::Column,
            TypeParams::Beam(_) => TypeCategory::Beam,
            TypeParams::Object(_) => TypeCategory::Object,
            TypeParams::Footing(_) => TypeCategory::Footing,
        }
    }

    pub fn is_curtain(&self) -> bool {
        matches!(&self.params, TypeParams::Wall(w) if w.curtain.is_some())
    }

    /// The heading it is listed under: curtain walls apart from walls.
    pub fn group_label(&self) -> &'static str {
        if self.is_curtain() {
            "Curtain walls"
        } else {
            self.category().label()
        }
    }

    /// The IFC type entity it exports as.
    pub fn ifc_class(&self) -> &'static str {
        match &self.params {
            TypeParams::Wall(w) if w.curtain.is_some() => "IfcCurtainWallType",
            TypeParams::Wall(_) => "IfcWallType",
            TypeParams::Door(_) => "IfcDoorType",
            TypeParams::Window(_) => "IfcWindowType",
            TypeParams::Slab(_) => "IfcSlabType",
            TypeParams::Roof(_) => "IfcRoofType",
            TypeParams::Column(_) => "IfcColumnType",
            TypeParams::Beam(_) => "IfcBeamType",
            TypeParams::Footing(_) => "IfcFootingType",
            TypeParams::Object(o) => o.ifc.type_class(),
        }
    }

    pub fn material(&self) -> Option<ElementId> {
        match &self.params {
            TypeParams::Wall(w) => w.material,
            TypeParams::Door(o) | TypeParams::Window(o) => o.material,
            TypeParams::Slab(s) | TypeParams::Roof(s) => s.material,
            TypeParams::Column(c) => c.material,
            TypeParams::Beam(b) => b.material,
            TypeParams::Footing(f) => f.material,
            TypeParams::Object(o) => o.materials().first().copied(),
        }
    }

    pub fn assembly(&self) -> Option<ElementId> {
        match &self.params {
            TypeParams::Wall(w) if w.curtain.is_none() => w.assembly,
            TypeParams::Slab(s) | TypeParams::Roof(s) => s.assembly,
            _ => None,
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
        match &self.params {
            TypeParams::Wall(w) => {
                positive(w.thickness, "wall type thickness")?;
                if let Some(h) = w.height {
                    positive(h, "wall type height")?;
                }
                match &w.curtain {
                    Some(c) => c.validate(),
                    None => Ok(()),
                }
            }
            TypeParams::Door(o) | TypeParams::Window(o) => {
                positive(o.width, "type width")?;
                positive(o.height, "type height")?;
                if !(o.sill.is_finite() && o.sill >= 0.0) {
                    return Err("type sill must not be negative".into());
                }
                Ok(())
            }
            TypeParams::Slab(s) | TypeParams::Roof(s) => positive(s.thickness, "type thickness"),
            TypeParams::Column(c) => match c.profile {
                Profile::Rectangle { width, depth } => {
                    positive(width, "column width")?;
                    positive(depth, "column depth")
                }
                Profile::Round { diameter } => positive(diameter, "column diameter"),
            },
            TypeParams::Beam(b) => {
                positive(b.width, "beam width")?;
                positive(b.depth, "beam depth")
            }
            TypeParams::Object(o) => o.validate(),
            TypeParams::Footing(f) => {
                positive(f.width, "footing width")?;
                positive(f.thickness, "footing thickness")?;
                if f.kind == crate::FootingKind::Pad {
                    positive(f.length, "footing length")?;
                }
                Ok(())
            }
        }
    }

    /// A short summary: "200 mm", "Exterior wall 365…", "900 × 2100".
    pub fn summary(&self, model: &Model) -> String {
        let u = model.units();
        let d = |v: f64| u.dimension(v);
        let name = |id: Option<ElementId>| id.and_then(|i| model.get(i)).map(|e| e.name.clone());
        match &self.params {
            TypeParams::Wall(w) => match &w.curtain {
                Some(c) => format!(
                    "{} grid {} × {}, {} panels",
                    "Curtain",
                    rule_text(&u, c.vertical),
                    rule_text(&u, c.horizontal),
                    c.panel.label().to_lowercase()
                ),
                None => name(w.assembly).unwrap_or_else(|| {
                    let m = name(w.material).map_or(String::new(), |m| format!(", {m}"));
                    format!("{} thick{m}", d(w.thickness))
                }),
            },
            TypeParams::Door(o) | TypeParams::Window(o) => {
                let mut s = format!("{} × {}", d(o.width), d(o.height));
                if o.sill > 0.0 {
                    s.push_str(&format!(", sill {}", d(o.sill)));
                }
                s
            }
            TypeParams::Slab(s) | TypeParams::Roof(s) => {
                name(s.assembly).unwrap_or_else(|| format!("{} thick", d(s.thickness)))
            }
            TypeParams::Column(c) => c.profile.label_in(&u),
            TypeParams::Beam(b) => format!("{} × {}", d(b.width), d(b.depth)),
            TypeParams::Footing(f) => match f.kind {
                crate::FootingKind::Strip => {
                    format!("strip {} wide, {} thick", d(f.width), d(f.thickness))
                }
                crate::FootingKind::Pad => format!(
                    "pad {} × {}, {} thick",
                    d(f.width),
                    d(f.length),
                    d(f.thickness)
                ),
            },
            TypeParams::Object(o) => o.summary(model),
        }
    }
}

fn rule_text(u: &crate::Units, r: GridRule) -> String {
    match r {
        GridRule::Spacing(s) => u.dimension(s),
        GridRule::Count(n) => format!("{n} bays"),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// The type of an element, if it has one that still exists.
pub fn type_of<'a>(model: &'a Model, e: &Element) -> Option<(&'a Element, &'a ElementType)> {
    let r = e.of_type.as_ref()?;
    let t = model.get(r.id)?;
    match &t.kind {
        ElementKind::ElementType(ty) => Some((t, ty)),
        _ => None,
    }
}

/// Can elements of `kind` be of this type?
pub fn fits(params: &TypeParams, kind: &ElementKind) -> bool {
    matches!(
        (params, kind),
        (TypeParams::Wall(_), ElementKind::Wall(_))
            | (TypeParams::Slab(_), ElementKind::Slab(_))
            | (TypeParams::Roof(_), ElementKind::Roof(_))
            | (TypeParams::Column(_), ElementKind::Column(_))
            | (TypeParams::Beam(_), ElementKind::Beam(_))
            | (TypeParams::Object(_), ElementKind::Object(_))
    ) || matches!((params, kind),
        (TypeParams::Footing(t), ElementKind::Footing(f)) if t.kind == f.kind)
        || matches!((params, kind),
        (TypeParams::Door(_), ElementKind::Opening(o)) if o.kind == OpeningKind::Door)
        || matches!((params, kind),
        (TypeParams::Window(_), ElementKind::Opening(o)) if o.kind == OpeningKind::Window)
}

/// A type's parameters taken from an element: what "Make a type of this"
/// starts from.
pub fn from_instance(e: &Element) -> Option<TypeParams> {
    Some(match &e.kind {
        ElementKind::Wall(w) => TypeParams::Wall(WallType {
            thickness: w.thickness,
            assembly: w.assembly,
            material: w.material,
            location: w.location,
            height: None,
            curtain: w.curtain.clone(),
        }),
        ElementKind::Opening(o) => {
            let t = OpeningType {
                width: o.width,
                height: o.height,
                sill: o.sill,
                frame: o.frame,
                material: o.material,
            };
            match o.kind {
                OpeningKind::Door => TypeParams::Door(t),
                OpeningKind::Window => TypeParams::Window(t),
                OpeningKind::Hole => return None,
            }
        }
        ElementKind::Slab(s) => TypeParams::Slab(SlabType {
            thickness: s.thickness,
            assembly: s.assembly,
            material: s.material,
        }),
        ElementKind::Roof(r) => TypeParams::Roof(SlabType {
            thickness: r.thickness,
            assembly: r.assembly,
            material: r.material,
        }),
        ElementKind::Column(c) => TypeParams::Column(ColumnType {
            profile: c.profile,
            material: c.material,
        }),
        ElementKind::Beam(b) => TypeParams::Beam(BeamType {
            width: b.width,
            depth: b.depth,
            material: b.material,
        }),
        ElementKind::Footing(f) => TypeParams::Footing(FootingType {
            kind: f.kind,
            width: f.width,
            length: f.length,
            thickness: f.thickness,
            material: f.material,
        }),
        _ => return None,
    })
}

/// The parameters in which an element differs from a type: what it
/// overrides. A layered element's thickness follows its layers, so it
/// never counts on its own.
pub fn differences(params: &TypeParams, kind: &ElementKind) -> Vec<Param> {
    let mut out = vec![];
    let mut flag = |p: Param, differs: bool| {
        if differs {
            out.push(p);
        }
    };
    match (params, kind) {
        (TypeParams::Wall(t), ElementKind::Wall(w)) => {
            flag(Param::Curtain, t.curtain != w.curtain);
            if t.curtain.is_none() && w.curtain.is_none() {
                flag(Param::Assembly, t.assembly != w.assembly);
                flag(
                    Param::Thickness,
                    t.assembly.is_none() && w.assembly.is_none() && !near(t.thickness, w.thickness),
                );
            }
            flag(Param::Material, t.material != w.material);
            flag(Param::Location, t.location != w.location);
        }
        (TypeParams::Door(t) | TypeParams::Window(t), ElementKind::Opening(o)) => {
            flag(Param::Width, !near(t.width, o.width));
            flag(Param::Height, !near(t.height, o.height));
            flag(Param::Sill, !near(t.sill, o.sill));
            flag(Param::Frame, t.frame != o.frame);
            flag(Param::Material, t.material != o.material);
        }
        (TypeParams::Slab(t), ElementKind::Slab(s)) => {
            flag(Param::Assembly, t.assembly != s.assembly);
            flag(
                Param::Thickness,
                t.assembly.is_none() && s.assembly.is_none() && !near(t.thickness, s.thickness),
            );
            flag(Param::Material, t.material != s.material);
        }
        (TypeParams::Roof(t), ElementKind::Roof(r)) => {
            flag(Param::Assembly, t.assembly != r.assembly);
            flag(
                Param::Thickness,
                t.assembly.is_none() && r.assembly.is_none() && !near(t.thickness, r.thickness),
            );
            flag(Param::Material, t.material != r.material);
        }
        (TypeParams::Column(t), ElementKind::Column(c)) => {
            flag(Param::Profile, t.profile != c.profile);
            flag(Param::Material, t.material != c.material);
        }
        (TypeParams::Beam(t), ElementKind::Beam(b)) => {
            flag(Param::Width, !near(t.width, b.width));
            flag(Param::Depth, !near(t.depth, b.depth));
            flag(Param::Material, t.material != b.material);
        }
        (TypeParams::Footing(t), ElementKind::Footing(f)) => {
            flag(Param::Width, !near(t.width, f.width));
            flag(
                Param::Length,
                f.kind == crate::FootingKind::Pad && !near(t.length, f.length),
            );
            flag(Param::Thickness, !near(t.thickness, f.thickness));
            flag(Param::Material, t.material != f.material);
        }
        _ => {}
    }
    out
}

/// Write a type's parameters into an element, all but those in `keep`.
/// A layered element ends up as thick as its layers.
pub fn apply(model: &Model, params: &TypeParams, e: &mut Element, keep: &[Param]) {
    let set = |p: Param| !keep.contains(&p);
    match (params, &mut e.kind) {
        (TypeParams::Wall(t), ElementKind::Wall(w)) => {
            if set(Param::Curtain) {
                w.curtain = t.curtain.clone();
            }
            match &w.curtain {
                Some(c) => {
                    if set(Param::Curtain) {
                        w.thickness = c.mullion_depth;
                        w.assembly = None;
                    }
                }
                None => {
                    if set(Param::Assembly) {
                        w.assembly = t.assembly;
                    }
                    if set(Param::Thickness) && set(Param::Assembly) {
                        w.thickness = t.thickness;
                    }
                }
            }
            if set(Param::Material) {
                w.material = t.material;
            }
            if set(Param::Location) {
                w.location = t.location;
            }
        }
        (TypeParams::Door(t) | TypeParams::Window(t), ElementKind::Opening(o)) => {
            if set(Param::Width) {
                o.width = t.width;
            }
            if set(Param::Height) {
                o.height = t.height;
            }
            if set(Param::Sill) {
                o.sill = t.sill;
            }
            if set(Param::Frame) {
                o.frame = t.frame;
            }
            if set(Param::Material) {
                o.material = t.material;
            }
        }
        (TypeParams::Slab(t), ElementKind::Slab(s)) => {
            if set(Param::Assembly) {
                s.assembly = t.assembly;
            }
            if set(Param::Thickness) && set(Param::Assembly) {
                s.set_thickness(t.thickness, false);
            }
            if set(Param::Material) {
                s.material = t.material;
            }
        }
        (TypeParams::Roof(t), ElementKind::Roof(r)) => {
            if set(Param::Assembly) {
                r.assembly = t.assembly;
            }
            if set(Param::Thickness) && set(Param::Assembly) {
                r.thickness = t.thickness;
            }
            if set(Param::Material) {
                r.material = t.material;
            }
        }
        (TypeParams::Column(t), ElementKind::Column(c)) => {
            if set(Param::Profile) {
                c.profile = t.profile;
            }
            if set(Param::Material) {
                c.material = t.material;
            }
        }
        (TypeParams::Beam(t), ElementKind::Beam(b)) => {
            if set(Param::Width) {
                b.width = t.width;
            }
            if set(Param::Depth) {
                b.depth = t.depth;
            }
            if set(Param::Material) {
                b.material = t.material;
            }
        }
        (TypeParams::Footing(t), ElementKind::Footing(f)) => {
            if set(Param::Width) {
                f.width = t.width;
            }
            if set(Param::Length) && f.kind == crate::FootingKind::Pad {
                f.length = t.length;
            }
            if set(Param::Thickness) {
                f.thickness = t.thickness;
            }
            if set(Param::Material) {
                f.material = t.material;
            }
        }
        _ => return,
    }
    // a layered element is as thick as its layers
    let slot = assembly::assembly_slot(e).flatten();
    if slot.is_some() {
        assembly::set_assembly(model, e, slot);
    }
}

/// Make a new element an instance of `type_id`, taking all its values (a
/// wall also its default height). False when the type doesn't fit.
pub fn stamp(model: &Model, e: &mut Element, type_id: ElementId) -> bool {
    let Some(ElementKind::ElementType(t)) = model.get(type_id).map(|t| &t.kind) else {
        return false;
    };
    if !fits(&t.params, &e.kind) {
        return false;
    }
    apply(model, &t.params, e, &[]);
    if let (TypeParams::Wall(wt), ElementKind::Wall(w)) = (&t.params, &mut e.kind) {
        if let Some(h) = wt.height {
            w.height = h;
        }
    }
    e.of_type = Some(TypeRef::new(type_id));
    true
}

/// Every type of the model: (id, name, type), in model order.
pub fn types(model: &Model) -> Vec<(ElementId, &str, &ElementType)> {
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::ElementType(t) => Some((e.id, e.name.as_str(), t)),
            _ => None,
        })
        .collect()
}

/// The types elements of `kind` could be.
pub fn types_for<'a>(
    model: &'a Model,
    kind: &ElementKind,
) -> Vec<(ElementId, &'a str, &'a ElementType)> {
    types(model)
        .into_iter()
        .filter(|(_, _, t)| fits(&t.params, kind))
        .collect()
}

/// The elements of a type.
pub fn instances(model: &Model, type_id: ElementId) -> Vec<ElementId> {
    model
        .iter()
        .filter(|e| e.of_type.as_ref().is_some_and(|r| r.id == type_id))
        .map(|e| e.id)
        .collect()
}

/// Make each of `elements` (those it fits) an instance of `type_id`,
/// dropping their overrides, or with `None` of no type (keeping their
/// values). Returns the transaction and how many elements it changes.
pub fn assign(
    model: &Model,
    elements: &[ElementId],
    type_id: Option<ElementId>,
) -> (Transaction, usize) {
    let mut tx = Transaction::new("Change type");
    let mut n = 0;
    for id in elements {
        let Some(e) = model.get(*id) else { continue };
        let mut after = e.clone();
        match type_id {
            Some(t) => {
                if !stamp_keeping_height(model, &mut after, t) {
                    continue;
                }
            }
            None => {
                // an object can't be without its type: it is its shape
                if e.of_type.is_none() || matches!(e.kind, ElementKind::Object(_)) {
                    continue;
                }
                after.of_type = None;
            }
        }
        if after != *e {
            tx = tx.replace(e.clone(), after);
            n += 1;
        }
    }
    (tx, n)
}

/// [`stamp`] for an element that is already placed: a wall keeps its height.
fn stamp_keeping_height(model: &Model, e: &mut Element, type_id: ElementId) -> bool {
    let height = match &e.kind {
        ElementKind::Wall(w) => Some(w.height),
        _ => None,
    };
    if !stamp(model, e, type_id) {
        return false;
    }
    if let (Some(h), ElementKind::Wall(w)) = (height, &mut e.kind) {
        w.height = h;
    }
    true
}

/// Change a type, and every instance of it but for what each overrides.
/// One undo step.
pub fn edit(model: &Model, id: ElementId, new: ElementType, label: &str) -> Option<Transaction> {
    let e = model.get(id)?;
    let ElementKind::ElementType(old) = &e.kind else {
        return None;
    };
    if old.category() != new.category() {
        return None;
    }
    let mut after = e.clone();
    after.kind = ElementKind::ElementType(new.clone());
    let mut tx = Transaction::new(label).replace(e.clone(), after);
    for u in instances(model, id) {
        let Some(ue) = model.get(u) else { continue };
        let keep = ue
            .of_type
            .as_ref()
            .map(|r| r.overrides.clone())
            .unwrap_or_default();
        let mut ua = ue.clone();
        apply(model, &new.params, &mut ua, &keep);
        if ua != *ue {
            tx = tx.replace(ue.clone(), ua);
        }
    }
    Some(tx)
}

/// Put an instance back to its type: drop its overrides.
pub fn reset(model: &Model, id: ElementId) -> Option<Transaction> {
    let e = model.get(id)?;
    let (_, t) = type_of(model, e)?;
    let mut after = e.clone();
    apply(model, &t.params, &mut after, &[]);
    if let Some(r) = &mut after.of_type {
        r.overrides.clear();
    }
    Some(Transaction::new("Reset to type").replace(e.clone(), after))
}

/// Make an instance's values its type's, so every other instance follows.
pub fn save_to_type(model: &Model, id: ElementId) -> Option<Transaction> {
    let e = model.get(id)?;
    let (te, t) = type_of(model, e)?;
    let mut params = from_instance(e)?;
    if let (TypeParams::Wall(new), TypeParams::Wall(old)) = (&mut params, &t.params) {
        new.height = old.height;
    }
    let mut new = t.clone();
    new.params = params;
    let mut tx = edit(model, te.id, new, "Update type from element")?;
    // the element itself overrides nothing any more
    let mut after = e.clone();
    if let Some(r) = &mut after.of_type {
        r.overrides.clear();
    }
    if after != *e {
        tx = tx.replace(e.clone(), after);
    }
    Some(tx)
}

/// Delete a type; its instances keep their values, with no type. One undo step.
pub fn delete(model: &Model, id: ElementId) -> Transaction {
    let (mut tx, _) = assign(model, &instances(model, id), None);
    tx.label = "Delete type".into();
    tx.remove_cascade(model, id)
}

/// A name no other type of the model has: `base`, else `base 2`...
pub fn unique_name(model: &Model, base: &str) -> String {
    let taken: Vec<&str> = types(model).into_iter().map(|(_, n, _)| n).collect();
    if !taken.contains(&base) {
        return base.to_string();
    }
    (2..)
        .map(|i| format!("{base} {i}"))
        .find(|n| !taken.contains(&n.as_str()))
        .expect("an unused name exists")
}

/// A copy of a type under an unused name, not yet in the model.
pub fn duplicate(model: &Model, id: ElementId) -> Option<Element> {
    let e = model.get(id)?;
    let ElementKind::ElementType(t) = &e.kind else {
        return None;
    };
    let name = unique_name(model, &format!("{} copy", e.name));
    let mut t = t.clone();
    // a copy is no longer the catalogue's object: placing that again
    // should not find this one
    if let TypeParams::Object(o) = &mut t.params {
        o.catalogue = None;
    }
    Some(Element::new(name, ElementKind::ElementType(t)))
}

/// A new type made from an element, and the element made its instance:
/// (the type, the element after).
pub fn from_element(model: &Model, id: ElementId, name: &str) -> Option<(Element, Element)> {
    let e = model.get(id)?;
    let params = from_instance(e)?;
    let t = Element::new(
        unique_name(model, name),
        ElementKind::ElementType(ElementType::new(params)),
    );
    let mut after = e.clone();
    after.of_type = Some(TypeRef::new(t.id));
    Some((t, after))
}

/// After an edit: each instance the edit touched overrides just what now
/// differs from its type. Run by [`crate::Document`] on every commit, so
/// grips, schedules and Properties all mark overrides the same way.
pub fn settle(model: &Model, tx: &Transaction) -> Transaction {
    let mut out = Transaction::new("Settle types");
    let mut seen = std::collections::HashSet::new();
    for op in &tx.ops {
        let id = match op {
            Op::Insert(e) => e.id,
            Op::Replace { after, .. } => after.id,
            Op::Remove(_) => continue,
        };
        if !seen.insert(id) {
            continue;
        }
        let Some(e) = model.get(id) else { continue };
        let Some((_, t)) = type_of(model, e) else {
            continue;
        };
        let mut now = differences(&t.params, &e.kind);
        now.sort();
        let r = e.of_type.as_ref().expect("has a type");
        let mut had = r.overrides.clone();
        had.sort();
        if now != had {
            let mut after = e.clone();
            if let Some(r) = &mut after.of_type {
                r.overrides = now;
            }
            out = out.replace(e.clone(), after);
        }
    }
    out
}

/// Does `tx` touch an element that has a type?
pub fn touches_instances(tx: &Transaction) -> bool {
    tx.ops.iter().any(|op| match op {
        Op::Insert(e) | Op::Replace { after: e, .. } => e.of_type.is_some(),
        Op::Remove(_) => false,
    })
}

type Starter = (&'static str, &'static str, fn(&[ElementId]) -> TypeParams);

fn wall(thickness: f64, assembly: Option<ElementId>) -> TypeParams {
    TypeParams::Wall(WallType {
        thickness,
        assembly,
        material: None,
        location: LocationLine::Centre,
        height: None,
        curtain: None,
    })
}

fn curtain(vertical: GridRule, horizontal: GridRule, panel: PanelKind) -> TypeParams {
    let c = CurtainGrid {
        vertical,
        horizontal,
        panel,
        ..Default::default()
    };
    TypeParams::Wall(WallType {
        thickness: c.mullion_depth,
        assembly: None,
        material: None,
        location: LocationLine::Centre,
        height: None,
        curtain: Some(c),
    })
}

fn opening(width: f64, height: f64, sill: f64, across: u32, up: u32) -> OpeningType {
    OpeningType {
        width,
        height,
        sill,
        frame: OpeningFrame {
            panes_across: across,
            panes_up: up,
            ..Default::default()
        },
        material: None,
    }
}

/// The starter types: (name, description, parameters from the starter
/// assemblies' ids).
pub const STARTERS: &[Starter] = &[
    ("Generic wall 200", "A plain 200 mm wall", |_| {
        wall(0.2, None)
    }),
    ("Partition 100", "A plain 100 mm partition", |_| {
        wall(0.1, None)
    }),
    (
        "Exterior wall 365, brick cavity",
        "Built of the cavity wall assembly",
        |a| wall(0.365, a.first().copied()),
    ),
    (
        "Interior wall 126, block",
        "Built of the block partition assembly",
        |a| wall(0.126, a.get(1).copied()),
    ),
    (
        "Curtain wall 1500 grid",
        "Glazing on a 1.5 m square grid",
        |_| {
            curtain(
                GridRule::Spacing(1.5),
                GridRule::Spacing(1.5),
                PanelKind::Glass,
            )
        },
    ),
    (
        "Storefront, 4 bays",
        "Four equal glazed bays, a transom at 2.4 m",
        |_| curtain(GridRule::Count(4), GridRule::Spacing(2.4), PanelKind::Glass),
    ),
    ("Single door 900 × 2100", "", |_| {
        TypeParams::Door(opening(0.9, 2.1, 0.0, 1, 1))
    }),
    ("Single door 800 × 2100", "", |_| {
        TypeParams::Door(opening(0.8, 2.1, 0.0, 1, 1))
    }),
    ("Wide door 1200 × 2100", "", |_| {
        TypeParams::Door(opening(1.2, 2.1, 0.0, 1, 1))
    }),
    ("Window 1200 × 1300, two panes", "", |_| {
        TypeParams::Window(opening(1.2, 1.3, 0.9, 2, 1))
    }),
    ("Window 600 × 1300", "", |_| {
        TypeParams::Window(opening(0.6, 1.3, 0.9, 1, 1))
    }),
    ("Tall window 900 × 2100", "", |_| {
        TypeParams::Window(opening(0.9, 2.1, 0.1, 1, 3))
    }),
    ("Slab 200", "A plain 200 mm slab", |_| {
        TypeParams::Slab(SlabType {
            thickness: 0.2,
            assembly: None,
            material: None,
        })
    }),
    (
        "Floor 300, concrete",
        "Built of the concrete floor assembly",
        |a| {
            TypeParams::Slab(SlabType {
                thickness: 0.3,
                assembly: a.get(2).copied(),
                material: None,
            })
        },
    ),
    ("Roof 250", "A plain 250 mm roof", |_| {
        TypeParams::Roof(SlabType {
            thickness: 0.25,
            assembly: None,
            material: None,
        })
    }),
    (
        "Pitched roof 288, tiled",
        "Built of the tiled roof assembly",
        |a| {
            TypeParams::Roof(SlabType {
                thickness: 0.288,
                assembly: a.get(3).copied(),
                material: None,
            })
        },
    ),
    ("Column 300 × 300", "", |_| {
        TypeParams::Column(ColumnType {
            profile: Profile::Rectangle {
                width: 0.3,
                depth: 0.3,
            },
            material: None,
        })
    }),
    ("Column Ø 300", "", |_| {
        TypeParams::Column(ColumnType {
            profile: Profile::Round { diameter: 0.3 },
            material: None,
        })
    }),
    ("Beam 200 × 400", "", |_| {
        TypeParams::Beam(BeamType {
            width: 0.2,
            depth: 0.4,
            material: None,
        })
    }),
    ("Beam 300 × 600", "", |_| {
        TypeParams::Beam(BeamType {
            width: 0.3,
            depth: 0.6,
            material: None,
        })
    }),
    (
        "Strip footing 600 × 300",
        "A concrete strip under a wall",
        |_| footing(crate::FootingKind::Strip, 0.6, 0.0, 0.3),
    ),
    (
        "Strip footing 900 × 400",
        "A wider strip, for heavier walls",
        |_| footing(crate::FootingKind::Strip, 0.9, 0.0, 0.4),
    ),
    (
        "Pad footing 1200 × 1200 × 400",
        "A square pad under a column",
        |_| footing(crate::FootingKind::Pad, 1.2, 1.2, 0.4),
    ),
    (
        "Pad footing 1800 × 1800 × 500",
        "A larger pad, for heavier columns",
        |_| footing(crate::FootingKind::Pad, 1.8, 1.8, 0.5),
    ),
];

fn footing(kind: crate::FootingKind, width: f64, length: f64, thickness: f64) -> TypeParams {
    TypeParams::Footing(FootingType {
        kind,
        width,
        length,
        thickness,
        material: None,
    })
}

/// Add the starter types the model doesn't have yet (by name), with the
/// starter assemblies they are built of. Returns the transaction and the
/// ids of the starters (new or already there), in [`STARTERS`] order.
pub fn add_starters(model: &Model) -> (Transaction, Vec<ElementId>) {
    let (mut tx, assemblies) = assembly::add_starters(model);
    tx.label = "Add starter types".into();
    // the assemblies' totals, for thicknesses, whether new or already there
    let mut scratch = model.clone();
    let _ = tx.apply(&mut scratch);
    let mut ids = vec![];
    for (name, description, make) in STARTERS {
        if let Some((id, _, _)) = types(model).into_iter().find(|(_, n, _)| n == name) {
            ids.push(id);
            continue;
        }
        let mut t = ElementType::new(make(&assemblies));
        t.description = description.to_string();
        if let Some(a) = t.assembly() {
            let total = match scratch.get(a).map(|e| &e.kind) {
                Some(ElementKind::Assembly(a)) => Some(a.total()),
                _ => None,
            };
            if let Some(total) = total {
                match &mut t.params {
                    TypeParams::Wall(w) => w.thickness = total,
                    TypeParams::Slab(s) | TypeParams::Roof(s) => s.thickness = total,
                    _ => {}
                }
            }
        }
        let e = Element::new(*name, ElementKind::ElementType(t));
        ids.push(e.id);
        tx = tx.insert(e);
    }
    (tx, ids)
}

/// Parameters for a new type of `category` (a curtain wall type with
/// `curtain`): the first starter's of that kind, built of no assembly.
pub fn default_params(category: TypeCategory, curtain: bool) -> TypeParams {
    STARTERS
        .iter()
        .map(|(_, _, make)| make(&[]))
        .find(|p| {
            let t = ElementType::new(p.clone());
            t.category() == category
                && t.is_curtain() == (curtain && category == TypeCategory::Wall)
        })
        .expect("a starter of every category")
}

/// The first starter type of a category, or the first type of it.
pub fn first_of(model: &Model, category: TypeCategory, curtain: bool) -> Option<ElementId> {
    types(model)
        .into_iter()
        .find(|(_, _, t)| t.category() == category && t.is_curtain() == curtain)
        .map(|(id, _, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    fn house() -> (Document, Vec<ElementId>) {
        let mut doc = Document::from_model(crate::sample::sample_house());
        let (tx, ids) = add_starters(doc.model());
        doc.commit(tx).unwrap();
        (doc, ids)
    }

    fn ids_of(doc: &Document, f: impl Fn(&ElementKind) -> bool) -> Vec<ElementId> {
        doc.model()
            .iter()
            .filter(|e| f(&e.kind))
            .map(|e| e.id)
            .collect()
    }

    fn window_width(doc: &Document, id: ElementId) -> f64 {
        match &doc.model().get(id).unwrap().kind {
            ElementKind::Opening(o) => o.width,
            _ => unreachable!(),
        }
    }

    #[test]
    fn starters_add_once_with_their_assemblies() {
        let (doc, ids) = house();
        assert_eq!(ids.len(), STARTERS.len());
        let (tx, again) = add_starters(doc.model());
        assert!(tx.ops.is_empty());
        assert_eq!(again, ids);
        // the cavity wall type is built of the cavity wall assembly, as thick
        let ElementKind::ElementType(t) = &doc.model().get(ids[2]).unwrap().kind else {
            unreachable!()
        };
        let a = t.assembly().unwrap();
        assert!(matches!(
            doc.model().get(a).unwrap().kind,
            ElementKind::Assembly(_)
        ));
        let TypeParams::Wall(w) = &t.params else {
            unreachable!()
        };
        assert!((w.thickness - 0.365).abs() < 1e-9);
        assert!(STARTERS.iter().all(|s| !s.0.is_empty()));
        for c in TypeCategory::ALL {
            assert!(first_of(doc.model(), c, false).is_some(), "{c:?}");
        }
        assert!(first_of(doc.model(), TypeCategory::Wall, true).is_some());
    }

    #[test]
    fn swapping_many_then_editing_the_type_updates_all_but_overrides() {
        let (mut doc, ids) = house();
        let windows = ids_of(
            &doc,
            |k| matches!(k, ElementKind::Opening(o) if o.kind == OpeningKind::Window),
        );
        assert!(windows.len() >= 2);
        let wide = ids[9];
        // doors don't take a window type
        let mut all = windows.clone();
        all.extend(ids_of(
            &doc,
            |k| matches!(k, ElementKind::Opening(o) if o.kind == OpeningKind::Door),
        ));
        let (tx, n) = assign(doc.model(), &all, Some(wide));
        assert_eq!(n, windows.len());
        doc.commit(tx).unwrap();
        assert_eq!(instances(doc.model(), wide), windows);
        assert!(windows
            .iter()
            .all(|w| (window_width(&doc, *w) - 1.2).abs() < 1e-9));
        // one window is made narrower by hand: it overrides its width
        doc.edit(windows[0], "Width", |e| {
            if let ElementKind::Opening(o) = &mut e.kind {
                o.width = 0.8;
            }
        })
        .unwrap();
        let r = doc
            .model()
            .get(windows[0])
            .unwrap()
            .of_type
            .clone()
            .unwrap();
        assert_eq!(r.overrides, vec![Param::Width]);
        // the type gets wider and taller: the others follow, the first keeps its width
        let ElementKind::ElementType(mut t) = doc.model().get(wide).unwrap().kind.clone() else {
            unreachable!()
        };
        if let TypeParams::Window(o) = &mut t.params {
            o.width = 1.5;
            o.height = 1.4;
        }
        let tx = edit(doc.model(), wide, t, "Type width").unwrap();
        assert_eq!(tx.ops.len(), 1 + windows.len());
        doc.commit(tx).unwrap();
        assert!((window_width(&doc, windows[0]) - 0.8).abs() < 1e-9);
        assert!((window_width(&doc, windows[1]) - 1.5).abs() < 1e-9);
        let ElementKind::Opening(o) = &doc.model().get(windows[0]).unwrap().kind else {
            unreachable!()
        };
        assert!((o.height - 1.4).abs() < 1e-9);
        // reset puts it back; undo the whole type edit in one step
        doc.commit(reset(doc.model(), windows[0]).unwrap()).unwrap();
        assert!((window_width(&doc, windows[0]) - 1.5).abs() < 1e-9);
        doc.undo();
        doc.undo();
        assert!((window_width(&doc, windows[1]) - 1.2).abs() < 1e-9);
        // deleting the type keeps the values
        let model = doc.model().clone();
        doc.commit(delete(&model, wide)).unwrap();
        assert!(instances(doc.model(), wide).is_empty());
        assert!(doc.model().get(windows[1]).unwrap().of_type.is_none());
        assert!((window_width(&doc, windows[1]) - 1.2).abs() < 1e-9);
    }

    #[test]
    fn a_wall_type_brings_its_assembly_and_a_curtain_type_its_grid() {
        let (mut doc, ids) = house();
        let walls = ids_of(&doc, |k| matches!(k, ElementKind::Wall(_)));
        let (tx, n) = assign(doc.model(), &walls, Some(ids[2]));
        assert_eq!(n, walls.len());
        doc.commit(tx).unwrap();
        let ElementKind::Wall(w) = &doc.model().get(walls[0]).unwrap().kind else {
            unreachable!()
        };
        assert!(w.assembly.is_some() && (w.thickness - 0.365).abs() < 1e-9);
        let height = w.height;
        let (tx, _) = assign(doc.model(), &walls[..1], Some(ids[4]));
        doc.commit(tx).unwrap();
        let ElementKind::Wall(w) = &doc.model().get(walls[0]).unwrap().kind else {
            unreachable!()
        };
        assert!(w.curtain.is_some() && w.assembly.is_none());
        assert!((w.thickness - 0.15).abs() < 1e-9);
        assert_eq!(w.height, height, "a swapped wall keeps its height");
        // the instance is not marked as overriding anything
        assert!(doc
            .model()
            .get(walls[0])
            .unwrap()
            .of_type
            .as_ref()
            .unwrap()
            .overrides
            .is_empty());
        // a type made from an element
        let (t, after) = from_element(doc.model(), walls[1], "My wall").unwrap();
        assert_eq!(after.of_type.as_ref().unwrap().id, t.id);
    }

    #[test]
    fn elements_from_older_files_have_no_type_and_types_round_trip() {
        let e = Element::new("Level", ElementKind::Units(crate::Units::metric()));
        let text = serde_json::to_string(&e).unwrap();
        assert!(!text.contains("\"type\""), "{text}");
        let back: Element = serde_json::from_str(&text).unwrap();
        assert!(back.of_type.is_none());
        let (mut doc, ids) = house();
        let walls = ids_of(&doc, |k| matches!(k, ElementKind::Wall(_)));
        let (tx, _) = assign(doc.model(), &walls, Some(ids[0]));
        doc.commit(tx).unwrap();
        doc.edit(walls[0], "Thicker", |e| {
            if let ElementKind::Wall(w) = &mut e.kind {
                w.thickness = 0.3;
            }
        })
        .unwrap();
        let back = Model::from_json(&doc.model().to_json()).unwrap();
        assert_eq!(&back, doc.model());
        assert_eq!(
            back.get(walls[0])
                .unwrap()
                .of_type
                .as_ref()
                .unwrap()
                .overrides,
            vec![Param::Thickness]
        );
    }

    #[test]
    fn the_typed_sample_is_typed_throughout() {
        let m = crate::sample::typed_house();
        let named = |n: &str| m.iter().find(|e| e.name == n).unwrap();
        let typed = m.iter().filter(|e| e.of_type.is_some()).count();
        assert!(typed > 20, "{typed}");
        for e in m.iter() {
            if let Some(r) = &e.of_type {
                assert!(r.overrides.is_empty(), "{}: {:?}", e.name, r.overrides);
            }
        }
        let back = named("GF Wall 3");
        assert!(matches!(&back.kind, ElementKind::Wall(w) if w.curtain.is_some()));
        // the shaft cuts the first floor slab but not the ground floor's
        let ff = crate::slab::holes_of(&m, named("FF Slab"));
        let gf = crate::slab::holes_of(&m, named("GF Slab"));
        assert_eq!((ff.len(), gf.len()), (2, 0));
    }

    #[test]
    fn schedule_views_read_without_grouping() {
        let old = r#"{"Schedule":{"category":"Walls"}}"#;
        let k: crate::ViewKind = serde_json::from_str(old).unwrap();
        assert_eq!(
            k,
            crate::ViewKind::Schedule {
                category: crate::ScheduleCategory::Walls,
                by_type: false
            }
        );
        assert_eq!(serde_json::to_string(&k).unwrap(), old);
    }
}
