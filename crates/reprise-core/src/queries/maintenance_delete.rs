//! Transactional track deletion shared by deliberate and automatic paths.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::library::playlists;
use crate::models::MissingReason;

use super::clauses::MISSING;
use super::maintenance::RemoveGuard;

pub(super) fn remove_track_requests_impl<'a>(
    conn: &Connection,
    requests: impl IntoIterator<Item = (i64, Option<&'a Path>)>,
    guard: RemoveGuard,
    exclusion_time: Option<i64>,
    remember_deletion: bool,
) -> Result<Vec<i64>, rusqlite::Error> {
    let requests = requests.into_iter().collect::<Vec<_>>();
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    // Deliberate deletion starts with an immediate transaction so the
    // eligibility preflight, metadata memory, guarded deletes, compaction,
    // and catalog hide cannot race another writer between their steps.
    let tx = if remember_deletion {
        Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?
    } else {
        conn.unchecked_transaction()?
    };
    let requests = eligible_memory_requests(&tx, requests, guard, remember_deletion)?;
    if remember_deletion {
        let ids = requests.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        let now = tx.query_row("SELECT unixepoch()", [], |row| row.get(0))?;
        crate::deleted_releases::remember_deleted_releases(&tx, &ids, now)?;
    }

    let mut removed = Vec::with_capacity(requests.len());
    for (id, expected_path) in requests {
        if let (Some(excluded_at), Some(expected_path)) = (exclusion_time, expected_path) {
            if !crate::library::exclusions::record_track(&tx, id, expected_path, excluded_at)? {
                continue;
            }
        }
        let mut stmt =
            tx.prepare("SELECT DISTINCT playlist_id FROM playlist_tracks WHERE track_id = ?1")?;
        let affected_playlists: Vec<i64> = stmt
            .query_map(rusqlite::params![id], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        drop(stmt);

        let deleted = delete_guarded_track(&tx, id, expected_path, guard)?;
        if deleted == 0 {
            continue;
        }
        removed.push(id);
        for playlist_id in affected_playlists {
            playlists::renumber_positions(&tx, playlist_id)?;
        }
    }
    if remember_deletion {
        crate::deleted_releases::apply_deleted_release_memory(&tx)?;
    }
    tx.commit()?;
    Ok(removed)
}

fn eligible_memory_requests<'a>(
    tx: &Transaction<'_>,
    requests: Vec<(i64, Option<&'a Path>)>,
    guard: RemoveGuard,
    remember_deletion: bool,
) -> Result<Vec<(i64, Option<&'a Path>)>, rusqlite::Error> {
    if !remember_deletion {
        return Ok(requests);
    }
    requests
        .into_iter()
        .filter_map(|request @ (id, expected_path)| {
            let exists = match (guard, expected_path) {
                (RemoveGuard::Any, Some(path)) => tx
                    .query_row(
                        "SELECT 1 FROM tracks WHERE id = ?1 AND path = ?2",
                        rusqlite::params![id, path.to_string_lossy()],
                        |_| Ok(()),
                    )
                    .optional(),
                (RemoveGuard::TombstonedOnly, None) => tx
                    .query_row(
                        "SELECT 1 FROM tracks WHERE id = ?1 AND removed_at IS NOT NULL",
                        [id],
                        |_| Ok(()),
                    )
                    .optional(),
                _ => unreachable!("only deliberate deletion paths record release memory"),
            };
            match exists {
                Ok(Some(())) => Some(Ok(request)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect()
}

fn delete_guarded_track(
    tx: &Transaction<'_>,
    id: i64,
    expected_path: Option<&Path>,
    guard: RemoveGuard,
) -> Result<usize, rusqlite::Error> {
    match (guard, expected_path) {
        (RemoveGuard::Any, Some(expected_path)) => tx.execute(
            "DELETE FROM tracks WHERE id = ?1 AND path = ?2",
            rusqlite::params![id, expected_path.to_string_lossy()],
        ),
        (RemoveGuard::Any, None) => {
            unreachable!("RemoveGuard::Any is only ever paired with a path-identity check")
        }
        (RemoveGuard::MissingOnly, None) => tx.execute(
            &format!("DELETE FROM tracks WHERE id = ?1 AND {MISSING}"),
            [id],
        ),
        (RemoveGuard::TombstonedOnly, None) => tx.execute(
            "DELETE FROM tracks WHERE id = ?1 AND removed_at IS NOT NULL",
            [id],
        ),
        (
            RemoveGuard::AutoCleanEligible {
                grace_period_seconds,
                now,
            },
            None,
        ) => {
            let Some(armed_at) = crate::library::settings::get_auto_clean_armed_at_in(tx)? else {
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
        (_, Some(_)) => unreachable!("path identity is only valid with RemoveGuard::Any"),
    }
}
