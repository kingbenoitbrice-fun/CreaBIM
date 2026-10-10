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

/// The material an element is made of, if it has one and it still exists.
pub fn material_of<'a>(model: &'a Model, e: &Element) -> Option<(&'a Element, &'a Material)> {
    let id = match &e.kind {
        ElementKind::Wall(w) => w.material,
        ElementKind::Slab(s) => s.material,
        ElementKind::FreeMesh(m) => m.material,
        _ => None,
    }?;
    let m = model.get(id)?;
    match &m.kind {
        ElementKind::Material(mat) => Some((m, mat)),
        _ => None,
    }
}
