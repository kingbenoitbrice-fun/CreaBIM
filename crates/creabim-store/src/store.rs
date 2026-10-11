//! Incremental recording into an open project file.

use crate::{
    append_commit, append_move, now_ms, read_document, read_meta_from, save_fresh, schema,
    set_meta, timeline, to_json, touch, Meta, Move, Result,
};
use creabim_core::{Document, Element, Op, StepId};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

/// An open project file that follows a [`Document`] as it is edited.
///
/// The store remembers which undo steps it has written. [`Store::record`]
/// compares that with the document and appends only the difference, in one
/// SQLite transaction, so the file is always either before or after a whole
/// edit. Call it after each commit, undo or redo (or less often: any number
/// of changes in between are worked out).
pub struct Store {
    conn: Connection,
    path: PathBuf,
    /// Step ids of the recorded timeline (undo stack, then redo reversed).
    steps: Vec<StepId>,
    /// Journal row of the commit behind each entry of `steps`.
    seqs: Vec<i64>,
    /// How many timeline steps are applied (the undo stack's length).
    cursor: usize,
    /// Newest journal row, to tell whether the top step may be amended in place.
    last_seq: i64,
}

impl Store {
    /// Save `doc` to `path` (replacing any file there) and keep it open.
    pub fn create(path: impl AsRef<Path>, doc: &Document) -> Result<Store> {
        let path = path.as_ref();
        let seqs = save_fresh(path, doc)?;
        Self::attach(path, doc, seqs)
    }

    /// Open a project and keep it open; returns the store and the document it
    /// follows.
    pub fn open(path: impl AsRef<Path>) -> Result<(Store, Document)> {
        let path = path.as_ref();
        let (doc, seqs) = {
            let mut conn = schema::open_existing(path)?;
            let tx = conn.transaction()?;
            read_document(&tx)?
        };
        Ok((Self::attach(path, &doc, seqs)?, doc))
    }

    fn attach(path: &Path, doc: &Document, seqs: Vec<i64>) -> Result<Store> {
        let conn = schema::open_existing(path)?;
        // WAL: a commit appends to a side file and never rewrites pages in
        // place, so a crash mid-write cannot damage what was already saved.
        // NORMAL sync may drop the very last commit on power loss, never corrupt.
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let last_seq = conn.query_row("SELECT COALESCE(MAX(seq), 0) FROM ops", [], |r| r.get(0))?;
        Ok(Store {
            conn,
            path: path.to_path_buf(),
            steps: timeline(doc).map(|s| s.id).collect(),
            seqs,
            cursor: doc.undo_steps().len(),
            last_seq,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn meta(&self) -> Result<Meta> {
        read_meta_from(&self.conn)
    }

    pub fn set_project_name(&mut self, name: &str) -> Result<()> {
        set_meta(&self.conn, "project_name", name)
    }

    /// Write whatever changed in `doc` since the last call. `doc` must be the
    /// document this store was created or opened with.
    pub fn record(&mut self, doc: &Document) -> Result<()> {
        let new_steps: Vec<StepId> = timeline(doc).map(|s| s.id).collect();
        let new_cursor = doc.undo_steps().len();
        if new_steps == self.steps && new_cursor == self.cursor {
            return Ok(());
        }
        let step_at = |i: usize| timeline(doc).nth(i).expect("index within timeline");
        let now = now_ms();
        let tx = self.conn.transaction()?;
        let fork = self
            .steps
            .iter()
            .zip(&new_steps)
            .take_while(|(a, b)| a == b)
            .count();
        let len = self.steps.len();
        let mut last_seq = self.last_seq;
        let mut seqs = self.seqs.clone();

        let amended_top = fork + 1 == len
            && new_steps.len() == len
            && self.cursor == len
            && new_cursor == len
            && self.seqs[fork] == self.last_seq;
        if amended_top {
            // A merged edit (dragging a value) rewrote the newest step: rewrite
            // its row too instead of logging one step per frame.
            let old_inverse: String = tx.query_row(
                "SELECT inverse FROM ops WHERE seq = ?1",
                [self.last_seq],
                |r| r.get(0),
            )?;
            apply_ops(&tx, &crate::from_json::<Vec<Op>>(&old_inverse)?)?;
            let t = &step_at(fork).tx;
            tx.execute(
                "UPDATE ops SET ts = ?1, label = ?2, forward = ?3, inverse = ?4 WHERE seq = ?5",
                params![
                    now,
                    t.label,
                    to_json(&t.ops)?,
                    to_json(&t.inverse().ops)?,
                    self.last_seq
                ],
            )?;
            apply_ops(&tx, &t.ops)?;
        } else {
            let mut cursor = self.cursor;
            if new_steps != self.steps {
                // walk the old timeline to where the histories part, then
                // commit the new branch from there
                while cursor > fork {
                    cursor -= 1;
                    last_seq = move_step(&tx, now, Move::Undo, seqs[cursor])?;
                }
                while cursor < fork {
                    last_seq = move_step(&tx, now, Move::Redo, seqs[cursor])?;
                    cursor += 1;
                }
                seqs.truncate(fork);
                for i in fork..new_steps.len() {
                    let t = &step_at(i).tx;
                    last_seq = append_commit(&tx, now, t)?;
                    apply_ops(&tx, &t.ops)?;
                    seqs.push(last_seq);
                }
                cursor = new_steps.len();
            }
            while cursor > new_cursor {
                cursor -= 1;
                last_seq = move_step(&tx, now, Move::Undo, seqs[cursor])?;
            }
            while cursor < new_cursor {
                last_seq = move_step(&tx, now, Move::Redo, seqs[cursor])?;
                cursor += 1;
            }
        }

        let count: i64 = tx.query_row("SELECT COUNT(*) FROM elements", [], |r| r.get(0))?;
        if count as usize != doc.model().len() {
            // only reachable if `doc` is not the document this store follows;
            // keep the file truthful by rewriting the elements wholesale
            debug_assert!(false, "store and document diverged");
            rewrite_elements(&tx, doc)?;
        }
        touch(&tx)?;
        tx.commit()?;

        self.steps = new_steps;
        self.seqs = seqs;
        self.cursor = new_cursor;
        self.last_seq = last_seq;
        Ok(())
    }

    /// Close the file, folding the WAL back into it so the `.creabim` file is
    /// complete on its own (also happens on drop, minus error reporting).
    pub fn close(self) -> Result<()> {
        self.conn.close().map_err(|(_, e)| e.into())
    }
}

fn move_step(conn: &Connection, ts: i64, mv: Move, step: i64) -> Result<i64> {
    let (seq, ops) = append_move(conn, ts, mv, step)?;
    apply_ops(conn, &ops)?;
    Ok(seq)
}

/// Mirror ops onto the `elements` table the way [`creabim_core::Model`]
/// applies them: inserts go to the end, replacements keep their place.
fn apply_ops(conn: &Connection, ops: &[Op]) -> Result<()> {
    for op in ops {
        match op {
            Op::Insert(e) => {
                delete(conn, e)?;
                conn.prepare_cached(
                    "INSERT INTO elements (id, ord, kind, name, data)
                     VALUES (?1, (SELECT COALESCE(MAX(ord), -1) + 1 FROM elements), ?2, ?3, ?4)",
                )?
                .execute(params![
                    id_text(e),
                    e.kind.name(),
                    e.name,
                    to_json(e)?
                ])?;
            }
            Op::Remove(e) => delete(conn, e)?,
            Op::Replace { after: e, .. } => {
                conn.prepare_cached(
                    "UPDATE elements SET kind = ?2, name = ?3, data = ?4 WHERE id = ?1",
                )?
                .execute(params![
                    id_text(e),
                    e.kind.name(),
                    e.name,
                    to_json(e)?
                ])?;
            }
        }
    }
    Ok(())
}

fn delete(conn: &Connection, e: &Element) -> Result<()> {
    conn.prepare_cached("DELETE FROM elements WHERE id = ?1")?
        .execute([id_text(e)])?;
    Ok(())
}

fn rewrite_elements(conn: &Connection, doc: &Document) -> Result<()> {
    conn.execute("DELETE FROM elements", [])?;
    let ops: Vec<Op> = doc.model().iter().cloned().map(Op::Insert).collect();
    apply_ops(conn, &ops)
}

fn id_text(e: &Element) -> String {
    e.id.0.hyphenated().to_string()
}
