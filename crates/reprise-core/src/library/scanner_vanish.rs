//! Task 1.5's vanish-mark phase: the candidate query, the root-guard
//! evidence check, and the mark loop `scan_folder_inner` folds in after its
//! walk. Split into its own file purely to keep `scanner.rs` itself under
//! the project's 800-line rule — `scanner.rs` declares this via `#[path =
//! "scanner_vanish.rs"] mod vanish;`, so this is still the crate-private
//! `crate::library::scanner::vanish` module, not a rewrite of the logic. See
//! `scan_folder_inner`'s doc comment in `scanner.rs` for the full fold and
//! root-guard rationale these functions implement.

use super::*;
use crate::library::source::{LibraryLinkMode, LibraryPathPresence, LibrarySource};

/// Shared row-fetch behind both [`present_candidates_under_root`] (the mark
/// phase) and [`guard_evidence_under_root`] (the root guard's evidence
/// check): every row under `root` matching `presence_clause`, paired with
/// its recorded `device`. Membership is decided in two stages, same as the
/// pre-fold `mark_vanished_under_root` this replaces:
///
/// A SQL `LIKE '<root>/%'` (metacharacters escaped) prefilter narrows the
/// candidate rows read out of the database, instead of streaming every
/// matching track through Rust on every scan. Its `/` before `%` means it
/// can never match a *sibling* root sharing a string prefix (`/music` vs
/// `/music2`).
///
/// `Path::starts_with` then remains the sole AUTHORITATIVE membership check,
/// applied to every row the prefilter returns: it compares path
/// *components*, not raw bytes, so a track at `/music/foobar/x.flac` does
/// NOT count as being under `/music/foo` — which a naive string/`LIKE
/// 'foo%'` prefix check would incorrectly include. This is also what keeps
/// this function from ever touching a track outside `root` — the guarantee
/// a future multi-folder library depends on — even when that other track's
/// file has also vanished from disk; only a scan of *that* track's own root
/// is ever responsible for it. The LIKE prefilter is deliberately only a
/// *superset* filter: it never decides membership on its own, so the result
/// is byte-identical to a hypothetical full-table-scan implementation
/// regardless of LIKE's ASCII case-insensitivity or any other way the
/// pattern is wider than the component check.
fn candidates_under_root(
    tx: &rusqlite::Transaction,
    root: &Path,
    presence_clause: &str,
) -> Result<Vec<(i64, String, Option<i64>)>, ScanError> {
    let root_str = root.to_string_lossy();
    let pattern = format!(
        "{}/%",
        crate::library::playlists::escape_like(root_str.trim_end_matches('/'))
    );
    let rows: Vec<(i64, String, Option<i64>)> = {
        let mut stmt = tx.prepare(&format!(
            "SELECT id, path, device FROM tracks WHERE {presence_clause} AND path LIKE ?1 ESCAPE '\\'"
        ))?;
        let mapped = stmt
            .query_map(rusqlite::params![pattern], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<Result<_, _>>()?;
        mapped
    };
    Ok(rows
        .into_iter()
        .filter(|(_, path, _)| Path::new(path).starts_with(root))
        .collect())
}

/// Candidate rows the mark phase must consider: every currently-present
/// (`PRESENT`) track whose recorded `path` is under `root`, paired with its
/// recorded `device` (the residence token
/// [`LibrarySource::reachability`] compares against). `PRESENT` is the
/// correct — and only correct — filter here: the mark phase only ever wants
/// to *newly* flag a row that isn't already flagged, exactly like the
/// pre-fold `mark_vanished_under_root` this replaces. See
/// [`guard_evidence_under_root`]'s doc comment for why the root guard's own
/// evidence query deliberately does NOT reuse this narrower list.
pub(super) fn present_candidates_under_root(
    tx: &rusqlite::Transaction,
    root: &Path,
) -> Result<Vec<(i64, String, Option<i64>)>, ScanError> {
    candidates_under_root(tx, root, PRESENT)
}

/// The root guard's own candidate list: every NOT-YET-TOMBSTONED (`removed_at
/// IS NULL`) track under `root`, present or already-missing alike — the
/// union of `PRESENT` and `queries::MISSING`, deliberately wider than
/// [`present_candidates_under_root`]'s `PRESENT`-only list.
///
/// This must stay wider than the mark phase's own candidate list: the guard
/// asks "does the database have evidence `root`'s filesystem is the one it
/// remembers?", and a row already flagged missing by an earlier reconcile is
/// still exactly that evidence — its recorded `device` doesn't stop meaning
/// anything just because `missing_since` got set. Reusing the `PRESENT`-only
/// list here would silently drop that evidence: a root whose tracks are ALL
/// already flagged missing, and whose mount point then gets a *different*
/// filesystem swapped underneath it, would see an empty candidate list, the
/// guard's `!candidates.is_empty()` check would be false, and the guard
/// would never trip — `scan_folder` would report `Completed` with
/// `vanished == 0` instead of the `RootUnavailable` the situation actually
/// calls for, silently telling the watcher/GUI "your library is just empty"
/// when the truth is "your library folder isn't reachable". A tombstoned row
/// (`removed_at` set) is excluded even from this wider list — it's been
/// explicitly removed from the library and carries no evidence about
/// anything anymore.
pub(super) fn guard_evidence_under_root(
    tx: &rusqlite::Transaction,
    root: &Path,
) -> Result<Vec<(i64, String, Option<i64>)>, ScanError> {
    candidates_under_root(tx, root, "removed_at IS NULL")
}

/// Already-missing rows whose stored reason is not yet the proven `deleted`
/// state. The scan walk has already healed any file that reappeared, so this
/// list contains only unresolved state that is safe to re-probe after the root
/// guard confirms the scan location itself is reachable.
fn reclassification_candidates_under_root(
    tx: &rusqlite::Transaction,
    root: &Path,
) -> Result<Vec<(i64, String, Option<i64>)>, ScanError> {
    candidates_under_root(
        tx,
        root,
        "missing_since IS NOT NULL AND removed_at IS NULL AND \
         (missing_reason IS NULL OR missing_reason <> 'deleted')",
    )
}

/// The root guard's evidence check: does ANY guard-evidence candidate's
/// recorded residence token match the token `root` currently resolves to?
/// Always called with [`guard_evidence_under_root`]'s wider list, never
/// [`present_candidates_under_root`]'s — see that function's doc comment for
/// why. See `scan_folder_inner`'s `## Root guard` doc section for why a
/// single match is enough to treat `root` as provably reachable. A `NULL`
/// (`None`) recorded device never counts as a match, even in the
/// (should-not-happen, since the caller already probed `root`)
/// case where `root`'s own device can't be resolved either — two unknowns
/// are never evidence of each other.
pub(super) fn any_candidate_confirms_root_with(
    source: &dyn LibrarySource,
    candidates: &[(i64, String, Option<i64>)],
    root: &Path,
) -> bool {
    let root_token = source.residence_token(root);
    candidates
        .iter()
        .any(|(_, _, stored)| matches!((stored, root_token), (Some(stored), Some(current)) if *stored == current))
}

/// The mark phase itself: for every `candidates` row whose file no longer
/// exists at its source, sets `missing_since`/`missing_reason` (via
/// [`LibrarySource::reachability`]) and returns the count newly marked. A row
/// that's still
/// present (e.g. the walk's own move-detection just relocated a different
/// row onto this path, or the file genuinely never left) is left untouched
/// — this is the same per-row presence check `mark_vanished_under_root` used
/// before the fold, just running inside the walk's own `tx` now instead of a
/// separate connection/transaction afterward. Paths already delivered by
/// the current walk are known present and are skipped without another source
/// query; only unseen candidates need a probe.
pub(super) fn mark_vanished_with(
    source: &dyn LibrarySource,
    tx: &rusqlite::Transaction,
    candidates: Vec<(i64, String, Option<i64>)>,
    observed_paths: &std::collections::HashSet<std::path::PathBuf>,
) -> Result<u32, ScanError> {
    let mut marked = 0u32;
    for (id, path_str, device) in candidates {
        let path = Path::new(&path_str);
        if observed_paths.contains(path) {
            continue;
        }
        // This write needs confirmed absence. Present and Unknown both keep
        // the row live; inability to reach a source is not a missing verdict.
        if source.probe(path, LibraryLinkMode::Follow) != LibraryPathPresence::Absent {
            continue;
        }
        let reason = source.reachability(path, device);
        // `mount_point` is only ever read back for `unmounted` rows (see
        // `queries::issues`' `query_unavailable_groups`, which binds the
        // reason). Resolving it costs `mounts::mount_point_of` its own
        // ancestor walk, on top of the one `reachability` just did, so it is
        // resolved for the one reason that consumes it and left as-is
        // otherwise — a `deleted` row keeps whatever the last successful
        // scan recorded, which no query looks at.
        if reason == MissingReason::Unmounted {
            let mount_point = source
                .mount_point(path)
                .map(|mount| mount.to_string_lossy().into_owned());
            tx.execute(
                "UPDATE tracks SET missing_since = ?2, missing_reason = ?3, mount_point = ?4 \
                 WHERE id = ?1",
                rusqlite::params![id, now_unix(), reason.as_str(), mount_point],
            )?;
            tracing::info!(
                path = %path_str,
                reason = reason.as_str(),
                mount_point = ?mount_point,
                "scan: marked vanished track missing (mount currently absent)"
            );
        } else {
            tx.execute(
                "UPDATE tracks SET missing_since = ?2, missing_reason = ?3 WHERE id = ?1",
                rusqlite::params![id, now_unix(), reason.as_str()],
            )?;
            tracing::info!(
                path = %path_str,
                reason = reason.as_str(),
                "scan: marked vanished track missing"
            );
        }
        marked += 1;
    }
    Ok(marked)
}

/// Corrects stale `unmounted`/`unknown` reasons only when current source
/// evidence positively resolves the still-absent item as [`MissingReason::Deleted`].
/// `missing_since` is deliberately left untouched because it is the user-facing
/// first-absence time. If auto-clean was already armed, its global lower-bound
/// clock is advanced to `now`, giving every corrected row the full configured
/// grace period without changing that display timestamp.
///
/// That advance is what makes this function a *frequent* writer of
/// `auto_clean_armed_at`, where before it moved only on a rare user action.
/// `maintenance::remove_auto_clean_eligible_tracks` re-checks the deadline at
/// delete time for exactly that reason — see its guard.
pub(super) fn reclassify_missing_with(
    source: &dyn LibrarySource,
    tx: &rusqlite::Transaction,
    root: &Path,
    now: i64,
) -> Result<u32, ScanError> {
    let candidates = reclassification_candidates_under_root(tx, root)?;
    let mut corrected = 0u32;
    for (id, path_str, device) in candidates {
        let path = Path::new(&path_str);
        if source.probe(path, LibraryLinkMode::Follow) != LibraryPathPresence::Absent {
            continue;
        }
        if source.reachability(path, device) != MissingReason::Deleted {
            continue;
        }
        // No `mount_point` write here: this path only ever lands on
        // `deleted`, and that column is read back exclusively for
        // `unmounted` rows. Resolving it would buy a second ancestor walk
        // per corrected row for a value nothing queries.
        let changed = tx.execute(
            "UPDATE tracks SET missing_reason = 'deleted' \
             WHERE id = ?1 AND missing_since IS NOT NULL AND removed_at IS NULL AND \
             (missing_reason IS NULL OR missing_reason <> 'deleted')",
            rusqlite::params![id],
        )?;
        corrected = corrected.saturating_add(changed as u32);
    }

    if corrected > 0 {
        rearm_auto_clean_if_armed(tx, now)?;
    }
    Ok(corrected)
}

fn rearm_auto_clean_if_armed(tx: &rusqlite::Transaction, now: i64) -> Result<(), rusqlite::Error> {
    // Goes through `settings`' own accessors rather than re-deriving "read
    // the key, parse it as i64" from the raw primitives: the parse fallback
    // is a decision (a corrupt value means "inert", not "armed at 0"), and
    // it belongs in one place. `&Transaction` derefs to `&Connection`, so
    // the `_in` variants take this transaction directly.
    let Some(armed_at) = crate::library::settings::get_auto_clean_armed_at_in(tx)? else {
        return Ok(());
    };
    crate::library::settings::set_auto_clean_armed_at_in(tx, armed_at.max(now))
}

#[cfg(test)]
mod android_uri_tests {
    use super::*;
    use crate::library::source_test_support::UnknownProbeSource;

    const TREE_URI: &str = "content://com.android.externalstorage.documents/tree/primary%3AMusic";
    const ESCAPED_TREE_URI: &str =
        "content://com.android.externalstorage.documents/tree/primary\\%3AMusic";
    const TRACK_URI: &str = "content://com.android.externalstorage.documents/tree/primary%3AMusic/document/primary%3AMusic%2Fsong.flac";

    #[test]
    fn android_a2_content_uri_path_operations_preserve_tree_membership_and_extension() {
        let tree = std::path::PathBuf::from(TREE_URI);
        let track = std::path::PathBuf::from(TRACK_URI);

        assert!(track.starts_with(&tree));
        assert_eq!(
            track.extension().and_then(std::ffi::OsStr::to_str),
            Some("flac")
        );
    }

    #[test]
    fn android_a3_content_uri_root_survives_vanish_like_prefilter() {
        assert_eq!(
            crate::library::playlists::escape_like(TREE_URI),
            ESCAPED_TREE_URI
        );

        let mut conn = crate::db::open_migrated(None).unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, added_at, device) VALUES (?1, ?2, 0, ?3)",
            rusqlite::params![1_i64, TRACK_URI, 7_i64],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, added_at, device) VALUES (?1, ?2, 0, ?3)",
            rusqlite::params![
                2_i64,
                "content://com.android.externalstorage.documents/tree/primaryX3AMusic/document/primaryX3AMusic%2Fother.flac",
                9_i64,
            ],
        )
        .unwrap();
        let tx = conn.transaction().unwrap();

        let candidates =
            candidates_under_root(&tx, Path::new(TREE_URI), "removed_at IS NULL").unwrap();

        assert_eq!(candidates, vec![(1_i64, TRACK_URI.to_owned(), Some(7_i64))]);
    }

    #[test]
    fn unknown_probe_does_not_mark_vanished_track_missing() {
        let mut conn = crate::db::open_migrated(None).unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, added_at, device) VALUES (1, ?1, 0, 41)",
            [TRACK_URI],
        )
        .unwrap();
        let tx = conn.transaction().unwrap();

        let marked = mark_vanished_with(
            &UnknownProbeSource,
            &tx,
            vec![(1, TRACK_URI.to_owned(), Some(41))],
            &std::collections::HashSet::new(),
        )
        .unwrap();
        let missing: (Option<i64>, Option<String>) = tx
            .query_row(
                "SELECT missing_since, missing_reason FROM tracks WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(marked, 0);
        assert_eq!(missing, (None, None));
    }
}
