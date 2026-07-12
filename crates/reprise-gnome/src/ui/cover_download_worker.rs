//! Dedicated serial worker for opt-in online cover downloads. Only plain,
//! `Send` data crosses the thread boundary; GTK objects and textures stay on
//! the main thread in `cover_loader`.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use reprise_core::cover::{read_cover_tag, CoverTag};
use reprise_core::cover_download::{album_key, fetch_and_cache};

pub struct DownloadRequest {
    pub(super) track_path: String,
    pub(super) response: async_channel::Sender<Option<PathBuf>>,
}

#[derive(Clone)]
pub struct CoverDownloadRuntime {
    pub(super) enabled: Rc<Cell<bool>>,
    pub(super) worker: async_channel::Sender<DownloadRequest>,
}

/// Reads the persisted module flag and starts the one shared serial worker.
/// The idle worker performs no I/O and cannot touch the network until a
/// `CoverLoader` posts a request while this flag is enabled.
pub(super) fn setup(conn: &rusqlite::Connection) -> CoverDownloadRuntime {
    let enabled = reprise_core::modules::is_enabled(
        conn,
        &reprise_core::modules::COVER_DOWNLOAD_MODULE,
    )
    .unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read module.cover_download.enabled; defaulting to off");
        false
    });
    CoverDownloadRuntime {
        enabled: Rc::new(Cell::new(enabled)),
        worker: spawn(),
    }
}

pub(super) fn spawn() -> async_channel::Sender<DownloadRequest> {
    let (sender, receiver) = async_channel::unbounded::<DownloadRequest>();
    let result = std::thread::Builder::new()
        .name("reprise-cover-download".into())
        .spawn(move || {
            let mut attempted = HashMap::new();
            while let Ok(request) = receiver.recv_blocking() {
                let tag = read_cover_tag(Path::new(&request.track_path));
                let result = result_for_tag(tag, &mut attempted);
                let _ = request.response.try_send(result);
            }
        });
    if let Err(error) = result {
        tracing::warn!(%error, "could not start cover-download worker");
    }
    sender
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
    use std::collections::HashMap;
    use std::path::PathBuf;

    use reprise_core::cover::CoverTag;
    use reprise_core::cover_download::album_key;

    use super::result_for_tag;

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
