//! What is selected: one active element (the one Properties shows and
//! handles grab) and any others picked with it. Edits that make sense for
//! many elements (delete, move, rotate, scale, copy, hide) take them all.

use creabim_core::ElementId;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// The element Properties shows and handles act on.
    pub active: Option<ElementId>,
    /// Everything else selected with it, oldest first.
    pub others: Vec<ElementId>,
}

impl Selection {
    /// Select only `id`.
    pub fn only(&mut self, id: ElementId) {
        self.active = Some(id);
        self.others.clear();
    }

    /// Select only `id`, or nothing.
    pub fn set(&mut self, id: Option<ElementId>) {
        self.active = id;
        self.others.clear();
    }

    pub fn clear(&mut self) {
        self.set(None);
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_none()
    }

    pub fn is_some(&self) -> bool {
        self.active.is_some()
    }

    pub fn len(&self) -> usize {
        self.active.map_or(0, |_| 1 + self.others.len())
    }

    pub fn contains(&self, id: ElementId) -> bool {
        self.active == Some(id) || self.others.contains(&id)
    }

    /// Everything selected, the active element last.
    pub fn all(&self) -> Vec<ElementId> {
        let mut v = self.others.clone();
        v.extend(self.active);
        v
    }

    /// Shift+click: add `id` and make it active, or take it out if it was
    /// already selected.
    pub fn toggle(&mut self, id: ElementId) {
        if self.active == Some(id) {
            self.active = self.others.pop();
        } else if let Some(i) = self.others.iter().position(|o| *o == id) {
            self.others.remove(i);
        } else {
            self.others.extend(self.active);
            self.active = Some(id);
        }
    }

    /// Select `ids` (from a box), adding to what is selected when `add`.
    /// The last one becomes active unless something already was.
    pub fn extend(&mut self, ids: impl IntoIterator<Item = ElementId>, add: bool) {
        if !add {
            self.clear();
        }
        for id in ids {
            if self.contains(id) {
                continue;
            }
            match self.active {
                None => self.active = Some(id),
                Some(_) => self.others.push(id),
            }
        }
    }

    /// Drop ids that are no longer in the model.
    pub fn retain(&mut self, keep: impl Fn(ElementId) -> bool) {
        self.others.retain(|id| keep(*id));
        if self.active.is_some_and(|a| !keep(a)) {
            self.active = self.others.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_clicks_add_and_remove() {
        let (a, b, c) = (ElementId::new(), ElementId::new(), ElementId::new());
        let mut s = Selection::default();
        s.only(a);
        s.toggle(b);
        s.toggle(c);
        assert_eq!(s.active, Some(c));
        assert_eq!(s.all(), vec![a, b, c]);
        s.toggle(c);
        assert_eq!(s.active, Some(b));
        s.toggle(a);
        assert_eq!(s.all(), vec![b]);
        s.extend([a, b, c], true);
        assert_eq!(s.len(), 3);
        s.extend([c], false);
        assert_eq!(s.all(), vec![c]);
    }
}
