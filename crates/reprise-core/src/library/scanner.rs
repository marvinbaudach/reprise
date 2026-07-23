use rusqlite::Connection;
use std::path::Path;

use super::import_errors;
use super::mounts;
#[cfg(test)]
use crate::models::ImportErrorKind;
use crate::models::MissingReason;
use crate::queries::PRESENT;

#[path = "scanner_types.rs"]
mod scanner_types;
pub use scanner_types::{
    finalize_completed_scan, ScanError, ScanOutcome, ScanProgress, ScanReport, ScanResult,
};

const AUDIO_EXTENSIONS: [&str; 7] = ["mp3", "flac", "ogg", "opus", "m4a", "aac", "wav"];

pub(crate) fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| AUDIO_EXTENSIONS.contains(&extension.as_str()))
}

pub(crate) fn file_mtime(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

/// `(file_size, device, inode)` for the move-detection fingerprint. The app
/// runs on Linux and uses the Unix `(device, inode)` identity
/// (`std::os::unix::fs::MetadataExt`); the non-Unix arm exists only to keep
/// `reprise-core` cross-checkable (spec I / cross-target CI). There is no
/// stable portable device/inode (`std::os::windows::fs::MetadataExt`'s
/// equivalents are still behind the unstable `windows_by_handle` feature), so
/// off Unix identity degrades to `(0, 0)` — never reached at runtime. Returns
/// `None` if `stat` fails (e.g. a race where the file vanished between
/// `walkdir` listing it and this call) — Stage 3 Task 1: a file that can't be
/// stat'd has no reliable filesystem identity to fingerprint, so `scan_folder`
/// skips the move-detection step entirely for it (rather than the pre-Task-1
/// behavior of silently fingerprinting on placeholder zeros, which could have
/// coincidentally matched an unrelated `(device, inode)` of `(0, 0)`) and
/// stores `NULL` device/inode for the row, same as any pre-Stage-2 row that
/// predates these columns. `file_size` still defaults to `0` in that case —
/// unlike device/inode it is `NOT NULL DEFAULT 0` in the schema, matching every
/// other tag-derived column's non-null convention, so `0` (rather than `NULL`)
/// is the only representable "unknown" value for it anyway.
pub(crate) fn file_stat(path: &Path) -> Option<(u64, u64, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some((metadata.size(), metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        Some((metadata.len(), 0, 0))
    }
}

/// Return type for tag_param_values: (title, artist, album, album_artist,
/// artist_mbid, year, track_no, disc_no, genre, duration_ms, bitrate_kbps,
/// untagged).
type TagParams<'a> = (
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    Option<&'a str>,
    Option<i32>,
    Option<i32>,
    Option<i32>,
    &'a str,
    i64,
    Option<i32>,
    i64,
);

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Extracts tag-derived column values in the canonical order used by both
/// move-UPDATE and INSERT/upsert statements: title, artist, album, album_artist,
/// artist_mbid, year, track_no, disc_no, genre, duration_ms, bitrate_kbps,
/// untagged. Having a single
/// source for this ordering ensures that adding/removing columns is a
/// one-place change. `untagged` (Task 1.8) is threaded through here rather
/// than left for each call site to append separately, same reasoning as
/// every other column in this tuple.
fn tag_param_values<'a>(
    title: &'a str,
    meta: &'a track_meta::TrackMeta,
    untagged: bool,
) -> TagParams<'a> {
    (
        title,
        &meta.artist,
        &meta.album,
        &meta.album_artist,
        meta.artist_mbid.as_deref(),
        meta.year,
        meta.track_no,
        meta.disc_no,
        &meta.genre,
        meta.duration_ms,
        meta.bitrate_kbps,
        i64::from(untagged),
    )
}

pub fn scan_folder(conn: &mut Connection, root: &Path) -> Result<ScanOutcome, ScanError> {
    scan_folder_inner(conn, root, None)
}

pub fn scan_folder_with_progress(
    conn: &mut Connection,
    root: &Path,
    mut on_progress: impl FnMut(ScanProgress),
) -> Result<ScanOutcome, ScanError> {
    on_progress(ScanProgress::Discovering);
    let total = scan_progress::count_audio_files(root);
    let reporter = scan_progress::ScanProgressReporter::new(&mut on_progress, total);
    scan_folder_inner(conn, root, Some(reporter))
}

/// Walks `root`, upserting every audio file found, then — in the SAME
/// transaction — reconciles whatever the walk did NOT find: rows the DB
/// still believes are present under `root` whose file has actually vanished.
///
/// ## Fold: scan IS reconcile, not scan-then-mark
///
/// Through Stage 3, this was two separate calls: `scan_folder` (which
/// committed its own transaction), then the folder watcher separately called
/// `mark_vanished_under_root` — and that function's own doc comment spent
/// three paragraphs establishing a rule every caller had to remember: mark-
/// vanished must run strictly AFTER scan_folder, never before, because a
/// file moved/renamed within `root` is only reconciled by move detection
/// (which updates its row's `path` in place) during the walk — running
/// mark-vanished first would transiently and wrongly flag a moved-but-not-
/// yet-rescanned file as missing. A convention that needs three paragraphs
/// of documentation and that every call site must get in the right order
/// belongs in the structure, not in a comment: Task 1.5 folds the mark phase
/// into this function, after the walk loop, inside the walk's own `tx` —
/// there is now nothing left to call in the wrong order, and a move and an
/// unrelated deletion discovered in the same pass reconcile as one atomic
/// transaction instead of leaving a window (between the old two commits)
/// where the database briefly says a moved file is gone.
///
/// ## Root guard: no evidence about `root` must never look like "all gone"
///
/// A scan whose own `root` cannot be seen has no evidence about any
/// individual file under it — it only knows "my root is unreachable". Before
/// the walk even starts, `!root.exists()` short-circuits straight to
/// [`ScanOutcome::RootUnavailable`] with no walk and no database write at
/// all (`import_errors` included) — see Root-Guard case (a) in this
/// function's test suite.
///
/// A subtler case remains even when `root` itself resolves to *some*
/// directory: a removable/network mount that hasn't come up yet often still
/// has an empty directory sitting at its mount point (owned by whatever
/// filesystem is underneath, typically the root filesystem) — walking it
/// finds zero audio files, indistinguishable at a glance from a genuinely
/// emptied folder. Marking every track under it "unmounted" would still make
/// the whole library look empty in the UI the moment that scan lands (see
/// this module's `library::mounts::classify_missing`'s own doc comment for
/// why `Unmounted` vs `Deleted` matters — this guard is about whether ANY
/// marking should happen at all, not which reason to use once it does). So,
/// only once the walk found zero audio files AND at least one NOT-YET-
/// TOMBSTONED track (`removed_at IS NULL` — present or already-missing
/// alike, via `scanner_vanish::guard_evidence_under_root`) is recorded under
/// `root`, this function asks one more question before marking anything:
/// does ANY of those tracks' recorded `device` match the device `root`
/// itself currently resolves to? A `NULL` (`None`) recorded device never
/// counts as a match. If yes, at least one track proves `root`'s filesystem
/// really is the one previously scanned — proceed to mark normally
/// (Root-Guard case (c): a real, provable deletion). If no such evidence
/// exists, mark nothing and return [`ScanOutcome::RootUnavailable`] instead
/// (Root-Guard case (b)) — the transaction still commits whatever the
/// (empty) walk itself produced, but the mark phase never runs.
///
/// This evidence set is deliberately wider than the mark phase's own
/// `PRESENT`-only candidate list (`scanner_vanish::present_candidates_under_
/// root`): a row an earlier reconcile already flagged missing still carries
/// a recorded `device` that means exactly as much as a present row's does.
/// If the guard's evidence were narrowed to `PRESENT`, a root whose tracks
/// are ALL already flagged missing — and whose mount point then gets a
/// different filesystem swapped underneath it — would look like it has no
/// evidence at all (empty candidate list) and would never trip the guard,
/// silently reporting `Completed`/`vanished == 0` instead of surfacing
/// `RootUnavailable` — exactly the "empty library" lie this guard exists to
/// prevent. A tombstoned row (`removed_at` set) is still excluded even from
/// this wider set: it's been explicitly removed from the library and no
/// longer carries evidence about anything.
///
/// This guard is deliberately root-only: it decides whether to run the mark
/// phase over `root` at all, never which individual rows within it get
/// marked. If `root` itself is confirmed reachable but some *subfolder*
/// under it sits on its own, now-absent mount, that subtree's tracks still
/// get marked — normally, each via its own `classify_missing` call — because
/// that is an honest partial outage, not "we have no evidence".
///
/// ## Hint coexistence: a `tracks` row and an `import_errors` row can now
/// both exist for the same path
///
/// Before Task 1.8, a `tracks` row and an `import_errors` row for the same
/// `path` were mutually exclusive — any successful import cleared the error
/// row. `track_meta::read_meta_with_fallback`'s pass-2 rescue breaks that: a
/// file with unreadable tags but an intact container now gets BOTH a
/// `tracks` row (`untagged = 1`, so the collection has no hole for it) AND
/// an `import_errors` row, which becomes a HINT ("imported without
/// metadata") rather than a failure — see `import_errors.rs`'s module doc
/// comment for the exact hint contract a later query layer/sidebar badge
/// must use.
///
/// Concretely, in the walk loop below: a pass-1 success still calls
/// `import_errors::clear_error` (unchanged — the self-healing rule
/// sharpens, it doesn't change, for that case); a pass-2 (untagged) success
/// calls `import_errors::record_error` with pass 1's own `(kind, detail)`
/// instead — refreshing the hint's `last_seen`/`seen_count` rather than
/// deleting it. Only a later scan that achieves a real pass-1 success (the
/// file got re-tagged) clears it.
fn scan_folder_inner(
    conn: &mut Connection,
    root: &Path,
    mut progress: Option<scan_progress::ScanProgressReporter<'_>>,
) -> Result<ScanOutcome, ScanError> {
    debug_assert!(
        root.is_absolute(),
        "scan roots must be absolute paths — library roots (GTK folder chooser, \
         persisted settings) are always absolute in this codebase, and mounts::\
         nearest_existing_ancestor_dev's walk-to-`/` guarantee assumes it"
    );
    if !root.exists() {
        // Root-Guard case (a): no walk, no database write at all — see this
        // function's `## Root guard` doc section.
        tracing::warn!(
            root = %root.display(),
            "scan: root does not exist; reporting RootUnavailable without touching the database"
        );
        return Ok(ScanOutcome::RootUnavailable {
            root: root.to_path_buf(),
        });
    }

    let mut report = ScanReport::default();
    let mut audio_files_seen: u64 = 0;
    let mut mount_cache = mount::MountPointCache::new();
    let tx = conn.transaction()?;
    for entry in walkdir::WalkDir::new(root).follow_links(false).into_iter() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                // `err.path()` is the DIRECTORY walkdir failed to enter,
                // never a file underneath it — those were never seen, so
                // inventing rows for them would be fiction.
                let err_path = err.path().map_or_else(
                    || root.to_string_lossy().to_string(),
                    |p| p.to_string_lossy().to_string(),
                );
                import_errors::record_error(
                    &tx,
                    &err_path,
                    import_errors::classify_walkdir(&err),
                    &format!("directory traversal error: {err}"),
                    now_unix(),
                )?;
                report.errors += 1;
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !is_audio_file(path) {
            continue;
        }
        // Root-Guard input: "did the walk find any audio file at all under
        // `root`?" — counted regardless of whether this particular file
        // goes on to be added/updated/skipped/errored below. See this
        // function's `## Root guard` doc section.
        audio_files_seen += 1;
        let path_str = path.to_string_lossy().to_string();
        let mtime = file_mtime(path);
        // Compute identity before touching tags. An exclusion follows the
        // same file across a rename and must win over move detection.
        let stat = file_stat(path);
        let (file_size, device, inode): (i64, Option<i64>, Option<i64>) = match stat {
            Some((size, dev, ino)) => (size as i64, Some(dev as i64), Some(ino as i64)),
            None => (0, None, None),
        };
        if super::exclusions::matches_file(&tx, path, device, inode)? {
            report.excluded += 1;
            if let Some(progress) = &mut progress {
                progress.advance(path);
            }
            continue;
        }
        let known: Option<(i64, Option<i64>, Option<i64>, i64)> = tx
            .query_row(
                "SELECT file_mtime, missing_since, removed_at, untagged FROM tracks WHERE path = ?1",
                [&path_str],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .ok();
        let known_mtime = known.map(|(file_mtime, ..)| file_mtime);
        let known_missing = known.is_some_and(|(_, missing_since, ..)| missing_since.is_some());
        // Task 1.9: a row can be tombstoned (`removed_at` set, via a future
        // "Remove from library") independently of ever having been marked
        // missing — evidence that the file is still sitting at its exact
        // recorded path outranks that removal (evidence rule, Beschluss
        // 7/12), so this reappearance check must fire for a tombstoned row
        // too, not only a missing one.
        let known_removed = known.is_some_and(|(_, _, removed_at, _)| removed_at.is_some());
        // A present row still flagged `untagged` (an earlier scan couldn't parse
        // its container) must NOT take the unchanged-mtime fast path: excluding
        // it here drops it through to re-read + `repair_damaged_tags`, so a
        // library imported before auto-repair existed stops staying untagged.
        let known_untagged = known.is_some_and(|(_, _, _, untagged)| untagged != 0);
        if known_mtime == Some(mtime) && !known_untagged {
            if known_missing || known_removed {
                // The file reappeared at its exact recorded path with an
                // unchanged mtime (NAS remount, restore-from-trash, or a
                // tombstoned row whose object turned out to still be
                // there): the ordinary incremental fast path would
                // otherwise skip it forever, silently ignoring `missing_
                // since`/`removed_at` — this is the one case the fast path
                // must NOT take, since the row still needs both cleared
                // even though nothing else changed. This is also the ONLY
                // chance a row whose `mount_point` is NULL (a pre-schema-v10
                // row, or any row that was never re-scanned since) has to
                // acquire one without its file actually changing — see
                // `scanner_mount.rs`'s module doc comment.
                let mount_point = mount_cache.resolve(path);
                tx.execute(
                    "UPDATE tracks SET missing_since = NULL, missing_reason = NULL, \
                     removed_at = NULL, mount_point = ?2 WHERE path = ?1",
                    rusqlite::params![path_str, mount_point],
                )?;
                if import_errors::clear_error(&tx, &path_str)? {
                    report.healed += 1;
                }
                report.updated += 1;
                tracing::info!(
                    path = %path_str,
                    was_missing = known_missing,
                    was_removed = known_removed,
                    "restored track from evidence (unchanged mtime)"
                );
            } else {
                report.skipped_unchanged += 1;
            }
            if let Some(progress) = &mut progress {
                progress.advance(path);
            }
            continue;
        }
        // Dismiss-skip fast path: a `stat`, not a tag parse. Must run BEFORE
        // `read_meta` — see `check_dismissed`'s doc comment. An `untagged` row
        // is exempt: a dismissal only silences the notification and predates
        // auto-repair, so skipping here would strand a now-repairable file
        // forever (its mtime never changes, so it is never re-read).
        if !known_untagged
            && import_errors::check_dismissed(&tx, &path_str, mtime, file_size, now_unix())?
        {
            if let Some(progress) = &mut progress {
                progress.advance(path);
            }
            continue;
        }
        match track_meta::read_meta_with_fallback(path) {
            Ok(outcome) => {
                // Task 1.8: `hint` is `Some((kind, detail))` only when pass 1
                // failed but pass 2 rescued the container — see this
                // function's `## Hint coexistence` doc section just below.
                let (meta, hint) = match outcome {
                    track_meta::MetaOutcome::Tagged(meta) => (meta, None),
                    // A file the strict reader couldn't parse is repaired in
                    // place (damaged containers stripped, fresh ID3v2 written
                    // from the file name / folder), then re-read as a normal
                    // tagged import. On any repair failure it stays untagged.
                    track_meta::MetaOutcome::Untagged { meta, kind, detail } => {
                        match repair::repair_damaged_tags(path, &meta, kind) {
                            Some(repaired) => (repaired, None),
                            None => (meta, Some((kind, detail))),
                        }
                    }
                };
                let untagged = hint.is_some();
                let is_update = known_mtime.is_some();
                let title = if meta.title.is_empty() {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string()
                } else {
                    meta.title.clone()
                };
                // Task 1.6: recorded now, while still reachable, and
                // memoized per parent dir — see `scanner_mount.rs`.
                let mount_point = mount_cache.resolve(path);
                // A pass-1 success clears any previous failure for this path
                // (a file that errored once and is now readable again must
                // not stay in the error log). A pass-2 (untagged) success
                // must NOT clear it — instead it refreshes the row with
                // pass 1's diagnosis, keeping it alive as a HINT. See this
                // function's `## Hint coexistence` doc section.
                if let Some((kind, detail)) = hint {
                    import_errors::record_error(&tx, &path_str, kind, &detail, now_unix())?;
                } else if import_errors::clear_error(&tx, &path_str)? {
                    // Task 1.9: a real pass-1 success (never the pass-2
                    // hint-refresh branch above) that actually deleted a
                    // prior error row — see `ScanReport::healed`'s doc
                    // comment for why the hint case must never land here.
                    report.healed += 1;
                }

                // Move detection (Stage 2 Task 8) only ever applies to a path
                // the DB has never seen before — a file whose path is already
                // known just falls through to the ordinary upsert below, even
                // if its content changed.
                // Skip move detection entirely when `stat` failed above (no
                // reliable device/inode to key step 1 off, and step 2's
                // fingerprint would be matching against a placeholder
                // `file_size` of 0) — see `file_stat`'s doc comment.
                let move_candidate = if is_update {
                    None
                } else {
                    match (device, inode) {
                        (Some(device), Some(inode)) => move_detect::find_move_candidate(
                            &tx,
                            &move_detect::MoveLookup {
                                device,
                                inode,
                                title: &title,
                                artist: &meta.artist,
                                album: &meta.album,
                                duration_ms: meta.duration_ms,
                                file_size,
                            },
                        )?,
                        _ => None,
                    }
                };

                if let Some(candidate) = move_candidate {
                    // A move: refresh path/tags/filesystem-identity on the
                    // existing row by id via the shared `apply_file_identity`
                    // — see its own doc comment for exactly what it touches
                    // (and, deliberately, doesn't).
                    move_detect::apply_file_identity(
                        &tx,
                        candidate.id,
                        path,
                        &title,
                        &meta,
                        untagged,
                        &move_detect::FileIdentity {
                            file_mtime: mtime,
                            file_size,
                            device,
                            inode,
                            mount_point: mount_point.clone(),
                        },
                    )?;
                    // Clear a stale import_errors row under the old path too
                    // (e.g. the old location briefly failed to read before
                    // being moved away) — the new path was already cleared
                    // above. Unconditional even for an untagged import: this
                    // is the OLD path's row, a different path string from
                    // the hint (if any) recorded above for the CURRENT path.
                    if import_errors::clear_error(&tx, &candidate.path)? {
                        report.healed += 1;
                    }
                    report.moved += 1;
                } else {
                    // `ON CONFLICT(path)` fires whenever this path already
                    // has a row — including one still carrying `removed_at`
                    // from a prior tombstone: the walk just proved the file
                    // is there, so `removed_at=NULL` in the `DO UPDATE SET`
                    // below resurrects it here too (evidence rule, Beschluss
                    // 7/12), same as the fast-path-restore branch and
                    // `apply_file_identity`'s move arm above.
                    let (
                        title_p,
                        artist_p,
                        album_p,
                        album_artist_p,
                        artist_mbid_p,
                        year_p,
                        track_no_p,
                        disc_no_p,
                        genre_p,
                        duration_ms_p,
                        bitrate_kbps_p,
                        untagged_p,
                    ) = tag_param_values(&title, &meta, untagged);
                    tx.execute(
                        "INSERT INTO tracks (path, title, artist, album, album_artist, artist_mbid,
                           year, track_no, disc_no, genre, duration_ms, bitrate_kbps, added_at,
                           file_mtime, file_size, device, inode, mount_point, untagged)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
                         ON CONFLICT(path) DO UPDATE SET
                           title=?2, artist=?3, album=?4, album_artist=?5,
                           artist_mbid=COALESCE(?6, artist_mbid),
                           artist_mbid_negative=CASE WHEN ?6 IS NOT NULL THEN 0 ELSE artist_mbid_negative END,
                           year=?7, track_no=?8, disc_no=?9, genre=?10,
                           duration_ms=?11, bitrate_kbps=?12, file_mtime=?14,
                           missing_since=NULL, missing_reason=NULL, removed_at=NULL,
                           file_size=?15, device=?16, inode=?17, mount_point=?18,
                           untagged=?19",
                        rusqlite::params![
                            path_str,
                            title_p,
                            artist_p,
                            album_p,
                            album_artist_p,
                            artist_mbid_p,
                            year_p,
                            track_no_p,
                            disc_no_p,
                            genre_p,
                            duration_ms_p,
                            bitrate_kbps_p,
                            now_unix(),
                            mtime,
                            file_size,
                            device,
                            inode,
                            mount_point,
                            untagged_p,
                        ],
                    )?;
                    if is_update {
                        report.updated += 1;
                    } else {
                        report.added += 1;
                    }
                }
            }
            Err(ScanError::Import { kind, detail }) => {
                // Both passes failed: `kind`/`detail` are pass 2's
                // classification (see `read_meta_with_fallback`'s doc
                // comment). Episode upsert — see `record_error`'s doc
                // comment.
                import_errors::record_error(&tx, &path_str, kind, &detail, now_unix())?;
                report.errors += 1;
            }
            // `read_meta_with_fallback` only ever produces `Import`;
            // propagating any other variant is safer than an
            // `unreachable!()` panic if that changes.
            Err(other) => return Err(other),
        }
        if let Some(progress) = &mut progress {
            progress.advance(path);
        }
    }

    // `candidates` (`PRESENT`-only) feeds the mark phase below regardless of
    // outcome. The guard's own evidence, `guard_evidence` (the wider
    // `removed_at IS NULL` list — see `scanner_vanish::guard_evidence_under_
    // root`'s doc comment for why it must NOT be `candidates`), is only
    // queried when the walk found nothing, the same short-circuit
    // `root_unavailable` used before this was split into two lists — so a
    // scan that actually found audio files never pays for the extra query.
    let candidates = vanish::present_candidates_under_root(&tx, root)?;
    let guard_evidence = if audio_files_seen == 0 {
        Some(vanish::guard_evidence_under_root(&tx, root)?)
    } else {
        None
    };
    let root_unavailable = guard_evidence.as_ref().is_some_and(|evidence| {
        !evidence.is_empty() && !vanish::any_candidate_confirms_root_device(evidence, root)
    });

    let outcome = if root_unavailable {
        // Root-Guard case (b): see this function's `## Root guard` doc
        // section. The upserts the walk itself produced (normally none,
        // since `audio_files_seen == 0`, but a traversal error is still
        // possible) still commit below — only the mark phase is skipped.
        tracing::warn!(
            root = %root.display(),
            candidate_count = guard_evidence.map_or(0, |e| e.len()),
            "scan: walk found no audio files and no known track under root confirms the \
             root's current device; reporting RootUnavailable instead of marking tracks missing"
        );
        ScanOutcome::RootUnavailable {
            root: root.to_path_buf(),
        }
    } else {
        report.vanished = vanish::mark_vanished(&tx, candidates)?;
        // T0.3: one collective change-log row per scan that actually touched
        // the catalog (never per track, never for a no-op reconcile), inside
        // the same transaction as the walk so the event and the rows it
        // announces commit together. Foreign scanners (`reprise-cli scan`)
        // wake the running app through this; the app's own scans carry its
        // writer token and are filtered out by its own consumer.
        if scan_touched_library(&report) {
            crate::events::record(&tx, "library", "", "scan")?;
        }
        ScanOutcome::Completed(report)
    };
    tx.commit()?;
    Ok(outcome)
}

/// Whether a completed scan changed anything a consumer's view reflects — any
/// catalog upsert/move/vanish/exclusion or an import-error row added or healed.
/// A scan that only skipped unchanged files leaves every view identical and so
/// logs no event.
fn scan_touched_library(report: &ScanReport) -> bool {
    report.added
        + report.updated
        + report.moved
        + report.vanished
        + report.excluded
        + report.healed
        + report.errors
        > 0
}

// Task 1.5: the vanish-mark phase `scan_folder_inner` folds in above lives in
// its own file purely to keep this one under the project's 800-line rule —
// see `scanner_vanish.rs`'s own module doc comment. Not `#[cfg(test)]`: this
// is production code, always compiled.
// Scan progress counting/reporting lives in its own file for the same
// 800-line reason — see `scanner_progress.rs`'s own module doc comment.
#[path = "scanner_progress.rs"]
mod scan_progress;

#[path = "scanner_vanish.rs"]
mod vanish;

// Task 1.6: the mount_point memoization used above lives in its own file for
// the same 800-line reason — see `scanner_mount.rs`'s own doc comment.
#[path = "scanner_mount.rs"]
mod mount;

// Task 1.8: `TrackMeta`, the pass-1/pass-2 lofty reads, and their
// orchestration live in their own file for the same 800-line reason — see
// `scanner_meta.rs`'s own module doc comment.
#[path = "scanner_meta.rs"]
pub(crate) mod track_meta;

#[path = "scanner_repair.rs"]
mod repair;

// Task 1.8: move detection also moved out to make room for the above — see
// `scanner_move.rs`'s own module doc comment.
#[path = "scanner_move.rs"]
pub(crate) mod move_detect;

#[cfg(test)]
#[path = "scanner_progress_tests.rs"]
mod progress_tests;

// Stage 3 Task 1: the test suite moved to its own file purely to keep this
// file under the project's 800-line rule — see `scanner_tests.rs`'s module
// doc comment.
#[cfg(test)]
#[path = "scanner_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "scanner_metadata_persistence_tests.rs"]
mod metadata_persistence_tests;

// Task 1.7: the episode/dismiss/directory-dedup test suite lives in its own
// file, same 800-line reason as every other `_tests.rs` sibling here.
#[cfg(test)]
#[path = "scanner_import_errors_tests.rs"]
mod import_errors_tests;

#[cfg(test)]
#[path = "scanner_vanished_tests.rs"]
mod vanished_tests;

// Task 1.8: the tag-free relaxed second-pass test suite lives in its own
// file, same 800-line reason as every other `_tests.rs` sibling here.
#[cfg(test)]
#[path = "scanner_untagged_tests.rs"]
mod untagged_tests;

// Task 1.9: the tombstone-resurrect + `healed`-counter test suite lives in
// its own file, same 800-line reason as every other `_tests.rs` sibling
// here.
#[cfg(test)]
#[path = "scanner_tombstone_tests.rs"]
mod tombstone_tests;

#[cfg(test)]
#[path = "scanner_exclusion_tests.rs"]
mod exclusion_tests;
