//! Reading the meshes of a glTF 2.0 file (`.glb`, or `.gltf` with its
//! buffers embedded or beside it), as Blender and most modelling tools
//! export them: every triangle primitive of every node in the default
//! scene, with the node transforms applied, as one incoming object each in
//! its material.
//!
//! glTF is Y-up; CreaBIM is Z-up, so a glTF (x, y, z) reads as (x, -z, y).

use crate::{ExchangeError, Incoming};
use creabim_core::{EditMesh, Material};
use glam::{DMat4, DQuat, DVec3};
use serde_json::Value;
use std::path::Path;

fn bad(m: impl Into<String>) -> ExchangeError {
    ExchangeError::Other(format!("glTF: {}", m.into()))
}

/// The objects of a file and its materials by name.
pub type Read = (Vec<Incoming>, Vec<(String, Material)>);

/// The objects of a glTF file, and its materials by name.
pub fn read_file(path: &Path) -> Result<Read, ExchangeError> {
    let bytes = std::fs::read(path).map_err(|e| bad(e.to_string()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    read(&bytes, &|uri| {
        // only files beside the glTF, never paths that climb out of its folder
        let name = Path::new(uri).file_name()?;
        std::fs::read(dir.join(name)).ok()
    })
}

/// [`read_file`] on the bytes of a file, `external` reading a buffer it
/// names by URI.
pub fn read(
    bytes: &[u8],
    external: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Read, ExchangeError> {
    let (json, bin) = if bytes.starts_with(b"glTF") {
        glb_chunks(bytes)?
    } else {
        (bytes, None)
    };
    let doc: Value = serde_json::from_slice(json).map_err(|e| bad(e.to_string()))?;
    let materials = crate::materials::read_gltf(json).map_err(|e| bad(e.to_string()))?;
    // the buffers
    let mut buffers: Vec<Vec<u8>> = vec![];
    for (i, b) in doc["buffers"].as_array().into_iter().flatten().enumerate() {
        let data = match b["uri"].as_str() {
            None if i == 0 => bin.map(<[u8]>::to_vec),
            None => None,
            Some(uri) => match uri.split_once(";base64,") {
                Some((head, data)) if head.starts_with("data:") => base64(data),
                _ => external(uri),
            },
        };
        buffers.push(data.ok_or_else(|| bad(format!("buffer {i} could not be read")))?);
    }
    let r = Reader {
        doc: &doc,
        buffers: &buffers,
    };
    // the nodes of the default scene, depth first
    let scene = doc["scene"].as_u64().unwrap_or(0) as usize;
    let roots: Vec<usize> = match doc["scenes"].get(scene) {
        Some(s) => s["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n.as_u64().map(|n| n as usize))
            .collect(),
        None => (0..doc["nodes"].as_array().map_or(0, Vec::len)).collect(),
    };
    let y_up = DMat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, -1.0, 0.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]);
    let mut out = vec![];
    let mut stack: Vec<(usize, DMat4, usize)> = roots.into_iter().map(|n| (n, y_up, 0)).collect();
    while let Some((n, parent, depth)) = stack.pop() {
        let node = &doc["nodes"][n];
        if node.is_null() || depth > 64 {
            continue;
        }
        let world = parent * local(node);
        for c in node["children"].as_array().into_iter().flatten() {
            if let Some(c) = c.as_u64() {
                stack.push((c as usize, world, depth + 1));
            }
        }
        let Some(m) = node["mesh"].as_u64() else {
            continue;
        };
        let mesh = &doc["meshes"][m as usize];
        let node_name = node["name"]
            .as_str()
            .or(mesh["name"].as_str())
            .unwrap_or("Object")
            .to_string();
        let prims = mesh["primitives"].as_array().cloned().unwrap_or_default();
        for (pi, p) in prims.iter().enumerate() {
            // triangles only (the default mode)
            if p["mode"].as_u64().unwrap_or(4) != 4 {
                continue;
            }
            let Some(pos) = p["attributes"]["POSITION"]
                .as_u64()
                .and_then(|a| r.floats(a as usize, 3))
            else {
                continue;
            };
            let count = pos.len() / 3;
            let indices: Vec<u32> = match p["indices"].as_u64() {
                Some(a) => r.indices(a as usize).unwrap_or_default(),
                None => (0..count as u32).collect(),
            };
            let vertices: Vec<DVec3> = pos
                .chunks(3)
                .map(|v| world.transform_point3(DVec3::new(v[0], v[1], v[2])))
                .collect();
            let flip = world.determinant() < 0.0;
            let faces: Vec<Vec<u32>> = indices
                .as_chunks::<3>()
                .0
                .iter()
                .filter(|t| t.iter().all(|i| (*i as usize) < count))
                .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
                .map(|t| {
                    if flip {
                        vec![t[0], t[2], t[1]]
                    } else {
                        t.to_vec()
                    }
                })
                .collect();
            if faces.is_empty() {
                continue;
            }
            let material = p["material"]
                .as_u64()
                .and_then(|i| materials.get(i as usize))
                .map(|(name, m)| (name.clone(), m.color));
            let object = if prims.len() > 1 {
                format!("{node_name}.{pi}")
            } else {
                node_name.clone()
            };
            out.push(Incoming {
                object: object.clone(),
                name: object,
                mesh: EditMesh { vertices, faces },
                hint: node["extras"]["ifc_class"]
                    .as_str()
                    .and_then(crate::Assign::parse),
                element: None,
                material,
            });
        }
    }
    Ok((out, materials))
}

/// A node's own transform: its matrix, or translation, rotation and scale.
fn local(node: &Value) -> DMat4 {
    let nums = |v: &Value| -> Vec<f64> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .collect()
    };
    let m = nums(&node["matrix"]);
    if m.len() == 16 {
        let mut a = [0.0; 16];
        a.copy_from_slice(&m);
        return DMat4::from_cols_array(&a);
    }
    let t = nums(&node["translation"]);
    let r = nums(&node["rotation"]);
    let s = nums(&node["scale"]);
    let t = if t.len() == 3 {
        DVec3::new(t[0], t[1], t[2])
    } else {
        DVec3::ZERO
    };
    let r = if r.len() == 4 {
        DQuat::from_xyzw(r[0], r[1], r[2], r[3]).normalize()
    } else {
        DQuat::IDENTITY
    };
    let s = if s.len() == 3 {
        DVec3::new(s[0], s[1], s[2])
    } else {
        DVec3::ONE
    };
    DMat4::from_scale_rotation_translation(s, r, t)
}

struct Reader<'a> {
    doc: &'a Value,
    buffers: &'a [Vec<u8>],
}

impl Reader<'_> {
    /// An accessor's bytes, one slice per element, and its component type.
    fn elements(&self, accessor: usize) -> Option<(Vec<&[u8]>, u64)> {
        let a = &self.doc["accessors"][accessor];
        let view = &self.doc["bufferViews"][a["bufferView"].as_u64()? as usize];
        let buf = self.buffers.get(view["buffer"].as_u64()? as usize)?;
        let ctype = a["componentType"].as_u64()?;
        let csize = match ctype {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return None,
        };
        let n = match a["type"].as_str()? {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => return None,
        };
        let size = csize * n;
        let stride = view["byteStride"].as_u64().map_or(size, |s| s as usize);
        let start = view["byteOffset"].as_u64().unwrap_or(0) as usize
            + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let count = a["count"].as_u64()? as usize;
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let at = start + i * stride;
            out.push(buf.get(at..at + size)?);
        }
        Some((out, ctype))
    }

    /// A float accessor of `n` components, flattened.
    fn floats(&self, accessor: usize, n: usize) -> Option<Vec<f64>> {
        let (els, ctype) = self.elements(accessor)?;
        if ctype != 5126 {
            return None;
        }
        let mut out = Vec::with_capacity(els.len() * n);
        for e in els {
            if e.len() < n * 4 {
                return None;
            }
            for k in 0..n {
                let b = &e[k * 4..k * 4 + 4];
                let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64;
                out.push(if v.is_finite() { v } else { 0.0 });
            }
        }
        Some(out)
    }

    fn indices(&self, accessor: usize) -> Option<Vec<u32>> {
        let (els, ctype) = self.elements(accessor)?;
        Some(
            els.iter()
                .map(|b| match ctype {
                    5121 => b[0] as u32,
                    5123 => u16::from_le_bytes([b[0], b[1]]) as u32,
                    _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                })
                .collect(),
        )
    }
}

/// The JSON and binary chunks of a `.glb`.
fn glb_chunks(bytes: &[u8]) -> Result<(&[u8], Option<&[u8]>), ExchangeError> {
    let u32_at = |i: usize| {
        bytes
            .get(i..i + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    if u32_at(4) != Some(2) {
        return Err(bad("only glTF 2.0 binaries are read"));
    }
    let len = u32_at(12).ok_or_else(|| bad("truncated"))?;
    if bytes.get(16..20) != Some(b"JSON") {
        return Err(bad("the first chunk isn't JSON"));
    }
    let json = bytes.get(20..20 + len).ok_or_else(|| bad("truncated"))?;
    let at = 20 + len;
    let bin = match (u32_at(at), bytes.get(at + 4..at + 8)) {
        (Some(n), Some(b"BIN\0")) => bytes.get(at + 8..at + 8 + n),
        _ => None,
    };
    Ok((json, bin))
}

/// Standard base64, as in `data:` URIs.
fn base64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    };
    let clean: Vec<u8> = s
        .bytes()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
        .collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        let mut acc = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            acc |= val(*c)? << (18 - 6 * i);
        }
        let n = chunk.len();
        let bytes = acc.to_be_bytes();
        out.extend_from_slice(&bytes[1..n.max(1)]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("aGk=").unwrap(), b"hi");
        assert_eq!(base64("YWJj").unwrap(), b"abc");
    }

    #[test]
    fn our_own_glb_reads_back_where_it_was() {
        let model = creabim_core::sample::furnished_house();
        let glb = crate::gltf::export_glb(&model);
        let (objects, materials) = read(&glb, &|_| None).unwrap();
        assert!(objects.len() > 30, "{}", objects.len());
        assert!(!materials.is_empty());
        // the sample house's walls stand between 0 and 6 m, and so do they
        let lo = objects
            .iter()
            .flat_map(|o| o.mesh.vertices.iter())
            .fold(DVec3::splat(1e9), |a, v| a.min(*v));
        let hi = objects
            .iter()
            .flat_map(|o| o.mesh.vertices.iter())
            .fold(DVec3::splat(-1e9), |a, v| a.max(*v));
        assert!(lo.z > -0.5 && hi.z < 7.5 && hi.z > 5.9, "{lo} {hi}");
        assert!(lo.x < 0.0 && hi.x > 10.0 && hi.y > 6.0, "{lo} {hi}");
    }
}
