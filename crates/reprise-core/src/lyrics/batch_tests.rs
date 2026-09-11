use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::*;
use crate::lyrics::{cache, LookupOptions, LyricsProvider, LyricsSource, SourceOutcome, TimedLine};

struct ThreadSafeProvider {
    source: LyricsSource,
    outcome: SourceOutcome,
    calls: AtomicUsize,
}

impl ThreadSafeProvider {
    fn new(source: LyricsSource, outcome: SourceOutcome) -> Self {
        Self {
            source,
            outcome,
            calls: AtomicUsize::new(0),
        }
    }
}

impl LyricsProvider for ThreadSafeProvider {
    fn source(&self) -> LyricsSource {
        self.source
    }

    fn lookup(&self, _query: &LyricsQuery, _track_path: Option<&Path>) -> SourceOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.outcome.clone()
    }
}

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

fn services(
    online: impl Fn(&LyricsQuery, &Path) -> Result<LyricsHit, LyricsError> + Send + Sync + 'static,
) -> BatchServices<'static> {
    BatchServices {
        local: Arc::new(|_| false),
        needs: Arc::new(|query| cache_decision(query, NeedsFetch::Fetch)),
        online: Arc::new(move |query, path, _| online(query, path)),
        all_breakers_open: Arc::new(|| false),
    }
}

fn cache_decision(query: &LyricsQuery, expected: NeedsFetch) -> CacheDecision {
    let temp = tempfile::tempdir().unwrap();
    match expected {
        NeedsFetch::Fetch => {}
        NeedsFetch::Skip => cache::write_not_found(temp.path(), 100, query),
        NeedsFetch::RetryForSynced => cache::write_found(
            temp.path(),
            100,
            query,
            &LyricsHit {
                body: LyricsBody::Plain("cached".into()),
                source: LyricsSource::Lrclib,
            },
            false,
        ),
    }
    let decision = cache::decision_at(temp.path(), 101, query);
    assert_eq!(decision.classification(), expected);
    decision
}

fn run(
    tracks: &[BatchTrack],
    services: &BatchServices<'_>,
    is_cancelled: impl Fn() -> bool,
    network_allowed: impl Fn() -> bool,
) -> (BatchRunStatus, Vec<BatchProgress>) {
    let mut progress = Vec::new();
    let status = run_batch_with_services(
        tracks,
        services,
        is_cancelled,
        network_allowed,
        |snapshot| {
            progress.push(snapshot);
            true
        },
    );
    (status, progress)
}

#[test]
fn progress_counts_checked_downloaded_and_unavailable_without_counting_skips() {
    let progress = BatchProgress::running(3)
        .advance(BatchItemOutcome::Skipped)
        .advance(BatchItemOutcome::Downloaded)
        .advance(BatchItemOutcome::Unavailable);

    assert_eq!(progress.state, BatchState::Complete);
    assert_eq!(progress.checked, 3);
    assert_eq!(progress.downloaded, 1);
    assert_eq!(progress.unavailable, 1);
    assert_eq!(progress.failed, 0);
    assert_eq!(progress.fraction(), 1.0);
}

#[test]
fn transient_failures_remain_visible_after_the_batch_reaches_the_end() {
    let progress = BatchProgress::running(1).advance(BatchItemOutcome::Failed);

    assert_eq!(progress.state, BatchState::Complete);
    assert_eq!(progress.failed, 1);
}

#[test]
fn every_open_network_breaker_fails_before_a_provider_call() {
    let calls = Arc::new(Mutex::new(0));
    let mut services = services({
        let calls = calls.clone();
        move |_, _| {
            *calls.lock().unwrap() += 1;
            Err(LyricsError::Temporary)
        }
    });
    services.all_breakers_open = Arc::new(|| true);

    let (_, progress) = run(&[track("One")], &services, || false, || true);

    assert_eq!(*calls.lock().unwrap(), 0);
    assert_eq!(progress.last().unwrap().state, BatchState::Failed);
}

#[test]
fn cancellation_keeps_the_first_completed_lookup_and_never_starts_the_second() {
    let written = Arc::new(Mutex::new(Vec::new()));
    let cancelled = Arc::new(AtomicBool::new(false));
    let services = services({
        let written = written.clone();
        let cancelled = cancelled.clone();
        move |query, _| {
            written.lock().unwrap().push(query.title.clone());
            cancelled.store(true, Ordering::Relaxed);
            Ok(LyricsHit {
                body: LyricsBody::Plain("cached".into()),
                source: LyricsSource::Lrclib,
            })
        }
    });

    let (status, progress) = run(
        &[track("One"), track("Two")],
        &services,
        || cancelled.load(Ordering::Relaxed),
        || true,
    );

    assert_eq!(&*written.lock().unwrap(), &["One"]);
    assert_eq!(status, BatchRunStatus::Cancelled);
    assert_eq!(progress.len(), 1);
    assert_eq!(progress[0].checked, 1);
    assert_eq!(progress[0].downloaded, 1);
}

#[test]
fn net_1a_switching_the_module_off_mid_run_stops_before_the_next_request() {
    let enabled = Arc::new(AtomicBool::new(true));
    let calls = Arc::new(Mutex::new(0));
    let services = services({
        let calls = calls.clone();
        let enabled = enabled.clone();
        move |_, _| {
            *calls.lock().unwrap() += 1;
            enabled.store(false, Ordering::Relaxed);
            Ok(LyricsHit {
                body: LyricsBody::Synced(Vec::new()),
                source: LyricsSource::Lrclib,
            })
        }
    });

    let (status, progress) = run(
        &[track("One"), track("Two")],
        &services,
        || false,
        || enabled.load(Ordering::Relaxed),
    );

    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(status, BatchRunStatus::Cancelled);
    assert_eq!(progress.len(), 1);
    assert_eq!(progress[0].checked, 1);
}

#[test]
fn a_synced_retry_only_counts_when_it_actually_improves_the_cached_result() {
    for (body, expected) in [
        (LyricsBody::Plain("already cached".into()), 0),
        (LyricsBody::Synced(vec![]), 1),
    ] {
        let mut services = services(move |_, _| {
            Ok(LyricsHit {
                body: body.clone(),
                source: LyricsSource::Lrclib,
            })
        });
        services.needs = Arc::new(|query| cache_decision(query, NeedsFetch::RetryForSynced));

        let (_, progress) = run(&[track("One")], &services, || false, || true);

        assert_eq!(progress[0].checked, 1);
        assert_eq!(progress[0].downloaded, expected);
    }
}

#[test]
fn batch_passes_its_precomputed_classification_to_the_lookup() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut services = services(|_, _| Err(LyricsError::Temporary));
    services.needs = Arc::new(|query| cache_decision(query, NeedsFetch::RetryForSynced));
    services.online = Arc::new({
        let seen = seen.clone();
        move |_, _, decision| {
            seen.lock().unwrap().push(decision.classification());
            Ok(LyricsHit {
                body: LyricsBody::Plain("already cached".into()),
                source: LyricsSource::Lrclib,
            })
        }
    });

    let _ = run(&[track("One")], &services, || false, || true);

    assert_eq!(&*seen.lock().unwrap(), &[NeedsFetch::RetryForSynced]);
}

#[test]
fn lyr_6_local_and_cache_hits_skip_network_but_still_advance_progress() {
    let calls = Arc::new(Mutex::new(0));
    let mut services = services({
        let calls = calls.clone();
        move |_, _| {
            *calls.lock().unwrap() += 1;
            Err(LyricsError::Temporary)
        }
    });
    services.local = Arc::new(|path| path.ends_with("Local.flac"));
    services.needs = Arc::new(|query| {
        cache_decision(
            query,
            if query.title == "Cached" {
                NeedsFetch::Skip
            } else {
                NeedsFetch::Fetch
            },
        )
    });

    let (_, progress) = run(
        &[track("Local"), track("Cached")],
        &services,
        || false,
        || true,
    );

    assert_eq!(*calls.lock().unwrap(), 0);
    let progress = progress.last().unwrap();
    assert_eq!(progress.state, BatchState::Complete);
    assert_eq!(progress.checked, 2);
    assert_eq!(progress.downloaded, 0);
}

#[test]
fn lyr_6_a_plain_sidecar_reaches_the_online_lookup() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Plain.flac");
    std::fs::write(&path, b"fixture").unwrap();
    std::fs::write(path.with_extension("lrc"), "plain sidecar text").unwrap();
    let track = BatchTrack {
        query: track("Plain").query,
        path,
    };
    let calls = Arc::new(Mutex::new(0));
    let mut services = BatchServices::production(&crate::library::source::UnixLibrarySource);
    services.needs = Arc::new(|query| cache_decision(query, NeedsFetch::RetryForSynced));
    services.online = Arc::new({
        let calls = calls.clone();
        move |_, _, _| {
            *calls.lock().unwrap() += 1;
            Ok(LyricsHit {
                body: LyricsBody::Plain("plain sidecar text".into()),
                source: LyricsSource::Sidecar,
            })
        }
    });

    let (_, progress) = run(&[track], &services, || false, || true);

    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(progress.last().unwrap().checked, 1);
}

#[test]
fn lyr_6_a_synced_sidecar_still_skips_the_online_lookup() {
    // Control arm: this passed before plain sidecars became upgrade candidates.
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Synced.flac");
    std::fs::write(&path, b"fixture").unwrap();
    std::fs::write(path.with_extension("lrc"), "[00:01.00]synced line").unwrap();
    let track = BatchTrack {
        query: track("Synced").query,
        path,
    };
    let calls = Arc::new(Mutex::new(0));
    let mut services = BatchServices::production(&crate::library::source::UnixLibrarySource);
    services.needs = Arc::new(|query| cache_decision(query, NeedsFetch::RetryForSynced));
    services.online = Arc::new({
        let calls = calls.clone();
        move |_, _, _| {
            *calls.lock().unwrap() += 1;
            Err(LyricsError::Temporary)
        }
    });

    let (_, progress) = run(&[track], &services, || false, || true);

    assert_eq!(*calls.lock().unwrap(), 0);
    assert_eq!(progress.last().unwrap().checked, 1);
}

#[test]
fn lyr_6_only_synced_and_instrumental_local_hits_are_complete() {
    // Control arm: this body-only matrix passed before the production guard changed.
    for (body, expected) in [
        (LyricsBody::Plain("plain".into()), false),
        (LyricsBody::Synced(Vec::new()), true),
        (LyricsBody::Instrumental, true),
    ] {
        assert_eq!(
            local_hit_is_complete(&LyricsHit {
                body,
                source: LyricsSource::Sidecar,
            }),
            expected
        );
    }
}

#[test]
fn lyr_6_plain_tag_hit_is_complete() {
    assert!(local_hit_is_complete(&LyricsHit {
        body: LyricsBody::Plain("embedded lyrics".into()),
        source: LyricsSource::Tag,
    }));
}

#[test]
fn lyr_6_a_stamped_plain_sidecar_is_skipped_on_the_second_batch_run() {
    let temp = tempfile::tempdir().unwrap();
    let cache_dir = temp.path().join("cache");
    let path = temp.path().join("Stamped.flac");
    std::fs::write(&path, b"fixture").unwrap();
    std::fs::write(path.with_extension("lrc"), "plain sidecar text").unwrap();
    let track = BatchTrack {
        query: track("Stamped").query,
        path,
    };
    let source = &crate::library::source::UnixLibrarySource;
    let answered = Arc::new(ThreadSafeProvider::new(
        LyricsSource::Lrclib,
        SourceOutcome::NotFound,
    ));
    let failed = Arc::new(ThreadSafeProvider::new(
        LyricsSource::Netease,
        SourceOutcome::Failed,
    ));
    let mut services = BatchServices::production(source);
    services.needs = Arc::new({
        let cache_dir = cache_dir.clone();
        move |query| cache::decision_at(&cache_dir, 100, query)
    });
    services.online = Arc::new({
        let cache_dir = cache_dir.clone();
        let answered = answered.clone();
        let failed = failed.clone();
        move |query, path, decision| {
            let local = crate::lyrics::LocalProvider { source };
            crate::lyrics::load_or_fetch_with_cache_context_at(
                &cache_dir,
                100,
                query,
                Some(path),
                LookupOptions::default(),
                Some(decision),
                crate::lyrics::LookupProviders {
                    source,
                    local: &[&local],
                    network: &[answered.as_ref(), failed.as_ref()],
                },
            )
        }
    });

    let _ = run(std::slice::from_ref(&track), &services, || false, || true);
    let (_, second_progress) = run(&[track], &services, || false, || true);

    assert_eq!(answered.calls.load(Ordering::SeqCst), 1);
    assert_eq!(failed.calls.load(Ordering::SeqCst), 1);
    assert_eq!(second_progress.last().unwrap().downloaded, 0);
}

#[test]
fn lyr_7_the_batch_runs_the_sidecar_writing_lookup_for_the_whole_library() {
    let temp = tempfile::tempdir().unwrap();
    let tracks = ["One", "Two"]
        .into_iter()
        .map(|title| {
            let path = temp.path().join(format!("{title}.flac"));
            std::fs::write(&path, b"fixture").unwrap();
            BatchTrack {
                query: LyricsQuery {
                    title: title.into(),
                    artist: "Synthetic Artist".into(),
                    album: "Synthetic Album".into(),
                    duration_ms: 10_000,
                },
                path,
            }
        })
        .collect::<Vec<_>>();
    let services = services(|query, path| {
        std::fs::write(
            path.with_extension("lrc"),
            format!("[00:01.00]{} lyrics\n", query.title),
        )
        .unwrap();
        Ok(LyricsHit {
            body: LyricsBody::Synced(vec![TimedLine::new(
                1_000,
                format!("{} lyrics", query.title),
            )]),
            source: LyricsSource::Lrclib,
        })
    });

    let (_, progress) = run(&tracks, &services, || false, || true);

    for title in ["One", "Two"] {
        assert_eq!(
            std::fs::read_to_string(temp.path().join(format!("{title}.lrc"))).unwrap(),
            format!("[00:01.00]{title} lyrics\n")
        );
    }
    let progress = progress.last().unwrap();
    assert_eq!(progress.state, BatchState::Complete);
    assert_eq!(progress.checked, 2);
    assert_eq!(progress.downloaded, 2);
}
