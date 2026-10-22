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
    let ElementKind::Wall(w0) = &model.iter().find(|e| e.name == "GF Wall 3").unwrap().kind else {
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

#[test]
fn a_curtain_wall_set_in_a_wall_voids_and_fills_it_through_ifc() {
    use creabim_core::{curtain, Element, WallTop};
    let house = creabim_core::sample::sample_house();
    // a storefront in the middle of the longest wall
    let (host, w) = house
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Wall(w) if w.curtain.is_none() => Some((e.id, w.clone())),
            _ => None,
        })
        .max_by(|a, b| a.1.length().total_cmp(&b.1.length()))
        .unwrap();
    let mid = (w.start + w.end) / 2.0;
    let d = w.direction();
    let mut cw = creabim_core::Wall::new(w.level, mid - d * 1.0, mid + d * 1.0, 0.15, 2.2);
    cw.base_offset = w.base_offset;
    cw.top = WallTop::Unconnected;
    cw.curtain = Some(Default::default());
    let model = Model::from_elements(
        house
            .iter()
            .cloned()
            .chain([Element::new("Storefront", ElementKind::Wall(cw))]),
    )
    .unwrap();
    let voids = curtain::voids_in(&model, host);
    assert_eq!(voids.len(), 1);
    let openings = |m: &Model| {
        m.iter()
            .filter(|e| matches!(e.kind, ElementKind::Opening(_)))
            .count()
    };

    let ifc = export_ifc4(&model, &ExportOptions::default());
    // the opening voids the wall and the curtain wall fills it
    let line_of = |needle: &str| {
        ifc.lines()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no {needle}"))
            .to_string()
    };
    let cw_ref = line_of("IFCCURTAINWALL(").split('=').next().unwrap().to_string();
    let opening_ref = line_of("'Storefront opening'")
        .split('=')
        .next()
        .unwrap()
        .to_string();
    assert!(ifc
        .lines()
        .any(|l| l.contains("IFCRELFILLSELEMENT(") && l.contains(&format!("{opening_ref},{cw_ref})"))));
    assert!(ifc
        .lines()
        .any(|l| l.contains("IFCRELVOIDSELEMENT(") && l.ends_with(&format!(",{opening_ref});"))));

    // back again: the curtain wall is native, the void comes from it, and
    // no stray hole is added
    let back = import_ifc(&ifc).unwrap().model;
    assert_eq!(openings(&back), openings(&model));
    let host_back = back
        .iter()
        .find(|e| e.name == model.get(host).unwrap().name)
        .unwrap()
        .id;
    let back_voids = curtain::voids_in(&back, host_back);
    assert_eq!(back_voids.len(), 1, "{:?}", back_voids);
    assert!((back_voids[0].1.width - voids[0].1.width).abs() < 1e-6);
    assert!((back_voids[0].1.offset - voids[0].1.offset).abs() < 1e-6);
}
