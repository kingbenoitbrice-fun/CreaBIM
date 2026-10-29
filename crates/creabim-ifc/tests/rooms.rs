//! Rooms go out as IfcSpace (bounded by their walls, covered by their
//! ceilings) and ceilings as IfcCovering CEILING, and both come back from
//! IFC4 and IFCX with their areas.

use creabim_core::starter::starter_house;
use creabim_core::{room, ElementKind, Model};
use creabim_ifc::{export_ifc4, export_ifcx, import_ifc, import_ifcx, ExportOptions};

/// Each room's number, name, area and drawn-or-found, by number.
fn rooms(m: &Model) -> Vec<(String, String, f64, bool)> {
    let mut v: Vec<(String, String, f64, bool)> = m
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Room(r) => Some((
                r.number.clone(),
                e.name.clone(),
                room::area(m, e.id),
                r.is_drawn(),
            )),
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn ceilings(m: &Model) -> (usize, usize) {
    let all: Vec<&creabim_core::Ceiling> = m
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Ceiling(c) => Some(c),
            _ => None,
        })
        .collect();
    (all.len(), all.iter().filter(|c| c.room.is_some()).count())
}

#[test]
fn rooms_export_as_spaces_with_boundaries_and_ceilings_as_coverings() {
    let m = starter_house();
    let text = export_ifc4(&m, &ExportOptions::default());
    let count = |what: &str| text.lines().filter(|l| l.contains(what)).count();
    assert_eq!(count("IFCSPACE("), 16);
    assert_eq!(count("IFCCOVERING("), 9);
    assert!(text
        .lines()
        .all(|l| !l.contains("IFCCOVERING(") || l.contains(".CEILING.")));
    // the study has four walls round it
    assert!(count("IFCRELSPACEBOUNDARY(") >= 4 * 13);
    assert_eq!(count("IFCRELCOVERSSPACES("), 9);
    assert!(text.contains("'Qto_SpaceBaseQuantities'"));
    assert!(text.contains("'Pset_SpaceCoveringRequirements'"));
    // the living room's LongName, its number as Name
    assert!(text.lines().any(|l| l.contains("IFCSPACE(")
        && l.contains("'0.06'")
        && l.contains("'Living / Dining'")));
}

#[test]
fn rooms_and_ceilings_round_trip_through_ifc4() {
    let m = starter_house();
    let before = rooms(&m);
    let r = import_ifc(&export_ifc4(&m, &ExportOptions::default())).unwrap();
    let after = rooms(&r.model);
    assert_eq!(after.len(), before.len(), "{:?}", r.warnings);
    for (a, b) in before.iter().zip(&after) {
        assert_eq!((&a.0, &a.1, a.3), (&b.0, &b.1, b.3));
        // the stair well comes back as a hole in the floor, not a shaft,
        // so the landing's area no longer leaves it out
        if a.1 != "Landing" {
            assert!((a.2 - b.2).abs() < 0.01, "{} {} vs {}", a.1, a.2, b.2);
        }
    }
    assert_eq!(ceilings(&r.model), ceilings(&m));
    assert!(
        !r.warnings.iter().any(|w| w.contains("space")),
        "{:?}",
        r.warnings
    );
}

#[test]
fn rooms_and_ceilings_round_trip_through_ifcx() {
    let m = starter_house();
    let r = import_ifcx(&export_ifcx(&m)).unwrap();
    let (before, after) = (rooms(&m), rooms(&r.model));
    assert_eq!(after.len(), before.len());
    for (a, b) in before.iter().zip(&after) {
        assert_eq!((&a.0, &a.1, a.3), (&b.0, &b.1, b.3));
        assert!((a.2 - b.2).abs() < 1e-6);
    }
    assert_eq!(ceilings(&r.model), ceilings(&m));
}
