//! The material library: common building materials ready to add to a
//! project, and the edits that manage a project's materials (assigning,
//! deleting with their users reassigned, merging imported materials).
//!
//! Categories follow IFC's `IfcMaterial.Category`, using the values IFC4
//! recommends (`concrete`, `steel`, `aluminium`, `block`, `brick`, `stone`,
//! `wood`, `glass`, `gypsum`, `plastic`, `earth`) and a few more for what
//! those leave out (`insulation`, `plaster`, `finish`, `roofing`,
//! `membrane`, `metal`).

use crate::{Element, ElementId, ElementKind, Hatch, Material, Model, Transaction};

/// The categories offered when editing a material: (IFC value, label).
pub const CATEGORIES: &[(&str, &str)] = &[
    ("concrete", "Concrete"),
    ("brick", "Brick"),
    ("block", "Block"),
    ("stone", "Stone"),
    ("wood", "Wood"),
    ("steel", "Steel"),
    ("aluminium", "Aluminium"),
    ("metal", "Other metal"),
    ("glass", "Glass"),
    ("insulation", "Insulation"),
    ("gypsum", "Gypsum"),
    ("plaster", "Plaster and render"),
    ("finish", "Finishes"),
    ("roofing", "Roofing"),
    ("membrane", "Membranes"),
    ("plastic", "Plastic"),
    ("ceramic", "Ceramic"),
    ("textile", "Textiles"),
    ("earth", "Earth"),
];

/// A category's label, or the category itself when it isn't one of
/// [`CATEGORIES`] ("Uncategorised" when empty).
pub fn category_label(category: &str) -> String {
    let c = category.trim();
    if c.is_empty() {
        return "Uncategorised".into();
    }
    match CATEGORIES.iter().find(|(v, _)| v.eq_ignore_ascii_case(c)) {
        Some((_, label)) => label.to_string(),
        None => {
            let mut s = c.to_string();
            if let Some(f) = s.get_mut(0..1) {
                f.make_ascii_uppercase();
            }
            s
        }
    }
}

/// One material of the built-in library.
#[derive(Clone, Copy, Debug)]
pub struct LibraryEntry {
    pub name: &'static str,
    pub category: &'static str,
    pub hatch: Hatch,
    /// sRGB, 0..1.
    pub color: [f32; 3],
    pub roughness: f32,
    pub metallic: f32,
    pub opacity: f32,
    pub description: &'static str,
}

impl LibraryEntry {
    pub fn material(&self) -> Material {
        Material {
            hatch: self.hatch,
            color: self.color,
            category: self.category.into(),
            description: self.description.into(),
            roughness: self.roughness,
            metallic: self.metallic,
            opacity: self.opacity,
        }
    }

    pub fn element(&self) -> Element {
        Element::new(self.name, ElementKind::Material(self.material()))
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

const fn entry(
    name: &'static str,
    category: &'static str,
    hatch: Hatch,
    color: [f32; 3],
    roughness: f32,
    description: &'static str,
) -> LibraryEntry {
    LibraryEntry {
        name,
        category,
        hatch,
        color,
        roughness,
        metallic: 0.0,
        opacity: 1.0,
        description,
    }
}

const fn metal(
    name: &'static str,
    category: &'static str,
    color: [f32; 3],
    roughness: f32,
    description: &'static str,
) -> LibraryEntry {
    LibraryEntry {
        name,
        category,
        hatch: Hatch::Solid,
        color,
        roughness,
        metallic: 1.0,
        opacity: 1.0,
        description,
    }
}

const fn glass(
    name: &'static str,
    color: [f32; 3],
    opacity: f32,
    description: &'static str,
) -> LibraryEntry {
    LibraryEntry {
        name,
        category: "glass",
        hatch: Hatch::Empty,
        color,
        roughness: 0.05,
        metallic: 0.0,
        opacity,
        description,
    }
}

use Hatch::Timber as TIM;
use Hatch::{Brick as BRK, Concrete as CON, Cross as CRS, Diagonal as DIA, Empty, Solid as SOL};

/// The built-in library, grouped by category.
pub const LIBRARY: &[LibraryEntry] = &[
    // concretes
    entry(
        "Concrete, cast in place",
        "concrete",
        CON,
        rgb(178, 178, 172),
        0.85,
        "Reinforced concrete C30/37, cast on site",
    ),
    entry(
        "Concrete, precast",
        "concrete",
        CON,
        rgb(196, 194, 188),
        0.75,
        "Precast reinforced concrete elements",
    ),
    entry(
        "Concrete, lightweight",
        "concrete",
        CON,
        rgb(201, 198, 186),
        0.9,
        "Lightweight aggregate concrete",
    ),
    entry(
        "Concrete, polished",
        "concrete",
        CON,
        rgb(160, 160, 158),
        0.3,
        "Polished concrete floor",
    ),
    entry(
        "Screed, cement",
        "concrete",
        CON,
        rgb(186, 182, 172),
        0.85,
        "Cement and sand floor screed",
    ),
    // masonry
    entry(
        "Brick, common red",
        "brick",
        BRK,
        rgb(170, 84, 62),
        0.9,
        "Fired clay facing brick",
    ),
    entry(
        "Brick, yellow stock",
        "brick",
        BRK,
        rgb(214, 182, 120),
        0.9,
        "Yellow London stock brick",
    ),
    entry(
        "Brick, engineering blue",
        "brick",
        BRK,
        rgb(72, 74, 88),
        0.7,
        "Class B engineering brick",
    ),
    entry(
        "Block, concrete",
        "block",
        BRK,
        rgb(168, 166, 160),
        0.95,
        "Dense concrete blockwork",
    ),
    entry(
        "Block, aerated concrete",
        "block",
        BRK,
        rgb(226, 224, 216),
        0.95,
        "Autoclaved aerated concrete blockwork",
    ),
    entry(
        "Block, clay hollow",
        "block",
        BRK,
        rgb(196, 112, 78),
        0.9,
        "Hollow fired clay block",
    ),
    entry(
        "Stone, limestone",
        "stone",
        DIA,
        rgb(222, 212, 184),
        0.75,
        "Dressed limestone",
    ),
    entry(
        "Stone, granite",
        "stone",
        DIA,
        rgb(130, 128, 128),
        0.5,
        "Granite slabs",
    ),
    entry(
        "Stone, slate",
        "stone",
        DIA,
        rgb(74, 80, 88),
        0.6,
        "Natural slate",
    ),
    // timbers
    entry(
        "Timber, softwood",
        "wood",
        TIM,
        rgb(214, 174, 120),
        0.75,
        "Structural softwood C24",
    ),
    entry(
        "Timber, oak",
        "wood",
        TIM,
        rgb(170, 124, 76),
        0.6,
        "European oak, oiled",
    ),
    entry(
        "Timber, glulam",
        "wood",
        TIM,
        rgb(222, 184, 132),
        0.7,
        "Glued laminated timber GL24h",
    ),
    entry(
        "Timber, CLT",
        "wood",
        TIM,
        rgb(226, 196, 150),
        0.75,
        "Cross-laminated timber panels",
    ),
    entry(
        "Plywood",
        "wood",
        TIM,
        rgb(212, 176, 128),
        0.7,
        "Birch plywood",
    ),
    entry(
        "OSB",
        "wood",
        TIM,
        rgb(204, 166, 104),
        0.85,
        "Oriented strand board",
    ),
    // metals
    metal(
        "Steel, structural",
        "steel",
        rgb(112, 116, 122),
        0.5,
        "Structural steel S355, painted",
    ),
    metal(
        "Steel, stainless",
        "steel",
        rgb(196, 198, 200),
        0.25,
        "Brushed stainless steel",
    ),
    metal(
        "Steel, galvanised",
        "steel",
        rgb(170, 176, 178),
        0.45,
        "Hot-dip galvanised steel",
    ),
    metal(
        "Aluminium, anodised",
        "aluminium",
        rgb(186, 188, 192),
        0.35,
        "Anodised aluminium",
    ),
    metal("Copper", "metal", rgb(184, 115, 72), 0.35, "Copper sheet"),
    metal(
        "Zinc",
        "metal",
        rgb(150, 156, 160),
        0.5,
        "Pre-weathered zinc",
    ),
    // glass
    glass("Glass, clear", rgb(210, 230, 235), 0.3, "Clear float glass"),
    glass(
        "Glass, tinted",
        rgb(120, 150, 160),
        0.45,
        "Solar control tinted glass",
    ),
    glass(
        "Glass, frosted",
        rgb(225, 235, 238),
        0.7,
        "Acid-etched glass",
    ),
    // insulation
    entry(
        "Insulation, mineral wool",
        "insulation",
        CRS,
        rgb(236, 214, 120),
        1.0,
        "Glass or stone wool batts",
    ),
    entry(
        "Insulation, EPS",
        "insulation",
        CRS,
        rgb(240, 240, 236),
        0.95,
        "Expanded polystyrene boards",
    ),
    entry(
        "Insulation, PIR",
        "insulation",
        CRS,
        rgb(232, 210, 160),
        0.9,
        "Foil-faced polyisocyanurate boards",
    ),
    entry(
        "Insulation, wood fibre",
        "insulation",
        CRS,
        rgb(176, 140, 96),
        1.0,
        "Wood fibre boards",
    ),
    // plasters and boards
    entry(
        "Plasterboard",
        "gypsum",
        CRS,
        rgb(236, 234, 228),
        0.9,
        "Gypsum plasterboard",
    ),
    entry(
        "Plaster, gypsum",
        "plaster",
        SOL,
        rgb(240, 238, 232),
        0.9,
        "Gypsum plaster skim",
    ),
    entry(
        "Render, lime",
        "plaster",
        SOL,
        rgb(234, 226, 208),
        0.95,
        "Lime render, external",
    ),
    entry(
        "Render, cement",
        "plaster",
        SOL,
        rgb(214, 210, 200),
        0.9,
        "Cement render, external",
    ),
    // finishes
    entry(
        "Paint, white",
        "finish",
        Empty,
        rgb(244, 244, 240),
        0.7,
        "Emulsion paint",
    ),
    entry(
        "Ceramic tile",
        "finish",
        Empty,
        rgb(232, 228, 220),
        0.25,
        "Glazed ceramic tiles",
    ),
    entry(
        "Terrazzo",
        "finish",
        Empty,
        rgb(208, 204, 196),
        0.3,
        "Terrazzo flooring",
    ),
    entry(
        "Sanitary ceramic",
        "ceramic",
        Empty,
        rgb(244, 244, 240),
        0.12,
        "Glazed vitreous china for basins and WCs",
    ),
    entry(
        "Fabric, woven",
        "textile",
        Empty,
        rgb(118, 122, 128),
        0.95,
        "Upholstery fabric",
    ),
    entry(
        "Vinyl flooring",
        "plastic",
        Empty,
        rgb(180, 170, 156),
        0.6,
        "Vinyl sheet flooring",
    ),
    entry(
        "Carpet",
        "finish",
        Empty,
        rgb(110, 112, 120),
        1.0,
        "Carpet tiles",
    ),
    entry(
        "Parquet, oak",
        "wood",
        TIM,
        rgb(184, 136, 86),
        0.45,
        "Oak parquet, lacquered",
    ),
    // roofing
    entry(
        "Roof tile, clay",
        "roofing",
        SOL,
        rgb(166, 76, 52),
        0.8,
        "Clay roof tiles",
    ),
    entry(
        "Roof tile, concrete",
        "roofing",
        SOL,
        rgb(96, 92, 90),
        0.85,
        "Concrete roof tiles",
    ),
    entry(
        "Roof slate",
        "roofing",
        SOL,
        rgb(64, 68, 76),
        0.6,
        "Natural slate roofing",
    ),
    metal(
        "Roof sheet, metal",
        "roofing",
        rgb(90, 98, 104),
        0.45,
        "Profiled metal roof sheet",
    ),
    entry(
        "Membrane, bitumen",
        "membrane",
        SOL,
        rgb(40, 40, 42),
        0.7,
        "Bituminous roofing membrane",
    ),
    entry(
        "Membrane, EPDM",
        "membrane",
        SOL,
        rgb(34, 34, 36),
        0.8,
        "EPDM rubber membrane",
    ),
    entry(
        "Green roof",
        "roofing",
        SOL,
        rgb(98, 140, 70),
        1.0,
        "Extensive sedum roof",
    ),
    // ground
    entry(
        "Gravel",
        "earth",
        DIA,
        rgb(170, 164, 150),
        1.0,
        "Gravel fill",
    ),
    entry(
        "Soil",
        "earth",
        DIA,
        rgb(116, 92, 68),
        1.0,
        "Compacted earth",
    ),
    entry(
        "Grass",
        "earth",
        DIA,
        rgb(104, 146, 74),
        1.0,
        "Lawn on topsoil",
    ),
    // hard landscape
    entry(
        "Paving, concrete flags",
        "concrete",
        CON,
        rgb(196, 192, 184),
        0.85,
        "Precast concrete paving flags",
    ),
    entry(
        "Block paving",
        "concrete",
        CON,
        rgb(150, 120, 104),
        0.85,
        "Concrete block paviors, for drives",
    ),
    entry(
        "Asphalt",
        "earth",
        SOL,
        rgb(70, 72, 74),
        0.9,
        "Bituminous macadam surfacing",
    ),
    entry(
        "Timber decking",
        "wood",
        TIM,
        rgb(150, 112, 76),
        0.7,
        "Hardwood deck boards",
    ),
];

/// The material named `name` in the model, or the library's entry of that
/// name as a new material to add: (its id, the element to insert, if new).
pub fn find_or_new(model: &Model, name: &str) -> Option<(ElementId, Option<Element>)> {
    if let Some(e) = model
        .iter()
        .find(|e| e.name == name && matches!(e.kind, ElementKind::Material(_)))
    {
        return Some((e.id, None));
    }
    let e = LIBRARY.iter().find(|l| l.name == name)?.element();
    Some((e.id, Some(e)))
}

/// A guess at a material's category from its name and description, for
/// materials read from files that don't give one. Empty when nothing fits.
pub fn guess_category(text: &str) -> &'static str {
    let t = text.to_lowercase();
    let words: Vec<&str> = t.split(|c: char| !c.is_alphanumeric()).collect();
    // short keys (EPS, OSB...) only as whole words
    let has = |keys: &[&str]| {
        keys.iter().any(|k| {
            if k.len() <= 3 {
                words.contains(k)
            } else {
                t.contains(k)
            }
        })
    };
    if has(&[
        "insulat",
        "mineral wool",
        "rock wool",
        "glass wool",
        "eps",
        "xps",
        "polystyrene",
        "pir",
    ]) {
        "insulation"
    } else if has(&["glass", "glazing", "glazed"]) && !has(&["tile"]) {
        "glass"
    } else if has(&["concrete", "beton", "béton", "cement", "screed"]) {
        "concrete"
    } else if has(&["brick", "masonry", "brique"]) {
        "brick"
    } else if has(&["block", "aac", "cmu"]) {
        "block"
    } else if has(&["stone", "granite", "marble", "limestone", "slate", "pierre"]) {
        "stone"
    } else if has(&[
        "wood", "timber", "oak", "pine", "lumber", "plywood", "osb", "clt", "glulam", "bois",
        "parquet",
    ]) {
        "wood"
    } else if has(&["aluminium", "aluminum"]) {
        "aluminium"
    } else if has(&["steel", "acier", "iron"]) {
        "steel"
    } else if has(&["metal", "copper", "zinc", "brass", "bronze", "chrome"]) {
        "metal"
    } else if has(&["gypsum", "plasterboard", "drywall", "stud"]) {
        "gypsum"
    } else if has(&["plaster", "render", "stucco"]) {
        "plaster"
    } else if has(&["roof", "tile"]) {
        "roofing"
    } else if has(&["membrane", "bitumen", "epdm"]) {
        "membrane"
    } else if has(&["paint", "carpet", "finish", "terrazzo"]) {
        "finish"
    } else if has(&["plastic", "pvc", "vinyl", "acrylic"]) {
        "plastic"
    } else if has(&["earth", "soil", "gravel", "sand", "ground"]) {
        "earth"
    } else {
        ""
    }
}

/// The usual cut hatch for a category.
pub fn hatch_for_category(category: &str) -> Hatch {
    match category.to_ascii_lowercase().as_str() {
        "concrete" => Hatch::Concrete,
        "brick" | "block" => Hatch::Brick,
        "wood" => Hatch::Timber,
        "stone" | "earth" => Hatch::Diagonal,
        "insulation" | "gypsum" => Hatch::Cross,
        "glass" | "finish" | "plastic" => Hatch::Empty,
        _ => Hatch::Solid,
    }
}

/// Roughness and metallic typical of a category, for materials read from
/// files that give only a colour.
pub fn surface_for_category(category: &str) -> (f32, f32) {
    match category.to_ascii_lowercase().as_str() {
        "steel" | "aluminium" | "metal" => (0.4, 1.0),
        "glass" => (0.05, 0.0),
        "stone" => (0.6, 0.0),
        "wood" => (0.7, 0.0),
        _ => (crate::element::DEFAULT_ROUGHNESS, 0.0),
    }
}

/// A material read from another file: its name and what could be read of
/// it. Category, hatch and surface are filled in from the name when the
/// file didn't give them.
pub fn imported(name: &str, mut m: Material, has_surface: bool) -> Material {
    if m.category.trim().is_empty() {
        m.category = guess_category(&format!("{name} {}", m.description)).into();
    }
    if !has_surface {
        let (r, metal) = surface_for_category(&m.category);
        m.roughness = r;
        m.metallic = metal;
    }
    m.clamped()
}

/// Every material of the model: (id, name, material), in model order.
pub fn materials(model: &Model) -> Vec<(ElementId, &str, &Material)> {
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Material(m) => Some((e.id, e.name.as_str(), m)),
            _ => None,
        })
        .collect()
}

/// The elements made of a material.
pub fn users(model: &Model, material: ElementId) -> Vec<ElementId> {
    model
        .iter()
        .filter(|e| {
            material_slot(e) == Some(Some(material))
                || matches!(&e.kind, ElementKind::ElementType(crate::ElementType {
                    params: crate::TypeParams::Object(t), ..
                }) if t.parts.iter().any(|p| p.material == Some(material)))
        })
        .map(|e| e.id)
        .collect()
}

/// An element's material, `Some(None)` for an element that can have one
/// but has none, `None` for kinds that can't have one.
pub fn material_slot(e: &Element) -> Option<Option<ElementId>> {
    match &e.kind {
        ElementKind::Wall(w) => Some(w.material),
        ElementKind::Slab(s) => Some(s.material),
        ElementKind::FreeMesh(m) => Some(m.material),
        ElementKind::Opening(o) => Some(o.material),
        k if crate::members::is_member(k) => Some(crate::members::material(k)),
        _ => None,
    }
}

/// Change what an element is made of. False for kinds without a material.
pub fn set_material(e: &mut Element, material: Option<ElementId>) -> bool {
    match &mut e.kind {
        ElementKind::Wall(w) => w.material = material,
        ElementKind::Slab(s) => s.material = material,
        ElementKind::FreeMesh(m) => m.material = material,
        ElementKind::Opening(o) => o.material = material,
        k => match crate::members::material_mut(k) {
            Some(m) => *m = material,
            None => return false,
        },
    }
    true
}

/// A name no other material of the model has: `base`, else `base 2`, `base 3`...
pub fn unique_name(model: &Model, base: &str) -> String {
    let taken: Vec<&str> = materials(model).into_iter().map(|(_, n, _)| n).collect();
    if !taken.contains(&base) {
        return base.to_string();
    }
    (2..)
        .map(|i| format!("{base} {i}"))
        .find(|n| !taken.contains(&n.as_str()))
        .expect("an unused name exists")
}

/// Make `material` what each of `elements` is made of (those that can have
/// one). Returns the transaction and how many elements it changes.
pub fn assign(
    model: &Model,
    elements: &[ElementId],
    material: Option<ElementId>,
) -> (Transaction, usize) {
    let mut tx = Transaction::new("Assign material");
    let mut n = 0;
    for id in elements {
        let Some(e) = model.get(*id) else { continue };
        if material_slot(e).is_none_or(|m| m == material) {
            continue;
        }
        let mut after = e.clone();
        set_material(&mut after, material);
        tx = tx.replace(e.clone(), after);
        n += 1;
    }
    (tx, n)
}

/// Delete a material, first making its users use `replacement` instead
/// (`None`: their type's default look). One undo step.
pub fn delete(model: &Model, material: ElementId, replacement: Option<ElementId>) -> Transaction {
    let users = users(model, material);
    let replacement = replacement.filter(|r| *r != material);
    let (mut tx, _) = assign(model, &users, replacement);
    tx.label = "Delete material".into();
    // layers of assemblies made of it too
    for (before, after) in crate::assembly::replace_material(model, material, replacement) {
        tx = tx.replace(before, after);
    }
    // and parts of objects
    for (before, after) in crate::objects::replace_material(model, material, replacement) {
        tx = tx.replace(before, after);
    }
    tx.remove_cascade(model, material)
}

/// A copy of a material under an unused name, not yet in the model.
pub fn duplicate(model: &Model, material: ElementId) -> Option<Element> {
    let e = model.get(material)?;
    let ElementKind::Material(m) = &e.kind else {
        return None;
    };
    let name = unique_name(model, &format!("{} copy", e.name));
    Some(Element::new(name, ElementKind::Material(m.clone())))
}

/// What merging imported materials into a model did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Merge {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
}

/// Merge materials read from another file (or the library) into a model:
/// a material whose name the model already has is updated in place (so
/// what is made of it changes with it), others are added. One undo step.
pub fn merge(
    model: &Model,
    incoming: Vec<(String, Material)>,
    label: &str,
) -> (Transaction, Merge, Vec<ElementId>) {
    let mut tx = Transaction::new(label);
    let mut report = Merge::default();
    let mut ids = vec![];
    let mut seen: Vec<String> = vec![];
    for (name, m) in incoming {
        let name = if name.trim().is_empty() {
            "Material".to_string()
        } else {
            name.trim().to_string()
        };
        if seen.contains(&name) {
            continue;
        }
        seen.push(name.clone());
        let m = m.clamped();
        let existing = model
            .iter()
            .find(|e| e.name == name && matches!(e.kind, ElementKind::Material(_)));
        match existing {
            Some(e) if e.kind == ElementKind::Material(m.clone()) => {
                report.unchanged += 1;
                ids.push(e.id);
            }
            Some(e) => {
                let mut after = e.clone();
                after.kind = ElementKind::Material(m);
                tx = tx.replace(e.clone(), after);
                report.updated += 1;
                ids.push(e.id);
            }
            None => {
                let e = Element::new(name, ElementKind::Material(m));
                ids.push(e.id);
                tx = tx.insert(e);
                report.added += 1;
            }
        }
    }
    (tx, report, ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    #[test]
    fn library_entries_are_valid_and_named_once() {
        let mut names: Vec<&str> = LIBRARY.iter().map(|e| e.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), LIBRARY.len());
        assert!(LIBRARY.len() >= 40);
        for e in LIBRARY {
            e.element().validate().unwrap();
            assert!(
                CATEGORIES.iter().any(|(c, _)| *c == e.category),
                "{}",
                e.name
            );
        }
        // every category the editor offers has something in the library
        for (c, _) in CATEGORIES {
            assert!(LIBRARY.iter().any(|e| e.category == *c), "{c}");
        }
    }

    #[test]
    fn guesses_follow_names() {
        assert_eq!(guess_category("Concrete, Cast-in-Place gray"), "concrete");
        assert_eq!(guess_category("Metal - Steel - 345 MPa"), "steel");
        assert_eq!(guess_category("Glass"), "glass");
        assert_eq!(guess_category("Ceramic tile"), "roofing");
        assert_eq!(guess_category("Mystery"), "");
        assert_eq!(category_label("concrete"), "Concrete");
        assert_eq!(category_label("tarmac"), "Tarmac");
        assert_eq!(category_label(""), "Uncategorised");
        let m = imported("Steel beam", Material::default(), false);
        assert_eq!((m.category.as_str(), m.metallic), ("steel", 1.0));
    }

    #[test]
    fn delete_reassigns_users_in_one_step() {
        let mut doc = Document::from_model(crate::sample::sample_house());
        let model = doc.model().clone();
        let (brick, _, _) = materials(&model)
            .into_iter()
            .find(|(_, n, _)| *n == "Brick")
            .unwrap();
        let (concrete, _, _) = materials(&model)
            .into_iter()
            .find(|(_, n, _)| *n == "Concrete")
            .unwrap();
        let used = users(&model, brick);
        assert!(!used.is_empty());
        doc.commit(delete(&model, brick, Some(concrete))).unwrap();
        assert!(doc.model().get(brick).is_none());
        for u in &used {
            let e = doc.model().get(*u).unwrap();
            assert_eq!(material_slot(e), Some(Some(concrete)));
        }
        doc.undo();
        assert_eq!(users(doc.model(), brick), used);
    }

    #[test]
    fn doors_windows_and_members_take_a_material() {
        use crate::members::{Column, Profile};
        use crate::Model;
        let mut model = crate::sample::sample_house();
        let level = model
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Level(_)))
            .unwrap()
            .id;
        let column = Element::new(
            "C1",
            ElementKind::Column(Column::new(
                level,
                glam::DVec2::new(3.0, 3.0),
                Profile::Round { diameter: 0.3 },
            )),
        );
        model = Model::from_elements(model.iter().cloned().chain([column.clone()])).unwrap();
        let (timber, _, _) = materials(&model)
            .into_iter()
            .find(|(_, n, _)| *n == "Timber")
            .unwrap();
        let door = model
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Opening(_)))
            .unwrap()
            .clone();
        let mut doc = Document::from_model(model);
        for e in [door.clone(), column.clone()] {
            assert_eq!(material_slot(&e), Some(None), "{}", e.name);
            let mut after = e.clone();
            assert!(set_material(&mut after, Some(timber)));
            doc.commit(crate::Transaction::new("t").replace(e, after))
                .unwrap();
        }
        let used = users(doc.model(), timber);
        assert!(used.contains(&door.id) && used.contains(&column.id));
        let e = doc.model().get(door.id).unwrap();
        assert_eq!(
            crate::material_of(doc.model(), e).map(|(m, _)| m.id),
            Some(timber)
        );
        // deleting the material leaves them with none
        let model = doc.model().clone();
        doc.commit(delete(&model, timber, None)).unwrap();
        assert_eq!(
            material_slot(doc.model().get(column.id).unwrap()),
            Some(None)
        );
        assert_eq!(material_slot(doc.model().get(door.id).unwrap()), Some(None));
    }

    #[test]
    fn merge_adds_updates_and_skips() {
        let model = crate::sample::sample_house();
        let mut brick = materials(&model)
            .into_iter()
            .find(|(_, n, _)| *n == "Brick")
            .map(|(_, _, m)| m.clone())
            .unwrap();
        let timber = materials(&model)
            .into_iter()
            .find(|(_, n, _)| *n == "Timber")
            .map(|(_, _, m)| m.clone())
            .unwrap();
        brick.color = [0.5, 0.2, 0.1];
        let (tx, r, ids) = merge(
            &model,
            vec![
                ("Brick".into(), brick),
                ("Timber".into(), timber),
                ("Zinc".into(), Material::default()),
                ("Zinc".into(), Material::default()),
            ],
            "Import materials",
        );
        assert_eq!(
            r,
            Merge {
                added: 1,
                updated: 1,
                unchanged: 1
            }
        );
        assert_eq!(ids.len(), 3);
        let mut doc = Document::from_model(model);
        doc.commit(tx).unwrap();
        assert_eq!(unique_name(doc.model(), "Zinc"), "Zinc 2");
        assert_eq!(unique_name(doc.model(), "Lead"), "Lead");
    }
}
