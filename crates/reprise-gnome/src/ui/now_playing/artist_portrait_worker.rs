//! Live permission and a bounded off-thread queue for artist portraits shown
//! in My Stats (STATS-23).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gtk4::gio;
use gtk4::glib;
use reprise_core::db::Db;

type PortraitResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type PortraitCallback = Rc<dyn Fn(Option<PathBuf>)>;

/// Twenty ranks can appear at once. Keeping only three portrait requests in
/// flight avoids flooding the blocking pool and the remote provider.
const MAX_IN_FLIGHT: usize = 3;

pub(in crate::ui) struct ArtistPortraitRuntime {
    pub enabled: Rc<Cell<bool>>,
    worker_enabled: Arc<AtomicBool>,
    resolve: PortraitResolver,
    in_flight: Rc<Cell<usize>>,
    queue: Rc<RefCell<VecDeque<(String, PortraitCallback)>>>,
}

impl ArtistPortraitRuntime {
    pub(in crate::ui) fn setup(conn: &Db) -> Rc<Self> {
        let enabled = reprise_core::online_sources::network_allowed_or_off(
            conn,
            &reprise_core::modules::ARTWORK_MODULE,
        );
        Self::new(
            enabled,
            |artist| match reprise_core::artist_portrait::load_or_fetch(artist) {
                Ok(reprise_core::artist_portrait::PortraitOutcome::Found(path)) => Some(path),
                Ok(reprise_core::artist_portrait::PortraitOutcome::NotFound) => None,
                Err(error) => {
                    tracing::debug!(%error, %artist, "artist portrait request failed");
                    None
                }
            },
        )
    }

    /// `NET-1a`: re-derives `enabled` from the global online-sources gate.
    pub(in crate::ui) fn recompute_enabled(&self, conn: &Db) {
        let enabled = reprise_core::online_sources::network_allowed_or_off(
            conn,
            &reprise_core::modules::ARTWORK_MODULE,
        );
        self.worker_enabled.store(enabled, Ordering::Relaxed);
        self.enabled.set(enabled);
    }

    fn new(
        enabled: bool,
        resolve: impl Fn(&str) -> Option<PathBuf> + Send + Sync + 'static,
    ) -> Rc<Self> {
        Rc::new(Self {
            enabled: Rc::new(Cell::new(enabled)),
            worker_enabled: Arc::new(AtomicBool::new(enabled)),
            resolve: Arc::new(resolve),
            in_flight: Rc::new(Cell::new(0)),
            queue: Rc::new(RefCell::new(VecDeque::new())),
        })
    }

    #[cfg(test)]
    pub(in crate::ui) fn for_test(
        enabled: bool,
        resolve: impl Fn(&str) -> Option<PathBuf> + Send + Sync + 'static,
    ) -> Rc<Self> {
        Self::new(enabled, resolve)
    }

    pub(in crate::ui) fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    /// Whether requesting `name` can call the resolver. Pure so non-display
    /// tests can prove the network gate without observing a request.
    pub(in crate::ui) fn request_would_run(&self, name: &str) -> bool {
        self.is_enabled() && !name.trim().is_empty()
    }

    /// Queues one portrait lookup and calls `on_ready` on the main context.
    /// Disabled and blank requests resolve locally and never enter the queue.
    pub(in crate::ui) fn request(
        self: &Rc<Self>,
        name: String,
        on_ready: impl Fn(Option<PathBuf>) + 'static,
    ) {
        if !self.request_would_run(&name) {
            on_ready(None);
            return;
        }
        self.queue.borrow_mut().push_back((name, Rc::new(on_ready)));
        self.pump();
    }

    fn pump(self: &Rc<Self>) {
        while self.in_flight.get() < MAX_IN_FLIGHT {
            let next = self.queue.borrow_mut().pop_front();
            let Some((name, on_ready)) = next else {
                return;
            };
            self.in_flight.set(self.in_flight.get() + 1);
            let this = self.clone();
            let gate = self.worker_enabled.clone();
            let resolve = self.resolve.clone();
            glib::spawn_future_local(async move {
                let worker_gate = gate.clone();
                let found = gio::spawn_blocking(move || {
                    worker_gate
                        .load(Ordering::Relaxed)
                        .then(|| resolve(&name))
                        .flatten()
                })
                .await
                .ok()
                .flatten();
                this.in_flight.set(this.in_flight.get().saturating_sub(1));
                on_ready(gate.load(Ordering::Relaxed).then_some(found).flatten());
                this.pump();
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_recomputes_the_live_artwork_setting() {
        let conn = crate::test_db::open().unwrap();
        let runtime = ArtistPortraitRuntime::setup(&conn);
        assert!(!runtime.enabled.get());

        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE, true)
            .unwrap();
        runtime.recompute_enabled(&conn);

        assert!(runtime.enabled.get());
        assert!(
            reprise_core::modules::is_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE)
                .unwrap()
        );
    }

    #[test]
    fn net_1a_recompute_enabled_reflects_the_global_gate() {
        let conn = crate::test_db::open().unwrap();
        let runtime = ArtistPortraitRuntime::setup(&conn);
        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE, true)
            .unwrap();
        runtime.recompute_enabled(&conn);
        assert!(runtime.enabled.get());

        reprise_core::online_sources::set_enabled(&conn, false).unwrap();
        runtime.recompute_enabled(&conn);
        assert!(!runtime.enabled.get());

        reprise_core::online_sources::set_enabled(&conn, true).unwrap();
        runtime.recompute_enabled(&conn);
        assert!(runtime.enabled.get());
    }
}
