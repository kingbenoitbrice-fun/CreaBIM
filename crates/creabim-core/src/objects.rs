//! Objects: furniture and fixtures (a washbasin, a kitchen unit, a sofa, a
//! bed) placed in the model from a catalogue.
//!
//! An object type ([`ObjectType`], one of the [`crate::types`] element
//! types) holds the shape, as parts each with its own material, the plan
//! symbol, what it is (its catalogue category and IFC class) and how it is
//! placed. Each placed [`Object`] is an instance of it with its own level,
//! position, height and turn, so editing the type changes every copy.
//!
//! An object's own axes: `x` across its width, `y` from its back (at
//! `y = 0`) towards its front (at `y < 0`), `z` up from the floor. The
//! origin is the middle of the back edge on the floor, so an object goes
//! against a wall by putting its origin on the wall's face.
//!
//! The catalogue lives outside projects as [`CatalogueObject`]s: an object
//! type with its materials by name, so it can be saved to a file and added
//! to any project ([`add_to_model`]), and made from any object type of a
//! project again ([`from_type`]). The starter catalogue is in
//! [`crate::catalogue`].

use crate::types::{ElementType, TypeParams};
use crate::{EditMesh, Element, ElementId, ElementKind, Material, Model, Transaction};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// Where an object belongs, for browsing the catalogue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObjectCategory {
    Bathroom,
    Kitchen,
    Living,
    Dining,
    Bedroom,
    Office,
    #[default]
    Other,
}

impl ObjectCategory {
    pub const ALL: [ObjectCategory; 7] = [
        ObjectCategory::Bathroom,
        ObjectCategory::Kitchen,
        ObjectCategory::Living,
        ObjectCategory::Dining,
        ObjectCategory::Bedroom,
        ObjectCategory::Office,
        ObjectCategory::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ObjectCategory::Bathroom => "Bathroom",
            ObjectCategory::Kitchen => "Kitchen",
            ObjectCategory::Living => "Living",
            ObjectCategory::Dining => "Dining",
            ObjectCategory::Bedroom => "Bedroom",
            ObjectCategory::Office => "Office",
            ObjectCategory::Other => "Other",
        }
    }

    pub fn from_label(s: &str) -> ObjectCategory {
        ObjectCategory::ALL
            .into_iter()
            .find(|c| c.label().eq_ignore_ascii_case(s.trim()))
            .unwrap_or_default()
    }
}

/// The IFC classes an object can be, each with its predefined types:
/// (occurrence class, type class, predefined types). `USERDEFINED` takes
/// its name from the object type.
pub const IFC_CLASSES: &[(&str, &str, &[&str])] = &[
    (
        "IfcFurniture",
        "IfcFurnitureType",
        &[
            "CHAIR",
            "TABLE",
            "DESK",
            "BED",
            "FILECABINET",
            "SHELF",
            "SOFA",
            "TECHNICALCABINET",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
    (
        "IfcSanitaryTerminal",
        "IfcSanitaryTerminalType",
        &[
            "BATH",
            "BIDET",
            "CISTERN",
            "SHOWER",
            "SINK",
            "SANITARYFOUNTAIN",
            "TOILETPAN",
            "URINAL",
            "WASHHANDBASIN",
            "WCSEAT",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
    (
        "IfcElectricAppliance",
        "IfcElectricApplianceType",
        &[
            "DISHWASHER",
            "ELECTRICCOOKER",
            "FREESTANDINGELECTRICHEATER",
            "FREESTANDINGFAN",
            "FREESTANDINGWATERHEATER",
            "FREESTANDINGWATERCOOLER",
            "FREEZER",
            "FRIDGE_FREEZER",
            "HANDDRYER",
            "KITCHENMACHINE",
            "MICROWAVE",
            "PHOTOCOPIER",
            "REFRIGERATOR",
            "TUMBLEDRYER",
            "VENDINGMACHINE",
            "WASHINGMACHINE",
            "USERDEFINED",
            "NOTDEFINED",
        ],
    ),
    (
        "IfcSystemFurnitureElement",
        "IfcSystemFurnitureElementType",
        &["PANEL", "WORKSURFACE", "SUBRACK", "USERDEFINED", "NOTDEFINED"],
    ),
];

/// The predefined types of an occurrence class, or none for classes not in
/// [`IFC_CLASSES`].
pub fn predefined_types(class: &str) -> &'static [&'static str] {
    IFC_CLASSES
        .iter()
        .find(|c| c.0.eq_ignore_ascii_case(class))
        .map_or(&[], |c| c.2)
}

/// The occurrence class of an IFC type class (`IfcFurnitureType` →
/// `IfcFurniture`), as spelled in [`IFC_CLASSES`].
pub fn class_of_type(type_class: &str) -> Option<&'static str> {
    IFC_CLASSES
        .iter()
        .find(|c| c.1.eq_ignore_ascii_case(type_class))
        .map(|c| c.0)
}

/// What an object is in IFC: its class and predefined type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IfcKind {
    pub class: String,
    pub predefined: String,
}

impl IfcKind {
    pub fn new(class: &str, predefined: &str) -> Self {
        Self {
            class: class.into(),
            predefined: predefined.into(),
        }
    }

    /// The type class it exports as (`IfcFurnitureType`...).
    pub fn type_class(&self) -> &'static str {
        IFC_CLASSES
            .iter()
            .find(|c| c.0.eq_ignore_ascii_case(&self.class))
            .map_or("IfcFurnitureType", |c| c.1)
    }

    /// The class as one of [`IFC_CLASSES`] (furniture when it isn't), and
    /// the predefined type as one of its own (`USERDEFINED` when it isn't).
    pub fn normalized(&self) -> IfcKind {
        let Some(c) = IFC_CLASSES
            .iter()
            .find(|c| c.0.eq_ignore_ascii_case(&self.class))
        else {
            return IfcKind::new("IfcFurniture", "USERDEFINED");
        };
        let p = self.predefined.trim().to_ascii_uppercase();
        let p = if c.2.contains(&p.as_str()) {
            p
        } else {
            "USERDEFINED".into()
        };
        IfcKind::new(c.0, &p)
    }
}

impl Default for IfcKind {
    fn default() -> Self {
        IfcKind::new("IfcFurniture", "NOTDEFINED")
    }
}

/// How an object is usually placed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mount {
    /// With its back against a wall: placing it near one turns it to face
    /// the room and slides it along the wall.
    #[default]
    Wall,
    /// Free-standing, wherever it is clicked.
    Floor,
}

/// One line of a plan symbol, in the object's own axes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SymbolItem {
    /// A closed outline.
    Loop(Vec<DVec2>),
    Line(DVec2, DVec2),
}

/// One piece of an object, of one material.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectPart {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub mesh: EditMesh,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<ElementId>,
}

/// What objects of a type share: their shape, symbol and meaning.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectType {
    #[serde(default)]
    pub category: ObjectCategory,
    #[serde(default)]
    pub ifc: IfcKind,
    #[serde(default)]
    pub mount: Mount,
    pub parts: Vec<ObjectPart>,
    /// The plan symbol; empty draws each part's outline seen from above.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbol: Vec<SymbolItem>,
    /// The catalogue object it came from, so placing that object again
    /// reuses this type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalogue: Option<String>,
}

impl ObjectType {
    pub fn validate(&self) -> Result<(), String> {
        if self.parts.is_empty() {
            return Err("an object type needs at least one part".into());
        }
        for p in &self.parts {
            p.mesh.validate()?;
        }
        Ok(())
    }

    /// The box round every part, in the object's own axes.
    pub fn bounds(&self) -> (DVec3, DVec3) {
        let mut it = self.parts.iter().flat_map(|p| p.mesh.vertices.iter());
        let Some(first) = it.next() else {
            return (DVec3::ZERO, DVec3::ZERO);
        };
        it.fold((*first, *first), |(lo, hi), v| (lo.min(*v), hi.max(*v)))
    }

    /// Width, depth and height.
    pub fn size(&self) -> DVec3 {
        let (lo, hi) = self.bounds();
        hi - lo
    }

    /// The plan symbol: the type's own, else the outline of each part.
    pub fn plan_symbol(&self) -> Vec<SymbolItem> {
        if !self.symbol.is_empty() {
            return self.symbol.clone();
        }
        self.parts
            .iter()
            .filter_map(|p| {
                let pts: Vec<DVec2> = p.mesh.vertices.iter().map(|v| v.truncate()).collect();
                let h = hull(&pts);
                (h.len() >= 3).then_some(SymbolItem::Loop(h))
            })
            .collect()
    }

    /// The materials of its parts, each once.
    pub fn materials(&self) -> Vec<ElementId> {
        let mut out = vec![];
        for m in self.parts.iter().filter_map(|p| p.material) {
            if !out.contains(&m) {
                out.push(m);
            }
        }
        out
    }

    /// "1800 × 900 × 750, furniture (TABLE)".
    pub fn summary(&self, model: &Model) -> String {
        let u = model.units();
        let s = self.size();
        format!(
            "{} × {} × {}",
            u.dimension(s.x),
            u.dimension(s.y),
            u.dimension(s.z)
        )
    }
}

/// A placed object: furniture or a fixture, an instance of an object type.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub level: ElementId,
    /// Where its origin (the middle of its back edge) is in plan.
    pub position: DVec2,
    /// How far its floor is above its level.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: f64,
    /// Turn about its origin, degrees anticlockwise. At 0 its back is
    /// towards +y.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl Object {
    pub fn new(level: ElementId, position: DVec2, rotation: f64) -> Self {
        Self {
            level,
            position,
            offset: 0.0,
            rotation,
        }
    }

    /// From the object's own axes to plan.
    pub fn to_plan(&self, p: DVec2) -> DVec2 {
        let (s, c) = self.rotation.to_radians().sin_cos();
        self.position + DVec2::new(p.x * c - p.y * s, p.x * s + p.y * c)
    }

    /// From the object's own axes to the model's, its level at `level_z`.
    pub fn to_model(&self, p: DVec3, level_z: f64) -> DVec3 {
        self.to_plan(p.truncate())
            .extend(p.z + level_z + self.offset)
    }

    /// The way it faces in plan (its front).
    pub fn facing(&self) -> DVec2 {
        self.to_plan(-DVec2::Y) - self.position
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.position.is_finite() || !self.offset.is_finite() || !self.rotation.is_finite() {
            return Err("an object's position must be finite".into());
        }
        Ok(())
    }
}

/// The object type of a placed object.
pub fn type_of<'a>(model: &'a Model, e: &Element) -> Option<&'a ObjectType> {
    match crate::types::type_of(model, e)? {
        (
            _,
            ElementType {
                params: TypeParams::Object(t),
                ..
            },
        ) => Some(t),
        _ => None,
    }
}

/// The parts of a placed object in model coordinates, with their materials.
pub fn model_parts(model: &Model, e: &Element) -> Vec<(EditMesh, Option<ElementId>)> {
    let ElementKind::Object(o) = &e.kind else {
        return vec![];
    };
    let Some(t) = type_of(model, e) else {
        return vec![];
    };
    let z = model.level(o.level).map_or(0.0, |l| l.elevation);
    t.parts
        .iter()
        .map(|p| {
            let mut m = p.mesh.clone();
            for v in &mut m.vertices {
                *v = o.to_model(*v, z);
            }
            (m, p.material)
        })
        .collect()
}

/// Convex hull, anticlockwise (Andrew's monotone chain).
pub fn hull(points: &[DVec2]) -> Vec<DVec2> {
    let mut p: Vec<DVec2> = points.iter().copied().filter(|v| v.is_finite()).collect();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if p.len() < 3 {
        return p;
    }
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let mut lower: Vec<DVec2> = vec![];
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0
        {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<DVec2> = vec![];
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0
        {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Move parts so the middle of their back edge, on their floor, is the
/// origin: the object's own axes, for shapes modelled anywhere.
pub fn recentre(parts: &mut [ObjectPart]) {
    let mut it = parts.iter().flat_map(|p| p.mesh.vertices.iter());
    let Some(first) = it.next().copied() else {
        return;
    };
    let (lo, hi) = it.fold((first, first), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    let shift = DVec3::new(-(lo.x + hi.x) / 2.0, -hi.y, -lo.z);
    for p in parts {
        for v in &mut p.mesh.vertices {
            *v += shift;
        }
    }
}

/// An object of the catalogue, outside any project: an object type whose
/// parts' materials are given by name and look rather than by id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatalogueObject {
    /// Unique in the catalogue: `starter/sofa-3` for the starters, the file
    /// name for the user's own.
    pub key: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The type, its parts' `material` left empty.
    pub object: ObjectType,
    /// Each part's material, by name, in part order.
    #[serde(default)]
    pub materials: Vec<Option<(String, Material)>>,
}

impl CatalogueObject {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("catalogue objects serialize")
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let c: CatalogueObject = serde_json::from_str(text).map_err(|e| e.to_string())?;
        c.object.validate()?;
        Ok(c)
    }
}

/// The object type of the model made from catalogue object `key`.
pub fn type_for_key(model: &Model, key: &str) -> Option<ElementId> {
    model.iter().find_map(|e| match &e.kind {
        ElementKind::ElementType(ElementType {
            params: TypeParams::Object(t),
            ..
        }) if t.catalogue.as_deref() == Some(key) => Some(e.id),
        _ => None,
    })
}

/// Bring a catalogue object into a model as an object type, with the
/// materials it is made of: those the model has by name are used as they
/// are, others are added. A type already made from it is reused. Returns
/// the transaction (empty when nothing needs adding) and the type's id.
pub fn add_to_model(model: &Model, c: &CatalogueObject) -> (Transaction, ElementId) {
    let mut tx = Transaction::new(format!("Add {}", c.name));
    if let Some(id) = type_for_key(model, &c.key) {
        return (tx, id);
    }
    let mut by_name: Vec<(String, ElementId)> = crate::library::materials(model)
        .into_iter()
        .map(|(id, n, _)| (n.to_string(), id))
        .collect();
    let mut t = c.object.clone();
    t.catalogue = Some(c.key.clone());
    for (i, p) in t.parts.iter_mut().enumerate() {
        p.material = None;
        let Some(Some((name, m))) = c.materials.get(i) else {
            continue;
        };
        let id = match by_name.iter().find(|(n, _)| n == name) {
            Some((_, id)) => *id,
            None => {
                let e = Element::new(name.clone(), ElementKind::Material(m.clone().clamped()));
                by_name.push((name.clone(), e.id));
                let id = e.id;
                tx = tx.insert(e);
                id
            }
        };
        p.material = Some(id);
    }
    let mut ty = ElementType::new(TypeParams::Object(t));
    ty.description = c.description.clone();
    let e = Element::new(
        crate::types::unique_name(model, &c.name),
        ElementKind::ElementType(ty),
    );
    let id = e.id;
    (tx.insert(e), id)
}

/// A catalogue object made from an object type of a model, under `key`.
pub fn from_type(model: &Model, type_id: ElementId, key: &str) -> Option<CatalogueObject> {
    let e = model.get(type_id)?;
    let ElementKind::ElementType(ElementType {
        params: TypeParams::Object(t),
        description,
    }) = &e.kind
    else {
        return None;
    };
    let materials = t
        .parts
        .iter()
        .map(|p| {
            let m = model.get(p.material?)?;
            match &m.kind {
                ElementKind::Material(mat) => Some((m.name.clone(), mat.clone())),
                _ => None,
            }
        })
        .collect();
    let mut object = t.clone();
    object.catalogue = Some(key.to_string());
    for p in &mut object.parts {
        p.material = None;
    }
    Some(CatalogueObject {
        key: key.to_string(),
        name: e.name.clone(),
        description: description.clone(),
        object,
        materials,
    })
}

/// A catalogue object made from free meshes of a model (shapes modelled
/// here or linked from Blender or FreeCAD), one part each, as they stand
/// in the model; the first one's IFC class says what it is.
pub fn from_meshes(model: &Model, ids: &[ElementId], name: &str, key: &str) -> Option<CatalogueObject> {
    let mut parts = vec![];
    let mut materials = vec![];
    let mut class = None;
    for id in ids {
        let Some(e) = model.get(*id) else { continue };
        let ElementKind::FreeMesh(f) = &e.kind else {
            continue;
        };
        let z = f
            .level
            .and_then(|l| model.level(l))
            .map_or(0.0, |l| l.elevation);
        let mut mesh = f.evaluated().mesh;
        for v in &mut mesh.vertices {
            *v += f.origin + DVec3::Z * z;
        }
        class.get_or_insert(f.ifc_class.clone());
        parts.push(ObjectPart {
            name: e.name.clone(),
            mesh,
            material: None,
        });
        materials.push(f.material.and_then(|m| {
            let me = model.get(m)?;
            match &me.kind {
                ElementKind::Material(mat) => Some((me.name.clone(), mat.clone())),
                _ => None,
            }
        }));
    }
    if parts.is_empty() {
        return None;
    }
    recentre(&mut parts);
    let ifc = class
        .map(|c| IfcKind::new(&c, "NOTDEFINED").normalized())
        .unwrap_or_default();
    Some(CatalogueObject {
        key: key.to_string(),
        name: name.to_string(),
        description: String::new(),
        object: ObjectType {
            category: ObjectCategory::Other,
            ifc,
            mount: Mount::Floor,
            parts,
            symbol: vec![],
            catalogue: Some(key.to_string()),
        },
        materials,
    })
}

/// Point every part made of `from` at `to` instead, across the model's
/// object types: (before, after) for each type that changes.
pub fn replace_material(
    model: &Model,
    from: ElementId,
    to: Option<ElementId>,
) -> Vec<(Element, Element)> {
    let mut out = vec![];
    for e in model.iter() {
        let ElementKind::ElementType(ElementType {
            params: TypeParams::Object(t),
            ..
        }) = &e.kind
        else {
            continue;
        };
        if !t.parts.iter().any(|p| p.material == Some(from)) {
            continue;
        }
        let mut after = e.clone();
        if let ElementKind::ElementType(ElementType {
            params: TypeParams::Object(t),
            ..
        }) = &mut after.kind
        {
            for p in &mut t.parts {
                if p.material == Some(from) {
                    p.material = to;
                }
            }
        }
        out.push((e.clone(), after));
    }
    out
}

/// Where an object goes against a wall near `p` on `level`: the point on
/// the nearer face of the nearest wall within `reach`, and the turn that
/// puts its back to that face. None when no wall is that near.
pub fn against_wall(model: &Model, level: ElementId, p: DVec2, reach: f64) -> Option<(DVec2, f64)> {
    let mut best: Option<(f64, DVec2, f64)> = None;
    for e in model.iter() {
        let ElementKind::Wall(w) = &e.kind else {
            continue;
        };
        if w.level != level {
            continue;
        }
        let d = w.direction();
        let len = w.length();
        if len < 1e-6 {
            continue;
        }
        let u = (p - w.start).dot(d);
        if u < -0.05 || u > len + 0.05 {
            continue;
        }
        let n = d.perp();
        let s = (p - w.start).dot(n);
        let side = if s >= 0.0 { 1.0 } else { -1.0 };
        let gap = s.abs() - w.thickness / 2.0;
        if !(-1e-6..=reach).contains(&gap) {
            continue;
        }
        if best.is_some_and(|b| b.0 <= gap) {
            continue;
        }
        let face = w.start + d * u.clamp(0.0, len) + n * side * w.thickness / 2.0;
        // the back (+y at no turn) towards the wall: the front along the face's normal
        let front = n * side;
        let rotation = (front.to_angle().to_degrees() + 90.0).rem_euclid(360.0);
        best = Some((gap, face, rotation));
    }
    best.map(|b| (b.1, b.2))
}

/// The edits placing `c` at `position` on `level`, turned by `rotation`:
/// its type (and materials) when the model doesn't have them yet, and the
/// object. Returns the transaction and the new object's id.
pub fn place(
    model: &Model,
    c: &CatalogueObject,
    level: ElementId,
    position: DVec2,
    rotation: f64,
) -> (Transaction, ElementId) {
    let (tx, type_id) = add_to_model(model, c);
    let mut tx = tx;
    tx.label = format!("Place {}", c.name);
    let e = instance(type_id, &c.name, level, position, rotation);
    let id = e.id;
    (tx.insert(e), id)
}

/// A new object of type `type_id`.
pub fn instance(
    type_id: ElementId,
    name: &str,
    level: ElementId,
    position: DVec2,
    rotation: f64,
) -> Element {
    let mut e = Element::new(
        name,
        ElementKind::Object(Object::new(level, position, rotation)),
    );
    e.of_type = Some(crate::TypeRef::new(type_id));
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    fn level(m: &Model) -> ElementId {
        m.levels().next().unwrap().0.id
    }

    #[test]
    fn placing_a_catalogue_object_adds_its_type_once_and_its_materials_by_name() {
        let mut doc = Document::from_model(crate::sample::sample_house());
        let sofa = crate::catalogue::starter("starter/sofa-3").unwrap();
        let l = level(doc.model());
        let before = crate::library::materials(doc.model()).len();
        let (tx, a) = place(doc.model(), &sofa, l, DVec2::new(1.0, 1.0), 0.0);
        doc.commit(tx).unwrap();
        let added = crate::library::materials(doc.model()).len() - before;
        assert!(added >= 2, "{added}");
        let (tx, b) = place(doc.model(), &sofa, l, DVec2::new(3.0, 1.0), 90.0);
        assert_eq!(tx.ops.len(), 1, "the second sofa reuses the type");
        doc.commit(tx).unwrap();
        let m = doc.model();
        let ta = m.get(a).unwrap().of_type.as_ref().unwrap().id;
        let tb = m.get(b).unwrap().of_type.as_ref().unwrap().id;
        assert_eq!(ta, tb);
        assert_eq!(crate::types::instances(m, ta).len(), 2);
        // the parts stand in the model where the sofa is, turned
        let parts = model_parts(m, m.get(b).unwrap());
        assert!(parts.iter().all(|(_, mat)| mat.is_some()));
        let size = type_of(m, m.get(b).unwrap()).unwrap().size();
        for (mesh, _) in &parts {
            for v in &mesh.vertices {
                assert!(v.x <= 3.0 + size.y + 1e-6 && v.x >= 3.0 - 1e-6, "{v}");
            }
        }
        // and back out to the catalogue, materials and all
        let back = from_type(m, ta, "mine/sofa").unwrap();
        assert_eq!(back.object.parts.len(), sofa.object.parts.len());
        assert_eq!(back.materials, sofa.materials);
        let text = back.to_json();
        // (floats read back to within a hair: compare what matters)
        let read = CatalogueObject::from_json(&text).unwrap();
        assert_eq!((&read.key, &read.name, &read.materials), (&back.key, &back.name, &back.materials));
        assert!((read.object.size() - back.object.size()).length() < 1e-9);
        // deleting the type deletes its objects with it, in one step
        let model = doc.model().clone();
        doc.commit(crate::types::delete(&model, ta)).unwrap();
        assert!(doc.model().get(a).is_none() && doc.model().get(b).is_none());
        doc.undo();
        assert!(doc.model().get(a).is_some());
    }

    #[test]
    fn an_object_near_a_wall_turns_its_back_to_it() {
        let m = crate::sample::sample_house();
        let l = level(&m);
        // the sample's south wall runs along y = 0
        let (at, rot) = against_wall(&m, l, DVec2::new(3.0, 0.4), 0.6).unwrap();
        let o = Object::new(l, at, rot);
        // facing into the house, away from the wall
        assert!(o.facing().y > 0.99, "{:?} {rot}", o.facing());
        assert!(against_wall(&m, l, DVec2::new(3.0, 3.0), 0.6).is_none());
    }

    #[test]
    fn hull_and_recentre() {
        let h = hull(&[
            DVec2::ZERO,
            DVec2::X,
            DVec2::ONE,
            DVec2::Y,
            DVec2::splat(0.5),
        ]);
        assert_eq!(h.len(), 4);
        let mut parts = vec![ObjectPart {
            name: String::new(),
            mesh: EditMesh::cuboid(DVec3::new(2.0, 1.0, 0.5)),
            material: None,
        }];
        for v in &mut parts[0].mesh.vertices {
            *v += DVec3::new(5.0, 5.0, 1.0);
        }
        recentre(&mut parts);
        let t = ObjectType {
            category: ObjectCategory::Other,
            ifc: IfcKind::default(),
            mount: Mount::Floor,
            parts,
            symbol: vec![],
            catalogue: None,
        };
        let (lo, hi) = t.bounds();
        assert!((lo - DVec3::new(-1.0, -1.0, 0.0)).length() < 1e-9, "{lo}");
        assert!((hi - DVec3::new(1.0, 0.0, 0.5)).length() < 1e-9, "{hi}");
        assert_eq!(
            IfcKind::new("IfcSanitaryTerminal", "wc").normalized(),
            IfcKind::new("IfcSanitaryTerminal", "USERDEFINED")
        );
        assert_eq!(class_of_type("IFCFURNITURETYPE"), Some("IfcFurniture"));
    }
}
