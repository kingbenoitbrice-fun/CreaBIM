//! Element types, curtain walls, bare openings and slab holes through IFC.

use creabim_core::sample::typed_house;
use creabim_core::{types, ElementKind, Model, OpeningKind};
use creabim_ifc::{export_ifc4, export_ifcx, import_ifc, import_ifcx, ExportOptions};

/// (instance name, type name) for every typed element, sorted.
fn typed(m: &Model) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = m
        .iter()
        .filter_map(|e| {
            let (t, _) = types::type_of(m, e)?;
            Some((e.name.clone(), t.name.clone()))
        })
        .collect();
    v.sort();
    v
}

fn type_names(m: &Model) -> Vec<String> {
    let mut v: Vec<String> = types::types(m)
        .into_iter()
        .map(|(_, n, t)| format!("{n} {:?}", t.category()))
        .collect();
    v.sort();
    v
}

#[test]
fn typed_house_round_trips_through_ifc() {
    let model = typed_house();
    let before = typed(&model);
    assert!(before.len() > 20, "{before:?}");
    let ifc = export_ifc4(&model, &ExportOptions::default());
    for class in [
        "IFCWALLTYPE(",
        "IFCCURTAINWALLTYPE(",
        "IFCDOORTYPE(",
        "IFCWINDOWTYPE(",
        "IFCSLABTYPE(",
        "IFCROOFTYPE(",
        "IFCCOLUMNTYPE(",
        "IFCBEAMTYPE(",
        "IFCRELDEFINESBYTYPE(",
        "IFCCURTAINWALL(",
        "IFCMEMBER(",
        "IFCPLATE(",
    ] {
        assert!(ifc.contains(class), "{class}");
    }
    let back = import_ifc(&ifc).unwrap().model;
    // every type comes back, and every instance is of the same type
    assert_eq!(type_names(&back), type_names(&model));
    assert_eq!(typed(&back), before);
    // with no overrides, as none were made
    for e in back.iter() {
        if let Some(r) = &e.of_type {
            assert!(r.overrides.is_empty(), "{} {:?}", e.name, r.overrides);
        }
    }
    // the storefront is a native curtain wall again, with its door
    let curtain = back
        .iter()
        .find(|e| matches!(&e.kind, ElementKind::Wall(w) if w.curtain.is_some()))
        .expect("a curtain wall");
    let ElementKind::Wall(w) = &curtain.kind else {
        unreachable!()
    };
    let ElementKind::Wall(w0) = &model
        .iter()
        .find(|e| e.name == "GF Wall 3")
        .unwrap()
        .kind
    else {
        unreachable!()
    };
    assert_eq!(w.curtain, w0.curtain);
    assert!((w.start - w0.start).length() < 1e-6 && (w.end - w0.end).length() < 1e-6);
    assert!(back.iter().any(
        |e| matches!(&e.kind, ElementKind::Opening(o) if o.host == curtain.id && o.kind == OpeningKind::Door)
    ));
    // no curtain wall parts come in as loose elements
    assert!(!back.iter().any(|e| e.name.contains("mullion")));
    // the bare wall opening is still a hole
    let hole = back
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::Opening(o) if o.kind == OpeningKind::Hole => Some(o),
            _ => None,
        })
        .expect("a bare opening");
    assert!((hole.width - 0.6).abs() < 1e-6 && (hole.sill - 2.2).abs() < 1e-6);
    // the first floor slab has its own flue hole and the shaft's
    let ff = back
        .iter()
        .find(|e| e.name == "FF Slab")
        .expect("the first floor slab");
    let ElementKind::Slab(s) = &ff.kind else {
        panic!("a slab")
    };
    assert_eq!(s.holes.len(), 2, "{:?}", s.holes);
}

#[test]
fn typed_house_round_trips_through_ifcx() {
    let model = typed_house();
    let back = import_ifcx(&export_ifcx(&model)).unwrap().model;
    assert_eq!(type_names(&back), type_names(&model));
    assert_eq!(typed(&back), typed(&model));
}
