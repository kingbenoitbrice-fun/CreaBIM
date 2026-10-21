//! What each view leaves out (Revit's Visibility/Graphics): whole
//! categories, single elements and grid groups. It is stored on the view, so
//! a sheet's viewports, which show their view, leave out the same things;
//! changes go through transactions like any edit, so they undo.

use crate::{Element, ElementId, ElementKind, Model, OpeningKind, ViewKind};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A kind of element a view can leave out. New kinds go at the end of
/// [`Category::ALL`] with a label and a line in [`Category::of`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Category {
    Walls,
    Slabs,
    Doors,
    Windows,
    /// Elements used to cut voids into others (their ghost in 3D).
    Openings,
    /// Freeform masses and imported meshes.
    FreeMeshes,
    Grids,
    Levels,
    /// Section and elevation lines drawn in plans.
    SectionMarks,
    Dimensions,
    Tags,
    /// Text notes and their leaders.
    Notes,
    Sketches,
    Guides,
    Columns,
    Beams,
    Roofs,
    Stairs,
    Railings,
    /// A category from a newer CreaBIM, kept so older versions still read
    /// the file.
    #[serde(other)]
    Other,
}

impl Category {
    pub const ALL: [Category; 19] = [
        Category::Walls,
        Category::Slabs,
        Category::Doors,
        Category::Windows,
        Category::Openings,
        Category::FreeMeshes,
        Category::Grids,
        Category::Levels,
        Category::SectionMarks,
        Category::Dimensions,
        Category::Tags,
        Category::Notes,
        Category::Sketches,
        Category::Guides,
        Category::Columns,
        Category::Beams,
        Category::Roofs,
        Category::Stairs,
        Category::Railings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Walls => "Walls",
            Category::Slabs => "Slabs",
            Category::Doors => "Doors",
            Category::Windows => "Windows",
            Category::Openings => "Openings (void cutters)",
            Category::FreeMeshes => "Masses and meshes",
            Category::Grids => "Grids",
            Category::Levels => "Levels",
            Category::SectionMarks => "Section marks",
            Category::Dimensions => "Dimensions",
            Category::Tags => "Tags",
            Category::Notes => "Notes",
            Category::Sketches => "Sketches",
            Category::Guides => "Guides",
            Category::Columns => "Columns",
            Category::Beams => "Beams",
            Category::Roofs => "Roofs",
            Category::Stairs => "Stairs",
            Category::Railings => "Railings",
            Category::Other => "Other",
        }
    }

    /// The category an element is in, if views can leave it out. `cutters`
    /// are the elements used to cut others (see [`cutters`]).
    pub fn of(e: &Element, cutters: &HashSet<ElementId>) -> Option<Category> {
        Some(match &e.kind {
            ElementKind::Wall(_) => Category::Walls,
            ElementKind::Slab(_) => Category::Slabs,
            ElementKind::Opening(o) => match o.kind {
                OpeningKind::Door => Category::Doors,
                OpeningKind::Window => Category::Windows,
            },
            _ if cutters.contains(&e.id) => Category::Openings,
            ElementKind::FreeMesh(_) => Category::FreeMeshes,
            ElementKind::Grid(_) | ElementKind::GridGroup(_) => Category::Grids,
            ElementKind::Level(_) => Category::Levels,
            ElementKind::View(v) if matches!(v.kind, ViewKind::Section { .. }) => {
                Category::SectionMarks
            }
            ElementKind::Dimension(_) => Category::Dimensions,
            ElementKind::Tag(_) => Category::Tags,
            ElementKind::Note(_) => Category::Notes,
            ElementKind::Sketch(_) => Category::Sketches,
            ElementKind::Guide(_) => Category::Guides,
            ElementKind::Column(_) => Category::Columns,
            ElementKind::Beam(_) => Category::Beams,
            ElementKind::Roof(_) => Category::Roofs,
            ElementKind::Stair(_) => Category::Stairs,
            ElementKind::Railing(_) => Category::Railings,
            _ => return None,
        })
    }
}

/// Elements that cut others.
pub fn cutters(model: &Model) -> HashSet<ElementId> {
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Cut(c) => Some(c.cutter),
            _ => None,
        })
        .collect()
}

/// What a view leaves out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewHidden {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<Category>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<ElementId>,
    /// Grid groups whose lines are left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grid_groups: Vec<ElementId>,
}

impl ViewHidden {
    pub fn is_empty(&self) -> bool {
        self.categories.is_empty() && self.elements.is_empty() && self.grid_groups.is_empty()
    }

    pub fn hides_category(&self, c: Category) -> bool {
        self.categories.contains(&c)
    }

    pub fn set_category(&mut self, c: Category, hidden: bool) {
        self.categories.retain(|x| *x != c);
        if hidden {
            self.categories.push(c);
        }
    }

    pub fn set_element(&mut self, id: ElementId, hidden: bool) {
        self.elements.retain(|x| *x != id);
        if hidden {
            self.elements.push(id);
        }
    }

    pub fn set_grid_group(&mut self, id: ElementId, hidden: bool) {
        self.grid_groups.retain(|x| *x != id);
        if hidden {
            self.grid_groups.push(id);
        }
    }

    /// Every element of `model` this leaves out: the ones listed, those in
    /// a hidden category or grid group, and what hangs on them (a hidden
    /// wall's doors and windows, the tags of hidden elements).
    pub fn hidden_set(&self, model: &Model) -> HashSet<ElementId> {
        let mut out = HashSet::new();
        if self.is_empty() {
            return out;
        }
        let cutters = cutters(model);
        for e in model.iter() {
            let listed = self.elements.contains(&e.id);
            let category = Category::of(e, &cutters).is_some_and(|c| self.categories.contains(&c));
            let group = match &e.kind {
                ElementKind::Grid(g) => g.group.is_some_and(|g| self.grid_groups.contains(&g)),
                _ => false,
            };
            if listed || category || group {
                out.insert(e.id);
            }
        }
        // what hangs on something hidden goes too
        for e in model.iter() {
            let host_hidden = match &e.kind {
                ElementKind::Opening(o) => out.contains(&o.host),
                ElementKind::Tag(t) => out.contains(&t.host),
                ElementKind::Cut(c) => out.contains(&c.target),
                _ => false,
            };
            if host_hidden {
                out.insert(e.id);
            }
        }
        out
    }
}

/// A copy of `model` for drawing a view that leaves out `hidden`. Levels,
/// doors and windows stay in it (walls still take their height from their
/// level and keep their holes) and cutters stay while what they cut is
/// shown: callers drop those from the drawing by id instead. The copy may
/// hold references to elements it left out, so it is for reading only.
pub fn without(model: &Model, hidden: &HashSet<ElementId>) -> Model {
    let mut m = model.clone();
    if hidden.is_empty() {
        return m;
    }
    let mut keep_cutters = HashSet::new();
    for e in model.iter() {
        if let ElementKind::Cut(c) = &e.kind {
            if !hidden.contains(&c.target) {
                keep_cutters.insert(c.cutter);
            }
        }
    }
    let removed: HashSet<ElementId> = model
        .iter()
        .filter(|e| hidden.contains(&e.id))
        .filter(|e| {
            !matches!(
                e.kind,
                ElementKind::Level(_)
                    | ElementKind::Opening(_)
                    | ElementKind::View(_)
                    | ElementKind::Sheet(_)
                    | ElementKind::Viewport(_)
                    | ElementKind::Material(_)
            ) && !keep_cutters.contains(&e.id)
        })
        .map(|e| e.id)
        .collect();
    m.retain(|e| match &e.kind {
        ElementKind::Cut(c) => !removed.contains(&c.target) && !removed.contains(&c.cutter),
        _ => !removed.contains(&e.id),
    });
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::sample_house;

    fn count(model: &Model, set: &HashSet<ElementId>, f: impl Fn(&ElementKind) -> bool) -> usize {
        model
            .iter()
            .filter(|e| set.contains(&e.id) && f(&e.kind))
            .count()
    }

    #[test]
    fn hiding_a_category_hides_its_elements_and_what_hangs_on_them() {
        let m = sample_house();
        let mut h = ViewHidden::default();
        assert!(h.hidden_set(&m).is_empty());
        h.set_category(Category::Windows, true);
        let set = h.hidden_set(&m);
        let windows = m
            .iter()
            .filter(|e| matches!(&e.kind, ElementKind::Opening(o) if o.kind == OpeningKind::Window))
            .count();
        assert!(windows > 0);
        assert_eq!(
            count(&m, &set, |k| matches!(k, ElementKind::Opening(_))),
            windows
        );
        assert_eq!(count(&m, &set, |k| matches!(k, ElementKind::Wall(_))), 0);

        // a hidden wall takes its doors and windows along
        let wall = m
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)) && !m.dependents(e.id).is_empty())
            .unwrap();
        let mut h = ViewHidden::default();
        h.set_element(wall.id, true);
        let set = h.hidden_set(&m);
        assert!(set.contains(&wall.id));
        for d in m.dependents(wall.id) {
            if matches!(m.get(d).unwrap().kind, ElementKind::Opening(_)) {
                assert!(set.contains(&d));
            }
        }
        // the copy for drawing loses the wall but keeps levels and openings
        let copy = without(&m, &set);
        assert!(copy.get(wall.id).is_none());
        assert_eq!(copy.levels().count(), m.levels().count());
        h.set_element(wall.id, false);
        assert!(h.is_empty());
    }

    #[test]
    fn hidden_grid_groups_hide_only_their_lines() {
        let mut m = Model::new();
        let a = crate::grids::new_group(&m);
        m.insert(a.clone()).unwrap();
        let ga = crate::grids::new_grid(&m, Some(a.id), glam::DVec2::ZERO, glam::DVec2::Y);
        m.insert(ga.clone()).unwrap();
        let free = crate::grids::new_grid(&m, None, glam::DVec2::X, glam::DVec2::ONE);
        m.insert(free.clone()).unwrap();
        let mut h = ViewHidden::default();
        h.set_grid_group(a.id, true);
        let set = h.hidden_set(&m);
        assert!(set.contains(&ga.id));
        assert!(!set.contains(&free.id));
        h.set_category(Category::Grids, true);
        assert!(h.hidden_set(&m).contains(&free.id));
    }

    #[test]
    fn views_from_older_files_read_with_nothing_hidden() {
        let old = r#"{"kind":{"Axonometric":{"azimuth":30.0,"altitude":30.0}},"scale":100.0}"#;
        let v: crate::View = serde_json::from_str(old).unwrap();
        assert!(v.hidden.is_empty());
        assert!(!v.show_hidden);
        // nothing hidden writes nothing
        assert!(!serde_json::to_string(&v).unwrap().contains("hidden"));
        // showing elements hidden inside others is kept
        let shown = crate::View {
            show_hidden: true,
            ..v.clone()
        };
        let back: crate::View =
            serde_json::from_str(&serde_json::to_string(&shown).unwrap()).unwrap();
        assert!(back.show_hidden);
        // a category this version doesn't know still reads
        let h: ViewHidden =
            serde_json::from_str(r#"{"categories":["Walls","StructuralColumns"]}"#).unwrap();
        assert_eq!(h.categories, vec![Category::Walls, Category::Other]);
    }
}
