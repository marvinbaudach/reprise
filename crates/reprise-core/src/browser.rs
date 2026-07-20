//! Pure browser vocabulary shared by every frontend.
//!
//! A [`BrowserPlace`] is a navigable place. Album and artist are scopes of
//! the Library collection, never parallel presentation modes. Each track
//! place owns its refinements and stable view bookmarks so history entries
//! cannot leak filters, selection, or scroll state into one another.

use serde::{Deserialize, Serialize};

use crate::queries::BrowseFilter;
use crate::view_source::ViewSource;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AlbumKey {
    pub album: String,
    pub album_artist: String,
}

impl PartialEq for AlbumKey {
    fn eq(&self, other: &Self) -> bool {
        normalized_identity(&self.album) == normalized_identity(&other.album)
            && normalized_identity(&self.album_artist) == normalized_identity(&other.album_artist)
    }
}

impl Eq for AlbumKey {}

impl AlbumKey {
    #[must_use]
    pub fn new(album: impl Into<String>, album_artist: impl Into<String>) -> Self {
        Self {
            album: album.into().trim().to_owned(),
            album_artist: album_artist.into().trim().to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtistKey {
    pub artist: String,
}

impl PartialEq for ArtistKey {
    fn eq(&self, other: &Self) -> bool {
        normalized_identity(&self.artist) == normalized_identity(&other.artist)
    }
}

impl Eq for ArtistKey {}

fn normalized_identity(value: &str) -> String {
    value.trim().to_lowercase()
}

impl ArtistKey {
    #[must_use]
    pub fn new(artist: impl Into<String>) -> Self {
        Self {
            artist: artist.into().trim().to_owned(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LibraryScope {
    #[default]
    All,
    Album(AlbumKey),
    Artist(ArtistKey),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackCollection {
    Library(LibraryScope),
    Playlist(i64),
    Smart(i64),
    Queue,
    Missing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackSort {
    pub field: String,
    pub direction: SortDirection,
}

impl TrackSort {
    #[must_use]
    pub fn new(field: impl Into<String>, direction: SortDirection) -> Self {
        Self {
            field: field.into(),
            direction,
        }
    }
}

impl Default for TrackSort {
    fn default() -> Self {
        Self::new("artist", SortDirection::Ascending)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackAnchor {
    pub track_id: i64,
    pub row_offset: f64,
}

impl TrackAnchor {
    #[must_use]
    pub fn new(track_id: i64, row_offset: f64) -> Self {
        Self {
            track_id,
            row_offset: if row_offset.is_finite() {
                row_offset.max(0.0)
            } else {
                0.0
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackFocus {
    Track(i64),
    #[default]
    Content,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackViewState {
    pub search: String,
    pub browse: BrowseFilter,
    pub sort: TrackSort,
    pub anchor: Option<TrackAnchor>,
    pub selected_ids: Vec<i64>,
    pub focus: TrackFocus,
}

impl Default for TrackViewState {
    fn default() -> Self {
        Self {
            search: String::new(),
            browse: BrowseFilter::default(),
            sort: TrackSort::default(),
            anchor: None,
            selected_ids: Vec::new(),
            focus: TrackFocus::Content,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackPlace {
    pub collection: TrackCollection,
    pub state: TrackViewState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BrowserPlace {
    Tracks(Box<TrackPlace>),
    ImportErrors,
    MyStats,
    Device { serial: String },
}

impl BrowserPlace {
    #[must_use]
    pub fn tracks(collection: TrackCollection, state: TrackViewState) -> Self {
        Self::Tracks(Box::new(TrackPlace { collection, state }))
    }

    #[must_use]
    pub fn fresh_album(album: impl Into<String>, album_artist: impl Into<String>) -> Self {
        Self::tracks(
            TrackCollection::Library(LibraryScope::Album(AlbumKey::new(album, album_artist))),
            TrackViewState::default(),
        )
    }

    #[must_use]
    pub fn collection(&self) -> Option<&TrackCollection> {
        match self {
            Self::Tracks(place) => Some(&place.collection),
            Self::ImportErrors | Self::MyStats | Self::Device { .. } => None,
        }
    }

    #[must_use]
    pub fn track_state(&self) -> Option<&TrackViewState> {
        match self {
            Self::Tracks(place) => Some(&place.state),
            Self::ImportErrors | Self::MyStats | Self::Device { .. } => None,
        }
    }

    #[must_use]
    pub fn view_source(&self) -> ViewSource {
        match self {
            Self::Tracks(place) => match &place.collection {
                TrackCollection::Library(LibraryScope::All) => ViewSource::Library,
                TrackCollection::Library(LibraryScope::Album(key)) => ViewSource::Album {
                    album: key.album.clone(),
                    album_artist: key.album_artist.clone(),
                },
                TrackCollection::Library(LibraryScope::Artist(key)) => {
                    ViewSource::Artist(key.artist.clone())
                }
                TrackCollection::Playlist(id) => ViewSource::Playlist(*id),
                TrackCollection::Smart(id) => ViewSource::Smart(*id),
                TrackCollection::Queue => ViewSource::Queue,
                TrackCollection::Missing => ViewSource::Missing,
            },
            Self::ImportErrors => ViewSource::ImportErrors,
            Self::MyStats => ViewSource::MyStats,
            Self::Device { serial } => ViewSource::Device {
                serial: serial.clone(),
            },
        }
    }
}

impl From<ViewSource> for BrowserPlace {
    fn from(source: ViewSource) -> Self {
        let collection = match source {
            ViewSource::Library => TrackCollection::Library(LibraryScope::All),
            ViewSource::Playlist(id) => TrackCollection::Playlist(id),
            ViewSource::Smart(id) => TrackCollection::Smart(id),
            ViewSource::Queue => TrackCollection::Queue,
            ViewSource::Missing => TrackCollection::Missing,
            ViewSource::ImportErrors => return Self::ImportErrors,
            ViewSource::Album {
                album,
                album_artist,
            } => TrackCollection::Library(LibraryScope::Album(AlbumKey::new(album, album_artist))),
            ViewSource::Artist(artist) => {
                TrackCollection::Library(LibraryScope::Artist(ArtistKey::new(artist)))
            }
            ViewSource::MyStats => return Self::MyStats,
            ViewSource::Device { serial } => return Self::Device { serial },
        };
        Self::tracks(collection, TrackViewState::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::BrowseFilter;
    use crate::view_source::ViewSource;

    #[test]
    fn browse_1_album_and_artist_are_library_scopes_not_modes() {
        let album = BrowserPlace::from(ViewSource::Album {
            album: "  Blue  ".into(),
            album_artist: "  Joni Mitchell ".into(),
        });
        let artist = BrowserPlace::from(ViewSource::Artist("  Björk ".into()));

        assert_eq!(
            album.collection(),
            Some(&TrackCollection::Library(LibraryScope::Album(
                AlbumKey::new("Blue", "Joni Mitchell")
            )))
        );
        assert_eq!(
            artist.collection(),
            Some(&TrackCollection::Library(LibraryScope::Artist(
                ArtistKey::new("Björk")
            )))
        );
    }

    #[test]
    fn browse_2_each_place_owns_independent_refinements_and_view_state() {
        let root = BrowserPlace::tracks(
            TrackCollection::Library(LibraryScope::All),
            TrackViewState {
                search: "shore".into(),
                browse: BrowseFilter {
                    genre: Some("Metal".into()),
                    ..BrowseFilter::default()
                },
                sort: TrackSort::new("artist", SortDirection::Ascending),
                anchor: Some(TrackAnchor::new(87, 6.5)),
                selected_ids: vec![87, 91],
                focus: TrackFocus::Track(87),
            },
        );
        let album = BrowserPlace::fresh_album("Pain Remains", "Lorna Shore");

        assert_eq!(root.track_state().unwrap().search, "shore");
        assert_eq!(root.track_state().unwrap().selected_ids, vec![87, 91]);
        assert_eq!(album.track_state(), Some(&TrackViewState::default()));
    }

    #[test]
    fn browse_3_view_source_round_trip_preserves_query_identity() {
        let sources = [
            ViewSource::Library,
            ViewSource::Playlist(7),
            ViewSource::Smart(9),
            ViewSource::Queue,
            ViewSource::Missing,
            ViewSource::ImportErrors,
            ViewSource::Album {
                album: "Blue".into(),
                album_artist: "Joni Mitchell".into(),
            },
            ViewSource::Artist("Björk".into()),
            ViewSource::MyStats,
            ViewSource::Device {
                serial: "pixel-8".into(),
            },
        ];

        for source in sources {
            assert_eq!(BrowserPlace::from(source.clone()).view_source(), source);
        }
    }

    #[test]
    fn browse_4_my_stats_remains_a_dashboard_without_track_state() {
        let stats = BrowserPlace::from(ViewSource::MyStats);

        assert_eq!(stats, BrowserPlace::MyStats);
        assert_eq!(stats.collection(), None);
        assert_eq!(stats.track_state(), None);
    }

    #[test]
    fn browse_1_non_track_content_never_pretends_to_own_track_state() {
        let import_errors = BrowserPlace::from(ViewSource::ImportErrors);

        assert_eq!(import_errors, BrowserPlace::ImportErrors);
        assert_eq!(import_errors.collection(), None);
        assert_eq!(import_errors.track_state(), None);
    }

    #[test]
    fn browse_1_scope_identity_is_trimmed_and_case_insensitive() {
        assert_eq!(
            AlbumKey::new(" BLUE ", "JONI MITCHELL"),
            AlbumKey::new("blue", "joni mitchell")
        );
        assert_eq!(ArtistKey::new(" BJÖRK "), ArtistKey::new("björk"));
    }

    #[test]
    fn browse_2_track_anchor_rejects_invalid_geometry() {
        assert_eq!(TrackAnchor::new(7, -12.0).row_offset, 0.0);
        assert_eq!(TrackAnchor::new(7, f64::NAN).row_offset, 0.0);
        assert_eq!(TrackAnchor::new(7, f64::INFINITY).row_offset, 0.0);
    }
}
