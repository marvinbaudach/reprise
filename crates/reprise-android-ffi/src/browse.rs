//! Owned UniFFI records for bounded library browsing.

use reprise_core::queries;

/// One row as the UI needs it — deliberately not the full Core `Track`, so
/// the binding surface stays a decision rather than an accident.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TrackRow {
    pub uri: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
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
            uri: track.path,
            title: track.title,
            artist: track.artist,
            album: track.album,
            duration_ms: track.duration_ms,
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
