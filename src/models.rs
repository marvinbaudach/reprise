#[derive(Debug, Clone)]
pub struct Track {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: Option<i32>,
    pub track_no: Option<i32>,
    pub genre: String,
    pub duration_ms: i64,
    pub bitrate_kbps: Option<i32>,
    pub rating: i32,
    pub play_count: i64,
    pub last_played_at: Option<i64>,
    pub added_at: i64,
    pub file_mtime: i64,
    pub missing: bool,
    /// Schema v2 (Stage 2 Task 8 — scanner move detection): filesystem
    /// identity captured on every insert/update, used to recognize a
    /// relocated file on rescan. `device`/`inode` are `None` for a row that
    /// predates v2 and hasn't been rescanned since.
    pub file_size: i64,
    pub device: Option<i64>,
    pub inode: Option<i64>,
    /// The row's true `playlist_tracks.position` — `Some(pos)` only when this
    /// `Track` came from `queries::query_track_window`'s `ViewSource::
    /// Playlist` branch (`row_to_playlist_track`), `None` for every other
    /// source. This is the durable fix for Stage 3 Task 5's "remove from
    /// playlist deletes the wrong row" bug: a playlist view's on-screen row
    /// order diverges from `pt.position` the moment a column-header sort or
    /// a live search filter is active, so `ui::track_actions::
    /// remove_selected_from_playlist` must resolve each selected *view* row
    /// to its true `pt.position` via this field rather than assuming the two
    /// coincide — see that function's doc comment.
    pub playlist_position: Option<i64>,
}
