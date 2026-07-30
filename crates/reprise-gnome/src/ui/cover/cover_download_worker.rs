//! Dedicated serial worker for automatic online cover downloads. Only plain,
//! `Send` data crosses the thread boundary; GTK objects and textures stay on
//! the main thread in `cover_loader`.

use std::cell::Cell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use reprise_core::cover::{read_cover_tag, resolve_source, CoverSource, CoverTag};
use reprise_core::cover_download::{album_key, fetch_and_cache};
use reprise_core::db::Db;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) enum DownloadOutcome {
    AlreadyCovered,
    Downloaded(PathBuf),
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CoverStatus {
    Covered,
    NeedsSharedCover,
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
pub(in crate::ui) fn setup(conn: &Db) -> CoverDownloadRuntime {
    CoverDownloadRuntime {
        enabled: Rc::new(Cell::new(
            reprise_core::online_sources::network_allowed_or_off(
                conn,
                &reprise_core::modules::COVER_DOWNLOAD_MODULE,
            ),
        )),
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
        conn: &Db,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        reprise_core::modules::set_enabled(
            conn,
            &reprise_core::modules::COVER_DOWNLOAD_MODULE,
            enabled,
        )?;
        self.enabled
            .set(reprise_core::online_sources::network_allowed_or_off(
                conn,
                &reprise_core::modules::COVER_DOWNLOAD_MODULE,
            ));
        Ok(())
    }

    /// `NET-1a`: re-derives `enabled` from the global online-sources gate —
    /// called after the gate toggles on the Online sources page, so a
    /// module that is itself on still stops immediately when the gate goes
    /// off (`SET-4`).
    pub(in crate::ui) fn recompute_enabled(&self, conn: &Db) {
        self.enabled
            .set(reprise_core::online_sources::network_allowed_or_off(
                conn,
                &reprise_core::modules::COVER_DOWNLOAD_MODULE,
            ));
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
            let mut observed_embedded = HashMap::new();
            while let Ok(request) = receiver.recv_blocking() {
                let result = result_for_path(
                    Path::new(&request.track_path),
                    request.skip_if_covered,
                    &mut attempted,
                    &mut observed_embedded,
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
    observed_embedded: &mut HashMap<String, u64>,
) -> DownloadOutcome {
    let tag = read_cover_tag(track_path);
    if skip_if_covered {
        if let Some(source) = resolve_source(track_path) {
            if cover_status(&tag, &source, observed_embedded) == CoverStatus::Covered {
                return DownloadOutcome::AlreadyCovered;
            }
        }
    }
    match result_for_tag(tag, attempted) {
        Some(path) => DownloadOutcome::Downloaded(path),
        None => DownloadOutcome::Unavailable,
    }
}

fn cover_status(
    tag: &CoverTag,
    source: &CoverSource,
    observed_embedded: &mut HashMap<String, u64>,
) -> CoverStatus {
    let CoverSource::Embedded(bytes) = source else {
        return CoverStatus::Covered;
    };
    let (Some(album_artist), Some(album)) = (tag.album_artist.as_deref(), tag.album.as_deref())
    else {
        return CoverStatus::Covered;
    };
    if album_artist.trim().is_empty() || album.trim().is_empty() {
        return CoverStatus::Covered;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let fingerprint = hasher.finish();
    match observed_embedded.entry(album_key(album_artist, album)) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(fingerprint);
            CoverStatus::Covered
        }
        std::collections::hash_map::Entry::Occupied(entry) if *entry.get() == fingerprint => {
            CoverStatus::Covered
        }
        std::collections::hash_map::Entry::Occupied(_) => CoverStatus::NeedsSharedCover,
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
    use std::future::Future;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::task::{Context, Poll, Waker};

    use reprise_core::cover::{CoverSource, CoverTag};
    use reprise_core::cover_download::album_key;

    use super::{
        cover_status, result_for_path, result_for_tag, setup, CoverDownloadRuntime, CoverStatus,
        DownloadOutcome, DownloadRequest,
    };

    #[test]
    fn net_1a_cover_download_respects_the_module() {
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
        let mut future = std::pin::pin!(runtime.request(request()));
        let mut context = Context::from_waker(Waker::noop());
        assert_eq!(future.as_mut().poll(&mut context), Poll::Ready(false));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn net_1a_recompute_enabled_reflects_the_global_gate() {
        let conn = crate::test_db::open().unwrap();
        reprise_core::modules::set_enabled(
            &conn,
            &reprise_core::modules::COVER_DOWNLOAD_MODULE,
            true,
        )
        .unwrap();
        let runtime = setup(&conn);
        assert!(runtime.enabled.get());

        reprise_core::online_sources::set_enabled(&conn, false).unwrap();
        runtime.recompute_enabled(&conn);
        assert!(
            !runtime.enabled.get(),
            "global gate off must disable dispatch even with the module on"
        );

        reprise_core::online_sources::set_enabled(&conn, true).unwrap();
        runtime.recompute_enabled(&conn);
        assert!(runtime.enabled.get());
    }

    #[test]
    fn runtime_reads_and_updates_the_live_module_setting() {
        let conn = crate::test_db::open().unwrap();
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
        let mut observed = HashMap::new();

        assert_eq!(
            result_for_path(&track, true, &mut attempted, &mut observed),
            DownloadOutcome::AlreadyCovered
        );
        assert!(attempted.is_empty());
    }

    #[test]
    fn batch_request_reports_unavailable_for_missing_tags() {
        let mut attempted = HashMap::new();
        let mut observed = HashMap::new();
        assert_eq!(
            result_for_path(
                std::path::Path::new("/does/not/exist.mp3"),
                true,
                &mut attempted,
                &mut observed
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

    #[test]
    fn browse_10_same_album_with_different_embedded_art_requires_a_shared_cover() {
        let mut observed = HashMap::new();
        let first_tag = CoverTag {
            picture: Some(vec![1, 2, 3]),
            album_artist: Some("Consistency Artist".into()),
            album: Some("Consistency Album".into()),
            release_mbid: None,
        };
        let matching_tag = CoverTag {
            picture: Some(vec![1, 2, 3]),
            album_artist: Some("  consistency   artist ".into()),
            album: Some("CONSISTENCY ALBUM".into()),
            release_mbid: None,
        };
        let second_tag = CoverTag {
            picture: Some(vec![4, 5, 6]),
            album_artist: Some("consistency artist".into()),
            album: Some(" consistency  album ".into()),
            release_mbid: None,
        };

        assert_eq!(
            cover_status(
                &first_tag,
                &CoverSource::Embedded(vec![1, 2, 3]),
                &mut observed
            ),
            CoverStatus::Covered
        );
        assert_eq!(
            cover_status(
                &matching_tag,
                &CoverSource::Embedded(vec![1, 2, 3]),
                &mut observed
            ),
            CoverStatus::Covered
        );
        assert_eq!(
            cover_status(
                &second_tag,
                &CoverSource::Embedded(vec![4, 5, 6]),
                &mut observed
            ),
            CoverStatus::NeedsSharedCover
        );
    }
}
