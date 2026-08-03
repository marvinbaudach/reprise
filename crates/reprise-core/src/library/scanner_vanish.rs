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
    resolve_native_mount_points: bool,
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
        tx.execute(
            "UPDATE tracks SET missing_since = ?2, missing_reason = ?3 WHERE id = ?1",
            rusqlite::params![id, now_unix(), reason.as_str()],
        )?;
        marked += 1;
        if reason == MissingReason::Unmounted {
            // Diagnostic only: `mount_point_of` exists to group "what
            // disappears together when a mount goes away" (see `mounts`'
            // own module doc) — a later task's status card is the real
            // consumer; for now this just makes that grouping visible in
            // the log for an `Unmounted` row, where it's actually
            // informative (a `Deleted` row has no mount to report).
            let mount_point = resolve_native_mount_points
                .then(|| mounts::mount_point_of(path))
                .flatten();
            tracing::info!(
                path = %path_str,
                reason = reason.as_str(),
                mount_point = ?mount_point,
                "scan: marked vanished track missing (mount currently absent)"
            );
        } else {
            tracing::info!(
                path = %path_str,
                reason = reason.as_str(),
                "scan: marked vanished track missing"
            );
        }
    }
    Ok(marked)
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
            true,
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
