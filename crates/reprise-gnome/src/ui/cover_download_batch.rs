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

    #[allow(dead_code)] // Read by the Preferences progress row in Task 3.
    pub(super) fn fraction(self) -> f64 {
        if self.total == 0 {
            return f64::from(self.state == BatchState::Complete);
        }
        self.checked as f64 / self.total as f64
    }
}

type OnProgress = Rc<dyn Fn(BatchProgress)>;

pub(super) struct CoverDownloadBatch {
    conn: Rc<RefCell<Connection>>,
    runtime: CoverDownloadRuntime,
    track_list: Rc<TrackList>,
    player: Option<Rc<PlayerController>>,
    generation: Cell<u64>,
    progress: Cell<BatchProgress>,
    on_progress: RefCell<Option<OnProgress>>,
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
            on_progress: RefCell::new(None),
        })
    }

    #[allow(dead_code)] // Wired by the Preferences progress row in Task 3.
    pub(super) fn set_on_progress(&self, callback: impl Fn(BatchProgress) + 'static) {
        let callback: OnProgress = Rc::new(callback);
        self.on_progress.borrow_mut().replace(callback.clone());
        callback(self.progress.get());
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
        let callback = self.on_progress.borrow().clone();
        if let Some(callback) = callback {
            callback(progress);
        }
    }
}

fn live_track_paths(conn: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = conn.prepare("SELECT path FROM tracks WHERE missing = 0 ORDER BY path")?;
    let paths = statement.query_map([], |row| row.get(0))?.collect();
    paths
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{live_track_paths, BatchProgress, BatchState};
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
}
