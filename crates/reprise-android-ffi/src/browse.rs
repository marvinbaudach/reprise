//! Owned UniFFI records for bounded library browsing.

use reprise_core::queries;

use crate::{LibraryError, MusicLibrary};

/// One row as the UI needs it — deliberately not the full Core `Track`, so
/// the binding surface stays a decision rather than an accident.
///
/// # Two identities, and which one a new method takes
///
/// A row carries *both* [`id`](Self::id) and [`uri`](Self::uri), and they are
/// not interchangeable. A method that picks the wrong one is not a compile
/// error and usually not a visible bug either — it just quietly breaks the
/// first time a rescan runs. So the rule is:
///
/// * **`id` addresses the library row.** Anything that reads or writes what
///   Reprise knows *about* a track takes the id: `MusicLibrary::set_track_rating`
///   and `AndroidPlaybackSession::play_tracks`, whose queue has to keep pointing
///   at the same rows while it plays. The id survives a rescan; a moved or
///   renamed file keeps its row.
/// * **`uri` addresses the bytes on the device.** Anything that has to open the
///   file through the document provider takes the uri:
///   `MusicLibrary::track_artwork` reads the track's embedded picture and its
///   album folder, which no row id can locate. A rescan may hand out a
///   different uri for the same id.
///
/// The one place both appear together is `play_tracks`, which takes the ids to
/// count plays against and the uris to hand Media3 — the same row, addressed
/// twice, on purpose.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TrackRow {
    /// Stable across a rescan. The key for everything the library stores about
    /// this track — see the type's documentation.
    pub id: i64,
    /// Where the bytes are. The key for everything that has to open the file —
    /// see the type's documentation.
    pub uri: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    pub play_count: i64,
    pub rating: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AlbumRow {
    pub album: String,
    pub album_artist: String,
    pub representative_uri: String,
    pub track_count: i64,
    pub year: Option<i32>,
    pub total_duration_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ArtistRow {
    pub artist: String,
    pub track_count: i64,
    pub album_count: i64,
    pub representative_uri: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct WindowRange {
    pub offset: i64,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TrackWindow {
    pub total: i64,
    pub rows: Vec<TrackRow>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AlbumWindow {
    pub total: i64,
    pub rows: Vec<AlbumRow>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ArtistWindow {
    pub total: i64,
    pub rows: Vec<ArtistRow>,
    pub has_more: bool,
}

impl From<reprise_core::models::Track> for TrackRow {
    fn from(track: reprise_core::models::Track) -> Self {
        Self {
            id: track.id,
            uri: track.path,
            title: track.title,
            artist: track.artist,
            album: track.album,
            duration_ms: track.duration_ms,
            play_count: track.play_count,
            rating: track.rating,
        }
    }
}

impl From<WindowRange> for queries::WindowRange {
    fn from(window: WindowRange) -> Self {
        Self {
            offset: window.offset,
            limit: window.limit,
        }
    }
}

impl From<queries::TrackWindow> for TrackWindow {
    fn from(window: queries::TrackWindow) -> Self {
        Self {
            total: window.total,
            rows: window.rows.into_iter().map(TrackRow::from).collect(),
            has_more: window.has_more,
        }
    }
}

impl From<queries::AlbumWindow> for AlbumWindow {
    fn from(window: queries::AlbumWindow) -> Self {
        Self {
            total: window.total,
            rows: window
                .rows
                .into_iter()
                .map(|album| AlbumRow {
                    album: album.album,
                    album_artist: album.album_artist,
                    representative_uri: album.representative_path,
                    track_count: album.track_count,
                    year: album.year,
                    total_duration_ms: album.total_duration_ms,
                })
                .collect(),
            has_more: window.has_more,
        }
    }
}

impl From<queries::ArtistWindow> for ArtistWindow {
    fn from(window: queries::ArtistWindow) -> Self {
        Self {
            total: window.total,
            rows: window
                .rows
                .into_iter()
                .map(|artist| ArtistRow {
                    artist: artist.artist,
                    track_count: artist.track_count,
                    album_count: artist.album_count,
                    representative_uri: artist.representative_path,
                })
                .collect(),
            has_more: window.has_more,
        }
    }
}

#[uniffi::export]
impl MusicLibrary {
    /// Loads one present track by its stable library identity.
    pub fn track_by_id(&self, track_id: i64) -> Result<Option<TrackRow>, LibraryError> {
        let reader = self.reader()?;
        queries::query_present_track_by_id(&reader, track_id)
            .map(|track| track.map(TrackRow::from))
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })
    }

    /// Returns every present track in one album in canonical disc/track order.
    pub fn album_track_ids(
        &self,
        album: String,
        album_artist: String,
    ) -> Result<Vec<i64>, LibraryError> {
        let album = album.into_boxed_str();
        let album_artist = album_artist.into_boxed_str();
        let reader = self.reader()?;
        queries::query_album_canonical_track_ids(&reader, &album, &album_artist).map_err(|error| {
            LibraryError::Query {
                detail: error.to_string(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MusicLibrary;

    #[test]
    fn the_android_library_loads_one_present_track_by_stable_id() {
        let directory = tempfile::tempdir().unwrap();
        let music = directory.path().join("music");
        std::fs::create_dir(&music).unwrap();
        let track_path = music.join("playing.flac");
        std::fs::copy(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../android/app/src/main/assets/sine.flac"),
            &track_path,
        )
        .unwrap();
        let database_path = directory.path().join("reprise.db");
        let database = reprise_core::db::Db::open_migrated(Some(&database_path)).unwrap();
        reprise_core::library::scanner::scan_folder(&database, &music).unwrap();
        let expected = queries::query_library_text_search(
            &database,
            "",
            queries::WindowRange {
                offset: 0,
                limit: 1,
            },
        )
        .unwrap()
        .rows
        .remove(0);
        drop(database);
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();

        let playing = library.track_by_id(expected.id).unwrap().unwrap();

        assert_eq!(playing, TrackRow::from(expected));
        assert_eq!(library.track_by_id(999).unwrap(), None);
    }

    #[test]
    fn album_track_ids_cross_the_browse_window_in_canonical_order() {
        const TRACK_COUNT: usize = 501;
        let directory = tempfile::tempdir().unwrap();
        let music = directory.path().join("music");
        std::fs::create_dir(&music).unwrap();
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../android/app/src/main/assets/sine.flac");
        let templates = directory.path().join("templates");
        std::fs::create_dir(&templates).unwrap();
        let template = |track_no: u32| {
            let path = templates.join(format!("track-{track_no}.flac"));
            std::fs::copy(&fixture, &path).unwrap();
            reprise_core::library::tag_edit::apply_patch_to_file(
                &path,
                &reprise_core::library::tag_edit::TagPatch {
                    title: Some(format!("Track {track_no}")),
                    artist: Some("Window Artist".into()),
                    album: Some("Window Album".into()),
                    album_artist: Some("Window Artist".into()),
                    year: Some(Some(2026)),
                    track_no: Some(Some(track_no)),
                    genre: Some("Test".into()),
                },
            )
            .unwrap();
            path
        };
        let first = template(1);
        let second = template(2);
        for index in 0..TRACK_COUNT {
            std::fs::copy(
                if index % 2 == 0 { &second } else { &first },
                music.join(format!("track-{index:03}.flac")),
            )
            .unwrap();
        }
        let database_path = directory.path().join(crate::DATABASE_FILE_NAME);
        let database = reprise_core::db::Db::open_migrated(Some(&database_path)).unwrap();
        reprise_core::library::scanner::scan_folder(&database, &music).unwrap();
        drop(database);
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();
        let page = |offset| {
            library
                .list_album_tracks(
                    "Window Album".into(),
                    "Window Artist".into(),
                    WindowRange { offset, limit: 500 },
                )
                .unwrap()
        };
        let first_page = page(0);
        let second_page = page(500);
        assert!(first_page.has_more);
        let expected = first_page
            .rows
            .into_iter()
            .chain(second_page.rows)
            .map(|track| track.id)
            .collect::<Vec<_>>();

        assert_eq!(
            library
                .album_track_ids("window album".into(), "window artist".into())
                .unwrap(),
            expected
        );
        assert_eq!(
            library
                .album_track_ids("Unknown".into(), "Nobody".into())
                .unwrap(),
            Vec::<i64>::new()
        );
    }
}
