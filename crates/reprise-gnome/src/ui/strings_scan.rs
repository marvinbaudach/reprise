//! Scan completion and unavailable-root copy.

use super::{formatted, plural, text};

pub const LIBRARY_FOLDER_UNAVAILABLE: &str = N_!("Library folder unavailable");
pub const LIBRARY_FOLDER_NOT_MOUNTED: &str = N_!("{root} not mounted");
pub const RETRY: &str = N_!("Retry");
const MOVED_FILE_RELINKED_ONE: &str = N_!("1 moved file relinked");
const MOVED_FILES_RELINKED: &str = N_!("{count} moved files relinked");
const FAILED_FILE_IMPORTED_ONE: &str = N_!("1 previously failed file imported");
const FAILED_FILES_IMPORTED: &str = N_!("{count} previously failed files imported");

pub fn library_folder_not_mounted(root: &str) -> String {
    formatted(LIBRARY_FOLDER_NOT_MOUNTED, &[("root", root)])
}

pub fn moved_files_relinked(count: u32) -> String {
    plural(
        MOVED_FILE_RELINKED_ONE,
        MOVED_FILES_RELINKED,
        count as usize,
        &[("count", &count.to_string())],
    )
}

pub fn failed_files_imported(count: u32) -> String {
    plural(
        FAILED_FILE_IMPORTED_ONE,
        FAILED_FILES_IMPORTED,
        count as usize,
        &[("count", &count.to_string())],
    )
}

pub fn unavailable_title() -> String {
    text(LIBRARY_FOLDER_UNAVAILABLE)
}
