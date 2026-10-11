//! IFC5 (IFCX alpha) import and export.

use creabim_core::{sample::sample_house, ElementKind, Model, OpeningKind};
use creabim_ifc::{export_ifcx, import_ifcx};
use glam::{DVec2, DVec3};
use std::collections::BTreeMap;

fn building_counts(m: &Model) -> BTreeMap<String, usize> {
    let mut c = BTreeMap::new();
    for e in m.iter() {
        let class = e.ifc_class();
        if class.starts_with("Ifc") {
            *c.entry(class.to_string()).or_insert(0) += 1;
        }
    }
    c
}

#[test]
fn sample_house_round_trips_losslessly() {
    let model = sample_house();
    let text = export_ifcx(&model);
    let json: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(json["header"]["ifcxVersion"], "ifcx_alpha");
    let r = import_ifcx(&text).unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(building_counts(&r.model), building_counts(&model));
    for e in model.iter() {
        if !e.ifc_class().starts_with("Ifc") {
            continue;
        }
        let back = r
            .model
            .get(e.id)
            .unwrap_or_else(|| panic!("{} lost", e.name));
        assert_eq!(back, e, "{} changed", e.name);
    }
}

/// Other tools do not write CreaBIM's parameters: strip them and the
/// geometry alone must give back the same walls, slabs and openings.
#[test]
fn sample_house_is_recognised_from_geometry_alone() {
    let model = sample_house();
    let mut json: serde_json::Value = serde_json::from_str(&export_ifcx(&model)).unwrap();
    for node in json["data"].as_array_mut().unwrap() {
        if let Some(a) = node.get_mut("attributes").and_then(|a| a.as_object_mut()) {
            a.remove("creabim::element");
        }
    }
    let r = import_ifcx(&json.to_string()).unwrap();
    let m = &r.model;
    let mut expected = building_counts(&model);
    expected.remove("IfcGrid"); // grids are only read back from CreaBIM's own data
    assert_eq!(building_counts(m), expected, "{:?}", r.warnings);
    for e in model.iter() {
        match &e.kind {
            ElementKind::Wall(w) => {
                let Some(ElementKind::Wall(b)) = m.get(e.id).map(|x| &x.kind) else {
                    panic!("{} is not a wall", e.name)
                };
                assert!(
                    w.start.distance(b.start) < 1e-3,
                    "{}: {} vs {}",
                    e.name,
                    w.start,
                    b.start
                );
                assert!(w.end.distance(b.end) < 1e-3, "{}", e.name);
                assert!((w.thickness - b.thickness).abs() < 1e-4);
                assert!((w.height - b.height).abs() < 1e-4);
                assert_eq!(w.level, b.level);
            }
            ElementKind::Slab(s) => {
                let Some(ElementKind::Slab(b)) = m.get(e.id).map(|x| &x.kind) else {
                    panic!("{} is not a slab", e.name)
                };
                assert_eq!(b.level, s.level);
                assert!((b.thickness - s.thickness).abs() < 1e-4);
                for p in &s.boundary {
                    assert!(b.boundary.iter().any(|q| q.distance(*p) < 1e-4));
                }
            }
            ElementKind::Opening(o) => {
                let Some(ElementKind::Opening(b)) = m.get(e.id).map(|x| &x.kind) else {
                    panic!("{} is not an opening", e.name)
                };
                assert_eq!((b.host, b.kind), (o.host, o.kind));
                for (x, y) in [
                    (o.offset, b.offset),
                    (o.sill, b.sill),
                    (o.width, b.width),
                    (o.height, b.height),
                ] {
                    assert!((x - y).abs() < 1e-4, "{}: {x} vs {y}", e.name);
                }
            }
            _ => {}
        }
    }
}

/// A hand-written file in the style of buildingSMART's examples: layered
/// entries, a window type inherited with an invisible void, USD transforms
/// and an inherited material.
#[test]
fn layered_file_with_types_and_transforms() {
    let path = format!("{}/fixtures/layered-wall.ifcx", env!("CARGO_MANIFEST_DIR"));
    let r = import_ifcx(&std::fs::read_to_string(path).unwrap()).unwrap();
    let m = &r.model;
    assert_eq!(r.schema, "ifcx_alpha");
    let levels: Vec<f64> = m.levels().map(|(_, l)| l.elevation).collect();
    assert_eq!(levels, vec![0.0, 3.0]);
    let (we, w) = m
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::Wall(w) => Some((e, w)),
            _ => None,
        })
        .expect("a native wall");
    assert_eq!(we.id.0.to_string(), "0b3e2a8e-0000-4000-8000-000000000006");
    assert!(w.start.distance(DVec2::new(1.0, 1.0)) < 1e-9, "{}", w.start);
    assert!(w.end.distance(DVec2::new(1.0, 5.0)) < 1e-9, "{}", w.end);
    assert!((w.thickness - 0.2).abs() < 1e-9 && (w.height - 2.5).abs() < 1e-9);
    assert_eq!(m.level(w.level).unwrap().elevation, 3.0);
    assert!(w.base_offset.abs() < 1e-9);
    let mat = m.get(w.material.unwrap()).unwrap();
    assert_eq!(mat.name, "CONCRETE");
    assert!(matches!(&mat.kind, ElementKind::Material(x) if x.color == [0.5, 0.6, 0.7]));

    let o = m
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::Opening(o) => Some(o),
            _ => None,
        })
        .expect("the window is hosted");
    assert_eq!((o.host, o.kind), (we.id, OpeningKind::Window));
    for (x, y) in [
        (o.offset, 1.0),
        (o.sill, 1.0),
        (o.width, 1.0),
        (o.height, 1.2),
    ] {
        assert!((x - y).abs() < 1e-9, "{x} vs {y}");
    }

    let f = m
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::FreeMesh(f) => Some(f),
            _ => None,
        })
        .expect("the table");
    assert_eq!(f.ifc_class, "IfcFurniture");
    assert_eq!(f.level, Some(w.level));
    assert!((f.origin - DVec3::new(3.0, 2.0, 0.0)).length() < 1e-9);
    assert!((f.mesh.volume() - 1.0 / 6.0).abs() < 1e-9);
    assert!(r.warnings.iter().any(|w| w.contains("spaces")));
}
