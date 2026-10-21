use creabim_core::*;
use creabim_geom::annotate::{centre, tag_text};
use creabim_geom::section::view_drawing;
use creabim_geom::PlanItem;

/// The roof, columns, beams, stairs and railings of a model.
fn members(model: &Model) -> Vec<ElementId> {
    model
        .iter()
        .filter(|e| members::is_member(&e.kind))
        .map(|e| e.id)
        .collect()
}

#[test]
fn roofs_and_other_members_have_a_place_and_a_text_for_their_tags() {
    let m = sample::layered_house();
    let ids = members(&m);
    assert!(ids
        .iter()
        .any(|id| matches!(m.get(*id).unwrap().kind, ElementKind::Roof(_))));
    for id in ids {
        assert!(centre(&m, id).is_some(), "{}", m.get(id).unwrap().name);
        assert!(!tag_text(&m, id).0.is_empty());
    }
}

#[test]
fn a_tag_on_the_roof_shows_in_a_section() {
    let mut m = sample::layered_house();
    let roof = m
        .iter()
        .find(|e| matches!(e.kind, ElementKind::Roof(_)))
        .unwrap()
        .id;
    let section = m
        .iter()
        .find(|e| {
            matches!(&e.kind, ElementKind::View(v) if matches!(v.kind, ViewKind::Section { cut: true, .. }))
        })
        .unwrap()
        .id;
    let tag = Element::new(
        "Tag",
        ElementKind::Tag(Tag::new(section, roof, glam::DVec2::new(1.0, 1.0))),
    );
    let id = tag.id;
    Transaction::new("Tag the roof")
        .insert(tag)
        .apply(&mut m)
        .unwrap();
    let d = view_drawing(&m, section).unwrap();
    let text = tag_text(&m, roof).0;
    assert!(text.contains('°'), "{text}");
    assert!(d
        .items
        .iter()
        .any(|i| matches!(i, PlanItem::Tag { id: t, .. } if *t == id)));
}
