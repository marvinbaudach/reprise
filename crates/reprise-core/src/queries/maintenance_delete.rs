//! Transactional track deletion shared by deliberate and automatic paths.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::library::playlists;
use crate::models::MissingReason;

use super::clauses::MISSING;
use super::maintenance::RemoveGuard;

#[derive(Clone, Copy)]
struct PathRequest<'a> {
    id: i64,
    expected_path: &'a Path,
}

#[derive(Clone, Copy)]
enum RemovalRequest<'a> {
    Path(PathRequest<'a>),
    Missing(i64),
    Tombstoned(i64),
    AutoClean {
        id: i64,
        grace_period_seconds: i64,
        now: i64,
    },
}

impl RemovalRequest<'_> {
    fn id(self) -> i64 {
        match self {
            Self::Path(request) => request.id,
            Self::Missing(id) | Self::Tombstoned(id) | Self::AutoClean { id, .. } => id,
        }
    }
}

pub(super) fn remove_path_requests_impl<'a>(
    conn: &Connection,
    requests: impl IntoIterator<Item = (i64, &'a Path)>,
    exclusion_time: Option<i64>,
    remember_deletion: bool,
) -> Result<Vec<i64>, rusqlite::Error> {
    let requests = requests
        .into_iter()
        .map(|(id, expected_path)| PathRequest { id, expected_path })
        .collect::<Vec<_>>();
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    // ADR-002 permits unchecked transaction construction from the shared
    // `&Connection`; every nested call here was audited not to open another.
    // Deliberate deletion starts with an immediate transaction so the
    // eligibility preflight, metadata memory, guarded deletes, compaction,
    // and catalog hide cannot race another writer between their steps.
    let tx = if remember_deletion {
        Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?
    } else {
        unchecked_transaction(conn)?
    };
    let requests = if remember_deletion {
        eligible_path_requests(&tx, requests)?
    } else {
        requests
    };
    if remember_deletion {
        let ids = requests
            .iter()
            .map(|request| request.id)
            .collect::<Vec<_>>();
        let now = tx.query_row("SELECT CAST(strftime('%s', 'now') AS INTEGER)", [], |row| {
            row.get(0)
        })?;
        crate::deleted_releases::remember_deleted_releases(&tx, &ids, now)?;
    }
    let removed = delete_requests(
        &tx,
        requests.into_iter().map(RemovalRequest::Path),
        exclusion_time,
    )?;
    if remember_deletion {
        crate::deleted_releases::hide_deleted_release_memory(&tx)?;
    }
    tx.commit()?;
    Ok(removed)
}

pub(super) fn remove_id_requests_impl(
    conn: &Connection,
    ids: &[i64],
    guard: RemoveGuard,
) -> Result<Vec<i64>, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let requests = ids.iter().copied().map(|id| match guard {
        RemoveGuard::MissingOnly => RemovalRequest::Missing(id),
        RemoveGuard::TombstonedOnly => RemovalRequest::Tombstoned(id),
        RemoveGuard::AutoCleanEligible {
            grace_period_seconds,
            now,
        } => RemovalRequest::AutoClean {
            id,
            grace_period_seconds,
            now,
        },
    });
    let tx = unchecked_transaction(conn)?;
    let removed = delete_requests(&tx, requests, None)?;
    tx.commit()?;
    Ok(removed)
}

fn unchecked_transaction(conn: &Connection) -> Result<Transaction<'_>, rusqlite::Error> {
    // `Db` exposes a shared `&Connection`, so ADR-002 deliberately uses
    // `unchecked_transaction()` here after auditing every nested call to
    // ensure none opens another transaction.
    conn.unchecked_transaction()
}

fn eligible_path_requests<'a>(
    tx: &Transaction<'_>,
    requests: Vec<PathRequest<'a>>,
) -> Result<Vec<PathRequest<'a>>, rusqlite::Error> {
    let mut statement = tx.prepare_cached("SELECT 1 FROM tracks WHERE id = ?1 AND path = ?2")?;
    requests
        .into_iter()
        .filter_map(|request| {
            match statement
                .query_row(
                    rusqlite::params![request.id, request.expected_path.to_string_lossy()],
                    |_| Ok(()),
                )
                .optional()
            {
                Ok(Some(())) => Some(Ok(request)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect()
}

fn delete_requests<'a>(
    tx: &Transaction<'_>,
    requests: impl IntoIterator<Item = RemovalRequest<'a>>,
    exclusion_time: Option<i64>,
) -> Result<Vec<i64>, rusqlite::Error> {
    let requests = requests.into_iter().collect::<Vec<_>>();
    let mut removed = Vec::with_capacity(requests.len());
    for request in requests {
        let id = request.id();
        if let (Some(excluded_at), RemovalRequest::Path(path_request)) = (exclusion_time, request) {
            if !crate::library::exclusions::record_track(
                tx,
                id,
                path_request.expected_path,
                excluded_at,
            )? {
                continue;
            }
        }
        let mut statement =
            tx.prepare("SELECT DISTINCT playlist_id FROM playlist_tracks WHERE track_id = ?1")?;
        let affected_playlists = statement
            .query_map([id], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);

        if delete_guarded_track(tx, request)? == 0 {
            continue;
        }
        removed.push(id);
        for playlist_id in affected_playlists {
            playlists::renumber_positions(tx, playlist_id)?;
        }
    }
    Ok(removed)
}

fn delete_guarded_track(
    tx: &Transaction<'_>,
    request: RemovalRequest<'_>,
) -> Result<usize, rusqlite::Error> {
    match request {
        RemovalRequest::Path(request) => tx.execute(
            "DELETE FROM tracks WHERE id = ?1 AND path = ?2",
            rusqlite::params![request.id, request.expected_path.to_string_lossy()],
        ),
        RemovalRequest::Missing(id) => tx.execute(
            &format!("DELETE FROM tracks WHERE id = ?1 AND {MISSING}"),
            [id],
        ),
        RemovalRequest::Tombstoned(id) => tx.execute(
            "DELETE FROM tracks WHERE id = ?1 AND removed_at IS NOT NULL",
            [id],
        ),
        RemovalRequest::AutoClean {
            id,
            grace_period_seconds,
            now,
        } => {
            let Some(armed_at) = crate::library::settings::get_auto_clean_armed_at_in(tx)? else {
                // Auto-clean was disarmed after the caller selected this row.
                // Its deadline is no longer authorized to remove anything.
                return Ok(0);
            };
            let deadline = super::issues::auto_clean_deadline_clause(3, 4, 5);
            tx.execute(
                &format!(
                    "DELETE FROM tracks WHERE id = ?1 AND {MISSING} \
                     AND missing_reason = ?2 AND {deadline}"
                ),
                rusqlite::params![
                    id,
                    MissingReason::Deleted.as_str(),
                    armed_at,
                    grace_period_seconds,
                    now
                ],
            )
        }
    }
}
