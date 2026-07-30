use std::collections::HashSet;
use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension};

use crate::db::Db;

use super::{DoctorScopeRequest, DoctorTrackRef, DoctorViewSnapshot, FrozenScope};

const PAGE_SIZE: i64 = 200;

pub(super) fn freeze_scope(
    db: &Db,
    request: &DoctorScopeRequest,
) -> Result<FrozenScope, rusqlite::Error> {
    let conn = db.conn();
    let tracks = match request {
        DoctorScopeRequest::WholeLibrary => whole_library(conn)?,
        DoctorScopeRequest::CurrentView(snapshot) => current_view(db, conn, snapshot)?,
        DoctorScopeRequest::Selection { track_ids } => selection(conn, track_ids)?,
    };
    if !matches!(request, DoctorScopeRequest::WholeLibrary) && tracks.is_empty() {
        return Ok(FrozenScope::FallbackRequired);
    }
    Ok(FrozenScope::Tracks(tracks))
}

fn whole_library(conn: &Connection) -> Result<Vec<DoctorTrackRef>, rusqlite::Error> {
    let mut statement = conn.prepare(&format!(
        "SELECT id, path, file_mtime, file_size, device, inode FROM tracks WHERE {} ORDER BY id",
        crate::queries::PRESENT
    ))?;
    let tracks = statement
        .query_map([], row_to_track_ref)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tracks)
}

fn current_view(
    db: &Db,
    conn: &Connection,
    snapshot: &DoctorViewSnapshot,
) -> Result<Vec<DoctorTrackRef>, rusqlite::Error> {
    conn.execute_batch("BEGIN DEFERRED")?;
    let result = current_view_in_transaction(db, conn, snapshot);
    match result {
        Ok(tracks) => {
            conn.execute_batch("COMMIT")?;
            Ok(tracks)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn current_view_in_transaction(
    db: &Db,
    conn: &Connection,
    snapshot: &DoctorViewSnapshot,
) -> Result<Vec<DoctorTrackRef>, rusqlite::Error> {
    let total = if snapshot.source == crate::view_source::ViewSource::Queue {
        i64::try_from(snapshot.queue_ids.len()).unwrap_or(i64::MAX)
    } else {
        crate::queries::query_track_count_browsed(
            db,
            &snapshot.source,
            &snapshot.filter,
            &snapshot.browse,
            &snapshot.queue_ids,
        )?
    };
    let mut offset = 0;
    let mut seen = HashSet::new();
    let mut tracks = Vec::with_capacity(usize::try_from(total).unwrap_or_default());
    while offset < total {
        let page = crate::queries::query_track_window_browsed(
            db,
            &snapshot.source,
            &snapshot.sort_field,
            &snapshot.sort_dir,
            &snapshot.filter,
            &snapshot.browse,
            offset,
            PAGE_SIZE,
            &snapshot.queue_ids,
        )?;
        offset = offset.saturating_add(PAGE_SIZE);
        for track in page {
            if !seen.insert(track.id) {
                continue;
            }
            if let Some(reference) = present_track_ref(conn, track.id)? {
                tracks.push(reference);
            }
        }
    }
    Ok(tracks)
}

fn selection(conn: &Connection, track_ids: &[i64]) -> Result<Vec<DoctorTrackRef>, rusqlite::Error> {
    let mut seen = HashSet::new();
    let mut tracks = Vec::new();
    for track_id in track_ids.iter().copied() {
        if !seen.insert(track_id) {
            continue;
        }
        let track = conn
            .query_row(
                &format!(
                    "SELECT id, path, file_mtime, file_size, device, inode \
                     FROM tracks WHERE id=?1 AND {}",
                    crate::queries::PRESENT
                ),
                [track_id],
                row_to_track_ref,
            )
            .optional()?;
        if let Some(track) = track {
            tracks.push(track);
        }
    }
    Ok(tracks)
}

fn row_to_track_ref(row: &rusqlite::Row<'_>) -> rusqlite::Result<DoctorTrackRef> {
    Ok(DoctorTrackRef {
        track_id: row.get(0)?,
        path: PathBuf::from(row.get::<_, String>(1)?),
        file_mtime: row.get(2)?,
        file_size: row.get(3)?,
        device: row.get(4)?,
        inode: row.get(5)?,
    })
}

fn present_track_ref(
    conn: &Connection,
    track_id: i64,
) -> Result<Option<DoctorTrackRef>, rusqlite::Error> {
    conn.query_row(
        &format!(
            "SELECT id, path, file_mtime, file_size, device, inode \
             FROM tracks WHERE id=?1 AND {}",
            crate::queries::PRESENT
        ),
        [track_id],
        row_to_track_ref,
    )
    .optional()
}
