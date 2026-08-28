use reprise_core::podcasts::pipeline::{SyncAbort, SyncProgress};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SyncStep {
    Added,
    ReadingFeed,
    DownloadingArtwork,
    Failed,
}

#[derive(Clone, Debug)]
pub(super) struct SyncRowState {
    pub step: SyncStep,
    pub episodes_found: usize,
    pub error: Option<String>,
    pub abort: SyncAbort,
}

impl SyncRowState {
    pub(super) fn new(abort: SyncAbort) -> Self {
        Self {
            step: SyncStep::Added,
            episodes_found: 0,
            error: None,
            abort,
        }
    }

    pub(super) fn apply(&mut self, progress: &SyncProgress) {
        match progress {
            SyncProgress::Started => self.step = SyncStep::ReadingFeed,
            SyncProgress::FeedRead { episodes_found } => {
                self.step = SyncStep::ReadingFeed;
                self.episodes_found = *episodes_found;
            }
            SyncProgress::FetchingArtwork => self.step = SyncStep::DownloadingArtwork,
            SyncProgress::Failed(error) => {
                self.step = SyncStep::Failed;
                self.error = Some(format!("{error:?}"));
            }
            SyncProgress::Done(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::podcasts::pipeline::{SyncAbort, SyncProgress};

    use super::{SyncRowState, SyncStep};

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
        assert!(state.error.is_some());
    }
}
