//! Centralized English UI strings. All user-facing text in the `ui` module
//! must come from here rather than being inlined at call sites — this keeps
//! translation and copy review a one-file affair.

pub const APP_NAME: &str = "Reprise";
pub const SEARCH_PLACEHOLDER: &str = "Search all fields";
pub const SCAN_FOLDER: &str = "Scan folder…";
pub const EMPTY_LIBRARY_TITLE: &str = "No music yet";
pub const EMPTY_LIBRARY_DESCRIPTION: &str = "Scan a folder to build your library";

// Track list column headers (src/ui/track_list.rs).
pub const COLUMN_TITLE: &str = "Title";
pub const COLUMN_ARTIST: &str = "Artist";
pub const COLUMN_ALBUM: &str = "Album";
pub const COLUMN_YEAR: &str = "Year";
pub const COLUMN_LENGTH: &str = "Length";
pub const COLUMN_RATING: &str = "Rating";
