//! Pure view-model for the conversion/staging view (plan §2.4/7; docs/ux-rules
//! Section AB). No GTK: it maps `ai_jobs` rows + staging presence onto per-row
//! states, the play/wait decision (INST-4/INST-5), and the single aggregate
//! progress figure (INST-2), so every rule that governs the view is testable
//! without a display.
//!
//! All numbers come from the job rows — `aggregate` mirrors
//! `reprise_core::ai_jobs::batch_progress`'s formula over the whole active list,
//! so the bar shows the same figures the CLI/MCP report (plan §2.2).

use reprise_core::ai_jobs::{AiJob, JobState};
use reprise_core::stem_separation::PROGRESS_COMPLETE;

/// One conversion row's user-visible state (INST-3). `Cancelled` jobs never
/// reach the view (`list_active_jobs` filters them), so there is no variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum RowState {
    /// Waiting for a worker to claim it.
    Queued,
    /// A worker is rendering it; carries the live permille for the row bar.
    Processing { permille: u16 },
    /// Finished, render in staging, not yet saved — `playable` iff the staging
    /// file is actually present (INST-4/INST-8).
    DoneUnsaved { playable: bool },
    /// Saved: promoted to a library track and kept in the row until cleanup
    /// (INST-6). Always playable (the library track exists).
    Saved,
    /// The render failed (INST-3); a diagnostic kind is shown, nothing plays.
    Failed,
}

/// What activating (clicking) a row does — the heart of the wait-with-progress
/// rule (INST-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum RowClickAction {
    /// Play the finished render/track immediately (INST-4).
    Play,
    /// The row is still processing: block the start and show render progress —
    /// **no** original fallback, **no** auto-skip (INST-5).
    WaitWithProgress,
    /// Nothing to do (queued or failed): the click neither plays nor waits.
    Inert,
}

/// Derives a row's state from its job row and whether a staging render exists.
/// `staged` is the caller's `StagingStore::exists(job.id)` (a finished-but-
/// unsaved render whose file was discarded out from under the row is no longer
/// playable).
pub(in crate::ui) fn row_state(job: &AiJob, staged: bool) -> RowState {
    match job.state {
        JobState::Queued => RowState::Queued,
        JobState::Running => RowState::Processing {
            permille: job.progress_permille,
        },
        JobState::Done => {
            if job.result_track_id.is_some() {
                RowState::Saved
            } else {
                RowState::DoneUnsaved { playable: staged }
            }
        }
        JobState::Failed => RowState::Failed,
        // Cancelled rows are filtered before the view; treat defensively as gone.
        JobState::Cancelled => RowState::Failed,
    }
}

/// Whether a row can be played right now (its Play affordance is enabled).
pub(in crate::ui) fn is_playable(state: RowState) -> bool {
    matches!(
        state,
        RowState::Saved | RowState::DoneUnsaved { playable: true }
    )
}

/// The wait-with-progress rule (INST-5): a click on a processing row waits with
/// progress, a finished/playable row plays, everything else is inert. Crucially
/// a processing row never resolves to `Play` (no original fallback, no skip).
pub(in crate::ui) fn click_action(state: RowState) -> RowClickAction {
    match state {
        RowState::Processing { .. } => RowClickAction::WaitWithProgress,
        _ if is_playable(state) => RowClickAction::Play,
        _ => RowClickAction::Inert,
    }
}

/// The single aggregate progress figure for the header bar (INST-2). Mirrors
/// `reprise_core::ai_jobs::batch_progress` over the whole active list: `total`
/// rows, `done` finished renders, and `permille` the mean completion (a `done`
/// row counts as fully complete).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::ui) struct Aggregate {
    pub done: usize,
    pub total: usize,
    pub permille: u16,
}

impl Aggregate {
    /// Completion as a 0.0..=1.0 fraction — the `gtk::ProgressBar` value.
    pub(in crate::ui) fn fraction(self) -> f64 {
        f64::from(self.permille) / f64::from(PROGRESS_COMPLETE)
    }

    /// Whole-percent completion for the caption.
    pub(in crate::ui) fn percent(self) -> u16 {
        // Round to nearest percent from permille.
        (self.permille + 5) / 10
    }
}

/// Computes the aggregate over the active jobs (all rows the view shows).
pub(in crate::ui) fn aggregate(jobs: &[AiJob]) -> Aggregate {
    let total = jobs.len();
    if total == 0 {
        return Aggregate::default();
    }
    let mut done = 0usize;
    let mut permille_sum = 0u32;
    for job in jobs {
        let permille = match job.state {
            JobState::Done => {
                done += 1;
                u32::from(PROGRESS_COMPLETE)
            }
            _ => u32::from(job.progress_permille),
        };
        permille_sum += permille;
    }
    let permille = (permille_sum / total as u32) as u16;
    Aggregate {
        done,
        total,
        permille,
    }
}

/// Whether any row is still undecided (a finished, unsaved render) — the
/// predicate "clear playlist" warns on (INST-7) and the save-all/discard flow
/// keys on.
pub(in crate::ui) fn has_undecided(jobs: &[AiJob]) -> bool {
    jobs.iter()
        .any(|job| job.state == JobState::Done && job.result_track_id.is_none())
}

#[cfg(test)]
#[path = "conversion_model_tests.rs"]
mod tests;
