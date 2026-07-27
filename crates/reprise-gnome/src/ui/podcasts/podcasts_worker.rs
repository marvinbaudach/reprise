//! Long-lived podcast refresh and download worker.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::podcasts;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PodcastsOperation {
    Refresh { force: bool },
    Download { episode_id: i64 },
}

#[derive(Debug)]
pub(in crate::ui) struct PodcastsRequest {
    pub generation: u64,
    pub operation: PodcastsOperation,
    pub response: async_channel::Sender<PodcastsResponse>,
}

#[derive(Debug)]
pub(in crate::ui) struct PodcastsResponse {
    pub generation: u64,
    pub result: Result<PodcastsWorkerResult, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PodcastsWorkerResult {
    Refreshed(podcasts::pipeline::RefreshSummary),
    Downloaded(i64),
}

type OnEnabled = Rc<dyn Fn(bool)>;

pub(in crate::ui) struct PodcastsRuntime {
    pub enabled: Rc<Cell<bool>>,
    worker: async_channel::Sender<PodcastsRequest>,
    subscribers: RefCell<Vec<OnEnabled>>,
}

impl PodcastsRuntime {
    pub(in crate::ui) fn setup(conn: &rusqlite::Connection) -> Rc<Self> {
        let enabled =
            reprise_core::modules::is_enabled(conn, &reprise_core::modules::PODCASTS_MODULE)
                .unwrap_or(false);
        Rc::new(Self {
            enabled: Rc::new(Cell::new(enabled)),
            worker: spawn(database_path(conn)),
            subscribers: RefCell::new(Vec::new()),
        })
    }

    pub(in crate::ui) fn set_enabled(
        &self,
        conn: &rusqlite::Connection,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        reprise_core::modules::set_enabled(conn, &reprise_core::modules::PODCASTS_MODULE, enabled)?;
        if self.enabled.replace(enabled) != enabled {
            for callback in self.subscribers.borrow().iter() {
                callback(enabled);
            }
        }
        Ok(())
    }

    pub(in crate::ui) fn subscribe_enabled(&self, callback: impl Fn(bool) + 'static) {
        let callback: OnEnabled = Rc::new(callback);
        callback(self.enabled.get());
        self.subscribers.borrow_mut().push(callback);
    }

    pub(in crate::ui) fn request(&self, request: PodcastsRequest) -> bool {
        if !self.enabled.get() {
            return false;
        }
        match self.worker.try_send(request) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "could not queue podcast work");
                false
            }
        }
    }

    pub(in crate::ui) fn automatic_refresh_allowed(
        &self,
        subscription_count: usize,
        metered: bool,
        due: bool,
    ) -> bool {
        automatic_refresh_allowed(self.enabled.get(), subscription_count, metered, due)
    }
}

pub(in crate::ui) fn automatic_refresh_allowed(
    enabled: bool,
    subscription_count: usize,
    metered: bool,
    due: bool,
) -> bool {
    enabled && subscription_count > 0 && !metered && due
}

fn database_path(conn: &rusqlite::Connection) -> Option<PathBuf> {
    let mut statement = conn.prepare("PRAGMA database_list").ok()?;
    let mut rows = statement.query([]).ok()?;
    while let Some(row) = rows.next().ok()? {
        if row.get::<_, String>(1).ok()?.as_str() == "main" {
            let path = row.get::<_, String>(2).ok()?;
            return (!path.is_empty()).then(|| PathBuf::from(path));
        }
    }
    None
}

fn spawn(database_path: Option<PathBuf>) -> async_channel::Sender<PodcastsRequest> {
    let (sender, receiver) = async_channel::unbounded::<PodcastsRequest>();
    let result = std::thread::Builder::new()
        .name("reprise-podcasts".into())
        .spawn(move || {
            let connection = database_path
                .as_deref()
                .map(|path| reprise_core::db::open_migrated(Some(path)));
            while let Ok(request) = receiver.recv_blocking() {
                let result = match connection.as_ref() {
                    Some(Ok(conn)) => run(conn, request.operation),
                    Some(Err(error)) => Err(error.to_string()),
                    None => Err("the active database has no persistent path".into()),
                };
                let _ = request.response.try_send(PodcastsResponse {
                    generation: request.generation,
                    result,
                });
            }
        });
    if let Err(error) = result {
        tracing::warn!(%error, "could not start podcast worker");
    }
    sender
}

fn run(
    conn: &rusqlite::Connection,
    operation: PodcastsOperation,
) -> Result<PodcastsWorkerResult, String> {
    match operation {
        PodcastsOperation::Refresh { force } => {
            let config = podcasts::config::load(conn).map_err(|error| error.to_string())?;
            let ytdlp = podcasts::ytdlp::YtDlp::discover(config.ytdlp_path.as_deref());
            podcasts::pipeline::refresh(
                conn,
                &podcasts::pipeline::HttpFeedFetcher,
                &ytdlp,
                chrono::Utc::now().timestamp(),
                force,
            )
            .map(PodcastsWorkerResult::Refreshed)
            .map_err(|error| error.to_string())
        }
        PodcastsOperation::Download { episode_id } => {
            download_episode(conn, episode_id)?;
            Ok(PodcastsWorkerResult::Downloaded(episode_id))
        }
    }
}

fn download_episode(conn: &rusqlite::Connection, episode_id: i64) -> Result<(), String> {
    use podcasts::pipeline::FeedFetcher;

    let episode = podcasts::store::episode(conn, episode_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "podcast episode no longer exists".to_owned())?;
    let subscription = podcasts::store::subscription(conn, episode.subscription_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "podcast subscription no longer exists".to_owned())?;
    let extension = match episode.kind {
        podcasts::PodcastKind::Rss => podcasts::downloads::extension_from_url(&episode.audio_url),
        podcasts::PodcastKind::Youtube => "audio",
    };
    let destination = podcasts::downloads::download_path(
        &podcasts::downloads::default_download_root(),
        &subscription.feed_url,
        &episode.guid,
        extension,
    );
    podcasts::downloads::prepare_destination(&destination).map_err(|error| error.to_string())?;
    match episode.kind {
        podcasts::PodcastKind::Rss => {
            podcasts::pipeline::HttpFeedFetcher
                .download(&episode.audio_url, &destination)
                .map_err(|error| error.to_string())?;
        }
        podcasts::PodcastKind::Youtube => {
            let config = podcasts::config::load(conn).map_err(|error| error.to_string())?;
            podcasts::ytdlp::YtDlp::discover(config.ytdlp_path.as_deref())
                .download(&episode.audio_url, &destination)
                .map_err(|error| error.to_string())?;
        }
    }
    let bytes = std::fs::metadata(&destination)
        .map_err(|error| error.to_string())?
        .len()
        .min(i64::MAX as u64) as i64;
    podcasts::store::set_downloaded_file(conn, episode.id, destination.to_str(), Some(bytes))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_refresh_requires_every_gate() {
        assert!(automatic_refresh_allowed(true, 1, false, true));
        assert!(!automatic_refresh_allowed(false, 1, false, true));
        assert!(!automatic_refresh_allowed(true, 0, false, true));
        assert!(!automatic_refresh_allowed(true, 1, true, true));
        assert!(!automatic_refresh_allowed(true, 1, false, false));
    }
}
