//! Toolkit-neutral state of a running library analysis.
//!
//! Every frontend that can analyze a library — the GTK window, the Android app,
//! the Tauri shell — has to answer the same three questions: what state is the
//! run in, how far along is it, and should the user be shown anything at all.
//! Those answers live here so they cannot drift apart per frontend; what stays
//! in each frontend is the wiring and the words.

use reprise_core::spectrogram_backfill::{BackfillStatus, BackfillSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisState {
    Idle,
    Running,
    Complete,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisProgress {
    pub state: AnalysisState,
    pub analyzed: usize,
    pub total: usize,
    pub failed: usize,
}

impl AnalysisProgress {
    #[must_use]
    pub fn idle() -> Self {
        Self {
            state: AnalysisState::Idle,
            analyzed: 0,
            total: 0,
            failed: 0,
        }
    }

    #[must_use]
    pub fn running() -> Self {
        Self {
            state: AnalysisState::Running,
            ..Self::idle()
        }
    }

    #[must_use]
    pub fn failed() -> Self {
        Self {
            state: AnalysisState::Failed,
            ..Self::idle()
        }
    }

    #[must_use]
    pub fn is_running(self) -> bool {
        self.state == AnalysisState::Running
    }

    #[must_use]
    pub fn fraction(self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.analyzed as f64 / self.total as f64).clamp(0.0, 1.0)
    }

    /// Whether the user should be shown anything at all.
    ///
    /// The analysis starts by itself on every launch, so the common case is a
    /// library that is already done. That run must leave no trace — otherwise
    /// every start flashes a card reporting "0 analyzed". A failure still
    /// shows: that is not noise.
    #[must_use]
    pub fn is_worth_showing(self) -> bool {
        match self.state {
            AnalysisState::Idle => false,
            AnalysisState::Failed => true,
            _ => self.total > 0 || self.analyzed > 0 || self.failed > 0,
        }
    }

    /// Whether this state is terminal and should fade out on its own.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self.state,
            AnalysisState::Complete | AnalysisState::Stopped | AnalysisState::Failed
        )
    }
}

/// The closing state of a run, from its summary.
///
/// A worker that vanished without a summary counts as failed rather than
/// complete: reporting a clean finish for a run whose outcome nobody saw is
/// exactly the lie that made a missing backfill hard to notice in the first
/// place.
#[must_use]
pub fn settled(progress: AnalysisProgress, summary: Option<BackfillSummary>) -> AnalysisProgress {
    let Some(summary) = summary else {
        return AnalysisProgress {
            state: AnalysisState::Failed,
            ..progress
        };
    };
    AnalysisProgress {
        state: match summary.status {
            BackfillStatus::Completed => AnalysisState::Complete,
            BackfillStatus::Cancelled => AnalysisState::Stopped,
        },
        analyzed: summary.stored,
        total: progress.total.max(summary.stored),
        failed: summary.failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(status: BackfillStatus, stored: usize, failed: usize) -> BackfillSummary {
        BackfillSummary {
            status,
            stored,
            failed,
            source_changed: 0,
        }
    }

    #[test]
    fn a_completed_run_carries_its_counts() {
        let settled = settled(
            AnalysisProgress {
                total: 4,
                ..AnalysisProgress::running()
            },
            Some(summary(BackfillStatus::Completed, 3, 1)),
        );

        assert_eq!(settled.state, AnalysisState::Complete);
        assert_eq!((settled.analyzed, settled.failed), (3, 1));
        assert!(settled.is_terminal());
    }

    #[test]
    fn a_cancelled_run_is_stopped_not_complete() {
        let settled = settled(
            AnalysisProgress::running(),
            Some(summary(BackfillStatus::Cancelled, 2, 0)),
        );

        assert_eq!(settled.state, AnalysisState::Stopped);
    }

    #[test]
    fn a_run_without_a_summary_counts_as_failed() {
        assert_eq!(
            settled(AnalysisProgress::running(), None).state,
            AnalysisState::Failed
        );
    }

    #[test]
    fn an_autostarted_run_with_nothing_to_do_is_never_shown() {
        assert!(!AnalysisProgress::idle().is_worth_showing());
        assert!(!AnalysisProgress::running().is_worth_showing());
        assert!(!settled(
            AnalysisProgress::running(),
            Some(summary(BackfillStatus::Completed, 0, 0))
        )
        .is_worth_showing());
    }

    #[test]
    fn a_run_with_real_work_and_any_failure_is_shown() {
        assert!(AnalysisProgress {
            total: 1846,
            ..AnalysisProgress::running()
        }
        .is_worth_showing());
        assert!(AnalysisProgress::failed().is_worth_showing());
    }

    #[test]
    fn the_fraction_never_leaves_the_bar() {
        assert!((AnalysisProgress::idle().fraction() - 0.0).abs() < f64::EPSILON);
        let over = AnalysisProgress {
            analyzed: 9,
            total: 4,
            ..AnalysisProgress::running()
        };
        assert!((over.fraction() - 1.0).abs() < f64::EPSILON);
    }
}
