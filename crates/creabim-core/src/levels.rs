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
        // one under a wall or a column sits below its host's base, which moves itself
        ElementKind::Footing(f) => {
            f.level = to;
            if f.host.is_none() {
                f.below -= dz;
            }
        }
        ElementKind::View(View {
            kind: ViewKind::Plan { level, .. },
            ..
        }) => *level = to,
        _ => {}
    }
    out
}

// ------------------------------------------------------- copying to levels

/// The level an element stands on, for copying and moving between
/// levels: [`level_of`]'s, and a shaft's base level. Plan views are not
/// moved this way.
fn base_level(kind: &ElementKind) -> Option<ElementId> {
    match kind {
        ElementKind::View(_) => None,
        ElementKind::Shaft(s) => Some(s.level),
        k => level_of(k),
    }
}

/// The level as many storeys above (or below) `to` as `l` is above
/// `from`: where a top tied to `l` goes when what it tops moves from
/// `from` to `to`. `None` when there are not that many levels.
pub fn shifted(model: &Model, from: ElementId, to: ElementId, l: ElementId) -> Option<ElementId> {
    let list = sorted(model);
    let at = |id: ElementId| list.iter().position(|(i, _, _)| *i == id);
    let k = at(to)? as isize + at(l)? as isize - at(from)? as isize;
    list.get(usize::try_from(k).ok()?).map(|(i, _, _)| *i)
}

/// `e` set on level `to` from level `from`, keeping its offsets: it sits
/// on `to` as it sat on `from`, at the same place in plan. Tops tied to a
/// level move as many storeys as the element does, so a wall or column
/// running to the level above still does; where there is no such level
/// the top lets go and keeps its height.
pub fn rebase(model: &Model, e: &Element, from: ElementId, to: ElementId) -> Element {
    let mut out = match &e.kind {
        ElementKind::Shaft(_) => e.clone(),
        _ => rehost(e, to, 0.0, 0.0),
    };
    let height = |c: &crate::Column| c.height_in(model);
    match &mut out.kind {
        ElementKind::Wall(w) => {
            if let crate::WallTop::UpToLevel { level, offset } = w.top {
                w.top = match shifted(model, from, to, level) {
                    Some(l) => crate::WallTop::UpToLevel { level: l, offset },
                    None => crate::WallTop::Unconnected,
                };
            }
            // a wall's height follows its top constraint as levels settle;
            // its attachments are to things on the old level
            w.attach_top.clear();
            if let Some(h) = crate::walls::constrained_height(model, w) {
                if h >= crate::walls::MIN_HEIGHT {
                    w.height = h;
                }
            }
        }
        ElementKind::Column(c) => {
            if let crate::WallTop::UpToLevel { level, offset } = c.top {
                let h = match &e.kind {
                    ElementKind::Column(old) => height(old),
                    _ => 0.0,
                };
                c.top = match shifted(model, from, to, level) {
                    Some(l) => crate::WallTop::UpToLevel { level: l, offset },
                    None => {
                        c.height = Some(h);
                        crate::WallTop::Unconnected
                    }
                };
            }
        }
        ElementKind::Shaft(s) => {
            s.level = to;
            s.top_level = s.top_level.and_then(|l| shifted(model, from, to, l));
        }
        _ => {}
    }
    out
}

/// The elements of `ids` that can go to another level, with what goes
/// with them: the doors and windows in their walls, the cuts on them,
/// and footings under them that are selected too.
pub fn movable(model: &Model, ids: &[ElementId]) -> Vec<ElementId> {
    let own: Vec<ElementId> = ids
        .iter()
        .copied()
        .filter(|id| {
            model.get(*id).is_some_and(|e| {
                base_level(&e.kind).is_some() || matches!(e.kind, ElementKind::Opening(_))
            })
        })
        .collect();
    crate::transform::copy_set(model, &own)
}

/// Copies of `ids` on each level of `targets`, at the same place in plan,
/// each set on its new level as the original sits on its own (see
/// [`rebase`]). Doors and windows come with their walls, a footing with
/// its wall or column when both are copied (one copied alone lets go of
/// its host). One undo step; returns it and the copies' ids.
pub fn copy_to_levels(
    model: &Model,
    ids: &[ElementId],
    targets: &[ElementId],
) -> (Transaction, Vec<ElementId>) {
    let set = movable(model, ids);
    let originals: Vec<Element> = set.iter().filter_map(|i| model.get(*i).cloned()).collect();
    let n = targets
        .iter()
        .filter(|t| model.level(**t).is_some())
        .count();
    let mut tx = Transaction::new(match n {
        1 => "Copy to level".to_string(),
        n => format!("Copy to {n} levels"),
    });
    let mut out = vec![];
    for to in targets {
        if model.level(*to).is_none() {
            continue;
        }
        let (copies, _) = crate::transform::fresh_copies(model, &originals);
        let new_ids: Vec<ElementId> = copies.iter().map(|c| c.id).collect();
        for c in copies {
            let mut c = match base_level(&c.kind) {
                Some(from) => rebase(model, &c, from, *to),
                None => c,
            };
            // a footing copied without its host stays where the host put it
            if let ElementKind::Footing(f) = &c.kind {
                if f.host.is_some_and(|h| !new_ids.contains(&h)) {
                    let from = f.plan_level(model);
                    let free = crate::footing::detached(model, f);
                    let mut fe = c.clone();
                    fe.kind = ElementKind::Footing(free);
                    c = rebase(model, &fe, from, *to);
                }
            }
            out.push(c.id);
            tx = tx.insert(c);
        }
    }
    (tx, out)
}

/// Move `ids` to level `to`, at the same place in plan, each set on it as
/// it sat on its own level (see [`rebase`]); doors, windows and footings
/// under them go too. One undo step.
pub fn move_to_level(model: &Model, ids: &[ElementId], to: ElementId) -> Transaction {
    let mut tx = Transaction::new("Move to level");
    if model.level(to).is_none() {
        return tx;
    }
    let mut set = movable(model, ids);
    // footings under what moves move too
    for id in set.clone() {
        for f in crate::footing::footings_under(model, id) {
            if !set.contains(&f) {
                set.push(f);
            }
        }
    }
    for id in set {
        let Some(e) = model.get(id) else { continue };
        let Some(from) = base_level(&e.kind) else {
            continue;
        };
        if from == to {
            continue;
        }
        let after = rebase(model, e, from, to);
        if after != *e {
            tx = tx.replace(e.clone(), after);
        }
    }
    tx
}

/// `copies` (fresh, not in the model yet) set on level `to` from wherever
/// they were: paste aligned to a level.
pub fn paste_onto(model: &Model, copies: Vec<Element>, to: ElementId) -> Vec<Element> {
    copies
        .into_iter()
        .map(|c| match base_level(&c.kind) {
            Some(from) if from != to => rebase(model, &c, from, to),
            _ => c,
        })
        .collect()
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

    /// The sample house with two more storeys: levels at 0, 3, 6 and 9 m.
    fn four_storeys() -> (Document, [ElementId; 4]) {
        let mut doc = Document::from_model(sample_house());
        let g = named(doc.model(), "Ground Floor");
        let f = named(doc.model(), "First Floor");
        let (tx, l2, _) = add(doc.model(), "Level 2", 6.0, 3.0, false);
        doc.commit(tx).unwrap();
        let (tx, l3, _) = add(doc.model(), "Level 3", 9.0, 3.0, false);
        doc.commit(tx).unwrap();
        (doc, [g, f, l2, l3])
    }

    fn of_kind(m: &Model, level: ElementId, f: impl Fn(&ElementKind) -> bool) -> Vec<ElementId> {
        m.iter()
            .filter(|e| f(&e.kind) && base_level(&e.kind) == Some(level))
            .map(|e| e.id)
            .collect()
    }

    #[test]
    fn walls_copied_to_two_levels_bring_their_windows_and_undo_as_one() {
        let (mut doc, [_, f, l2, l3]) = four_storeys();
        let before = doc.model().clone();
        let walls = of_kind(doc.model(), f, |k| matches!(k, ElementKind::Wall(_)));
        assert_eq!(walls.len(), 4);
        let windows = |m: &Model, walls: &[ElementId]| {
            m.iter()
                .filter(|e| matches!(&e.kind, ElementKind::Opening(o) if walls.contains(&o.host)))
                .count()
        };
        let n_windows = windows(doc.model(), &walls);
        assert!(n_windows > 0);
        let (tx, ids) = copy_to_levels(doc.model(), &walls, &[l2, l3]);
        doc.commit(tx).unwrap();
        assert_eq!(ids.len(), 2 * (4 + n_windows));
        for l in [l2, l3] {
            let copies = of_kind(doc.model(), l, |k| matches!(k, ElementKind::Wall(_)));
            assert_eq!(copies.len(), 4, "four walls on each level");
            assert_eq!(windows(doc.model(), &copies), n_windows);
            for c in copies {
                let ElementKind::Wall(w) = &doc.model().get(c).unwrap().kind else {
                    panic!()
                };
                // at the same place in plan as one of the originals
                assert!(walls.iter().any(|o| matches!(
                    &doc.model().get(*o).unwrap().kind,
                    ElementKind::Wall(ow) if ow.start == w.start && ow.end == w.end
                )));
            }
        }
        doc.undo();
        assert!(same(doc.model(), &before));
    }

    #[test]
    fn a_column_drawn_high_up_stands_on_a_lower_level_and_stretches() {
        let (mut doc, [g, f, l2, l3]) = four_storeys();
        let col = Element::new(
            "C",
            ElementKind::Column(crate::Column::new(
                l2,
                glam::DVec2::new(5.0, 3.0),
                crate::Profile::Round { diameter: 0.3 },
            )),
        );
        let id = col.id;
        doc.commit(Transaction::new("c").insert(col)).unwrap();
        let column = |m: &Model| match &m.get(id).unwrap().kind {
            ElementKind::Column(c) => c.clone(),
            _ => panic!(),
        };
        // it runs up its storey, from 6 to 9 m
        assert_eq!(column(doc.model()).span(doc.model()), (6.0, 9.0));
        // stood on the first floor, its top stays at Level 3
        let m = doc.model().clone();
        doc.edit(id, "base", |e| {
            if let ElementKind::Column(c) = &mut e.kind {
                c.set_base_level(&m, f);
            }
        })
        .unwrap();
        let c = column(doc.model());
        assert_eq!(c.level, f);
        assert_eq!(c.top_level(), Some(l3));
        assert_eq!(c.span(doc.model()), (3.0, 9.0));
        // moving its top level stretches it
        doc.commit(set(doc.model(), l3, 9.5, 3.0)).unwrap();
        assert_eq!(column(doc.model()).span(doc.model()), (3.0, 9.5));
        // it shows in the plan of the storey it runs through
        assert!(column(doc.model()).crosses(doc.model(), l2, 1.2));
        assert!(!column(doc.model()).crosses(doc.model(), g, 1.2));
        // copied down a storey, it runs one storey lower
        let (tx, ids) = copy_to_levels(doc.model(), &[id], &[g]);
        doc.commit(tx).unwrap();
        let ElementKind::Column(copy) = &doc.model().get(ids[0]).unwrap().kind else {
            panic!()
        };
        assert_eq!(copy.level, g);
        assert_eq!(copy.top_level(), Some(l2));
        // copied to the top, there is no level that far up: it keeps its height
        let (tx, ids) = copy_to_levels(doc.model(), &[id], &[l3]);
        doc.commit(tx).unwrap();
        let ElementKind::Column(top) = &doc.model().get(ids[0]).unwrap().kind else {
            panic!()
        };
        assert!(top.top.is_unconnected());
        assert!((top.height_in(doc.model()) - 6.5).abs() < 1e-9);
    }

    #[test]
    fn moving_walls_to_another_level_takes_their_footings_and_tops_along() {
        let (mut doc, [g, f, l2, _]) = four_storeys();
        let walls = of_kind(doc.model(), g, |k| matches!(k, ElementKind::Wall(_)));
        // tie their tops to the first floor, and put footings under them
        for w in &walls {
            doc.edit(*w, "top", |e| {
                if let ElementKind::Wall(w) = &mut e.kind {
                    w.top = crate::WallTop::UpToLevel {
                        level: f,
                        offset: 0.0,
                    };
                }
            })
            .unwrap();
        }
        let (tx, footings) = crate::footing::under(doc.model(), &walls, &|_| {});
        doc.commit(tx).unwrap();
        let tx = move_to_level(doc.model(), &walls, f);
        doc.commit(tx).unwrap();
        for w in &walls {
            let ElementKind::Wall(w) = &doc.model().get(*w).unwrap().kind else {
                panic!()
            };
            assert_eq!(w.level, f);
            assert_eq!(
                w.top,
                crate::WallTop::UpToLevel {
                    level: l2,
                    offset: 0.0
                }
            );
        }
        for id in footings {
            let ElementKind::Footing(ft) = &doc.model().get(id).unwrap().kind else {
                panic!()
            };
            assert_eq!(ft.level, f);
            assert!((ft.top(doc.model()) - 3.0).abs() < 1e-9);
        }
    }

    #[test]
    fn a_footing_copied_without_its_wall_lets_go_and_stays_in_place() {
        let (mut doc, [g, f, _, _]) = four_storeys();
        let walls = of_kind(doc.model(), g, |k| matches!(k, ElementKind::Wall(_)));
        let (tx, footings) = crate::footing::under(doc.model(), &walls[..1], &|_| {});
        doc.commit(tx).unwrap();
        let (tx, ids) = copy_to_levels(doc.model(), &footings, &[f]);
        doc.commit(tx).unwrap();
        let ElementKind::Footing(copy) = &doc.model().get(ids[0]).unwrap().kind else {
            panic!()
        };
        assert!(copy.host.is_none());
        assert_eq!(copy.level, f);
        let ElementKind::Wall(w) = &doc.model().get(walls[0]).unwrap().kind else {
            panic!()
        };
        assert_eq!((copy.start, copy.end), (w.start, w.end));
    }
}
