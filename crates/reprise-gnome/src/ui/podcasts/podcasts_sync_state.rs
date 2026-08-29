use reprise_core::podcasts::pipeline::{SyncAbort, SyncProgress};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SyncStep {
    Added,
    ReadingFeed,
    DownloadingArtwork,
    Failed,
}

/// Drops the rows of failed syncs whose source has meanwhile refreshed
/// successfully, keeping only those the refresh reported as still failing.
///
/// A failed sync row otherwise outlives the failure: nothing removes its entry
/// but a Cancel click, while the ordinary refresh keeps running and fills the
/// source with real episodes. The row then states a failure over live, correct
/// data — the exact lie this whole state exists to prevent. The bulk refresh
/// reports its own failures through `show_refresh_failures`, so a source that
/// is genuinely still broken loses nothing by having its row cleared here.
pub(super) fn clear_failed_syncs_that_recovered(
    syncing: &mut HashMap<i64, SyncRowState>,
    still_failing: &[i64],
) -> bool {
    let before = syncing.len();
    syncing.retain(|subscription_id, state| {
        state.is_loading() || still_failing.contains(subscription_id)
    });
    syncing.len() != before
}

pub(super) fn remove_subscription_sync_if_owned(
    syncing: &mut HashMap<i64, SyncRowState>,
    subscription_id: i64,
    owner: &SyncAbort,
) -> bool {
    let owned = syncing
        .get(&subscription_id)
        .is_some_and(|state| state.abort.is_same_request(owner));
    if owned {
        syncing.remove(&subscription_id);
    }
    owned
}

#[derive(Clone, Debug)]
pub(super) struct SyncRowState {
    pub step: SyncStep,
    pub episodes_found: usize,
    pub abort: SyncAbort,
}

impl SyncRowState {
    pub(super) fn new(abort: SyncAbort) -> Self {
        Self {
            step: SyncStep::Added,
            episodes_found: 0,
            abort,
        }
    }

    /// Whether a sync is still in flight for this row.
    ///
    /// Only a loading row may lock its disclosure and hold back its artwork —
    /// it genuinely has nothing to show yet. A failed row has stopped working,
    /// so it keeps its progress list and its Retry button but stops hiding
    /// whatever episodes and artwork the source does have.
    pub(super) fn is_loading(&self) -> bool {
        !matches!(self.step, SyncStep::Failed)
    }

    pub(super) fn apply(&mut self, progress: &SyncProgress) {
        match progress {
            SyncProgress::Started => self.step = SyncStep::ReadingFeed,
            SyncProgress::FeedRead { episodes_found } => {
                self.step = SyncStep::ReadingFeed;
                self.episodes_found = *episodes_found;
            }
            SyncProgress::FetchingArtwork => self.step = SyncStep::DownloadingArtwork,
            SyncProgress::Failed(_) => self.step = SyncStep::Failed,
            SyncProgress::Done(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::podcasts::pipeline::{SyncAbort, SyncProgress};

    use super::{clear_failed_syncs_that_recovered, SyncRowState, SyncStep};
    use std::collections::HashMap;

    fn failed_state() -> SyncRowState {
        let mut state = SyncRowState::new(SyncAbort::new());
        state.apply(&SyncProgress::Failed(
            reprise_core::podcasts::pipeline::SyncError::Database,
        ));
        state
    }

    #[test]
    fn pod_26_a_recovered_source_stops_claiming_its_sync_failed() {
        let mut syncing = HashMap::from([(7, failed_state())]);

        // The ordinary refresh succeeded for 7: no failure names it.
        let cleared = clear_failed_syncs_that_recovered(&mut syncing, &[]);

        assert!(cleared, "the recovered row should have been dropped");
        assert!(
            syncing.is_empty(),
            "a source with real episodes must not keep a failure overlay"
        );
    }

    #[test]
    fn pod_26_a_source_that_is_still_broken_keeps_its_failed_row() {
        let mut syncing = HashMap::from([(7, failed_state()), (9, failed_state())]);

        let cleared = clear_failed_syncs_that_recovered(&mut syncing, &[9]);

        assert!(cleared);
        assert!(!syncing.contains_key(&7), "7 refreshed cleanly");
        assert!(syncing.contains_key(&9), "9 is still failing");
    }

    #[test]
    fn pod_26_a_running_sync_survives_an_unrelated_refresh() {
        let mut syncing = HashMap::from([(7, SyncRowState::new(SyncAbort::new()))]);

        let cleared = clear_failed_syncs_that_recovered(&mut syncing, &[]);

        assert!(!cleared, "nothing was dropped");
        assert!(
            syncing.contains_key(&7),
            "a sync still in flight keeps its row"
        );
    }

    #[test]
    fn pod_26_only_a_running_sync_locks_the_row() {
        let running = SyncRowState::new(SyncAbort::new());
        assert!(
            running.is_loading(),
            "a running sync locks expansion and holds back artwork"
        );
        assert!(
            !failed_state().is_loading(),
            "a failed sync must release the row it can no longer fill"
        );
    }

    #[test]
    fn scoped_sync_progress_stays_owned_by_one_subscription() {
        let mut state = SyncRowState::new(SyncAbort::new());

        assert_eq!(state.step, SyncStep::Added);
        assert_eq!(state.episodes_found, 0);

        state.apply(&SyncProgress::Started);
        assert_eq!(state.step, SyncStep::ReadingFeed);

        state.apply(&SyncProgress::FeedRead { episodes_found: 7 });
        assert_eq!(state.step, SyncStep::ReadingFeed);
        assert_eq!(state.episodes_found, 7);

        state.apply(&SyncProgress::FetchingArtwork);
        assert_eq!(state.step, SyncStep::DownloadingArtwork);
        assert_eq!(state.episodes_found, 7);
    }

    #[test]
    fn failed_sync_reuses_the_three_line_state_instead_of_adding_a_step() {
        let mut state = SyncRowState::new(SyncAbort::new());

        state.apply(&SyncProgress::Failed(
            reprise_core::podcasts::pipeline::SyncError::Database,
        ));

        assert_eq!(state.step, SyncStep::Failed);
    }
}
