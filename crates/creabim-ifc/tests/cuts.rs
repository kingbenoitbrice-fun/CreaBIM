//! Boolean cuts in IFC: the cutter stays out, the cut wall goes out as its
//! final shape.

use creabim_core::*;
use glam::{DVec2, DVec3};

#[test]
fn a_cut_wall_exports_its_final_shape_without_the_cutter() {
    let mut m = sample::sample_house();
    let (wall, level) = m
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::Wall(w) => Some((e.id, w.level)),
            _ => None,
        })
        .unwrap();
    let w = match &m.get(wall).unwrap().kind {
        ElementKind::Wall(w) => w.clone(),
        _ => unreachable!(),
    };
    let at = (w.start + w.end) / 2.0;
    let cutter = Element::new(
        "Niche",
        ElementKind::FreeMesh(FreeMesh::new(
            Some(level),
            DVec3::new(at.x, at.y, 0.5),
            EditMesh::cuboid(DVec3::new(1.0, 1.0, 1.0)),
        )),
    );
    let cut = Element::new(
        "Cut",
        ElementKind::Cut(Cut {
            target: wall,
            cutter: cutter.id,
            op: BoolOp::Difference,
        }),
    );
    let guide = Element::new(
        "Guide",
        ElementKind::Guide(Guide {
            level: Some(level),
            a: DVec2::ZERO.extend(0.0),
            b: DVec2::X.extend(0.0),
        }),
    );
    let cutter_guid = cutter.id.ifc_guid();
    let mut doc = Document::from_model(m);
    doc.commit(
        Transaction::new("t")
            .insert(cutter)
            .insert(cut)
            .insert(guide),
    )
    .unwrap();
    m = doc.model().clone();
    let ifc = creabim_ifc::export_ifc4(&m, &Default::default());
    assert!(!ifc.contains(&cutter_guid), "the cutter is not exported");
    let wall_line = ifc
        .lines()
        .find(|l| l.contains(&m.get(wall).unwrap().id.ifc_guid()))
        .unwrap();
    assert!(wall_line.contains("IFCWALL"));
    assert!(ifc.contains("'Tessellation'"));
    if let Ok(out) = std::env::var("CREABIM_CUT_IFC") {
        std::fs::write(out, &ifc).unwrap();
    }
}
