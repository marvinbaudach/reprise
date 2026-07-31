//! Serial off-main lyrics lookup worker.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use reprise_core::lyrics::{LookupOptions, LyricsError, LyricsHit, LyricsQuery};

pub(in crate::ui) type Lookup = Arc<
    dyn Fn(&LyricsQuery, Option<&Path>, LookupOptions) -> Result<LyricsHit, LyricsError>
        + Send
        + Sync,
>;

pub(in crate::ui) struct LyricsRequest {
    pub(in crate::ui) generation: u64,
    pub(in crate::ui) query: LyricsQuery,
    pub(in crate::ui) track_path: Option<PathBuf>,
    pub(in crate::ui) options: LookupOptions,
    pub(in crate::ui) response: async_channel::Sender<LyricsResponse>,
}

pub(in crate::ui) struct LyricsResponse {
    pub(in crate::ui) generation: u64,
    pub(in crate::ui) options: LookupOptions,
    pub(in crate::ui) result: Result<LyricsHit, LyricsError>,
}

pub(in crate::ui) struct LyricsRuntime {
    sender: async_channel::Sender<LyricsRequest>,
}

impl LyricsRuntime {
    pub(in crate::ui) fn setup() -> Rc<Self> {
        Self::from_lookup(Arc::new(|query, track_path, options| {
            reprise_core::lyrics::load_or_fetch_with_options(query, track_path, options)
        }))
    }

    #[cfg(test)]
    pub(in crate::ui) fn setup_with_lookup(lookup: Lookup) -> Rc<Self> {
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

    pub(in crate::ui) fn request(&self, request: LyricsRequest) {
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
        let result = lookup(
            &request.query,
            request.track_path.as_deref(),
            request.options,
        );
        let response = LyricsResponse {
            generation: request.generation,
            options: request.options,
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

    use reprise_core::lyrics::{LookupOptions, LyricsBody, LyricsHit, LyricsQuery, LyricsSource};

    use super::*;

    fn query(title: &str) -> LyricsQuery {
        LyricsQuery {
            title: title.into(),
            artist: "Synthetic Artist".into(),
            album: "Synthetic Album".into(),
            duration_ms: 10_000,
        }
    }

    fn hit(title: &str) -> LyricsHit {
        LyricsHit {
            body: LyricsBody::Plain(format!("{title} text")),
            source: LyricsSource::Lrclib,
        }
    }

    fn request(
        generation: u64,
        title: &str,
        response: async_channel::Sender<LyricsResponse>,
    ) -> LyricsRequest {
        LyricsRequest {
            generation,
            query: query(title),
            track_path: Some(PathBuf::from(format!("/music/{title}.flac"))),
            options: LookupOptions::default(),
            response,
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
            move |request: &LyricsQuery, _track_path, _options| {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(now, Ordering::SeqCst);
                order.lock().unwrap().push(request.title.clone());
                if request.title == "First" {
                    started.wait();
                    release.wait();
                }
                std::thread::sleep(Duration::from_millis(10));
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(hit(&request.title))
            }
        }));

        let (first_tx, first_rx) = async_channel::bounded(1);
        let (second_tx, second_rx) = async_channel::bounded(1);
        runtime.request(request(7, "First", first_tx));
        started.wait();
        runtime.request(request(8, "Second", second_tx));
        release.wait();

        let first = first_rx.recv_blocking().unwrap();
        let second = second_rx.recv_blocking().unwrap();
        assert_eq!(first.generation, 7);
        assert_eq!(first.result, Ok(hit("First")));
        assert_eq!(second.generation, 8);
        assert_eq!(second.result, Ok(hit("Second")));
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
            move |request: &LyricsQuery, _track_path, _options| {
                order.lock().unwrap().push(request.title.clone());
                if request.title == "First" {
                    started.wait();
                    release.wait();
                }
                Ok(hit(&request.title))
            }
        }));

        let (first_tx, first_rx) = async_channel::bounded(1);
        runtime.request(request(1, "First", first_tx));
        started.wait();

        let (second_tx, second_rx) = async_channel::bounded(1);
        let (third_tx, third_rx) = async_channel::bounded(1);
        runtime.request(request(2, "Second", second_tx));
        runtime.request(request(3, "Third", third_tx));
        release.wait();

        assert_eq!(first_rx.recv_blocking().unwrap().generation, 1);
        assert!(second_rx.recv_blocking().is_err());
        assert_eq!(third_rx.recv_blocking().unwrap().generation, 3);
        assert_eq!(&*order.lock().unwrap(), &["First", "Third"]);
    }
}
