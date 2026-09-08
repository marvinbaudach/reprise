use std::path::{Path, PathBuf};
use std::sync::Arc;

use reprise_core::artist_portrait::{
    load_cached_from, verdict, PortraitBackfillListener as CorePortraitBackfillListener,
    PortraitBackfillProgress, PortraitBackfillState, PortraitOutcome,
};
use reprise_core::cover::{self, CoverSource};
use reprise_core::library::source::UnixLibrarySource;

use crate::{AndroidArtworkSize, MusicLibrary};

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
        Self {
            run_id: progress.run_id,
            state: ArtistPortraitProgressState::from(progress.state),
            done: progress.done,
            failed: progress.failed,
            total: progress.total,
        }
    }
}

#[uniffi::export(callback_interface)]
pub trait ArtistPortraitProgressListener: Send + Sync {
    fn on_progress(&self, update: ArtistPortraitProgressUpdate);
}

#[uniffi::export]
impl MusicLibrary {
    pub fn artist_portrait_backfill_progress(&self) -> ArtistPortraitProgressUpdate {
        self.portrait_backfill.progress().into()
    }

    pub fn start_artist_portrait_backfill(
        &self,
        listener: Box<dyn ArtistPortraitProgressListener>,
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

        let forward: Arc<CorePortraitBackfillListener> = Arc::new(move |progress| {
            listener.on_progress(progress.into());
        });
        self.portrait_backfill.start(
            self.database_path.clone(),
            self.portrait_dir(),
            Arc::clone(&self.portrait_fetch),
            forward,
        );
    }

    pub fn cancel_artist_portrait_backfill(&self) {
        self.portrait_backfill.cancel();
    }
}

#[cfg(test)]
#[path = "artist_portrait_tests.rs"]
mod tests;
