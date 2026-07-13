use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::glib;
use rusqlite::Connection;

use super::cover_download_worker::{CoverDownloadRuntime, DownloadOutcome, DownloadRequest};
use super::player_controller::PlayerController;
use super::track_list::TrackList;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BatchState {
    Idle,
    Running,
    Complete,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BatchProgress {
    pub(super) state: BatchState,
    pub(super) checked: usize,
    pub(super) total: usize,
    pub(super) downloaded: usize,
    pub(super) unavailable: usize,
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

    fn stopped(mut self) -> Self {
        if self.state == BatchState::Running {
            self.state = BatchState::Stopped;
        }
        self
    }

    pub(super) fn fraction(self) -> f64 {
        if self.total == 0 {
            return f64::from(self.state == BatchState::Complete);
        }
        self.checked as f64 / self.total as f64
    }
}

type OnProgress = Rc<dyn Fn(BatchProgress) -> bool>;

#[derive(Clone)]
struct ProgressSubscriber {
    id: u64,
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
        callback: impl Fn(BatchProgress) -> bool + 'static,
    ) {
        self.notify(current);
        let callback: OnProgress = Rc::new(callback);
        if !callback(current) {
            return;
        }
        let id = self.next_id.get().wrapping_add(1);
        self.next_id.set(id);
        self.entries
            .borrow_mut()
            .push(ProgressSubscriber { id, callback });
    }

    fn notify(&self, progress: BatchProgress) {
        let entries = self.entries.borrow().clone();
        let dead: Vec<u64> = entries
            .iter()
            .filter_map(|entry| (!(entry.callback)(progress)).then_some(entry.id))
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

pub(super) struct CoverDownloadBatch {
    conn: Rc<RefCell<Connection>>,
    runtime: CoverDownloadRuntime,
    track_list: Rc<TrackList>,
    player: Option<Rc<PlayerController>>,
    generation: Cell<u64>,
    progress: Cell<BatchProgress>,
    progress_subscribers: ProgressSubscribers,
}

impl CoverDownloadBatch {
    pub(super) fn new(
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

    pub(super) fn subscribe_progress(&self, callback: impl Fn(BatchProgress) -> bool + 'static) {
        self.progress_subscribers
            .subscribe(self.progress.get(), callback);
    }

    pub(super) fn set_enabled(self: &Rc<Self>, enabled: bool) {
        if enabled {
            self.start();
        } else {
            self.stop();
        }
    }

    pub(super) fn start_if_enabled(self: &Rc<Self>) {
        if self.runtime.enabled.get() {
            self.start();
        }
    }

    fn start(self: &Rc<Self>) {
        let paths = {
            let conn = self.conn.borrow();
            live_track_paths(&conn)
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
                if this.generation.get() != generation || !this.runtime.enabled.get() {
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

    fn stop(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        self.set_progress(self.progress.get().stopped());
    }

    fn set_progress(&self, progress: BatchProgress) {
        self.progress.set(progress);
        self.progress_subscribers.notify(progress);
    }
}

fn live_track_paths(conn: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = conn.prepare("SELECT path FROM tracks WHERE missing = 0 ORDER BY path")?;
    let paths = statement.query_map([], |row| row.get(0))?.collect();
    paths
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;
    use std::rc::Rc;

    use super::{live_track_paths, BatchProgress, BatchState, ProgressSubscribers};
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
    fn stopping_preserves_partial_counts_and_never_completes_the_run() {
        let progress = BatchProgress::running(4)
            .advance(&DownloadOutcome::AlreadyCovered)
            .stopped();

        assert_eq!(progress.state, BatchState::Stopped);
        assert_eq!(progress.checked, 1);
        assert_eq!(progress.total, 4);
        assert_eq!(progress.fraction(), 0.25);
    }

    #[test]
    fn live_track_paths_excludes_missing_rows_and_is_stable() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, added_at, missing) \
             VALUES ('/music/b.mp3', 'B', 1, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, added_at, missing) \
             VALUES ('/music/a.mp3', 'A', 1, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, added_at, missing) \
             VALUES ('/music/gone.mp3', 'Gone', 1, 1)",
            [],
        )
        .unwrap();

        assert_eq!(
            live_track_paths(&conn).unwrap(),
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
            subscribers.subscribe(BatchProgress::idle(), move |progress| {
                received.borrow_mut().push(progress);
                true
            });
        }
        let running = BatchProgress::running(4);
        subscribers.notify(running);

        assert_eq!(
            *first.borrow(),
            vec![BatchProgress::idle(), BatchProgress::idle(), running]
        );
        assert_eq!(*second.borrow(), vec![BatchProgress::idle(), running]);
    }

    #[test]
    fn subscriber_returning_false_is_removed_after_that_update() {
        let subscribers = ProgressSubscribers::default();
        let calls = Rc::new(Cell::new(0));
        let calls_for_callback = calls.clone();
        subscribers.subscribe(BatchProgress::idle(), move |_| {
            calls_for_callback.set(calls_for_callback.get() + 1);
            calls_for_callback.get() < 2
        });

        subscribers.notify(BatchProgress::running(2));
        subscribers.notify(BatchProgress::running(3));

        assert_eq!(calls.get(), 2);
        assert_eq!(subscribers.len(), 0);
    }
}
