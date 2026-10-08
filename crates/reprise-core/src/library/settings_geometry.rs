//! Persisted list geometry for the single, comfortable list density.

use rusqlite::{Connection, Error as SqlError};

use super::{get_setting_in, set_setting_in};

pub const ROW_HEIGHT_KEY: &str = "ui.row_height";
pub const SECTION_HEADER_HEIGHT_KEY: &str = "ui.section_header_height";

/// Keys that were written by older versions and replaced by `ROW_HEIGHT_KEY`.
/// The split between comfortable and compact density was collapsed into a single
/// comfortable size (v79), so these rows are dead weight in any database that
/// ran the old schema.
const DEAD_ROW_HEIGHT_KEYS: &[&str] = &["ui.row_height.comfortable", "ui.row_height.compact"];
const POISONED_GEOMETRY_KEYS: &[&str] = &[ROW_HEIGHT_KEY, SECTION_HEADER_HEIGHT_KEY];

fn get_height_in(conn: &Connection, key: &str) -> Result<Option<f64>, SqlError> {
    Ok(get_setting_in(conn, key)?
        .and_then(|value| value.parse().ok())
        .filter(|height: &f64| height.is_finite() && *height > 0.0))
}

fn set_height_in(conn: &Connection, key: &str, height: Option<f64>) -> Result<(), SqlError> {
    set_setting_in(conn, key, &height.unwrap_or(0.0).to_string())
}

pub fn get_row_height(db: &crate::db::Db) -> Result<Option<f64>, SqlError> {
    get_height_in(db.conn(), ROW_HEIGHT_KEY)
}

pub fn set_row_height(db: &crate::db::Db, height: Option<f64>) -> Result<(), SqlError> {
    set_height_in(db.conn(), ROW_HEIGHT_KEY, height)
}

pub fn get_section_header_height(db: &crate::db::Db) -> Result<Option<f64>, SqlError> {
    get_height_in(db.conn(), SECTION_HEADER_HEIGHT_KEY)
}

pub fn set_section_header_height(db: &crate::db::Db, height: Option<f64>) -> Result<(), SqlError> {
    set_height_in(db.conn(), SECTION_HEADER_HEIGHT_KEY, height)
}

pub fn set_row_and_section_header_heights(
    db: &crate::db::Db,
    row_height: f64,
    section_header_height: f64,
) -> Result<(), SqlError> {
    // Settle an unchanged pair before any transaction opens: a settled list
    // re-persists the heights it already has, and that must never queue for
    // the write lock behind another writer (mirrors `set_setting_in`).
    let conn = db.conn();
    let row_text = row_height.to_string();
    let header_text = section_header_height.to_string();
    if conn.is_autocommit()
        && get_setting_in(conn, ROW_HEIGHT_KEY)?.as_deref() == Some(row_text.as_str())
        && get_setting_in(conn, SECTION_HEADER_HEIGHT_KEY)?.as_deref() == Some(header_text.as_str())
    {
        return Ok(());
    }
    // IMMEDIATE: each `set_height_in` reads (the dedup check) before it writes,
    // so a deferred outer transaction would pin a read snapshot first and fail
    // the write-lock upgrade with `SQLITE_BUSY_SNAPSHOT` when a rival commits
    // in between (see `events::in_txn_immediate`).
    crate::events::in_txn_immediate(conn, |conn| {
        set_height_in(conn, ROW_HEIGHT_KEY, Some(row_height))?;
        set_height_in(conn, SECTION_HEADER_HEIGHT_KEY, Some(section_header_height))
    })
}

/// Schema v79: deletes settings rows written by the old per-density row-height
/// scheme (`ui.row_height.comfortable`, `ui.row_height.compact`). Both were
/// retired when the list density was consolidated into a single comfortable
/// size persisted under `ui.row_height`. Any live measurement in the database
/// already lives under that key; the compact and comfortable rows are orphaned.
///
/// Idempotent: the `DELETE` is a no-op when the rows are already gone, and the
/// version guard short-circuits on every subsequent open.
pub(crate) fn migrate_v79(conn: &Connection) -> Result<(), SqlError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 79 {
        return Ok(());
    }
    let transaction = crate::db_migrations::begin_step(conn)?;
    for key in DEAD_ROW_HEIGHT_KEYS {
        transaction.execute(
            "DELETE FROM settings WHERE key = ?1",
            rusqlite::params![key],
        )?;
    }
    transaction.pragma_update(None, "user_version", 79)?;
    transaction.commit()
}

/// Schema v80: clears list geometry persisted by the self-certifying height
/// loop. With no stored value, the frontend starts from its conservative
/// assumed height until GTK supplies an authoritative measurement.
///
/// Idempotent: the version guard leaves databases that already completed this
/// one-time cleanup entirely alone.
pub(crate) fn migrate_v80(conn: &Connection) -> Result<(), SqlError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 80 {
        return Ok(());
    }
    let transaction = crate::db_migrations::begin_step(conn)?;
    for key in POISONED_GEOMETRY_KEYS {
        transaction.execute(
            "DELETE FROM settings WHERE key = ?1",
            rusqlite::params![key],
        )?;
    }
    transaction.pragma_update(None, "user_version", 80)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::super::{get_setting_in, set_setting_in};
    use super::*;

    /// The pair write reads each stored height (the dedup check) before it
    /// writes. Under a deferred outer transaction a rival commit between that
    /// read and the write fails it with `SQLITE_BUSY_SNAPSHOT` (#1185).
    #[test]
    fn the_height_pair_write_survives_a_rival_commit_between_its_read_and_its_write() {
        use std::sync::atomic::Ordering;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reprise.db");
        let writer = crate::db::Db::open_migrated(Some(&path)).unwrap();
        let interleaved =
            crate::library::rival_commit_test_support::arm(writer.conn(), &path, "settings");

        set_row_and_section_header_heights(&writer, 34.0, 28.0).unwrap();

        assert!(
            interleaved.load(Ordering::SeqCst),
            "the rival write must have been attempted between the read and the write",
        );
        assert_eq!(get_row_height(&writer).unwrap(), Some(34.0));
        assert_eq!(get_section_header_height(&writer).unwrap(), Some(28.0));
    }

    /// Re-persisting the heights already stored is the common case; it must
    /// not wait for the write lock another connection holds.
    #[test]
    fn an_unchanged_height_pair_is_a_no_op_that_never_waits_for_the_write_lock() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reprise.db");
        let writer = crate::db::Db::open_migrated(Some(&path)).unwrap();
        writer
            .conn()
            .pragma_update(None, "busy_timeout", 0)
            .unwrap();
        set_row_and_section_header_heights(&writer, 34.0, 28.0).unwrap();
        let holder = Connection::open(&path).unwrap();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();

        set_row_and_section_header_heights(&writer, 34.0, 28.0).unwrap();

        assert!(
            set_row_and_section_header_heights(&writer, 34.0, 30.0).is_err(),
            "a changed pair does need the write lock the holder keeps",
        );
    }

    fn open_at_v78() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        // Downgrade to v78 so v79 migration runs from the expected starting state.
        conn.pragma_update(None, "user_version", 78).unwrap();
        conn
    }

    #[test]
    fn v79_drops_dead_per_density_row_height_keys() {
        let conn = open_at_v78();
        // Seed the dead keys as an old installation would have left them.
        set_setting_in(&conn, "ui.row_height.comfortable", "34").unwrap();
        set_setting_in(&conn, "ui.row_height.compact", "28").unwrap();
        // The live key must survive the migration.
        set_setting_in(&conn, ROW_HEIGHT_KEY, "34").unwrap();

        migrate_v79(&conn).unwrap();

        assert_eq!(
            get_setting_in(&conn, "ui.row_height.comfortable").unwrap(),
            None,
            "comfortable key must be deleted"
        );
        assert_eq!(
            get_setting_in(&conn, "ui.row_height.compact").unwrap(),
            None,
            "compact key must be deleted"
        );
        assert_eq!(
            get_setting_in(&conn, ROW_HEIGHT_KEY).unwrap(),
            Some("34".to_owned()),
            "the live row height key must survive the migration"
        );
    }

    #[test]
    fn v79_is_idempotent_when_dead_keys_are_already_absent() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        // Second call must be a no-op (version guard).
        migrate_v79(&conn).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, crate::db::SUPPORTED_SCHEMA_VERSION);
    }

    #[test]
    fn row_and_section_header_heights_round_trip_independently() {
        let db = crate::db::Db::open_in_memory().unwrap();

        set_row_height(&db, Some(34.0)).unwrap();
        set_section_header_height(&db, Some(36.0)).unwrap();

        assert_eq!(get_row_height(&db).unwrap(), Some(34.0));
        assert_eq!(get_section_header_height(&db).unwrap(), Some(36.0));
    }

    #[test]
    fn invalid_or_cleared_geometry_is_not_returned() {
        let db = crate::db::Db::open_in_memory().unwrap();

        set_setting_in(db.conn(), SECTION_HEADER_HEIGHT_KEY, "NaN").unwrap();
        assert_eq!(get_section_header_height(&db).unwrap(), None);
        set_setting_in(db.conn(), SECTION_HEADER_HEIGHT_KEY, "-1").unwrap();
        assert_eq!(get_section_header_height(&db).unwrap(), None);
        set_section_header_height(&db, None).unwrap();
        assert_eq!(get_section_header_height(&db).unwrap(), None);
    }

    #[test]
    fn settled_row_and_header_heights_are_written_together() {
        let db = crate::db::Db::open_in_memory().unwrap();

        set_row_and_section_header_heights(&db, 34.0, 38.0).unwrap();

        assert_eq!(get_row_height(&db).unwrap(), Some(34.0));
        assert_eq!(get_section_header_height(&db).unwrap(), Some(38.0));
    }
}
