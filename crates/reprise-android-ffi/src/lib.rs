//! Minimal Android library surface over `reprise-core`.

use reprise_core::db::Db;
use reprise_core::library::scanner::{scan_folder_with_source_and_progress, ScanOutcome};
use reprise_core::library::settings;
use reprise_core::queries;
use source::{BridgedSource, SafSource};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod appearance;
mod artist_portrait;
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
mod online_sources;
mod play_journal;
mod play_recorder;
pub mod playback;
mod playback_session;
mod playback_settings;
#[cfg(test)]
mod read_during_scan_tests;
pub mod source;
mod source_error;
mod source_names;
#[cfg(test)]
mod source_tests;
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
use library_types::{ConfiguredTree, PortraitFetch, DATABASE_FILE_NAME};
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

impl MusicLibrary {
    fn open_with_portrait_fetcher(
        app_private_directory: &str,
        app_cache_directory: &str,
        portrait_fetch: Arc<PortraitFetch>,
    ) -> Result<Self, LibraryError> {
        let db_path = Path::new(app_private_directory).join(DATABASE_FILE_NAME);
        let writer = Db::open_migrated(Some(&db_path)).map_err(|error| LibraryError::Database {
            detail: error.to_string(),
        })?;
        // The migrating writer must establish the current schema before the
        // non-migrating reader asserts that the database is ready.
        let reader = Db::open_ready(&db_path).map_err(|error| LibraryError::Database {
            detail: error.to_string(),
        })?;
        Ok(Self {
            writer: Mutex::new(writer),
            reader: Mutex::new(reader),
            tree: Mutex::new(None),
            cache_root: PathBuf::from(app_cache_directory),
            database_path: db_path,
            portrait_fetch,
        })
    }

    #[cfg(test)]
    pub(crate) fn open_with_portrait_fetch(
        app_private_directory: &str,
        app_cache_directory: &str,
        fetch: impl Fn(
                &str,
                &Path,
            ) -> Result<
                reprise_core::artist_portrait::PortraitOutcome,
                reprise_core::artist_portrait::PortraitError,
            > + Send
            + Sync
            + 'static,
    ) -> Result<Self, LibraryError> {
        Self::open_with_portrait_fetcher(
            app_private_directory,
            app_cache_directory,
            Arc::new(fetch),
        )
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
        Self::open_with_portrait_fetcher(
            app_private_directory,
            app_cache_directory,
            Arc::new(reprise_core::artist_portrait::load_or_fetch_in),
        )
    }

    pub fn set_tree_uri(
        &self,
        tree_uri: String,
        source: Box<dyn SafSource>,
    ) -> Result<(), LibraryError> {
        let writer = self.writer()?;
        settings::set_library_root(&writer, &tree_uri).map_err(|error| LibraryError::Database {
            detail: error.to_string(),
        })?;
        let mut tree = self.tree.lock().map_err(|_| LibraryError::Database {
            detail: "library handle poisoned by an earlier panic".to_owned(),
        })?;
        *tree = Some(ConfiguredTree {
            uri: tree_uri.clone().into(),
            source: Arc::new(BridgedSource::with_tree_root(source, tree_uri)),
        });
        Ok(())
    }

    pub fn scan(
        &self,
        progress: Box<dyn ScanProgressListener>,
    ) -> Result<ScanSummary, LibraryError> {
        let writer = self.writer()?;
        let (tree_uri, source) = self.configured_tree()?;
        let outcome =
            scan_folder_with_source_and_progress(source.as_ref(), &writer, &tree_uri, |event| {
                progress.on_progress(event.into());
            });
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
        let reader = self.reader()?;
        queries::query_library_text_search(&reader, "", window.into())
            .map(TrackWindow::from)
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })
    }

    pub fn search_albums(
        &self,
        text: &str,
        window: WindowRange,
    ) -> Result<AlbumWindow, LibraryError> {
        let reader = self.reader()?;
        queries::query_albums(&reader, bounded_search_text(text), window.into())
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
        let reader = self.reader()?;
        queries::query_artists(&reader, bounded_search_text(text), window.into())
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
        let reader = self.reader()?;
        queries::query_album_tracks(&reader, &album, &album_artist, window.into())
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
        let reader = self.reader()?;
        queries::query_library_metadata_text_search(
            &reader,
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
        let (_, source) = self.configured_tree()?;
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
