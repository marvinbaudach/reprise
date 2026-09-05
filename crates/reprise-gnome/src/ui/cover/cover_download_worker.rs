//! Dedicated serial worker for automatic online cover downloads. Only plain,
//! `Send` data crosses the thread boundary; GTK objects and textures stay on
//! the main thread in `cover_loader`.

use std::cell::Cell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use reprise_core::cover::{read_cover_tag, resolve_source, CoverSource, CoverTag};
use reprise_core::cover_download::{album_key, fetch_and_cache, CoverFetchOutcome};
use reprise_core::db::Db;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) enum DownloadOutcome {
    AlreadyCovered,
    Downloaded(PathBuf),
    Unavailable,
    TransientFailure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CoverStatus {
    Covered,
    NeedsSharedCover {
        also: Option<(String, String, Option<String>)>,
    },
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
                &reprise_core::modules::ARTWORK_MODULE,
            ),
        )),
        worker: spawn(conn.path()),
    }
}

#[cfg(test)]
pub(in crate::ui) fn setup_for_test() -> CoverDownloadRuntime {
    CoverDownloadRuntime {
        enabled: Rc::new(Cell::new(false)),
        worker: spawn(None),
    }
}

impl CoverDownloadRuntime {
    /// `NET-1a`: re-derives `enabled` from the global online-sources gate —
    /// called after the gate toggles on the Online sources page, so a
    /// module that is itself on still stops immediately when the gate goes
    /// off (`SET-4`).
    pub(in crate::ui) fn recompute_enabled(&self, conn: &Db) {
        self.enabled
            .set(reprise_core::online_sources::network_allowed_or_off(
                conn,
                &reprise_core::modules::ARTWORK_MODULE,
            ));
    }

    pub(in crate::ui) fn try_request(&self, request: DownloadRequest) -> bool {
        self.enabled.get() && self.worker.try_send(request).is_ok()
    }

    pub(in crate::ui) async fn request(&self, request: DownloadRequest) -> bool {
        self.enabled.get() && self.worker.send(request).await.is_ok()
    }
}

pub(in crate::ui) fn spawn(
    database_path: Option<PathBuf>,
) -> async_channel::Sender<DownloadRequest> {
    let (sender, receiver) = async_channel::unbounded::<DownloadRequest>();
    let result = std::thread::Builder::new()
        .name("reprise-cover-download".into())
        .spawn(move || {
            let db = open_library(database_path.as_deref());
            let mut attempted = HashMap::new();
            let mut observed_embedded = HashMap::new();
            let mut observed_fingerprints = HashMap::new();
            while let Ok(request) = receiver.recv_blocking() {
                let result = result_for_path(
                    Path::new(&request.track_path),
                    request.skip_if_covered,
                    &mut attempted,
                    &mut observed_embedded,
                    &mut observed_fingerprints,
                    db.as_ref(),
                );
                let _ = request.response.try_send(result);
            }
        });
    if let Err(error) = result {
        tracing::warn!(%error, "could not start cover-download worker");
    }
    sender
}

/// Opens the live library for this worker's one `SELECT` over the album's
/// track directories. Read-only on purpose: nothing here writes, and a
/// background thread holding a writable handle on the user's real library is
/// a hazard the type system can rule out for free.
fn open_library(database_path: Option<&Path>) -> Option<Db> {
    database_path.and_then(|path| {
        Db::open_ready_read_only(path)
            .inspect_err(|error| {
                tracing::warn!(%error, "could not open library for cover writeback");
            })
            .ok()
    })
}

fn result_for_path(
    track_path: &Path,
    skip_if_covered: bool,
    attempted: &mut HashMap<String, CoverFetchOutcome>,
    observed_embedded: &mut HashMap<String, u64>,
    observed_fingerprints: &mut HashMap<u64, (String, String, String, Option<String>)>,
    db: Option<&Db>,
) -> DownloadOutcome {
    let tag = read_cover_tag(track_path);
    let mut also = None;
    if skip_if_covered {
        if let Some(source) = resolve_source(track_path) {
            match cover_status(&tag, &source, observed_embedded, observed_fingerprints) {
                CoverStatus::Covered => return DownloadOutcome::AlreadyCovered,
                CoverStatus::NeedsSharedCover { also: remembered } => also = remembered,
            }
        }
    }
    let also = also.map(|(album_artist, album, release_mbid)| CoverTag {
        picture: None,
        album_artist: Some(album_artist),
        album: Some(album),
        release_mbid,
    });
    let result = fetch_collision_pair(tag, also, &mut |tag| result_for_tag(tag, attempted, db));
    match result {
        CoverFetchOutcome::Downloaded(path) => DownloadOutcome::Downloaded(path),
        CoverFetchOutcome::NotFound => DownloadOutcome::Unavailable,
        CoverFetchOutcome::TransientFailure => DownloadOutcome::TransientFailure,
    }
}

fn fetch_collision_pair(
    primary: CoverTag,
    also: Option<CoverTag>,
    fetch: &mut impl FnMut(CoverTag) -> CoverFetchOutcome,
) -> CoverFetchOutcome {
    if let Some(mirror) = also {
        if fetch(mirror) == CoverFetchOutcome::TransientFailure {
            return CoverFetchOutcome::TransientFailure;
        }
    }
    fetch(primary)
}

fn cover_status(
    tag: &CoverTag,
    _source: &CoverSource,
    observed_embedded: &mut HashMap<String, u64>,
    observed_fingerprints: &mut HashMap<u64, (String, String, String, Option<String>)>,
) -> CoverStatus {
    let Some(bytes) = tag.picture.as_deref() else {
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
    let key = album_key(album_artist, album);
    let album_conflicts = match observed_embedded.entry(key.clone()) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(fingerprint);
            false
        }
        std::collections::hash_map::Entry::Occupied(entry) if *entry.get() == fingerprint => false,
        std::collections::hash_map::Entry::Occupied(_) => true,
    };
    let also = match observed_fingerprints.entry(fingerprint) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert((
                key.clone(),
                album_artist.to_owned(),
                album.to_owned(),
                tag.release_mbid.clone(),
            ));
            None
        }
        std::collections::hash_map::Entry::Occupied(entry) if entry.get().0 == key => None,
        std::collections::hash_map::Entry::Occupied(entry) => Some((
            entry.get().1.clone(),
            entry.get().2.clone(),
            entry.get().3.clone(),
        )),
    };
    if also.is_some() || album_conflicts {
        CoverStatus::NeedsSharedCover { also }
    } else {
        CoverStatus::Covered
    }
}

fn result_for_tag(
    tag: CoverTag,
    attempted: &mut HashMap<String, CoverFetchOutcome>,
    db: Option<&Db>,
) -> CoverFetchOutcome {
    let (Some(album_artist), Some(album)) = (tag.album_artist, tag.album) else {
        return CoverFetchOutcome::NotFound;
    };
    if album_artist.trim().is_empty() || album.trim().is_empty() {
        return CoverFetchOutcome::NotFound;
    }
    let key = album_key(&album_artist, &album);
    if let Some(result) = attempted.get(&key) {
        return result.clone();
    }
    let album_dirs = db
        .map(|db| {
            reprise_core::queries::query_album_directories(db, &album, &album_artist)
                .inspect_err(|error| {
                    tracing::warn!(%error, "could not query album directories for cover writeback");
                })
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let result = fetch_and_cache(
        &album_artist,
        &album,
        tag.release_mbid.as_deref(),
        &album_dirs,
    );
    if result != CoverFetchOutcome::TransientFailure {
        attempted.insert(key, result.clone());
    }
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
    use reprise_core::cover_download::{album_key, CoverFetchOutcome};

    use super::{
        cover_status, fetch_collision_pair, open_library, result_for_path, result_for_tag, setup,
        CoverDownloadRuntime, CoverStatus, DownloadOutcome, DownloadRequest,
    };

    #[test]
    fn cover_1_the_writeback_worker_holds_the_library_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("library.db");
        reprise_core::db::Db::open_migrated(Some(&path)).unwrap();

        let db = open_library(Some(path.as_path())).expect("the worker opens the library");

        assert!(
            reprise_core::queries::query_album_directories(&db, "Album", "Album Artist").is_ok(),
            "the one SELECT this worker runs must still work"
        );
        assert!(
            reprise_core::modules::set_enabled(&db, &reprise_core::modules::ARTWORK_MODULE, true)
                .is_err(),
            "a background worker that only reads must be unable to write"
        );
    }

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
        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE, true)
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
    fn runtime_recomputes_the_live_artwork_setting() {
        let conn = crate::test_db::open().unwrap();
        let runtime = setup(&conn);
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
        let mut observed_fingerprints = HashMap::new();

        assert_eq!(
            result_for_path(
                &track,
                true,
                &mut attempted,
                &mut observed,
                &mut observed_fingerprints,
                None,
            ),
            DownloadOutcome::AlreadyCovered
        );
        assert!(attempted.is_empty());
    }

    #[test]
    fn batch_request_reports_unavailable_for_missing_tags() {
        let mut attempted = HashMap::new();
        let mut observed = HashMap::new();
        let mut observed_fingerprints = HashMap::new();
        assert_eq!(
            result_for_path(
                std::path::Path::new("/does/not/exist.mp3"),
                true,
                &mut attempted,
                &mut observed,
                &mut observed_fingerprints,
                None
            ),
            DownloadOutcome::Unavailable
        );
        assert!(attempted.is_empty());
    }

    #[test]
    fn missing_tags_do_not_create_an_attempted_album() {
        let mut attempted = HashMap::new();
        assert_eq!(
            result_for_tag(CoverTag::default(), &mut attempted, None),
            CoverFetchOutcome::NotFound
        );
        assert!(attempted.is_empty());
    }

    #[test]
    fn an_attempted_album_reuses_its_result_without_fetching_again() {
        let key = album_key("Dedup Artist", "Dedup Album");
        let cached = PathBuf::from("/cached/cover.jpg");
        let mut attempted = HashMap::from([(key, CoverFetchOutcome::Downloaded(cached.clone()))]);
        let tag = CoverTag {
            picture: None,
            album_artist: Some("Dedup Artist".into()),
            album: Some("Dedup Album".into()),
            release_mbid: None,
        };
        assert_eq!(
            result_for_tag(tag, &mut attempted, None),
            CoverFetchOutcome::Downloaded(cached)
        );
        assert_eq!(attempted.len(), 1);
    }

    #[test]
    fn browse_10_same_album_with_different_embedded_art_requires_a_shared_cover() {
        let mut observed = HashMap::new();
        let mut observed_fingerprints = HashMap::new();
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
                &mut observed,
                &mut observed_fingerprints,
            ),
            CoverStatus::Covered
        );
        assert_eq!(
            cover_status(
                &matching_tag,
                &CoverSource::Embedded(vec![1, 2, 3]),
                &mut observed,
                &mut observed_fingerprints,
            ),
            CoverStatus::Covered
        );
        assert_eq!(
            cover_status(
                &second_tag,
                &CoverSource::Embedded(vec![4, 5, 6]),
                &mut observed,
                &mut observed_fingerprints,
            ),
            CoverStatus::NeedsSharedCover { also: None }
        );
    }

    #[test]
    fn browse_10_same_embedded_art_across_album_keys_requires_both_shared_covers() {
        let mut observed_by_album = HashMap::new();
        let mut observed_by_fingerprint = HashMap::new();
        let first_tag = CoverTag {
            picture: Some(vec![1, 2, 3]),
            album_artist: Some("First Artist".into()),
            album: Some("First Album".into()),
            release_mbid: Some("first-release-mbid".into()),
        };
        let second_tag = CoverTag {
            picture: Some(vec![1, 2, 3]),
            album_artist: Some("Second Artist".into()),
            album: Some("Second Album".into()),
            release_mbid: None,
        };

        assert_eq!(
            cover_status(
                &first_tag,
                &CoverSource::Embedded(vec![1, 2, 3]),
                &mut observed_by_album,
                &mut observed_by_fingerprint,
            ),
            CoverStatus::Covered
        );
        assert_eq!(
            cover_status(
                &second_tag,
                &CoverSource::Embedded(vec![1, 2, 3]),
                &mut observed_by_album,
                &mut observed_by_fingerprint,
            ),
            CoverStatus::NeedsSharedCover {
                also: Some((
                    "First Artist".into(),
                    "First Album".into(),
                    Some("first-release-mbid".into()),
                )),
            }
        );
    }

    #[test]
    fn downloaded_cover_keeps_embedded_art_in_cross_album_collision_detection() {
        let mut observed_by_album = HashMap::new();
        let mut observed_by_fingerprint = HashMap::new();
        let covered_tag = CoverTag {
            picture: Some(vec![1, 2, 3]),
            album_artist: Some("Covered Artist".into()),
            album: Some("Covered Album".into()),
            release_mbid: Some("covered-release-mbid".into()),
        };
        let colliding_tag = CoverTag {
            picture: Some(vec![1, 2, 3]),
            album_artist: Some("Retry Artist".into()),
            album: Some("Retry Album".into()),
            release_mbid: None,
        };

        assert_eq!(
            cover_status(
                &covered_tag,
                &CoverSource::FolderImage(PathBuf::from("/cache/covered.jpg")),
                &mut observed_by_album,
                &mut observed_by_fingerprint,
            ),
            CoverStatus::Covered
        );
        assert_eq!(
            cover_status(
                &colliding_tag,
                &CoverSource::Embedded(vec![1, 2, 3]),
                &mut observed_by_album,
                &mut observed_by_fingerprint,
            ),
            CoverStatus::NeedsSharedCover {
                also: Some((
                    "Covered Artist".into(),
                    "Covered Album".into(),
                    Some("covered-release-mbid".into()),
                )),
            }
        );
    }

    #[test]
    fn a_transient_mirror_failure_defers_the_primary_collision_fetch() {
        let primary = CoverTag {
            album_artist: Some("Primary Artist".into()),
            album: Some("Primary Album".into()),
            ..CoverTag::default()
        };
        let mirror = CoverTag {
            album_artist: Some("Mirror Artist".into()),
            album: Some("Mirror Album".into()),
            ..CoverTag::default()
        };
        let mut fetched_albums = Vec::new();

        let result = fetch_collision_pair(primary, Some(mirror), &mut |tag| {
            fetched_albums.push(tag.album.unwrap());
            CoverFetchOutcome::TransientFailure
        });

        assert_eq!(result, CoverFetchOutcome::TransientFailure);
        assert_eq!(fetched_albums, ["Mirror Album"]);
    }

    #[test]
    fn a_definitive_mirror_result_is_followed_by_the_primary_collision_fetch() {
        let primary = CoverTag {
            album: Some("Primary Album".into()),
            ..CoverTag::default()
        };
        let mirror = CoverTag {
            album: Some("Mirror Album".into()),
            ..CoverTag::default()
        };
        let mut fetched_albums = Vec::new();

        let result = fetch_collision_pair(primary, Some(mirror), &mut |tag| {
            let album = tag.album.unwrap();
            fetched_albums.push(album.clone());
            if album == "Mirror Album" {
                CoverFetchOutcome::NotFound
            } else {
                CoverFetchOutcome::Downloaded(PathBuf::from("/cache/primary.jpg"))
            }
        });

        assert_eq!(
            result,
            CoverFetchOutcome::Downloaded(PathBuf::from("/cache/primary.jpg"))
        );
        assert_eq!(fetched_albums, ["Mirror Album", "Primary Album"]);
    }

    #[test]
    fn browse_10_same_embedded_art_within_one_album_stays_covered() {
        let mut observed_by_album = HashMap::new();
        let mut observed_by_fingerprint = HashMap::new();
        let tag = CoverTag {
            picture: Some(vec![1, 2, 3]),
            album_artist: Some("One Artist".into()),
            album: Some("One Album".into()),
            release_mbid: None,
        };

        for _ in 0..2 {
            assert_eq!(
                cover_status(
                    &tag,
                    &CoverSource::Embedded(vec![1, 2, 3]),
                    &mut observed_by_album,
                    &mut observed_by_fingerprint,
                ),
                CoverStatus::Covered
            );
        }
    }

    #[test]
    fn browse_10_embedded_art_without_album_identity_stays_covered() {
        let mut observed_by_album = HashMap::new();
        let mut observed_by_fingerprint = HashMap::new();

        assert_eq!(
            cover_status(
                &CoverTag::default(),
                &CoverSource::Embedded(vec![1, 2, 3]),
                &mut observed_by_album,
                &mut observed_by_fingerprint,
            ),
            CoverStatus::Covered
        );
        assert!(observed_by_album.is_empty());
        assert!(observed_by_fingerprint.is_empty());
    }
}
