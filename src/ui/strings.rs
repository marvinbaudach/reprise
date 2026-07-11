//! Centralized English UI strings. All user-facing text in the `ui` module
//! must come from here rather than being inlined at call sites — this keeps
//! translation and copy review a one-file affair.

pub const APP_NAME: &str = "Reprise";
pub const SEARCH_PLACEHOLDER: &str = "Search all fields";
pub const SCAN_FOLDER: &str = "Scan folder…";
pub const EMPTY_LIBRARY_TITLE: &str = "No music yet";
pub const EMPTY_LIBRARY_DESCRIPTION: &str = "Scan a folder to build your library";
pub const NO_RESULTS_TITLE: &str = "No results";
pub const NO_RESULTS_DESCRIPTION: &str = "Try a different search";

// Scan flow (src/ui/window.rs).
pub const SCAN_DIALOG_TITLE: &str = "Select Music Folder";
pub const SCANNING: &str = "Scanning…";
/// Prefix for the error toast shown after a failed scan; the underlying
/// `ScanError`'s `Display` text is appended by the caller.
pub const SCAN_FAILED_PREFIX: &str = "Scan failed: ";

// Status bar (src/ui/status_bar.rs).
pub const STATUS_TRACK_SINGULAR: &str = "track";
pub const STATUS_TRACK_PLURAL: &str = "tracks";
/// Middle-dot separator between the track count and total duration, per the
/// design mockup (e.g. "1,704 tracks · 4 days, 6 hours and 28 minutes").
pub const STATUS_SEPARATOR: &str = " · ";

// Track list column headers (src/ui/track_list.rs).
pub const COLUMN_TITLE: &str = "Title";
pub const COLUMN_ARTIST: &str = "Artist";
pub const COLUMN_ALBUM: &str = "Album";
pub const COLUMN_YEAR: &str = "Year";
pub const COLUMN_LENGTH: &str = "Length";
// The Rating column's header reuses `RATING` below rather than having its
// own `COLUMN_RATING` const — the column header and the `RatingWidget`
// tooltip (src/ui/rating.rs) are the same word, so one const serves both.

// Player bar (src/ui/player_bar.rs).
pub const PLAY: &str = "Play";
pub const PAUSE: &str = "Pause";
pub const PLAYBACK_POSITION: &str = "Playback position";
pub const VOLUME: &str = "Volume";
pub const SHUFFLE: &str = "Shuffle";
pub const PREVIOUS: &str = "Previous";
pub const NEXT: &str = "Next";
pub const REPEAT: &str = "Repeat";

// Rating: used both as the track list's Rating column header
// (src/ui/track_list.rs) and as the RatingWidget's tooltip
// (src/ui/rating.rs).
pub const RATING: &str = "Rating";

// Playback fault tolerance (src/ui/player_controller.rs, Stage 2 Task 5): a
// physically deleted or otherwise unplayable queued file must never crash or
// dead-end the app — it surfaces here as a toast instead.

/// Toast shown when a queued track's file no longer exists on disk: the
/// underlying row has just been marked `missing` in the DB (see
/// `queries::mark_track_missing`) and will disappear from the track list on
/// its next reload.
pub fn file_missing_toast(title: &str) -> String {
    format!("File not found — marked as missing: {title}")
}

/// Toast shown when a queued track's file exists but playback still failed
/// (e.g. corrupt/unsupported content) — the track is skipped, but *not*
/// marked missing, since the file itself is still there.
pub fn could_not_play_toast(title: &str) -> String {
    format!("Could not play {title} — skipping")
}

/// Toast shown when the skip-loop guard (`should_stop_skipping`) trips:
/// auto-skip gives up after enough consecutive unplayable tracks rather than
/// spinning through an entire broken queue.
pub const PLAYBACK_STOPPED_TOO_MANY_UNPLAYABLE: &str =
    "Playback stopped — too many unplayable tracks";
