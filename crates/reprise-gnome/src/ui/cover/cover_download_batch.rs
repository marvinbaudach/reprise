use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::glib;
use reprise_core::db::Db;

use super::cover_download_worker::{CoverDownloadRuntime, DownloadOutcome, DownloadRequest};
use super::player_controller::PlayerController;
use super::progress_subscribers::ProgressSubscribers;
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

pub(in crate::ui) struct CoverDownloadBatch {
    conn: Rc<Db>,
    runtime: CoverDownloadRuntime,
    track_list: Rc<TrackList>,
    player: Option<Rc<PlayerController>>,
    generation: Cell<u64>,
    progress: Cell<BatchProgress>,
    progress_subscribers: ProgressSubscribers<BatchProgress>,
}

impl CoverDownloadBatch {
    pub(in crate::ui) fn new(
        conn: &Rc<Db>,
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
        if !self.runtime.enabled.get() {
            self.set_progress(BatchProgress::idle());
            return;
        }
        let paths = {
            let conn = &self.conn;
            reprise_core::queries::query_live_track_paths(conn)
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
            // Every track the download side has already settled is dropped
            // here, before anything opens a file. Asking the worker means it
            // reads the track's tags to work out which album to look up, and
            // this batch runs over the whole library on every launch — so
            // without this filter a settled library re-reads every file it
            // owns, every time, to be told what it was told last time.
            let paths = {
                let candidates = paths.clone();
                gtk4::gio::spawn_blocking(move || {
                    candidates
                        .into_iter()
                        .filter(|path| {
                            !reprise_core::cover::download_marked_unavailable(
                                std::path::Path::new(path),
                                reprise_core::cover::ThumbnailSize::List,
                            )
                        })
                        .collect::<Vec<String>>()
                })
                .await
                .unwrap_or(paths)
            };
            if this.generation.get() != generation {
                return;
            }

            for path in &paths {
                if this.generation.get() != generation {
                    return;
                }
                let (response, result) = async_channel::bounded(1);
                if !this
                    .runtime
                    .request(DownloadRequest {
                        track_path: path.clone(),
                        skip_if_covered: true,
                        response,
                    })
                    .await
                {
                    let progress = if this.runtime.enabled.get() {
                        BatchProgress::failed()
                    } else {
                        BatchProgress::idle()
                    };
                    this.set_progress(progress);
                    return;
                }
                let outcome = result.recv().await.unwrap_or(DownloadOutcome::Unavailable);
                if this.generation.get() != generation {
                    return;
                }
                // Settled either way: covered already, or nothing to be had.
                // Both mean the next launch has no reason to open this file.
                if matches!(
                    outcome,
                    DownloadOutcome::AlreadyCovered | DownloadOutcome::Unavailable
                ) {
                    let settled = path.clone();
                    gtk4::gio::spawn_blocking(move || {
                        reprise_core::cover::remember_download_unavailable(
                            std::path::Path::new(&settled),
                            reprise_core::cover::ThumbnailSize::List,
                        );
                    })
                    .await
                    .ok();
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
    use std::path::PathBuf;

    use super::{BatchProgress, BatchState};
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
        let conn = crate::test_db::open().unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO tracks (path, title, added_at) \
             VALUES ('/music/b.mp3', 'B', 1)",
                [],
            )
            .unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO tracks (path, title, added_at) \
             VALUES ('/music/a.mp3', 'A', 1)",
                [],
            )
            .unwrap();
        crate::test_db::connection(&conn)
            .execute(
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
}
