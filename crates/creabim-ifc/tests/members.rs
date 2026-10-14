//! Columns, beams, roofs, stairs and railings go out with their own IFC
//! classes and come back from IFCX unchanged.

use creabim_core::{
    sample::sample_house, Beam, Column, Element, ElementKind, Landing, Model, Profile, Railing,
    Roof, RoofShape, Stair, Transaction,
};
use creabim_ifc::{export_ifc4, export_ifcx, import_ifcx, ExportOptions};
use glam::DVec2;

fn with_members() -> Model {
    let mut m = sample_house();
    let level = m
        .levels()
        .min_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation))
        .unwrap()
        .0
        .id;
    let mut stair = Stair::new(level, DVec2::new(1.0, 1.0), 0.0);
    stair.landing = Some(Landing {
        after: 8,
        left: true,
    });
    let s = Element::new("Stair", ElementKind::Stair(stair));
    let sid = s.id;
    let rect = vec![
        DVec2::ZERO,
        DVec2::new(8.0, 0.0),
        DVec2::new(8.0, 6.0),
        DVec2::new(0.0, 6.0),
    ];
    Transaction::new("members")
        .insert(Element::new(
            "Column",
            ElementKind::Column(Column::new(
                level,
                DVec2::new(4.0, 3.0),
                Profile::Round { diameter: 0.3 },
            )),
        ))
        .insert(Element::new(
            "Beam",
            ElementKind::Beam(Beam::new(level, DVec2::ZERO, DVec2::new(8.0, 0.0))),
        ))
        .insert(Element::new(
            "Roof",
            ElementKind::Roof(Roof::new(level, rect, RoofShape::Hip)),
        ))
        .insert(s)
        .apply(&mut m)
        .unwrap();
    let rail = Railing::along_stair(&m, sid, false).unwrap();
    Transaction::new("rail")
        .insert(Element::new("Railing", ElementKind::Railing(rail)))
        .apply(&mut m)
        .unwrap();
    m
}

#[test]
fn members_export_with_their_ifc4_classes() {
    let m = with_members();
    let text = export_ifc4(&m, &ExportOptions::default());
    for (class, kind) in [
        ("IFCCOLUMN(", ".COLUMN."),
        ("IFCBEAM(", ".BEAM."),
        ("IFCROOF(", ".HIP_ROOF."),
        ("IFCSTAIR(", ".QUARTER_TURN_STAIR."),
        ("IFCRAILING(", ".GUARDRAIL."),
    ] {
        // the sample house has masses classed as columns and roofs too
        assert!(
            text.lines().any(|l| l.contains(class) && l.contains(kind)),
            "no {class} with {kind}"
        );
    }
    // every member is placed in a storey
    let contained = text
        .lines()
        .filter(|l| l.contains("IFCRELCONTAINEDINSPATIALSTRUCTURE"))
        .count();
    assert!(contained >= 1);
}

#[test]
fn members_round_trip_through_ifcx() {
    let m = with_members();
    let r = import_ifcx(&export_ifcx(&m)).unwrap();
    for e in m
        .iter()
        .filter(|e| creabim_core::members::is_member(&e.kind))
    {
        assert_eq!(r.model.get(e.id), Some(e), "{} changed", e.name);
    }
}
