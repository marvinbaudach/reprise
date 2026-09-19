use std::path::{Path, PathBuf};
use std::sync::Arc;

use reprise_core::artist_portrait::{
    load_cached_from, verdict, CoverBackfillFetch, CoverBackfillListener, CoverBackfillProgress,
    PortraitBackfillListener as CorePortraitBackfillListener, PortraitBackfillProgress,
    PortraitBackfillState, PortraitOutcome,
};
use reprise_core::cover::{self, CoverSource};
use reprise_core::cover_download::{self, CoverFetchOutcome};
use reprise_core::db::Db;
use reprise_core::library::source::UnixLibrarySource;

use crate::{AndroidArtworkSize, MusicLibrary};

mod album_cover;

impl MusicLibrary {
    pub(crate) fn portrait_dir(&self) -> PathBuf {
        self.cache_root.join("artist-portraits")
    }

    fn reduced_portrait_path(&self, path: &Path, size: AndroidArtworkSize) -> Option<String> {
        match cover::thumbnail_with_source(
            &UnixLibrarySource,
            &CoverSource::FolderImage(path.to_owned()),
            size.thumbnail_size(),
            &self.cache_root,
        ) {
            Ok(path) => Some(path.to_string_lossy().into_owned()),
            Err(error @ cover::CoverError::Io(_)) => {
                tracing::debug!(%error, "no artist portrait: cover cache unusable");
                None
            }
            Err(error) => {
                tracing::debug!(%error, "no artist portrait: image did not decode");
                None
            }
        }
    }
}

#[uniffi::export]
impl MusicLibrary {
    pub fn artists_missing_portraits(
        &self,
        limit: u32,
    ) -> Result<Vec<String>, crate::LibraryError> {
        let allowed = {
            let reader = self.reader()?;
            reprise_core::online_sources::network_allowed_or_off(
                &reader,
                &reprise_core::modules::ARTWORK_MODULE,
            )
        };
        if !allowed {
            return Ok(Vec::new());
        }

        let wanted = limit as usize;
        if wanted == 0 {
            return Ok(Vec::new());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs() as i64);
        let portrait_dir = self.portrait_dir();
        let mut missing = Vec::new();
        let mut offset = 0;
        loop {
            let window = {
                let reader = self.reader()?;
                reprise_core::queries::query_artists(
                    &reader,
                    "",
                    reprise_core::queries::WindowRange {
                        offset,
                        limit: i64::MAX,
                    },
                )
            }
            .map_err(|error| crate::LibraryError::Query {
                detail: error.to_string(),
            })?;
            let returned = window.rows.len();
            for artist in window.rows {
                if verdict(&portrait_dir, &artist.artist, now).needs_fetch() {
                    missing.push(artist.artist);
                    if missing.len() == wanted {
                        return Ok(missing);
                    }
                }
            }
            if !window.has_more || returned == 0 {
                return Ok(missing);
            }
            offset = offset.saturating_add(i64::try_from(returned).unwrap_or(i64::MAX));
        }
    }
}

#[uniffi::export]
impl MusicLibrary {
    pub fn artist_portrait_cached(&self, name: &str, size: AndroidArtworkSize) -> Option<String> {
        match load_cached_from(name, &self.portrait_dir()) {
            PortraitOutcome::Found(path) => self.reduced_portrait_path(&path, size),
            PortraitOutcome::NotFound => None,
        }
    }

    pub fn artist_portrait_fetch(
        &self,
        name: &str,
        size: AndroidArtworkSize,
    ) -> Result<Option<String>, crate::LibraryError> {
        let allowed = {
            let reader = self.reader()?;
            reprise_core::online_sources::network_allowed_or_off(
                &reader,
                &reprise_core::modules::ARTWORK_MODULE,
            )
        };
        if !allowed {
            return Ok(None);
        }

        match (self.portrait_fetch)(name, &self.portrait_dir()) {
            Ok(PortraitOutcome::Found(path)) => Ok(self.reduced_portrait_path(&path, size)),
            Ok(PortraitOutcome::NotFound) => Ok(None),
            Err(error) => {
                tracing::debug!(%error, "artist portrait request failed");
                Ok(None)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ArtistPortraitProgressUpdate {
    pub run_id: u64,
    pub state: ArtistPortraitProgressState,
    pub done: u32,
    pub failed: u32,
    pub total: u32,
    /// The cover pass that rides this same handle after the portraits
    /// (B3): `0`/`0` before it has started, whether because the portrait
    /// run is not `Complete` yet or because no tree was configured to fetch
    /// covers through.
    pub covers_done: u32,
    pub covers_total: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ArtistPortraitProgressState {
    Preparing,
    Running,
    Paused,
    Complete,
}

impl From<PortraitBackfillState> for ArtistPortraitProgressState {
    fn from(state: PortraitBackfillState) -> Self {
        match state {
            PortraitBackfillState::Preparing => Self::Preparing,
            PortraitBackfillState::Running => Self::Running,
            PortraitBackfillState::Paused => Self::Paused,
            PortraitBackfillState::Complete => Self::Complete,
        }
    }
}

impl From<PortraitBackfillProgress> for ArtistPortraitProgressUpdate {
    fn from(progress: PortraitBackfillProgress) -> Self {
        merged_progress_update(progress, CoverBackfillProgress::default())
    }
}

/// Combines a portrait-run snapshot with the cover pass riding beside it
/// into the one update Kotlin's listener sees (decision 9: "same handle,
/// same progress").
///
/// The portrait state is `Complete` the instant the portraits finish, but
/// the merged `total` grows again right after as the cover worklist lands —
/// reporting `Complete` through that would read as a finished bar whose
/// count then runs backwards. While the cover pass still has work left,
/// this reports `Running` instead; the portrait state (including a real
/// `Complete`, once covers have none left either) passes through untouched
/// otherwise.
fn merged_progress_update(
    portrait: PortraitBackfillProgress,
    covers: CoverBackfillProgress,
) -> ArtistPortraitProgressUpdate {
    let state = if covers.total > 0 && covers.done < covers.total {
        ArtistPortraitProgressState::Running
    } else {
        ArtistPortraitProgressState::from(portrait.state)
    };
    ArtistPortraitProgressUpdate {
        run_id: portrait.run_id,
        state,
        done: portrait.done,
        failed: portrait.failed,
        total: portrait.total,
        covers_done: covers.done,
        covers_total: covers.total,
    }
}

#[uniffi::export(callback_interface)]
pub trait ArtistPortraitProgressListener: Send + Sync {
    fn on_progress(&self, update: ArtistPortraitProgressUpdate);
}

#[uniffi::export]
impl MusicLibrary {
    pub fn artist_portrait_backfill_progress(&self) -> ArtistPortraitProgressUpdate {
        merged_progress_update(
            self.portrait_backfill.progress(),
            album_cover::cover_backfill().progress(),
        )
    }

    pub fn start_artist_portrait_backfill(
        &self,
        listener: Box<dyn ArtistPortraitProgressListener>,
    ) {
        let cache_root = self.cache_root.clone();
        self.start_artist_portrait_backfill_with(
            listener,
            Arc::new(move |album_artist, album, mbid| {
                cover_download::fetch_and_cache_in(&cache_root, album_artist, album, mbid, &[])
            }),
        );
    }

    pub fn cancel_artist_portrait_backfill(&self) {
        self.portrait_backfill.cancel();
        album_cover::cover_backfill().cancel();
    }
}

/// `(album_artist, album, mbid)` — the network-shaped step of a cover
/// fetch, post local-resolution, the same shape as `album_cover.rs`'s
/// `album_cover_fetch_with` takes (there as `&dyn Fn`; `Arc` here since
/// this one outlives the call, held inside the `forward` closure).
type NetworkCoverFetch = dyn Fn(&str, &str, Option<&str>) -> CoverFetchOutcome + Send + Sync;

impl MusicLibrary {
    /// `start_artist_portrait_backfill` with its network-shaped step
    /// (post local-resolution, same shape as `album_cover_fetch_with`'s
    /// `fetch`) injectable — a test-only seam so a test can prove the
    /// chain never reaches the real MusicBrainz/CAA network for an album
    /// that resolves locally (B3 review finding 2), the same seam
    /// `album_cover_fetch` already has through `album_cover_fetch_with`.
    fn start_artist_portrait_backfill_with(
        &self,
        listener: Box<dyn ArtistPortraitProgressListener>,
        network_fetch: Arc<NetworkCoverFetch>,
    ) {
        let allowed = match self.reader() {
            Ok(reader) => reprise_core::online_sources::network_allowed_or_off(
                &reader,
                &reprise_core::modules::ARTWORK_MODULE,
            ),
            Err(error) => {
                tracing::warn!(%error, "artist portrait backfill could not check its network gate");
                false
            }
        };
        if !allowed {
            return;
        }

        let listener: Arc<dyn ArtistPortraitProgressListener> = Arc::from(listener);
        let database_path = self.database_path.clone();
        let cache_root = self.cache_root.clone();
        // Only the FFI knows how to open a track's bytes (SAF); a
        // MusicLibrary with no configured tree yet simply never chains a
        // cover pass — the portrait run still proceeds on its own.
        let tree_source = self.configured_tree().ok().map(|(_, source)| source);

        let forward_listener = Arc::clone(&listener);
        let forward: Arc<CorePortraitBackfillListener> = Arc::new(move |progress| {
            let just_completed =
                progress.state == PortraitBackfillState::Complete && progress.run_id != 0;
            let will_chain = just_completed && tree_source.is_some();
            if will_chain {
                // A cover pass is about to start riding this same
                // completion: pushing the raw `Complete` here would be
                // revoked the instant that pass's own `Running` update
                // lands, reading to Kotlin as a finished bar whose count
                // then runs backwards (`merged_progress_update`'s own
                // invariant, violated at exactly this call site — B3
                // review finding 1). Report `Running` instead; the cover
                // pass's own listener (below) reports the real terminal
                // state once it actually knows one, including if `start`
                // never gets to run at all (`finish_without_run`).
                forward_listener.on_progress(ArtistPortraitProgressUpdate {
                    state: ArtistPortraitProgressState::Running,
                    ..progress.into()
                });
            } else {
                forward_listener.on_progress(progress.into());
            }
            if !just_completed {
                return;
            }
            let Some(source) = tree_source.clone() else {
                return;
            };

            let fetch_cache_root = cache_root.clone();
            let fetch_source = Arc::clone(&source);
            let network_fetch = Arc::clone(&network_fetch);
            let fetch: Arc<CoverBackfillFetch> =
                Arc::new(move |album_artist, album, representative_uri| {
                    let path = std::path::Path::new(representative_uri);
                    let locally_resolved = cover::resolve_source_with_source(
                        fetch_source.as_ref(),
                        path,
                        &fetch_cache_root,
                    )
                    .is_some();
                    if locally_resolved {
                        // Local art already resolves: settled without a
                        // request (decision 9). The cover pass only counts
                        // albums, it never reads this path back — a
                        // placeholder marks "no fetch needed" rather than
                        // claiming a made-up cache path is real.
                        return CoverFetchOutcome::Downloaded(fetch_cache_root.clone());
                    }
                    let mbid =
                        cover::read_cover_tag_with_source(fetch_source.as_ref(), path).release_mbid;
                    network_fetch(album_artist, album, mbid.as_deref())
                });

            let cover_listener_target = Arc::clone(&listener);
            let portrait_snapshot = progress;
            let cover_listener: Arc<CoverBackfillListener> = Arc::new(move |covers| {
                cover_listener_target
                    .on_progress(merged_progress_update(portrait_snapshot, covers));
            });

            let consent_database = database_path.clone();
            let consent_allowed: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
                Db::open_ready(&consent_database).is_ok_and(|db| {
                    reprise_core::online_sources::network_allowed_or_off(
                        &db,
                        &reprise_core::modules::ARTWORK_MODULE,
                    )
                })
            });

            album_cover::cover_backfill().start(
                database_path.clone(),
                fetch,
                cover_listener,
                consent_allowed,
            );
        });
        self.portrait_backfill.start(
            self.database_path.clone(),
            self.portrait_dir(),
            Arc::clone(&self.portrait_fetch),
            forward,
        );
    }
}

#[cfg(test)]
#[path = "artist_portrait_tests.rs"]
mod tests;
