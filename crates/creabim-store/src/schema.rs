//! Table layout, file identification and migrations.

use crate::{Result, StoreError};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

/// The file format this build writes, stored in `PRAGMA user_version`.
pub const FORMAT_VERSION: i32 = 1;

/// `PRAGMA application_id` of every `.creabim` file ("CRBI"), so a random
/// SQLite database is recognised as "not a project" rather than misread.
pub(crate) const APPLICATION_ID: i32 = 0x4352_4249;

/// Version 1 of the schema.
///
/// * `meta`: key/value facts about the project (see [`crate::Meta`]).
/// * `elements`: the current model, one row per element. `data` is the whole
///   element as JSON, so `json_extract` can query any field; `kind` and `name`
///   are copied out for cheap filtering. `ord` keeps the model's order exact.
/// * `ops`: the append-only edit journal. A `commit` row holds a transaction;
///   `undo` and `redo` rows point (`step`) at the commit they move. Every row
///   carries the ops that change the model in its own direction (`forward`)
///   and back (`inverse`), so the journal alone can replay any version.
const SCHEMA_V1: &str = "
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE elements (
    id   TEXT PRIMARY KEY,
    ord  INTEGER NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE UNIQUE INDEX elements_ord ON elements(ord);
CREATE INDEX elements_kind ON elements(kind);

CREATE TABLE ops (
    seq     INTEGER PRIMARY KEY,
    ts      INTEGER NOT NULL,
    action  TEXT NOT NULL CHECK (action IN ('commit', 'undo', 'redo')),
    label   TEXT NOT NULL,
    forward TEXT NOT NULL,
    inverse TEXT NOT NULL,
    step    INTEGER REFERENCES ops(seq)
);
";

/// SQL upgrading a file from version `i + 1` to `i + 2`. Empty while version 1
/// is the only format; a future change appends its `ALTER TABLE ...` here and
/// bumps [`FORMAT_VERSION`].
const MIGRATIONS: &[&str] = &[];
const _: () = assert!(MIGRATIONS.len() as i32 == FORMAT_VERSION - 1);

/// Create the current schema in an empty database.
pub(crate) fn create(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_V1)?;
    conn.pragma_update(None, "application_id", APPLICATION_ID)?;
    conn.pragma_update(None, "user_version", FORMAT_VERSION)?;
    Ok(())
}

/// Open an existing project file for reading and writing, check it is one of
/// ours and bring it up to the current format.
pub(crate) fn open_existing(path: &Path) -> Result<Connection> {
    if !path.is_file() {
        return Err(StoreError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no such project file",
        )));
    }
    // no CREATE flag: a typo must not leave an empty database behind
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let mut conn = Connection::open_with_flags(path, flags)?;
    check_and_migrate(&mut conn)?;
    Ok(conn)
}

fn check_and_migrate(conn: &mut Connection) -> Result<()> {
    let app: i32 = conn.pragma_query_value(None, "application_id", |r| r.get(0))?;
    if app != APPLICATION_ID {
        return Err(StoreError::NotAProject);
    }
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > FORMAT_VERSION {
        return Err(StoreError::NewerFormat {
            found: version,
            supported: FORMAT_VERSION,
        });
    }
    if version < 1 {
        return Err(StoreError::Corrupt(format!(
            "unknown format version {version}"
        )));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize - 1) {
        // each step is its own transaction, so an interrupted upgrade leaves
        // the file at a valid older version that the next open resumes from
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i32 + 2)?;
        tx.commit()?;
    }
    Ok(())
}
