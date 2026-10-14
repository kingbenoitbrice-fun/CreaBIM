use crate::{Element, ElementId, ElementKind, Level, ModelError};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// The single source of truth every view reads from.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Model {
    elements: IndexMap<ElementId, Element>,
    /// Bumped on every change so views know when to rebuild.
    #[serde(skip)]
    revision: u64,
}

/// Two models are equal when they hold the same elements in the same order;
/// the revision counter is bookkeeping, not content.
impl PartialEq for Model {
    fn eq(&self, other: &Self) -> bool {
        self.elements.len() == other.elements.len() && self.iter().eq(other.iter())
    }
}

impl Model {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn get(&self, id: ElementId) -> Option<&Element> {
        self.elements.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Element> {
        self.elements.values()
    }

    pub fn levels(&self) -> impl Iterator<Item = (&Element, &Level)> {
        self.iter().filter_map(|e| match &e.kind {
            ElementKind::Level(l) => Some((e, l)),
            _ => None,
        })
    }

    pub fn level(&self, id: ElementId) -> Option<&Level> {
        match self.get(id).map(|e| &e.kind) {
            Some(ElementKind::Level(l)) => Some(l),
            _ => None,
        }
    }

    /// Keep only the elements `keep` accepts, without checking references:
    /// for read-only copies such as a view's model with what it hides left
    /// out ([`crate::visibility::without`]). Never commit edits from one.
    pub fn retain(&mut self, mut keep: impl FnMut(&Element) -> bool) {
        self.elements.retain(|_, e| keep(e));
        self.revision += 1;
    }

    /// Elements that directly reference `id`.
    pub fn dependents(&self, id: ElementId) -> Vec<ElementId> {
        self.iter()
            .filter(|e| e.references().contains(&id))
            .map(|e| e.id)
            .collect()
    }

    fn check_refs(&self, e: &Element) -> Result<(), ModelError> {
        e.validate().map_err(|m| ModelError::Invalid(e.id, m))?;
        for r in e.references() {
            if !self.elements.contains_key(&r) {
                return Err(ModelError::DanglingReference {
                    element: e.id,
                    missing: r,
                });
            }
        }
        if let ElementKind::Opening(o) = &e.kind {
            match self.get(o.host).map(|h| &h.kind) {
                Some(ElementKind::Wall(_)) => {}
                _ => {
                    return Err(ModelError::Invalid(
                        e.id,
                        "opening host must be a wall".into(),
                    ))
                }
            }
        }
        Ok(())
    }

    pub(crate) fn insert(&mut self, e: Element) -> Result<(), ModelError> {
        if self.elements.contains_key(&e.id) {
            return Err(ModelError::Duplicate(e.id));
        }
        self.check_refs(&e)?;
        self.elements.insert(e.id, e);
        self.revision += 1;
        Ok(())
    }

    pub(crate) fn remove(&mut self, id: ElementId) -> Result<Element, ModelError> {
        if let Some(by) = self.dependents(id).first() {
            return Err(ModelError::StillReferenced {
                element: id,
                by: *by,
            });
        }
        let e = self
            .elements
            .shift_remove(&id)
            .ok_or(ModelError::NotFound(id))?;
        self.revision += 1;
        Ok(e)
    }

    pub(crate) fn replace(&mut self, e: Element) -> Result<Element, ModelError> {
        if !self.elements.contains_key(&e.id) {
            return Err(ModelError::NotFound(e.id));
        }
        self.check_refs(&e)?;
        let old = self.elements.insert(e.id, e).expect("checked above");
        self.revision += 1;
        Ok(old)
    }

    /// Build a model from elements in the given order, checking references
    /// once all are present. Unlike [`Model::from_json`] this keeps the order
    /// exactly, which matters because an element may legitimately reference
    /// one inserted after it (a wall moved to a newer level).
    pub fn from_elements(elements: impl IntoIterator<Item = Element>) -> Result<Self, ModelError> {
        let mut m = Model::new();
        for e in elements {
            if m.elements.contains_key(&e.id) {
                return Err(ModelError::Duplicate(e.id));
            }
            m.elements.insert(e.id, e);
        }
        for e in m.elements.values() {
            m.check_refs(e)?;
        }
        m.revision = 1;
        Ok(m)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("model is always serializable")
    }

    /// Load a saved model, re-checking every reference.
    pub fn from_json(s: &str) -> Result<Self, String> {
        let raw: Model = serde_json::from_str(s).map_err(|e| e.to_string())?;
        let mut m = Model::new();
        // Insert in dependency order: an element goes in once its references exist.
        let mut pending: Vec<Element> = raw.elements.into_values().collect();
        while !pending.is_empty() {
            let before = pending.len();
            let mut rest = Vec::new();
            for e in pending {
                if e.references().iter().all(|r| m.elements.contains_key(r)) {
                    m.insert(e).map_err(|e| e.to_string())?;
                } else {
                    rest.push(e);
                }
            }
            if rest.len() == before {
                return Err(format!("{} elements have missing references", rest.len()));
            }
            pending = rest;
        }
        Ok(m)
    }
}
