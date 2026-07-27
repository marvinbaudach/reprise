//! `ViewSource`: identifies which of the six sources (Stage 3 Task 3) the
//! shared track list is currently displaying. One `TrackListModel`/
//! `GtkColumnView` (see `track_list.rs`/`track_list_model.rs`) serves all of
//! them — only the SQL behind `queries::query_track_window`/
//! `query_track_count`/`query_track_ids` changes per variant (see that
//! module's per-source query functions, each `match`ed on this enum).
//!
//! `ImportErrors` carries no id and no row shape yet: Task 8 defines its own
//! (non-`tracks`) columns for the last scan's import failures. For now the
//! track list simply shows an empty list for this source (see `queries::
//! query_track_window`'s `ImportErrors` arm) — the variant exists already so
//! the sidebar (Task 4) and this enum agree on the full set of six sources
//! from day one, rather than growing the enum again later.

/// Which source the track list is currently querying. `Playlist`/`Smart`
/// carry the referenced row's id; `Library`/`Queue`/`Missing`/`ImportErrors`
/// are singletons (only one of each can ever be shown at a time).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum ViewSource {
    /// The whole present library (`queries::clauses::PRESENT`) — today's
    /// only source before this task, and still the default.
    #[default]
    Library,
    /// Present tracks added during the rolling seven-day window. This is a
    /// Library scope, not a capped user-defined smart playlist.
    RecentlyAdded,
    /// A manual playlist, ordered by `playlist_tracks.position` by default
    /// (a column-header click temporarily overrides — see `queries.rs`'s
    /// `"playlist_order"` whitelist sentinel). Carries the `playlists.id`.
    Playlist(i64),
    /// A smart playlist: rows matching `smart_playlists.rules_json`, sorted
    /// and capped by that row's own `sort_field`/`sort_dir`/`limit_count`.
    /// Carries the `smart_playlists.id`.
    Smart(i64),
    /// The current playback queue, in its current (possibly shuffled) play
    /// order — see `queries.rs`'s `queue_ids` parameter and `queue::Queue::
    /// ids_in_order`.
    Queue,
    /// Tracks matching `queries::clauses::MISSING` (Stage 2 Task 5): files
    /// that vanished from disk since they were scanned in, resurfaced here
    /// rather than silently dropped from the database.
    Missing,
    /// Import failures from the last scan (the existing `import_errors`
    /// table). Task 8 builds the real backing query/columns; the track list
    /// degrades to an empty list for this source until then. `queries::
    /// query_import_error_count` exposes the one piece of this source this
    /// task builds ahead of time (a bare count, for a future sidebar badge).
    ImportErrors,
    /// A read-only album detail reached from the visual Albums grid. The
    /// pair is the album identity used by the grid query; both fields are
    /// matched case-insensitively after trimming, and no database state is
    /// written when entering or leaving this source.
    Album { album: String, album_artist: String },
    /// A read-only artist detail reached from the visual Artists view.
    /// Artist identity is the trimmed, case-insensitive effective album
    /// artist used by the summary query.
    Artist(String),
    /// A read-only genre scope reached from My Stats. The value remains a
    /// durable scope independent of clearable Library browse facets.
    Genre(String),
    /// The "My Stats" screen — a dedicated view backed by
    /// `library::stats_screen` rather than the shared track list. The
    /// sidebar routes to it; the content area shows the stats view widget
    /// instead of the `ColumnView`.
    MyStats,
    /// The full releases table backed by the New Releases cache.
    Releases,
    /// The full upcoming-concerts table backed by the Concerts cache.
    Concerts,
    /// The Podcasts source — a dedicated episode table rather than the
    /// shared local-track list.
    Podcasts,
    /// The Internet Radio source — a dedicated station table rather than the
    /// shared local-track list.
    Radio,
    /// The instrumental conversion/staging view (experimental) — a dedicated
    /// view backed by `ai_jobs` + the staging store rather than the shared
    /// track list. The sidebar routes to it only while the experimental switch
    /// is on (INST-11/INST-13); the content area shows the conversion widget.
    Conversions,
}

impl ViewSource {
    /// Short, stable, log-friendly label — used by `tracing` fields and the
    /// `REPRISE_SMOKE_SOURCE` hook (`track_list.rs`) rather than `{:?}`
    /// (which would print `Playlist(3)`, fine for `Debug` but noisier to
    /// grep than the flatter `playlist:3` shape this gives).
    pub fn label(&self) -> String {
        match self {
            Self::Library => "library".to_string(),
            Self::RecentlyAdded => "recently_added".to_string(),
            Self::Playlist(id) => format!("playlist:{id}"),
            Self::Smart(id) => format!("smart:{id}"),
            Self::Queue => "queue".to_string(),
            Self::Missing => "missing".to_string(),
            Self::ImportErrors => "import_errors".to_string(),
            Self::Album {
                album,
                album_artist,
            } => format!("album:{album}:{album_artist}"),
            Self::Artist(artist) => format!("artist:{artist}"),
            Self::Genre(genre) => format!("genre:{genre}"),
            Self::MyStats => "my_stats".to_string(),
            Self::Releases => "releases".to_string(),
            Self::Concerts => "concerts".to_string(),
            Self::Podcasts => "podcasts".to_string(),
            Self::Radio => "radio".to_string(),
            Self::Conversions => "conversions".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_library() {
        assert_eq!(ViewSource::default(), ViewSource::Library);
    }

    #[test]
    fn label_formats_each_variant() {
        assert_eq!(ViewSource::Library.label(), "library");
        assert_eq!(ViewSource::RecentlyAdded.label(), "recently_added");
        assert_eq!(ViewSource::Playlist(3).label(), "playlist:3");
        assert_eq!(ViewSource::Smart(7).label(), "smart:7");
        assert_eq!(ViewSource::Queue.label(), "queue");
        assert_eq!(ViewSource::Missing.label(), "missing");
        assert_eq!(ViewSource::ImportErrors.label(), "import_errors");
        assert_eq!(ViewSource::Releases.label(), "releases");
        assert_eq!(ViewSource::Concerts.label(), "concerts");
        assert_eq!(ViewSource::MyStats.label(), "my_stats");
        assert_eq!(ViewSource::Conversions.label(), "conversions");
        assert_eq!(ViewSource::Podcasts.label(), "podcasts");
        assert_eq!(ViewSource::Radio.label(), "radio");
        assert_eq!(
            ViewSource::Album {
                album: "Blue".into(),
                album_artist: "Joni Mitchell".into(),
            }
            .label(),
            "album:Blue:Joni Mitchell"
        );
        assert_eq!(ViewSource::Artist("Björk".into()).label(), "artist:Björk");
        assert_eq!(
            ViewSource::Genre("Metalcore".into()).label(),
            "genre:Metalcore"
        );
    }
}
