//! Marks travel as the IFC Tag attribute.

use creabim_core::{sample::sample_house, Document, ElementKind, OpeningKind};
use creabim_ifc::{export_ifc4, import_ifc, ExportOptions};

#[test]
fn marks_round_trip_through_the_ifc_tag() {
    let mut doc = Document::from_model(sample_house());
    let ids: Vec<_> = doc
        .model()
        .iter()
        .filter(|e| matches!(&e.kind, ElementKind::Opening(o) if o.kind == OpeningKind::Door))
        .map(|e| e.id)
        .collect();
    let wall = doc
        .model()
        .iter()
        .find(|e| matches!(e.kind, ElementKind::Wall(_)))
        .unwrap()
        .id;
    doc.edit(ids[0], "Mark", |e| e.mark = Some("D-101".into()))
        .unwrap();
    doc.edit(wall, "Mark", |e| e.mark = Some("WT1".into()))
        .unwrap();
    let ifc = export_ifc4(doc.model(), &ExportOptions::default());
    assert!(ifc.contains("'D-101'"), "the typed mark is the door's Tag");
    // a door without a typed mark writes the number the rules give it
    assert!(ifc.contains("'D01'"));
    assert!(ifc.contains("'WT1'"));
    let back = import_ifc(&ifc).expect("our own export imports").model;
    assert_eq!(back.get(ids[0]).unwrap().mark.as_deref(), Some("D-101"));
    assert_eq!(back.get(ids[1]).unwrap().mark.as_deref(), Some("D01"));
    assert_eq!(back.get(wall).unwrap().mark.as_deref(), Some("WT1"));
    assert_eq!(
        creabim_core::marks::mark_of(&back, ids[0]).as_deref(),
        Some("D-101")
    );
}

#[test]
fn bare_number_tags_from_other_programs_are_not_marks() {
    let ifc = export_ifc4(&sample_house(), &ExportOptions::default());
    // what Revit writes there: its own element number
    let revit = ifc.replace("'D01'", "'312456'");
    let back = import_ifc(&revit).unwrap().model;
    assert!(back
        .iter()
        .filter(|e| matches!(&e.kind, ElementKind::Opening(o) if o.kind == OpeningKind::Door))
        .any(|e| e.mark.is_none()));
}
