//! Unbounded, coalescing source-artwork worker queue.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use reprise_core::remote_image::{CacheScope, ImageOutcome};

use super::{decode_pixels, DecodedPixels};

const ARTWORK_WORKERS: usize = 8;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct ArtworkKey {
    url: String,
    // A transient result must never satisfy a persistent request without
    // populating the persistent store, so scope is part of request identity.
    cache_scope: CacheScope,
}

struct ArtworkWaiter {
    width: i32,
    height: i32,
    response: async_channel::Sender<Option<DecodedPixels>>,
}

type Pending = Arc<Mutex<HashMap<ArtworkKey, Vec<ArtworkWaiter>>>>;

#[derive(Clone)]
pub(super) struct ArtworkQueue {
    sender: async_channel::Sender<ArtworkKey>,
    pending: Pending,
}

impl ArtworkQueue {
    fn start() -> Self {
        let (sender, receiver) = async_channel::unbounded();
        let pending = Arc::new(Mutex::new(HashMap::new()));
        for index in 0..ARTWORK_WORKERS {
            let receiver = receiver.clone();
            let pending = pending.clone();
            if let Err(error) = std::thread::Builder::new()
                .name(format!("reprise-source-artwork-{index}"))
                .spawn(move || {
                    while let Ok(job) = receiver.recv_blocking() {
                        process_job(&pending, job, &mut |url| {
                            reprise_core::podcasts::source_artwork::fetch(url)
                                .map_err(|error| error.to_string())
                        });
                    }
                })
            {
                tracing::warn!(%error, "could not start source artwork worker");
            }
        }
        Self { sender, pending }
    }

    pub(super) fn submit(
        &self,
        url: String,
        width: i32,
        height: i32,
        cache_scope: CacheScope,
    ) -> async_channel::Receiver<Option<DecodedPixels>> {
        let key = ArtworkKey { url, cache_scope };
        let (response, receiver) = async_channel::bounded(1);
        let waiter = ArtworkWaiter {
            width,
            height,
            response,
        };
        let is_new_job = {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            match pending.get_mut(&key) {
                Some(waiters) => {
                    waiters.push(waiter);
                    false
                }
                None => {
                    pending.insert(key.clone(), vec![waiter]);
                    true
                }
            }
        };
        if is_new_job && self.sender.try_send(key.clone()).is_err() {
            finish_without_image(&self.pending, &key);
        }
        receiver
    }

    #[cfg(test)]
    fn test_queue() -> (Self, async_channel::Receiver<ArtworkKey>) {
        let (sender, receiver) = async_channel::unbounded();
        (
            Self {
                sender,
                pending: Arc::new(Mutex::new(HashMap::new())),
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
) -> async_channel::Receiver<Option<DecodedPixels>> {
    static QUEUE: OnceLock<ArtworkQueue> = OnceLock::new();
    QUEUE
        .get_or_init(ArtworkQueue::start)
        .submit(url, width, height, cache_scope)
}

/// Resolves one URL against the gate state at fetch time, then fans the result
/// out to every waiter that joined while the job was queued or running.
fn process_job(
    pending: &Pending,
    key: ArtworkKey,
    fetch: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) {
    let allowed = super::GATE_OPEN.load(std::sync::atomic::Ordering::Relaxed);
    let outcome =
        reprise_core::remote_image::resolve(Some(&key.url), key.cache_scope, allowed, fetch);
    let path = match outcome {
        ImageOutcome::Cached(path) | ImageOutcome::Fetched(path) => Some(path),
        ImageOutcome::NotAllowed | ImageOutcome::NoUrl | ImageOutcome::FetchFailed => None,
    };

    loop {
        let waiters = {
            let mut pending = pending.lock().unwrap_or_else(|error| error.into_inner());
            let Some(waiters) = pending.get_mut(&key) else {
                return;
            };
            std::mem::take(waiters)
        };
        for waiter in waiters {
            let pixels = path.as_ref().and_then(|path| {
                decode_pixels(path, waiter.width, waiter.height)
                    .map_err(|error| {
                        tracing::debug!(
                            %error,
                            url = %key.url,
                            path = %path.display(),
                            "source artwork could not be decoded"
                        );
                    })
                    .ok()
            });
            let _ = waiter.response.send_blocking(pixels);
        }

        let mut pending = pending.lock().unwrap_or_else(|error| error.into_inner());
        if pending.get(&key).is_some_and(Vec::is_empty) {
            pending.remove(&key);
            return;
        }
    }
}

fn finish_without_image(pending: &Pending, key: &ArtworkKey) {
    let waiters = pending
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(key)
        .unwrap_or_default();
    for waiter in waiters {
        let _ = waiter.response.send_blocking(None);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn src_11_many_concurrent_requests_all_receive_a_result() {
        let _gate = super::super::GATE_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
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
            super::process_job(&queue.pending, job, &mut |_| Ok(TINY_PNG.to_vec()));
        }
        let results = results.lock().unwrap();
        assert_eq!(results.len(), 160);
        assert!(results
            .iter()
            .all(|result| matches!(result.try_recv(), Ok(Some(_)))));
    }

    #[test]
    fn src_11_duplicate_url_fetches_once_and_answers_every_waiter() {
        let _gate = super::super::GATE_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        super::super::GATE_OPEN.store(true, std::sync::atomic::Ordering::SeqCst);
        let (queue, jobs) = super::ArtworkQueue::test_queue();
        let url = unique_url("shared");
        let first = queue.submit(url.clone(), 40, 40, CacheScope::Persistent);
        assert_eq!(jobs.len(), 1, "an in-flight URL has one queued job");

        let mut fetches = 0;
        let mut second = None;
        super::process_job(&queue.pending, jobs.try_recv().unwrap(), &mut |_| {
            fetches += 1;
            second = Some(queue.submit(url.clone(), 80, 80, CacheScope::Persistent));
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
    }

    #[test]
    fn src_11_gate_closed_after_enqueue_still_prevents_the_fetch() {
        use std::sync::atomic::Ordering;

        let _gate = super::super::GATE_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
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
        super::process_job(&queue.pending, jobs.try_recv().unwrap(), &mut |_| {
            fetch_called = true;
            Err("must not fetch".into())
        });

        assert!(!fetch_called);
        assert!(matches!(result.try_recv(), Ok(None)));
    }
}
