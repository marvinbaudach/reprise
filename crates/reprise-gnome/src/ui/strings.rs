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

fn formatted(message: &str, values: &[(&str, &str)]) -> String {
    crate::i18n::format_message(&text(message), values)
}

fn plural(singular: &str, plural: &str, count: usize, values: &[(&str, &str)]) -> String {
    let count = u32::try_from(count).unwrap_or(u32::MAX);
    crate::i18n::format_message(&crate::i18n::ngettext(singular, plural, count), values)
}

pub const APP_NAME: &str = N_!("Reprise");
pub const LIBRARY_VIEW_TRACKS: &str = N_!("Tracks");
pub const LIBRARY_VIEW_ALBUMS: &str = N_!("Albums");
pub const LIBRARY_VIEW_ARTISTS: &str = N_!("Artists");
pub const ALBUMS_EMPTY_TITLE: &str = N_!("No Albums Yet");
pub const ALBUMS_EMPTY_DESCRIPTION: &str = N_!("Scan a music folder to see album covers here.");
pub const ARTISTS_EMPTY_TITLE: &str = N_!("No Artists Yet");
pub const ARTISTS_EMPTY_DESCRIPTION: &str = N_!("Scan a music folder to see artists here.");
pub const UNKNOWN_ARTIST: &str = N_!("Unknown Artist");

pub fn artist_counts(album_count: i64, track_count: i64) -> String {
    let album_count = usize::try_from(album_count).unwrap_or(usize::MAX);
    let track_count = usize::try_from(track_count).unwrap_or(usize::MAX);
    let albums = plural(
        "{count} album",
        "{count} albums",
        album_count,
        &[("count", &album_count.to_string())],
    );
    let tracks = plural(
        "{count} track",
        "{count} tracks",
        track_count,
        &[("count", &track_count.to_string())],
    );
    format!("{albums} · {tracks}")
}
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
pub const COMPACT_MENU: &str = N_!("Compact player menu");
#[allow(dead_code)]
pub const COMPACT_LAYOUT: &str = N_!("Layout");
pub const COMPACT_LAYOUT_COVER: &str = N_!("Cover");
pub const COMPACT_LAYOUT_PILL: &str = N_!("Pill");
pub const COMPACT_LAYOUT_CARD: &str = N_!("Card");
pub const COMPACT_COVER: &str = N_!("Album cover");
pub const COMPACT_TITLE: &str = N_!("Track title");
pub const COMPACT_ARTIST: &str = N_!("Track artist");
pub const COMPACT_ALBUM: &str = N_!("Track album");
pub const CURRENT_POSITION: &str = N_!("Current position");
pub const TOTAL_DURATION: &str = N_!("Total duration");
#[allow(dead_code)]
pub const REPEAT_OFF: &str = N_!("Repeat Off");
#[allow(dead_code)]
pub const REPEAT_ALL: &str = N_!("Repeat All");
#[allow(dead_code)]
pub const REPEAT_ONE: &str = N_!("Repeat One");
pub const VIEW_MODE_SAVE_FAILED: &str = N_!("Could not save the window view");
pub const COMPACT_LAYOUT_SAVE_FAILED: &str = N_!("Could not save the compact layout");
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
pub const BADGE_NEW: &str = N_!("NEW");
pub const CROSSFADE: &str = N_!("Crossfade");
pub const CROSSFADE_SUBTITLE: &str = N_!("Smoothly blend the end of a track into the next");
pub const CROSSFADE_OFF: &str = N_!("Off");
pub const GAPLESS_PLAYBACK: &str = N_!("Gapless Playback");
pub const GAPLESS_SUBTITLE: &str = N_!("No silence between tracks of the same album");
pub const AUDIO_EFFECTS_FAILED: &str = N_!("Could not apply audio effects");

#[path = "strings_scrobbling.rs"]
mod scrobbling;
pub use scrobbling::*;

pub const INFORMATION: &str = N_!("Information");
pub const ARTIST_NEWS: &str = N_!("Artist & Album News");
pub const ARTIST_NEWS_DESCRIPTION: &str =
    N_!("Show upcoming and newly released albums from MusicBrainz (network; off by default)");
pub const ARTIST_NEWS_PRIVACY: &str =
    N_!("When enabled, selected artist names are sent to MusicBrainz. Reprise never sends file paths or listening history.");
pub const NEWS_DISABLED_TITLE: &str = N_!("Artist News is Off");
pub const NEWS_SELECT_TRACK: &str = N_!("Select a track to see artist and album news.");
pub const NEWS_MULTIPLE_SELECTION: &str =
    N_!("Artist News is paused while multiple tracks are selected.");
pub const NEWS_NO_ARTIST: &str = N_!("This track has no artist information.");
pub const NEWS_LOADING: &str = N_!("Checking MusicBrainz for album news…");
pub const NEWS_NONE: &str = N_!("No new or upcoming regular albums found.");
pub const NEWS_ERROR: &str = N_!("Artist News is temporarily unavailable.");
pub const NEWS_UNMATCHED: &str = N_!("Artist could not be matched.");
pub const NEWS_AMBIGUOUS: &str = N_!("Artist could not be matched unambiguously.");
pub const NEWS_UPCOMING: &str = N_!("Upcoming");
pub const NEWS_NEW: &str = N_!("New");
pub const NEWS_REFRESH: &str = N_!("Refresh Artist News");
pub const NEWS_OPEN_MUSICBRAINZ: &str = N_!("Open in MusicBrainz");

pub fn tracks_selected(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track selected",
        "{count} tracks selected",
        count,
        &[("count", &count_text)],
    )
}

pub fn news_release_meta(primary_type: &str, date: &str) -> String {
    formatted(
        N_!("{type} · {date}"),
        &[("type", primary_type), ("date", date)],
    )
}

pub fn news_updated(timestamp: i64) -> String {
    let date = news_timestamp_date(timestamp);
    formatted(N_!("MusicBrainz · Updated {date}"), &[("date", &date)])
}

pub fn news_cached(timestamp: i64) -> String {
    let date = news_timestamp_date(timestamp);
    formatted(N_!("Cached · Updated {date}"), &[("date", &date)])
}

fn news_timestamp_date(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0).map_or_else(
        || text(N_!("unknown date")),
        |value| {
            value
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        },
    )
}
pub const COVER_DOWNLOAD_CHECKING: &str = N_!("Checking missing album covers…");
pub const COVER_DOWNLOAD_COMPLETE: &str = N_!("Cover check complete");
pub const COVER_DOWNLOAD_FAILED: &str = N_!("Could not check album covers");

pub fn cover_download_progress(
    checked: usize,
    total: usize,
    downloaded: usize,
    unavailable: usize,
) -> String {
    let checked = checked.to_string();
    let total = total.to_string();
    let downloaded = downloaded.to_string();
    let unavailable = unavailable.to_string();
    formatted(
        N_!("{checked} of {total} checked · {downloaded} downloaded · {unavailable} unavailable"),
        &[
            ("checked", &checked),
            ("total", &total),
            ("downloaded", &downloaded),
            ("unavailable", &unavailable),
        ],
    )
}
pub const LIBRARY_FOLDER: &str = N_!("Library Folder");
pub const NO_LIBRARY_FOLDER: &str = N_!("No folder selected");
pub const CHOOSE_FOLDER: &str = N_!("Choose Folder…");
pub const RESTART_REQUIRED: &str = N_!("Restart required");
pub const EDIT_COLUMN_LAYOUT: &str = N_!("Edit column layout…");
pub const RESET_TO_DEFAULT: &str = N_!("Reset to Default");
pub const CLOSE: &str = N_!("Close");
pub const COLUMN_ALWAYS_VISIBLE: &str = N_!("Always visible");
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
pub const APPLY: &str = N_!("Apply");
pub const MULTIPLE_VALUES: &str = N_!("(multiple values)");
pub const TAG_TITLE: &str = N_!("Title");
pub const TAG_ARTIST: &str = N_!("Artist");
pub const TAG_ALBUM: &str = N_!("Album");
pub const TAG_ALBUM_ARTIST: &str = N_!("Album artist");
pub const TAG_YEAR: &str = N_!("Year");
pub const TAG_TRACK_NUMBER: &str = N_!("Track number");
pub const TAG_GENRE: &str = N_!("Genre");
pub const TAG_NUMBER_ERROR: &str = N_!("Year and track number must be positive whole numbers");
pub const TAG_EDIT_DATABASE_UNAVAILABLE: &str =
    N_!("Could not open the library database for tag editing");
pub const TAG_EDIT_WORKER_FAILED: &str = N_!("Could not start the tag-edit worker");
pub const TAG_SAME_ON_ALL: &str = N_!("same on all");

pub fn tag_applied_to_all_hint(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "Will be applied to {count} track",
        "Will be applied to all {count} tracks",
        count,
        &[("count", &count_text)],
    )
}

// --- Redesigned tag-editor dialog (Task 3) ---
// These constants and helpers are consumed by `present_redesigned` in
// `tag_editor.rs`. Task 4 switches the flow to call that function; until
// then the compiler sees them as dead code.

#[allow(dead_code)]
pub const TAG_EDIT_TITLE_SINGLE: &str = N_!("Edit Tags");
#[allow(dead_code)]
pub const TAG_EDIT_TITLE_MULTI: &str = N_!("Edit {count} Tracks");
#[allow(dead_code)]
pub const TAG_PER_TRACK: &str = N_!("per track");
#[allow(dead_code)]
pub const TAG_MIXED_COUNT: &str = N_!("{count} values");
#[allow(dead_code)]
pub const TAG_WILL_APPLY: &str = N_!("will be applied to all {count}");
#[allow(dead_code)]
pub const TAG_ALBUM_ARTIST_PLACEHOLDER: &str = N_!("Same as artist");
#[allow(dead_code)]
pub const TAG_SAVE: &str = N_!("Save");
#[allow(dead_code)]
pub const TAG_SAVE_COUNT: &str = N_!("Save {count}");
#[allow(dead_code)]
pub const TAG_PENDING_CHANGES: &str = N_!("{count} change pending");
#[allow(dead_code)]
pub const TAG_PENDING_CHANGES_PLURAL: &str = N_!("{count} changes pending");
#[allow(dead_code)]
pub const TAG_REVERT: &str = N_!("Revert");
#[allow(dead_code)]
pub const TAG_FETCH_MUSICBRAINZ: &str = N_!("Fetch tags from MusicBrainz");
#[allow(dead_code)]
pub const TAG_FETCH_HINT: &str = N_!("runs per track, fills only empty fields");
#[allow(dead_code)]
pub const TAG_UNSAVED_TITLE: &str = N_!("Save changes?");
#[allow(dead_code)]
pub const TAG_UNSAVED_SAVE: &str = N_!("Save");
#[allow(dead_code)]
pub const TAG_UNSAVED_DISCARD: &str = N_!("Discard");
#[allow(dead_code)]
pub const TAG_TRACK_POSITION: &str = N_!("Track {current} of {total}");
#[allow(dead_code)]
pub const TAG_CHANGE_COVER: &str = N_!("Change cover\u{2026}");

#[allow(dead_code)]
pub fn tag_edit_title_multi(count: usize) -> String {
    let count_text = count.to_string();
    formatted(TAG_EDIT_TITLE_MULTI, &[("count", &count_text)])
}

#[allow(dead_code)]
pub fn tag_save_count(count: usize) -> String {
    let count_text = count.to_string();
    formatted(TAG_SAVE_COUNT, &[("count", &count_text)])
}

#[allow(dead_code)]
pub fn tag_pending_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        TAG_PENDING_CHANGES,
        TAG_PENDING_CHANGES_PLURAL,
        count,
        &[("count", &count_text)],
    )
}

#[allow(dead_code)]
pub fn tag_track_position(current: usize, total: usize) -> String {
    formatted(
        TAG_TRACK_POSITION,
        &[
            ("current", &current.to_string()),
            ("total", &total.to_string()),
        ],
    )
}

#[allow(dead_code)]
pub fn tag_mixed_count(count: usize) -> String {
    let count_text = count.to_string();
    formatted(TAG_MIXED_COUNT, &[("count", &count_text)])
}

#[allow(dead_code)]
pub fn tag_will_apply(count: usize) -> String {
    let count_text = count.to_string();
    formatted(TAG_WILL_APPLY, &[("count", &count_text)])
}

#[allow(dead_code)]
pub fn tag_cover_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} cover",
        "{count} covers",
        count,
        &[("count", &count_text)],
    )
}
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

// Status bar (src/ui/status_bar.rs).
pub const STATUS_TRACK_SINGULAR: &str = N_!("track");
pub const STATUS_TRACK_PLURAL: &str = N_!("tracks");
/// Middle-dot separator between the track count and total duration, per the
/// design mockup (e.g. "1,704 tracks · 4 days, 6 hours and 28 minutes").
pub const STATUS_SEPARATOR: &str = N_!(" · ");

/// "{filtered} of {total}" prefix shown ahead of the track word while a
/// search filter is active (e.g. "42 of 1,704 tracks · …" instead of
/// "1,704 tracks · …") — see `status_bar::format_status_text`. `filtered`/
/// `total` are already formatted (en-US thousands, via `format::
/// format_thousands`); this function owns only the "of" wording.
pub fn status_filtered_of_total(filtered: &str, total: &str) -> String {
    formatted(
        N_!("{filtered} of {total}"),
        &[("filtered", filtered), ("total", total)],
    )
}

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
pub const QUEUE: &str = N_!("Queue");

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

// Track list context menu (src/ui/track_list.rs, Stage 3 Task 5): row
// actions on the current selection — Play, Add to queue, Add to playlist
// (submenu of existing playlists plus "New playlist…"), and — only while
// viewing a playlist — Remove from playlist.

pub const CONTEXT_MENU_PLAY: &str = N_!("Play");
pub const CONTEXT_MENU_ADD_TO_QUEUE: &str = N_!("Add to queue");
pub const CONTEXT_MENU_ADD_TO_PLAYLIST: &str = N_!("Add to playlist");
/// Leaf item at the bottom of the "Add to playlist" submenu — ellipsis
/// matches this file's convention for menu items that open a dialog (e.g.
/// `SCAN_FOLDER`), unlike the sidebar's plain "New playlist" row label
/// (`SIDEBAR_NEW_PLAYLIST`), which doesn't open a dialog directly from a
/// menu context.
pub const CONTEXT_MENU_NEW_PLAYLIST: &str = N_!("New playlist…");
pub const CONTEXT_MENU_REMOVE_FROM_PLAYLIST: &str = N_!("Remove from playlist");

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

/// Toast shown when `queries::remove_missing_track` fails while handling
/// the context menu's "Remove from library" action.
pub fn tracks_removed_from_library_failed_toast() -> String {
    text(N_!("Could not remove tracks from library"))
}

// Import-errors panel (src/ui/import_errors_view.rs, Stage 3 Task 8): a
// dedicated three-column (path/reason/time) view for the `import_errors`
// table, since its rows aren't `Track`s and don't fit the shared
// title/artist/… `ColumnView`.

pub const IMPORT_ERROR_COLUMN_PATH: &str = N_!("Path");
pub const IMPORT_ERROR_COLUMN_REASON: &str = N_!("Reason");
pub const IMPORT_ERROR_COLUMN_TIME: &str = N_!("Time");
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
pub const ABOUT_REPRISE: &str = N_!("About Reprise");
pub const REPRISE_ENGINE_AND_LINUX_PLATFORM: &str = N_!("Reprise Engine and Linux Platform");

// Native offline Help dialog and its keyboard shortcut descriptions.
pub const HELP: &str = N_!("Help");
pub const NAVIGATION: &str = N_!("Navigation");
pub const PLAY_OR_PAUSE: &str = N_!("Play or Pause");
pub const SEARCH_LIBRARY: &str = N_!("Search Library");
pub const TOGGLE_COMPACT_VIEW: &str = N_!("Toggle Compact View");
pub const CLEAR_SEARCH_OR_RETURN_TO_TRACK_LIST: &str = N_!("Clear Search or Return to Track List");
pub const PLAY_SELECTED_TRACK: &str = N_!("Play Selected Track");
pub const OPEN_CONTEXT_MENU: &str = N_!("Open Context Menu");
pub const OPEN_HELP: &str = N_!("Open Help");

// Primary menu items.
pub const MY_STATS: &str = N_!("My Stats");
pub const RESCAN_LIBRARY: &str = N_!("Rescan Library");
pub const SYNC_DEVICE: &str = N_!("Sync Device…");
pub const KEYBOARD_SHORTCUTS: &str = N_!("Keyboard Shortcuts");
pub const OPEN_KEYBOARD_SHORTCUTS: &str = N_!("Open Keyboard Shortcuts");

// Compact menu items.
pub const ALWAYS_ON_TOP: &str = N_!("Always on Top");
pub const QUIT: &str = N_!("Quit");
