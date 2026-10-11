use creabim_core::*;
use creabim_geom::{plan_view, PlanItem};
use glam::DVec2;

#[test]
fn door_splits_wall_into_two_cut_pieces_with_a_swing() {
    let mut doc = Document::new();
    let l = Element::new(
        "L0",
        ElementKind::Level(Level {
            elevation: 0.0,
            height: 3.0,
        }),
    );
    let w = Element::new(
        "W",
        ElementKind::Wall(Wall {
            level: l.id,
            start: DVec2::ZERO,
            end: DVec2::new(5.0, 0.0),
            thickness: 0.2,
            height: 3.0,
            base_offset: 0.0,
            material: None,
        }),
    );
    let d = Element::new(
        "D",
        ElementKind::Opening(Opening {
            host: w.id,
            kind: OpeningKind::Door,
            offset: 2.0,
            sill: 0.0,
            width: 1.0,
            height: 2.1,
        }),
    );
    let lid = l.id;
    doc.commit(Transaction::new("t").insert(l).insert(w).insert(d))
        .unwrap();

    let plan = plan_view(doc.model(), lid, 1.2);
    let cuts: Vec<_> = plan
        .items
        .iter()
        .filter(|i| matches!(i, PlanItem::Fill { .. }))
        .collect();
    assert_eq!(cuts.len(), 2);
    assert!(plan.items.iter().any(|i| matches!(i, PlanItem::Arc { .. })));

    let svg = plan.to_svg(100.0);
    assert!(svg.starts_with("<svg"));
    // two filled pieces; their outlines and the swing are polylines
    assert_eq!(svg.matches("<polygon").count(), 2);
    assert!(svg.matches("<polyline").count() >= 8);
}

#[test]
fn a_mass_pushed_out_of_a_wall_face_shows_in_the_plan() {
    let mut m = sample::sample_house();
    let level = m.levels().next().map(|(e, _)| e.id).unwrap();
    let sketch = Sketch {
        level: Some(level),
        plane: Workplane::facing(glam::DVec3::new(-0.15, 1.35, 2.5), -glam::DVec3::X),
        points: vec![
            DVec2::ZERO,
            DVec2::new(0.0, -2.0),
            DVec2::new(1.5, -2.0),
            DVec2::new(1.5, 0.0),
        ],
        closed: true,
    };
    let mass = Element::new(
        "Mass",
        ElementKind::FreeMesh(FreeMesh::new(
            Some(level),
            sketch.plane.origin,
            sketch.extrude(1.0),
        )),
    );
    let id = mass.id;
    let mut doc = Document::from_model(m);
    doc.commit(Transaction::new("t").insert(mass)).unwrap();
    m = doc.model().clone();
    let plan = plan_view(&m, level, 1.2);
    let lines = plan.items.iter().filter(|i| i.id() == id).count();
    assert!(lines >= 4, "{lines} lines");
}

#[test]
fn a_box_subtracted_from_a_wall_leaves_a_gap_in_the_plan() {
    let mut doc = Document::new();
    let l = Element::new(
        "L0",
        ElementKind::Level(Level {
            elevation: 0.0,
            height: 3.0,
        }),
    );
    let w = Element::new(
        "W",
        ElementKind::Wall(Wall {
            level: l.id,
            start: DVec2::ZERO,
            end: DVec2::new(5.0, 0.0),
            thickness: 0.2,
            height: 3.0,
            base_offset: 0.0,
            material: None,
        }),
    );
    // a 1 m box through the wall, 0.5 to 2.5 m up
    let b = Element::new(
        "Box",
        ElementKind::FreeMesh(FreeMesh::new(
            Some(l.id),
            glam::DVec3::new(2.5, 0.0, 0.5),
            EditMesh::cuboid(glam::DVec3::new(1.0, 1.0, 2.0)),
        )),
    );
    let c = Element::new(
        "Cut",
        ElementKind::Cut(Cut {
            target: w.id,
            cutter: b.id,
            op: BoolOp::Difference,
        }),
    );
    let (lid, wid, bid) = (l.id, w.id, b.id);
    doc.commit(
        Transaction::new("t")
            .insert(l)
            .insert(w)
            .insert(b)
            .insert(c),
    )
    .unwrap();

    let plan = plan_view(doc.model(), lid, 1.2);
    let fills: Vec<_> = plan
        .items
        .iter()
        .filter(|i| matches!(i, PlanItem::Fill { id, .. } if *id == wid))
        .collect();
    assert_eq!(fills.len(), 2, "the wall is cut in two");
    assert!(
        plan.items.iter().all(|i| i.id() != bid),
        "cutters are not drawn"
    );

    // above the box the wall is whole again
    let plan = plan_view(doc.model(), lid, 2.8);
    let fills = plan
        .items
        .iter()
        .filter(|i| matches!(i, PlanItem::Fill { id, .. } if *id == wid))
        .count();
    assert_eq!(fills, 1);
}
