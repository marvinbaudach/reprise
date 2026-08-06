/// The subset of a track's columns the player bar and queue playback path
/// need: the file to hand `Player::play`, display metadata, and the duration
/// play-tracking's 50%-listened check requires
/// (`library::stats::should_count_play`). Deliberately narrower than the
/// full `Track` (no rating/play_count/etc. — the bar doesn't display those),
/// avoiding the cost of loading and holding the columns nothing here reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackSummary {
    pub path: String,
    pub title: String,
    pub artist: String,
    /// Stage 2 Task 6 (MPRIS): feeds `Metadata`'s `xesam:album`. Not used by
    /// the player bar (which only shows title/artist), so it went unused
    /// here until MPRIS needed it.
    pub album: String,
    /// Raw album artist tag (may be empty). Use `effective_album_artist()` to
    /// get the display value that matches `AlbumSummary::album_artist` — i.e.
    /// `album_artist` when non-empty, `artist` otherwise. Loaded alongside the
    /// other summary fields so `notify_now_playing_album_changed` can send the
    /// same effective-artist key the album grid uses for EQ-marker matching.
    pub album_artist: String,
    /// Raw genre and artist MBID are retained by the in-flight playback
    /// snapshot so local listen history remains complete after catalog
    /// deletion.
    pub genre: String,
    pub artist_mbid: Option<String>,
    /// Optional release year displayed by metadata-rich player surfaces.
    pub year: Option<i32>,
    pub duration_ms: i64,
}

impl TrackSummary {
    /// Returns the effective album artist: `album_artist` when non-empty
    /// (trimmed), `artist` otherwise. Mirrors the SQL expression
    /// `CASE WHEN TRIM(album_artist) <> '' THEN TRIM(album_artist) ELSE
    /// TRIM(artist) END` that `query_albums` uses for `AlbumSummary::
    /// album_artist`, so the two sources always agree on the grouping key.
    pub fn effective_album_artist(&self) -> &str {
        if self.album_artist.trim().is_empty() {
            &self.artist
        } else {
            &self.album_artist
        }
    }
}
