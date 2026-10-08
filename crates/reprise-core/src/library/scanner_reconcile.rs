//! The scan's tail: what the finished walk lets the catalog conclude about the
//! files it did not find, and the one atomic transaction that records it.
//!
//! Deciding that a file is gone takes filesystem questions, and on a slow share
//! a catalog's worth of them. They are therefore asked in between two leases,
//! with no lock held: a read lease gathers the facts, [`plan`] asks the source,
//! and a write lease (IMMEDIATE, so a rival commit cannot fail its upgrade)
//! applies the verdicts. Under the write lock the candidate rows are read again
//! and a verdict is applied only to a row that is still the same candidate, so
//! a row changed in between is left for the next scan and never marked on
//! stale evidence.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::vanish::{self, WalkEvidence};
use super::{mobile_sync, now_unix, ScanError, ScanOutcome, ScanReport, WalkTrace};
use crate::library::source::{LibraryPathPresence, LibrarySource};
use crate::models::MissingReason;

/// A catalog row the tail reasons about: id, recorded path, recorded device.
type Candidate = (i64, String, Option<i64>);

/// What the read lease takes from the catalog.
pub(super) struct TailFacts {
    candidates: Vec<Candidate>,
    reclassifiable: Vec<Candidate>,
    guard_evidence: Option<Vec<Candidate>>,
    evidence: Option<WalkEvidence>,
}

/// The verdicts, ready to apply under the write lock.
pub(super) struct TailPlan {
    root_unavailable: bool,
    corrections: Vec<Candidate>,
    marks: Vec<Mark>,
}

/// A present row the filesystem has confirmed gone.
struct Mark {
    id: i64,
    path: String,
    reason: MissingReason,
    mount_point: Option<String>,
    verdict: &'static str,
}

/// The metadata a mobile sync left beside the audio, applied inside the atomic
/// tail transaction together with sidecar registration and vanish decisions.
pub(super) fn apply_mobile_sync(
    mobile_sync: &mobile_sync::MobileSyncDiscovery,
    metadata: Option<&crate::device_sync::track_metadata_list::TrackMetadataList>,
    tx: &rusqlite::Transaction,
    report: &mut ScanReport,
) -> Result<(), ScanError> {
    report.updated = report
        .updated
        .saturating_add(mobile_sync.apply_metadata(metadata, tx)?);
    mobile_sync.register_analysis_sidecars(tx)?;
    mobile_sync.register_device_paths(tx)?;
    Ok(())
}

/// Reads the candidate rows. `candidates` (`PRESENT`-only) feeds the mark phase
/// regardless of outcome. The root guard's own evidence (the wider
/// `removed_at IS NULL` list — see `scanner_vanish::guard_evidence_under_root`'s
/// doc comment for why it must NOT be `candidates`) is only queried when the
/// walk found nothing, so a scan that actually found audio files never pays
/// for the extra query.
pub(super) fn read_facts(
    tx: &rusqlite::Transaction,
    root: &Path,
    trace: WalkTrace,
) -> Result<TailFacts, ScanError> {
    let WalkTrace {
        audio_files_seen,
        observed_paths,
        dirs,
        failed,
    } = trace;
    let guard_evidence = if audio_files_seen == 0 {
        Some(vanish::guard_evidence_under_root(tx, root)?)
    } else {
        None
    };
    Ok(TailFacts {
        candidates: vanish::present_candidates_under_root(tx, root)?,
        reclassifiable: vanish::reclassification_candidates_under_root(tx, root)?,
        // A walk that saw no audio file at all is exactly the situation Android
        // cannot distinguish from lost storage. An empty walk is a question, not
        // proof: layer 3 stays silent and only a real source `Absent` still marks.
        evidence: vanish::evidence_after_walk(audio_files_seen, observed_paths, &dirs, &failed),
        guard_evidence,
    })
}

/// Root-Guard case (b) or the mark phase: decides whether this scan may say
/// anything about the files it did not see, and asks the source about the rest.
/// Holds no database handle.
pub(super) fn plan(source: &dyn LibrarySource, root: &Path, facts: TailFacts) -> TailPlan {
    let root_unavailable = facts.guard_evidence.as_ref().is_some_and(|guard| {
        !guard.is_empty() && !vanish::any_candidate_confirms_root_with(source, guard, root)
    });
    if root_unavailable {
        // Root-Guard case (b): see `scan_folder_inner`'s `## Root guard` doc
        // section. Walk batches have already committed. The tail still commits
        // the mobile-sync changes; only the vanish and event-log phase is skipped.
        tracing::warn!(
            root = %root.display(),
            candidate_count = facts.guard_evidence.map_or(0, |e| e.len()),
            "scan: walk found no audio files and no known track under root confirms the \
             root's current device; reporting RootUnavailable instead of marking tracks missing"
        );
        return TailPlan {
            root_unavailable,
            corrections: Vec::new(),
            marks: Vec::new(),
        };
    }
    TailPlan {
        root_unavailable,
        corrections: plan_corrections(source, root, facts.reclassifiable, facts.evidence.as_ref()),
        marks: plan_marks(source, root, facts.candidates, facts.evidence.as_ref()),
    }
}

/// Applies the plan inside the tail transaction and returns the outcome it
/// commits.
pub(super) fn apply(
    tx: &rusqlite::Transaction,
    root: &Path,
    plan: &TailPlan,
    mut report: ScanReport,
) -> Result<ScanOutcome, ScanError> {
    if plan.root_unavailable {
        return Ok(ScanOutcome::RootUnavailable {
            root: root.to_path_buf(),
        });
    }
    let now = now_unix();
    let reclassified = apply_corrections(tx, root, &plan.corrections, now)?;
    report.vanished = apply_marks(tx, root, &plan.marks, now)?;
    // T0.3: one collective change-log row per scan that actually touched
    // the catalog (never per track, never for a no-op reconcile), inside
    // the same tail transaction as the final scan decisions. Foreign scanners
    // (`reprise-cli scan`) wake the running app through this; the app's own
    // scans carry its writer token and are filtered out by its own consumer.
    if super::scan_touched_library(&report) || reclassified > 0 {
        crate::events::record(tx, "library", "", "scan")?;
        crate::library::startup_tasks::advance_library_signature_in(tx)?;
    }
    Ok(ScanOutcome::Completed(report))
}

/// For every `candidates` row whose file no longer exists at its source, the
/// reason it is missing (via [`LibrarySource::reachability`]). A row that's
/// still present (e.g. the walk's own move-detection just relocated a different
/// row onto this path, or the file genuinely never left) gets no mark. Paths
/// already delivered by the current walk are known present and are skipped
/// without another source query; only unseen candidates need a probe.
fn plan_marks(
    source: &dyn LibrarySource,
    root: &Path,
    candidates: Vec<Candidate>,
    evidence: Option<&WalkEvidence>,
) -> Vec<Mark> {
    let mut marks = Vec::new();
    for (id, path_str, device) in candidates {
        let path = Path::new(&path_str);
        if evidence.is_some_and(|evidence| evidence.observed.contains(path)) {
            continue;
        }
        // A mark needs confirmed absence. `Present` always keeps the row
        // live. `Unknown` is not a verdict either, but the walk that just ran
        // may hold the evidence the source itself could not produce.
        let verdict = match source.probe(path, super::LibraryLinkMode::Follow) {
            LibraryPathPresence::Absent => Some("probe"),
            LibraryPathPresence::Present(_) => None,
            LibraryPathPresence::Unknown
                if vanish::absence_confirmed_by_walk(source, evidence, path, root) =>
            {
                Some("walk")
            }
            LibraryPathPresence::Unknown => None,
        };
        let Some(verdict) = verdict else {
            continue;
        };
        let reason = source.reachability(path, device);
        // `mount_point` is only ever read back for `unmounted` rows (see
        // `queries::issues`' `query_unavailable_groups`, which binds the
        // reason). Resolving it costs `mounts::mount_point_of` its own
        // ancestor walk, on top of the one `reachability` just did, so it is
        // resolved for the one reason that consumes it and left as-is
        // otherwise — a `deleted` row keeps whatever the last successful
        // scan recorded, which no query looks at.
        let mount_point = (reason == MissingReason::Unmounted)
            .then(|| source.mount_point(path))
            .flatten()
            .map(|mount| mount.to_string_lossy().into_owned());
        marks.push(Mark {
            id,
            path: path_str,
            reason,
            mount_point,
            verdict,
        });
    }
    marks
}

/// Marks every planned row that is still a present candidate and returns how
/// many it newly marked.
fn apply_marks(
    tx: &rusqlite::Transaction,
    root: &Path,
    marks: &[Mark],
    now: i64,
) -> Result<u32, ScanError> {
    let still_present: HashMap<i64, String> = vanish::present_candidates_under_root(tx, root)?
        .into_iter()
        .map(|(id, path, _)| (id, path))
        .collect();
    let mut marked = 0u32;
    for mark in marks {
        if still_present.get(&mark.id) != Some(&mark.path) {
            continue;
        }
        if mark.reason == MissingReason::Unmounted {
            tx.execute(
                "UPDATE tracks SET missing_since = ?2, missing_reason = ?3, mount_point = ?4 \
                 WHERE id = ?1",
                rusqlite::params![mark.id, now, mark.reason.as_str(), mark.mount_point],
            )?;
            tracing::info!(
                path = %mark.path,
                reason = mark.reason.as_str(),
                mount_point = ?mark.mount_point,
                verdict = mark.verdict,
                "scan: marked vanished track missing (mount currently absent)"
            );
        } else {
            tx.execute(
                "UPDATE tracks SET missing_since = ?2, missing_reason = ?3 WHERE id = ?1",
                rusqlite::params![mark.id, now, mark.reason.as_str()],
            )?;
            tracing::info!(
                path = %mark.path,
                reason = mark.reason.as_str(),
                verdict = mark.verdict,
                "scan: marked vanished track missing"
            );
        }
        marked += 1;
    }
    Ok(marked)
}

/// Corrects stale `unmounted`/`unknown` reasons only when current source
/// evidence positively resolves the still-absent item as [`MissingReason::Deleted`].
fn plan_corrections(
    source: &dyn LibrarySource,
    root: &Path,
    candidates: Vec<Candidate>,
    evidence: Option<&WalkEvidence>,
) -> Vec<Candidate> {
    candidates
        .into_iter()
        .filter(|(_, path_str, device)| {
            let path = Path::new(path_str);
            let absent = match source.probe(path, super::LibraryLinkMode::Follow) {
                LibraryPathPresence::Absent => true,
                LibraryPathPresence::Present(_) => false,
                LibraryPathPresence::Unknown => {
                    vanish::absence_confirmed_by_walk(source, evidence, path, root)
                }
            };
            absent && source.reachability(path, *device) == MissingReason::Deleted
        })
        .collect()
}

/// Writes the planned corrections that are still unresolved missing rows.
/// `missing_since` is deliberately left untouched because it is the user-facing
/// first-absence time. If auto-clean was already armed, its global lower-bound
/// clock is advanced to `now`, giving every corrected row the full configured
/// grace period without changing that display timestamp.
///
/// That advance is what makes this a *frequent* writer of `auto_clean_armed_at`,
/// where before it moved only on a rare user action.
/// `maintenance::remove_auto_clean_eligible_tracks` re-checks the deadline at
/// delete time for exactly that reason — see its guard.
fn apply_corrections(
    tx: &rusqlite::Transaction,
    root: &Path,
    corrections: &[Candidate],
    now: i64,
) -> Result<u32, ScanError> {
    let unresolved: HashSet<i64> = vanish::reclassification_candidates_under_root(tx, root)?
        .into_iter()
        .map(|(id, _, _)| id)
        .collect();
    let mut corrected = 0u32;
    for (id, _, _) in corrections {
        if !unresolved.contains(id) {
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
        vanish::rearm_auto_clean_if_armed(tx, now)?;
    }
    Ok(corrected)
}

/// Plans and applies the marks in one go, for tests that drive the mark phase
/// directly.
#[cfg(test)]
pub(super) fn mark_vanished_with(
    source: &dyn LibrarySource,
    tx: &rusqlite::Transaction,
    root: &Path,
    candidates: Vec<Candidate>,
    evidence: Option<&WalkEvidence>,
) -> Result<u32, ScanError> {
    let marks = plan_marks(source, root, candidates, evidence);
    apply_marks(tx, root, &marks, now_unix())
}

/// Plans and applies the reason corrections in one go, for tests that drive
/// the reclassification directly.
#[cfg(test)]
pub(super) fn reclassify_missing_with(
    source: &dyn LibrarySource,
    tx: &rusqlite::Transaction,
    root: &Path,
    evidence: Option<&WalkEvidence>,
    now: i64,
) -> Result<u32, ScanError> {
    let candidates = vanish::reclassification_candidates_under_root(tx, root)?;
    let corrections = plan_corrections(source, root, candidates, evidence);
    apply_corrections(tx, root, &corrections, now)
}
