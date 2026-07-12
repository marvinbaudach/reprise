//! Centralized English UI strings. All user-facing text in the `ui` module
//! must come from here rather than being inlined at call sites — this keeps
//! translation and copy review a one-file affair.
//!
//! Single-codepoint glyphs (e.g. rating stars ★/☆) whose meaning is purely
//! positional and do not participate in translation may live at their use
//! site rather than here.

pub const APP_NAME: &str = "Reprise";
pub const MAIN_MENU: &str = "Main menu";
pub const DOWNLOAD_MISSING_COVERS: &str = "Download missing album covers";
pub const IMPORT_RHYTHMBOX_COLUMNS: &str = "Import Rhythmbox column layout";
pub const RHYTHMBOX_COLUMNS_IMPORTED: &str = "Rhythmbox column layout imported";
pub const RHYTHMBOX_COLUMNS_IMPORT_SAVE_FAILED: &str = "Could not save the imported column layout";

pub fn rhythmbox_columns_import_failed(error: &str) -> String {
    format!("Could not import Rhythmbox columns: {error}")
}
pub const EDIT_TAGS: &str = "Edit tags…";
pub const APPLY: &str = "Apply";
pub const MULTIPLE_VALUES: &str = "(multiple values)";
pub const TAG_TITLE: &str = "Title";
pub const TAG_ARTIST: &str = "Artist";
pub const TAG_ALBUM: &str = "Album";
pub const TAG_ALBUM_ARTIST: &str = "Album artist";
pub const TAG_YEAR: &str = "Year";
pub const TAG_TRACK_NUMBER: &str = "Track number";
pub const TAG_GENRE: &str = "Genre";
pub const TAG_NUMBER_ERROR: &str = "Year and track number must be positive whole numbers";
pub const TAG_EDIT_DATABASE_UNAVAILABLE: &str =
    "Could not open the library database for tag editing";
pub const TAG_EDIT_WORKER_FAILED: &str = "Could not start the tag-edit worker";
pub const BROWSE_GENRE: &str = "Genre";
pub const BROWSE_ARTIST: &str = "Artist";
pub const BROWSE_ALBUM: &str = "Album";
pub const ALL_GENRES: &str = "All genres";
pub const ALL_ARTISTS: &str = "All artists";
pub const ALL_ALBUMS: &str = "All albums";
pub const UNKNOWN_GENRE: &str = "Unknown genre";
pub const UNKNOWN_ARTIST: &str = "Unknown artist";
pub const UNKNOWN_ALBUM: &str = "Unknown album";
pub const REMOVE_FROM_LIBRARY: &str = "Remove from library…";
pub const MOVE_TO_TRASH: &str = "Move to Trash…";
pub const DELETE_TRACKS_HEADING: &str = "Remove Selected Tracks?";
pub const DELETE_TRACKS_CHOICE: &str =
    "Remove only the library entries, or move the music files to Trash as well.";
pub const DELETE_TRACKS_CANCEL: &str = "Cancel";
pub const DELETE_TRACKS_REMOVE: &str = "Remove Only";
pub const DELETE_TRACKS_TRASH: &str = "Move to Trash";
pub const DELETE_DATABASE_UNAVAILABLE: &str = "Could not open the library database for removal";
pub const DELETE_WORKER_FAILED: &str = "Could not start the removal worker";

pub fn remove_confirmation_body(count: usize) -> String {
    format!(
        "Remove {count} {} from the library? The music {} will remain on disk.",
        if count == 1 { "track" } else { "tracks" },
        if count == 1 { "file" } else { "files" }
    )
}

pub fn trash_confirmation_body(count: usize) -> String {
    format!(
        "Move {count} music {} to Trash and remove the library {}?",
        if count == 1 { "file" } else { "files" },
        if count == 1 { "entry" } else { "entries" }
    )
}

pub fn delete_result_toast(removed: usize, failed: usize, trashed: bool) -> String {
    let action = if trashed { "moved to Trash" } else { "removed" };
    match failed {
        0 => format!(
            "{removed} {} {action}",
            if removed == 1 { "track" } else { "tracks" }
        ),
        _ => format!(
            "{removed} {} {action}; {failed} failed",
            if removed == 1 { "track" } else { "tracks" }
        ),
    }
}

pub fn tag_edit_result_toast(updated: usize, failed: usize) -> String {
    match (updated, failed) {
        (1, 0) => "Tags updated for 1 track".into(),
        (updated, 0) => format!("Tags updated for {updated} tracks"),
        (updated, failed) => format!("Tags updated for {updated} tracks; {failed} failed"),
    }
}
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
pub const COLUMN_TRACK_NUMBER: &str = "Track";
pub const COLUMN_GENRE: &str = "Genre";
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

// Track list context menu (src/ui/track_list.rs, Stage 3 Task 5): row
// actions on the current selection — Play, Add to queue, Add to playlist
// (submenu of existing playlists plus "New playlist…"), and — only while
// viewing a playlist — Remove from playlist.

pub const CONTEXT_MENU_PLAY: &str = "Play";
pub const CONTEXT_MENU_ADD_TO_QUEUE: &str = "Add to queue";
pub const CONTEXT_MENU_ADD_TO_PLAYLIST: &str = "Add to playlist";
/// Leaf item at the bottom of the "Add to playlist" submenu — ellipsis
/// matches this file's convention for menu items that open a dialog (e.g.
/// `SCAN_FOLDER`), unlike the sidebar's plain "New playlist" row label
/// (`SIDEBAR_NEW_PLAYLIST`), which doesn't open a dialog directly from a
/// menu context.
pub const CONTEXT_MENU_NEW_PLAYLIST: &str = "New playlist…";
pub const CONTEXT_MENU_REMOVE_FROM_PLAYLIST: &str = "Remove from playlist";

// Problem-source actions (src/ui/track_list_context_menu.rs, src/ui/
// import_errors_view.rs, Stage 3 Task 8): the Missing/Import-errors sidebar
// sources become actionable — "Rescan library"/"Remove from library" for a
// missing track, "Retry"/"Dismiss" for an import-error row.

/// Missing-source context menu item: re-runs a scan of the persisted library
/// root (`library::settings::get_library_root`) — a reappeared file clears
/// `missing` via the scanner's existing restore path.
pub const CONTEXT_MENU_RESCAN_LIBRARY: &str = "Rescan library";
/// Toast shown when "Rescan library" is invoked but no library folder has
/// ever been scanned/persisted yet (`library::settings::get_library_root`
/// returns `None`) — nothing to rescan.
pub fn no_library_root_to_rescan_toast() -> String {
    "No library folder to rescan yet — use \"Scan folder…\" first".to_string()
}

/// Toast shown when "Rescan library" is invoked while a scan (from any
/// trigger — the header button, a previous rescan, or this same action) is
/// already running.
pub fn scan_already_running_toast() -> String {
    "A scan is already running".to_string()
}

/// Toast for the "Remove from library" context-menu action — plural-correct,
/// same convention as the playlist-mutation toasts above.
pub fn tracks_removed_from_library_toast(count: usize) -> String {
    let noun = if count == 1 {
        STATUS_TRACK_SINGULAR
    } else {
        STATUS_TRACK_PLURAL
    };
    format!("{count} {noun} removed from library")
}

/// Toast shown when `queries::remove_missing_track` fails while handling
/// the context menu's "Remove from library" action.
pub fn tracks_removed_from_library_failed_toast() -> String {
    "Could not remove tracks from library".to_string()
}

// Import-errors panel (src/ui/import_errors_view.rs, Stage 3 Task 8): a
// dedicated three-column (path/reason/time) view for the `import_errors`
// table, since its rows aren't `Track`s and don't fit the shared
// title/artist/… `ColumnView`.

pub const IMPORT_ERROR_COLUMN_PATH: &str = "Path";
pub const IMPORT_ERROR_COLUMN_REASON: &str = "Reason";
pub const IMPORT_ERROR_COLUMN_TIME: &str = "Time";
/// "Retry" re-scans just that one path (`library::scanner::scan_folder`
/// against the single file); success clears the row, failure refreshes its
/// `reason`/`occurred_at`.
pub const IMPORT_ERROR_RETRY: &str = "Retry";
/// "Dismiss" deletes the `import_errors` row itself — never a file, never
/// the (nonexistent, for this row) `tracks` row.
pub const IMPORT_ERROR_DISMISS: &str = "Dismiss";

/// Toast shown when a "Retry" scan itself fails to run (a `ScanError`, not
/// "the file is still unreadable" — that case just leaves/updates the
/// `import_errors` row, which the panel's own refresh already shows).
pub fn import_error_retry_failed_toast() -> String {
    "Could not retry — see the log for details".to_string()
}

/// Toast for the "Add to queue" context-menu action — plural-correct
/// (reuses `STATUS_TRACK_SINGULAR`/`STATUS_TRACK_PLURAL` rather than
/// hardcoding "track"/"tracks" a second time).
pub fn tracks_added_to_queue_toast(count: usize) -> String {
    let noun = if count == 1 {
        STATUS_TRACK_SINGULAR
    } else {
        STATUS_TRACK_PLURAL
    };
    format!("{count} {noun} added to queue")
}

/// Toast for the "Add to playlist" context-menu action — used for both an
/// existing playlist and one just created via "New playlist…", since the
/// outcome reads identically either way. Plural-correct, same convention as
/// `tracks_added_to_queue_toast`.
pub fn tracks_added_to_playlist_toast(count: usize, playlist_name: &str) -> String {
    let noun = if count == 1 {
        STATUS_TRACK_SINGULAR
    } else {
        STATUS_TRACK_PLURAL
    };
    format!("{count} {noun} added to {playlist_name}")
}

/// Toast for the "Remove from playlist" context-menu action — plural-correct,
/// same convention as the two toasts above.
pub fn tracks_removed_from_playlist_toast(count: usize) -> String {
    let noun = if count == 1 {
        STATUS_TRACK_SINGULAR
    } else {
        STATUS_TRACK_PLURAL
    };
    format!("{count} {noun} removed from playlist")
}

/// Toast shown when `library::playlists::add_tracks` fails while handling
/// the context menu's "Add to playlist" action (existing playlist).
pub fn playlist_add_tracks_failed_toast(name: &str) -> String {
    format!("Could not add tracks to \"{name}\"")
}

/// Toast shown when `library::playlists::remove_positions` fails while
/// handling the context menu's "Remove from playlist" action.
pub fn playlist_remove_tracks_failed_toast() -> String {
    "Could not remove tracks from playlist".to_string()
}

/// Toast shown when `ui::track_actions::remove_selected_from_playlist`
/// returns `RemoveFromPlaylistError::Unresolvable` — the safety backstop
/// for when a selected row's true playlist position couldn't be resolved.
/// Nothing was removed; this tells the user to reload rather than silently
/// reporting success or failure with no explanation.
pub fn playlist_remove_tracks_unresolvable_toast() -> String {
    "Could not remove — reload the playlist and try again".to_string()
}

// Drag and drop (src/ui/track_list_dnd.rs, src/ui/sidebar.rs, Stage 3 Task 6):
// dragging the current selection onto a sidebar playlist row to add tracks,
// and reordering within a playlist/the queue view.

/// The drag icon's label text (`gtk::WidgetPaintable`-wrapped `Label`),
/// shown under the pointer while dragging — plural-correct, same convention
/// as the context menu's toasts.
pub fn drag_tracks_label(count: usize) -> String {
    let noun = if count == 1 {
        STATUS_TRACK_SINGULAR
    } else {
        STATUS_TRACK_PLURAL
    };
    format!("{count} {noun}")
}

/// Toast shown when dropping the current selection onto a sidebar playlist
/// row fails (`library::playlists::add_tracks` error).
pub fn playlist_drop_add_failed_toast(name: &str) -> String {
    format!("Could not add tracks to \"{name}\"")
}

/// Toast shown when `library::playlists::move_position` fails while handling
/// an in-list playlist drag-reorder.
pub fn playlist_reorder_failed_toast() -> String {
    "Could not reorder playlist".to_string()
}

// M3U import/export (src/ui/playlist_io.rs, src/ui/sidebar_export.rs,
// Stage 3 Task 7): a global "Import playlist…" headerbar button and a
// per-playlist "Export playlist…" sidebar context-menu action.

pub const IMPORT_PLAYLIST: &str = "Import playlist…";
pub const EXPORT_PLAYLIST: &str = "Export playlist…";
pub const IMPORT_PLAYLIST_DIALOG_TITLE: &str = "Import Playlist";
pub const EXPORT_PLAYLIST_DIALOG_TITLE: &str = "Export Playlist";
/// Name shown for the `gtk::FileFilter` restricting the import dialog to
/// `.m3u`/`.m3u8` files.
pub const M3U_FILE_FILTER_NAME: &str = "M3U Playlists";
/// Fallback playlist name when an imported `.m3u` file's name can't be used
/// as-is (empty file stem, or a non-UTF-8 stem lossily decoded down to
/// nothing meaningful).
pub const IMPORTED_PLAYLIST_FALLBACK_NAME: &str = "Imported playlist";

/// Toast shown after a successful import: `matched` of `total` path lines in
/// the `.m3u` file resolved to a track already in the library.
pub fn playlist_imported_toast(name: &str, matched: usize, total: usize) -> String {
    format!("Imported {name}: {matched} of {total} tracks matched")
}

/// Toast shown when an import matched zero of `total` path lines — no
/// playlist is created in that case (see `ui::playlist_io::import_playlist`'s
/// doc comment), so this explicitly calls out that nothing was added.
pub fn playlist_import_zero_matched_toast(name: &str, total: usize) -> String {
    format!("Imported {name}: 0 of {total} tracks matched — nothing added")
}

/// Toast shown when reading or parsing the chosen `.m3u` file fails, or the
/// new playlist can't be created/populated in the database.
pub fn playlist_import_failed_toast() -> String {
    "Could not import playlist".to_string()
}

/// Toast shown after a successful export.
pub fn playlist_exported_toast(name: &str) -> String {
    format!("Exported {name}")
}

/// Toast shown when writing the exported `.m3u` file fails.
pub fn playlist_export_failed_toast(name: &str) -> String {
    format!("Could not export \"{name}\"")
}
