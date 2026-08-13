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
type PortraitGuard = Rc<dyn Fn() -> bool>;

/// Twenty ranks can appear at once. Keeping only three portrait requests in
/// flight avoids flooding the blocking pool and the remote provider.
const MAX_IN_FLIGHT: usize = 3;

pub(in crate::ui) struct ArtistPortraitRuntime {
    pub enabled: Rc<Cell<bool>>,
    worker_enabled: Arc<AtomicBool>,
    resolve: PortraitResolver,
    in_flight: Rc<Cell<usize>>,
    queue: Rc<RefCell<VecDeque<(String, PortraitGuard, PortraitCallback)>>>,
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
    #[cfg_attr(not(test), allow(dead_code))]
    pub(in crate::ui) fn request(
        self: &Rc<Self>,
        name: String,
        on_ready: impl Fn(Option<PathBuf>) + 'static,
    ) {
        self.request_while(name, || true, on_ready);
    }

    /// Like `request`, but drops work that stopped being visible while it was
    /// waiting for one of the bounded worker slots (STATS-23).
    pub(in crate::ui) fn request_while(
        self: &Rc<Self>,
        name: String,
        still_visible: impl Fn() -> bool + 'static,
        on_ready: impl Fn(Option<PathBuf>) + 'static,
    ) {
        if !self.request_would_run(&name) || !still_visible() {
            on_ready(None);
            return;
        }
        self.queue
            .borrow_mut()
            .push_back((name, Rc::new(still_visible), Rc::new(on_ready)));
        self.pump();
    }

    fn pump(self: &Rc<Self>) {
        while self.in_flight.get() < MAX_IN_FLIGHT {
            let next = self.queue.borrow_mut().pop_front();
            let Some((name, still_visible, on_ready)) = next else {
                return;
            };
            if !still_visible() {
                on_ready(None);
                continue;
            }
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

    #[test]
    fn stats_23_stale_queued_portraits_never_reach_the_resolver() {
        let resolver_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let runtime = ArtistPortraitRuntime::for_test(true, {
            let resolver_calls = resolver_calls.clone();
            move |_| {
                resolver_calls.fetch_add(1, Ordering::SeqCst);
                None
            }
        });
        runtime.in_flight.set(MAX_IN_FLIGHT);
        let still_visible = Rc::new(Cell::new(true));
        let result = Rc::new(RefCell::new(Vec::new()));

        runtime.request_while(
            "Former leader".to_string(),
            {
                let still_visible = still_visible.clone();
                move || still_visible.get()
            },
            {
                let result = result.clone();
                move |path| result.borrow_mut().push(path)
            },
        );
        assert_eq!(runtime.queue.borrow().len(), 1);

        still_visible.set(false);
        runtime.in_flight.set(0);
        runtime.pump();

        assert_eq!(resolver_calls.load(Ordering::SeqCst), 0);
        assert_eq!(&*result.borrow(), &[None]);
    }
}
