//! Blender files read as incoming objects: every visible mesh object, in
//! metres with its object transform applied. Custom properties steer the
//! import: `creabim_id` names the CreaBIM element an object was exported
//! from, and `creabim_kind` (or `ifc_class`) asks for a wall, slab, door,
//! window or any IFC class. A collection named "Walls", "Slabs", "Doors",
//! "Windows" or after an IFC class does the same for everything in it.

use crate::{Assign, ExchangeError, Incoming};
use creabim_blend::{BlendObject, BlendScene, ObjectKind};
use creabim_core::{EditMesh, ElementId};
use glam::{DMat4, DVec3, DVec4};
use std::path::Path;

pub fn read_file(path: &Path) -> Result<Vec<Incoming>, ExchangeError> {
    Ok(incoming(&creabim_blend::read_file(path)?))
}

pub fn incoming(scene: &BlendScene) -> Vec<Incoming> {
    scene
        .objects
        .iter()
        .filter(|o| o.kind == ObjectKind::Mesh && !o.hidden)
        .filter_map(|o| object(scene, o))
        .collect()
}

fn object(scene: &BlendScene, o: &BlendObject) -> Option<Incoming> {
    let m = o.mesh.as_ref()?;
    if m.faces.is_empty() {
        return None;
    }
    let cols = o.world.map(DVec4::from_array);
    let world = DMat4::from_cols(cols[0], cols[1], cols[2], cols[3]);
    let s = scene.unit_scale;
    let vertices: Vec<DVec3> = m
        .vertices
        .iter()
        .map(|v| world.transform_point3(DVec3::from_array(*v)) * s)
        .collect();
    let mut faces = m.faces.clone();
    // a mirrored object turns its faces inside out
    if world.determinant() < 0.0 {
        for f in &mut faces {
            f.reverse();
        }
    }
    let prop = |keys: &[&str]| {
        o.custom_props
            .iter()
            .find(|(k, _)| keys.iter().any(|q| k.eq_ignore_ascii_case(q)))
            .map(|(_, v)| v.trim().trim_matches('"').to_string())
    };
    let element = prop(&["creabim_id"])
        .and_then(|v| uuid::Uuid::parse_str(&v).ok())
        .map(ElementId);
    let hint = prop(&["creabim_kind"])
        .and_then(|v| Assign::parse(&v))
        .or_else(|| prop(&["ifc_class", "IfcClass", "ifc_type"]).and_then(|v| Assign::parse(&v)))
        .or_else(|| o.collections.iter().find_map(|c| collection_hint(c)));
    let material = main_material(scene, o);
    Some(Incoming {
        object: o.name.clone(),
        name: o.name.clone(),
        mesh: EditMesh { vertices, faces },
        hint,
        element,
        material,
    })
}

/// "Walls", "Slab", "IfcColumn" and the like.
fn collection_hint(name: &str) -> Option<Assign> {
    let l = name.trim().to_ascii_lowercase();
    let single = l.strip_suffix('s').unwrap_or(&l);
    if matches!(single, "wall" | "slab" | "floor" | "door" | "window") || l.starts_with("ifc") {
        Assign::parse(if l.starts_with("ifc") { name } else { single })
    } else {
        None
    }
}

/// The material on most of the object's faces, with its colour in sRGB.
fn main_material(scene: &BlendScene, o: &BlendObject) -> Option<(String, [f32; 3])> {
    let m = o.mesh.as_ref()?;
    let mut counts = vec![0usize; o.material_slots.len().max(1)];
    for &slot in &m.face_material {
        if let Some(c) = counts.get_mut(slot as usize) {
            *c += 1;
        }
    }
    let slot = (0..counts.len()).max_by_key(|&i| counts[i])?;
    let mat = &scene.materials[(*o.material_slots.get(slot)?)?];
    let srgb = |c: f32| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    };
    Some((
        mat.name.clone(),
        [srgb(mat.color[0]), srgb(mat.color[1]), srgb(mat.color[2])],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collections_name_kinds() {
        assert_eq!(collection_hint("Walls"), Some(Assign::Wall));
        assert_eq!(
            collection_hint("IfcColumn"),
            Some(Assign::Class("IfcColumn".into()))
        );
        assert_eq!(collection_hint("Furniture"), None);
    }
}
