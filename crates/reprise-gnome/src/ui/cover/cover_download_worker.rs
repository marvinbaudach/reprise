//! Dedicated serial worker for automatic online cover downloads. Only plain,
//! `Send` data crosses the thread boundary; GTK objects and textures stay on
//! the main thread in `cover_loader`.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use reprise_core::cover::{read_cover_tag, resolve_source, CoverTag};
use reprise_core::cover_download::{album_key, fetch_and_cache};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) enum DownloadOutcome {
    AlreadyCovered,
    Downloaded(PathBuf),
    Unavailable,
}

pub struct DownloadRequest {
    pub(in crate::ui) track_path: String,
    pub(in crate::ui) skip_if_covered: bool,
    pub(in crate::ui) response: async_channel::Sender<DownloadOutcome>,
}

#[derive(Clone)]
pub struct CoverDownloadRuntime {
    pub(in crate::ui) enabled: Rc<Cell<bool>>,
    pub(in crate::ui) worker: async_channel::Sender<DownloadRequest>,
}

/// Starts the one shared serial worker and seeds its live opt-in state.
pub(in crate::ui) fn setup(conn: &rusqlite::Connection) -> CoverDownloadRuntime {
    let enabled = reprise_core::modules::is_enabled(
        conn,
        &reprise_core::modules::COVER_DOWNLOAD_MODULE,
    )
    .unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read cover-download module state; defaulting to off");
        false
    });
    CoverDownloadRuntime {
        enabled: Rc::new(Cell::new(enabled)),
        worker: spawn(),
    }
}

#[cfg(test)]
pub(in crate::ui) fn setup_for_test() -> CoverDownloadRuntime {
    CoverDownloadRuntime {
        enabled: Rc::new(Cell::new(false)),
        worker: spawn(),
    }
}

impl CoverDownloadRuntime {
    pub(in crate::ui) fn set_enabled(
        &self,
        conn: &rusqlite::Connection,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        reprise_core::modules::set_enabled(
            conn,
            &reprise_core::modules::COVER_DOWNLOAD_MODULE,
            enabled,
        )?;
        self.enabled.set(enabled);
        Ok(())
    }

    pub(in crate::ui) fn try_request(&self, request: DownloadRequest) -> bool {
        self.enabled.get() && self.worker.try_send(request).is_ok()
    }

    pub(in crate::ui) async fn request(&self, request: DownloadRequest) -> bool {
        self.enabled.get() && self.worker.send(request).await.is_ok()
    }
}

pub(in crate::ui) fn spawn() -> async_channel::Sender<DownloadRequest> {
    let (sender, receiver) = async_channel::unbounded::<DownloadRequest>();
    let result = std::thread::Builder::new()
        .name("reprise-cover-download".into())
        .spawn(move || {
            let mut attempted = HashMap::new();
            while let Ok(request) = receiver.recv_blocking() {
                let result = result_for_path(
                    Path::new(&request.track_path),
                    request.skip_if_covered,
                    &mut attempted,
                );
                let _ = request.response.try_send(result);
            }
        });
    if let Err(error) = result {
        tracing::warn!(%error, "could not start cover-download worker");
    }
    sender
}

fn result_for_path(
    track_path: &Path,
    skip_if_covered: bool,
    attempted: &mut HashMap<String, Option<PathBuf>>,
) -> DownloadOutcome {
    if skip_if_covered && resolve_source(track_path).is_some() {
        return DownloadOutcome::AlreadyCovered;
    }
    let tag = read_cover_tag(track_path);
    match result_for_tag(tag, attempted) {
        Some(path) => DownloadOutcome::Downloaded(path),
        None => DownloadOutcome::Unavailable,
    }
}

fn result_for_tag(
    tag: CoverTag,
    attempted: &mut HashMap<String, Option<PathBuf>>,
) -> Option<PathBuf> {
    let (Some(album_artist), Some(album)) = (tag.album_artist, tag.album) else {
        return None;
    };
    if album_artist.trim().is_empty() || album.trim().is_empty() {
        return None;
    }
    let key = album_key(&album_artist, &album);
    if let Some(result) = attempted.get(&key) {
        return result.clone();
    }
    let result = fetch_and_cache(&album_artist, &album, tag.release_mbid.as_deref());
    attempted.insert(key, result.clone());
    result
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::rc::Rc;

    use reprise_core::cover::CoverTag;
    use reprise_core::cover_download::album_key;

    use gtk4::glib;

    use super::{
        result_for_path, result_for_tag, setup, CoverDownloadRuntime, DownloadOutcome,
        DownloadRequest,
    };

    #[test]
    fn net_1_cover_download_respects_the_module() {
        let (worker, receiver) = async_channel::unbounded();
        let runtime = CoverDownloadRuntime {
            enabled: Rc::new(Cell::new(false)),
            worker,
        };
        let request = || {
            let (response, _result) = async_channel::bounded(1);
            DownloadRequest {
                track_path: "/missing.flac".into(),
                skip_if_covered: false,
                response,
            }
        };

        assert!(!runtime.try_request(request()));
        assert!(!glib::MainContext::default().block_on(runtime.request(request())));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn runtime_reads_and_updates_the_live_module_setting() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let runtime = setup(&conn);
        assert!(!runtime.enabled.get());

        runtime.set_enabled(&conn, true).unwrap();
        assert!(runtime.enabled.get());
        assert!(reprise_core::modules::is_enabled(
            &conn,
            &reprise_core::modules::COVER_DOWNLOAD_MODULE
        )
        .unwrap());
    }

    #[test]
    fn batch_request_reports_an_existing_folder_cover_without_fetching() {
        let dir = tempfile::tempdir().unwrap();
        let track = dir.path().join("track.mp3");
        fs::write(
            dir.path().join("cover.jpg"),
            b"not decoded for source resolution",
        )
        .unwrap();
        let mut attempted = HashMap::new();

        assert_eq!(
            result_for_path(&track, true, &mut attempted),
            DownloadOutcome::AlreadyCovered
        );
        assert!(attempted.is_empty());
    }

    #[test]
    fn batch_request_reports_unavailable_for_missing_tags() {
        let mut attempted = HashMap::new();
        assert_eq!(
            result_for_path(
                std::path::Path::new("/does/not/exist.mp3"),
                true,
                &mut attempted
            ),
            DownloadOutcome::Unavailable
        );
        assert!(attempted.is_empty());
    }

    #[test]
    fn missing_tags_do_not_create_an_attempted_album() {
        let mut attempted = HashMap::new();
        assert_eq!(result_for_tag(CoverTag::default(), &mut attempted), None);
        assert!(attempted.is_empty());
    }

    #[test]
    fn an_attempted_album_reuses_its_result_without_fetching_again() {
        let key = album_key("Dedup Artist", "Dedup Album");
        let cached = PathBuf::from("/cached/cover.jpg");
        let mut attempted = HashMap::from([(key, Some(cached.clone()))]);
        let tag = CoverTag {
            picture: None,
            album_artist: Some("Dedup Artist".into()),
            album: Some("Dedup Album".into()),
            release_mbid: None,
        };
        assert_eq!(result_for_tag(tag, &mut attempted), Some(cached));
        assert_eq!(attempted.len(), 1);
    }
}
