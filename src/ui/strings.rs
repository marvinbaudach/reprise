//! Centralized English UI strings. All user-facing text in the `ui` module
//! must come from here rather than being inlined at call sites — this keeps
//! translation and copy review a one-file affair.
//!
//! Single-codepoint glyphs (e.g. rating stars ★/☆) whose meaning is purely
//! positional and do not participate in translation may live at their use
//! site rather than here.

pub const APP_NAME: &str = "Reprise";
pub const SEARCH_PLACEHOLDER: &str = "Search all fields";
pub const SCAN_FOLDER: &str = "Scan folder…";
pub const EMPTY_LIBRARY_TITLE: &str = "No music yet";
pub const EMPTY_LIBRARY_DESCRIPTION: &str = "Scan a folder to build your library";
pub const NO_RESULTS_TITLE: &str = "No results";
pub const NO_RESULTS_DESCRIPTION: &str = "Try a different search";

// Neutral "nothing here" empty state (src/ui/track_list.rs, Stage 3 Task 3):
// shown for the Missing/ImportErrors sources when they have no rows and no
// search filter is active — deliberately not the "no music yet" copy above,
// which would read oddly for e.g. "no files are currently missing".
pub const NOTHING_HERE_TITLE: &str = "Nothing here";
pub const NOTHING_HERE_DESCRIPTION: &str = "This view has no tracks right now";

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

/// "{filtered} of {total}" prefix shown ahead of the track word while a
/// search filter is active (e.g. "42 of 1,704 tracks · …" instead of
/// "1,704 tracks · …") — see `status_bar::format_status_text`. `filtered`/
/// `total` are already formatted (en-US thousands, via `format::
/// format_thousands`); this function owns only the "of" wording.
pub fn status_filtered_of_total(filtered: &str, total: &str) -> String {
    format!("{filtered} of {total}")
}

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

/// Accessible name for a rating star button (1-based). Returns
/// "Rate 1 star" for n=1, "Rate N stars" for n>1.
pub fn rate_n_stars(n: i32) -> String {
    if n == 1 {
        "Rate 1 star".to_string()
    } else {
        format!("Rate {n} stars")
    }
}

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

// Rating write failures (src/ui/track_list.rs, Stage 3 Task 1 backlog item a):
// the on-screen rating already updated by the time the write is attempted, so
// a failure needs a toast, not just a log line, or the user has no way to
// know it didn't actually persist.

/// Toast shown when persisting a rating change (`library::stats::set_rating`)
/// fails.
pub fn rating_save_failed_toast(title: &str) -> String {
    format!("Could not save rating for {title}")
}

// Sidebar (src/ui/sidebar.rs, Stage 3 Task 4): navigation section headers,
// row labels, and the "New playlist" dialog. Section headers are given in
// the design mockup's all-caps form directly (not upper-cased at render
// time) since that's the exact copy the mockup shows, not a text-transform.

pub const SIDEBAR_SECTION_LIBRARY: &str = "LIBRARY";
pub const SIDEBAR_SECTION_PLAYLISTS: &str = "PLAYLISTS";
pub const SIDEBAR_SECTION_SMART: &str = "SMART";

pub const SIDEBAR_MUSIC: &str = "Music";
pub const SIDEBAR_QUEUE: &str = "Queue";
pub const SIDEBAR_NEW_PLAYLIST: &str = "New playlist";
pub const SIDEBAR_IMPORT_ERRORS: &str = "Import errors";
pub const SIDEBAR_MISSING_FILES: &str = "Missing files";

/// Tooltip/accessible name for the headerbar's sidebar-visibility toggle
/// (only shown once the `AdwNavigationSplitView` collapses at narrow
/// widths).
pub const SIDEBAR_TOGGLE: &str = "Toggle sidebar";

pub const NEW_PLAYLIST_DIALOG_HEADING: &str = "New playlist";
pub const NEW_PLAYLIST_ENTRY_PLACEHOLDER: &str = "Playlist name";
pub const CANCEL: &str = "Cancel";
pub const CREATE: &str = "Create";

/// Toast shown when `library::playlists::create` fails while handling the
/// sidebar's "New playlist" dialog.
pub fn playlist_create_failed_toast(name: &str) -> String {
    format!("Could not create playlist \"{name}\"")
}
