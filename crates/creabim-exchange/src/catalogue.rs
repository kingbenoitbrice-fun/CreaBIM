//! Objects for the catalogue from other tools' files: a Blender (`.blend`),
//! FreeCAD (`.FCStd`) or glTF (`.gltf`, `.glb`) file read as one object,
//! each of its meshes a part in its own material, set on the floor with the
//! middle of its back edge at the origin. What it is (category, IFC class,
//! wall or floor) comes from an IFC class asked for in the file, else is
//! guessed from the names; all of it can be changed once it is in.

use crate::{ExchangeError, Incoming};
use creabim_core::objects::{
    self, CatalogueObject, IfcKind, Mount, ObjectCategory, ObjectPart, ObjectType,
};
use creabim_core::{Hatch, Material};
use std::path::Path;

/// The file kinds an object can be read from (lower-case extensions).
pub const EXTENSIONS: &[&str] = &["blend", "fcstd", "gltf", "glb"];

/// Read a file as a catalogue object keyed `key`.
pub fn read_file(path: &Path, key: &str) -> Result<CatalogueObject, ExchangeError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (incoming, looks) = match ext.as_str() {
        "blend" => {
            let scene = creabim_blend::read_file(path)?;
            (
                crate::blend::incoming(&scene),
                crate::materials::read_blend(&scene),
            )
        }
        "fcstd" => (crate::freecad::read_file(path)?, vec![]),
        "gltf" | "glb" => crate::gltf_read::read_file(path)?,
        other => {
            return Err(ExchangeError::Other(format!(
                "CreaBIM can't read objects from .{other} files"
            )))
        }
    };
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Object")
        .replace(['_', '-'], " ");
    object(&incoming, &looks, &stem, key).ok_or_else(|| {
        ExchangeError::Other(format!(
            "{} has no meshes to make an object of",
            path.display()
        ))
    })
}

/// The object made of `incoming`, its materials' full looks taken from
/// `looks` by name when there.
pub fn object(
    incoming: &[Incoming],
    looks: &[(String, Material)],
    name: &str,
    key: &str,
) -> Option<CatalogueObject> {
    let mut parts = vec![];
    let mut materials = vec![];
    let mut class = None;
    for inc in incoming.iter().filter(|i| !i.mesh.faces.is_empty()) {
        if let Some(crate::Assign::Class(c)) = &inc.hint {
            let c = objects::class_of_type(c).unwrap_or(c);
            if !objects::predefined_types(c).is_empty() {
                class.get_or_insert(c.to_string());
            }
        }
        parts.push(ObjectPart {
            name: inc.name.clone(),
            mesh: inc.mesh.clone(),
            material: None,
        });
        materials.push(inc.material.as_ref().map(|(n, color)| {
            let m = looks
                .iter()
                .find(|(l, _)| l == n)
                .map(|(_, m)| m.clone())
                .unwrap_or_else(|| Material::new(Hatch::Empty, *color));
            (n.clone(), m)
        }));
    }
    if parts.is_empty() {
        return None;
    }
    objects::recentre(&mut parts);
    let words = std::iter::once(name)
        .chain(parts.iter().map(|p| p.name.as_str()))
        .collect::<Vec<_>>()
        .join(" ");
    let (category, guessed, mount) = guess(&words);
    let ifc = match class {
        Some(c) => IfcKind::new(&c, guessed.predefined.as_str()).normalized(),
        None => guessed,
    };
    let name = name.trim();
    let name = if name.is_empty() { "Object" } else { name };
    let mut chars = name.chars();
    let name: String = chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    Some(CatalogueObject {
        key: key.to_string(),
        name,
        description: String::new(),
        object: ObjectType {
            category,
            ifc,
            mount,
            parts,
            symbol: vec![],
            catalogue: Some(key.to_string()),
        },
        materials,
    })
}

/// What a thing called `words` probably is.
pub fn guess(words: &str) -> (ObjectCategory, IfcKind, Mount) {
    use ObjectCategory as C;
    // whole words only, so a "floor plane" is no loo; plurals count
    let lower = words.to_ascii_lowercase();
    let list: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let joined = format!(" {} ", list.join(" "));
    let has = |k: &[&str]| {
        k.iter().any(|k| {
            if k.contains(' ') {
                joined.contains(&format!(" {k} "))
            } else {
                list.iter()
                    .any(|w| *w == *k || w.strip_suffix('s').or(w.strip_suffix("es")) == Some(k))
            }
        })
    };
    let (c, class, p, m) = if has(&["washbasin", "basin", "lavatory", "vanity"]) {
        (
            C::Bathroom,
            "IfcSanitaryTerminal",
            "WASHHANDBASIN",
            Mount::Wall,
        )
    } else if has(&["toilet", "wc", "loo"]) {
        (C::Bathroom, "IfcSanitaryTerminal", "TOILETPAN", Mount::Wall)
    } else if has(&["bathtub", "bath tub", "tub", "bath"]) {
        (C::Bathroom, "IfcSanitaryTerminal", "BATH", Mount::Wall)
    } else if has(&["shower"]) {
        (C::Bathroom, "IfcSanitaryTerminal", "SHOWER", Mount::Wall)
    } else if has(&["sink"]) {
        (C::Kitchen, "IfcSanitaryTerminal", "SINK", Mount::Wall)
    } else if has(&["fridge", "refrigerator", "freezer"]) {
        (
            C::Kitchen,
            "IfcElectricAppliance",
            "FRIDGE_FREEZER",
            Mount::Wall,
        )
    } else if has(&["oven", "cooker", "hob", "stove", "range"]) {
        (
            C::Kitchen,
            "IfcElectricAppliance",
            "ELECTRICCOOKER",
            Mount::Wall,
        )
    } else if has(&["dishwasher"]) {
        (
            C::Kitchen,
            "IfcElectricAppliance",
            "DISHWASHER",
            Mount::Wall,
        )
    } else if has(&["sofa", "couch", "settee"]) {
        (C::Living, "IfcFurniture", "SOFA", Mount::Wall)
    } else if has(&["armchair", "chair", "stool", "seat"]) {
        (C::Dining, "IfcFurniture", "CHAIR", Mount::Floor)
    } else if has(&["desk"]) {
        (C::Office, "IfcFurniture", "DESK", Mount::Wall)
    } else if has(&["bed"]) {
        (C::Bedroom, "IfcFurniture", "BED", Mount::Wall)
    } else if has(&["table"]) {
        (C::Dining, "IfcFurniture", "TABLE", Mount::Floor)
    } else if has(&["wardrobe", "closet", "shelf", "shelves", "bookcase"]) {
        (C::Bedroom, "IfcFurniture", "SHELF", Mount::Wall)
    } else if has(&["cabinet", "cupboard", "drawer", "unit"]) {
        (C::Kitchen, "IfcFurniture", "FILECABINET", Mount::Wall)
    } else if has(&["lamp", "light", "plant", "rug", "mirror"]) {
        (C::Living, "IfcFurniture", "NOTDEFINED", Mount::Floor)
    } else {
        (C::Other, "IfcFurniture", "NOTDEFINED", Mount::Floor)
    };
    (c, IfcKind::new(class, p).normalized(), m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(rel: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(rel)
    }

    fn sits_on_floor(c: &CatalogueObject) {
        let (lo, hi) = c.object.bounds();
        assert!(lo.z.abs() < 1e-9, "{lo:?}");
        assert!(hi.y.abs() < 1e-9, "{hi:?}");
        assert!((lo.x + hi.x).abs() < 1e-9, "{lo:?} {hi:?}");
        assert_eq!(c.materials.len(), c.object.parts.len());
        c.object.validate().unwrap();
    }

    #[test]
    fn blender_and_freecad_files_become_objects() {
        let b = read_file(
            &fixture("creabim-blend/fixtures/scene_4_0.blend"),
            "user/scene",
        )
        .unwrap();
        assert!(!b.object.parts.is_empty());
        sits_on_floor(&b);
        assert_eq!(b.name, "Scene 4 0");
        let f = read_file(
            &fixture("creabim-freecad/fixtures/parts.FCStd"),
            "user/parts",
        )
        .unwrap();
        assert!(!f.object.parts.is_empty());
        sits_on_floor(&f);
        assert!(read_file(Path::new("chair.obj"), "x").is_err());
    }

    #[test]
    fn a_sofa_exported_to_gltf_comes_back_as_a_sofa() {
        let model = creabim_core::sample::furnished_house();
        let glb = crate::gltf::export_glb(&model);
        let (inc, looks) = crate::gltf_read::read(&glb, &|_| None).unwrap();
        let sofa: Vec<Incoming> = inc
            .into_iter()
            .filter(|i| i.name.starts_with("Sofa"))
            .collect();
        assert!(!sofa.is_empty());
        let back = object(&sofa, &looks, "sofa from blender", "user/sofa").unwrap();
        sits_on_floor(&back);
        assert_eq!(back.name, "Sofa from blender");
        assert_eq!(back.object.ifc.predefined, "SOFA");
        assert_eq!(back.object.category, ObjectCategory::Living);
        let starter = creabim_core::catalogue::starter("starter/sofa-3").unwrap();
        assert!((back.object.size().z - starter.object.size().z).abs() < 1e-3);
        assert!(back.materials.iter().any(|m| m.is_some()));
    }

    #[test]
    fn names_say_what_things_are() {
        assert_eq!(guess("Kitchen sink").1.predefined, "SINK");
        assert_eq!(guess("toilet").1.predefined, "TOILETPAN");
        assert_eq!(guess("Office desk").0, ObjectCategory::Office);
        assert_eq!(guess("Bar stool").2, Mount::Floor);
        assert_eq!(guess("Thing").1, IfcKind::default());
        // words, not letters inside them
        assert_eq!(guess("Floor lamp").1, IfcKind::default());
        assert_eq!(guess("Floor lamp").0, ObjectCategory::Living);
        assert_eq!(guess("Kitchen_Sink.001").1.predefined, "SINK");
        assert_eq!(guess("Dining chairs").1.predefined, "CHAIR");
    }
}
