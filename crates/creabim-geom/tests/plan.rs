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
