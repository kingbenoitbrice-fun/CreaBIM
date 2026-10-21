use creabim_core::*;
use creabim_geom::section::view_drawing;
use std::collections::HashSet;

/// The model's first plan view, with `hide` applied to what it leaves out.
fn plan_with(model: &Model, hide: impl FnOnce(&mut ViewHidden)) -> (Model, ElementId) {
    let mut m = model.clone();
    let (view, _) = m
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::View(v) if matches!(v.kind, ViewKind::Plan { .. }) => Some((e.id, v)),
            _ => None,
        })
        .unwrap();
    let before = m.get(view).unwrap().clone();
    let mut e = before.clone();
    if let ElementKind::View(v) = &mut e.kind {
        hide(&mut v.hidden);
    }
    Transaction::new("Hide")
        .replace(before, e)
        .apply(&mut m)
        .unwrap();
    (m, view)
}

fn ids(model: &Model, view: ElementId) -> HashSet<ElementId> {
    view_drawing(model, view)
        .unwrap()
        .items
        .iter()
        .map(|i| i.id())
        .collect()
}

fn of_kind(model: &Model, f: impl Fn(&ElementKind) -> bool) -> Vec<ElementId> {
    model.iter().filter(|e| f(&e.kind)).map(|e| e.id).collect()
}

#[test]
fn plans_leave_out_hidden_categories_and_elements() {
    let house = sample::sample_house();
    let (m, view) = plan_with(&house, |_| {});
    let shown = ids(&m, view);
    let grids = of_kind(&m, |k| matches!(k, ElementKind::Grid(_)));
    let walls = of_kind(&m, |k| matches!(k, ElementKind::Wall(_)));
    assert!(grids.iter().all(|g| shown.contains(g)));
    assert!(walls.iter().any(|w| shown.contains(w)));

    // no grids
    let (m, view) = plan_with(&house, |h| h.set_category(Category::Grids, true));
    let shown = ids(&m, view);
    assert!(grids.iter().all(|g| !shown.contains(g)));
    assert!(walls.iter().any(|w| shown.contains(w)));

    // one wall less, and its doors and windows with it
    let wall = *walls.iter().find(|w| shown.contains(w)).unwrap();
    let (m, view) = plan_with(&house, |h| h.set_element(wall, true));
    let shown = ids(&m, view);
    assert!(!shown.contains(&wall));
    for d in m.dependents(wall) {
        assert!(!shown.contains(&d));
    }
    assert!(grids.iter().all(|g| shown.contains(g)));
}

#[test]
fn hidden_windows_leave_their_holes() {
    let house = sample::sample_house();
    let (m, view) = plan_with(&house, |h| h.set_category(Category::Windows, true));
    let d = view_drawing(&m, view).unwrap();
    let windows = of_kind(
        &m,
        |k| matches!(k, ElementKind::Opening(o) if o.kind == OpeningKind::Window),
    );
    assert!(d.items.iter().all(|i| !windows.contains(&i.id())));
    // the walls are still cut where the windows were: as many wall pieces
    // as with the windows shown
    let (full, view) = plan_with(&house, |_| {});
    let pieces = |m: &Model, view| {
        view_drawing(m, view)
            .unwrap()
            .items
            .iter()
            .filter(|i| matches!(m.get(i.id()).map(|e| &e.kind), Some(ElementKind::Wall(_))))
            .count()
    };
    assert_eq!(pieces(&m, view), pieces(&full, view));
}

/// The sample house with a slim beam inside one of its walls, below the
/// plans' cut, its plan, and an elevation looking at that wall.
fn beam_in_a_wall() -> (Model, ElementId, ElementId, ElementId) {
    let mut m = sample::sample_house();
    let (wall_level, start, end) = m
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::Wall(w) if w.length() > 3.0 => Some((w.level, w.start, w.end)),
            _ => None,
        })
        .unwrap();
    let mut beam = Beam::new(wall_level, start, end);
    beam.width = 0.08;
    beam.depth = 0.2;
    let z0 = m.level(wall_level).unwrap().elevation;
    beam.top_offset = beam.top(&m) - (z0 + 1.0);
    let beam = Element::new("Hidden beam", ElementKind::Beam(beam));
    let plan = m
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::View(View {
                kind: ViewKind::Plan { level, .. },
                ..
            }) if *level == wall_level => Some(e.id),
            _ => None,
        })
        .unwrap();
    // three metres off the wall's right side, looking back at it
    let d = (end - start).normalize();
    let out = -d.perp() * 3.0;
    let elevation = Element::new(
        "Elevation at the wall",
        ElementKind::View(View::section(start + out, end + out, 6.0, false)),
    );
    let (beam_id, elevation_id) = (beam.id, elevation.id);
    Transaction::new("Set up")
        .insert(beam)
        .insert(elevation)
        .apply(&mut m)
        .unwrap();
    (m, beam_id, plan, elevation_id)
}

/// How many dashed hidden lines of `id` the view draws.
fn hidden_lines_of(model: &Model, view: ElementId, id: ElementId) -> usize {
    view_drawing(model, view)
        .unwrap()
        .items
        .iter()
        .filter(|i| {
            matches!(i, creabim_geom::PlanItem::Line { id: l, pen: creabim_geom::plan::Pen::Hidden, .. } if *l == id)
        })
        .count()
}

fn show_hidden(model: &mut Model, view: ElementId) {
    let before = model.get(view).unwrap().clone();
    let mut e = before.clone();
    if let ElementKind::View(v) = &mut e.kind {
        v.show_hidden = true;
    }
    Transaction::new("Show hidden elements")
        .replace(before, e)
        .apply(model)
        .unwrap();
}

#[test]
fn views_showing_hidden_elements_draw_a_beam_inside_a_wall_dashed() {
    let (mut m, beam, plan, elevation) = beam_in_a_wall();
    // as usual the wall covers the beam
    assert_eq!(hidden_lines_of(&m, plan, beam), 0);
    assert_eq!(hidden_lines_of(&m, elevation, beam), 0);
    // in a plan, under the wall's cut
    show_hidden(&mut m, plan);
    assert!(hidden_lines_of(&m, plan, beam) > 0);
    // in an elevation, behind the wall's face
    show_hidden(&mut m, elevation);
    assert!(hidden_lines_of(&m, elevation, beam) > 0);
    // doors and windows in their holes are not drawn again
    for o in of_kind(&m, |k| matches!(k, ElementKind::Opening(_))) {
        assert_eq!(hidden_lines_of(&m, elevation, o), 0);
        assert_eq!(hidden_lines_of(&m, plan, o), 0);
    }
}
