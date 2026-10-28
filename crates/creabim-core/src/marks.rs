//! Derived properties shared by tags, schedules and exports.

use crate::{Element, ElementId, ElementKind, Material, Model, OpeningKind};
use std::collections::HashMap;

/// Door and window marks: D01, D02, … and W01, W02, …, numbered from the
/// lowest level up and in model order within a level. Tags and schedules
/// both use these, so they always agree.
pub fn marks(model: &Model) -> HashMap<ElementId, String> {
    let elevation = |host: ElementId| match model.get(host).map(|e| &e.kind) {
        Some(ElementKind::Wall(w)) => model.level(w.level).map_or(0.0, |l| l.elevation),
        _ => 0.0,
    };
    let mut out = HashMap::new();
    for (kind, prefix) in [(OpeningKind::Door, "D"), (OpeningKind::Window, "W")] {
        let mut openings: Vec<(usize, f64, ElementId)> = model
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match &e.kind {
                ElementKind::Opening(o) if o.kind == kind => Some((i, elevation(o.host), e.id)),
                _ => None,
            })
            .collect();
        openings.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        for (n, (_, _, id)) in openings.into_iter().enumerate() {
            out.insert(id, format!("{prefix}{:02}", n + 1));
        }
    }
    out
}

/// Object type marks: E01, E02, … for electrical types, L01 for lights,
/// P01 plumbing, M01 heating and ventilation, S01 sanitary ware, K01
/// kitchen and F01 furniture (see [`crate::ObjectCategory::mark_prefix`]),
/// numbered in name order within each letter. Each type's objects share
/// its mark, so a tag on a socket and the fixture schedule agree.
pub fn object_marks(model: &Model) -> HashMap<ElementId, String> {
    let mut types: Vec<(&'static str, &str, ElementId)> = model
        .iter()
        .filter_map(|e| {
            let t = crate::objects::type_of_id(model, e.id)?;
            Some((t.category.mark_prefix(), e.name.as_str(), e.id))
        })
        .collect();
    types.sort();
    let mut out = HashMap::new();
    for group in types.chunk_by(|a, b| a.0 == b.0) {
        for (n, (prefix, _, id)) in group.iter().enumerate() {
            out.insert(*id, format!("{prefix}{:02}", n + 1));
        }
    }
    out
}

/// The mark of a placed object: its type's (see [`object_marks`]).
pub fn object_mark(model: &Model, e: &Element) -> Option<String> {
    let t = e.of_type.as_ref()?.id;
    object_marks(model).remove(&t)
}

/// The material an element is made of, if it has one and it still exists.
pub fn material_of<'a>(model: &'a Model, e: &Element) -> Option<(&'a Element, &'a Material)> {
    let id = crate::library::material_slot(e)??;
    let m = model.get(id)?;
    match &m.kind {
        ElementKind::Material(mat) => Some((m, mat)),
        _ => None,
    }
}
