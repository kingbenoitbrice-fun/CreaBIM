//! IFC STEP import (IFC2X3, IFC4, IFC4X3).
//!
//! Storeys become levels. Walls, slabs, doors and windows that follow the
//! simple patterns CreaBIM models natively (a straight wall extruded from a
//! footprint between two parallel faces, a horizontal slab whose top sits on
//! a storey, a door or window filling an opening in such a wall) become
//! native elements, so they stay editable as walls and slabs. Everything
//! else with a body becomes a free mesh that keeps its IFC class, so nothing
//! visible is lost. Element ids are decoded from GlobalIds, so a model that
//! round-trips through IFC keeps its identity.

use crate::classes::product_class;
use crate::shape::{Geo, Warnings};
use crate::step_read::{parse, Entity, StepError, StepFile, Value};
use crate::tess::{area2, bounds, merge_coplanar, weld};
use creabim_core::{
    Document, EditMesh, Element, ElementId, ElementKind, FreeMesh, Grid, Hatch, Level, Material,
    Model, Opening, OpeningKind, Slab, Transaction, Wall,
};
use glam::{DMat4, DVec2, DVec3};
use std::collections::{HashMap, HashSet};

/// A model read from a file, with what could not be brought across.
#[derive(Debug)]
pub struct ImportResult {
    pub model: Model,
    /// Human-readable notes about anything skipped or approximated.
    pub warnings: Vec<String>,
    /// The schema named in the file, e.g. `IFC4` or `ifcx_alpha`.
    pub schema: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error(transparent)]
    Step(#[from] StepError),
    #[error("not an IFC file (schema {0:?}, no IfcProject)")]
    NotIfc(String),
    #[error("invalid IFCX file: {0}")]
    Ifcx(String),
    #[error("imported elements are inconsistent: {0}")]
    Model(String),
}

/// Read an IFC STEP file into a CreaBIM model.
pub fn import_ifc(text: &str) -> Result<ImportResult, ImportError> {
    let file = parse(text)?;
    let schema = file.schemas.first().cloned().unwrap_or_default();
    let by_class = file.by_class();
    let has_project = by_class.contains_key("IFCPROJECT");
    if !has_project && !schema.to_ascii_uppercase().starts_with("IFC") {
        return Err(ImportError::NotIfc(schema));
    }
    let mut imp = Importer::new(&file, &by_class);
    if !file.errors.is_empty() {
        imp.g.warnings.add(format!(
            "{} damaged records were skipped; first: {}",
            file.errors.len(),
            file.errors[0]
        ));
    }
    imp.run();
    let elements = std::mem::take(&mut imp.out);
    let mut warnings = std::mem::take(&mut imp.g.warnings);
    let model = build_model(elements, &mut warnings)?;
    Ok(ImportResult {
        model,
        warnings: warnings.into_vec(),
        schema,
    })
}

/// Read only the materials of an IFC STEP file: every IfcMaterial, with
/// its category and description, and its colour, transparency and (when
/// given) roughness from its IfcSurfaceStyle, or from the style on the body
/// of something made of it. Names are kept as they are, so importing again
/// updates the same materials.
pub fn import_ifc_materials(text: &str) -> Result<Vec<(String, Material)>, ImportError> {
    let file = parse(text)?;
    let schema = file.schemas.first().cloned().unwrap_or_default();
    let by_class = file.by_class();
    if !by_class.contains_key("IFCPROJECT") && !schema.to_ascii_uppercase().starts_with("IFC") {
        return Err(ImportError::NotIfc(schema));
    }
    let mut imp = Importer::new(&file, &by_class);
    imp.index_relationships();
    Ok(imp.all_materials())
}

/// Insert elements into a fresh model, dropping any that fail validation.
/// Elements must come in dependency order.
pub(crate) fn build_model(
    elements: Vec<Element>,
    warnings: &mut Warnings,
) -> Result<Model, ImportError> {
    let mut tx = Transaction::new("Import");
    let mut ids: HashSet<ElementId> = HashSet::new();
    for e in elements {
        if let Err(msg) = e.validate() {
            warnings.add(format!("{} {:?} skipped: {msg}", e.ifc_class(), e.name));
            continue;
        }
        if let Some(missing) = e.references().into_iter().find(|r| !ids.contains(r)) {
            warnings.add(format!(
                "{} {:?} skipped: it depends on {missing}, which was not imported",
                e.ifc_class(),
                e.name
            ));
            continue;
        }
        if !ids.insert(e.id) {
            warnings.add(format!("duplicate id for {:?}", e.name));
            continue;
        }
        tx = tx.insert(e);
    }
    let mut doc = Document::new();
    doc.commit(tx)
        .map_err(|e| ImportError::Model(e.to_string()))?;
    Ok(doc.model().clone())
}

/// Classes with no physical body of their own, or handled separately.
const SKIPPED: &[&str] = &[
    "IfcProject",
    "IfcBuilding",
    "IfcBuildingStorey",
    "IfcSpace",
    "IfcSpatialZone",
    "IfcExternalSpatialElement",
    "IfcFacility",
    "IfcFacilityPart",
    "IfcFacilityPartCommon",
    "IfcBridge",
    "IfcBridgePart",
    "IfcRoad",
    "IfcRoadPart",
    "IfcRailway",
    "IfcRailwayPart",
    "IfcMarineFacility",
    "IfcMarinePart",
    "IfcOpeningElement",
    "IfcOpeningStandardCase",
    "IfcVoidingFeature",
    "IfcAnnotation",
    "IfcGrid",
    "IfcVirtualElement",
    "IfcPort",
    "IfcDistributionPort",
    "IfcAlignment",
    "IfcAlignmentCant",
    "IfcAlignmentHorizontal",
    "IfcAlignmentSegment",
    "IfcAlignmentVertical",
    "IfcReferent",
    "IfcLinearPositioningElement",
    "IfcPositioningElement",
    "IfcLinearElement",
    "IfcProxy",
];

/// A wall that became native, for hosting its doors and windows.
#[derive(Clone, Copy)]
pub(crate) struct HostWall {
    pub id: ElementId,
    pub start: DVec2,
    pub end: DVec2,
    pub thickness: f64,
    pub base_z: f64,
}

struct Storey {
    id: ElementId,
    elevation: f64,
}

/// A vertical prism: a plan outline between two heights, in world coordinates.
pub(crate) struct Prism {
    pub footprint: Vec<DVec2>,
    pub z0: f64,
    pub z1: f64,
    pub holes: bool,
    pub clipped: bool,
}

struct Importer<'a> {
    f: &'a StepFile,
    by_class: &'a HashMap<&'a str, Vec<u64>>,
    g: Geo<'a>,
    out: Vec<Element>,
    used_ids: HashSet<ElementId>,
    storeys: HashMap<u64, Storey>,
    /// Levels by elevation, for elements not placed on a storey.
    levels: Vec<(f64, ElementId)>,
    contained: HashMap<u64, u64>,
    parent: HashMap<u64, u64>,
    voids: HashMap<u64, u64>,
    fills: HashMap<u64, u64>,
    /// Hosts with openings, openings with fillings, elements with parts.
    voided: HashSet<u64>,
    filled: HashSet<u64>,
    has_parts: HashSet<u64>,
    materials_of: HashMap<u64, u64>,
    type_of: HashMap<u64, u64>,
    styles: HashMap<u64, Surface>,
    material_colors: HashMap<u64, Surface>,
    /// Imported materials by name.
    materials: HashMap<String, ElementId>,
    /// Imported assemblies, by the IfcMaterialLayerSet they came from.
    layer_sets: HashMap<u64, ElementId>,
    assembly_elems: Vec<Element>,
    walls: HashMap<u64, HostWall>,
    /// Elements written so far, by IFC instance, so nothing is imported twice.
    done: HashSet<u64>,
    // elements are collected per kind, then emitted in dependency order
    material_elems: Vec<Element>,
    level_elems: Vec<Element>,
    grid_elems: Vec<Element>,
    wall_elems: Vec<Element>,
    other_elems: Vec<Element>,
    opening_elems: Vec<Element>,
}

impl<'a> Importer<'a> {
    fn new(f: &'a StepFile, by_class: &'a HashMap<&'a str, Vec<u64>>) -> Self {
        let (len, angle, unit_note) = units(f, by_class);
        let mut g = Geo::new(f, len, angle);
        if let Some(note) = unit_note {
            g.warnings.add(note);
        }
        Self {
            f,
            by_class,
            g,
            out: vec![],
            used_ids: HashSet::new(),
            storeys: HashMap::new(),
            levels: vec![],
            contained: HashMap::new(),
            parent: HashMap::new(),
            voids: HashMap::new(),
            fills: HashMap::new(),
            voided: HashSet::new(),
            filled: HashSet::new(),
            has_parts: HashSet::new(),
            materials_of: HashMap::new(),
            type_of: HashMap::new(),
            styles: HashMap::new(),
            material_colors: HashMap::new(),
            materials: HashMap::new(),
            layer_sets: HashMap::new(),
            assembly_elems: vec![],
            walls: HashMap::new(),
            done: HashSet::new(),
            material_elems: vec![],
            level_elems: vec![],
            grid_elems: vec![],
            wall_elems: vec![],
            other_elems: vec![],
            opening_elems: vec![],
        }
    }

    fn ids(&self, class: &str) -> &'a [u64] {
        self.by_class.get(class).map(Vec::as_slice).unwrap_or(&[])
    }

    fn run(&mut self) {
        self.index_relationships();
        self.read_storeys();
        self.read_grids();
        for class in ["IFCWALL", "IFCWALLSTANDARDCASE", "IFCWALLELEMENTEDCASE"] {
            for &id in self.ids(class) {
                self.import_wall(id);
            }
        }
        for class in ["IFCSLAB", "IFCSLABSTANDARDCASE", "IFCSLABELEMENTEDCASE"] {
            for &id in self.ids(class) {
                self.import_slab(id);
            }
        }
        for class in [
            "IFCDOOR",
            "IFCDOORSTANDARDCASE",
            "IFCWINDOW",
            "IFCWINDOWSTANDARDCASE",
        ] {
            for &id in self.ids(class) {
                self.import_filling(id);
            }
        }
        let mut rest: Vec<u64> = self
            .f
            .entities
            .iter()
            .filter(|(id, e)| {
                !self.done.contains(id)
                    && !e.arg(6).is_null()
                    && product_class(&e.class).is_some_and(|c| !SKIPPED.contains(&c))
                    && !e.class.starts_with("IFCSTRUCTURAL")
            })
            .map(|(id, _)| *id)
            .collect();
        rest.sort_unstable();
        for id in rest {
            self.import_free_mesh(id);
        }
        // the rest of the file's layer sets (a roof's, unused ones) come
        // along as assemblies, as its materials do
        let mut sets: Vec<u64> = self.ids("IFCMATERIALLAYERSET").to_vec();
        sets.sort_unstable();
        for set in sets {
            let roof = self.materials_of.iter().any(|(p, m)| {
                *m == set
                    && self
                        .f
                        .get(*p)
                        .is_some_and(|e| e.class.starts_with("IFCROOF"))
            });
            let name = self
                .f
                .get(set)
                .and_then(|e| e.arg(1).as_str())
                .unwrap_or("")
                .to_lowercase();
            let usage = if roof || name.contains("roof") {
                creabim_core::AssemblyUse::Roof
            } else if name.contains("floor") || name.contains("slab") {
                creabim_core::AssemblyUse::Floor
            } else {
                creabim_core::AssemblyUse::Wall
            };
            self.layer_set(set, usage);
        }
        let spaces = self.ids("IFCSPACE").len();
        if spaces > 0 {
            self.g.warnings.add(format!(
                "{spaces} spaces were not imported (CreaBIM has no rooms yet)"
            ));
        }
        let bare = self
            .ids("IFCOPENINGELEMENT")
            .iter()
            .chain(self.ids("IFCOPENINGSTANDARDCASE"))
            .filter(|o| !self.filled.contains(*o))
            .count();
        if bare > 0 {
            self.g.warnings.add(format!(
                "{bare} openings without a door or window were not cut (CreaBIM has no bare openings)"
            ));
        }
        self.out = [
            std::mem::take(&mut self.material_elems),
            std::mem::take(&mut self.assembly_elems),
            std::mem::take(&mut self.level_elems),
            std::mem::take(&mut self.grid_elems),
            std::mem::take(&mut self.wall_elems),
            std::mem::take(&mut self.other_elems),
            std::mem::take(&mut self.opening_elems),
        ]
        .concat();
    }

    fn index_relationships(&mut self) {
        let f = self.f;
        let by_class = self.by_class;
        let each = |class: &str| {
            by_class
                .get(class)
                .into_iter()
                .flatten()
                .filter_map(|id| f.get(*id))
        };
        for r in each("IFCRELCONTAINEDINSPATIALSTRUCTURE") {
            if let Some(s) = r.arg(5).as_ref() {
                for e in r.arg(4).refs() {
                    self.contained.insert(e, s);
                }
            }
        }
        for r in each("IFCRELAGGREGATES").chain(each("IFCRELNESTS")) {
            if let Some(p) = r.arg(4).as_ref() {
                for c in r.arg(5).refs() {
                    self.parent.insert(c, p);
                }
            }
        }
        for r in each("IFCRELVOIDSELEMENT") {
            if let (Some(host), Some(o)) = (r.arg(4).as_ref(), r.arg(5).as_ref()) {
                self.voids.insert(o, host);
            }
        }
        for r in each("IFCRELFILLSELEMENT") {
            if let (Some(o), Some(fill)) = (r.arg(4).as_ref(), r.arg(5).as_ref()) {
                self.fills.insert(fill, o);
            }
        }
        for r in each("IFCRELASSOCIATESMATERIAL") {
            if let Some(m) = r.arg(5).as_ref() {
                for o in r.arg(4).refs() {
                    self.materials_of.insert(o, m);
                }
            }
        }
        for r in each("IFCRELDEFINESBYTYPE") {
            if let Some(t) = r.arg(5).as_ref() {
                for o in r.arg(4).refs() {
                    self.type_of.insert(o, t);
                }
            }
        }
        self.voided = self.voids.values().copied().collect();
        self.filled = self.fills.values().copied().collect();
        self.has_parts = self.parent.values().copied().collect();
        for s in each("IFCSTYLEDITEM") {
            if let (Some(item), Some(c)) = (s.arg(0).as_ref(), style_surface(f, s.arg(1))) {
                self.styles.insert(item, c);
            }
        }
        for mdr in each("IFCMATERIALDEFINITIONREPRESENTATION") {
            let Some(m) = mdr.arg(3).as_ref() else {
                continue;
            };
            let color = mdr
                .arg(2)
                .refs()
                .filter_map(|r| f.get(r))
                .flat_map(|r| r.arg(3).refs().collect::<Vec<_>>())
                .filter_map(|i| f.get(i))
                .find_map(|si| style_surface(f, si.arg(1)));
            if let Some(c) = color {
                self.material_colors.insert(m, c);
            }
        }
    }

    fn new_id(&mut self, e: &Entity) -> ElementId {
        let id = e
            .arg(0)
            .as_str()
            .and_then(ElementId::from_ifc_guid)
            .filter(|id| !self.used_ids.contains(id))
            .unwrap_or_default();
        self.used_ids.insert(id);
        id
    }

    fn name(e: &Entity, fallback: &str) -> String {
        e.arg(2)
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| fallback.to_string())
    }

    fn read_storeys(&mut self) {
        let mut list: Vec<(f64, u64)> = Vec::new();
        for &id in self.ids("IFCBUILDINGSTOREY") {
            let Some(e) = self.f.get(id) else { continue };
            let z = if e.arg(5).is_null() {
                self.g.length(e.arg(9)).unwrap_or(0.0)
            } else {
                self.g.placement(e.arg(5)).w_axis.z
            };
            list.push((z, id));
        }
        list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if list.is_empty() {
            self.g
                .warnings
                .add("the file has no storeys; everything was put on one level at 0");
            let e = Element::new(
                "Level 0",
                ElementKind::Level(Level {
                    elevation: 0.0,
                    height: 3.0,
                }),
            );
            self.levels.push((0.0, e.id));
            self.level_elems.push(e);
            return;
        }
        for (i, &(z, id)) in list.iter().enumerate() {
            let Some(e) = self.f.get(id) else { continue };
            let height = list
                .get(i + 1)
                .map(|n| n.0 - z)
                .filter(|h| *h > 0.01)
                .unwrap_or(3.0);
            let lid = self.new_id(e);
            let mut name = Self::name(e, "");
            if name.is_empty() {
                name = e
                    .arg(7)
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("Level {i}"));
            }
            self.level_elems.push(Element {
                source: None,
                id: lid,
                name,
                kind: ElementKind::Level(Level {
                    elevation: z,
                    height,
                }),
            });
            self.storeys.insert(
                id,
                Storey {
                    id: lid,
                    elevation: z,
                },
            );
            self.levels.push((z, lid));
        }
    }

    fn read_grids(&mut self) {
        for &id in self.ids("IFCGRID") {
            let Some(e) = self.f.get(id) else { continue };
            let world = self.g.placement(e.arg(5));
            // a grid of several axes becomes a grid group holding them
            let count: usize = [7, 8, 9].iter().map(|i| e.arg(*i).as_list().len()).sum();
            let group = (count > 1).then(|| {
                let name = e.arg(2).as_str().unwrap_or("").trim().to_string();
                let g = Element::new(
                    if name.is_empty() { "Grid".into() } else { name },
                    ElementKind::GridGroup(creabim_core::GridGroup::default()),
                );
                let id = g.id;
                self.grid_elems.push(g);
                id
            });
            for axes in [7, 8, 9] {
                for ax in e.arg(axes).as_list() {
                    let Some(a) = self.g.get(ax) else { continue };
                    let Some(pts) = self.g.curve(a.arg(1)) else {
                        continue;
                    };
                    let (Some(p), Some(q)) = (pts.first(), pts.last()) else {
                        continue;
                    };
                    let (p, q) = (world.transform_point3(*p), world.transform_point3(*q));
                    let label = a.arg(0).as_str().unwrap_or("").to_string();
                    self.grid_elems.push(Element::new(
                        format!("Grid {label}"),
                        ElementKind::Grid(Grid {
                            label,
                            start: p.truncate(),
                            end: q.truncate(),
                            group,
                        }),
                    ));
                }
            }
        }
    }

    /// The storey an element sits on: through its spatial container, the
    /// element it is part of, or the container's own parents.
    fn storey_of(&self, id: u64) -> Option<&Storey> {
        let mut cur = id;
        for _ in 0..32 {
            if let Some(s) = self.storeys.get(&cur) {
                return Some(s);
            }
            // a door or window is usually contained, but may only fill an opening
            let next = self
                .contained
                .get(&cur)
                .or_else(|| self.parent.get(&cur))
                .or_else(|| self.fills.get(&cur))
                .or_else(|| self.voids.get(&cur))?;
            cur = *next;
        }
        None
    }

    /// The level with the highest elevation at or below `z`.
    fn level_at(&self, z: f64) -> (ElementId, f64) {
        let mut best = self.levels.first().map(|l| (l.1, l.0)).unwrap_or_default();
        for &(e, id) in &self.levels {
            if e <= z + 1e-3 {
                best = (id, e);
            }
        }
        best
    }

    fn level_for(&self, id: u64, z: f64) -> (ElementId, f64) {
        match self.storey_of(id) {
            Some(s) => (s.id, s.elevation),
            None => self.level_at(z),
        }
    }

    // ---- materials ----

    /// The CreaBIM material for a product, from its own association or its type's.
    fn material_for(&mut self, id: u64) -> Option<ElementId> {
        let assoc = self
            .materials_of
            .get(&id)
            .or_else(|| self.type_of.get(&id).and_then(|t| self.materials_of.get(t)))
            .copied()?;
        let (name, mat_id) = self.material_name(assoc, 0)?;
        if let Some(&m) = self.materials.get(&name) {
            return Some(m);
        }
        let f = self.f;
        let surface = mat_id
            .and_then(|m| self.material_colors.get(&m).copied())
            .or_else(|| self.product_surface(f.get(id)?));
        let material = material_from(f, &name, mat_id, surface);
        let e = Element::new(name.clone(), ElementKind::Material(material));
        let mid = e.id;
        self.material_elems.push(e);
        self.materials.insert(name, mid);
        Some(mid)
    }

    /// The CreaBIM material for an IfcMaterial, made on first use.
    fn material_entity(&mut self, mat: u64) -> Option<ElementId> {
        let f = self.f;
        let e = f.get(mat).filter(|e| e.is("IFCMATERIAL"))?;
        let name = e.arg(0).as_str().map(str::trim).filter(|n| !n.is_empty())?;
        if let Some(&m) = self.materials.get(name) {
            return Some(m);
        }
        let surface = self.material_colors.get(&mat).copied();
        let material = material_from(f, name, Some(mat), surface);
        let e = Element::new(name, ElementKind::Material(material));
        let id = e.id;
        self.material_elems.push(e);
        self.materials.insert(name.to_string(), id);
        Some(id)
    }

    /// The CreaBIM assembly for an IfcMaterialLayerSet of two or more
    /// layers, made on first use: its layers with their materials,
    /// thicknesses and functions. (A set of one layer is just a material.)
    fn layer_set(&mut self, set: u64, usage: creabim_core::AssemblyUse) -> Option<ElementId> {
        use creabim_core::assembly::{Layer, LayerFunction, MIN_LAYER};
        if let Some(&a) = self.layer_sets.get(&set) {
            return Some(a);
        }
        let f = self.f;
        let e = f.get(set).filter(|e| e.is("IFCMATERIALLAYERSET"))?;
        if e.arg(0).refs().count() < 2 {
            return None;
        }
        let mut layers = vec![];
        for l in e.arg(0).refs().filter_map(|l| f.get(l)) {
            if !l.is("IFCMATERIALLAYER") {
                continue;
            }
            let material = l.arg(0).as_ref().and_then(|m| self.material_entity(m));
            let thickness = self.g.length(l.arg(1)).unwrap_or(0.0).max(MIN_LAYER);
            let ventilated = l.arg(2).as_enum() == Some("T");
            let category = l.arg(5).as_str().or(l.arg(3).as_str()).unwrap_or("");
            let function = if material.is_none() && ventilated {
                LayerFunction::AirGap
            } else {
                LayerFunction::from_ifc(category, ventilated)
            };
            layers.push(Layer::new(function, material, thickness));
        }
        if layers.is_empty() {
            return None;
        }
        let base = e
            .arg(1)
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or("Assembly")
            .to_string();
        let mut name = base.clone();
        let mut n = 2;
        while self.assembly_elems.iter().any(|a| a.name == name) {
            name = format!("{base} {n}");
            n += 1;
        }
        let mut a = creabim_core::Assembly::new(usage, layers);
        a.description = e.arg(2).as_str().unwrap_or("").trim().to_string();
        let el = Element::new(name, ElementKind::Assembly(a));
        let id = el.id;
        self.assembly_elems.push(el);
        self.layer_sets.insert(set, id);
        Some(id)
    }

    /// What an imported assembly's core is made of: the element's material
    /// should it lose the assembly.
    fn core_material(&self, assembly: ElementId) -> Option<ElementId> {
        self.assembly_elems.iter().find_map(|e| match &e.kind {
            ElementKind::Assembly(a) if e.id == assembly => a.layers[a.core().0].material,
            _ => None,
        })
    }

    /// A layered product's assembly, and for one placed with a layer set
    /// usage, whether its layers run the usage axis's positive way.
    fn assembly_for(
        &mut self,
        id: u64,
        usage: creabim_core::AssemblyUse,
    ) -> Option<(ElementId, Option<bool>)> {
        let assoc = self
            .materials_of
            .get(&id)
            .or_else(|| self.type_of.get(&id).and_then(|t| self.materials_of.get(t)))
            .copied()?;
        let e = self.f.get(assoc)?;
        let (set, positive) = if e.is("IFCMATERIALLAYERSETUSAGE") {
            (
                e.arg(0).as_ref()?,
                Some(e.arg(2).as_enum() != Some("NEGATIVE")),
            )
        } else if e.is("IFCMATERIALLAYERSET") {
            (assoc, None)
        } else {
            return None;
        };
        Some((self.layer_set(set, usage)?, positive))
    }

    /// The surface style on a product's body, if it has one.
    fn product_surface(&self, e: &Entity) -> Option<Surface> {
        let body = self.g.body(e)?;
        body.items.iter().find_map(|i| self.styles.get(i).copied())
    }

    /// Every IfcMaterial of the file, whether anything uses it or not,
    /// with its look: its own surface style, else the style on the body of
    /// the first product made of it.
    fn all_materials(&self) -> Vec<(String, Material)> {
        let f = self.f;
        // the first product made of each material, for its body's style
        let mut product_of: HashMap<u64, u64> = HashMap::new();
        let mut assocs: Vec<(&u64, &u64)> = self.materials_of.iter().collect();
        assocs.sort();
        for (product, assoc) in assocs {
            if let Some((_, Some(m))) = self.material_name(*assoc, 0) {
                product_of.entry(m).or_insert(*product);
            }
        }
        let mut out: Vec<(String, Material)> = vec![];
        for &id in self.ids("IFCMATERIAL") {
            let Some(e) = f.get(id) else { continue };
            let Some(name) = e.arg(0).as_str().map(str::trim).filter(|n| !n.is_empty()) else {
                continue;
            };
            if out.iter().any(|(n, _)| n == name) {
                continue;
            }
            let surface = self.material_colors.get(&id).copied().or_else(|| {
                let p = f.get(*product_of.get(&id)?)?;
                self.product_surface(p)
            });
            out.push((name.to_string(), material_from(f, name, Some(id), surface)));
        }
        out
    }

    /// A material select's name and the IfcMaterial it is mostly made of.
    fn material_name(&self, id: u64, depth: usize) -> Option<(String, Option<u64>)> {
        let e = self.f.get(id)?;
        if depth > 8 {
            return None;
        }
        let named = |s: &Value| {
            s.as_str()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
        };
        match e.class.as_str() {
            "IFCMATERIAL" => Some((named(e.arg(0))?, Some(id))),
            "IFCMATERIALLAYERSETUSAGE" => self.material_name(e.arg(0).as_ref()?, depth + 1),
            "IFCMATERIALLAYERSET" => {
                // the thickest layer is what the element is mostly made of
                let main = e
                    .arg(0)
                    .refs()
                    .filter_map(|l| self.f.get(l))
                    .max_by(|a, b| {
                        let t = |l: &Entity| l.arg(1).as_f64().unwrap_or(0.0);
                        t(a).total_cmp(&t(b))
                    })
                    .and_then(|l| l.arg(0).as_ref());
                let main_name = main.and_then(|m| self.material_name(m, depth + 1));
                match named(e.arg(1)) {
                    Some(n) => Some((n, main_name.and_then(|m| m.1))),
                    None => main_name,
                }
            }
            "IFCMATERIALLAYER" | "IFCMATERIALPROFILE" | "IFCMATERIALCONSTITUENT" => {
                let m = if e.is("IFCMATERIALLAYER") {
                    e.arg(0)
                } else {
                    e.arg(2)
                };
                self.material_name(m.as_ref()?, depth + 1)
            }
            "IFCMATERIALLIST" => self.material_name(e.arg(0).refs().next()?, depth + 1),
            "IFCMATERIALPROFILESETUSAGE" => self.material_name(e.arg(0).as_ref()?, depth + 1),
            "IFCMATERIALPROFILESET" | "IFCMATERIALCONSTITUENTSET" => {
                let first = e
                    .arg(2)
                    .refs()
                    .find_map(|p| self.material_name(p, depth + 1));
                match named(e.arg(0)) {
                    Some(n) => Some((n, first.and_then(|m| m.1))),
                    None => first,
                }
            }
            _ => None,
        }
    }

    /// Total thickness of an element's layer set usage, and where its
    /// centre lies on the element's local y axis.
    fn layer_band(&self, id: u64) -> Option<(f64, f64)> {
        let usage = self.f.get(*self.materials_of.get(&id)?)?;
        if !usage.is("IFCMATERIALLAYERSETUSAGE") {
            return None;
        }
        let set = self.g.get(usage.arg(0))?;
        let total: f64 = set
            .arg(0)
            .refs()
            .filter_map(|l| self.f.get(l))
            .filter_map(|l| self.g.length(l.arg(1)))
            .sum();
        if total <= 0.0 {
            return None;
        }
        let sense = if usage.arg(2).as_enum() == Some("NEGATIVE") {
            -1.0
        } else {
            1.0
        };
        let offset = self.g.length(usage.arg(3)).unwrap_or(0.0);
        Some((total, offset + sense * total / 2.0))
    }

    // ---- shape analysis ----

    /// The body as a vertical prism, if it is one: a single extrusion
    /// straight up or down, or a mesh with flat top and bottom.
    fn prism(&mut self, e: &Entity, world: &DMat4) -> Option<Prism> {
        let body = self.g.body(e)?;
        let [mut item] = body.items[..] else {
            return self.mesh_prism(e, world);
        };
        let mut clipped = false;
        for _ in 0..8 {
            let it = self.f.get(item)?;
            if it.is("IFCBOOLEANCLIPPINGRESULT") {
                clipped = true;
                item = it.arg(1).as_ref()?;
            } else {
                break;
            }
        }
        let it = self.f.get(item)?;
        if !it.is("IFCEXTRUDEDAREASOLID") {
            if clipped {
                return None;
            }
            return self.mesh_prism(e, world);
        }
        let x = self.g.extrusion(item, world)?;
        let normal = x.position.z_axis.truncate();
        let v = x.position.transform_vector3(x.vector);
        if normal.cross(DVec3::Z).length() > 1e-6 || v.truncate().length() > 1e-6 * v.length() {
            return None;
        }
        let [region] = &x.regions[..] else {
            return None;
        };
        let footprint: Vec<DVec3> = region
            .outer
            .iter()
            .map(|p| x.position.transform_point3(p.extend(0.0)))
            .collect();
        let z = footprint.first()?.z;
        let mut plan: Vec<DVec2> = footprint.iter().map(|p| p.truncate()).collect();
        if area2(&plan) < 0.0 {
            plan.reverse();
        }
        Some(Prism {
            footprint: plan,
            z0: z.min(z + v.z),
            z1: z.max(z + v.z),
            holes: !region.holes.is_empty(),
            clipped,
        })
    }

    /// Recognise a prism in a mesh body (tessellated or B-rep walls and slabs).
    fn mesh_prism(&mut self, e: &Entity, world: &DMat4) -> Option<Prism> {
        prism_of_mesh(self.g.product_mesh(e, world))
    }

    /// A wall's axis in world plan coordinates, if it is a straight line.
    fn axis(&mut self, e: &Entity, world: &DMat4) -> Option<(DVec2, DVec2)> {
        let rep = self
            .g
            .reps(e)
            .into_iter()
            .find(|r| r.identifier.eq_ignore_ascii_case("Axis"))?;
        let pts = self.g.curve(&Value::Ref(*rep.items.first()?))?;
        let pts: Vec<DVec2> = pts
            .iter()
            .map(|p| world.transform_point3(*p).truncate())
            .collect();
        let (a, b) = (*pts.first()?, *pts.last()?);
        let d = (b - a).normalize_or_zero();
        if d == DVec2::ZERO || pts.iter().any(|p| (*p - a).perp_dot(d).abs() > 1e-4) {
            return None;
        }
        Some((a, b))
    }

    fn import_wall(&mut self, id: u64) {
        let Some(e) = self.f.get(id) else { return };
        let world = self.g.placement(e.arg(5));
        if let Some(w) = self.native_wall(id, e, &world) {
            self.done.insert(id);
            let lid = w.level;
            let level_elev = self.levels.iter().find(|l| l.1 == lid).map_or(0.0, |l| l.0);
            let layered = self.assembly_for(id, creabim_core::AssemblyUse::Wall);
            // the first (exterior) layer is on the wall's left when the
            // layers run towards its right
            let flip = layered.and_then(|l| l.1).is_some_and(|positive| {
                let y = world.y_axis.truncate().truncate();
                let left = (w.end - w.start).normalize_or_zero().perp();
                positive == (y.dot(left) < 0.0)
            });
            let material = match layered {
                Some((a, _)) => self.core_material(a),
                None => self.material_for(id),
            };
            let eid = self.new_id(e);
            self.walls.insert(
                id,
                HostWall {
                    id: eid,
                    start: w.start,
                    end: w.end,
                    thickness: w.thickness,
                    base_z: level_elev + w.base_offset,
                },
            );
            self.wall_elems.push(Element {
                source: None,
                id: eid,
                name: Self::name(e, "Wall"),
                kind: ElementKind::Wall(Wall {
                    material,
                    assembly: layered.map(|l| l.0),
                    flip,
                    ..w
                }),
            });
        } else {
            self.import_free_mesh(id);
        }
    }

    fn native_wall(&mut self, id: u64, e: &Entity, world: &DMat4) -> Option<Wall> {
        let prism = self.prism(e, world)?;
        if prism.holes {
            return None;
        }
        if prism.clipped {
            self.g
                .warnings
                .add("a wall's clipping was ignored to keep it a native wall");
        }
        let axis = self.axis(e, world);
        let fp = &prism.footprint;
        let (start, end, thickness) = match wall_from_footprint(fp, axis) {
            Some(w) => w,
            None => {
                // stepped ends: the layer set usage still gives the faces
                let (a, b) = axis?;
                let (t, centre) = self.layer_band(id)?;
                let n = world.y_axis.truncate().truncate().normalize_or_zero();
                let d = (b - a).normalize_or_zero();
                if n == DVec2::ZERO || d.perp().dot(n).abs() < 0.999 {
                    return None;
                }
                let (lo, hi) = (centre - t / 2.0, centre + t / 2.0);
                let inside = fp.iter().all(|p| {
                    let s = (*p - a).dot(n);
                    s > lo - 1e-3 && s < hi + 1e-3
                });
                if !inside {
                    return None;
                }
                (a + n * centre, b + n * centre, t)
            }
        };
        let (lid, elev) = self.level_for(id, prism.z0);
        Some(Wall {
            level: lid,
            start,
            end,
            thickness,
            height: prism.z1 - prism.z0,
            base_offset: prism.z0 - elev,
            material: None,
            assembly: None,
            flip: false,
        })
    }

    fn import_slab(&mut self, id: u64) {
        let Some(e) = self.f.get(id) else { return };
        let world = self.g.placement(e.arg(5));
        let native = self.prism(e, &world).filter(|p| !p.holes && !p.clipped);
        let level = native.as_ref().and_then(|p| {
            // the slab's own storey if its top is there, else any storey at its top
            let own = self.storey_of(id).map(|s| (s.id, s.elevation));
            own.filter(|(_, z)| (z - p.z1).abs() < 0.01).or_else(|| {
                self.levels
                    .iter()
                    .find(|(z, _)| (z - p.z1).abs() < 0.01)
                    .map(|&(z, l)| (l, z))
            })
        });
        let (Some(p), Some((lid, _))) = (native, level) else {
            self.import_free_mesh(id);
            return;
        };
        if self.voided.contains(&id) {
            self.g
                .warnings
                .add("openings in slabs are not cut (CreaBIM slabs have no holes yet)");
        }
        self.done.insert(id);
        let layered = self.assembly_for(id, creabim_core::AssemblyUse::Floor);
        let material = match layered {
            Some((a, _)) => self.core_material(a),
            None => self.material_for(id),
        };
        let eid = self.new_id(e);
        self.wall_elems.push(Element {
            source: None,
            id: eid,
            name: Self::name(e, "Slab"),
            kind: ElementKind::Slab(Slab {
                level: lid,
                boundary: p.footprint,
                thickness: p.z1 - p.z0,
                material,
                top_offset: 0.0,
                bulges: vec![],
                assembly: layered.map(|l| l.0),
            }),
        });
    }

    fn import_filling(&mut self, id: u64) {
        let Some(e) = self.f.get(id) else { return };
        let kind = if e.class.starts_with("IFCDOOR") {
            OpeningKind::Door
        } else {
            OpeningKind::Window
        };
        let world = self.g.placement(e.arg(5));
        let opening = self.fills.get(&id).copied();
        let host = opening.and_then(|o| self.voids.get(&o)).copied();
        let filled = host
            .and_then(|h| self.walls.get(&h).copied())
            .and_then(|wall| {
                // the hole is what the wall sees, so measure the opening element
                let o = self.f.get(opening?)?;
                let ow = self.g.placement(o.arg(5));
                let mut mesh = self.g.product_mesh(o, &ow);
                if mesh.faces.is_empty() {
                    mesh = self.g.product_mesh(e, &world);
                }
                let r = opening_in_wall(&wall, &mesh, None)?;
                Some((wall, r))
            });
        let placed = filled.or_else(|| {
            // no opening relationship: find a wall the door's body sits in
            let mesh = self.g.product_mesh(e, &world);
            let (lo, hi) = bounds(mesh.vertices.iter().copied())?;
            let c = (lo + hi) / 2.0;
            let overall = (self.g.length(e.arg(9)), self.g.length(e.arg(8)));
            let wall = self
                .walls
                .values()
                .filter(|w| {
                    let d = (w.end - w.start).normalize_or_zero();
                    let u = (c.truncate() - w.start).dot(d);
                    let s = (c.truncate() - w.start).perp_dot(d).abs();
                    s <= w.thickness / 2.0 + 0.05
                        && u >= 0.0
                        && u <= w.start.distance(w.end)
                        && c.z >= w.base_z - 0.01
                })
                .min_by(|a, b| {
                    let s = |w: &HostWall| {
                        let d = (w.end - w.start).normalize_or_zero();
                        (c.truncate() - w.start).perp_dot(d).abs()
                    };
                    s(a).total_cmp(&s(b))
                })
                .copied()?;
            let r = opening_in_wall(&wall, &mesh, Some(overall))?;
            Some((wall, r))
        });
        let Some((wall, (offset, sill, width, height))) = placed else {
            self.import_free_mesh(id);
            return;
        };
        if offset < -0.01 || sill < -0.01 {
            self.g
                .warnings
                .add("a door or window reaching past its wall's start or base was clipped to it");
        }
        self.done.insert(id);
        let material = self.material_for(id);
        let e = self.f.get(id).expect("checked above");
        let eid = self.new_id(e);
        let fallback = if kind == OpeningKind::Door {
            "Door"
        } else {
            "Window"
        };
        self.opening_elems.push(Element {
            source: None,
            id: eid,
            name: Self::name(e, fallback),
            kind: ElementKind::Opening(Opening {
                host: wall.id,
                kind,
                offset: offset.max(0.0),
                sill: sill.max(0.0),
                width,
                height,
                material,
            }),
        });
    }

    fn import_free_mesh(&mut self, id: u64) {
        if !self.done.insert(id) {
            return;
        }
        let Some(e) = self.f.get(id) else { return };
        let class = product_class(&e.class).unwrap_or("IfcBuildingElementProxy");
        let world = self.g.placement(e.arg(5));
        let mut mesh = self.g.product_mesh(e, &world);
        if mesh.faces.is_empty() {
            let has_parts = self.has_parts.contains(&id);
            if !has_parts && !e.arg(6).is_null() && class != "IfcSite" {
                self.g.warnings.add(format!(
                    "{class} {:?} has no geometry CreaBIM can read",
                    Self::name(e, "")
                ));
            }
            return;
        }
        weld(&mut mesh, 1e-6);
        merge_coplanar(&mut mesh);
        let Some((lo, _)) = bounds(mesh.vertices.iter().copied()) else {
            return;
        };
        let (lid, elev) = self.level_for(id, lo.z);
        // vertices relative to the element's own placement, as authored
        let at = world.w_axis.truncate();
        for v in &mut mesh.vertices {
            *v -= at;
        }
        if class.starts_with("IfcWall") {
            self.g
                .warnings
                .add("walls that are not straight vertical prisms were imported as meshes");
        } else if class.starts_with("IfcSlab") {
            self.g.warnings.add(
                "slabs that are not horizontal prisms topped at a storey were imported as meshes",
            );
        }
        if self.voided.contains(&id) {
            self.g
                .warnings
                .add("openings in elements imported as meshes are not cut");
        }
        let material = self.material_for(id);
        let eid = self.new_id(e);
        self.other_elems.push(Element {
            source: None,
            id: eid,
            name: Self::name(e, class),
            kind: ElementKind::FreeMesh(FreeMesh {
                ifc_class: class.to_string(),
                material,
                ..FreeMesh::new(
                    Some(lid),
                    DVec3::new(at.x, at.y, at.z - elev),
                    EditMesh {
                        vertices: mesh.vertices,
                        faces: mesh.faces,
                    },
                )
            }),
        });
    }
}

/// Recognise a vertical prism in a closed mesh: every vertex at the bottom or
/// the top, and the top faces outlining one loop.
pub(crate) fn prism_of_mesh(mut mesh: EditMesh) -> Option<Prism> {
    weld(&mut mesh, 1e-6);
    let (lo, hi) = bounds(mesh.vertices.iter().copied())?;
    let (z0, z1) = (lo.z, hi.z);
    if z1 - z0 < 1e-4
        || mesh
            .vertices
            .iter()
            .any(|v| (v.z - z0).abs() > 1e-5 && (v.z - z1).abs() > 1e-5)
    {
        return None;
    }
    // the outline of the top faces must be one loop
    let mut edges: HashMap<(u32, u32), i32> = HashMap::new();
    for f in &mesh.faces {
        if f.iter()
            .all(|&v| (mesh.vertices[v as usize].z - z1).abs() <= 1e-5)
        {
            for i in 0..f.len() {
                let (a, b) = (f[i], f[(i + 1) % f.len()]);
                if edges.remove(&(b, a)).is_none() {
                    *edges.entry((a, b)).or_insert(0) += 1;
                }
            }
        }
    }
    let next: HashMap<u32, u32> = edges.keys().map(|&(a, b)| (a, b)).collect();
    if next.len() != edges.len() || next.len() < 3 {
        return None;
    }
    let start = *next.keys().min()?;
    let mut lp = vec![start];
    let mut cur = start;
    while let Some(&n) = next.get(&cur) {
        if n == start {
            break;
        }
        if lp.len() > next.len() {
            return None;
        }
        lp.push(n);
        cur = n;
    }
    if lp.len() != next.len() {
        return None; // several loops: holes or separate pieces
    }
    let mut plan: Vec<DVec2> = lp
        .iter()
        .map(|&v| mesh.vertices[v as usize].truncate())
        .collect();
    if area2(&plan) < 0.0 {
        plan.reverse();
    }
    let volume = area2(&plan) / 2.0 * (z1 - z0);
    if (mesh.volume().abs() - volume).abs() > 1e-3 * volume.max(1e-6) {
        return None;
    }
    Some(Prism {
        footprint: remove_collinear(&plan),
        z0,
        z1,
        holes: false,
        clipped: false,
    })
}

/// A straight wall's centreline and thickness from its plan footprint: all
/// corners must lie on two lines parallel to the wall, whatever its ends do
/// (square, mitred or trimmed at a T). The axis, when given, sets the
/// direction and the ends.
pub(crate) fn wall_from_footprint(
    fp: &[DVec2],
    axis: Option<(DVec2, DVec2)>,
) -> Option<(DVec2, DVec2, f64)> {
    if fp.len() < 4 {
        return None;
    }
    let (origin, d) = match axis {
        Some((a, b)) if a.distance(b) > 1e-6 => (a, (b - a).normalize()),
        _ => {
            let (i, _) = (0..fp.len())
                .map(|i| (i, fp[i].distance(fp[(i + 1) % fp.len()])))
                .max_by(|a, b| a.1.total_cmp(&b.1))?;
            (fp[i], (fp[(i + 1) % fp.len()] - fp[i]).normalize_or_zero())
        }
    };
    if d == DVec2::ZERO {
        return None;
    }
    let n = d.perp();
    let s: Vec<f64> = fp.iter().map(|p| (*p - origin).dot(n)).collect();
    let smin = s.iter().copied().fold(f64::INFINITY, f64::min);
    let smax = s.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let tol = 1e-3;
    let thickness = smax - smin;
    if thickness < 2.0 * tol
        || s.iter()
            .any(|&v| (v - smin).abs() > tol && (v - smax).abs() > tol)
    {
        return None;
    }
    let centre = (smin + smax) / 2.0;
    if let Some((a, b)) = axis.filter(|(a, b)| a.distance(*b) > 1e-6) {
        return Some((a + n * centre, b + n * centre, thickness));
    }
    // the centreline ends halfway between the two faces' ends, which is
    // exact for square and mitred ends
    let mut ranges = [(f64::INFINITY, f64::NEG_INFINITY); 2];
    for (p, &sv) in fp.iter().zip(&s) {
        let k = usize::from((sv - smin).abs() > tol);
        let u = (*p - origin).dot(d);
        ranges[k] = (ranges[k].0.min(u), ranges[k].1.max(u));
    }
    let u0 = (ranges[0].0 + ranges[1].0) / 2.0;
    let u1 = (ranges[0].1 + ranges[1].1) / 2.0;
    if u1 - u0 <= 1e-3 {
        return None;
    }
    let base = origin + n * centre;
    Some((base + d * u0, base + d * u1, thickness))
}

/// Offset along the wall, sill above its base, width and height of a hole
/// measured from a mesh. With `overall` (width, height), those sizes win and
/// the opening is centred on the mesh.
pub(crate) fn opening_in_wall(
    wall: &HostWall,
    mesh: &EditMesh,
    overall: Option<(Option<f64>, Option<f64>)>,
) -> Option<(f64, f64, f64, f64)> {
    let d = (wall.end - wall.start).normalize_or_zero();
    if d == DVec2::ZERO {
        return None;
    }
    let mut u = (f64::INFINITY, f64::NEG_INFINITY);
    let mut z = (f64::INFINITY, f64::NEG_INFINITY);
    for v in &mesh.vertices {
        let t = (v.truncate() - wall.start).dot(d);
        u = (u.0.min(t), u.1.max(t));
        z = (z.0.min(v.z), z.1.max(v.z));
    }
    if !(u.1 - u.0 > 1e-3 && z.1 - z.0 > 1e-3) {
        return None;
    }
    let (mut width, mut height) = (u.1 - u.0, z.1 - z.0);
    let (mut offset, sill) = (u.0, z.0 - wall.base_z);
    if let Some((w, h)) = overall {
        if let Some(w) = w.filter(|w| *w > 0.0) {
            offset += (width - w) / 2.0;
            width = w;
        }
        if let Some(h) = h.filter(|h| *h > 0.0) {
            height = h;
        }
    }
    Some((offset, sill, width, height))
}

/// Drop vertices that lie on the line through their neighbours.
fn remove_collinear(poly: &[DVec2]) -> Vec<DVec2> {
    let n = poly.len();
    let keep: Vec<DVec2> = (0..n)
        .filter(|&i| {
            let (a, b, c) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
            (b - a).perp_dot(c - b).abs() > 1e-9 * (1.0 + (c - a).length_squared())
        })
        .map(|i| poly[i])
        .collect();
    if keep.len() >= 3 {
        keep
    } else {
        poly.to_vec()
    }
}

/// Metres per length unit and radians per angle unit, from the project's units.
fn units(f: &StepFile, by_class: &HashMap<&str, Vec<u64>>) -> (f64, f64, Option<String>) {
    let mut len = None;
    let mut angle = None;
    let project = by_class
        .get("IFCPROJECT")
        .and_then(|v| v.first())
        .and_then(|id| f.get(*id));
    let assignment = project.and_then(|p| f.deref(p.arg(8)));
    for u in assignment.map(|a| a.arg(0).as_list()).unwrap_or(&[]) {
        let Some(e) = f.deref(u) else { continue };
        match (e.arg(1).as_enum(), unit_factor(f, e, 0)) {
            (Some("LENGTHUNIT"), Some(k)) => len = len.or(Some(k)),
            (Some("PLANEANGLEUNIT"), Some(k)) => angle = angle.or(Some(k)),
            _ => {}
        }
    }
    let note = len
        .is_none()
        .then(|| "the file declares no length unit; metres were assumed".to_string());
    (len.unwrap_or(1.0), angle.unwrap_or(1.0), note)
}

/// The size of a unit in SI base units (metres, radians).
fn unit_factor(f: &StepFile, e: &Entity, depth: usize) -> Option<f64> {
    if depth > 8 {
        return None;
    }
    match e.class.as_str() {
        "IFCSIUNIT" => {
            let prefix = match e.arg(2).as_enum() {
                None => 1.0,
                Some("EXA") => 1e18,
                Some("PETA") => 1e15,
                Some("TERA") => 1e12,
                Some("GIGA") => 1e9,
                Some("MEGA") => 1e6,
                Some("KILO") => 1e3,
                Some("HECTO") => 1e2,
                Some("DECA") => 1e1,
                Some("DECI") => 1e-1,
                Some("CENTI") => 1e-2,
                Some("MILLI") => 1e-3,
                Some("MICRO") => 1e-6,
                Some("NANO") => 1e-9,
                Some("PICO") => 1e-12,
                Some(_) => 1.0,
            };
            Some(prefix)
        }
        "IFCCONVERSIONBASEDUNIT" | "IFCCONVERSIONBASEDUNITWITHOFFSET" => {
            let m = f.deref(e.arg(3))?;
            let value = m.arg(0).as_f64()?;
            let base = f
                .deref(m.arg(1))
                .and_then(|u| unit_factor(f, u, depth + 1))
                .unwrap_or(1.0);
            let k = value * base;
            (k.is_finite() && k > 0.0).then_some(k)
        }
        _ => None,
    }
}

/// How a surface style says a surface looks: its colour, and when the
/// style is a full rendering, how see-through, rough and metallic it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Surface {
    pub color: [f32; 3],
    /// 0 opaque .. 1 invisible.
    pub transparency: f32,
    pub roughness: Option<f32>,
    pub metallic: Option<f32>,
}

impl Surface {
    /// The material with this look, and whether the style said anything
    /// about roughness or metal (else the caller guesses from the name).
    pub fn apply(&self, m: &mut Material) -> bool {
        m.color = self.color;
        m.opacity = (1.0 - self.transparency).clamp(0.05, 1.0);
        if let Some(r) = self.roughness {
            m.roughness = r.clamp(0.0, 1.0);
        }
        if let Some(x) = self.metallic {
            m.metallic = x.clamp(0.0, 1.0);
        }
        self.roughness.is_some() || self.metallic.is_some()
    }
}

/// The surface of a list of presentation styles (IfcSurfaceStyleShading
/// or IfcSurfaceStyleRendering, read through style assignments).
///
/// A rendering's `SpecularHighlight` is used when it is an
/// `IfcSpecularRoughness`; `IfcSpecularExponent` is ignored, since some
/// exporters write the same exponent for every material. A `METAL`
/// reflectance method makes the surface metallic.
pub(crate) fn style_surface(f: &StepFile, styles: &Value) -> Option<Surface> {
    for s in styles.as_list() {
        let Some(e) = f.deref(s) else { continue };
        let found = match e.class.as_str() {
            "IFCPRESENTATIONSTYLEASSIGNMENT" => style_surface(f, e.arg(0)),
            "IFCSURFACESTYLE" => e.arg(2).as_list().iter().find_map(|r| {
                let r = f.deref(r)?;
                let rendering = r.class.starts_with("IFCSURFACESTYLERENDERING");
                if !r.class.starts_with("IFCSURFACESTYLESHADING") && !rendering {
                    return None;
                }
                let c = f.deref(r.arg(0))?;
                let ch = |i: usize| c.arg(i).as_f64().map(|v| v.clamp(0.0, 1.0) as f32);
                let mut out = Surface {
                    color: [ch(1)?, ch(2)?, ch(3)?],
                    transparency: r
                        .arg(1)
                        .as_f64()
                        .filter(|t| t.is_finite())
                        .map_or(0.0, |t| t.clamp(0.0, 1.0) as f32),
                    roughness: None,
                    metallic: None,
                };
                if rendering {
                    if let Value::Typed(t, v) = r.arg(7) {
                        if t == "IFCSPECULARROUGHNESS" {
                            out.roughness = v.as_f64().map(|v| v.clamp(0.0, 1.0) as f32);
                        }
                    }
                    match r.arg(8).as_enum() {
                        Some("METAL") => out.metallic = Some(1.0),
                        Some("GLASS" | "MIRROR") => {
                            out.roughness = out.roughness.or(Some(0.05));
                            out.metallic = Some(0.0);
                        }
                        Some("MATT") => out.roughness = out.roughness.or(Some(1.0)),
                        Some("PLASTIC") => out.roughness = out.roughness.or(Some(0.4)),
                        _ => {}
                    }
                }
                Some(out)
            }),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// A CreaBIM material from an IfcMaterial (or a layer set's name and its
/// main IfcMaterial) and the surface style found for it.
fn material_from(
    f: &StepFile,
    name: &str,
    mat_id: Option<u64>,
    surface: Option<Surface>,
) -> Material {
    let (description, category) = mat_id
        .and_then(|m| f.get(m))
        .filter(|m| m.is("IFCMATERIAL"))
        .map(|m| {
            (
                m.arg(1).as_str().unwrap_or(""),
                m.arg(2).as_str().unwrap_or(""),
            )
        })
        .unwrap_or(("", ""));
    let (_, plain) = split_description(description);
    let mut m = Material {
        hatch: hatch_for(name, description, category),
        color: [0.75, 0.75, 0.75],
        category: category.trim().to_lowercase(),
        description: plain,
        ..Default::default()
    };
    let has_surface = surface.is_some_and(|s| s.apply(&mut m));
    creabim_core::library::imported(name, m, has_surface)
}

/// The hatch CreaBIM wrote at the start of a material's description, and
/// the rest of the description.
pub(crate) fn split_description(description: &str) -> (Option<Hatch>, String) {
    let Some(rest) = description.strip_prefix(crate::export::HATCH_PREFIX) else {
        return (None, description.trim().to_string());
    };
    let (label, plain) = rest.split_once(". ").unwrap_or((rest, ""));
    let hatch = Hatch::ALL.into_iter().find(|h| h.label() == label.trim());
    (hatch, plain.trim().to_string())
}

/// The hatch CreaBIM wrote, or a guess from the material's name and category.
pub(crate) fn hatch_for(name: &str, description: &str, category: &str) -> Hatch {
    if let (Some(h), _) = split_description(description) {
        return h;
    }
    let text = format!("{name} {category}").to_lowercase();
    if ["brick", "masonry", "block"]
        .iter()
        .any(|k| text.contains(k))
    {
        Hatch::Brick
    } else if ["concrete", "beton", "cement"]
        .iter()
        .any(|k| text.contains(k))
    {
        Hatch::Concrete
    } else if ["wood", "timber", "oak", "pine", "lumber"]
        .iter()
        .any(|k| text.contains(k))
    {
        Hatch::Timber
    } else if ["insulation", "gypsum", "plaster", "stud"]
        .iter()
        .any(|k| text.contains(k))
    {
        Hatch::Cross
    } else {
        Hatch::Solid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mitred_footprint_gives_the_centreline() {
        // a 10 m wall, 0.3 thick, mitred at both ends as at two L corners
        let fp = [
            DVec2::new(-0.15, 0.15),
            DVec2::new(10.15, 0.15),
            DVec2::new(9.85, -0.15),
            DVec2::new(0.15, -0.15),
        ];
        let (a, b, t) = wall_from_footprint(&fp, None).unwrap();
        assert!((t - 0.3).abs() < 1e-9);
        let (a, b) = if a.x < b.x { (a, b) } else { (b, a) };
        assert!(a.distance(DVec2::ZERO) < 1e-9, "{a}");
        assert!(b.distance(DVec2::new(10.0, 0.0)) < 1e-9, "{b}");
        // an axis on the outer face moves to the centre
        let (a, b, _) =
            wall_from_footprint(&fp, Some((DVec2::new(0.0, 0.15), DVec2::new(10.0, 0.15))))
                .unwrap();
        assert!(a.distance(DVec2::ZERO) < 1e-9 && b.distance(DVec2::new(10.0, 0.0)) < 1e-9);
        // an L-shaped footprint is not a wall
        let l = [
            DVec2::new(0.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(2.0, 1.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 2.0),
            DVec2::new(0.0, 2.0),
        ];
        assert!(wall_from_footprint(&l, None).is_none());
    }

    #[test]
    fn hatches_round_trip_and_are_guessed() {
        let d = format!("{}Diagonal", crate::export::HATCH_PREFIX);
        assert_eq!(hatch_for("x", &d, ""), Hatch::Diagonal);
        assert_eq!(hatch_for("Concrete C30/37", "", ""), Hatch::Concrete);
        assert_eq!(hatch_for("Paint", "", ""), Hatch::Solid);
    }
}
