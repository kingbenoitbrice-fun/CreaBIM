//! Strip and pad footings go out as IfcFooting with their predefined
//! types, and come back as footings from IFC4 and IFCX alike.

use creabim_core::sample::typed_house;
use creabim_core::{ElementKind, Footing, FootingKind, Model};
use creabim_ifc::{export_ifc4, export_ifcx, import_ifc, import_ifcx, ExportOptions};

fn footings(m: &Model) -> Vec<(String, Footing)> {
    let mut v: Vec<(String, Footing)> = m
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Footing(f) => Some((e.id.0.to_string(), f.clone())),
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

#[test]
fn footings_export_as_ifc_footings_with_their_types() {
    let m = typed_house();
    let text = export_ifc4(&m, &ExportOptions::default());
    let strips = text
        .lines()
        .filter(|l| l.contains("IFCFOOTING(") && l.contains(".STRIP_FOOTING."))
        .count();
    let pads = text
        .lines()
        .filter(|l| l.contains("IFCFOOTING(") && l.contains(".PAD_FOOTING."))
        .count();
    // a strip under each ground floor wall but the storefront, a pad under the porch column
    assert_eq!((strips, pads), (4, 1));
    assert!(text
        .lines()
        .any(|l| l.contains("IFCFOOTINGTYPE(") && l.contains(".STRIP_FOOTING.")));
    assert!(text
        .lines()
        .any(|l| l.contains("IFCFOOTINGTYPE(") && l.contains(".PAD_FOOTING.")));
    assert!(text.contains("'CreaBIM_Footing'"));
}

#[test]
fn footings_round_trip_through_ifc4() {
    let m = typed_house();
    let before = footings(&m);
    assert_eq!(before.len(), 5);
    let r = import_ifc(&export_ifc4(&m, &ExportOptions::default())).unwrap();
    let after = footings(&r.model);
    assert_eq!(after.len(), before.len(), "{:?}", r.warnings);
    for ((id, a), (id2, b)) in before.iter().zip(&after) {
        assert_eq!(id, id2, "the footing keeps its id");
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.host, b.host, "and stays under the same wall or column");
        for (x, y) in [
            (a.width, b.width),
            (a.length, b.length),
            (a.thickness, b.thickness),
            (a.below, b.below),
            (a.rotation, b.rotation),
        ] {
            assert!((x - y).abs() < 1e-6, "{a:?} vs {b:?}");
        }
        assert!(a.start.distance(b.start) < 1e-6 && a.end.distance(b.end) < 1e-6);
        // and as built, in the same place
        let (ta, tb) = (a.top(&m), b.top(&r.model));
        assert!((ta - tb).abs() < 1e-6, "{ta} vs {tb}");
    }
    // their types come back as footing types
    let kinds: Vec<FootingKind> = r
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::ElementType(t) => match &t.params {
                creabim_core::TypeParams::Footing(f) => Some(f.kind),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert!(kinds.contains(&FootingKind::Strip) && kinds.contains(&FootingKind::Pad));
}

#[test]
fn footings_round_trip_through_ifcx() {
    let m = typed_house();
    let r = import_ifcx(&export_ifcx(&m)).unwrap();
    for e in m
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Footing(_)))
    {
        assert_eq!(r.model.get(e.id), Some(e), "{} changed", e.name);
    }
}
