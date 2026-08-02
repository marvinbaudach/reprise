//! Serial, cancellable library-wide lyrics cache population.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::queries::TrackSummary;

use super::cache::CacheDecision;
use super::{LyricsBody, LyricsError, LyricsHit, LyricsQuery, NeedsFetch};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchState {
    Idle,
    Running,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchProgress {
    pub state: BatchState,
    pub checked: usize,
    pub total: usize,
    pub downloaded: usize,
    pub unavailable: usize,
}

impl BatchProgress {
    #[must_use]
    pub fn idle() -> Self {
        Self {
            state: BatchState::Idle,
            checked: 0,
            total: 0,
            downloaded: 0,
            unavailable: 0,
        }
    }

    #[must_use]
    pub fn running(total: usize) -> Self {
        Self {
            state: if total == 0 {
                BatchState::Complete
            } else {
                BatchState::Running
            },
            total,
            ..Self::idle()
        }
    }

    fn advance(mut self, outcome: BatchItemOutcome) -> Self {
        self.checked = self.checked.saturating_add(1).min(self.total);
        match outcome {
            BatchItemOutcome::Skipped | BatchItemOutcome::Failed => {}
            BatchItemOutcome::Downloaded => self.downloaded += 1,
            BatchItemOutcome::Unavailable => self.unavailable += 1,
        }
        if self.checked == self.total {
            self.state = BatchState::Complete;
        }
        self
    }

    fn fail(mut self) -> Self {
        self.state = BatchState::Failed;
        self
    }

    #[must_use]
    pub fn fraction(self) -> f64 {
        if self.total == 0 {
            return f64::from(self.state == BatchState::Complete);
        }
        self.checked as f64 / self.total as f64
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchTrack {
    pub query: LyricsQuery,
    pub path: PathBuf,
}

impl From<TrackSummary> for BatchTrack {
    fn from(summary: TrackSummary) -> Self {
        Self {
            query: LyricsQuery {
                title: summary.title,
                artist: summary.artist,
                album: summary.album,
                duration_ms: summary.duration_ms,
            },
            path: summary.path.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchRunStatus {
    Finished,
    Cancelled,
}

#[derive(Clone, Copy)]
enum BatchItemOutcome {
    Skipped,
    Downloaded,
    Unavailable,
    Failed,
}

type LocalLookup<'a> = Arc<dyn Fn(&Path) -> bool + Send + Sync + 'a>;
type NeedsLookup<'a> = Arc<dyn Fn(&LyricsQuery) -> CacheDecision + Send + Sync + 'a>;
type OnlineLookup<'a> = Arc<
    dyn Fn(&LyricsQuery, &Path, &CacheDecision) -> Result<LyricsHit, LyricsError>
        + Send
        + Sync
        + 'a,
>;
type AllBreakersOpen<'a> = Arc<dyn Fn() -> bool + Send + Sync + 'a>;

#[derive(Clone)]
struct BatchServices<'a> {
    local: LocalLookup<'a>,
    needs: NeedsLookup<'a>,
    online: OnlineLookup<'a>,
    all_breakers_open: AllBreakersOpen<'a>,
}

impl<'a> BatchServices<'a> {
    fn production(source: &'a dyn crate::library::source::LibrarySource) -> Self {
        Self {
            local: Arc::new(|path| super::local_hit(path).is_some()),
            needs: Arc::new(super::cache::decision),
            online: Arc::new(move |query, path, decision| {
                super::load_or_fetch_with_cache_decision(source, query, Some(path), decision)
            }),
            all_breakers_open: Arc::new(super::all_network_breakers_open),
        }
    }
}

/// Populates the lyrics cache for `tracks` synchronously and serially.
///
/// Frontends own the worker thread and use `on_progress` to transport each
/// progress snapshot back to their UI loop. Returning `false` from the callback
/// stops the run because its consumer has gone away.
pub fn run_batch(
    tracks: &[BatchTrack],
    is_cancelled: impl Fn() -> bool,
    network_allowed: impl Fn() -> bool,
    on_progress: impl FnMut(BatchProgress) -> bool,
) -> BatchRunStatus {
    run_batch_with_source(
        &crate::library::source::UnixLibrarySource,
        tracks,
        is_cancelled,
        network_allowed,
        on_progress,
    )
}

pub fn run_batch_with_source(
    source: &dyn crate::library::source::LibrarySource,
    tracks: &[BatchTrack],
    is_cancelled: impl Fn() -> bool,
    network_allowed: impl Fn() -> bool,
    on_progress: impl FnMut(BatchProgress) -> bool,
) -> BatchRunStatus {
    run_batch_with_services(
        tracks,
        &BatchServices::production(source),
        is_cancelled,
        network_allowed,
        on_progress,
    )
}

fn run_batch_with_services(
    tracks: &[BatchTrack],
    services: &BatchServices<'_>,
    is_cancelled: impl Fn() -> bool,
    network_allowed: impl Fn() -> bool,
    mut on_progress: impl FnMut(BatchProgress) -> bool,
) -> BatchRunStatus {
    let mut progress = BatchProgress::running(tracks.len());
    for track in tracks {
        if is_cancelled() {
            return BatchRunStatus::Cancelled;
        }
        let decision = (!(services.local)(&track.path)).then(|| (services.needs)(&track.query));
        let needs = decision
            .as_ref()
            .map_or(NeedsFetch::Skip, CacheDecision::classification);
        let outcome = if needs == NeedsFetch::Skip {
            BatchItemOutcome::Skipped
        } else if !network_allowed() {
            return BatchRunStatus::Cancelled;
        } else if (services.all_breakers_open)() {
            let _ = on_progress(progress.fail());
            return BatchRunStatus::Finished;
        } else {
            item_outcome(
                needs,
                &(services.online)(
                    &track.query,
                    &track.path,
                    decision
                        .as_ref()
                        .expect("a non-skipped cache decision should exist"),
                ),
            )
        };
        progress = progress.advance(outcome);
        if matches!(outcome, BatchItemOutcome::Failed) && (services.all_breakers_open)() {
            progress = progress.fail();
        }
        let terminal = progress.state != BatchState::Running;
        if !on_progress(progress) || terminal {
            return BatchRunStatus::Finished;
        }
    }
    BatchRunStatus::Finished
}

/// A `RetryForSynced` lookup only re-asks the remaining sources for a synced
/// text. Plain text re-confirms the cache and is not a newly cached track.
fn item_outcome(needs: NeedsFetch, result: &Result<LyricsHit, LyricsError>) -> BatchItemOutcome {
    match result {
        Ok(hit) => {
            if needs == NeedsFetch::RetryForSynced && matches!(hit.body, LyricsBody::Plain(_)) {
                BatchItemOutcome::Skipped
            } else {
                BatchItemOutcome::Downloaded
            }
        }
        Err(LyricsError::NotFound | LyricsError::MissingMetadata) => BatchItemOutcome::Unavailable,
        Err(_) => BatchItemOutcome::Failed,
    }
}

#[cfg(test)]
#[path = "batch_tests.rs"]
mod tests;
