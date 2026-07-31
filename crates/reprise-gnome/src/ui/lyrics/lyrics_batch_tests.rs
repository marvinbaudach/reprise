use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use reprise_core::lyrics::{LyricsBody, LyricsHit, LyricsSource};

use super::*;

fn track(title: &str) -> BatchTrack {
    BatchTrack {
        query: LyricsQuery {
            title: title.into(),
            artist: "Synthetic Artist".into(),
            album: "Synthetic Album".into(),
            duration_ms: 10_000,
        },
        path: PathBuf::from(format!("/music/{title}.flac")),
    }
}

fn request(tracks: Vec<BatchTrack>) -> (WorkerRequest, async_channel::Receiver<WorkerEvent>) {
    let (events, receiver) = async_channel::unbounded();
    (
        WorkerRequest {
            generation: 1,
            generation_source: Arc::new(AtomicU64::new(1)),
            cancellation: ScanCancellation::default(),
            tracks,
            events,
        },
        receiver,
    )
}

fn services(
    online: impl Fn(&LyricsQuery, &Path) -> Result<LyricsHit, LyricsError> + Send + Sync + 'static,
) -> WorkerServices {
    WorkerServices {
        local: Arc::new(|_| false),
        needs: Arc::new(|_| NeedsFetch::Fetch),
        online: Arc::new(online),
        all_breakers_open: Arc::new(|| false),
    }
}

fn progress_events(receiver: &async_channel::Receiver<WorkerEvent>) -> Vec<LyricsBatchProgress> {
    std::iter::from_fn(|| receiver.try_recv().ok())
        .filter_map(|event| match event {
            WorkerEvent::Progress(progress) => Some(progress),
            WorkerEvent::Cancelled => None,
        })
        .collect()
}

#[test]
fn progress_counts_checked_downloaded_and_unavailable_without_counting_skips() {
    let progress = LyricsBatchProgress::running(3)
        .advance(BatchItemOutcome::Skipped)
        .advance(BatchItemOutcome::Downloaded)
        .advance(BatchItemOutcome::Unavailable);

    assert_eq!(progress.state, LyricsBatchState::Complete);
    assert_eq!(progress.checked, 3);
    assert_eq!(progress.downloaded, 1);
    assert_eq!(progress.unavailable, 1);
    assert_eq!(progress.fraction(), 1.0);
}

#[test]
fn cover_completion_starts_lyrics_only_after_the_subscription_is_armed() {
    assert!(!cover_batch_finished(false, CoverBatchState::Complete));
    assert!(!cover_batch_finished(true, CoverBatchState::Running));
    assert!(cover_batch_finished(true, CoverBatchState::Idle));
    assert!(cover_batch_finished(true, CoverBatchState::Complete));
    assert!(cover_batch_finished(true, CoverBatchState::Failed));
}

#[test]
fn every_open_network_breaker_fails_before_a_provider_call() {
    let calls = Arc::new(Mutex::new(0));
    let (request, receiver) = request(vec![track("One")]);
    let mut services = services({
        let calls = calls.clone();
        move |_, _| {
            *calls.lock().unwrap() += 1;
            Err(LyricsError::Temporary)
        }
    });
    services.all_breakers_open = Arc::new(|| true);

    run_request(&request, &services);

    assert_eq!(*calls.lock().unwrap(), 0);
    assert_eq!(
        progress_events(&receiver).last().unwrap().state,
        LyricsBatchState::Failed
    );
}

#[test]
fn cancellation_keeps_the_first_completed_lookup_and_never_starts_the_second() {
    let written = Arc::new(Mutex::new(Vec::new()));
    let (request, receiver) = request(vec![track("One"), track("Two")]);
    let cancellation = request.cancellation.clone();
    let services = services({
        let written = written.clone();
        move |query, _| {
            written.lock().unwrap().push(query.title.clone());
            cancellation.request();
            Ok(LyricsHit {
                body: LyricsBody::Plain("cached".into()),
                source: LyricsSource::Lrclib,
            })
        }
    });

    run_request(&request, &services);

    assert_eq!(&*written.lock().unwrap(), &["One"]);
    assert!(matches!(
        receiver.try_recv(),
        Ok(WorkerEvent::Progress(LyricsBatchProgress {
            checked: 1,
            downloaded: 1,
            ..
        }))
    ));
    assert!(matches!(receiver.try_recv(), Ok(WorkerEvent::Cancelled)));
}

#[test]
fn local_and_cache_hits_skip_network_but_still_advance_progress() {
    let calls = Arc::new(Mutex::new(0));
    let (request, receiver) = request(vec![track("Local"), track("Cached")]);
    let mut services = services({
        let calls = calls.clone();
        move |_, _| {
            *calls.lock().unwrap() += 1;
            Err(LyricsError::Temporary)
        }
    });
    services.local = Arc::new(|path| path.ends_with("Local.flac"));
    services.needs = Arc::new(|query| {
        if query.title == "Cached" {
            NeedsFetch::Skip
        } else {
            NeedsFetch::Fetch
        }
    });

    run_request(&request, &services);

    assert_eq!(*calls.lock().unwrap(), 0);
    let progress = progress_events(&receiver).pop().unwrap();
    assert_eq!(progress.state, LyricsBatchState::Complete);
    assert_eq!(progress.checked, 2);
    assert_eq!(progress.downloaded, 0);
}
