//! IFC import: our own exports round-trip, and files written by other tools
//! (committed under `fixtures/`) come in with the right elements in the
//! right places.

use creabim_core::{sample::sample_house, Element, ElementKind, Model, OpeningKind};
use creabim_ifc::{export_ifc4, import_ifc, ExportOptions};
use glam::DVec3;
use std::collections::BTreeMap;

fn fixture(name: &str) -> String {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    String::from_utf8_lossy(&bytes).into_owned()
}

fn counts(m: &Model) -> BTreeMap<String, usize> {
    let mut c = BTreeMap::new();
    for e in m.iter() {
        *c.entry(e.ifc_class().to_string()).or_insert(0) += 1;
    }
    c
}

/// Only the classes IFC carries; views, sheets and annotations stay behind.
fn building_counts(m: &Model) -> BTreeMap<String, usize> {
    let mut c = counts(m);
    c.retain(|k, _| k.starts_with("Ifc") && k != "IfcGrid");
    c
}

fn level_elevation(m: &Model, e: &Element) -> f64 {
    let level = match &e.kind {
        ElementKind::Wall(w) => Some(w.level),
        ElementKind::Slab(s) => Some(s.level),
        ElementKind::FreeMesh(f) => f.level,
        _ => None,
    };
    level.and_then(|l| m.level(l)).map_or(0.0, |l| l.elevation)
}

#[test]
fn sample_house_round_trips_through_ifc() {
    let model = sample_house();
    let ifc = export_ifc4(&model, &ExportOptions::default());
    let r = import_ifc(&ifc).expect("our own export imports");
    let back = &r.model;
    assert_eq!(r.schema, "IFC4");
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(building_counts(back), building_counts(&model));

    for e in model.iter() {
        let get = || {
            back.get(e.id)
                .unwrap_or_else(|| panic!("{} {:?} lost its id", e.ifc_class(), e.name))
        };
        match &e.kind {
            ElementKind::Level(l) => {
                let ElementKind::Level(b) = &get().kind else {
                    panic!()
                };
                assert!((l.elevation - b.elevation).abs() < 1e-6);
                assert_eq!(get().name, e.name);
            }
            ElementKind::Wall(w) => {
                let ElementKind::Wall(b) = &get().kind else {
                    panic!("{} is not a wall any more", e.name)
                };
                assert_eq!(b.level, w.level);
                assert!(
                    w.start.distance(b.start) < 1e-3,
                    "{}: {} vs {}",
                    e.name,
                    w.start,
                    b.start
                );
                assert!(
                    w.end.distance(b.end) < 1e-3,
                    "{}: {} vs {}",
                    e.name,
                    w.end,
                    b.end
                );
                assert!((w.length() - b.length()).abs() < 1e-3);
                assert!((w.thickness - b.thickness).abs() < 1e-6);
                assert!((w.height - b.height).abs() < 1e-6);
                assert!((w.base_offset - b.base_offset).abs() < 1e-6);
                let name = |m: &Model, id: Option<_>| {
                    id.and_then(|i| m.get(i)).map(|x: &Element| x.name.clone())
                };
                assert_eq!(name(back, b.material), name(&model, w.material));
            }
            ElementKind::Slab(s) => {
                let ElementKind::Slab(b) = &get().kind else {
                    panic!("{} is not a slab", e.name)
                };
                assert_eq!(b.level, s.level);
                assert!((b.thickness - s.thickness).abs() < 1e-6);
                assert_eq!(b.boundary.len(), s.boundary.len());
                for p in &s.boundary {
                    assert!(b.boundary.iter().any(|q| q.distance(*p) < 1e-6));
                }
            }
            ElementKind::Opening(o) => {
                let ElementKind::Opening(b) = &get().kind else {
                    panic!("{} is not an opening", e.name)
                };
                assert_eq!(b.host, o.host);
                assert_eq!(b.kind, o.kind);
                for (x, y) in [
                    (o.offset, b.offset),
                    (o.sill, b.sill),
                    (o.width, b.width),
                    (o.height, b.height),
                ] {
                    assert!((x - y).abs() < 1e-6, "{}: {x} vs {y}", e.name);
                }
            }
            ElementKind::FreeMesh(f) => {
                let ElementKind::FreeMesh(b) = &get().kind else {
                    panic!()
                };
                assert_eq!(b.ifc_class, f.ifc_class);
                assert_eq!(b.level, f.level);
                let shown = f.evaluated().mesh;
                assert!(
                    (b.mesh.volume() - shown.volume()).abs() < 1e-6,
                    "{}",
                    e.name
                );
                assert!(b.mesh.faces.len() <= shown.faces.len() * 2);
                // same place in the model
                let lo = |m: &creabim_core::EditMesh, o: DVec3| {
                    m.vertices
                        .iter()
                        .fold(DVec3::INFINITY, |a, v| a.min(*v + o))
                };
                let z = level_elevation(&model, e);
                assert!((lo(&shown, f.origin) - lo(&b.mesh, b.origin)).length() < 1e-6);
                assert_eq!(z, level_elevation(back, get()));
            }
            ElementKind::Material(m) => {
                let b = back
                    .iter()
                    .find(|x| x.name == e.name)
                    .unwrap_or_else(|| panic!("material {} lost", e.name));
                let ElementKind::Material(bm) = &b.kind else {
                    panic!()
                };
                assert_eq!(bm.hatch, m.hatch);
                for (x, y) in m.color.iter().zip(bm.color) {
                    assert!((x - y).abs() < 1e-6);
                }
            }
            _ => {}
        }
    }

    // a second trip changes nothing that IFC carries
    let again = import_ifc(&export_ifc4(back, &ExportOptions::default())).unwrap();
    assert_eq!(building_counts(&again.model), building_counts(back));
}

#[test]
fn garbage_is_an_error_not_a_panic() {
    assert!(import_ifc("").is_err());
    assert!(import_ifc("hello world").is_err());
    let truncated = export_ifc4(&sample_house(), &ExportOptions::default());
    for cut in [100, 1000, truncated.len() / 2, truncated.len() - 10] {
        let mut n = cut.min(truncated.len());
        while !truncated.is_char_boundary(n) {
            n -= 1;
        }
        let _ = import_ifc(&truncated[..n]);
    }
}

/// Fixtures written by IfcOpenShell (see `fixtures/make_fixtures.py`): a
/// project in millimetres with a wall, a door, a window, a slab, a column and
/// a tessellated proxy, in three schemas.
fn check_ifcopenshell_fixture(name: &str, schema: &str) {
    let r = import_ifc(&fixture(name)).unwrap();
    let m = &r.model;
    assert_eq!(r.schema, schema);
    let c = counts(m);
    let n = |k: &str| c.get(k).copied().unwrap_or(0);
    assert_eq!(n("IfcBuildingStorey"), 2, "{name}: {c:?} {:?}", r.warnings);
    assert_eq!(n("IfcWall"), 2, "{name}: {c:?} {:?}", r.warnings);
    assert_eq!(n("IfcSlab"), 1, "{name}: {c:?}");
    assert_eq!(n("IfcDoor"), 1, "{name}: {c:?}");
    assert_eq!(n("IfcWindow"), 1, "{name}: {c:?}");
    assert_eq!(n("IfcColumn"), 1, "{name}: {c:?}");
    assert_eq!(n("IfcBuildingElementProxy"), 1, "{name}: {c:?}");

    let levels: Vec<f64> = m.levels().map(|(_, l)| l.elevation).collect();
    assert!(levels.iter().any(|z| z.abs() < 1e-9) && levels.iter().any(|z| (z - 3.2).abs() < 1e-9));
    let gf = m
        .levels()
        .find(|(_, l)| l.elevation.abs() < 1e-9)
        .unwrap()
        .1;
    assert!((gf.height - 3.2).abs() < 1e-9);

    // the wall on the ground floor: 5 m from (1, 2) along x, 0.2 thick, 3 high
    let walls: Vec<_> = m
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Wall(w) => Some((e, w)),
            _ => None,
        })
        .collect();
    let (we, w) = walls
        .iter()
        .find(|(e, _)| e.name == "Wall A")
        .unwrap_or_else(|| panic!("{name}: no native Wall A in {c:?}: {:?}", r.warnings));
    assert!((w.length() - 5.0).abs() < 1e-3, "{name}: {}", w.length());
    assert!((w.thickness - 0.2).abs() < 1e-6, "{name}");
    assert!((w.height - 3.0).abs() < 1e-6, "{name}");
    assert!(
        w.start.distance(glam::DVec2::new(1.0, 2.1)) < 1e-3,
        "{name}: {}",
        w.start
    );
    assert!(
        w.end.distance(glam::DVec2::new(6.0, 2.1)) < 1e-3,
        "{name}: {}",
        w.end
    );
    assert_eq!(m.level(w.level).unwrap().elevation, 0.0);
    // a layer set is named for the set; its hatch is guessed from the name
    let mat = |id: Option<creabim_core::ElementId>| id.and_then(|id| m.get(id));
    let brick = mat(w.material).unwrap();
    assert_eq!(brick.name, "Brick wall", "{name}");
    assert!(
        matches!(&brick.kind, ElementKind::Material(x) if x.hatch == creabim_core::Hatch::Brick)
    );
    // wall B, turned 90° on the upper storey, takes its material from its type
    let (_, b) = walls.iter().find(|(e, _)| e.name == "Wall B").unwrap();
    assert_eq!(mat(b.material).unwrap().name, "Concrete wall", "{name}");
    assert!((b.thickness - 0.25).abs() < 1e-6 && (b.height - 2.8).abs() < 1e-6);
    assert!(
        b.start.distance(glam::DVec2::new(0.875, 2.0)) < 1e-3,
        "{name}: {}",
        b.start
    );
    assert!(
        b.end.distance(glam::DVec2::new(0.875, 6.0)) < 1e-3,
        "{name}: {}",
        b.end
    );
    assert!((m.level(b.level).unwrap().elevation - 3.2).abs() < 1e-9);

    // the door and the window are hosted by that wall
    for (kind, offset, sill, width, height) in [
        (OpeningKind::Door, 1.0, 0.0, 0.9, 2.1),
        (OpeningKind::Window, 3.0, 0.9, 1.2, 1.2),
    ] {
        let o = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Opening(o) if o.kind == kind => Some(o),
                _ => None,
            })
            .unwrap();
        assert_eq!(o.host, we.id, "{name}");
        assert!(
            (o.offset - offset).abs() < 1e-3,
            "{name} {kind:?}: offset {}",
            o.offset
        );
        assert!(
            (o.sill - sill).abs() < 1e-3,
            "{name} {kind:?}: sill {}",
            o.sill
        );
        assert!(
            (o.width - width).abs() < 1e-3,
            "{name} {kind:?}: width {}",
            o.width
        );
        assert!(
            (o.height - height).abs() < 1e-3,
            "{name} {kind:?}: height {}",
            o.height
        );
    }

    // the slab under the ground floor: 6 x 4 m, 0.25 thick, top at 0
    let (_, s) = m
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::Slab(s) => Some((e, s)),
            _ => None,
        })
        .unwrap();
    assert!((s.thickness - 0.25).abs() < 1e-6);
    let area: f64 = (0..s.boundary.len())
        .map(|i| s.boundary[i].perp_dot(s.boundary[(i + 1) % s.boundary.len()]))
        .sum::<f64>()
        / 2.0;
    assert!((area - 24.0).abs() < 1e-6, "{name}: {area}");

    // the column: an extruded 300 mm square at (7, 1), 3 m high, native
    let c = m
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::Column(c) => Some(c),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{name}: no native column: {:?}", r.details));
    assert!(
        c.position.distance(glam::DVec2::new(7.0, 1.0)) < 1e-6,
        "{name}: {}",
        c.position
    );
    assert!(
        matches!(c.profile, creabim_core::Profile::Rectangle { width, depth }
            if (width - 0.3).abs() < 1e-6 && (depth - 0.3).abs() < 1e-6),
        "{name}: {:?}",
        c.profile
    );
    assert!((c.height.unwrap() - 3.0).abs() < 1e-6);
    // the proxy stays a mesh
    let (e, f) = m
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::FreeMesh(f) if f.ifc_class == "IfcBuildingElementProxy" => Some((e, f)),
            _ => None,
        })
        .unwrap();
    assert!(
        (f.mesh.volume() - 1.0 / 6.0).abs() < 1e-6,
        "{name}: {}",
        f.mesh.volume()
    );
    let lo = f
        .mesh
        .vertices
        .iter()
        .fold(DVec3::INFINITY, |a, v| a.min(*v + f.origin));
    let world = lo + DVec3::Z * level_elevation(m, e);
    assert!(
        (world - DVec3::new(8.0, 3.0, 3.2)).length() < 1e-6,
        "{name}: {world}"
    );
}

#[test]
fn ifcopenshell_ifc4_millimetres() {
    check_ifcopenshell_fixture("ifcopenshell-ifc4.ifc", "IFC4");
}

#[test]
fn ifcopenshell_ifc2x3_millimetres() {
    check_ifcopenshell_fixture("ifcopenshell-ifc2x3.ifc", "IFC2X3");
}

#[test]
fn ifcopenshell_ifc4x3_millimetres() {
    check_ifcopenshell_fixture("ifcopenshell-ifc4x3.ifc", "IFC4X3_ADD2");
}

/// A Revit export (IFC2X3, millimetres, degrees) shipped with web-ifc: walls
/// are standard cases with layer set usages, slabs and footings are
/// extrusions of composite curves with arcs, beams and columns are mapped
/// items. Expected positions come from IfcOpenShell's geometry engine
/// (`fixtures/verify_import.py` checks every element the same way).
#[test]
fn revit_ifc2x3_from_web_ifc() {
    let r = import_ifc(&fixture("thirdparty/web-ifc-example.ifc")).unwrap();
    let m = &r.model;
    let c = counts(m);
    assert_eq!(r.schema, "IFC2X3");
    assert_eq!(c["IfcBuildingStorey"], 2);
    assert_eq!(c["IfcWall"], 17, "{c:?}");
    assert_eq!(c["IfcColumn"], 42);
    assert_eq!(c["IfcBeam"], 43);
    assert_eq!(c["IfcSlab"], 9);
    assert_eq!(c["IfcBuildingElementProxy"], 4);
    let walls = m
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Wall(_)))
        .count();
    assert_eq!(walls, 17, "every wall is native");

    let wall = |guid: &str| {
        let id = creabim_core::ElementId::from_ifc_guid(guid).unwrap();
        match &m.get(id).unwrap().kind {
            ElementKind::Wall(w) => w.clone(),
            k => panic!("{guid} is {k:?}"),
        }
    };
    // IfcOpenShell: x -36.2523..-33.6023, y 104.4143..104.5643, z 0..3.1952
    let w = wall("02QZndWnPCr8pqUFFegmJU");
    assert!((w.thickness - 0.15).abs() < 1e-6);
    assert!((w.height - 3.1952).abs() < 1e-3);
    for p in [w.start, w.end] {
        assert!((p.y - 104.4893).abs() < 1e-3, "{p}");
        assert!(p.x > -36.2523 - 1e-3 && p.x < -33.6023 + 1e-3, "{p}");
    }
    // a wall on the upper storey, its base 0.36 m above that storey
    let w = wall("03RdpMwXDAcxzTBzSbZ55z");
    let level = m.level(w.level).unwrap();
    assert!(
        (level.elevation * 1000.0 - 3140.0).abs() < 1e-3,
        "Level 2 is at 3140 mm"
    );
    assert!((level.elevation + w.base_offset - 3.5).abs() < 1e-3);
    assert!((level.elevation + w.base_offset + w.height - 5.3582).abs() < 1e-3);
    // a column: IfcOpenShell puts it at x -27.7603..-27.4555, y 104.2595..104.5643
    let id = creabim_core::ElementId::from_ifc_guid("20hzUDJSj9yPeKYPrwcRo3").unwrap();
    let ElementKind::Column(c) = &m.get(id).unwrap().kind else {
        panic!("the column is native")
    };
    let centre = glam::DVec2::new((-27.7603 - 27.4555) / 2.0, (104.2595 + 104.5643) / 2.0);
    assert!(c.position.distance(centre) < 1e-3, "{}", c.position);
    let base = m.level(c.level).unwrap().elevation + c.base_offset;
    assert!(base.abs() < 1e-3, "{base}");
    assert!((c.height.unwrap() - 3.1952).abs() < 1e-3);
}

/// Bonsai's test file: IFC4, every body a mapped polygonal face set from a type.
#[test]
fn bonsai_mapped_face_sets() {
    let r = import_ifc(&fixture("thirdparty/bonsai-basic.ifc")).unwrap();
    let c = counts(&r.model);
    assert_eq!(c["IfcBuildingStorey"], 2);
    for class in ["IfcWall", "IfcSlab", "IfcBeam"] {
        assert_eq!(c[class], 1, "{c:?}");
    }
    // every body is a 2 m cube centred on its placement
    for e in r.model.iter() {
        if let ElementKind::FreeMesh(f) = &e.kind {
            assert!((f.mesh.volume() - 8.0).abs() < 1e-9, "{}", e.name);
        }
    }
}
