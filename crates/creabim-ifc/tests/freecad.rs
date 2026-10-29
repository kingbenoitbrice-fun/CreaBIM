//! Golden import tests for files the way FreeCAD writes them: faceted
//! B-rep walls and slabs with their holes cut, windows with and without
//! opening relationships, multi-layer walls along a polyline axis and
//! furniture as meshes (`fixtures/freecad-style-house.ifc`, written by
//! `fixtures/make_freecad_style.py`). Also our own starter house, out to
//! IFC4 and back, keeps every native element; and, when
//! `CREABIM_OSA_M01` names it, the OSA-M01 FreeCAD export of the import
//! flaw report (a private file, so not committed).

use creabim_core::{starter::starter_house, ElementKind, Model, OpeningKind, ScheduleCategory};
use creabim_ifc::{export_ifc4, import_ifc, ExportOptions};
use std::collections::BTreeMap;

fn fixture(name: &str) -> String {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Native elements by kind (openings by door, window or hole), and free
/// meshes by IFC class as "mesh IfcWall".
fn kinds(m: &Model) -> BTreeMap<String, usize> {
    let mut c = BTreeMap::new();
    for e in m.iter() {
        let key = match &e.kind {
            ElementKind::FreeMesh(f) => format!("mesh {}", f.ifc_class),
            ElementKind::Opening(o) => format!("{:?}", o.kind),
            k => k.name().to_string(),
        };
        *c.entry(key).or_insert(0) += 1;
    }
    c
}

fn n(c: &BTreeMap<String, usize>, k: &str) -> usize {
    c.get(k).copied().unwrap_or(0)
}

#[test]
fn freecad_style_breps_become_native_walls_slabs_and_windows() {
    let r = import_ifc(&fixture("freecad-style-house.ifc")).unwrap();
    let m = &r.model;
    let c = kinds(m);
    let why = format!("{c:?}\n{}", r.details.join("\n"));
    // six straight walls, and the loggia's wall as its three runs
    assert_eq!(n(&c, "Wall"), 9, "{why}");
    // the L-shaped garden wall and the gable end stay meshes
    assert_eq!(n(&c, "mesh IfcWall"), 2, "{why}");
    assert_eq!(n(&c, "Slab"), 3, "{why}");
    assert_eq!(n(&c, "mesh IfcSlab"), 1, "the sloped roof slab: {why}");
    // W1..W4 and W6 in native walls, by relationship or by position; W5
    // is in the gable, which is a mesh
    assert_eq!(n(&c, "Window"), 5, "{why}");
    assert_eq!(n(&c, "mesh IfcWindow"), 1, "{why}");
    assert_eq!(n(&c, "Door"), 2, "{why}");
    // the service hole; W2, W3 and D2 fill the holes cut in their walls
    assert_eq!(n(&c, "Hole"), 1, "{why}");
    assert_eq!(n(&c, "Column"), 1, "{why}");
    assert_eq!(n(&c, "Beam"), 1, "{why}");
    assert_eq!(n(&c, "mesh IfcFurnishingElement"), 3, "{why}");
    let windows = creabim_geom::schedule::schedule(m, ScheduleCategory::Windows);
    assert_eq!(windows.listed, 5, "the window schedule lists them");

    let named = |name: &str| {
        m.iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("no {name}: {why}"))
    };
    // the wall's own hole sizes W2, found by its position
    let ElementKind::Opening(w2) = &named("W2").kind else {
        panic!("W2 is not native")
    };
    let host = &m.get(w2.host).unwrap().name;
    assert_eq!(host, "Wall-Ext-S");
    assert!((w2.width - 1.2).abs() < 1e-6 && (w2.height - 1.2).abs() < 1e-6);
    assert!((w2.sill - 0.9).abs() < 1e-6, "{}", w2.sill);
    assert!((w2.offset - 5.15).abs() < 1e-6, "{}", w2.offset);
    // the wall: 300 thick, 2.75 high, along y = 0
    let ElementKind::Wall(s) = &named("Wall-Ext-S").kind else {
        panic!()
    };
    assert!((s.thickness - 0.3).abs() < 1e-6 && (s.height - 2.75).abs() < 1e-6);
    assert!(s.start.y.abs() < 1e-6 && s.end.y.abs() < 1e-6);
    // W6 fills an opening and a hole cut in its wall: one window, no hole
    let ff_s = named("Wall-FF-S").id;
    let in_ff_s: Vec<_> = m
        .iter()
        .filter(|e| matches!(&e.kind, ElementKind::Opening(o) if o.host == ff_s))
        .collect();
    assert_eq!(in_ff_s.len(), 1, "{:?}", in_ff_s);
    // the terrace's top is 450 above the ground floor, with no storey there
    let ElementKind::Slab(t) = &named("Slab-Terrace").kind else {
        panic!()
    };
    assert!((t.top_offset - 0.45).abs() < 1e-6 && (t.thickness - 0.15).abs() < 1e-6);
    let ElementKind::Slab(ff) = &named("Slab-FF").kind else {
        panic!()
    };
    assert_eq!(ff.holes.len(), 1, "the stair hole");
    // the loggia's wall: three runs of the two layers' 250
    let runs: Vec<_> = m
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Wall(w) if e.name.starts_with("Wall-Loggia") => Some(w),
            _ => None,
        })
        .collect();
    assert_eq!(runs.len(), 3);
    assert!(runs.iter().all(|w| (w.thickness - 0.25).abs() < 1e-6));
    assert!(runs[0].end.distance(runs[1].start) < 1e-6, "the runs meet");
    // the gable's window opening is cut into its mesh
    let ElementKind::FreeMesh(g) = &named("Wall-Gable-E").kind else {
        panic!()
    };
    let full = 6.7 * (1.2 + 2.4 / 2.0) * 0.3;
    let cut = full - 1.2 * 1.2 * 0.3;
    assert!(
        (g.mesh.volume() - cut).abs() < 1e-6,
        "{} vs {cut}",
        g.mesh.volume()
    );

    // every element the import left as a mesh is named, with why
    for d in &r.details {
        assert!(d.contains('(') && d.contains(')'), "{d}");
    }
    let gable = r
        .details
        .iter()
        .find(|d| d.contains("Wall-Gable-E"))
        .unwrap();
    assert!(gable.starts_with("IfcWall 'Wall-Gable-E' ("), "{gable}");
    assert!(gable.contains("sloped faces"), "{gable}");
}

#[test]
fn the_starter_house_round_trips_natively_through_ifc4() {
    let house = starter_house();
    let r = import_ifc(&export_ifc4(&house, &ExportOptions::default())).unwrap();
    let (before, after) = (kinds(&house), kinds(&r.model));
    for (k, v) in &before {
        let building = !matches!(
            k.as_str(),
            "View" | "Sheet" | "Viewport" | "Dimension" | "Tag" | "Note" | "TextStyle"
                | "SheetTemplate" | "Guide" | "CropBox" | "Units" | "Sketch" | "Cut"
                // IFC4 export writes no grids, and a shaft goes out as the
                // holes it cuts (known gaps); layered materials add a few
                | "Grid" | "Shaft" | "Material"
        );
        if building {
            assert_eq!(
                n(&after, k),
                *v,
                "{k}: {before:?}\n{after:?}\n{:?}",
                r.details
            );
        }
    }
    assert!(!after.keys().any(|k| k.starts_with("mesh ")), "{after:?}");
    // and the members are the same members
    for e in house
        .iter()
        .filter(|e| creabim_core::members::is_member(&e.kind))
    {
        let back = r
            .model
            .get(e.id)
            .unwrap_or_else(|| panic!("{} lost", e.name));
        assert_eq!(back.kind.name(), e.kind.name(), "{}", e.name);
    }
}

/// The OSA-M01 FreeCAD export of the import flaw report (2026-10-28):
/// before this pass 7 of its 13 walls, 1 of its 4 slabs, 1 of its 3 doors
/// and none of its 11 windows came in native.
#[test]
fn osa_m01_freecad_export_when_given() {
    let Ok(path) = std::env::var("CREABIM_OSA_M01") else {
        eprintln!("set CREABIM_OSA_M01 to the OSA-M01 IFC file to run this");
        return;
    };
    let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
    let r = import_ifc(&text).unwrap();
    let c = kinds(&r.model);
    let why = format!("{c:?}\n{}", r.details.join("\n"));
    assert_eq!(n(&c, "Window"), 11, "{why}");
    assert_eq!(n(&c, "Door"), 3, "{why}");
    assert_eq!(n(&c, "Slab"), 4, "{why}");
    assert!(n(&c, "mesh IfcWall") <= 2, "{why}");
    assert!(n(&c, "Wall") >= 20, "{why}");
    let windows = creabim_geom::schedule::schedule(&r.model, ScheduleCategory::Windows);
    assert_eq!(windows.listed, 11);
    assert!(r
        .model
        .iter()
        .all(|e| !matches!(&e.kind, ElementKind::Opening(o) if o.kind == OpeningKind::Window && o.width <= 0.0)));
}
