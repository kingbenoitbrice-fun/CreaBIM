//! Native FreeCAD `.FCStd` import and export.
//!
//! An `.FCStd` file is a zip archive holding `Document.xml` (objects and
//! their properties), `GuiDocument.xml` (colours, visibility) and one file per
//! shape (OpenCascade BREP text, `*.brp`) or mesh (`*.bms`).
//!
//! This crate deliberately knows nothing about CreaBIM's data model: [`read`]
//! returns neutral [`FcObject`]s and [`write`] takes neutral [`FcExport`]
//! data, so the application maps them to its own elements.
//!
//! # Units and coordinates
//! Everything is in FreeCAD's internal units: millimetres, right-handed,
//! Z up. Matrices are 4x4, column-major (`m[column][row]`), translation in
//! `m[3]`.

mod brep;
mod document;
mod export;
mod math;
mod meshkernel;
mod triangulate;

pub use export::write;

/// Errors that make a whole file unreadable. Problems limited to one object
/// (an unsupported shape, a missing file) are reported as warnings instead.
#[derive(Debug, thiserror::Error)]
pub enum FcError {
    #[error("not a valid FCStd archive: {0}")]
    Zip(String),
    #[error("invalid XML in {file}: {message}")]
    Xml { file: String, message: String },
    #[error("missing Document.xml")]
    MissingDocument,
    #[error("BREP: {0}")]
    Brep(String),
    #[error("cannot write FCStd: {0}")]
    Write(String),
}

/// A FreeCAD document as read from an `.FCStd` file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FcDocument {
    /// The document's `Label` property.
    pub label: String,
    /// `ProgramVersion` of the FreeCAD that saved the file, e.g. "1.0R39319 (Git)".
    pub program_version: String,
    /// Objects in document order.
    pub objects: Vec<FcObject>,
    /// Non-fatal problems: skipped shapes, binary BREP, unknown formats...
    pub warnings: Vec<String>,
}

impl FcDocument {
    /// Converts what was read back into something [`write`] accepts:
    /// every object with a mesh becomes a plain `Part::Feature` (keeping
    /// label, placement, colour, visibility, IFC type and properties), and
    /// every object that has members becomes a group. Parametric history and
    /// Python proxies are not preserved.
    pub fn to_export(&self) -> FcExport {
        let mut group_of = vec![None; self.objects.len()];
        let mut groups = Vec::new();
        for (i, o) in self.objects.iter().enumerate() {
            if self.objects.iter().any(|c| c.parent == Some(i)) {
                group_of[i] = Some(groups.len());
                groups.push(FcExportGroup {
                    label: o.label.clone(),
                    parent: None,
                });
            }
        }
        for (i, o) in self.objects.iter().enumerate() {
            if let (Some(g), Some(p)) = (group_of[i], o.parent) {
                groups[g].parent = group_of[p];
            }
        }
        let in_part = |o: &FcObject| {
            let mut p = o.parent;
            let mut steps = 0;
            while let Some(i) = p {
                if self.objects[i].type_id == "App::Part" || steps > self.objects.len() {
                    return true;
                }
                steps += 1;
                p = self.objects[i].parent;
            }
            false
        };
        let objects = self
            .objects
            .iter()
            .filter_map(|o| {
                let mesh = o.mesh.as_ref()?;
                let (mesh, placement) = match o.local_mesh() {
                    Some(local) if !in_part(o) => (local, o.placement),
                    _ => (mesh.clone(), IDENTITY),
                };
                Some(FcExportObject {
                    label: o.label.clone(),
                    mesh,
                    placement,
                    color: o.color,
                    ifc_type: o.ifc_type.clone(),
                    group: o.parent.and_then(|p| group_of[p]),
                    visible: o.visible,
                    properties: o.properties.clone(),
                })
            })
            .collect();
        FcExport {
            label: self.label.clone(),
            groups,
            objects,
        }
    }
}

/// One document object.
#[derive(Clone, Debug, PartialEq)]
pub struct FcObject {
    /// Internal, unique name (e.g. "Wall001"). Links refer to it.
    pub name: String,
    /// User-visible label.
    pub label: String,
    /// C++ type, e.g. "Part::Box", "Part::FeaturePython", "Mesh::Feature".
    pub type_id: String,
    /// Type of the Python proxy of Arch/BIM objects, e.g. "Wall",
    /// "Window", "Structure", "BuildingPart", "Space".
    pub proxy_type: Option<String>,
    /// The `IfcType` property when present, e.g. "Wall", "Building Storey".
    pub ifc_type: Option<String>,
    /// The `Placement` property, column-major, millimetres.
    pub placement: [[f64; 4]; 4],
    /// Tessellated shape in **document (global) coordinates**, millimetres:
    /// the placement (and any enclosing `App::Part`) is already applied.
    /// See [`FcObject::local_mesh`] for object-local coordinates.
    /// `None` for objects without solid geometry (groups, sketches...) and
    /// for Arch `BuildingPart` containers, whose shape only repeats their
    /// children.
    pub mesh: Option<FcMesh>,
    /// Other simple properties as text (`Length`, `Width`, `Height`,
    /// `Material` -> label of the linked object, ...), sorted by name.
    pub properties: Vec<(String, String)>,
    /// Index in [`FcDocument::objects`] of the group, `App::Part` or Arch
    /// `BuildingPart` whose `Group` lists this object.
    pub parent: Option<usize>,
    /// Shape colour from `GuiDocument.xml` as linear RGBA in 0..1 (alpha =
    /// 1 - transparency). `None` when the file has no GUI document.
    pub color: Option<[f32; 4]>,
    pub visible: bool,
}

impl FcObject {
    /// Value of a property from [`FcObject::properties`].
    pub fn property(&self, name: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The mesh in the object's own coordinate system, i.e. with the inverse
    /// of [`FcObject::placement`] applied. Only meaningful for objects that
    /// are not inside an `App::Part`.
    pub fn local_mesh(&self) -> Option<FcMesh> {
        let inv = math::Trsf::from_cols(&self.placement).inverse()?;
        self.mesh.as_ref().map(|m| m.transformed(&inv.to_cols()))
    }
}

/// A polygon mesh. Read meshes contain triangles; written meshes may use any
/// planar polygons.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FcMesh {
    pub vertices: Vec<[f64; 3]>,
    /// Counter-clockwise seen from outside (outward normals for solids).
    pub faces: Vec<Vec<u32>>,
}

impl FcMesh {
    /// Signed volume (divergence theorem); positive for a closed mesh with
    /// outward-facing polygons.
    pub fn volume(&self) -> f64 {
        let mut vol = 0.0;
        for f in &self.faces {
            let Some(p0) = f.first().and_then(|&i| self.vertices.get(i as usize)) else {
                continue;
            };
            let p0 = math::V3::from_array(*p0);
            for w in f[1..].windows(2) {
                let (Some(a), Some(b)) = (
                    self.vertices.get(w[0] as usize),
                    self.vertices.get(w[1] as usize),
                ) else {
                    continue;
                };
                let (a, b) = (math::V3::from_array(*a), math::V3::from_array(*b));
                vol += p0.dot(a.cross(b));
            }
        }
        vol / 6.0
    }

    /// Axis-aligned bounds `(min, max)`, `None` when there are no vertices.
    pub fn bounding_box(&self) -> Option<([f64; 3], [f64; 3])> {
        let first = *self.vertices.first()?;
        let mut lo = first;
        let mut hi = first;
        for v in &self.vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        Some((lo, hi))
    }

    /// True when every edge is shared by exactly two faces in opposite
    /// directions (a watertight, consistently oriented surface).
    pub fn is_closed(&self) -> bool {
        let mut count = std::collections::HashMap::new();
        for f in &self.faces {
            for i in 0..f.len() {
                *count.entry((f[i], f[(i + 1) % f.len()])).or_insert(0u32) += 1;
            }
        }
        !count.is_empty()
            && count
                .iter()
                .all(|(&(a, b), &c)| c == 1 && count.get(&(b, a)) == Some(&1))
    }

    /// The mesh with a column-major 4x4 transform applied to its vertices.
    pub fn transformed(&self, m: &[[f64; 4]; 4]) -> FcMesh {
        let t = math::Trsf::from_cols(m);
        FcMesh {
            vertices: self
                .vertices
                .iter()
                .map(|v| t.point(math::V3::from_array(*v)).to_array())
                .collect(),
            faces: if t.is_mirror() {
                self.faces
                    .iter()
                    .map(|f| f.iter().rev().copied().collect())
                    .collect()
            } else {
                self.faces.clone()
            },
        }
    }
}

/// The identity placement.
pub const IDENTITY: [[f64; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// What to write into a new `.FCStd`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FcExport {
    /// Document label (also the name FreeCAD shows in its tab).
    pub label: String,
    /// `App::DocumentObjectGroup`s, e.g. one per storey.
    pub groups: Vec<FcExportGroup>,
    pub objects: Vec<FcExportObject>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FcExportGroup {
    pub label: String,
    /// Index into [`FcExport::groups`] for nested groups.
    pub parent: Option<usize>,
}

/// One solid or surface, written as a plain `Part::Feature` (no Python
/// proxy, so any FreeCAD opens it without the BIM workbench).
#[derive(Clone, Debug, PartialEq)]
pub struct FcExportObject {
    pub label: String,
    /// Polyhedral geometry in **object-local** coordinates, millimetres; each
    /// face a planar polygon, counter-clockwise seen from outside. A closed
    /// mesh becomes a solid, anything else a shell.
    pub mesh: FcMesh,
    /// Column-major, millimetres. Must be rigid (rotation + translation);
    /// otherwise the transform is baked into the vertices.
    pub placement: [[f64; 4]; 4],
    /// Linear RGBA in 0..1; alpha < 1 becomes FreeCAD transparency.
    pub color: Option<[f32; 4]>,
    /// Stored as a string property `IfcType` (FreeCAD's BIM naming, e.g.
    /// "Wall", "Slab", "Window"), which FreeCAD's IFC exporter honours.
    pub ifc_type: Option<String>,
    /// Index into [`FcExport::groups`].
    pub group: Option<usize>,
    pub visible: bool,
    /// Extra string properties (shown under "CreaBIM" in FreeCAD).
    pub properties: Vec<(String, String)>,
}

impl Default for FcExportObject {
    fn default() -> Self {
        FcExportObject {
            label: String::new(),
            mesh: FcMesh::default(),
            placement: IDENTITY,
            color: None,
            ifc_type: None,
            group: None,
            visible: true,
            properties: Vec::new(),
        }
    }
}

/// Reads an `.FCStd` archive.
pub fn read(bytes: &[u8]) -> Result<FcDocument, FcError> {
    document::read(bytes)
}
