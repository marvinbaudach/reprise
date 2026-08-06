//! Named filtered track windows for the Android library boundary.

use reprise_core::queries::{self, BrowseFilter};
use reprise_core::view_source::ViewSource;

use crate::{LibraryError, MusicLibrary, TrackRow, TrackWindow, WindowRange};

fn filtered_track_window(
    library: &MusicLibrary,
    browse: &BrowseFilter,
    sort_field: &str,
    window: WindowRange,
) -> Result<TrackWindow, LibraryError> {
    let state = library.lock()?;
    let total =
        queries::query_track_count_browsed(&state.db, &ViewSource::Library, "", browse, &[])
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })?;
    let rows = queries::query_track_window_browsed(
        &state.db,
        &ViewSource::Library,
        sort_field,
        "asc",
        "",
        browse,
        window.offset,
        window.limit,
        &[],
    )
    .map_err(|error| LibraryError::Query {
        detail: error.to_string(),
    })?;
    let returned = i64::try_from(rows.len()).unwrap_or(i64::MAX);
    Ok(TrackWindow {
        total,
        has_more: window.offset.max(0).saturating_add(returned) < total,
        rows: rows.into_iter().map(TrackRow::from).collect(),
    })
}

#[uniffi::export]
impl MusicLibrary {
    /// Returns one artist's present tracks in album and track-number order.
    pub fn list_artist_tracks(
        &self,
        artist: String,
        window: WindowRange,
    ) -> Result<TrackWindow, LibraryError> {
        filtered_track_window(
            self,
            &BrowseFilter {
                artist: Some(artist),
                ..BrowseFilter::default()
            },
            "album",
            window,
        )
    }

    /// Returns present tracks rated exactly five in artist, album, and
    /// track-number order.
    pub fn list_favourites(&self, window: WindowRange) -> Result<TrackWindow, LibraryError> {
        filtered_track_window(
            self,
            &BrowseFilter {
                rating: Some("5".to_owned()),
                ..BrowseFilter::default()
            },
            "artist",
            window,
        )
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
        for (name, title, artist, album, track_no) in [
            ("zulu-2.flac", "Gamma", "Ada", "Zulu", 2),
            ("alpha-2.flac", "Beta", "Ada", "Alpha", 2),
            ("other.flac", "Other", "Bela", "Middle", 1),
            ("alpha-1.flac", "Omega", "Ada", "Alpha", 1),
            ("zulu-1.flac", "Alpha", "Ada", "Zulu", 1),
        ] {
            let path = music.join(name);
            std::fs::copy(&fixture, &path).unwrap();
            reprise_core::library::tag_edit::apply_patch_to_file(
                &path,
                &reprise_core::library::tag_edit::TagPatch {
                    title: Some(title.to_owned()),
                    artist: Some(artist.to_owned()),
                    album: Some(album.to_owned()),
                    album_artist: Some(artist.to_owned()),
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
    fn artist_window_is_filtered_ordered_and_counted_before_paging() {
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

        assert_eq!(window.total, 4);
        assert!(window.has_more);
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
            [("Ada", "Alpha", "Beta"), ("Ada", "Zulu", "Alpha"),]
        );
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
    fn favourites_are_exactly_fives_in_artist_album_track_order() {
        let (_directory, library) = filtered_library();

        let window = library
            .list_favourites(WindowRange {
                offset: 0,
                limit: 2,
            })
            .unwrap();

        assert_eq!(window.total, 3);
        assert!(window.has_more);
        assert!(window.rows.iter().all(|track| track.rating == 5));
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
            [("Ada", "Alpha", "Omega"), ("Ada", "Zulu", "Gamma")]
        );
        assert!(window.rows.iter().all(|track| track.title != "Beta"));
    }

    #[test]
    fn empty_favourites_are_an_empty_window_not_an_error() {
        let (_directory, library) = filtered_library();
        for track in library
            .list_tracks(WindowRange {
                offset: 0,
                limit: 20,
            })
            .unwrap()
            .rows
        {
            library.set_track_rating(track.id, 0).unwrap();
        }

        let window = library
            .list_favourites(WindowRange {
                offset: 0,
                limit: 2,
            })
            .unwrap();

        assert_eq!(window.total, 0);
        assert!(window.rows.is_empty());
        assert!(!window.has_more);
    }
}
