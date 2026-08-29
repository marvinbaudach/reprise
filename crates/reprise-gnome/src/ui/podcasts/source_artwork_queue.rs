//! Unbounded, coalescing source-artwork worker queue.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use gtk4::prelude::*;
use reprise_core::remote_image::{CacheScope, ImageOutcome};

use super::source_artwork_measurement;
use super::{decode_pixels, DecodedPixels};

const ARTWORK_WORKERS: usize = 8;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct ArtworkKey {
    url: String,
}

struct ArtworkWaiter {
    width: i32,
    height: i32,
    cache_scope: CacheScope,
    response: async_channel::Sender<Option<DecodedPixels>>,
    measurement: Option<RequestMeasurement>,
}

#[derive(Default)]
struct PendingJob {
    waiters: Vec<ArtworkWaiter>,
    started: bool,
}

type Pending = Arc<Mutex<HashMap<ArtworkKey, PendingJob>>>;

#[derive(Clone)]
pub(super) struct ArtworkQueue {
    sender: async_channel::Sender<ArtworkKey>,
    pending: Pending,
    measurement: Option<Arc<MeasurementState>>,
}

struct MeasurementState {
    queued_jobs: AtomicUsize,
    next_request_id: AtomicU64,
}

impl MeasurementState {
    fn new() -> Self {
        Self {
            queued_jobs: AtomicUsize::new(0),
            next_request_id: AtomicU64::new(1),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RegistrationContext {
    pub(super) row_id: u64,
    pub(super) visible: bool,
    pub(super) startup_gate_open_at_request: bool,
}

pub(super) fn registration_context(
    row_id: u64,
    widget: &gtk4::glib::WeakRef<gtk4::Widget>,
    retained_is_startup_visible: bool,
    startup_gate_open_at_request: bool,
) -> RegistrationContext {
    if !measurement_enabled() {
        return RegistrationContext {
            row_id: 0,
            visible: false,
            startup_gate_open_at_request,
        };
    }
    let visible = widget.upgrade().is_some_and(|widget| {
        visible_in_viewport(&widget) || (retained_is_startup_visible && !widget.is_mapped())
    });
    RegistrationContext {
        row_id,
        visible,
        startup_gate_open_at_request,
    }
}

fn visible_in_viewport(widget: &gtk4::Widget) -> bool {
    let mut ancestor = Some(widget.clone());
    while let Some(current) = ancestor {
        if !current.is_visible() || !current.is_child_visible() {
            return false;
        }
        ancestor = current.parent();
    }
    let Some(root) = widget.root() else {
        return false;
    };
    let root_widget: &gtk4::Widget = root.upcast_ref();
    widget.compute_bounds(root_widget).is_some_and(|bounds| {
        bounds.x() + bounds.width() > 0.0
            && bounds.y() + bounds.height() > 0.0
            && bounds.x() < root_widget.width() as f32
            && bounds.y() < root_widget.height() as f32
    })
}

#[derive(Clone, Copy, Debug)]
struct RequestMeasurement {
    request_id: u64,
    row_id: u64,
    visible: bool,
    startup_gate_open_at_request: bool,
    jobs_ahead: usize,
    queued_at: Instant,
}

pub(super) struct ArtworkResponse {
    receiver: async_channel::Receiver<Option<DecodedPixels>>,
    measurement: Option<RequestMeasurement>,
}

impl ArtworkResponse {
    pub(super) async fn recv(&self) -> Result<Option<DecodedPixels>, async_channel::RecvError> {
        let result = self.receiver.recv().await;
        record_measurement("gtk_return", self.measurement);
        result
    }

    #[cfg(test)]
    fn measurement(&self) -> Option<RequestMeasurement> {
        self.measurement
    }

    #[cfg(test)]
    fn try_recv(&self) -> Result<Option<DecodedPixels>, async_channel::TryRecvError> {
        self.receiver.try_recv()
    }

    #[cfg(test)]
    fn recv_blocking(&self) -> Result<Option<DecodedPixels>, async_channel::RecvError> {
        self.receiver.recv_blocking()
    }
}

impl ArtworkQueue {
    fn start() -> Self {
        let (sender, receiver) = async_channel::unbounded();
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let measurement = measurement_enabled().then(|| Arc::new(MeasurementState::new()));
        for index in 0..ARTWORK_WORKERS {
            let receiver = receiver.clone();
            let pending = pending.clone();
            let measurement = measurement.clone();
            if let Err(error) = std::thread::Builder::new()
                .name(format!("reprise-source-artwork-{index}"))
                .spawn(move || {
                    run_worker_with_depth(
                        &receiver,
                        &pending,
                        measurement.as_deref(),
                        &mut |url| {
                            reprise_core::podcasts::source_artwork::fetch(url)
                                .map_err(|error| error.to_string())
                        },
                    );
                })
            {
                tracing::warn!(%error, "could not start source artwork worker");
            }
        }
        Self {
            sender,
            pending,
            measurement,
        }
    }

    #[cfg(test)]
    pub(super) fn submit(
        &self,
        url: String,
        width: i32,
        height: i32,
        cache_scope: CacheScope,
    ) -> ArtworkResponse {
        self.submit_measured(url, width, height, cache_scope, None)
    }

    pub(super) fn submit_measured(
        &self,
        url: String,
        width: i32,
        height: i32,
        cache_scope: CacheScope,
        context: Option<RegistrationContext>,
    ) -> ArtworkResponse {
        let key = ArtworkKey { url };
        let (response, receiver) = async_channel::bounded(1);
        let measurement = self.measurement.as_deref().map(|state| {
            let context = context.unwrap_or(RegistrationContext {
                row_id: 0,
                visible: false,
                startup_gate_open_at_request: false,
            });
            RequestMeasurement {
                request_id: state.next_request_id.fetch_add(1, Ordering::Relaxed),
                row_id: context.row_id,
                visible: context.visible,
                startup_gate_open_at_request: context.startup_gate_open_at_request,
                jobs_ahead: state.queued_jobs.load(Ordering::Relaxed),
                queued_at: Instant::now(),
            }
        });
        let waiter = ArtworkWaiter {
            width,
            height,
            cache_scope,
            response,
            measurement,
        };
        let (is_new_job, joined_started_job) = {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match pending.get_mut(&key) {
                Some(job) => {
                    let started = measurement.is_some() && job.started;
                    job.waiters.push(waiter);
                    (false, started)
                }
                None => {
                    pending.insert(
                        key.clone(),
                        PendingJob {
                            waiters: vec![waiter],
                            started: false,
                        },
                    );
                    (true, false)
                }
            }
        };
        record_measurement("queued", measurement);
        if joined_started_job {
            record_measurement("worker_start", measurement);
        }
        if is_new_job {
            if let Some(state) = &self.measurement {
                state.queued_jobs.fetch_add(1, Ordering::Relaxed);
            }
        }
        if is_new_job && self.sender.try_send(key.clone()).is_err() {
            if let Some(state) = &self.measurement {
                state.queued_jobs.fetch_sub(1, Ordering::Relaxed);
            }
            finish_without_image(&self.pending, &key);
        }
        ArtworkResponse {
            receiver,
            measurement,
        }
    }

    #[cfg(test)]
    fn test_queue() -> (Self, async_channel::Receiver<ArtworkKey>) {
        Self::test_queue_with_measurement(true)
    }

    #[cfg(test)]
    fn test_queue_without_measurement() -> (Self, async_channel::Receiver<ArtworkKey>) {
        Self::test_queue_with_measurement(false)
    }

    #[cfg(test)]
    fn test_queue_with_measurement(enabled: bool) -> (Self, async_channel::Receiver<ArtworkKey>) {
        let (sender, receiver) = async_channel::unbounded();
        (
            Self {
                sender,
                pending: Arc::new(Mutex::new(HashMap::new())),
                measurement: enabled.then(|| Arc::new(MeasurementState::new())),
            },
            receiver,
        )
    }
}

pub(super) fn queue(
    url: String,
    width: i32,
    height: i32,
    cache_scope: CacheScope,
    context: Option<RegistrationContext>,
) -> ArtworkResponse {
    #[cfg(test)]
    source_artwork_measurement::record_registration_for_test();
    static QUEUE: OnceLock<ArtworkQueue> = OnceLock::new();
    QUEUE
        .get_or_init(ArtworkQueue::start)
        .submit_measured(url, width, height, cache_scope, context)
}

pub(super) fn measurement_enabled() -> bool {
    source_artwork_measurement::enabled()
}

fn record_measurement(phase: &str, measurement: Option<RequestMeasurement>) {
    let Some(measurement) = measurement else {
        return;
    };
    eprintln!(
        "source-artwork-measure phase={phase} request={} row={} visible={} startup_gate_open_at_request={} jobs_ahead={} wait_us={}",
        measurement.request_id,
        measurement.row_id,
        measurement.visible,
        measurement.startup_gate_open_at_request,
        measurement.jobs_ahead,
        measurement.queued_at.elapsed().as_micros()
    );
}

/// Resolves one URL against the gate state at fetch time, then fans the result
/// out to every waiter that joined while the job was queued or running.
fn process_job(
    pending: &Pending,
    key: &ArtworkKey,
    fetch: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) {
    let requested_scopes = requested_scopes(pending, key);
    let cached = requested_scopes.into_iter().find_map(|scope| {
        let mut must_not_fetch = |_: &str| Err("cache-only lookup must not fetch".to_string());
        match reprise_core::remote_image::resolve(Some(&key.url), scope, false, &mut must_not_fetch)
        {
            ImageOutcome::Cached(path) => Some((scope, path)),
            ImageOutcome::Fetched(_)
            | ImageOutcome::NotAllowed
            | ImageOutcome::NoUrl
            | ImageOutcome::FetchFailed => None,
        }
    });
    let resolve_scope = cached
        .as_ref()
        .map_or_else(|| preferred_scope(pending, key), |(scope, _)| *scope);
    let allowed = super::GATE_OPEN.load(std::sync::atomic::Ordering::Relaxed);
    let path = match cached {
        Some((_, path)) => Some(path),
        None => {
            match reprise_core::remote_image::resolve(Some(&key.url), resolve_scope, allowed, fetch)
            {
                ImageOutcome::Cached(path) | ImageOutcome::Fetched(path) => Some(path),
                ImageOutcome::NotAllowed | ImageOutcome::NoUrl | ImageOutcome::FetchFailed => None,
            }
        }
    };

    loop {
        let waiters = {
            let mut pending = pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(job) = pending.get_mut(key) else {
                return;
            };
            std::mem::take(&mut job.waiters)
        };
        for waiter in waiters {
            let waiter_path = path.as_ref().and_then(|path| {
                if waiter.cache_scope == resolve_scope {
                    Some(path.clone())
                } else {
                    reprise_core::remote_image::cache_existing_file(
                        &key.url,
                        path,
                        waiter.cache_scope,
                    )
                    .or_else(|| Some(path.clone()))
                }
            });
            let pixels = waiter_path.as_ref().and_then(|waiter_path| {
                decode_pixels(waiter_path, waiter.width, waiter.height)
                    .map_err(|error| {
                        tracing::debug!(
                            %error,
                            url = %key.url,
                            path = %waiter_path.display(),
                            "source artwork could not be decoded"
                        );
                    })
                    .ok()
            });
            let _ = waiter.response.send_blocking(pixels);
        }

        let mut pending = pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if pending.get(key).is_some_and(|job| job.waiters.is_empty()) {
            pending.remove(key);
            return;
        }
    }
}

#[cfg(test)]
fn run_worker(
    receiver: &async_channel::Receiver<ArtworkKey>,
    pending: &Pending,
    fetch: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) {
    run_worker_with_depth(receiver, pending, None, fetch);
}

fn run_worker_with_depth(
    receiver: &async_channel::Receiver<ArtworkKey>,
    pending: &Pending,
    measurement: Option<&MeasurementState>,
    fetch: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) {
    while let Ok(job) = receiver.recv_blocking() {
        if let Some(measurement) = measurement {
            let _ = measurement.queued_jobs.fetch_update(
                Ordering::Relaxed,
                Ordering::Relaxed,
                |depth| depth.checked_sub(1),
            );
            let measurements = {
                let mut pending = pending
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                pending.get_mut(&job).map(|job| {
                    job.started = true;
                    job.waiters
                        .iter()
                        .filter_map(|waiter| waiter.measurement)
                        .collect::<Vec<_>>()
                })
            }
            .unwrap_or_default();
            for measurement in measurements {
                record_measurement("worker_start", Some(measurement));
            }
        }
        // UNWIND ASSUMPTION: `fetch` is the stateless wrapper around the free fetch
        // function. This reused `FnMut` must never retain partially mutated state after a panic.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            process_job(pending, &job, fetch);
        }));
        if result.is_err() {
            tracing::warn!(url = %job.url, "source artwork worker recovered from a panicking job");
            finish_without_image(pending, &job);
        }
    }
}

fn requested_scopes(pending: &Pending, key: &ArtworkKey) -> Vec<CacheScope> {
    let pending = pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(waiters) = pending.get(key) else {
        return vec![CacheScope::Persistent];
    };
    let persistent = waiters
        .waiters
        .iter()
        .any(|waiter| waiter.cache_scope == CacheScope::Persistent);
    let transient = waiters
        .waiters
        .iter()
        .any(|waiter| waiter.cache_scope == CacheScope::Transient);
    match (persistent, transient) {
        (true, true) => vec![CacheScope::Persistent, CacheScope::Transient],
        (true, false) => vec![CacheScope::Persistent],
        (false, true) => vec![CacheScope::Transient],
        (false, false) => vec![CacheScope::Persistent],
    }
}

fn preferred_scope(pending: &Pending, key: &ArtworkKey) -> CacheScope {
    requested_scopes(pending, key)
        .into_iter()
        .next()
        .unwrap_or(CacheScope::Persistent)
}

fn finish_without_image(pending: &Pending, key: &ArtworkKey) {
    let waiters = pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(key)
        .unwrap_or_default()
        .waiters;
    for waiter in waiters {
        let _ = waiter.response.send_blocking(None);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use reprise_core::remote_image::CacheScope;

    const TINY_PNG: [u8; 69] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xA8,
        0xAF, 0xAF, 0x07, 0x00, 0x02, 0xFE, 0x01, 0x7E, 0xBA, 0x25, 0x70, 0x25, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn unique_url(label: &str) -> String {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("https://images.test/{label}-{nonce}.png")
    }

    #[test]
    fn startup_measurement_keeps_row_visibility_and_queue_position_with_each_request() {
        let (queue, _jobs) = super::ArtworkQueue::test_queue();
        let first = queue.submit_measured(
            unique_url("measurement"),
            40,
            40,
            CacheScope::Persistent,
            Some(super::RegistrationContext {
                row_id: 17,
                visible: true,
                startup_gate_open_at_request: false,
            }),
        );

        let second = queue.submit_measured(
            unique_url("measurement-second"),
            40,
            40,
            CacheScope::Persistent,
            Some(super::RegistrationContext {
                row_id: 18,
                visible: false,
                startup_gate_open_at_request: true,
            }),
        );

        let first_measurement = first.measurement().expect("measurement is enabled");
        let second_measurement = second.measurement().expect("measurement is enabled");
        assert_eq!(first_measurement.row_id, 17);
        assert!(first_measurement.visible);
        assert!(!first_measurement.startup_gate_open_at_request);
        assert_eq!(first_measurement.jobs_ahead, 0);
        assert!(second_measurement.startup_gate_open_at_request);
        assert_eq!(second_measurement.jobs_ahead, 1);
    }

    #[test]
    fn disabled_startup_measurement_carries_no_request_metadata() {
        let (queue, _jobs) = super::ArtworkQueue::test_queue_without_measurement();

        let response = queue.submit(
            unique_url("measurement-disabled"),
            40,
            40,
            CacheScope::Persistent,
        );

        assert!(response.measurement().is_none());
    }

    #[test]
    fn src_11_many_concurrent_requests_all_receive_a_result() {
        let _gate = super::super::GATE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        super::super::GATE_OPEN.store(true, std::sync::atomic::Ordering::SeqCst);
        let (queue, jobs) = super::ArtworkQueue::test_queue();
        let results = Arc::new(Mutex::new(Vec::new()));
        let prefix = Arc::new(unique_url("burst"));

        std::thread::scope(|scope| {
            for worker in 0..16 {
                let queue = queue.clone();
                let results = results.clone();
                let prefix = prefix.clone();
                scope.spawn(move || {
                    for item in 0..10 {
                        let url = format!("{prefix}-{worker}-{item}");
                        let result = queue.submit(url, 40, 40, CacheScope::Persistent);
                        results.lock().unwrap().push(result);
                    }
                });
            }
        });

        assert_eq!(jobs.len(), 160);
        while let Ok(job) = jobs.try_recv() {
            super::process_job(&queue.pending, &job, &mut |_| Ok(TINY_PNG.to_vec()));
        }
        let results = results.lock().unwrap();
        assert_eq!(results.len(), 160);
        assert!(results
            .iter()
            .all(|result| matches!(result.try_recv(), Ok(Some(_)))));
    }

    #[test]
    fn src_11_duplicate_url_across_scopes_fetches_once_and_answers_every_waiter() {
        let _gate = super::super::GATE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        super::super::GATE_OPEN.store(true, std::sync::atomic::Ordering::SeqCst);
        let (queue, jobs) = super::ArtworkQueue::test_queue();
        let url = unique_url("shared");
        let first = queue.submit(url.clone(), 40, 40, CacheScope::Persistent);
        assert_eq!(jobs.len(), 1, "an in-flight URL has one queued job");

        let mut fetches = 0;
        let mut second = None;
        let job = jobs.try_recv().unwrap();
        super::process_job(&queue.pending, &job, &mut |_| {
            fetches += 1;
            second = Some(queue.submit(url.clone(), 80, 80, CacheScope::Transient));
            Ok(TINY_PNG.to_vec())
        });

        assert_eq!(fetches, 1);
        assert!(
            jobs.is_empty(),
            "the in-flight waiter must not enqueue again"
        );
        let first = first.try_recv().unwrap().unwrap();
        let second = second.unwrap().try_recv().unwrap().unwrap();
        assert_eq!((first.width, first.height), (80, 80));
        assert_eq!((second.width, second.height), (160, 160));
        for cache_scope in [CacheScope::Persistent, CacheScope::Transient] {
            let outcome =
                reprise_core::remote_image::resolve(Some(&url), cache_scope, false, &mut |_| {
                    panic!("both stores must be populated without another fetch")
                });
            let reprise_core::remote_image::ImageOutcome::Cached(path) = outcome else {
                panic!("expected a cached file in {cache_scope:?}, got {outcome:?}");
            };
            std::fs::remove_file(path).ok();
        }
    }

    #[test]
    fn src_11_secondary_scope_cache_hit_is_shown_with_the_gate_closed() {
        let _gate = super::super::GATE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (queue, jobs) = super::ArtworkQueue::test_queue();
        let url = unique_url("cross-scope-cached");
        let cached = reprise_core::remote_image::resolve(
            Some(&url),
            CacheScope::Transient,
            true,
            &mut |_| Ok(TINY_PNG.to_vec()),
        );
        assert!(matches!(
            cached,
            reprise_core::remote_image::ImageOutcome::Fetched(_)
        ));

        let persistent = queue.submit(url.clone(), 40, 40, CacheScope::Persistent);
        let transient = queue.submit(url.clone(), 40, 40, CacheScope::Transient);
        super::super::GATE_OPEN.store(false, std::sync::atomic::Ordering::SeqCst);
        let mut fetch_called = false;
        let job = jobs.try_recv().unwrap();
        super::process_job(&queue.pending, &job, &mut |_| {
            fetch_called = true;
            Err("must not fetch".into())
        });

        assert!(!fetch_called, "a cache hit must remain gate-independent");
        assert!(matches!(persistent.try_recv(), Ok(Some(_))));
        assert!(matches!(transient.try_recv(), Ok(Some(_))));
        for cache_scope in [CacheScope::Persistent, CacheScope::Transient] {
            let outcome =
                reprise_core::remote_image::resolve(Some(&url), cache_scope, false, &mut |_| {
                    panic!("the closed gate must prevent another fetch")
                });
            let reprise_core::remote_image::ImageOutcome::Cached(path) = outcome else {
                panic!("expected a cached file in {cache_scope:?}, got {outcome:?}");
            };
            std::fs::remove_file(path).ok();
        }
    }

    #[test]
    fn src_11_gate_closed_after_enqueue_still_prevents_the_fetch() {
        use std::sync::atomic::Ordering;

        let _gate = super::super::GATE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (queue, jobs) = super::ArtworkQueue::test_queue();
        super::super::GATE_OPEN.store(true, Ordering::SeqCst);
        let result = queue.submit(
            unique_url("gate-closes-while-waiting"),
            40,
            40,
            CacheScope::Persistent,
        );
        super::super::GATE_OPEN.store(false, Ordering::SeqCst);

        let mut fetch_called = false;
        let job = jobs.try_recv().unwrap();
        super::process_job(&queue.pending, &job, &mut |_| {
            fetch_called = true;
            Err("must not fetch".into())
        });

        assert!(!fetch_called);
        assert!(matches!(result.try_recv(), Ok(None)));
    }

    #[test]
    fn src_11_panicking_job_finishes_and_the_worker_accepts_the_same_url_again() {
        let _gate = super::super::GATE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        super::super::GATE_OPEN.store(true, std::sync::atomic::Ordering::SeqCst);
        let (queue, jobs) = super::ArtworkQueue::test_queue();
        let pending = queue.pending.clone();
        let worker = std::thread::spawn(move || {
            let mut attempts = 0;
            super::run_worker(&jobs, &pending, &mut |_| {
                attempts += 1;
                if attempts == 1 {
                    panic!("simulated artwork fetch panic");
                }
                Ok(TINY_PNG.to_vec())
            });
        });
        let url = unique_url("panic-recovery");

        let failed = queue.submit(url.clone(), 40, 40, CacheScope::Transient);
        assert!(matches!(failed.recv_blocking(), Ok(None)));

        let retried = queue.submit(url, 40, 40, CacheScope::Transient);
        assert!(matches!(retried.recv_blocking(), Ok(Some(_))));

        drop(queue);
        worker.join().unwrap();
    }

    #[test]
    fn src_11_panicking_decode_finishes_and_the_worker_accepts_the_same_url_again() {
        let _gate = super::super::GATE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        super::super::GATE_OPEN.store(true, std::sync::atomic::Ordering::SeqCst);
        let (queue, jobs) = super::ArtworkQueue::test_queue();
        let pending = queue.pending.clone();
        let worker = std::thread::spawn(move || {
            super::run_worker(&jobs, &pending, &mut |_| Ok(TINY_PNG.to_vec()));
        });
        let url = unique_url("decode-panic-recovery");
        let key = super::ArtworkKey { url: url.clone() };

        // A zero-width decode makes GdkPixbuf return NULL without setting a GError, so the
        // gtk-rs binding's null assertion unwinds in the real post-`mem::take` decode path.
        let failed = queue.submit(url.clone(), 0, 40, CacheScope::Transient);
        assert!(
            failed.recv_blocking().is_err(),
            "the taken waiter's sender must be dropped while decode unwinds"
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let is_pending = queue
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains_key(&key);
            if !is_pending {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the panicking job left its URL pending"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        let retried = queue.submit(url, 40, 40, CacheScope::Transient);
        assert!(matches!(retried.recv_blocking(), Ok(Some(_))));

        drop(queue);
        worker.join().unwrap();
    }
}
