use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::glib;
use rusqlite::Connection;

use super::cover_download_worker::{CoverDownloadRuntime, DownloadOutcome, DownloadRequest};
use super::player_controller::PlayerController;
use super::track_list::TrackList;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum BatchState {
    Idle,
    Running,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct BatchProgress {
    pub(in crate::ui) state: BatchState,
    pub(in crate::ui) checked: usize,
    pub(in crate::ui) total: usize,
    pub(in crate::ui) downloaded: usize,
    pub(in crate::ui) unavailable: usize,
}

impl BatchProgress {
    fn idle() -> Self {
        Self {
            state: BatchState::Idle,
            checked: 0,
            total: 0,
            downloaded: 0,
            unavailable: 0,
        }
    }

    fn running(total: usize) -> Self {
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

    fn failed() -> Self {
        Self {
            state: BatchState::Failed,
            ..Self::idle()
        }
    }

    fn advance(mut self, outcome: &DownloadOutcome) -> Self {
        if self.state != BatchState::Running {
            return self;
        }
        self.checked = self.checked.saturating_add(1).min(self.total);
        match outcome {
            DownloadOutcome::AlreadyCovered => {}
            DownloadOutcome::Downloaded(_) => self.downloaded += 1,
            DownloadOutcome::Unavailable => self.unavailable += 1,
        }
        if self.checked == self.total {
            self.state = BatchState::Complete;
        }
        self
    }

    pub(in crate::ui) fn fraction(self) -> f64 {
        if self.total == 0 {
            return f64::from(self.state == BatchState::Complete);
        }
        self.checked as f64 / self.total as f64
    }
}

type IsAlive = Rc<dyn Fn() -> bool>;
type OnProgress = Rc<dyn Fn(BatchProgress)>;

#[derive(Clone)]
struct ProgressSubscriber {
    id: u64,
    is_alive: IsAlive,
    callback: OnProgress,
}

#[derive(Default)]
struct ProgressSubscribers {
    next_id: Cell<u64>,
    entries: RefCell<Vec<ProgressSubscriber>>,
}

impl ProgressSubscribers {
    fn subscribe(
        &self,
        current: BatchProgress,
        is_alive: impl Fn() -> bool + 'static,
        callback: impl Fn(BatchProgress) + 'static,
    ) {
        self.prune();
        let is_alive: IsAlive = Rc::new(is_alive);
        if !is_alive() {
            return;
        }
        let callback: OnProgress = Rc::new(callback);
        callback(current);
        if !is_alive() {
            return;
        }
        let id = self.next_id.get().wrapping_add(1);
        self.next_id.set(id);
        self.entries.borrow_mut().push(ProgressSubscriber {
            id,
            is_alive,
            callback,
        });
    }

    fn notify(&self, progress: BatchProgress) {
        self.prune();
        let entries = self.entries.borrow().clone();
        for entry in entries {
            if (entry.is_alive)() {
                (entry.callback)(progress);
            }
        }
        self.prune();
    }

    fn prune(&self) {
        let entries = self.entries.borrow().clone();
        let dead: Vec<u64> = entries
            .iter()
            .filter_map(|entry| (!(entry.is_alive)()).then_some(entry.id))
            .collect();
        if dead.is_empty() {
            return;
        }
        self.entries
            .borrow_mut()
            .retain(|entry| !dead.contains(&entry.id));
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.borrow().len()
    }
}

pub(in crate::ui) struct CoverDownloadBatch {
    conn: Rc<RefCell<Connection>>,
    runtime: CoverDownloadRuntime,
    track_list: Rc<TrackList>,
    player: Option<Rc<PlayerController>>,
    generation: Cell<u64>,
    progress: Cell<BatchProgress>,
    progress_subscribers: ProgressSubscribers,
}

impl CoverDownloadBatch {
    pub(in crate::ui) fn new(
        conn: &Rc<RefCell<Connection>>,
        runtime: &CoverDownloadRuntime,
        track_list: &Rc<TrackList>,
        player: Option<&Rc<PlayerController>>,
    ) -> Rc<Self> {
        Rc::new(Self {
            conn: conn.clone(),
            runtime: runtime.clone(),
            track_list: track_list.clone(),
            player: player.cloned(),
            generation: Cell::new(0),
            progress: Cell::new(BatchProgress::idle()),
            progress_subscribers: ProgressSubscribers::default(),
        })
    }

    pub(in crate::ui) fn subscribe_progress(
        &self,
        is_alive: impl Fn() -> bool + 'static,
        callback: impl Fn(BatchProgress) + 'static,
    ) {
        self.progress_subscribers
            .subscribe(self.progress.get(), is_alive, callback);
    }

    pub(in crate::ui) fn start(self: &Rc<Self>) {
        let paths = {
            let conn = self.conn.borrow();
            reprise_core::queries::query_live_track_paths(&conn)
        };
        let paths = match paths {
            Ok(paths) => paths,
            Err(error) => {
                tracing::warn!(%error, "could not query tracks for cover download");
                self.set_progress(BatchProgress::failed());
                return;
            }
        };
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.set_progress(BatchProgress::running(paths.len()));
        if paths.is_empty() {
            return;
        }

        let this = self.clone();
        glib::spawn_future_local(async move {
            for path in &paths {
                if this.generation.get() != generation {
                    return;
                }
                let (response, result) = async_channel::bounded(1);
                if this
                    .runtime
                    .worker
                    .send(DownloadRequest {
                        track_path: path.clone(),
                        skip_if_covered: true,
                        response,
                    })
                    .await
                    .is_err()
                {
                    this.set_progress(BatchProgress::failed());
                    return;
                }
                let outcome = result.recv().await.unwrap_or(DownloadOutcome::Unavailable);
                if this.generation.get() != generation {
                    return;
                }
                this.set_progress(this.progress.get().advance(&outcome));
            }

            if this.generation.get() != generation {
                return;
            }
            let refreshed_paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
            this.track_list.reload();
            if let Some(player) = &this.player {
                player.refresh_edited_cover(&refreshed_paths);
            }
        });
    }

    fn set_progress(&self, progress: BatchProgress) {
        self.progress.set(progress);
        self.progress_subscribers.notify(progress);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;
    use std::rc::Rc;

    use super::{BatchProgress, BatchState, ProgressSubscribers};
    use crate::ui::cover_download_worker::DownloadOutcome;

    #[test]
    fn progress_counts_checked_downloaded_and_unavailable_outcomes() {
        let progress = BatchProgress::running(3)
            .advance(&DownloadOutcome::AlreadyCovered)
            .advance(&DownloadOutcome::Downloaded(PathBuf::from(
                "/cache/cover.jpg",
            )))
            .advance(&DownloadOutcome::Unavailable);

        assert_eq!(progress.state, BatchState::Complete);
        assert_eq!(progress.checked, 3);
        assert_eq!(progress.downloaded, 1);
        assert_eq!(progress.unavailable, 1);
        assert_eq!(progress.fraction(), 1.0);
    }

    #[test]
    fn live_track_paths_excludes_missing_rows_and_is_stable() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, added_at) \
             VALUES ('/music/b.mp3', 'B', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, added_at) \
             VALUES ('/music/a.mp3', 'A', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, added_at, missing_since) \
             VALUES ('/music/gone.mp3', 'Gone', 1, 1)",
            [],
        )
        .unwrap();

        assert_eq!(
            reprise_core::queries::query_live_track_paths(&conn).unwrap(),
            vec!["/music/a.mp3".to_string(), "/music/b.mp3".to_string()]
        );
    }

    #[test]
    fn multiple_progress_subscribers_receive_current_and_future_state() {
        let subscribers = ProgressSubscribers::default();
        let first = Rc::new(RefCell::new(Vec::new()));
        let second = Rc::new(RefCell::new(Vec::new()));

        for received in [&first, &second] {
            let received = received.clone();
            subscribers.subscribe(
                BatchProgress::idle(),
                || true,
                move |progress| {
                    received.borrow_mut().push(progress);
                },
            );
        }
        let running = BatchProgress::running(4);
        subscribers.notify(running);

        assert_eq!(*first.borrow(), vec![BatchProgress::idle(), running]);
        assert_eq!(*second.borrow(), vec![BatchProgress::idle(), running]);
    }

    #[test]
    fn dead_subscriber_is_removed_without_replaying_state_to_live_ones() {
        let subscribers = ProgressSubscribers::default();
        let calls = Rc::new(Cell::new(0));
        let alive = Rc::new(Cell::new(true));
        let calls_for_callback = calls.clone();
        let alive_for_probe = alive.clone();
        subscribers.subscribe(
            BatchProgress::idle(),
            move || alive_for_probe.get(),
            move |_| calls_for_callback.set(calls_for_callback.get() + 1),
        );

        alive.set(false);
        subscribers.subscribe(BatchProgress::idle(), || true, |_| {});
        subscribers.notify(BatchProgress::running(2));

        assert_eq!(calls.get(), 1);
        assert_eq!(subscribers.len(), 1);
    }

    #[test]
    fn subscriber_destroyed_by_initial_callback_is_not_retained() {
        let subscribers = ProgressSubscribers::default();
        let alive = Rc::new(Cell::new(true));
        let alive_for_probe = alive.clone();
        let alive_for_callback = alive.clone();
        subscribers.subscribe(
            BatchProgress::idle(),
            move || alive_for_probe.get(),
            move |_| alive_for_callback.set(false),
        );

        assert_eq!(subscribers.len(), 0);
    }
}
