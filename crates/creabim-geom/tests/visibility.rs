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
