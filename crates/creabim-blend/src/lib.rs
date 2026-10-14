//! Native Blender `.blend` reader for CreaBIM.
//!
//! A `.blend` file is a memory dump: a short header (pointer size,
//! endianness, version), then blocks ("BHeads") that each hold one or more
//! structs exactly as they were in RAM, tagged with the address they had.
//! The `DNA1` block (SDNA) describes every struct the file uses, field by
//! field, so this crate looks fields up by name rather than hard-coding
//! offsets, and follows pointers by matching saved addresses to blocks.
//! That is what lets one reader cover Blender 2.8x to 5.x, including the
//! gzip/zstd compression and the 64-bit block headers of Blender 5.
//!
//! The output is a neutral [`BlendScene`] (no CreaBIM types): objects with
//! world matrices, base (unevaluated) mesh data, collections, materials
//! and custom properties. Mapping it to BIM elements is the app's job.

mod container;
mod math;
mod scene;
mod sdna;
mod view;

use std::path::Path;

/// Everything read from one `.blend` file.
#[derive(Debug, Clone, PartialEq)]
pub struct BlendScene {
    pub objects: Vec<BlendObject>,
    pub materials: Vec<BlendMaterial>,
    /// Blender version that saved the file, e.g. `"4.2"`.
    pub version: String,
    /// The scene's `unit.scale_length` (1.0 when absent): one Blender unit
    /// is this many metres in the metric system.
    pub unit_scale: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlendObject {
    /// Object name without the `OB` ID prefix.
    pub name: String,
    /// Name of the object's data block (mesh, light...), without prefix.
    pub data_name: Option<String>,
    pub kind: ObjectKind,
    /// Index into [`BlendScene::objects`].
    pub parent: Option<usize>,
    /// Names of the collections that link this object directly. The
    /// scene's root collection is not listed.
    pub collections: Vec<String>,
    /// Column-major world matrix (`world[col][row]`), in Blender units.
    pub world: [[f64; 4]; 4],
    /// Base mesh data in object space, before modifiers.
    pub mesh: Option<BlendMesh>,
    /// Material per slot, as indices into [`BlendScene::materials`].
    pub material_slots: Vec<Option<usize>>,
    /// ID properties as text; nested groups use dotted names.
    pub custom_props: Vec<(String, String)>,
    /// Hidden in the viewport, either globally or in the view layer.
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectKind {
    Mesh,
    Curve,
    Empty,
    Light,
    Camera,
    /// Any other Blender object type, by name (`"Armature"`, `"Text"`...).
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlendMesh {
    pub vertices: Vec<[f64; 3]>,
    /// Polygons as vertex indices, counter-clockwise seen from outside.
    pub faces: Vec<Vec<u32>>,
    /// Material slot per face, parallel to `faces`.
    pub face_material: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlendMaterial {
    pub name: String,
    /// Viewport display colour (`diffuse_color`), linear RGBA.
    pub color: [f32; 4],
    /// Viewport display roughness (0.4 for a new material).
    pub roughness: f32,
    /// Viewport display metallic.
    pub metallic: f32,
    /// The inputs of the material's Principled BSDF node, when it uses
    /// nodes and has one (what Blender renders with).
    pub principled: Option<Principled>,
}

/// The unconnected input values of a Principled BSDF node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Principled {
    /// Base Color, linear RGBA.
    pub base_color: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
    pub alpha: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum BlendError {
    #[error("not a .blend file")]
    NotBlend,
    #[error("file is truncated")]
    Truncated,
    #[error("malformed .blend file: {0}")]
    Malformed(String),
    #[error("unsupported .blend file: {0}")]
    Unsupported(String),
    #[error("cannot decompress .blend file: {0}")]
    Decompress(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Reads a `.blend` file from memory (plain, gzip or zstd).
pub fn read(bytes: &[u8]) -> Result<BlendScene, BlendError> {
    let raw = container::decompress(bytes)?;
    let file = container::BlendFile::parse(&raw)?;
    Ok(scene::extract(&file))
}

pub fn read_file(path: &Path) -> Result<BlendScene, BlendError> {
    read(&std::fs::read(path)?)
}
