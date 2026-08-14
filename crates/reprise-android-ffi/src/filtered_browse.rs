//! Named filtered track windows for the Android library boundary.

use reprise_core::browser::SortDirection;
use reprise_core::queries::{self, LibraryTrackOrder, LibraryTrackRequest, LibraryTrackScope};

use crate::{AlbumWindow, LibraryError, MusicLibrary, TrackWindow, WindowRange};

#[uniffi::export]
impl MusicLibrary {
    /// Returns one artist's albums in newest-first order.
    #[allow(clippy::needless_pass_by_value)] // UniFFI owns exported strings.
    pub fn list_artist_albums(
        &self,
        artist: String,
        window: WindowRange,
    ) -> Result<AlbumWindow, LibraryError> {
        let reader = self.reader()?;
        queries::query_artist_albums(&reader, artist.as_str(), window.into())
            .map(AlbumWindow::from)
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })
    }

    /// Returns one artist's present tracks with no album tag.
    #[allow(clippy::needless_pass_by_value)] // UniFFI owns exported strings.
    pub fn list_artist_untagged_tracks(
        &self,
        artist: String,
        window: WindowRange,
    ) -> Result<TrackWindow, LibraryError> {
        let reader = self.reader()?;
        queries::query_artist_untagged_tracks(&reader, artist.as_str(), window.into())
            .map(TrackWindow::from)
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })
    }

    /// Returns one artist's present tracks in album and track-number order.
    pub fn list_artist_tracks(
        &self,
        artist: String,
        window: WindowRange,
    ) -> Result<TrackWindow, LibraryError> {
        let reader = self.reader()?;
        queries::query_library_tracks(
            &reader,
            &LibraryTrackRequest {
                scope: LibraryTrackScope::Artist { artist },
                search: String::new(),
                order: LibraryTrackOrder::Sorted {
                    field: "album".to_owned(),
                    direction: SortDirection::Ascending,
                },
                window: window.into(),
            },
        )
        .map(TrackWindow::from)
        .map_err(|error| LibraryError::Query {
            detail: error.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{MusicLibrary, WindowRange};

    fn filtered_library() -> (tempfile::TempDir, MusicLibrary) {
        let directory = tempfile::tempdir().unwrap();
        let music = directory.path().join("music");
        std::fs::create_dir(&music).unwrap();
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../android/app/src/main/assets/sine.flac");
        for (name, title, artist, album_artist, album, track_no) in [
            ("zulu-2.flac", "Gamma", "Ada", "Ada", "Zulu", 2),
            ("alpha-2.flac", "Beta", "Guest", "Ada", "Alpha", 2),
            ("other.flac", "Other", "Bela", "Bela", "Middle", 1),
            ("alpha-1.flac", "Omega", " aDa ", " ADA ", "Alpha", 1),
            ("zulu-1.flac", "Alpha", "Ada", "Ada", "Zulu", 1),
            ("loose.flac", "Loose", "Ada", "Ada", "", 0),
        ] {
            let path = music.join(name);
            std::fs::copy(&fixture, &path).unwrap();
            reprise_core::library::tag_edit::apply_patch_to_file(
                &path,
                &reprise_core::library::tag_edit::TagPatch {
                    title: Some(title.to_owned()),
                    artist: Some(artist.to_owned()),
                    album: Some(album.to_owned()),
                    album_artist: Some(album_artist.to_owned()),
                    track_no: Some(Some(track_no)),
                    ..reprise_core::library::tag_edit::TagPatch::default()
                },
            )
            .unwrap();
        }
        let database_path = directory.path().join("reprise.db");
        let database = reprise_core::db::Db::open_migrated(Some(&database_path)).unwrap();
        reprise_core::library::scanner::scan_folder(&database, &music).unwrap();
        for (title, rating) in [("Gamma", 5), ("Beta", 4), ("Other", 5), ("Omega", 5)] {
            let track = reprise_core::queries::query_library_text_search(
                &database,
                title,
                reprise_core::queries::WindowRange {
                    offset: 0,
                    limit: 1,
                },
            )
            .unwrap()
            .rows
            .remove(0);
            reprise_core::library::stats::set_rating(&database, track.id, rating).unwrap();
        }
        drop(database);
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();
        (directory, library)
    }

    #[test]
    fn artist_window_matches_the_effective_artist_tile_identity_and_order() {
        let (_directory, library) = filtered_library();
        let artists = library
            .list_artists(WindowRange {
                offset: 0,
                limit: 20,
            })
            .unwrap();
        let ada = artists
            .rows
            .iter()
            .find(|artist| artist.artist.trim().eq_ignore_ascii_case("Ada"))
            .unwrap();

        let window = library
            .list_artist_tracks(
                ada.artist.clone(),
                WindowRange {
                    offset: 0,
                    limit: 20,
                },
            )
            .unwrap();

        assert_eq!(ada.track_count, 5);
        assert_eq!(window.total, ada.track_count);
        assert_eq!(window.rows.len() as i64, window.total);
        assert!(!window.has_more);
        assert_eq!(
            window
                .rows
                .iter()
                .map(|track| (
                    track.artist.as_str(),
                    track.album.as_str(),
                    track.title.as_str()
                ))
                .collect::<Vec<_>>(),
            [
                ("Ada", "", "Loose"),
                (" aDa ", "Alpha", "Omega"),
                ("Guest", "Alpha", "Beta"),
                ("Ada", "Zulu", "Alpha"),
                ("Ada", "Zulu", "Gamma"),
            ]
        );
    }

    /// The tile's number has to survive paging.
    ///
    /// The test above asks for the whole artist at once, so a `total` that
    /// merely counted the returned page would agree with it by accident. A
    /// short window is the only shape that tells the two apart — and the
    /// artist window reaches its count through a different call than the
    /// favourites do, so proving it there proves nothing here.
    #[test]
    fn a_short_artist_window_still_counts_every_track_the_tile_promised() {
        let (_directory, library) = filtered_library();

        let window = library
            .list_artist_tracks(
                "Ada".to_owned(),
                WindowRange {
                    offset: 1,
                    limit: 2,
                },
            )
            .unwrap();

        assert_eq!(window.total, 5);
        assert_eq!(window.rows.len(), 2);
        assert!(window.has_more);
    }

    #[test]
    fn artist_values_without_matches_are_empty_windows_not_errors() {
        let (_directory, library) = filtered_library();

        for artist in ["Nobody", ""] {
            let window = library
                .list_artist_tracks(
                    artist.to_owned(),
                    WindowRange {
                        offset: 0,
                        limit: 2,
                    },
                )
                .unwrap();

            assert_eq!(window.total, 0);
            assert!(window.rows.is_empty());
            assert!(!window.has_more);
        }
    }

    #[test]
    fn artist_album_windows_keep_the_artist_filter_total_and_page_in_step() {
        let (_directory, library) = filtered_library();

        let window = library
            .list_artist_albums(
                "Ada".to_owned(),
                WindowRange {
                    offset: 0,
                    limit: 1,
                },
            )
            .unwrap();

        assert_eq!(window.total, 2);
        assert_eq!(window.rows.len(), 1);
        assert!(window.has_more);
        assert_eq!(window.rows[0].album, "Alpha");
        assert!(window
            .rows
            .iter()
            .all(|album| album.album_artist.trim().eq_ignore_ascii_case("Ada")));
    }

    #[test]
    fn unknown_artists_are_empty_album_windows_not_errors() {
        let (_directory, library) = filtered_library();

        let window = library
            .list_artist_albums(
                "Nobody".to_owned(),
                WindowRange {
                    offset: 0,
                    limit: 2,
                },
            )
            .unwrap();

        assert_eq!(window.total, 0);
        assert!(window.rows.is_empty());
        assert!(!window.has_more);
    }

    #[test]
    fn artist_untagged_windows_return_only_loose_tracks_and_empty_cleanly() {
        let (_directory, library) = filtered_library();

        let ada = library
            .list_artist_untagged_tracks(
                "Ada".to_owned(),
                WindowRange {
                    offset: 0,
                    limit: 20,
                },
            )
            .unwrap();
        let bela = library
            .list_artist_untagged_tracks(
                "Bela".to_owned(),
                WindowRange {
                    offset: 0,
                    limit: 20,
                },
            )
            .unwrap();

        assert_eq!(ada.total, 1);
        assert_eq!(
            ada.rows
                .iter()
                .map(|track| (track.title.as_str(), track.album.as_str()))
                .collect::<Vec<_>>(),
            [("Loose", "")]
        );
        assert!(!ada.has_more);
        assert_eq!(bela.total, 0);
        assert!(bela.rows.is_empty());
        assert!(!bela.has_more);
    }
}
