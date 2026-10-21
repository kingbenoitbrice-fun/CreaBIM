//! Wall behaviour through IFC: a wall's axis is its location line (with
//! the layer set usage measured from it), a top attached to a roof is
//! clipped by its underside, a top tied to a level gives the height the
//! level makes, and doors say which way they are hung.

use creabim_core::{
    sample::{layered_house, sample_house},
    Document, Element, ElementId, ElementKind, LocationLine, Model, Wall, WallTop,
};
use creabim_ifc::{export_ifc4, import_ifc, ExportOptions};

fn named(m: &Model, name: &str) -> ElementId {
    m.iter().find(|e| e.name == name).expect(name).id
}

fn edit(m: &Model, id: ElementId, f: impl FnOnce(&mut Element)) -> Model {
    let mut doc = Document::from_model(m.clone());
    doc.edit(id, "edit", f).unwrap();
    doc.model().clone()
}

fn wall(m: &Model, name: &str) -> Wall {
    match &m.get(named(m, name)).unwrap().kind {
        ElementKind::Wall(w) => w.clone(),
        _ => unreachable!(),
    }
}

#[test]
fn a_walls_axis_is_its_location_line() {
    let m = layered_house();
    let id = named(&m, "GF Wall 1");
    let m = edit(&m, id, |e| {
        if let ElementKind::Wall(w) = &mut e.kind {
            w.location = LocationLine::ExteriorFace;
        }
    });
    let w = wall(&m, "GF Wall 1");
    let t = w.thickness / 2.0;
    let ifc = export_ifc4(&m, &ExportOptions::default());
    // its layers start at the axis now: no offset to the exterior face
    assert!(
        ifc.contains(".AXIS2.,.POSITIVE.,0.,$"),
        "the usage starts at the axis"
    );
    let back = import_ifc(&ifc).unwrap().model;
    let b = wall(&back, "GF Wall 1");
    assert_eq!(b.location, LocationLine::ExteriorFace);
    assert!(b.start.distance(w.start) < 1e-6 && b.end.distance(w.end) < 1e-6);
    assert!((b.thickness - 2.0 * t).abs() < 1e-6);
    // the others still sit on their centrelines
    assert_eq!(wall(&back, "GF Wall 2").location, LocationLine::Centre);
}

#[test]
fn walls_attached_to_the_roof_are_clipped_by_it() {
    let m = layered_house();
    assert_eq!(
        m.iter()
            .filter(|e| matches!(&e.kind, ElementKind::Wall(w) if !w.attach_top.is_empty()))
            .count(),
        4
    );
    let ifc = export_ifc4(&m, &ExportOptions::default());
    assert!(ifc.contains("'Body','Clipping'"));
    assert!(ifc.matches("IFCPOLYGONALBOUNDEDHALFSPACE(").count() >= 4);
    // they still come back as walls, as tall as their gables
    let back = import_ifc(&ifc).unwrap();
    let gable = wall(&back.model, "FF Wall 2");
    assert!(gable.height > 4.5, "{}", gable.height);
}

#[test]
fn a_top_tied_to_a_level_exports_the_height_it_makes() {
    let m = sample_house();
    let first = named(&m, "First Floor");
    let id = named(&m, "GF Wall 1");
    let m = edit(&m, id, |e| {
        if let ElementKind::Wall(w) = &mut e.kind {
            w.top = WallTop::UpToLevel {
                level: first,
                offset: -0.25,
            };
        }
    });
    let mut doc = Document::from_model(m);
    doc.commit(creabim_core::levels::set(doc.model(), first, 3.4, 3.0))
        .unwrap();
    let w = wall(doc.model(), "GF Wall 1");
    assert!((w.height - 3.15).abs() < 1e-9);
    let back = import_ifc(&export_ifc4(doc.model(), &ExportOptions::default()))
        .unwrap()
        .model;
    assert!((wall(&back, "GF Wall 1").height - 3.15).abs() < 1e-6);
}

#[test]
fn doors_say_which_way_they_are_hung() {
    let m = sample_house();
    let ifc = export_ifc4(&m, &ExportOptions::default());
    let left = ifc.matches(".SINGLE_SWING_LEFT.").count();
    let id = named(&m, "Front Door");
    let m = edit(&m, id, |e| {
        if let ElementKind::Opening(o) = &mut e.kind {
            o.flip_hand = true;
        }
    });
    let ifc = export_ifc4(&m, &ExportOptions::default());
    assert_eq!(ifc.matches(".SINGLE_SWING_LEFT.").count(), left - 1);
    assert_eq!(ifc.matches(".SINGLE_SWING_RIGHT.").count(), 1);
    // opening out as well, it is turned round: seen the way it swings
    // its hinge is on the left again
    let m = edit(&m, id, |e| {
        if let ElementKind::Opening(o) = &mut e.kind {
            o.flip_facing = true;
        }
    });
    let ifc = export_ifc4(&m, &ExportOptions::default());
    assert_eq!(ifc.matches(".SINGLE_SWING_LEFT.").count(), left);
    // and the door still comes back in its wall
    let back = import_ifc(&ifc).unwrap().model;
    let o = back
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::Opening(o) if e.name == "Front Door" => Some(o.clone()),
            _ => None,
        })
        .unwrap();
    assert!((o.width - 1.0).abs() < 1e-6 && (o.offset - 1.6).abs() < 1e-6);
}
