//! Reads the FreeCAD-generated fixtures (see `fixtures/make_fixtures.py`
//! and `fixtures/expected.txt`, which lists FreeCAD's own volumes).

use creabim_freecad::{read, FcDocument, FcObject};
use std::f64::consts::PI;

fn load(name: &str) -> FcDocument {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    read(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn obj<'a>(doc: &'a FcDocument, name: &str) -> &'a FcObject {
    doc.objects
        .iter()
        .find(|o| o.name == name)
        .unwrap_or_else(|| panic!("no object {name}"))
}

fn assert_rel(actual: f64, expected: f64, tol: f64, what: &str) {
    let err = ((actual - expected) / expected).abs();
    assert!(
        err <= tol,
        "{what}: got {actual}, expected {expected} (error {:.3}%)",
        err * 100.0
    );
}

fn assert_bbox(o: &FcObject, lo: [f64; 3], hi: [f64; 3], tol: f64) {
    let (a, b) = o.mesh.as_ref().unwrap().bounding_box().unwrap();
    for k in 0..3 {
        assert!(
            (a[k] - lo[k]).abs() <= tol && (b[k] - hi[k]).abs() <= tol,
            "{} bbox {a:?}-{b:?}, expected {lo:?}-{hi:?}",
            o.name
        );
    }
}

#[test]
fn parts_fixture() {
    let doc = read_and_check_closed("parts.FCStd");
    assert_eq!(doc.label, "parts");
    assert!(doc.program_version.starts_with("1.0"));
    let names: Vec<&str> = doc.objects.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(
        names,
        ["Box", "Cylinder", "Slab", "Sphere", "Cone", "Torus", "Blob", "Mesh", "Group"]
    );

    let bx = obj(&doc, "Box");
    assert_eq!(bx.label, "Red box");
    assert_eq!(bx.type_id, "Part::Box");
    assert_eq!(bx.property("Length"), Some("1000.0000000000000000"));
    let m = bx.mesh.as_ref().unwrap();
    assert_eq!(m.vertices.len(), 8);
    assert_eq!(m.faces.len(), 12);
    assert_rel(m.volume(), 1000.0 * 500.0 * 300.0, 1e-9, "box volume");
    assert_bbox(bx, [-150.0, 200.0, 0.0], [966.025, 1133.013, 300.0], 1e-3);
    // Placement: translation (100, 200, 0), 30 degrees about Z.
    let p = bx.placement;
    assert!((p[3][0] - 100.0).abs() < 1e-9 && (p[3][1] - 200.0).abs() < 1e-9);
    assert!((p[0][0] - 30f64.to_radians().cos()).abs() < 1e-12);
    assert!((p[0][1] - 30f64.to_radians().sin()).abs() < 1e-12);
    let local = bx.local_mesh().unwrap().bounding_box().unwrap();
    assert!((local.1[0] - 1000.0).abs() < 1e-6 && local.0[0].abs() < 1e-6);
    let c = bx.color.unwrap();
    assert_eq!(c, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(bx.parent, Some(8));

    let cyl = obj(&doc, "Cylinder");
    assert_eq!(cyl.label, "Column");
    assert_rel(
        cyl.mesh.as_ref().unwrap().volume(),
        PI * 200.0 * 200.0 * 1000.0,
        0.01,
        "cylinder",
    );
    assert_bbox(cyl, [1800.0, -200.0, 0.0], [2200.0, 200.0, 1000.0], 2.0);
    let c = cyl.color.unwrap();
    assert_eq!(&c[..3], &[0.0, 0.0, 1.0]);
    assert!((c[3] - 0.5).abs() < 1e-6);

    let slab = obj(&doc, "Slab");
    assert_eq!(slab.type_id, "Part::Feature");
    let expected = (2000.0 * 1000.0 - 400.0 * 400.0 - PI * 150.0 * 150.0) * 200.0;
    assert_rel(
        slab.mesh.as_ref().unwrap().volume(),
        expected,
        0.002,
        "slab",
    );
    assert_bbox(slab, [0.0, 0.0, 3000.0], [2000.0, 1000.0, 3200.0], 1e-6);
    assert_eq!(slab.parent, None);

    let sph = obj(&doc, "Sphere");
    assert!(!sph.visible);
    assert_rel(
        sph.mesh.as_ref().unwrap().volume(),
        4.0 / 3.0 * PI * 300f64.powi(3),
        0.01,
        "sphere",
    );
    assert_bbox(sph, [-300.0, -2300.0, -300.0], [300.0, -1700.0, 300.0], 3.0);

    let cone = obj(&doc, "Cone");
    let v = PI * 500.0 / 3.0 * (300.0f64.powi(2) + 300.0 * 100.0 + 100.0f64.powi(2));
    assert_rel(cone.mesh.as_ref().unwrap().volume(), v, 0.01, "cone");

    let torus = obj(&doc, "Torus");
    let v = 2.0 * PI * PI * 500.0 * 100.0 * 100.0;
    assert_rel(torus.mesh.as_ref().unwrap().volume(), v, 0.01, "torus");
    assert_bbox(torus, [-600.0, 2400.0, -100.0], [600.0, 3600.0, 100.0], 3.0);

    // Periodic B-spline outline extruded; FreeCAD's own volume.
    let blob = obj(&doc, "Blob");
    assert_rel(
        blob.mesh.as_ref().unwrap().volume(),
        94_870_232.618,
        0.005,
        "blob",
    );
    assert_bbox(
        blob,
        [2613.741, 2749.219, 0.0],
        [3404.611, 3250.788, 300.0],
        1.0,
    );

    let mesh = obj(&doc, "Mesh");
    assert_eq!(mesh.type_id, "Mesh::Feature");
    let mm = mesh.mesh.as_ref().unwrap();
    assert_eq!(mm.faces.len(), 12);
    assert_rel(mm.volume(), 100.0 * 200.0 * 300.0, 1e-6, "mesh box");
    assert_bbox(mesh, [-50.0, -100.0, -1150.0], [50.0, 100.0, -850.0], 1e-4);

    let group = obj(&doc, "Group");
    assert_eq!(group.label, "Assembly");
    assert!(group.mesh.is_none());
}

fn read_and_check_closed(name: &str) -> FcDocument {
    let doc = load(name);
    for o in &doc.objects {
        if let Some(m) = &o.mesh {
            assert!(m.is_closed(), "{name}: {} mesh is not closed", o.name);
        }
    }
    doc
}

#[test]
fn arch_fixture() {
    let doc = read_and_check_closed("arch.FCStd");
    let wall = obj(&doc, "Wall");
    assert_eq!(wall.label, "South wall");
    assert_eq!(wall.type_id, "Part::FeaturePython");
    assert_eq!(wall.proxy_type.as_deref(), Some("Wall"));
    assert_eq!(wall.ifc_type.as_deref(), Some("Wall"));
    assert_eq!(wall.property("Width"), Some("200.0000000000000000"));
    assert_eq!(wall.property("Height"), Some("3000.0000000000000000"));
    // FreeCAD reports 2.28e9 mm3: the window opening is cut out.
    assert_rel(
        wall.mesh.as_ref().unwrap().volume(),
        2_280_000_000.0,
        1e-9,
        "wall",
    );
    assert_bbox(wall, [-2000.0, -100.0, 0.0], [2000.0, 100.0, 3000.0], 1e-6);

    let curved = obj(&doc, "Wall001");
    assert_eq!(curved.label, "Curved wall");
    assert_eq!(curved.proxy_type.as_deref(), Some("Wall"));
    assert_rel(
        curved.mesh.as_ref().unwrap().volume(),
        2_356_194_490.192,
        0.001,
        "curved wall",
    );
    assert_bbox(curved, [0.0, 5000.0, 0.0], [3100.0, 8100.0, 2500.0], 1.0);
    // The wall's base line has no faces.
    assert!(obj(&doc, "Arc").mesh.is_none());

    let win = obj(&doc, "Window");
    assert_eq!(win.proxy_type.as_deref(), Some("Window"));
    assert_eq!(win.ifc_type.as_deref(), Some("Window"));
    assert_eq!(win.property("Hosts"), None, "link lists are not flattened");
    assert_rel(
        win.mesh.as_ref().unwrap().volume(),
        30_900_000.0,
        1e-9,
        "window",
    );

    let col = obj(&doc, "Structure");
    assert_eq!(col.label, "Column");
    assert_eq!(col.proxy_type.as_deref(), Some("Structure"));
    assert_eq!(col.ifc_type.as_deref(), Some("Column"));
    assert_rel(
        col.mesh.as_ref().unwrap().volume(),
        270_000_000.0,
        1e-9,
        "column",
    );

    let level = obj(&doc, "BuildingPart");
    assert_eq!(level.label, "Ground floor");
    assert_eq!(level.proxy_type.as_deref(), Some("BuildingPart"));
    assert_eq!(level.ifc_type.as_deref(), Some("Building Storey"));
    assert!(level.mesh.is_none(), "container shapes are not duplicated");
    let li = doc.objects.iter().position(|o| o.name == "BuildingPart");
    for n in ["Wall", "Window", "Structure"] {
        assert_eq!(obj(&doc, n).parent, li, "{n} belongs to the level");
    }
    // The window's base sketch has no faces, hence no mesh.
    assert!(obj(&doc, "Sketch").mesh.is_none());
    let c = wall.color.unwrap();
    assert!((c[0] - 0.8).abs() < 0.01 && (c[2] - 0.7).abs() < 0.01);
}

#[test]
fn binary_brep_is_reported() {
    let doc = load("binary.FCStd");
    let bx = obj(&doc, "Box");
    assert!(bx.mesh.is_none());
    assert!(
        doc.warnings.iter().any(|w| w.contains("binary BREP")),
        "{:?}",
        doc.warnings
    );
}
