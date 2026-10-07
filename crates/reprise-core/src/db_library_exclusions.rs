//! Persistent, identity-based library exclusions: the v25 schema, and the typed
//! reads and writes of the columns v91 added for an excluded CUE segment.

use rusqlite::{Connection, OptionalExtension};

const SCHEMA_V25: &str = r#"
CREATE TABLE IF NOT EXISTS library_exclusions (
  id          INTEGER PRIMARY KEY,
  path        TEXT NOT NULL,
  device      INTEGER,
  inode       INTEGER,
  file_size   INTEGER NOT NULL DEFAULT 0,
  file_mtime  INTEGER NOT NULL DEFAULT 0,
  excluded_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_library_exclusions_path
  ON library_exclusions(path)
  WHERE device IS NULL OR inode IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_library_exclusions_identity
  ON library_exclusions(device, inode)
  WHERE device IS NOT NULL AND inode IS NOT NULL;
"#;

pub(crate) fn migrate_v25(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 25 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V25)?;
    transaction.pragma_update(None, "user_version", 25)?;
    transaction.commit()
}

/// Every column a track exclusion writes. A segment carries its start, title
/// and the version of the sheet beside the file; a whole file carries none of
/// them (v91).
const RECORDED_COLUMNS: &str = "path, device, inode, file_size, file_mtime, excluded_at, \
     segment_index, segment_start_ms, segment_title, cue_path, cue_mtime, cue_size";

/// The same columns, read from the track row being excluded.
const RECORDED_VALUES: &str = "path, device, inode, file_size, file_mtime, ?3, segment_index, \
     CASE WHEN segment_index > 0 THEN segment_start_ms END, \
     CASE WHEN segment_index > 0 THEN title END, \
     CASE WHEN segment_index > 0 THEN cue_path END, \
     CASE WHEN segment_index > 0 THEN cue_mtime END, \
     CASE WHEN segment_index > 0 THEN cue_size END";

/// On a clash with either unique index the exclusion takes the new values
/// column by column: every column is named, so none is left behind as NULL.
const RECORDED_UPDATE: &str = "path = excluded.path, device = excluded.device, \
     inode = excluded.inode, file_size = excluded.file_size, \
     file_mtime = excluded.file_mtime, excluded_at = excluded.excluded_at, \
     segment_start_ms = excluded.segment_start_ms, segment_title = excluded.segment_title, \
     cue_path = excluded.cue_path, cue_mtime = excluded.cue_mtime, cue_size = excluded.cue_size";

/// Excludes track `track_id`, when it still sits at `expected_path`, with
/// every identity column of its row. Returns whether a row was written.
pub(crate) fn record_track(
    conn: &Connection,
    track_id: i64,
    expected_path: &str,
    excluded_at: i64,
) -> Result<bool, rusqlite::Error> {
    let changed = conn.execute(
        &format!(
            "INSERT INTO library_exclusions ({RECORDED_COLUMNS})
             SELECT {RECORDED_VALUES} FROM tracks WHERE id = ?1 AND path = ?2
             ON CONFLICT (device, inode, segment_index)
               WHERE device IS NOT NULL AND inode IS NOT NULL
               DO UPDATE SET {RECORDED_UPDATE}
             ON CONFLICT (path, segment_index)
               WHERE device IS NULL OR inode IS NULL
               DO UPDATE SET {RECORDED_UPDATE}"
        ),
        rusqlite::params![track_id, expected_path, excluded_at],
    )?;
    Ok(changed == 1)
}

/// Which exclusions name a file: by its stable identity where both sides have
/// one, by its exact path otherwise. Parameters `?1` path, `?2` device, `?3` inode.
const NAMES_FILE: &str =
    "((device IS NOT NULL AND inode IS NOT NULL AND device = ?2 AND inode = ?3)
     OR ((device IS NULL OR inode IS NULL) AND path = ?1))";

/// Whether exclusion `segment_index` of the file exists.
pub(crate) fn exists(
    conn: &Connection,
    path: &str,
    device: Option<i64>,
    inode: Option<i64>,
    segment_index: i64,
) -> Result<bool, rusqlite::Error> {
    conn.prepare_cached(&format!(
        "SELECT EXISTS(SELECT 1 FROM library_exclusions WHERE segment_index = ?4 AND {NAMES_FILE})"
    ))?
    .query_row(
        rusqlite::params![path, device, inode, segment_index],
        |row| row.get(0),
    )
}

/// An excluded track of a CUE file, as the scanner matches it against the
/// file's current sheet. `index` is negative for one the sheet last read did
/// not have; it then matches by start and title only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SegmentExclusion {
    pub(crate) id: i64,
    pub(crate) index: i64,
    pub(crate) start_ms: Option<i64>,
    pub(crate) title: Option<String>,
}

/// The excluded tracks of a file, every one but the whole-file exclusion.
pub(crate) fn segment_exclusions(
    conn: &Connection,
    path: &str,
    device: Option<i64>,
    inode: Option<i64>,
) -> Result<Vec<SegmentExclusion>, rusqlite::Error> {
    let mut statement = conn.prepare_cached(&format!(
        "SELECT id, segment_index, segment_start_ms, segment_title FROM library_exclusions
         WHERE segment_index <> 0 AND {NAMES_FILE} ORDER BY segment_index, id"
    ))?;
    let rows = statement
        .query_map(rusqlite::params![path, device, inode], |row| {
            Ok(SegmentExclusion {
                id: row.get(0)?,
                index: row.get(1)?,
                start_ms: row.get(2)?,
                title: row.get(3)?,
            })
        })?
        .collect();
    rows
}

/// Moves every listed exclusion to a position no track has, so each can then
/// take its song's current position whatever another one held before.
pub(crate) fn park_segment_exclusions(
    conn: &Connection,
    ids: &[i64],
) -> Result<(), rusqlite::Error> {
    let mut statement =
        conn.prepare_cached("UPDATE library_exclusions SET segment_index = -id WHERE id = ?1")?;
    for id in ids {
        statement.execute([id])?;
    }
    Ok(())
}

/// Where an excluded song sits in the sheet the scan just applied.
pub(crate) struct SegmentPlacement<'a> {
    pub(crate) index: i64,
    pub(crate) start_ms: i64,
    pub(crate) title: &'a str,
    /// The sheet beside the file as `(path, mtime, size)`; `None` for an embedded one.
    pub(crate) sheet: Option<(&'a str, i64, i64)>,
    pub(crate) file_mtime: i64,
    pub(crate) file_size: i64,
}

/// Points exclusion `id` at its song's current place, sheet and file version.
pub(crate) fn place_segment_exclusion(
    conn: &Connection,
    id: i64,
    placement: &SegmentPlacement<'_>,
) -> Result<(), rusqlite::Error> {
    let (cue_path, cue_mtime, cue_size) = placement
        .sheet
        .map_or((None, None, None), |(path, mtime, size)| {
            (Some(path), Some(mtime), Some(size))
        });
    conn.prepare_cached(
        "UPDATE library_exclusions SET segment_index = ?2, segment_start_ms = ?3,
                segment_title = ?4, cue_path = ?5, cue_mtime = ?6, cue_size = ?7,
                file_mtime = ?8, file_size = ?9
         WHERE id = ?1",
    )?
    .execute(rusqlite::params![
        id,
        placement.index,
        placement.start_ms,
        placement.title,
        cue_path,
        cue_mtime,
        cue_size,
        placement.file_mtime,
        placement.file_size,
    ])?;
    Ok(())
}

/// `(sheet path, (sheet mtime, sheet size), audio path)`, as the scanner keys
/// the sheets it applied.
pub(crate) type AppliedSheetRow = (String, (i64, i64), String);

/// A hidden file's mtime and the `(path, mtime, size)` of the sheet beside it.
pub(crate) type HiddenFileVersion = (i64, Option<(String, i64, i64)>);

/// `(sheet path, (sheet mtime, sheet size), audio path)` of every excluded
/// track a sheet beside its file placed, for audio paths matching the `LIKE`
/// pattern (escaped with a backslash). A track the sheet last read did not have
/// says nothing about the sheet's current version and is left out.
pub(crate) fn applied_sheet_rows(
    conn: &Connection,
    path_pattern: &str,
) -> Result<Vec<AppliedSheetRow>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT DISTINCT cue_path, cue_mtime, cue_size, path FROM library_exclusions
         WHERE segment_index > 0 AND cue_path IS NOT NULL AND cue_mtime IS NOT NULL
           AND cue_size IS NOT NULL AND path LIKE ?1 ESCAPE '\\'",
    )?;
    let rows = statement
        .query_map([path_pattern], |row| {
            Ok((row.get(0)?, (row.get(1)?, row.get(2)?), row.get(3)?))
        })?
        .collect();
    rows
}

/// What the excluded tracks of a file agree on: the file's mtime and the sheet
/// they were placed by, `(path, mtime, size)` or `None` for an embedded one.
/// `None` overall when the file has no placed exclusion, when they disagree, or
/// when one was written before v91 and so cannot tell an embedded sheet from
/// one beside the file.
pub(crate) fn hidden_file_version(
    conn: &Connection,
    path: &str,
    device: Option<i64>,
    inode: Option<i64>,
) -> Result<Option<HiddenFileVersion>, rusqlite::Error> {
    type Agreement = (
        i64,
        Option<i64>,
        Option<i64>,
        i64,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    );
    let row: Option<Agreement> = conn
        .prepare_cached(&format!(
            "SELECT count(segment_title), min(file_mtime), max(file_mtime), count(cue_path),
                    min(cue_path), max(cue_path), min(cue_mtime), max(cue_mtime),
                    min(cue_size), max(cue_size)
             FROM library_exclusions WHERE segment_index > 0 AND {NAMES_FILE}
             HAVING count(*) = count(segment_title)"
        ))?
        .query_row(rusqlite::params![path, device, inode], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
            ))
        })
        .optional()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let (rows, min_mtime, max_mtime, sheet_rows, min_sheet, max_sheet) =
        (row.0, row.1, row.2, row.3, row.4, row.5);
    let (min_sheet_mtime, max_sheet_mtime, min_sheet_size, max_sheet_size) =
        (row.6, row.7, row.8, row.9);
    let Some(mtime) = min_mtime.filter(|_| rows > 0 && min_mtime == max_mtime) else {
        return Ok(None);
    };
    if sheet_rows == 0 {
        return Ok(Some((mtime, None)));
    }
    let agreed = sheet_rows == rows
        && min_sheet == max_sheet
        && min_sheet_mtime == max_sheet_mtime
        && min_sheet_size == max_sheet_size;
    Ok(match (agreed, min_sheet, min_sheet_mtime, min_sheet_size) {
        (true, Some(sheet), Some(sheet_mtime), Some(sheet_size)) => {
            Some((mtime, Some((sheet, sheet_mtime, sheet_size))))
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn browse_7_v24_upgrade_adds_idempotent_exclusion_schema() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute_batch(
            "DROP TABLE library_exclusions;
             PRAGMA user_version = 24;",
        )
        .unwrap();

        super::migrate_v25(&conn).unwrap();
        super::migrate_v25(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 25);
        let table: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE type='table' AND name='library_exclusions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table, 1);
    }
}
