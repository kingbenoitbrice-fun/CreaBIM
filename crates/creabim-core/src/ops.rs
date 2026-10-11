use crate::{Element, ElementId, Model, ModelError};
use serde::{Deserialize, Serialize};

/// One reversible change to the model. Ops carry full element values so every
/// op can be inverted without consulting the model.
// a replace holds two elements by design; boxing them buys nothing here
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Op {
    Insert(Element),
    Remove(Element),
    Replace { before: Element, after: Element },
}

impl Op {
    pub fn inverse(&self) -> Op {
        match self {
            Op::Insert(e) => Op::Remove(e.clone()),
            Op::Remove(e) => Op::Insert(e.clone()),
            Op::Replace { before, after } => Op::Replace {
                before: after.clone(),
                after: before.clone(),
            },
        }
    }

    fn apply(&self, model: &mut Model) -> Result<(), ModelError> {
        match self {
            Op::Insert(e) => model.insert(e.clone()),
            Op::Remove(e) => model.remove(e.id).map(|_| ()),
            Op::Replace { after, .. } => model.replace(after.clone()).map(|_| ()),
        }
    }
}

/// A user-level edit: a labelled group of ops applied all-or-nothing and
/// undone as one step.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub label: String,
    pub ops: Vec<Op>,
}

impl Transaction {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ops: vec![],
        }
    }

    pub fn insert(mut self, e: Element) -> Self {
        self.ops.push(Op::Insert(e));
        self
    }

    pub fn replace(mut self, before: Element, after: Element) -> Self {
        self.ops.push(Op::Replace { before, after });
        self
    }

    /// Remove an element and everything that depends on it (a wall and its
    /// doors, a level and everything on it), dependents first.
    pub fn remove_cascade(mut self, model: &Model, id: ElementId) -> Self {
        fn visit(model: &Model, id: ElementId, out: &mut Vec<ElementId>) {
            if out.contains(&id) {
                return;
            }
            for d in model.dependents(id) {
                visit(model, d, out);
            }
            out.push(id);
        }
        let mut order = vec![];
        visit(model, id, &mut order);
        for id in order {
            if let Some(e) = model.get(id) {
                self.ops.push(Op::Remove(e.clone()));
            }
        }
        self
    }

    pub fn inverse(&self) -> Transaction {
        Transaction {
            label: self.label.clone(),
            ops: self.ops.iter().rev().map(Op::inverse).collect(),
        }
    }

    /// Apply every op, or none: on failure the ops already applied are rolled back.
    pub fn apply(&self, model: &mut Model) -> Result<(), ModelError> {
        for (i, op) in self.ops.iter().enumerate() {
            if let Err(err) = op.apply(model) {
                for done in self.ops[..i].iter().rev() {
                    done.inverse()
                        .apply(model)
                        .expect("rolling back an applied op cannot fail");
                }
                return Err(err);
            }
        }
        Ok(())
    }
}
