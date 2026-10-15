//! FreeCAD files both ways. Reading: every visible object with a shape, in
//! metres, its BIM type (`IfcType`, or the Arch type) asking for a wall,
//! slab, door or window. Writing: every element as a solid with its IFC
//! type, grouped by kind, carrying its CreaBIM id so a file saved in
//! FreeCAD updates the same elements when it comes back.

use crate::{Assign, ExchangeError, Incoming};
use creabim_core::{EditMesh, ElementId, ElementKind, Model, OpeningKind};
use creabim_freecad::{FcExport, FcExportGroup, FcExportObject, FcMesh, IDENTITY};
use glam::DVec3;
use std::collections::HashMap;
use std::path::Path;

const ID_PROPERTY: &str = "CreaBIM_Id";

pub fn read_file(path: &Path) -> Result<Vec<Incoming>, ExchangeError> {
    let bytes = std::fs::read(path).map_err(|e| ExchangeError::Other(e.to_string()))?;
    let doc = creabim_freecad::read(&bytes).map_err(|e| ExchangeError::Other(e.to_string()))?;
    Ok(doc
        .objects
        .iter()
        .filter(|o| o.visible)
        .filter_map(|o| {
            let m = o.mesh.as_ref()?;
            if m.faces.is_empty() {
                return None;
            }
            let hint = o
                .ifc_type
                .as_deref()
                .or(o.proxy_type.as_deref())
                .filter(|t| !t.is_empty() && *t != "Undefined")
                .and_then(Assign::parse);
            let element = o
                .property(ID_PROPERTY)
                .and_then(|v| uuid::Uuid::parse_str(v.trim()).ok())
                .map(ElementId);
            let material = o.color.map(|c| {
                let s = c.map(to_srgb);
                (
                    format!(
                        "FreeCAD #{:02x}{:02x}{:02x}",
                        (s[0] * 255.0) as u8,
                        (s[1] * 255.0) as u8,
                        (s[2] * 255.0) as u8
                    ),
                    [s[0], s[1], s[2]],
                )
            });
            Some(Incoming {
                object: o.name.clone(),
                name: o.label.clone(),
                mesh: EditMesh {
                    vertices: m
                        .vertices
                        .iter()
                        .map(|v| DVec3::from_array(*v) * 0.001)
                        .collect(),
                    faces: m.faces.clone(),
                },
                hint,
                element,
                material,
            })
        })
        .collect())
}

fn to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// The model as a FreeCAD document.
pub fn export_fcstd(model: &Model, label: &str) -> Result<Vec<u8>, ExchangeError> {
    let scene = creabim_geom::scene::drawn_elements(model);
    // an element drawn in parts (a window's frame and glass, a wall's
    // layers) is one object
    let whole = creabim_geom::scene::element_meshes(&scene);
    let mut seen = std::collections::HashSet::new();
    let mut groups: Vec<FcExportGroup> = Vec::new();
    let mut group_of: HashMap<&'static str, usize> = HashMap::new();
    let mut objects = Vec::new();
    for item in &scene {
        let Some(e) = model.get(item.id) else {
            continue;
        };
        if !seen.insert(item.id) {
            continue;
        }
        let item_mesh = &whole[&item.id];
        let (group, ifc_type) = match &e.kind {
            ElementKind::Wall(_) => ("Walls", "Wall".to_string()),
            ElementKind::Slab(_) => ("Slabs", "Slab".to_string()),
            ElementKind::Opening(o) => match o.kind {
                OpeningKind::Door => ("Doors", "Door".to_string()),
                OpeningKind::Window => ("Windows", "Window".to_string()),
            },
            ElementKind::FreeMesh(f) => {
                ("Other", f.ifc_class.trim_start_matches("Ifc").to_string())
            }
            _ => ("Other", e.ifc_class().trim_start_matches("Ifc").to_string()),
        };
        let g = *group_of.entry(group).or_insert_with(|| {
            groups.push(FcExportGroup {
                label: group.into(),
                parent: None,
            });
            groups.len() - 1
        });
        // weld the triangles so each element is one closed solid
        let mut ids: HashMap<(i64, i64, i64), u32> = HashMap::new();
        let mut mesh = FcMesh::default();
        let mut tri = Vec::new();
        for &i in &item_mesh.indices {
            let p = item_mesh.positions[i as usize];
            let mm = [
                p[0] as f64 * 1000.0,
                p[1] as f64 * 1000.0,
                p[2] as f64 * 1000.0,
            ];
            let key = (
                mm[0].round() as i64,
                mm[1].round() as i64,
                mm[2].round() as i64,
            );
            let n = *ids.entry(key).or_insert_with(|| {
                mesh.vertices.push(mm);
                mesh.vertices.len() as u32 - 1
            });
            tri.push(n);
            if tri.len() == 3 {
                if tri[0] != tri[1] && tri[1] != tri[2] && tri[0] != tri[2] {
                    mesh.faces.push(std::mem::take(&mut tri));
                }
                tri.clear();
            }
        }
        if mesh.faces.is_empty() {
            continue;
        }
        let c = creabim_core::material_of(model, e).map_or(item.color, |(_, m)| m.color);
        objects.push(FcExportObject {
            label: e.name.clone(),
            mesh,
            placement: IDENTITY,
            color: Some([to_linear(c[0]), to_linear(c[1]), to_linear(c[2]), 1.0]),
            ifc_type: Some(ifc_type),
            group: Some(g),
            visible: true,
            properties: vec![(ID_PROPERTY.into(), e.id.0.to_string())],
        });
    }
    creabim_freecad::write(&FcExport {
        label: label.into(),
        groups,
        objects,
    })
    .map_err(|e| ExchangeError::Other(e.to_string()))
}
