/// Why a track is currently missing (schema v10, Task 1.1's `missing_reason`
/// column). `Unknown` is the honest default for anything that can't be told
/// apart — in particular every row backfilled by the v10 migration itself
/// (see `db::SCHEMA_V10`'s doc comment, which predates the `device` column
/// this enum's classifier needs) and any row whose `device` was never
/// recorded. Task 1.5's `library::mounts::classify_missing` is the real
/// classifier both `queries::mark_track_missing` and the scanner's folded-in
/// mark-vanished phase (`library::scanner::scan_folder`) call for every row
/// that DOES have a recorded device; nothing downstream may treat an
/// `Unknown`-reason row as safely auto-removable without re-verifying the
/// file first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingReason {
    /// The file's mount point is currently absent (e.g. an unplugged
    /// external drive) — the file itself may well still exist.
    Unmounted,
    /// The file was confirmed gone from a mounted, reachable filesystem.
    Deleted,
    /// Neither of the above could be established — see this enum's own doc
    /// comment for why this is the only honest default today.
    Unknown,
}

impl MissingReason {
    /// The exact string stored in `tracks.missing_reason` — the inverse of
    /// [`Self::parse`]. Kept as a plain `&'static str` (not `Display`) since
    /// this is a storage format, not user-facing text.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unmounted => "unmounted",
            Self::Deleted => "deleted",
            Self::Unknown => "unknown",
        }
    }

    /// Parses a `tracks.missing_reason` value back into a `MissingReason`.
    /// Anything other than the two named reasons — including a value this
    /// version of the app has never written, from a future schema or an
    /// edited-by-hand row — falls back to `Unknown` rather than erroring:
    /// a row's *presence* is decided by `missing_since` alone (see `Track::
    /// is_missing`), so a row mapper must never fail to load a track just
    /// because `missing_reason` holds an unrecognized string.
    pub fn parse(s: &str) -> Self {
        match s {
            "unmounted" => Self::Unmounted,
            "deleted" => Self::Deleted,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Schema v10 (Task 1.1): `Some(unix_seconds)` since the file was first
    /// found missing, `None` while the file is present. This — not the
    /// legacy `missing` boolean — is the one source of truth for a row's
    /// presence; see [`Self::is_missing`] and `queries::clauses::{PRESENT,
    /// MISSING}`'s doc comments for why a flag-plus-date pair is retired in
    /// favor of the date alone (a flag and a date can drift out of sync, and
    /// a planned auto-clean feature deletes rows based on how long this has
    /// been set — a row with an unclear start date can never be safely
    /// auto-removable). The legacy `tracks.missing` column still exists and
    /// is still populated by writers that predate this task, but nothing in
    /// this crate reads it anymore as of this task — a later migration
    /// (Task 1.3) drops the column outright.
    pub missing_since: Option<i64>,
    /// Why the file is missing — `None` while present, `Some(_)` (parsed via
    /// `MissingReason::parse`, never failing) whenever `missing_since` is
    /// set. See `MissingReason`'s own doc comment for the honest-`Unknown`
    /// default every writer uses today.
    pub missing_reason: Option<MissingReason>,
    /// Schema v10 (Task 1.1): tag-derived flag for a track whose metadata
    /// could not be read at scan time (title/artist fell back to filename-
    /// derived placeholders). Unrelated to the missing/import-errors
    /// rebuild this task is part of — it piggybacks on the same migration
    /// purely because it's the same shape of small tag-derived column.
    pub untagged: bool,
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

impl Track {
    /// Whether this track's file is currently missing — the one place that
    /// question is ever asked in Rust, mirroring `queries::clauses::PRESENT`/
    /// `MISSING` on the SQL side. See `missing_since`'s own doc comment for
    /// why this reads that field alone, never the legacy `missing` column.
    pub fn is_missing(&self) -> bool {
        self.missing_since.is_some()
    }
}
