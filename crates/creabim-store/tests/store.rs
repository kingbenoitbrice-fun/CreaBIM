use creabim_core::{sample::sample_house, Document, Element, ElementKind, Level, Transaction};
use creabim_store::{import_json, load, open, read_meta, save, Store, StoreError};
use std::path::{Path, PathBuf};

fn tmp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("house.creabim");
    (dir, path)
}

fn first_wall(doc: &Document) -> creabim_core::ElementId {
    doc.model()
        .iter()
        .find(|e| matches!(e.kind, ElementKind::Wall(_)))
        .unwrap()
        .id
}

fn set_thickness(t: f64) -> impl FnOnce(&mut Element) {
    move |e| {
        if let ElementKind::Wall(w) = &mut e.kind {
            w.thickness = t;
        }
    }
}

fn add_level(doc: &mut Document, name: &str) -> creabim_core::ElementId {
    let l = Element::new(
        name,
        ElementKind::Level(Level {
            elevation: 6.0,
            height: 2.8,
        }),
    );
    let id = l.id;
    doc.commit(Transaction::new(format!("Add {name}")).insert(l))
        .unwrap();
    id
}

/// Model and both stacks, compared step by step.
fn assert_same(a: &Document, b: &Document) {
    assert_eq!(a.model(), b.model());
    let txs = |s: &[creabim_core::Step]| s.iter().map(|s| s.tx.clone()).collect::<Vec<_>>();
    assert_eq!(
        txs(a.undo_steps()),
        txs(b.undo_steps()),
        "undo stacks differ"
    );
    assert_eq!(
        txs(a.redo_steps()),
        txs(b.redo_steps()),
        "redo stacks differ"
    );
}

/// A few edits of every kind on the sample house, one of them undone.
fn edited_sample() -> Document {
    let mut doc = Document::from_model(sample_house());
    let wall = first_wall(&doc);
    doc.edit(wall, "Thicken wall", set_thickness(0.35)).unwrap();
    add_level(&mut doc, "Roof");
    let tx = Transaction::new("Delete wall").remove_cascade(doc.model(), wall);
    doc.commit(tx).unwrap();
    doc.undo();
    doc
}

#[test]
fn save_then_open_round_trips_the_sample_house() {
    let (_dir, path) = tmp();
    let doc = Document::from_model(sample_house());
    save(&path, &doc).unwrap();
    let back = open(&path).unwrap();
    assert_same(&doc, &back);
    // element order matters to views, so it is kept exactly
    let ids = |d: &Document| d.model().iter().map(|e| e.id).collect::<Vec<_>>();
    assert_eq!(ids(&doc), ids(&back));
}

#[test]
fn save_then_open_keeps_undo_and_redo() {
    let (_dir, path) = tmp();
    let mut doc = edited_sample();
    save(&path, &doc).unwrap();
    let mut back = open(&path).unwrap();
    assert_same(&doc, &back);
    assert_eq!(back.redo_label(), Some("Delete wall"));

    // the restored history behaves exactly like the live one
    assert!(back.redo() && doc.redo());
    assert_same(&doc, &back);
    while doc.undo() {
        assert!(back.undo());
        assert_eq!(doc.model(), back.model());
    }
    assert!(!back.undo());
}

#[test]
fn saving_over_an_existing_project_replaces_it() {
    let (_dir, path) = tmp();
    save(&path, &edited_sample()).unwrap();
    let doc = Document::from_model(sample_house());
    save(&path, &doc).unwrap();
    assert_same(&open(&path).unwrap(), &doc);
    assert_eq!(read_meta(&path).unwrap().project_name, "house");
}

#[test]
fn recording_each_change_then_reopening_gives_the_same_document() {
    let (_dir, path) = tmp();
    let mut doc = Document::from_model(sample_house());
    let mut store = Store::create(&path, &doc).unwrap();
    let wall = first_wall(&doc);

    doc.edit(wall, "Thicken wall", set_thickness(0.3)).unwrap();
    store.record(&doc).unwrap();
    add_level(&mut doc, "Roof");
    store.record(&doc).unwrap();
    doc.undo();
    store.record(&doc).unwrap();
    doc.redo();
    store.record(&doc).unwrap();
    doc.undo();
    store.record(&doc).unwrap();
    // a new edit after an undo drops the redo branch
    let tx = Transaction::new("Delete wall").remove_cascade(doc.model(), wall);
    doc.commit(tx).unwrap();
    store.record(&doc).unwrap();
    doc.undo();
    store.record(&doc).unwrap();
    assert_same(&open(&path).unwrap(), &doc);
    drop(store);

    let (store, back) = Store::open(&path).unwrap();
    assert_same(&doc, &back);
    store.close().unwrap();
}

#[test]
fn recorded_changes_survive_the_app_dying_without_closing() {
    let (_dir, path) = tmp();
    let mut doc = Document::from_model(sample_house());
    let mut store = Store::create(&path, &doc).unwrap();
    add_level(&mut doc, "Roof");
    store.record(&doc).unwrap();
    // no close, no checkpoint: the change lives only in the WAL
    std::mem::forget(store);
    assert_same(&open(&path).unwrap(), &doc);
}

#[test]
fn merged_edits_amend_one_journal_row() {
    let (_dir, path) = tmp();
    let mut doc = Document::from_model(sample_house());
    let mut store = Store::create(&path, &doc).unwrap();
    let wall = first_wall(&doc);
    let rows = |p: &Path| -> i64 {
        let c = rusqlite::Connection::open(p).unwrap();
        c.query_row("SELECT COUNT(*) FROM ops", [], |r| r.get(0))
            .unwrap()
    };
    for t in [0.21, 0.22, 0.23, 0.24] {
        doc.edit_merged(wall, "Wall thickness", set_thickness(t))
            .unwrap();
        store.record(&doc).unwrap();
    }
    assert_eq!(rows(&path), 1, "a drag is one step on disk as in memory");
    assert_same(&open(&path).unwrap(), &doc);

    // after an undo/redo the step is no longer the newest row: amending it
    // again is logged, and still reopens to the same state
    doc.undo();
    doc.redo();
    store.record(&doc).unwrap();
    doc.edit_merged(wall, "Wall thickness", set_thickness(0.5))
        .unwrap();
    store.record(&doc).unwrap();
    store.close().unwrap();
    assert_same(&open(&path).unwrap(), &doc);
}

#[test]
fn recording_several_changes_at_once_and_continuing_after_reopen() {
    let (_dir, path) = tmp();
    let mut doc = edited_sample();
    let mut store = Store::create(&path, &doc).unwrap();
    // several changes between records, including undo past the fork point
    doc.undo();
    doc.undo();
    add_level(&mut doc, "Attic");
    add_level(&mut doc, "Loft");
    doc.undo();
    store.record(&doc).unwrap();
    store.record(&doc).unwrap(); // nothing changed: a no-op
    drop(store);

    let (mut store, mut back) = Store::open(&path).unwrap();
    assert_same(&doc, &back);
    for d in [&mut doc, &mut back] {
        d.redo();
        add_level(d, "Mezzanine");
        d.undo();
    }
    store.record(&back).unwrap();
    store.close().unwrap();
    let reopened = open(&path).unwrap();
    assert_eq!(reopened.model(), doc.model());
    assert_eq!(reopened.redo_label(), Some("Add Mezzanine"));
    assert_eq!(reopened.undo_steps().len(), doc.undo_steps().len());
}

#[test]
fn elements_are_queryable_by_kind() {
    let (_dir, path) = tmp();
    let doc = Document::from_model(sample_house());
    save(&path, &doc).unwrap();
    let c = rusqlite::Connection::open(&path).unwrap();
    let walls: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM elements WHERE kind = 'Wall'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let expected = doc
        .model()
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Wall(_)))
        .count();
    assert!(walls > 0);
    assert_eq!(walls as usize, expected);
    let name: String = c
        .query_row(
            "SELECT json_extract(data, '$.name') FROM elements WHERE kind = 'Level' ORDER BY ord",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name, "Ground Floor");
}

#[test]
fn a_file_from_a_newer_version_is_refused_clearly() {
    let (_dir, path) = tmp();
    save(&path, &Document::from_model(sample_house())).unwrap();
    rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    let err = open(&path).unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::NewerFormat {
                found: 99,
                supported: 1
            }
        ),
        "{err:?}"
    );
    assert!(err.to_string().contains("newer CreaBIM"));
}

/// Run SQL against a file and close it again (Windows will not replace a
/// file that is still open).
fn sql(path: &Path, statement: &str) {
    rusqlite::Connection::open(path)
        .unwrap()
        .execute_batch(statement)
        .unwrap();
}

#[test]
fn damaged_or_foreign_files_give_errors_not_panics() {
    let (dir, path) = tmp();

    std::fs::write(
        &path,
        b"this is not a database, just some text padding it out",
    )
    .unwrap();
    assert!(matches!(open(&path), Err(StoreError::NotAProject)));

    let other = dir.path().join("other.db");
    rusqlite::Connection::open(&other)
        .unwrap()
        .execute_batch("CREATE TABLE t (x)")
        .unwrap();
    assert!(matches!(open(&other), Err(StoreError::NotAProject)));

    save(&path, &edited_sample()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let cut = dir.path().join("cut.creabim");
    std::fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
    assert!(open(&cut).is_err());

    sql(&path, "UPDATE elements SET data = '{oops' WHERE ord = 0");
    assert!(matches!(open(&path), Err(StoreError::Corrupt(_))));

    save(&path, &edited_sample()).unwrap();
    sql(&path, "UPDATE ops SET step = 1 WHERE action = 'undo'");
    assert!(matches!(open(&path), Err(StoreError::Corrupt(_))));

    save(&path, &edited_sample()).unwrap();
    // drop the undo row: the journal now claims "Delete wall" is applied
    sql(
        &path,
        "DELETE FROM ops WHERE seq = (SELECT MAX(seq) FROM ops)",
    );
    assert!(
        matches!(open(&path), Err(StoreError::History(_))),
        "a history that no longer fits the model"
    );

    assert!(matches!(
        open(dir.path().join("missing.creabim")),
        Err(StoreError::Io(_))
    ));
    assert!(!dir.path().join("missing.creabim").exists());
}

#[test]
fn saving_over_a_project_that_is_open_is_refused() {
    let (_dir, path) = tmp();
    let doc = Document::from_model(sample_house());
    let store = Store::create(&path, &doc).unwrap();
    assert!(save(&path, &edited_sample()).is_err());
    store.close().unwrap();
    // the original is intact
    assert_same(&open(&path).unwrap(), &doc);
}

#[test]
fn json_projects_import_and_load_dispatches_by_extension() {
    let (dir, path) = tmp();
    let json = dir.path().join("house.creabim.json");
    let model = sample_house();
    std::fs::write(&json, model.to_json()).unwrap();
    let doc = import_json(&json).unwrap();
    assert_eq!(doc.model().len(), model.len());
    assert!(!doc.can_undo());
    save(&path, &doc).unwrap();
    assert_eq!(load(&path).unwrap().model(), doc.model());
    assert_eq!(load(&json).unwrap().model(), doc.model());
    assert!(matches!(
        load(dir.path().join("x.json")),
        Err(StoreError::Io(_))
    ));
}

#[test]
fn meta_records_the_project() {
    let (_dir, path) = tmp();
    let doc = Document::from_model(sample_house());
    let mut store = Store::create(&path, &doc).unwrap();
    store.set_project_name("Maison").unwrap();
    let meta = store.meta().unwrap();
    assert_eq!(meta.project_name, "Maison");
    assert_eq!(meta.format_version, creabim_store::FORMAT_VERSION);
    assert_eq!(meta.app_version, env!("CARGO_PKG_VERSION"));
    assert!(meta.created_ms > 0 && meta.modified_ms >= meta.created_ms);
}

#[test]
fn loading_the_sample_is_quick_to_do() {
    let (_dir, path) = tmp();
    let doc = Document::from_model(sample_house());
    save(&path, &doc).unwrap();
    for _ in 0..20 {
        assert_eq!(open(&path).unwrap().model().len(), doc.model().len());
    }
}
