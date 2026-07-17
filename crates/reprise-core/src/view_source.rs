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
    /// The whole non-missing library (`missing = 0`) — today's only source
    /// before this task, and still the default.
    #[default]
    Library,
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
    /// Tracks marked `missing = 1` (Stage 2 Task 5): files that vanished
    /// from disk since they were scanned in, resurfaced here rather than
    /// silently dropped from the database.
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
    /// The "My Stats" screen — a dedicated view backed by
    /// `library::stats_screen` rather than the shared track list. The
    /// sidebar routes to it; the content area shows the stats view widget
    /// instead of the `ColumnView`.
    MyStats,
    /// A connected MTP device. The device serial is the durable identity
    /// used by synchronization settings and managed-file inventory.
    Device { serial: String },
}

impl ViewSource {
    /// Short, stable, log-friendly label — used by `tracing` fields and the
    /// `REPRISE_SMOKE_SOURCE` hook (`track_list.rs`) rather than `{:?}`
    /// (which would print `Playlist(3)`, fine for `Debug` but noisier to
    /// grep than the flatter `playlist:3` shape this gives).
    pub fn label(&self) -> String {
        match self {
            Self::Library => "library".to_string(),
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
            Self::MyStats => "my_stats".to_string(),
            Self::Device { serial } => format!("device:{serial}"),
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
        assert_eq!(ViewSource::Playlist(3).label(), "playlist:3");
        assert_eq!(ViewSource::Smart(7).label(), "smart:7");
        assert_eq!(ViewSource::Queue.label(), "queue");
        assert_eq!(ViewSource::Missing.label(), "missing");
        assert_eq!(ViewSource::ImportErrors.label(), "import_errors");
        assert_eq!(ViewSource::MyStats.label(), "my_stats");
        assert_eq!(
            ViewSource::Device {
                serial: "pixel-8".into(),
            }
            .label(),
            "device:pixel-8"
        );
        assert_eq!(
            ViewSource::Album {
                album: "Blue".into(),
                album_artist: "Joni Mitchell".into(),
            }
            .label(),
            "album:Blue:Joni Mitchell"
        );
        assert_eq!(ViewSource::Artist("Björk".into()).label(), "artist:Björk");
    }
}
