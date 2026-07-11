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
}
