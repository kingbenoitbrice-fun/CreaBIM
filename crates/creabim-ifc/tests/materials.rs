//! Materials through IFC: the built-in library survives an IFC4 export and
//! a materials-only import, and files from other tools give their
//! materials' names, categories and surface colours.

use creabim_core::library::LIBRARY;
use creabim_core::{sample::sample_house, ElementKind, Model};
use creabim_ifc::{export_ifc4, import_ifc, import_ifc_materials, ExportOptions};

fn fixture(name: &str) -> String {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn the_library_round_trips_through_ifc() {
    let model = Model::from_elements(LIBRARY.iter().map(|e| e.element())).unwrap();
    let ifc = export_ifc4(&model, &ExportOptions::default());
    let back = import_ifc_materials(&ifc).unwrap();
    assert_eq!(back.len(), LIBRARY.len());
    for entry in LIBRARY {
        let (_, m) = back
            .iter()
            .find(|(n, _)| n == entry.name)
            .unwrap_or_else(|| panic!("{} lost", entry.name));
        let want = entry.material();
        assert_eq!(m.hatch, want.hatch, "{}", entry.name);
        assert_eq!(m.category, want.category, "{}", entry.name);
        assert_eq!(m.description, want.description, "{}", entry.name);
        let near = |a: f32, b: f32| (a - b).abs() < 1e-5;
        assert!(
            m.color.iter().zip(want.color).all(|(a, b)| near(*a, b)),
            "{}",
            entry.name
        );
        assert!(near(m.roughness, want.roughness), "{}", entry.name);
        assert!(near(m.opacity, want.opacity), "{}", entry.name);
        assert_eq!(m.metallic, want.metallic, "{}", entry.name);
    }
}

#[test]
fn a_full_import_keeps_what_the_material_says() {
    let mut model = sample_house();
    // make the sample's timber a glossy, see-through, described material
    let timber = model.iter().find(|e| e.name == "Timber").cloned().unwrap();
    let mut after = timber.clone();
    if let ElementKind::Material(m) = &mut after.kind {
        m.roughness = 0.35;
        m.opacity = 0.6;
        m.description = "Oiled oak. Grade A".into();
    }
    let mut doc = creabim_core::Document::from_model(model);
    doc.commit(creabim_core::Transaction::new("t").replace(timber, after))
        .unwrap();
    model = doc.model().clone();
    let back = import_ifc(&export_ifc4(&model, &ExportOptions::default()))
        .unwrap()
        .model;
    let m = back
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::Material(m) if e.name == "Timber" => Some(m.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(m.category, "wood");
    assert_eq!(m.description, "Oiled oak. Grade A");
    assert_eq!(m.hatch, creabim_core::Hatch::Timber);
    assert!((m.roughness - 0.35).abs() < 1e-6);
    assert!((m.opacity - 0.6).abs() < 1e-6);
}

#[test]
fn materials_of_a_revit_export() {
    let mats = import_ifc_materials(&fixture("thirdparty/web-ifc-example.ifc")).unwrap();
    let names: Vec<&str> = mats.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "Concrete, Cast-in-Place gray",
            "<Unnamed>",
            "Metal - Steel - 345 MPa"
        ]
    );
    let (_, concrete) = &mats[0];
    assert_eq!(concrete.category, "concrete");
    assert_eq!(concrete.hatch, creabim_core::Hatch::Concrete);
    assert!((concrete.color[0] - 0.752_941).abs() < 1e-5);
    // the specular exponent is not taken for a roughness
    assert_eq!(concrete.roughness, creabim_core::element::DEFAULT_ROUGHNESS);
    let (_, steel) = &mats[2];
    assert_eq!((steel.category.as_str(), steel.metallic), ("steel", 1.0));
    assert!((steel.color[0] - 0.968_627).abs() < 1e-5);
}

#[test]
fn materials_of_ifcopenshell_files() {
    for file in [
        "ifcopenshell-ifc2x3.ifc",
        "ifcopenshell-ifc4.ifc",
        "ifcopenshell-ifc4x3.ifc",
    ] {
        let mats = import_ifc_materials(&fixture(file)).unwrap();
        let brick = &mats
            .iter()
            .find(|(n, _)| n == "Brick")
            .unwrap_or_else(|| panic!("{file}"))
            .1;
        assert_eq!(brick.category, "brick", "{file}");
        assert_eq!(brick.hatch, creabim_core::Hatch::Brick, "{file}");
    }
    assert!(import_ifc_materials("not an ifc").is_err());
}

#[test]
fn a_door_keeps_its_material_through_ifc4_and_ifcx() {
    let model = sample_house();
    let timber = model.iter().find(|e| e.name == "Timber").unwrap().id;
    let door = model
        .iter()
        .find(|e| e.name == "Front Door")
        .cloned()
        .unwrap();
    let mut after = door.clone();
    assert!(creabim_core::library::set_material(
        &mut after,
        Some(timber)
    ));
    let mut doc = creabim_core::Document::from_model(model);
    doc.commit(creabim_core::Transaction::new("t").replace(door, after))
        .unwrap();
    let model = doc.model().clone();
    let material_of_door = |m: &Model| {
        let d = m.iter().find(|e| e.name == "Front Door").unwrap();
        assert!(matches!(d.kind, ElementKind::Opening(_)));
        creabim_core::material_of(m, d).map(|(e, _)| e.name.clone())
    };
    assert_eq!(material_of_door(&model).as_deref(), Some("Timber"));
    let ifc4 = import_ifc(&export_ifc4(&model, &ExportOptions::default()))
        .unwrap()
        .model;
    assert_eq!(material_of_door(&ifc4).as_deref(), Some("Timber"));
    let ifcx = creabim_ifc::import_ifcx(&creabim_ifc::export_ifcx(&model))
        .unwrap()
        .model;
    assert_eq!(material_of_door(&ifcx).as_deref(), Some("Timber"));
}
