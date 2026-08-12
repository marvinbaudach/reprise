//! Minimal Android library surface over `reprise-core`.

use reprise_core::db::Db;
use reprise_core::library::scanner::{scan_folder_with_source_and_progress, ScanOutcome};
use reprise_core::library::settings;
use reprise_core::queries;
use source::{BridgedSource, SafSource};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod appearance;
#[cfg(test)]
mod artwork_tests;
mod browse;
mod fallback_cover;
mod filtered_browse;
mod library_listen_report;
mod library_types;
mod listen_export_journal;
#[cfg(test)]
mod listen_export_journal_tests;
mod listen_export_recorder;
#[cfg(test)]
mod log_capture;
mod logging;
mod mobile_sync;
mod play_journal;
mod play_recorder;
pub mod playback;
mod playback_session;
mod playback_settings;
pub mod source;
mod source_error;
mod source_names;
mod track_analysis;
mod visualizer;
#[cfg(test)]
mod visualizer_tests;
pub use appearance::*;
pub use browse::{
    AlbumRow, AlbumWindow, ArtistRow, ArtistWindow, TrackRow, TrackWindow, WindowRange,
};
pub use fallback_cover::*;
pub use library_types::{
    AndroidArtworkSize, LibraryError, MusicLibrary, ScanProgressListener, ScanProgressUpdate,
    ScanSummary,
};
use library_types::{ConfiguredTree, LibraryState, DATABASE_FILE_NAME};
pub use logging::init_logging;
pub use playback_session::{
    AndroidPlaybackListener, AndroidPlaybackSession, AndroidPlaybackSnapshot, AndroidRepeatMode,
    AndroidTrashFailure, AndroidTrashReport, TrashAction,
};
pub use playback_settings::*;
pub use visualizer::*;
uniffi::setup_scaffolding!();

/// The longest search text this boundary passes on.
///
/// Every search is typed into a phone's search field, so a few hundred
/// characters is already far past anything a listener means by it — while the
/// `%text%` pattern behind it costs a full scan of the table. The cut is taken
/// on a character boundary rather than a byte one, so a clipped query is still
/// valid UTF-8 and still means what its beginning meant.
const MAX_SEARCH_TEXT_CHARS: usize = 256;

/// Borrows the search text the queries should see: the caller's own string,
/// clipped to [`MAX_SEARCH_TEXT_CHARS`]. Borrowed rather than copied, because
/// every one of these calls sits in front of a keystroke.
pub(crate) fn bounded_search_text(text: &str) -> &str {
    match text.char_indices().nth(MAX_SEARCH_TEXT_CHARS) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

#[uniffi::export]
impl MusicLibrary {
    /// Opens the library database inside the app's private directory.
    #[uniffi::constructor]
    pub fn open(
        app_private_directory: &str,
        app_cache_directory: &str,
    ) -> Result<Self, LibraryError> {
        let db_path = Path::new(app_private_directory).join(DATABASE_FILE_NAME);
        let db = Db::open_migrated(Some(&db_path)).map_err(|error| LibraryError::Database {
            detail: error.to_string(),
        })?;
        Ok(Self {
            state: Mutex::new(LibraryState { db, tree: None }),
            cache_root: PathBuf::from(app_cache_directory),
            database_path: db_path,
        })
    }

    pub fn set_tree_uri(
        &self,
        tree_uri: String,
        source: Box<dyn SafSource>,
    ) -> Result<(), LibraryError> {
        let mut state = self.lock()?;
        settings::set_library_root(&state.db, &tree_uri).map_err(|error| {
            LibraryError::Database {
                detail: error.to_string(),
            }
        })?;
        state.tree = Some(ConfiguredTree {
            uri: tree_uri.into(),
            source: Arc::new(BridgedSource::new(source)),
        });
        Ok(())
    }

    pub fn scan(
        &self,
        progress: Box<dyn ScanProgressListener>,
    ) -> Result<ScanSummary, LibraryError> {
        let state = self.lock()?;
        let tree = state.tree.as_ref().ok_or(LibraryError::TreeNotConfigured)?;
        let outcome = scan_folder_with_source_and_progress(
            tree.source.as_ref(),
            &state.db,
            &tree.uri,
            |event| {
                progress.on_progress(event.into());
            },
        );
        drop(progress);
        let outcome = outcome.map_err(|error| LibraryError::Scan {
            detail: error.to_string(),
        })?;
        match outcome {
            ScanOutcome::Completed(report) => Ok(ScanSummary {
                added: report.added,
                updated: report.updated,
                errors: report.errors,
            }),
            ScanOutcome::RootUnavailable { root } => Err(LibraryError::Scan {
                detail: format!("root unavailable: {}", root.display()),
            }),
        }
    }

    pub fn list_tracks(&self, window: WindowRange) -> Result<TrackWindow, LibraryError> {
        let state = self.lock()?;
        queries::query_library_text_search(&state.db, "", window.into())
            .map(TrackWindow::from)
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })
    }

    pub fn list_albums(&self, window: WindowRange) -> Result<AlbumWindow, LibraryError> {
        self.search_albums("", window)
    }

    pub fn search_albums(
        &self,
        text: &str,
        window: WindowRange,
    ) -> Result<AlbumWindow, LibraryError> {
        let state = self.lock()?;
        queries::query_albums(&state.db, bounded_search_text(text), window.into())
            .map(AlbumWindow::from)
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })
    }

    pub fn list_artists(&self, window: WindowRange) -> Result<ArtistWindow, LibraryError> {
        self.search_artists("", window)
    }

    pub fn search_artists(
        &self,
        text: &str,
        window: WindowRange,
    ) -> Result<ArtistWindow, LibraryError> {
        let state = self.lock()?;
        queries::query_artists(&state.db, bounded_search_text(text), window.into())
            .map(ArtistWindow::from)
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })
    }

    pub fn list_album_tracks(
        &self,
        album: String,
        album_artist: String,
        window: WindowRange,
    ) -> Result<TrackWindow, LibraryError> {
        let album = album.into_boxed_str();
        let album_artist = album_artist.into_boxed_str();
        let state = self.lock()?;
        queries::query_album_tracks(&state.db, &album, &album_artist, window.into())
            .map(TrackWindow::from)
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })
    }

    pub fn search_tracks(
        &self,
        text: &str,
        window: WindowRange,
    ) -> Result<TrackWindow, LibraryError> {
        let state = self.lock()?;
        queries::query_library_metadata_text_search(
            &state.db,
            bounded_search_text(text),
            window.into(),
        )
        .map(TrackWindow::from)
        .map_err(|error| LibraryError::Query {
            detail: error.to_string(),
        })
    }

    /// Resolves local artwork lazily for one track and returns its cached
    /// measured-size thumbnail path.
    ///
    /// The two answers are kept apart, the way every sibling method on this
    /// object keeps them apart:
    ///
    /// * `Ok(None)` — **this track has no artwork**. A missing picture, or one
    ///   that does not decode, is an ordinary answer the UI renders as the
    ///   no-artwork symbol. It is not an error and Kotlin must not treat it as
    ///   one.
    /// * `Err(_)` — the library itself could not answer: a handle poisoned by
    ///   an earlier panic, or no configured tree to read covers through. Those
    ///   are the same conditions `set_track_rating`, `scan`, `list_*` and
    ///   `search_tracks` all report as a typed [`LibraryError`], and folding
    ///   them into the `None` that means "no cover" is what made a broken
    ///   library indistinguishable from a picture-less one.
    ///
    /// A full disk still answers `Ok(None)` — the cover cache being unwritable
    /// affects the *thumbnail*, not the library, and every following track will
    /// fail the same way — but it says so in the log rather than passing
    /// silently.
    pub fn track_artwork(
        &self,
        track_uri: &str,
        size: AndroidArtworkSize,
    ) -> Result<Option<String>, LibraryError> {
        let source = {
            let state = self.lock()?;
            let tree = state.tree.as_ref().ok_or(LibraryError::TreeNotConfigured)?;
            Arc::clone(&tree.source)
        };
        let Some(cover) = reprise_core::cover::resolve_source_with_source(
            source.as_ref(),
            Path::new(&track_uri),
            &self.cache_root,
        ) else {
            return Ok(None);
        };
        match reprise_core::cover::thumbnail_with_source(
            source.as_ref(),
            &cover,
            size.thumbnail_size(),
            &self.cache_root,
        ) {
            Ok(path) => Ok(Some(path.to_string_lossy().into_owned())),
            // The cache is unusable: every following track will fail the same
            // way, so this is the one worth waking someone up for.
            Err(error @ reprise_core::cover::CoverError::Io(_)) => {
                tracing::warn!(%error, track = track_uri, "no artwork: cover cache unusable");
                Ok(None)
            }
            // One unrenderable image among many. Expected in the wild, so it
            // stays at debug.
            Err(error) => {
                tracing::debug!(%error, track = track_uri, "no artwork: cover did not decode");
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
