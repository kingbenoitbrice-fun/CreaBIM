//! Managing levels: adding, duplicating, renaming, moving and deleting
//! them. Each function builds one [`Transaction`], so every change is a
//! single undo step.
//!
//! Elements hang off their level with offsets relative to it, so moving a
//! level moves everything on it. Deleting a level either deletes what is on
//! it or moves those elements to another level, keeping them where they
//! are in space (their offsets change by the difference in elevation).

use crate::{Element, ElementId, ElementKind, Level, Model, Transaction, View, ViewKind};

/// The project's levels, lowest first: (id, name, level).
pub fn sorted(model: &Model) -> Vec<(ElementId, String, Level)> {
    let mut v: Vec<(ElementId, String, Level)> = model
        .levels()
        .map(|(e, l)| (e.id, e.name.clone(), l.clone()))
        .collect();
    v.sort_by(|a, b| a.2.elevation.total_cmp(&b.2.elevation));
    v
}

/// A name no level has yet: "Level 2", "Level 3"...
pub fn next_name(model: &Model) -> String {
    let taken = |n: &str| model.levels().any(|(e, _)| e.name == n);
    let mut i = model.levels().count();
    loop {
        let name = format!("Level {i}");
        if !taken(&name) {
            return name;
        }
        i += 1;
    }
}

/// The plan views of a level.
pub fn plan_views(model: &Model, level: ElementId) -> Vec<ElementId> {
    model
        .iter()
        .filter(|e| {
            matches!(&e.kind, ElementKind::View(View { kind: ViewKind::Plan { level: l, .. }, .. }) if *l == level)
        })
        .map(|e| e.id)
        .collect()
}

/// What is on a level: the elements hosted on it, not its views.
pub fn on_level(model: &Model, level: ElementId) -> Vec<ElementId> {
    model
        .iter()
        .filter(|e| !matches!(e.kind, ElementKind::View(_)) && level_of(&e.kind) == Some(level))
        .map(|e| e.id)
        .collect()
}

/// The level an element hangs off, if it hangs off one.
pub fn level_of(kind: &ElementKind) -> Option<ElementId> {
    match kind {
        ElementKind::Wall(w) => Some(w.level),
        ElementKind::Slab(s) => Some(s.level),
        ElementKind::FreeMesh(m) => m.level,
        ElementKind::Sketch(s) => s.level,
        ElementKind::Guide(g) => g.level,
        ElementKind::Object(o) => Some(o.level),
        ElementKind::View(View {
            kind: ViewKind::Plan { level, .. },
            ..
        }) => Some(*level),
        k => crate::members::level_of(k),
    }
}

/// The element moved onto level `to` (at `to_elevation`) from a level at
/// `from_elevation`, staying where it is in space.
pub fn rehost(e: &Element, to: ElementId, from_elevation: f64, to_elevation: f64) -> Element {
    let dz = from_elevation - to_elevation;
    let mut out = e.clone();
    match &mut out.kind {
        ElementKind::Wall(w) => {
            w.level = to;
            w.base_offset += dz;
        }
        ElementKind::Slab(s) => {
            s.level = to;
            s.top_offset += dz;
        }
        ElementKind::FreeMesh(m) => {
            m.level = Some(to);
            m.origin.z += dz;
        }
        ElementKind::Sketch(s) => {
            s.level = Some(to);
            s.plane.origin.z += dz;
        }
        // guide points are absolute; the level only says which plan shows it
        ElementKind::Guide(g) => g.level = Some(to),
        ElementKind::Column(c) => {
            c.level = to;
            c.base_offset += dz;
        }
        ElementKind::Beam(b) => {
            b.level = to;
            b.top_offset += dz;
        }
        ElementKind::Roof(r) => {
            r.level = to;
            r.base_offset += dz;
        }
        ElementKind::Stair(s) => {
            s.level = to;
            s.base_offset += dz;
        }
        ElementKind::Railing(r) => {
            r.level = to;
            for p in &mut r.path {
                p.z += dz;
            }
        }
        ElementKind::Object(o) => {
            o.level = to;
            o.offset += dz;
        }
        ElementKind::View(View {
            kind: ViewKind::Plan { level, .. },
            ..
        }) => *level = to,
        _ => {}
    }
    out
}

/// A new level, and with `plan` a floor plan of it.
/// Returns the edit, the level's id and the plan's.
pub fn add(
    model: &Model,
    name: &str,
    elevation: f64,
    height: f64,
    plan: bool,
) -> (Transaction, ElementId, Option<ElementId>) {
    let _ = model;
    let level = Element::new(name, ElementKind::Level(Level { elevation, height }));
    let id = level.id;
    let mut tx = Transaction::new("Add level").insert(level);
    let mut view = None;
    if plan {
        let v = Element::new(format!("{name} plan"), ElementKind::View(View::plan(id)));
        view = Some(v.id);
        tx = tx.insert(v);
    }
    (tx, id, view)
}

/// Where a new level above the top one goes: one storey up, as tall as
/// the top one. (0, 3 m) in a project without levels.
pub fn above_top(model: &Model) -> (f64, f64) {
    match sorted(model).last() {
        Some((_, _, l)) => (l.elevation + l.height, l.height),
        None => (0.0, 3.0),
    }
}

/// A copy of a level one storey above the top level, with `elements` also
/// a copy of everything on it (doors and windows in its walls, cuts) and
/// with `plan` a floor plan.
pub fn duplicate(
    model: &Model,
    level: ElementId,
    elements: bool,
    plan: bool,
) -> Option<(Transaction, ElementId)> {
    let src = model.level(level)?.clone();
    let (elevation, _) = above_top(model);
    let name = next_name(model);
    let (mut tx, id, _) = add(model, &name, elevation, src.height, plan);
    tx.label = "Duplicate level".into();
    if elements {
        let ids = crate::transform::copy_set(model, &on_level(model, level));
        let originals: Vec<Element> = ids.iter().filter_map(|i| model.get(*i).cloned()).collect();
        let (copies, _) = crate::transform::fresh_copies(model, &originals);
        for c in copies {
            // same offsets on the new level: the copy sits one storey up
            let mut c = if level_of(&c.kind) == Some(level) {
                rehost(&c, id, 0.0, 0.0)
            } else {
                c
            };
            // a copied wall keeps its height, not the original's top level
            if let ElementKind::Wall(w) = &mut c.kind {
                w.top = crate::WallTop::Unconnected;
            }
            tx = tx.insert(c);
        }
    }
    Some((tx, id))
}

/// Is `view` a name made from the level name `level`: the name itself, or
/// it followed by more words ("Level 2 plan", "Level 2 plan - furniture")?
/// Those follow the level when it is renamed; a name the user typed for the
/// plan ("Roof terrace", or "Level 20 plan" for "Level 2") is left alone.
pub fn named_after(view: &str, level: &str) -> bool {
    !level.is_empty()
        && view
            .strip_prefix(level)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
}

/// Rename a level and the plan views named after it (see [`named_after`]).
pub fn rename(model: &Model, level: ElementId, name: &str) -> Transaction {
    let mut tx = Transaction::new("Rename level");
    let Some(e) = model.get(level) else {
        return tx;
    };
    if e.name == name {
        return tx;
    }
    let old = e.name.clone();
    let mut after = e.clone();
    after.name = name.to_string();
    tx = tx.replace(e.clone(), after);
    for v in plan_views(model, level) {
        let Some(ve) = model.get(v) else { continue };
        if named_after(&ve.name, &old) {
            let mut after = ve.clone();
            after.name = format!("{name}{}", &ve.name[old.len()..]);
            tx = tx.replace(ve.clone(), after);
        }
    }
    tx
}

/// Give a level a new elevation and height. Everything on it moves with it.
pub fn set(model: &Model, level: ElementId, elevation: f64, height: f64) -> Transaction {
    let tx = Transaction::new("Change level");
    let Some(e) = model.get(level) else {
        return tx;
    };
    let mut after = e.clone();
    if let ElementKind::Level(l) = &mut after.kind {
        l.elevation = elevation;
        l.height = height;
    }
    if after == *e {
        return tx;
    }
    tx.replace(e.clone(), after)
}

/// What happens to the elements on a level being deleted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnDelete {
    /// They go with it, and so do its plan views.
    Delete,
    /// They move to this level, staying where they are; its plan views go.
    MoveTo(ElementId),
}

/// Delete a level. Its plan views always go (with what is drawn on them,
/// and their places on sheets), as do dimensions measured from the level;
/// what is on the level goes too or moves.
pub fn delete(model: &Model, level: ElementId, what: OnDelete) -> Transaction {
    let tx = Transaction::new("Delete level");
    let Some(e) = model.get(level) else {
        return tx;
    };
    let target = match what {
        OnDelete::MoveTo(t) if t != level => model.level(t).map(|l| (t, l.elevation)),
        _ => None,
    };
    let Some((to, to_elevation)) = target else {
        return tx.remove_cascade(model, level);
    };
    let from_elevation = model.level(level).map_or(0.0, |l| l.elevation);
    let mut tx = tx;
    for id in on_level(model, level) {
        if let Some(el) = model.get(id) {
            tx = tx.replace(el.clone(), rehost(el, to, from_elevation, to_elevation));
        }
    }
    // its plans go, and annotations measuring from the level itself
    let moved = on_level(model, level);
    let gone: Vec<ElementId> = model
        .dependents(level)
        .into_iter()
        .filter(|d| !moved.contains(d))
        .collect();
    tx = tx.remove_all_cascade(model, &gone);
    tx.ops.push(crate::Op::Remove(e.clone()));
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sample::sample_house, Document};

    /// The same elements, whatever their order (undoing a delete puts
    /// elements back at the end).
    fn same(a: &Model, b: &Model) -> bool {
        a.len() == b.len() && a.iter().all(|e| b.get(e.id) == Some(e))
    }

    fn named(model: &Model, name: &str) -> ElementId {
        model
            .levels()
            .find(|(e, _)| e.name == name)
            .map(|(e, _)| e.id)
            .unwrap()
    }

    #[test]
    fn a_level_added_on_top_comes_with_its_plan_and_undoes() {
        let mut doc = Document::from_model(sample_house());
        let before = doc.model().clone();
        let (elevation, height) = above_top(doc.model());
        assert_eq!((elevation, height), (6.0, 3.0));
        let name = next_name(doc.model());
        assert_eq!(name, "Level 2");
        let (tx, id, view) = add(doc.model(), &name, elevation, height, true);
        doc.commit(tx).unwrap();
        assert_eq!(doc.model().level(id).unwrap().elevation, 6.0);
        assert_eq!(plan_views(doc.model(), id), vec![view.unwrap()]);
        assert_eq!(sorted(doc.model()).last().unwrap().0, id);
        doc.undo();
        assert!(same(doc.model(), &before));
    }

    #[test]
    fn deleting_a_level_takes_what_is_on_it_and_undoes() {
        let mut doc = Document::from_model(sample_house());
        let before = doc.model().clone();
        let first = named(doc.model(), "First Floor");
        let on = on_level(doc.model(), first);
        assert!(!on.is_empty());
        doc.commit(delete(doc.model(), first, OnDelete::Delete))
            .unwrap();
        assert!(doc.model().get(first).is_none());
        assert!(on.iter().all(|id| doc.model().get(*id).is_none()));
        doc.undo();
        assert!(same(doc.model(), &before));
    }

    #[test]
    fn moving_elements_off_a_deleted_level_keeps_them_in_place() {
        let mut doc = Document::from_model(sample_house());
        let before = doc.model().clone();
        let ground = named(doc.model(), "Ground Floor");
        let first = named(doc.model(), "First Floor");
        let on = on_level(doc.model(), first);
        let views = plan_views(doc.model(), first);
        let tops: Vec<(ElementId, f64)> = on
            .iter()
            .filter_map(|id| match &doc.model().get(*id)?.kind {
                ElementKind::Wall(w) => Some((*id, 3.0 + w.base_offset)),
                ElementKind::Slab(s) => Some((*id, 3.0 + s.top_offset)),
                _ => None,
            })
            .collect();
        assert!(!tops.is_empty());
        doc.commit(delete(doc.model(), first, OnDelete::MoveTo(ground)))
            .unwrap();
        let m = doc.model();
        assert!(m.get(first).is_none());
        assert!(views.iter().all(|v| m.get(*v).is_none()));
        for id in &on {
            assert_eq!(level_of(&m.get(*id).unwrap().kind), Some(ground));
        }
        for (id, z) in tops {
            let now = match &m.get(id).unwrap().kind {
                ElementKind::Wall(w) => w.base_offset,
                ElementKind::Slab(s) => s.top_offset,
                _ => unreachable!(),
            };
            assert!((now - z).abs() < 1e-9, "{id}: {now} vs {z}");
        }
        doc.undo();
        assert!(same(doc.model(), &before));
    }

    #[test]
    fn duplicating_a_level_copies_what_is_on_it_one_storey_up() {
        let mut doc = Document::from_model(sample_house());
        let ground = named(doc.model(), "Ground Floor");
        let walls = |m: &Model, l: ElementId| {
            m.iter()
                .filter(|e| matches!(&e.kind, ElementKind::Wall(w) if w.level == l))
                .count()
        };
        let n = walls(doc.model(), ground);
        let (tx, id) = duplicate(doc.model(), ground, true, true).unwrap();
        doc.commit(tx).unwrap();
        assert_eq!(walls(doc.model(), id), n);
        assert_eq!(doc.model().level(id).unwrap().elevation, 6.0);
        assert_eq!(plan_views(doc.model(), id).len(), 1);
    }

    #[test]
    fn renaming_a_level_renames_its_plan() {
        let mut doc = Document::from_model(sample_house());
        let ground = named(doc.model(), "Ground Floor");
        let (tx, _, view) = add(doc.model(), "Roof", 6.0, 3.0, true);
        doc.commit(tx).unwrap();
        let roof = named(doc.model(), "Roof");
        doc.commit(rename(doc.model(), roof, "Roof deck")).unwrap();
        assert_eq!(
            doc.model().get(view.unwrap()).unwrap().name,
            "Roof deck plan"
        );
        doc.commit(set(doc.model(), ground, -0.5, 3.5)).unwrap();
        assert_eq!(doc.model().level(ground).unwrap().elevation, -0.5);
    }

    #[test]
    fn plans_the_user_named_keep_their_names_when_the_level_is_renamed() {
        let mut doc = Document::from_model(sample_house());
        let (tx, level, plan) = add(doc.model(), "Level 2", 6.0, 3.0, true);
        doc.commit(tx).unwrap();
        let plan = plan.unwrap();
        // a second plan the user named, and one that only starts alike
        let own = Element::new("Terrace layout", ElementKind::View(View::plan(level)));
        let alike = Element::new("Level 20 plan", ElementKind::View(View::plan(level)));
        let (own_id, alike_id) = (own.id, alike.id);
        doc.commit(Transaction::new("Add plans").insert(own).insert(alike))
            .unwrap();
        let tx = rename(doc.model(), level, "Roof");
        assert_eq!(tx.ops.len(), 2, "the level and its derived plan only");
        doc.commit(tx).unwrap();
        let name = |id: ElementId| doc.model().get(id).unwrap().name.clone();
        assert_eq!(name(plan), "Roof plan");
        assert_eq!(name(own_id), "Terrace layout");
        assert_eq!(name(alike_id), "Level 20 plan");
        assert!(named_after("Level 2 plan", "Level 2"));
        assert!(named_after("Level 2", "Level 2"));
        assert!(!named_after("Level 20 plan", "Level 2"));
        assert!(!named_after("anything", ""));
    }
}
