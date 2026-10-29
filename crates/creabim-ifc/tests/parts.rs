//! Extrusions, sweeps, voids, screen walls and site elements in IFC: the
//! classes they export as, voids as IfcOpeningElements in what they cut,
//! and a native round trip through IFCX.

use creabim_core::garden::garden_house;
use creabim_core::{Element, ElementKind, Model};
use creabim_ifc::{export_ifc4, export_ifcx, import_ifc, import_ifcx, ExportOptions};

fn natives(m: &Model) -> Vec<Element> {
    let mut v: Vec<Element> = m
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Part(_) | ElementKind::Site(_)))
        .cloned()
        .collect();
    v.sort_by_key(|e| e.id.0);
    v
}

fn count(text: &str, entity: &str) -> usize {
    text.lines().filter(|l| l.contains(entity)).count()
}

#[test]
fn parts_and_site_export_as_their_classes() {
    let m = garden_house();
    let text = export_ifc4(&m, &ExportOptions::default());
    // the stack as the class picked for it, the hearth as a proxy
    assert!(text
        .lines()
        .any(|l| l.contains("IFCCHIMNEY(") && l.contains("'Chimney stack'")));
    assert!(text
        .lines()
        .any(|l| l.contains("IFCBUILDINGELEMENTPROXY(") && l.contains("'Hearth'")));
    assert!(text
        .lines()
        .any(|l| l.contains("IFCCOVERING(") && l.contains("'Coping'")));
    // terrain, plants and hard landscape
    assert!(text
        .lines()
        .any(|l| l.contains("IFCGEOGRAPHICELEMENT(") && l.contains(".TERRAIN.")));
    assert!(text
        .lines()
        .any(|l| l.contains("IFCGEOGRAPHICELEMENT(") && l.contains("'Oak'")));
    assert!(text
        .lines()
        .any(|l| l.contains("IFCCIVILELEMENT(") && l.contains("'Patio'")));
    // the firebox is no element of its own: one opening in each thing it cuts
    assert!(!text.contains("'Firebox'"));
    let firebox_openings = text
        .lines()
        .filter(|l| l.contains("IFCOPENINGELEMENT(") && l.contains("'Firebox in "))
        .count();
    assert_eq!(firebox_openings, 2);
    assert!(count(&text, "IFCRELVOIDSELEMENT(") >= 2);
}

#[test]
fn the_step_file_reads_back() {
    let m = garden_house();
    let r = import_ifc(&export_ifc4(&m, &ExportOptions::default())).unwrap();
    // without a native property set yet the parts come back as shapes,
    // but nothing is lost
    for name in [
        "Chimney stack",
        "Coping",
        "Ground",
        "Patio",
        "Oak",
        "Garden screen",
    ] {
        assert!(
            r.model.iter().any(|e| e.name == name),
            "{name}: {:?}",
            r.warnings
        );
    }
}

#[test]
fn parts_and_site_round_trip_natively_through_ifcx() {
    let m = garden_house();
    let r = import_ifcx(&export_ifcx(&m)).unwrap();
    let (before, after) = (natives(&m), natives(&r.model));
    assert_eq!(before.len(), after.len(), "{:?}", r.warnings);
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.kind, b.kind, "{} comes back as the same element", a.name);
    }
    // the coping still follows its wall, and the void still cuts
    let cuts = creabim_core::parts::void_cuts(&r.model);
    assert_eq!(cuts.len(), creabim_core::parts::void_cuts(&m).len());
    let screen = r.model.iter().find(|e| e.name == "Garden screen").unwrap();
    assert!(creabim_core::screen::is_screen(&r.model, screen.id));
}
