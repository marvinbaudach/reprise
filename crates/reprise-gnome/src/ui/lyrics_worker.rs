//! Serial off-main lyrics lookup worker.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the runtime is connected to the Lyrics view in the planned integration task"
    )
)]

use std::rc::Rc;
use std::sync::Arc;

use reprise_core::lyrics::{LyricsBody, LyricsError, LyricsQuery};

type Lookup = Arc<dyn Fn(&LyricsQuery) -> Result<LyricsBody, LyricsError> + Send + Sync>;

pub(super) struct LyricsRequest {
    pub(super) generation: u64,
    pub(super) query: LyricsQuery,
    pub(super) response: async_channel::Sender<LyricsResponse>,
}

pub(super) struct LyricsResponse {
    pub(super) generation: u64,
    pub(super) result: Result<LyricsBody, LyricsError>,
}

pub(super) struct LyricsRuntime {
    sender: async_channel::Sender<LyricsRequest>,
}

impl LyricsRuntime {
    #[cfg_attr(
        test,
        expect(dead_code, reason = "tests inject a deterministic lookup instead")
    )]
    pub(super) fn setup() -> Rc<Self> {
        Self::from_lookup(Arc::new(reprise_core::lyrics::load_or_fetch))
    }

    #[cfg(test)]
    fn setup_with_lookup(lookup: Lookup) -> Rc<Self> {
        Self::from_lookup(lookup)
    }

    fn from_lookup(lookup: Lookup) -> Rc<Self> {
        let (sender, receiver) = async_channel::unbounded::<LyricsRequest>();
        std::thread::Builder::new()
            .name("reprise-lyrics".into())
            .spawn(move || run(&receiver, &lookup))
            .expect("lyrics worker thread should start");
        Rc::new(Self { sender })
    }

    pub(super) fn request(&self, request: LyricsRequest) {
        if let Err(error) = self.sender.try_send(request) {
            tracing::warn!(%error, "lyrics request dropped: worker is gone");
        }
    }
}

fn run(receiver: &async_channel::Receiver<LyricsRequest>, lookup: &Lookup) {
    while let Ok(mut request) = receiver.recv_blocking() {
        while let Ok(newer) = receiver.try_recv() {
            request = newer;
        }
        let result = lookup(&request.query);
        let response = LyricsResponse {
            generation: request.generation,
            result,
        };
        if request.response.send_blocking(response).is_err() {
            tracing::debug!(
                generation = request.generation,
                "lyrics response dropped: requester is gone"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Mutex};
    use std::time::Duration;

    use reprise_core::lyrics::{LyricsBody, LyricsQuery};

    use super::*;

    fn query(title: &str) -> LyricsQuery {
        LyricsQuery {
            title: title.into(),
            artist: "Synthetic Artist".into(),
            album: "Synthetic Album".into(),
            duration_ms: 10_000,
        }
    }

    #[test]
    fn worker_executes_requests_serially_and_preserves_generation() {
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let order = Arc::new(Mutex::new(Vec::new()));
        let started = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let runtime = LyricsRuntime::setup_with_lookup(Arc::new({
            let active = active.clone();
            let maximum = maximum.clone();
            let order = order.clone();
            let started = started.clone();
            let release = release.clone();
            move |request: &LyricsQuery| {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(now, Ordering::SeqCst);
                order.lock().unwrap().push(request.title.clone());
                if request.title == "First" {
                    started.wait();
                    release.wait();
                }
                std::thread::sleep(Duration::from_millis(10));
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(LyricsBody::Plain(format!("{} text", request.title)))
            }
        }));

        let (first_tx, first_rx) = async_channel::bounded(1);
        let (second_tx, second_rx) = async_channel::bounded(1);
        runtime.request(LyricsRequest {
            generation: 7,
            query: query("First"),
            response: first_tx,
        });
        started.wait();
        runtime.request(LyricsRequest {
            generation: 8,
            query: query("Second"),
            response: second_tx,
        });
        release.wait();

        let first = first_rx.recv_blocking().unwrap();
        let second = second_rx.recv_blocking().unwrap();
        assert_eq!(first.generation, 7);
        assert_eq!(first.result, Ok(LyricsBody::Plain("First text".into())));
        assert_eq!(second.generation, 8);
        assert_eq!(second.result, Ok(LyricsBody::Plain("Second text".into())));
        assert_eq!(maximum.load(Ordering::SeqCst), 1);
        assert_eq!(&*order.lock().unwrap(), &["First", "Second"]);
    }

    #[test]
    fn worker_discards_superseded_pending_requests_before_lookup() {
        let started = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let order = Arc::new(Mutex::new(Vec::new()));
        let runtime = LyricsRuntime::setup_with_lookup(Arc::new({
            let started = started.clone();
            let release = release.clone();
            let order = order.clone();
            move |request: &LyricsQuery| {
                order.lock().unwrap().push(request.title.clone());
                if request.title == "First" {
                    started.wait();
                    release.wait();
                }
                Ok(LyricsBody::Plain(format!("{} text", request.title)))
            }
        }));

        let (first_tx, first_rx) = async_channel::bounded(1);
        runtime.request(LyricsRequest {
            generation: 1,
            query: query("First"),
            response: first_tx,
        });
        started.wait();

        let (second_tx, second_rx) = async_channel::bounded(1);
        let (third_tx, third_rx) = async_channel::bounded(1);
        runtime.request(LyricsRequest {
            generation: 2,
            query: query("Second"),
            response: second_tx,
        });
        runtime.request(LyricsRequest {
            generation: 3,
            query: query("Third"),
            response: third_tx,
        });
        release.wait();

        assert_eq!(first_rx.recv_blocking().unwrap().generation, 1);
        assert!(second_rx.recv_blocking().is_err());
        assert_eq!(third_rx.recv_blocking().unwrap().generation, 3);
        assert_eq!(&*order.lock().unwrap(), &["First", "Third"]);
    }
}
