//! glTF 2.0 binary (.glb) export: every element as it shows in 3D, one
//! node each, welded so it edits as one piece in Blender. Each node's
//! extras carry `creabim_id`, `creabim_kind` and `ifc_class`, which Blender
//! keeps as custom properties: saving the .blend then updates the same
//! elements when CreaBIM reloads it.
//!
//! glTF is Y-up; CreaBIM is Z-up, so (x, y, z) is written as (x, z, -y).

use creabim_core::{ElementKind, Model, OpeningKind};
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn export_glb(model: &Model) -> Vec<u8> {
    let scene = creabim_geom::scene::drawn_elements(model);
    let mut bin: Vec<u8> = Vec::new();
    let (mut views, mut accessors, mut meshes, mut nodes) = (vec![], vec![], vec![], vec![]);
    let mut materials: Vec<Value> = Vec::new();
    let mut material_ids: HashMap<String, usize> = HashMap::new();

    for item in &scene {
        let Some(e) = model.get(item.id) else {
            continue;
        };
        // weld corners so faces share them
        let mut ids: HashMap<(i64, i64, i64), u32> = HashMap::new();
        let mut pos: Vec<[f32; 3]> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        for &i in &item.mesh.indices {
            let p = item.mesh.positions[i as usize];
            let y_up = [p[0], p[2], -p[1]];
            let key = (
                (y_up[0] as f64 * 1e5).round() as i64,
                (y_up[1] as f64 * 1e5).round() as i64,
                (y_up[2] as f64 * 1e5).round() as i64,
            );
            let n = *ids.entry(key).or_insert_with(|| {
                pos.push(y_up);
                pos.len() as u32 - 1
            });
            idx.push(n);
        }
        // drop triangles welded flat
        let idx: Vec<u32> = idx
            .chunks(3)
            .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
            .flatten()
            .copied()
            .collect();
        if idx.is_empty() {
            continue;
        }
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let pv = push_view(
            &mut bin,
            &mut views,
            pos.iter().flatten().flat_map(|f| f.to_le_bytes()),
            34962,
        );
        accessors.push(json!({
            "bufferView": pv, "componentType": 5126, "count": pos.len(),
            "type": "VEC3", "min": lo, "max": hi,
        }));
        let pa = accessors.len() - 1;
        let iv = push_view(
            &mut bin,
            &mut views,
            idx.iter().flat_map(|i| i.to_le_bytes()),
            34963,
        );
        accessors.push(json!({
            "bufferView": iv, "componentType": 5125, "count": idx.len(), "type": "SCALAR",
        }));
        let ia = accessors.len() - 1;

        let (mat_name, color, [roughness, metallic, opacity]) =
            match (item.finish, creabim_core::material_of(model, e)) {
                // a window's glass, a door's handle
                (Some(name), _) => (name.to_string(), item.color, item.surface),
                (None, Some((m, mat))) => (m.name.clone(), mat.color, mat.surface()),
                (None, None) => (
                    default_material(&e.kind).to_string(),
                    item.color,
                    [0.8, 0.0, 1.0],
                ),
            };
        let mi = *material_ids.entry(mat_name.clone()).or_insert_with(|| {
            let lin = color.map(|c| {
                if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            });
            let mut m = json!({
                "name": mat_name,
                "pbrMetallicRoughness": {
                    "baseColorFactor": [lin[0], lin[1], lin[2], opacity],
                    "metallicFactor": metallic, "roughnessFactor": roughness,
                },
            });
            if opacity < 1.0 {
                m["alphaMode"] = json!("BLEND");
            }
            materials.push(m);
            materials.len() - 1
        });
        meshes.push(json!({
            "name": e.name,
            "primitives": [{ "attributes": { "POSITION": pa }, "indices": ia, "material": mi }],
        }));
        let (kind, class) = kind_and_class(e);
        nodes.push(json!({
            "name": e.name,
            "mesh": meshes.len() - 1,
            "extras": { "creabim_id": e.id.0.to_string(), "creabim_kind": kind, "ifc_class": class },
        }));
    }

    let doc = json!({
        "asset": { "version": "2.0", "generator": "CreaBIM" },
        "scene": 0,
        "scenes": [{ "name": "CreaBIM", "nodes": (0..nodes.len()).collect::<Vec<_>>() }],
        "nodes": nodes, "meshes": meshes, "materials": materials,
        "accessors": accessors, "bufferViews": views,
        "buffers": [{ "byteLength": bin.len() }],
    });
    glb(&doc, bin)
}

fn push_view(
    bin: &mut Vec<u8>,
    views: &mut Vec<Value>,
    bytes: impl Iterator<Item = u8>,
    target: u32,
) -> usize {
    let start = bin.len();
    bin.extend(bytes);
    views.push(json!({ "buffer": 0, "byteOffset": start, "byteLength": bin.len() - start, "target": target }));
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    views.len() - 1
}

fn kind_and_class(e: &creabim_core::Element) -> (&'static str, String) {
    match &e.kind {
        ElementKind::Wall(_) => ("wall", "IfcWall".into()),
        ElementKind::Slab(_) => ("slab", "IfcSlab".into()),
        ElementKind::Opening(o) => match o.kind {
            OpeningKind::Door => ("door", "IfcDoor".into()),
            OpeningKind::Window => ("window", "IfcWindow".into()),
        },
        ElementKind::FreeMesh(f) => ("mesh", f.ifc_class.clone()),
        _ => ("mesh", e.ifc_class().to_string()),
    }
}

fn default_material(k: &ElementKind) -> &'static str {
    match k {
        ElementKind::Wall(_) => "Wall",
        ElementKind::Slab(_) => "Slab",
        ElementKind::Opening(o) => match o.kind {
            OpeningKind::Door => "Door",
            OpeningKind::Window => "Glass",
        },
        _ => "Mass",
    }
}

fn glb(doc: &Value, mut bin: Vec<u8>) -> Vec<u8> {
    let mut js = serde_json::to_vec(doc).expect("json");
    while !js.len().is_multiple_of(4) {
        js.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let total = 12 + 8 + js.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((js.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(js);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_house_exports_a_valid_glb_with_ids() {
        let m = creabim_core::sample::sample_house();
        let glb = export_glb(&m);
        assert_eq!(&glb[..4], b"glTF");
        let total = u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize;
        assert_eq!(total, glb.len());
        let jlen = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        let doc: Value = serde_json::from_slice(&glb[20..20 + jlen]).unwrap();
        let nodes = doc["nodes"].as_array().unwrap();
        assert!(nodes.len() > 10);
        let wall = nodes
            .iter()
            .find(|n| n["extras"]["creabim_kind"] == "wall")
            .unwrap();
        let id = wall["extras"]["creabim_id"].as_str().unwrap();
        assert!(uuid::Uuid::parse_str(id).is_ok());
        // every accessor fits in the buffer
        let blen = doc["buffers"][0]["byteLength"].as_u64().unwrap();
        for v in doc["bufferViews"].as_array().unwrap() {
            assert!(v["byteOffset"].as_u64().unwrap() + v["byteLength"].as_u64().unwrap() <= blen);
        }
    }
}
