//! Materials read from other tools' files, as (name, material) pairs ready
//! for [`creabim_core::library::merge`]:
//!
//! - FreeCAD material cards (`.FCMat`), both the INI-like cards of FreeCAD
//!   0.x and the YAML cards of FreeCAD 1.x: name, description, the
//!   diffuse colour, transparency and shininess of their rendering section,
//!   and their "father" (the kind of material) for the category.
//! - glTF (`.gltf`, `.glb`), as Blender exports it: each material's name and
//!   `pbrMetallicRoughness` base colour, roughness and metallic factors,
//!   with its alpha (blended materials) or transmission for opacity.
//! - Blender (`.blend`): each material's Principled BSDF inputs when it
//!   uses nodes, else its viewport display colour, roughness and metallic.
//!
//! Only names and numbers are read; textures are not.

use creabim_core::library::{guess_category, hatch_for_category, imported};
use creabim_core::Material;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum MaterialError {
    #[error("not a FreeCAD material card: {0}")]
    FcMat(String),
    #[error("not a glTF file: {0}")]
    Gltf(String),
    #[error(transparent)]
    Blend(#[from] creabim_blend::BlendError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("CreaBIM can't read materials from .{0} files")]
    Unsupported(String),
}

/// The file kinds this module reads materials from (lower-case extensions).
pub const EXTENSIONS: &[&str] = &["fcmat", "gltf", "glb", "blend"];

/// Read the materials of a FreeCAD card, glTF or Blender file, by extension.
pub fn read_file(path: &Path) -> Result<Vec<(String, Material)>, MaterialError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "fcmat" => {
            let text = std::fs::read_to_string(path)?;
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            Ok(vec![read_fcmat(&text, stem)?])
        }
        "gltf" | "glb" => read_gltf(&std::fs::read(path)?),
        "blend" => Ok(read_blend(&creabim_blend::read_file(path)?)),
        other => Err(MaterialError::Unsupported(other.to_string())),
    }
}

/// sRGB from linear light, for colours glTF and Blender keep linear.
pub fn srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c >= 1.0 {
        1.0
    } else if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// A material with the hatch its category usually has, then filled in by
/// [`imported`].
fn finish(name: &str, mut m: Material, has_surface: bool) -> Material {
    if m.category.is_empty() {
        m.category = guess_category(&format!("{name} {}", m.description)).into();
    }
    m.hatch = hatch_for_category(&m.category);
    imported(name, m, has_surface)
}

// ---- FreeCAD ----

/// Read a FreeCAD material card. `fallback_name` (the file's name) names a
/// card that doesn't name itself.
pub fn read_fcmat(text: &str, fallback_name: &str) -> Result<(String, Material), MaterialError> {
    // (section path, key, value), from either format
    let mut entries: Vec<(String, String, String)> = vec![];
    let mut section = String::new();
    // YAML: the keys open at each indentation
    let mut stack: Vec<(usize, String)> = vec![];
    for raw in text.lines() {
        let line = raw.trim_end();
        let t = line.trim_start();
        if t.is_empty() || t.starts_with(';') || t.starts_with('#') || t == "---" {
            continue;
        }
        if t.starts_with('[') && t.ends_with(']') {
            section = t[1..t.len() - 1].trim().to_string();
            stack.clear();
            continue;
        }
        let indent = line.len() - t.len();
        // INI uses '=', YAML ':'; a YAML value may itself hold '=' or ':'
        let eq = t.find('=');
        let colon = t.find(':');
        let (key, value) = match (eq, colon) {
            (Some(e), Some(c)) if c < e => (&t[..c], &t[c + 1..]),
            (Some(e), _) => (&t[..e], &t[e + 1..]),
            (None, Some(c)) => (&t[..c], &t[c + 1..]),
            (None, None) => continue,
        };
        let key = key.trim().trim_matches('"').to_string();
        let value = unquote(value.trim());
        while stack.last().is_some_and(|(i, _)| *i >= indent) {
            stack.pop();
        }
        let path = if stack.is_empty() {
            section.clone()
        } else {
            stack
                .iter()
                .map(|(_, k)| k.as_str())
                .collect::<Vec<_>>()
                .join(".")
        };
        if value.is_empty() && colon.is_some() && eq.is_none() {
            // a YAML mapping opens
            stack.push((indent, key));
            continue;
        }
        entries.push((path, key, value));
    }
    if entries.is_empty() {
        return Err(MaterialError::FcMat("no entries".into()));
    }
    let get = |key: &str| {
        entries
            .iter()
            .find(|(_, k, v)| k.eq_ignore_ascii_case(key) && !v.is_empty())
            .map(|(_, _, v)| v.as_str())
    };
    let name = entries
        .iter()
        .find(|(s, k, v)| {
            k == "Name" && !v.is_empty() && (s.is_empty() || s == "General" || s == "FCMat")
        })
        .map(|(_, _, v)| v.clone())
        .or_else(|| get("Name").map(str::to_string))
        .unwrap_or_else(|| fallback_name.to_string());
    if name.trim().is_empty() {
        return Err(MaterialError::FcMat("the card has no name".into()));
    }
    let mut m = Material {
        description: get("Description").unwrap_or("").to_string(),
        ..Default::default()
    };
    let mut has_look = false;
    if let Some(c) = get("DiffuseColor").and_then(parse_tuple) {
        if c.len() >= 3 {
            m.color = [c[0], c[1], c[2]];
            has_look = true;
        }
    }
    if let Some(t) = get("Transparency").and_then(parse_number) {
        // 0..1, or a percentage in some cards
        let t = if t > 1.0 { t / 100.0 } else { t };
        m.opacity = 1.0 - t;
    }
    let mut has_surface = false;
    if let Some(r) = get("Roughness").and_then(parse_number) {
        m.roughness = r;
        has_surface = true;
    } else if let Some(s) = get("Shininess").and_then(parse_number) {
        m.roughness = 1.0 - s.clamp(0.0, 1.0);
        has_surface = true;
    }
    if let Some(x) = get("Metallic")
        .or_else(|| get("Metalness"))
        .and_then(parse_number)
    {
        m.metallic = x;
        has_surface = true;
    }
    let father = get("Father")
        .or_else(|| get("KindOfMaterial"))
        .unwrap_or("");
    m.category = guess_category(&format!("{name} {father}")).into();
    if m.category.is_empty() && father.eq_ignore_ascii_case("aggregate") {
        m.category = "concrete".into();
    }
    if !has_look && m.category.is_empty() {
        return Err(MaterialError::FcMat(format!(
            "{name} has neither a colour nor a kind of material"
        )));
    }
    let mut m = finish(&name, m, has_surface);
    if m.category == "steel" || m.category == "aluminium" || m.category == "metal" {
        // FreeCAD cards rarely say so
        if !has_surface || get("Metallic").is_none() {
            m.metallic = 1.0;
        }
    }
    Ok((name, m))
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// `(0.5, 0.5, 0.5, 1.0)` -> numbers.
fn parse_tuple(s: &str) -> Option<Vec<f32>> {
    let inner = s.trim().trim_start_matches('(').trim_end_matches(')');
    let v: Option<Vec<f32>> = inner
        .split(',')
        .map(|p| p.trim().parse::<f32>().ok().filter(|x| x.is_finite()))
        .collect();
    v.filter(|v| !v.is_empty())
}

/// `0.06`, or the number at the start of `25 MPa`.
fn parse_number(s: &str) -> Option<f32> {
    s.split_whitespace()
        .next()?
        .parse::<f32>()
        .ok()
        .filter(|x| x.is_finite())
}

// ---- glTF ----

/// Read the materials of a `.gltf` (JSON) or `.glb` (binary) file.
pub fn read_gltf(bytes: &[u8]) -> Result<Vec<(String, Material)>, MaterialError> {
    let json = if bytes.starts_with(b"glTF") {
        glb_json(bytes)?
    } else {
        bytes
    };
    let doc: Value =
        serde_json::from_slice(json).map_err(|e| MaterialError::Gltf(e.to_string()))?;
    if !doc.get("asset").is_some_and(Value::is_object) {
        return Err(MaterialError::Gltf("no asset description".into()));
    }
    let mut out = vec![];
    for (i, mat) in doc["materials"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let name = mat["name"]
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map_or_else(|| format!("Material {}", i + 1), str::to_string);
        let pbr = &mat["pbrMetallicRoughness"];
        let num = |v: &Value, d: f64| v.as_f64().filter(|x| x.is_finite()).unwrap_or(d) as f32;
        let base: Vec<f32> = pbr["baseColorFactor"]
            .as_array()
            .map(|a| a.iter().map(|v| num(v, 1.0)).collect())
            .unwrap_or_else(|| vec![1.0; 4]);
        let ch = |i: usize| base.get(i).copied().unwrap_or(1.0);
        let alpha = if mat["alphaMode"].as_str() == Some("BLEND") {
            ch(3)
        } else {
            1.0
        };
        let transmission = num(
            &mat["extensions"]["KHR_materials_transmission"]["transmissionFactor"],
            0.0,
        );
        let m = Material {
            color: [srgb(ch(0)), srgb(ch(1)), srgb(ch(2))],
            // glTF's defaults for absent factors are 1
            roughness: num(&pbr["roughnessFactor"], 1.0),
            metallic: num(&pbr["metallicFactor"], 1.0),
            opacity: alpha.min(1.0 - 0.75 * transmission.clamp(0.0, 1.0)),
            ..Default::default()
        };
        out.push((name.clone(), finish(&name, m, true)));
    }
    Ok(out)
}

/// The JSON chunk of a binary glTF.
fn glb_json(bytes: &[u8]) -> Result<&[u8], MaterialError> {
    let u32_at = |i: usize| {
        bytes
            .get(i..i + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let bad = |m: &str| MaterialError::Gltf(m.into());
    if u32_at(4) != Some(2) {
        return Err(bad("only glTF 2.0 binaries are read"));
    }
    let len = u32_at(12).ok_or_else(|| bad("truncated"))?;
    if bytes.get(16..20) != Some(b"JSON") {
        return Err(bad("the first chunk isn't JSON"));
    }
    bytes.get(20..20 + len).ok_or_else(|| bad("truncated"))
}

// ---- Blender ----

/// The materials of a Blender file.
pub fn read_blend(scene: &creabim_blend::BlendScene) -> Vec<(String, Material)> {
    scene
        .materials
        .iter()
        .map(|b| {
            // Blender 5 gives every material nodes; a Principled BSDF left at
            // its defaults says less than a viewport colour that was set
            let default_node = b.principled.is_some_and(|p| {
                p.base_color == [0.8, 0.8, 0.8, 1.0]
                    && p.roughness == 0.5
                    && p.metallic == 0.0
                    && p.alpha == 1.0
            });
            let viewport_set = b.color != [0.8, 0.8, 0.8, 1.0];
            let (color, roughness, metallic, alpha) = match b.principled {
                Some(p) if !(default_node && viewport_set) => {
                    (p.base_color, p.roughness, p.metallic, p.alpha)
                }
                _ => (b.color, b.roughness, b.metallic, b.color[3]),
            };
            let m = Material {
                color: [srgb(color[0]), srgb(color[1]), srgb(color[2])],
                roughness,
                metallic,
                opacity: alpha,
                ..Default::default()
            };
            (b.name.clone(), finish(&b.name, m, true))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::Hatch;

    const FCMAT_OLD: &str = "; Concrete-Generic
; (c) 2013 Yorik van Havre (CC-BY 3.0)
; file created by FreeCAD0.19

[General]
Name = Concrete-Generic
Description = A standard C-25 construction concrete
ReferenceSource =

[Mechanical]
CompressiveStrength = 25 MPa
Density = 2400 kg/m^3

[Rendering]
AmbientColor = (0.0, 0.0, 0.0, 1.0)
DiffuseColor = (0.6, 0.6, 0.55, 1.0)
EmissiveColor = (0.0, 0.0, 0.0, 1.0)
Shininess = 0.1
SpecularColor = (0.1, 0.1, 0.1, 1.0)
Transparency = 0.0
";

    const FCMAT_YAML: &str = "---
# File created by ConvertFCMat.py
General:
  UUID: \"92589471-a6cb-4bbc-b748-d425a17dea7d\"
  Author: \"Yorik van Havre\"
  License: \"CC-BY-3.0\"
  Name: \"Glass-Generic\"
  Description: \"Generic soda-lime glass\"
Models:
  Father:
    UUID: \"9cdda8b6-b606-4778-8f13-3934d8668e67\"
    Father: \"Glass\"
  Density:
    UUID: \"454661e5-265b-4320-8e6f-fcf6223ac3af\"
    Density: \"2500 kg/m^3\"
AppearanceModels:
  BasicRendering:
    UUID: \"f006c7e4-35b7-43d5-bbf9-c5d572309e6e\"
    AmbientColor: \"(0.0, 0.0, 0.0, 1.0)\"
    DiffuseColor: \"(0.8, 0.9, 0.95, 1.0)\"
    Shininess: \"0.9\"
    Transparency: \"0.7\"
";

    #[test]
    fn freecad_cards_old_and_new() {
        let (name, m) = read_fcmat(FCMAT_OLD, "x").unwrap();
        assert_eq!(name, "Concrete-Generic");
        assert_eq!(m.category, "concrete");
        assert_eq!(m.hatch, Hatch::Concrete);
        assert_eq!(m.color, [0.6, 0.6, 0.55]);
        assert!((m.roughness - 0.9).abs() < 1e-6);
        assert_eq!(m.opacity, 1.0);
        assert_eq!(m.description, "A standard C-25 construction concrete");

        let (name, m) = read_fcmat(FCMAT_YAML, "x").unwrap();
        assert_eq!(name, "Glass-Generic");
        assert_eq!(m.category, "glass");
        assert_eq!(m.hatch, Hatch::Empty);
        assert_eq!(m.color, [0.8, 0.9, 0.95]);
        assert!((m.opacity - 0.3).abs() < 1e-6);
        assert!((m.roughness - 0.1).abs() < 1e-6);
        assert_eq!(m.description, "Generic soda-lime glass");

        // a steel card without a colour still says what it is
        let (name, m) = read_fcmat("[FCMat]\nFather = Metal\n", "Steel-S355").unwrap();
        assert_eq!(name, "Steel-S355");
        assert_eq!(m.category, "steel");
        assert_eq!(m.metallic, 1.0);

        assert!(read_fcmat("hello", "x").is_err());
        assert!(read_fcmat("[General]\nName = Unknown\n", "x").is_err());
    }

    fn gltf_json() -> String {
        serde_json::json!({
            "asset": {"version": "2.0", "generator": "Khronos glTF Blender I/O"},
            "materials": [
                {"name": "Oak", "pbrMetallicRoughness": {
                    "baseColorFactor": [0.4, 0.2, 0.08, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.6}},
                {"name": "Brushed steel", "pbrMetallicRoughness": {
                    "baseColorFactor": [0.6, 0.6, 0.6, 1.0], "roughnessFactor": 0.3}},
                {"name": "Window glass", "alphaMode": "BLEND", "pbrMetallicRoughness": {
                    "baseColorFactor": [0.8, 0.9, 0.9, 0.25], "metallicFactor": 0.0, "roughnessFactor": 0.05}},
                {"pbrMetallicRoughness": {"metallicFactor": 0.0}}
            ]
        })
        .to_string()
    }

    fn check_gltf(mats: &[(String, Material)]) {
        assert_eq!(mats.len(), 4);
        let (n, oak) = &mats[0];
        assert_eq!(n, "Oak");
        assert_eq!((oak.category.as_str(), oak.hatch), ("wood", Hatch::Timber));
        assert!((oak.color[0] - srgb(0.4)).abs() < 1e-6);
        assert!((oak.roughness - 0.6).abs() < 1e-6);
        assert_eq!(oak.metallic, 0.0);
        let (_, steel) = &mats[1];
        assert_eq!(steel.metallic, 1.0, "glTF's default metallic is 1");
        assert_eq!(steel.category, "steel");
        let (_, glass) = &mats[2];
        assert!((glass.opacity - 0.25).abs() < 1e-6);
        assert_eq!(glass.category, "glass");
        let (n, plain) = &mats[3];
        assert_eq!(n, "Material 4");
        assert_eq!(plain.color, [1.0, 1.0, 1.0]);
        assert_eq!(plain.roughness, 1.0);
    }

    #[test]
    fn gltf_and_glb_materials() {
        let json = gltf_json();
        check_gltf(&read_gltf(json.as_bytes()).unwrap());
        // the same as a binary glTF
        let mut chunk = json.into_bytes();
        while !chunk.len().is_multiple_of(4) {
            chunk.push(b' ');
        }
        let mut glb = b"glTF".to_vec();
        glb.extend(2u32.to_le_bytes());
        glb.extend(((12 + 8 + chunk.len()) as u32).to_le_bytes());
        glb.extend((chunk.len() as u32).to_le_bytes());
        glb.extend(b"JSON");
        glb.extend(&chunk);
        check_gltf(&read_gltf(&glb).unwrap());
        assert!(read_gltf(b"{}").is_err());
        assert!(read_gltf(b"glTF\x01\0\0\0").is_err());
    }

    #[test]
    fn creabim_gltf_export_reads_back() {
        let model = creabim_core::sample::sample_house();
        let glb = crate::gltf::export_glb(&model);
        let mats = read_gltf(&glb).unwrap();
        let brick = mats.iter().find(|(n, _)| n == "Brick").unwrap();
        let want = creabim_core::library::materials(&model)
            .into_iter()
            .find(|(_, n, _)| *n == "Brick")
            .unwrap()
            .2
            .clone();
        for (a, b) in brick.1.color.iter().zip(want.color) {
            assert!((a - b).abs() < 1e-3);
        }
        assert_eq!(brick.1.roughness, want.roughness);
        assert_eq!(brick.1.category, "brick");
    }

    #[test]
    fn blender_materials() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../creabim-blend/fixtures");
        for file in ["scene_4_2_zstd.blend", "scene_5_2.blend"] {
            let mats = read_file(&dir.join(file)).unwrap();
            assert_eq!(mats.len(), 2, "{file}");
            let concrete = &mats.iter().find(|(n, _)| n == "Concrete").unwrap().1;
            // the viewport colour the generator set, also in Blender 5
            // where the Principled BSDF was left at its defaults
            assert!((concrete.color[2] - srgb(0.75)).abs() < 1e-5, "{file}");
            assert_eq!(concrete.category, "concrete");
            assert_eq!(concrete.hatch, Hatch::Concrete);
            assert!((concrete.roughness - 0.4).abs() < 1e-6, "{file}");
            let wood = &mats.iter().find(|(n, _)| n == "Wood").unwrap().1;
            assert!((wood.opacity - 0.5).abs() < 1e-6, "{file}");
            assert_eq!(wood.hatch, Hatch::Timber);
        }
    }
}
