use crate::{Element, ElementId, Model, ModelError, Transaction};

/// A model plus its edit history. All edits go through [`Document::commit`].
#[derive(Debug, Default)]
pub struct Document {
    model: Model,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_model(model: Model) -> Self {
        Self {
            model,
            ..Self::default()
        }
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    pub fn commit(&mut self, tx: Transaction) -> Result<(), ModelError> {
        if tx.ops.is_empty() {
            return Ok(());
        }
        tx.apply(&mut self.model)?;
        self.undo.push(tx);
        self.redo.clear();
        Ok(())
    }

    /// Convenience: change one element through a closure, as one undo step.
    pub fn edit(
        &mut self,
        id: ElementId,
        label: &str,
        f: impl FnOnce(&mut Element),
    ) -> Result<(), ModelError> {
        let before = self.model.get(id).ok_or(ModelError::NotFound(id))?.clone();
        let mut after = before.clone();
        f(&mut after);
        if after == before {
            return Ok(());
        }
        self.commit(Transaction::new(label).replace(before, after))
    }

    /// Like [`Document::edit`], but merges into the previous undo step when it
    /// edited the same element under the same label. Dragging a value in the
    /// properties panel then undoes in one step, not one per frame.
    pub fn edit_merged(
        &mut self,
        id: ElementId,
        label: &str,
        f: impl FnOnce(&mut Element),
    ) -> Result<(), ModelError> {
        let mergeable = self.redo.is_empty()
            && matches!(self.undo.last(), Some(t) if t.label == label
                && matches!(t.ops.as_slice(), [crate::Op::Replace { after, .. }] if after.id == id));
        if !mergeable {
            return self.edit(id, label, f);
        }
        let current = self.model.get(id).ok_or(ModelError::NotFound(id))?.clone();
        let mut after = current.clone();
        f(&mut after);
        if after == current {
            return Ok(());
        }
        Transaction::new(label)
            .replace(current, after.clone())
            .apply(&mut self.model)?;
        if let Some(crate::Op::Replace { after: a, .. }) =
            self.undo.last_mut().and_then(|t| t.ops.first_mut())
        {
            *a = after;
        }
        Ok(())
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|t| t.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|t| t.label.as_str())
    }

    pub fn undo(&mut self) -> bool {
        let Some(tx) = self.undo.pop() else {
            return false;
        };
        tx.inverse()
            .apply(&mut self.model)
            .expect("undoing a committed transaction cannot fail");
        self.redo.push(tx);
        true
    }

    /// Take back the last edit for good, without offering it for redo: for
    /// a drag or a keyboard move that was cancelled.
    pub fn discard_last(&mut self) -> bool {
        let Some(tx) = self.undo.pop() else {
            return false;
        };
        tx.inverse()
            .apply(&mut self.model)
            .expect("undoing a committed transaction cannot fail");
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(tx) = self.redo.pop() else {
            return false;
        };
        tx.apply(&mut self.model)
            .expect("redoing an undone transaction cannot fail");
        self.undo.push(tx);
        true
    }
}
