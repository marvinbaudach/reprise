//! Centralized English UI-string catalog. User-facing text in the `ui`
//! module comes from here or a cohesive `strings_*` sibling re-exported here,
//! rather than being inlined at widget call sites.
//!
//! Single-codepoint glyphs (e.g. rating stars ★/☆) whose meaning is purely
//! positional and do not participate in translation may live at their use
//! site rather than here.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}

pub(super) fn formatted(message: &str, values: &[(&str, &str)]) -> String {
    crate::i18n::format_message(&text(message), values)
}

pub(super) fn plural(
    singular: &str,
    plural: &str,
    count: usize,
    values: &[(&str, &str)],
) -> String {
    let count = u32::try_from(count).unwrap_or(u32::MAX);
    crate::i18n::format_message(&crate::i18n::ngettext(singular, plural, count), values)
}

#[path = "strings_artist.rs"]
mod artist;
pub use artist::*;

#[path = "strings_issues.rs"]
mod issues;
pub use issues::*;
#[path = "strings_scan.rs"]
mod scan;
pub use scan::*;
#[path = "strings_news.rs"]
mod news;
pub use news::*;

#[path = "strings_filter.rs"]
mod filter;
pub use filter::*;

#[path = "strings_autocomplete.rs"]
mod autocomplete;
pub use autocomplete::*;

#[path = "strings_tooltips.rs"]
mod tooltips;
pub use tooltips::*;

#[path = "strings_app_shell.rs"]
mod app_shell;
pub use app_shell::*;

#[path = "strings_tag_edit.rs"]
mod tag_edit;
pub use tag_edit::*;

#[path = "strings_track_menu.rs"]
mod track_menu;
pub use track_menu::*;

pub const ONBOARDING_WELCOME: &str = N_!("Welcome to Reprise");
pub const ONBOARDING_PRIVACY: &str = N_!("Reprise keeps your library local. Missing album covers are retrieved automatically from MusicBrainz and Cover Art Archive. Music files are changed only when you explicitly edit tags or move tracks to Trash.");
pub const ONBOARDING_IMPORT_FROM_RHYTHMBOX: &str = N_!("Import from Rhythmbox");
pub const ONBOARDING_IMPORT_FROM_RHYTHMBOX_DESCRIPTION: &str =
    N_!("Rhythmbox was found. Choose what Reprise should import.");
pub const ONBOARDING_RHYTHMBOX_COLUMN_LAYOUT: &str = N_!("Column layout");
pub const ONBOARDING_RHYTHMBOX_COLUMN_LAYOUT_SUBTITLE: &str =
    N_!("Read the layout without changing Rhythmbox settings");

#[path = "strings_rhythmbox.rs"]
mod rhythmbox;
pub use rhythmbox::*;

pub const ONBOARDING_SKIP: &str = N_!("Skip for Now");
pub const ONBOARDING_SET_UP: &str = N_!("Set Up Library");
pub const MAIN_MENU: &str = N_!("Main menu");
pub const COMPACT_MODE: &str = N_!("Compact Mode");
// Compact Mode opens through the menu action; the Library header has no duplicate control.
pub const RESTORE_FULL_WINDOW: &str = N_!("Restore Full Window");
#[allow(dead_code)]
pub const REPEAT_OFF: &str = N_!("Repeat Off");
#[allow(dead_code)]
pub const REPEAT_ALL: &str = N_!("Repeat All");
#[allow(dead_code)]
pub const REPEAT_ONE: &str = N_!("Repeat One");
pub const VIEW_MODE_SAVE_FAILED: &str = N_!("Could not save the window view");
pub const COMPACT_PLAYER_UNAVAILABLE: &str = N_!("Compact player is unavailable");
pub const PREFERENCES: &str = N_!("Preferences");
pub const PREFERENCES_APPEARANCE: &str = N_!("Appearance");
pub const PREFERENCES_LAYOUT: &str = N_!("Layout");
pub const PLAYER_BAR_POSITION: &str = N_!("Player Bar Position");
pub const POSITION_BOTTOM: &str = N_!("Bottom");
pub const POSITION_TOP: &str = N_!("Top");
pub const SHOW_SIDEBAR: &str = N_!("Show Sidebar");
pub const SHOW_STATUS_LINE: &str = N_!("Show Status Line");
pub const LIST_DENSITY: &str = N_!("List Density");
pub const DENSITY_COMFORTABLE: &str = N_!("Comfortable");
pub const DENSITY_STANDARD: &str = N_!("Standard");
pub const DENSITY_COMPACT: &str = N_!("Compact");
pub const PREFERENCES_LIBRARY: &str = N_!("Library");
pub const PREFERENCES_PLUGINS: &str = N_!("Plugins");
pub const PREFERENCES_PLAYBACK: &str = N_!("Playback");
pub const EQUALIZER: &str = N_!("Equalizer");
pub const ENABLE_EQUALIZER: &str = N_!("Enable Equalizer");
pub const EQUALIZER_PRESET: &str = N_!("Preset");
pub const PRESET_FLAT: &str = N_!("Flat");
pub const PRESET_ROCK: &str = N_!("Rock");
pub const PRESET_POP: &str = N_!("Pop");
pub const PRESET_BASS: &str = N_!("Bass Boost");
pub const REPLAYGAIN: &str = N_!("ReplayGain");
pub const REPLAYGAIN_MODE: &str = N_!("Volume Normalization");
pub const REPLAYGAIN_OFF: &str = N_!("Off");
pub const REPLAYGAIN_TRACK: &str = N_!("Per Track");
pub const REPLAYGAIN_ALBUM: &str = N_!("Per Album");
pub const AUDIO_TRANSITIONS: &str = N_!("Audio Transitions");
pub const CROSSFADE: &str = N_!("Crossfade");
pub const CROSSFADE_SUBTITLE: &str = N_!("Smoothly blend the end of a track into the next");
pub const CROSSFADE_OFF: &str = N_!("Off");
pub const GAPLESS_PLAYBACK: &str = N_!("Gapless Playback");
pub const GAPLESS_SUBTITLE: &str = N_!("No silence between tracks of the same album");
pub const GAPLESS_CROSSFADE_ACTIVE_SUBTITLE: &str = N_!("Inactive while Crossfade is enabled");
pub const AUDIO_EFFECTS_FAILED: &str = N_!("Could not apply audio effects");

#[path = "strings_scrobbling.rs"]
mod scrobbling;
pub use scrobbling::*;

pub const LIBRARY_FOLDER: &str = N_!("Library Folder");
pub const NO_LIBRARY_FOLDER: &str = N_!("No folder selected");
pub const CHOOSE_FOLDER: &str = N_!("Choose Folder…");
pub const RESTART_REQUIRED: &str = N_!("Restart required");
pub const EDIT_COLUMN_LAYOUT: &str = N_!("Edit column layout…");
pub const RESET_TO_DEFAULT: &str = N_!("Reset to Default");
pub const CLOSE: &str = N_!("Close");
pub const DRAG_TO_REORDER: &str = N_!("Drag to reorder");
pub const COLUMN_LAYOUT_SAVE_FAILED: &str = N_!("Could not save the column layout");
pub const RHYTHMBOX_COLUMNS_IMPORTED: &str = N_!("Rhythmbox column layout imported");
pub const RHYTHMBOX_COLUMNS_IMPORT_SAVE_FAILED: &str =
    N_!("Could not save the imported column layout");

pub fn rhythmbox_columns_import_failed(error: &str) -> String {
    formatted(
        N_!("Could not import Rhythmbox columns: {error}"),
        &[("error", error)],
    )
}
pub const EDIT_TAGS: &str = N_!("Edit tags…");
pub const REMOVE_FROM_LIBRARY: &str = N_!("Remove from library…");
pub const MOVE_TO_TRASH: &str = N_!("Move to Trash…");
pub const DELETE_TRACKS_HEADING: &str = N_!("Remove Selected Tracks?");
pub const DELETE_TRACKS_CHOICE: &str =
    N_!("Remove only the library entries, or move the music files to Trash as well.");
pub const DELETE_TRACKS_CANCEL: &str = N_!("Cancel");
pub const DELETE_TRACKS_REMOVE: &str = N_!("Remove Only");
pub const DELETE_TRACKS_TRASH: &str = N_!("Move to Trash");
pub const DELETE_DATABASE_UNAVAILABLE: &str =
    N_!("Could not open the library database for removal");
pub const DELETE_WORKER_FAILED: &str = N_!("Could not start the removal worker");

pub fn remove_confirmation_body(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "Remove {count} track from the library? The music file will remain on disk.",
        "Remove {count} tracks from the library? The music files will remain on disk.",
        count,
        &[("count", &count_text)],
    )
}

pub fn trash_confirmation_body(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "Move {count} music file to Trash and remove the library entry?",
        "Move {count} music files to Trash and remove the library entries?",
        count,
        &[("count", &count_text)],
    )
}

pub fn delete_result_toast(removed: usize, failed: usize, trashed: bool) -> String {
    let removed_text = removed.to_string();
    let failed_text = failed.to_string();
    let values = [
        ("removed", removed_text.as_str()),
        ("failed", failed_text.as_str()),
    ];
    match (trashed, failed == 0) {
        (true, true) => plural(
            "{removed} track moved to Trash",
            "{removed} tracks moved to Trash",
            removed,
            &values,
        ),
        (false, true) => plural(
            "{removed} track removed",
            "{removed} tracks removed",
            removed,
            &values,
        ),
        (true, false) => plural(
            "{removed} track moved to Trash; {failed} failed",
            "{removed} tracks moved to Trash; {failed} failed",
            removed,
            &values,
        ),
        (false, false) => plural(
            "{removed} track removed; {failed} failed",
            "{removed} tracks removed; {failed} failed",
            removed,
            &values,
        ),
    }
}

// Superseded by Task F2's FB-3 split: `tag_save_result_toast` (no
// failures) and `tag_save_result_toast_with_failures` (paired with the
// "Details" action button and the 10 s unverdrängbar timeout FB-1 requires
// for an action toast) in strings_tag_edit.rs. Kept — strings.rs is
// append-only — rather than deleted.
#[allow(dead_code)]
pub fn track_edit_result_toast(updated: usize, failed: usize) -> String {
    let updated_text = updated.to_string();
    let failed_text = failed.to_string();
    let values = [
        ("updated", updated_text.as_str()),
        ("failed", failed_text.as_str()),
    ];
    if failed == 0 {
        plural(
            "Updated {updated} track",
            "Updated {updated} tracks",
            updated,
            &values,
        )
    } else {
        plural(
            "Updated {updated} track; {failed} failed",
            "Updated {updated} tracks; {failed} failed",
            updated,
            &values,
        )
    }
}
pub const SEARCH_PLACEHOLDER: &str = N_!("Search all fields");
pub const SCAN_FOLDER: &str = N_!("Scan folder…");
pub const EMPTY_LIBRARY_TITLE: &str = N_!("No music yet");
pub const EMPTY_LIBRARY_DESCRIPTION: &str = N_!("Scan a folder to build your library");
pub const NO_RESULTS_TITLE: &str = N_!("No results");
pub const NO_RESULTS_DESCRIPTION: &str = N_!("Try a different search");

// Neutral "nothing here" empty state (src/ui/track_list.rs, Stage 3 Task 3):
// shown for the Missing/ImportErrors sources when they have no rows and no
// search filter is active — deliberately not the "no music yet" copy above,
// which would read oddly for e.g. "no files are currently missing".
pub const NOTHING_HERE_TITLE: &str = N_!("Nothing here");
pub const NOTHING_HERE_DESCRIPTION: &str = N_!("This view has no tracks right now");

// Scan flow (src/ui/scan_flow.rs and src/ui/scan_progress.rs).
pub const SCAN_CARD_TITLE: &str = N_!("Scanning library");

pub fn scan_complete_toast(new_tracks: u32, failed: u32) -> String {
    if failed > 0 {
        format!("Scan complete · {new_tracks} new, {failed} failed")
    } else {
        format!("Scan complete · {new_tracks} new tracks")
    }
}

pub const SCAN_DIALOG_TITLE: &str = N_!("Select Music Folder");
pub const SCANNING: &str = N_!("Scanning…");
pub const SCAN_DISCOVERING: &str = N_!("Finding music files…");

pub fn scan_progress(processed: u64, total: u64) -> String {
    let processed = processed.to_string();
    let total = total.to_string();
    formatted(
        N_!("{processed} of {total} files scanned"),
        &[("processed", &processed), ("total", &total)],
    )
}
/// Prefix for the error toast shown after a failed scan; the underlying
/// `ScanError`'s `Display` text is appended by the caller.
pub const SCAN_FAILED_PREFIX: &str = N_!("Scan failed: ");
pub const FETCH_DETAIL: &str = N_!("covers & lyrics…");

pub fn fetch_progress(done: u64, total: u64) -> String {
    format!("{done} of {total}")
}

// Status bar (src/ui/status_bar.rs).
pub const STATUS_TRACK_SINGULAR: &str = N_!("track");
pub const STATUS_TRACK_PLURAL: &str = N_!("tracks");
/// Middle-dot separator between the track count and total duration, per the
/// design mockup (e.g. "1,704 tracks · 4 days, 6 hours and 28 minutes").
pub const STATUS_SEPARATOR: &str = N_!(" · ");

// Track list column headers (src/ui/track_list.rs).
pub const COLUMN_TITLE: &str = N_!("Title");
pub const COLUMN_COVER: &str = N_!("Cover");
pub const COLUMN_ARTIST: &str = N_!("Artist");
pub const COLUMN_ALBUM: &str = N_!("Album");
pub const COLUMN_TRACK_NUMBER: &str = N_!("Track");
pub const COLUMN_GENRE: &str = N_!("Genre");
pub const COLUMN_YEAR: &str = N_!("Year");
pub const COLUMN_LENGTH: &str = N_!("Length");
pub const COLUMN_PLAY_COUNT: &str = N_!("Plays");
// The Rating column's header reuses `RATING` below rather than having its
// own `COLUMN_RATING` const — the column header and the `RatingWidget`
// tooltip (src/ui/rating.rs) are the same word, so one const serves both.

// Player bar (src/ui/player_bar.rs).
pub const PLAY: &str = N_!("Play");
pub const PAUSE: &str = N_!("Pause");
pub const PLAYBACK_POSITION: &str = N_!("Playback position");
pub const VOLUME: &str = N_!("Volume");
pub const SHUFFLE: &str = N_!("Shuffle");
pub const PREVIOUS: &str = N_!("Previous");
pub const NEXT: &str = N_!("Next");
pub const REPEAT: &str = N_!("Repeat");

// Rating: used both as the track list's Rating column header
// (src/ui/track_list.rs) and as the RatingWidget's tooltip
// (src/ui/rating.rs).
pub const RATING: &str = N_!("Rating");

/// Accessible name for a rating star button (1-based). Returns
/// "Rate 1 star" for n=1, "Rate N stars" for n>1.
pub fn rate_n_stars(n: i32) -> String {
    let count = usize::try_from(n).unwrap_or_default();
    let count_text = n.to_string();
    plural(
        "Rate {count} star",
        "Rate {count} stars",
        count,
        &[("count", &count_text)],
    )
}

// Playback fault tolerance (src/ui/player_controller.rs, Stage 2 Task 5): a
// physically deleted or otherwise unplayable queued file must never crash or
// dead-end the app — it surfaces here as a toast instead.

/// Toast shown when a queued track's file no longer exists on disk: the
/// underlying row has just been marked `missing` in the DB (see
/// `queries::mark_track_missing`) and will disappear from the track list on
/// its next reload.
pub fn file_missing_toast(title: &str) -> String {
    formatted(
        N_!("File not found — marked as missing: {title}"),
        &[("title", title)],
    )
}

/// Toast shown when a queued track's file exists but playback still failed
/// (e.g. corrupt/unsupported content) — the track is skipped, but *not*
/// marked missing, since the file itself is still there.
pub fn could_not_play_toast(title: &str) -> String {
    formatted(
        N_!("Could not play {title} — skipping"),
        &[("title", title)],
    )
}

/// Toast shown when the skip-loop guard (`should_stop_skipping`) trips:
/// auto-skip gives up after enough consecutive unplayable tracks rather than
/// spinning through an entire broken queue.
pub const PLAYBACK_STOPPED_TOO_MANY_UNPLAYABLE: &str =
    N_!("Playback stopped — too many unplayable tracks");

// Rating write failures (src/ui/track_list.rs, Stage 3 Task 1 backlog item a):
// the on-screen rating already updated by the time the write is attempted, so
// a failure needs a toast, not just a log line, or the user has no way to
// know it didn't actually persist.

/// Toast shown when persisting a rating change (`library::stats::set_rating`)
/// fails.
pub fn rating_save_failed_toast(title: &str) -> String {
    formatted(
        N_!("Could not save rating for {title}"),
        &[("title", title)],
    )
}

// Sidebar (src/ui/sidebar.rs, Stage 3 Task 4): navigation section headers,
// row labels, and the "New playlist" dialog. Section headers are given in
// the design mockup's all-caps form directly (not upper-cased at render
// time) since that's the exact copy the mockup shows, not a text-transform.

pub const SIDEBAR_SECTION_LIBRARY: &str = N_!("LIBRARY");
pub const SIDEBAR_SECTION_PLAYLISTS: &str = N_!("PLAYLISTS");
pub const SIDEBAR_SECTION_SMART: &str = N_!("SMART");
pub const SIDEBAR_SECTION_ISSUES: &str = N_!("ISSUES");

pub const SIDEBAR_MUSIC: &str = N_!("Music");
pub const SIDEBAR_QUEUE: &str = N_!("Queue");
pub const QUEUE_SECTION_NOW_PLAYING: &str = N_!("Now Playing");
pub const JUMP_TO_NOW_PLAYING: &str = N_!("Jump to now playing");
pub const NAVIGATE_BACK: &str = N_!("Back to previous view");
pub const CONTEXT_MENU_PLAY_NEXT: &str = N_!("Play next");
pub const QUEUE_CLEAR_PLAY_NEXT: &str = N_!("Clear");
pub const QUEUE_SECTION_PLAY_NEXT: &str = N_!("Play Next");
/// `{}` is the playback origin's display label (playlist/album/artist name
/// or the localized "Music").
pub const QUEUE_SECTION_UP_NEXT_FROM: &str = N_!("Up Next · from {}");
pub const EMPTY_QUEUE_TITLE: &str = N_!("Nothing queued");
pub const EMPTY_QUEUE_DESCRIPTION: &str = N_!("Play something");
pub const SIDEBAR_NEW_PLAYLIST: &str = N_!("New playlist");
pub const SIDEBAR_IMPORT_ERRORS: &str = N_!("Import errors");
pub const SIDEBAR_MISSING_FILES: &str = N_!("Missing files");
pub const SIDEBAR_MY_STATS: &str = N_!("My Stats");

/// Tooltip/accessible name for the headerbar's persistent sidebar-visibility
/// toggle.
pub const SIDEBAR_TOGGLE: &str = N_!("Toggle sidebar");

pub const NEW_PLAYLIST_DIALOG_HEADING: &str = N_!("New playlist");
pub const NEW_PLAYLIST_ENTRY_PLACEHOLDER: &str = N_!("Playlist name");
pub const CANCEL: &str = N_!("Cancel");
pub const CREATE: &str = N_!("Create");

/// Toast shown when `library::playlists::create` fails while handling the
/// sidebar's "New playlist" dialog.
pub fn playlist_create_failed_toast(name: &str) -> String {
    formatted(N_!("Could not create playlist “{name}”"), &[("name", name)])
}

// Problem-source actions (src/ui/track_list_context_menu.rs, src/ui/
// import_errors_view.rs, Stage 3 Task 8): the Missing/Import-errors sidebar
// sources become actionable — "Rescan library"/"Remove from library" for a
// missing track, "Retry"/"Dismiss" for an import-error row.

/// Missing-source context menu item: re-runs a scan of the persisted library
/// root (`library::settings::get_library_root`) — a reappeared file clears
/// `missing` via the scanner's existing restore path.
pub const CONTEXT_MENU_RESCAN_LIBRARY: &str = N_!("Rescan library");
/// Toast shown when "Rescan library" is invoked but no library folder has
/// ever been scanned/persisted yet (`library::settings::get_library_root`
/// returns `None`) — nothing to rescan.
pub fn no_library_root_to_rescan_toast() -> String {
    text(N_!(
        "No library folder to rescan yet — use “Scan folder…” first"
    ))
}

/// Toast shown when "Rescan library" is invoked while a scan (from folder
/// selection, initial setup, a previous rescan, or this same action) is
/// already running.
pub fn scan_already_running_toast() -> String {
    text(N_!("A scan is already running"))
}

/// Toast for the "Remove from library" context-menu action — plural-correct,
/// same convention as the playlist-mutation toasts above.
pub fn tracks_removed_from_library_toast(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track removed from library",
        "{count} tracks removed from library",
        count,
        &[("count", &count_text)],
    )
}

/// Toast shown when `queries::remove_missing_tracks` fails while handling
/// the context menu's "Remove from library" action.
pub fn tracks_removed_from_library_failed_toast() -> String {
    text(N_!("Could not remove tracks from library"))
}

// Import-errors panel (src/ui/import_errors_view.rs, Stage 3 Task 8): a
// dedicated three-column (path/reason/time) view for the `import_errors`
// table, since its rows aren't `Track`s and don't fit the shared
// title/artist/… `ColumnView`.

/// "Retry" re-scans just that one path (`library::scanner::scan_folder`
/// against the single file); success clears the row, failure refreshes its
/// `reason`/`occurred_at`.
pub const IMPORT_ERROR_RETRY: &str = N_!("Retry");
/// "Dismiss" deletes the `import_errors` row itself — never a file, never
/// the (nonexistent, for this row) `tracks` row.
pub const IMPORT_ERROR_DISMISS: &str = N_!("Dismiss");

/// Toast shown when a "Retry" scan itself fails to run (a `ScanError`, not
/// "the file is still unreadable" — that case just leaves/updates the
/// `import_errors` row, which the panel's own refresh already shows).
pub fn import_error_retry_failed_toast() -> String {
    text(N_!("Could not retry — see the log for details"))
}

/// Toast for the "Add to queue" context-menu action — plural-correct
/// (reuses `STATUS_TRACK_SINGULAR`/`STATUS_TRACK_PLURAL` rather than
/// hardcoding "track"/"tracks" a second time).
pub fn tracks_added_to_queue_toast(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track added to queue",
        "{count} tracks added to queue",
        count,
        &[("count", &count_text)],
    )
}

/// Toast for the "Add to playlist" context-menu action — used for both an
/// existing playlist and one just created via "New playlist…", since the
/// outcome reads identically either way. Plural-correct, same convention as
/// `tracks_added_to_queue_toast`.
pub fn tracks_added_to_playlist_toast(count: usize, playlist_name: &str) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track added to {playlist}",
        "{count} tracks added to {playlist}",
        count,
        &[("count", &count_text), ("playlist", playlist_name)],
    )
}

/// Toast for the "Remove from playlist" context-menu action — plural-correct,
/// same convention as the two toasts above.
pub fn tracks_removed_from_playlist_toast(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track removed from playlist",
        "{count} tracks removed from playlist",
        count,
        &[("count", &count_text)],
    )
}

/// Toast shown when `library::playlists::add_tracks` fails while handling
/// the context menu's "Add to playlist" action (existing playlist).
pub fn playlist_add_tracks_failed_toast(name: &str) -> String {
    formatted(N_!("Could not add tracks to “{name}”"), &[("name", name)])
}

/// Toast shown when `library::playlists::remove_positions` fails while
/// handling the context menu's "Remove from playlist" action.
pub fn playlist_remove_tracks_failed_toast() -> String {
    text(N_!("Could not remove tracks from playlist"))
}

/// Toast shown when `ui::track_actions::remove_selected_from_playlist`
/// returns `RemoveFromPlaylistError::Unresolvable` — the safety backstop
/// for when a selected row's true playlist position couldn't be resolved.
/// Nothing was removed; this tells the user to reload rather than silently
/// reporting success or failure with no explanation.
pub fn playlist_remove_tracks_unresolvable_toast() -> String {
    text(N_!("Could not remove — reload the playlist and try again"))
}

// Drag and drop (src/ui/track_list_dnd.rs, src/ui/sidebar.rs, Stage 3 Task 6):
// dragging the current selection onto a sidebar playlist row to add tracks,
// and reordering within a playlist/the queue view.

/// The drag icon's label text (`gtk::WidgetPaintable`-wrapped `Label`),
/// shown under the pointer while dragging — plural-correct, same convention
/// as the context menu's toasts.
pub fn drag_tracks_label(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track",
        "{count} tracks",
        count,
        &[("count", &count_text)],
    )
}

/// Toast shown when dropping the current selection onto a sidebar playlist
/// row fails (`library::playlists::add_tracks` error).
pub fn playlist_drop_add_failed_toast(name: &str) -> String {
    playlist_add_tracks_failed_toast(name)
}

/// Toast shown when `library::playlists::move_position` fails while handling
/// an in-list playlist drag-reorder.
pub fn playlist_reorder_failed_toast() -> String {
    text(N_!("Could not reorder playlist"))
}

// M3U import/export (src/ui/playlist_io.rs, src/ui/sidebar_export.rs,
// Stage 3 Task 7): an "Import playlist…" sidebar action and a per-playlist
// "Export playlist…" sidebar context-menu action.

pub const IMPORT_PLAYLIST: &str = N_!("Import playlist…");
pub const EXPORT_PLAYLIST: &str = N_!("Export playlist…");
pub const DELETE_PLAYLIST: &str = N_!("Delete playlist…");
pub const PLAYLIST_DELETE_RESPONSE: &str = N_!("Delete Playlist");
pub const PLAYLIST_DELETE_BODY: &str =
    N_!("The playlist will be deleted. Its tracks will remain in your library.");
pub fn playlist_delete_heading(name: &str) -> String {
    formatted(N_!("Delete “{name}”?"), &[("name", name)])
}
pub fn playlist_deleted_toast(name: &str) -> String {
    formatted(N_!("Deleted playlist “{name}”"), &[("name", name)])
}
pub fn playlist_delete_failed_toast(name: &str) -> String {
    formatted(N_!("Could not delete playlist “{name}”"), &[("name", name)])
}
pub const IMPORT_PLAYLIST_DIALOG_TITLE: &str = N_!("Import Playlist");
pub const EXPORT_PLAYLIST_DIALOG_TITLE: &str = N_!("Export Playlist");
/// Name shown for the `gtk::FileFilter` restricting the import dialog to
/// `.m3u`/`.m3u8` files.
pub const M3U_FILE_FILTER_NAME: &str = N_!("M3U Playlists");
/// Fallback playlist name when an imported `.m3u` file's name can't be used
/// as-is (empty file stem, or a non-UTF-8 stem lossily decoded down to
/// nothing meaningful).
pub const IMPORTED_PLAYLIST_FALLBACK_NAME: &str = N_!("Imported playlist");

/// Toast shown after a successful import: `matched` of `total` path lines in
/// the `.m3u` file resolved to a track already in the library.
pub fn playlist_imported_toast(name: &str, matched: usize, total: usize) -> String {
    let matched_text = matched.to_string();
    let total_text = total.to_string();
    formatted(
        N_!("Imported {name}: {matched} of {total} tracks matched"),
        &[
            ("name", name),
            ("matched", &matched_text),
            ("total", &total_text),
        ],
    )
}

/// Toast shown when an import matched zero of `total` path lines — no
/// playlist is created in that case (see `ui::playlist_io::import_playlist`'s
/// doc comment), so this explicitly calls out that nothing was added.
pub fn playlist_import_zero_matched_toast(name: &str, total: usize) -> String {
    let total_text = total.to_string();
    formatted(
        N_!("Imported {name}: 0 of {total} tracks matched — nothing added"),
        &[("name", name), ("total", &total_text)],
    )
}

/// Toast shown when reading or parsing the chosen `.m3u` file fails, or the
/// new playlist can't be created/populated in the database.
pub fn playlist_import_failed_toast() -> String {
    text(N_!("Could not import playlist"))
}

pub fn file_open_not_in_library_toast(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "One opened audio file is not in the library",
        "{count} opened audio files are not in the library",
        count,
        &[("count", &count_text)],
    )
}

pub fn file_open_unsupported_toast(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "One opened file is not supported",
        "{count} opened files are not supported",
        count,
        &[("count", &count_text)],
    )
}

pub fn file_open_playback_unavailable_toast() -> String {
    text(N_!("Playback is unavailable"))
}

/// Toast shown after a successful export.
pub fn playlist_exported_toast(name: &str) -> String {
    formatted(N_!("Exported {name}"), &[("name", name)])
}

/// Toast shown when writing the exported `.m3u` file fails.
pub fn playlist_export_failed_toast(name: &str) -> String {
    formatted(N_!("Could not export “{name}”"), &[("name", name)])
}

// Application identity and legal information shown in the native About dialog.
// Album view (library_views/album_view.rs, album_card.rs).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_complete_toast_without_failures() {
        assert_eq!(scan_complete_toast(38, 0), "Scan complete · 38 new tracks");
    }

    #[test]
    fn scan_complete_toast_with_failures() {
        assert_eq!(
            scan_complete_toast(38, 3),
            "Scan complete · 38 new, 3 failed"
        );
    }
}
