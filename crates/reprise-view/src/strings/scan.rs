//! Scan completion and unavailable-root messages.

use super::Message;

pub const LIBRARY_FOLDER_UNAVAILABLE: &str = N_!("Library folder unavailable");
pub const LIBRARY_FOLDER_NOT_MOUNTED: &str = N_!("{root} not mounted");
pub const RETRY: &str = N_!("Retry");
pub const MOVED_FILE_RELINKED_ONE: &str = N_!("1 moved file relinked");
pub const MOVED_FILES_RELINKED: &str = N_!("{count} moved files relinked");
pub const FAILED_FILE_IMPORTED_ONE: &str = N_!("1 previously failed file imported");
pub const FAILED_FILES_IMPORTED: &str = N_!("{count} previously failed files imported");

pub fn library_folder_not_mounted(root: &str) -> Message {
    Message {
        id: LIBRARY_FOLDER_NOT_MOUNTED,
        plural_id: None,
        count: None,
        args: vec![("root", root.to_owned())],
    }
}

pub fn moved_files_relinked(count: u32) -> Message {
    Message {
        id: MOVED_FILE_RELINKED_ONE,
        plural_id: Some(MOVED_FILES_RELINKED),
        count: Some(u64::from(count)),
        args: vec![("count", count.to_string())],
    }
}

pub fn failed_files_imported(count: u32) -> Message {
    Message {
        id: FAILED_FILE_IMPORTED_ONE,
        plural_id: Some(FAILED_FILES_IMPORTED),
        count: Some(u64::from(count)),
        args: vec![("count", count.to_string())],
    }
}

pub fn unavailable_title() -> Message {
    Message {
        id: LIBRARY_FOLDER_UNAVAILABLE,
        plural_id: None,
        count: None,
        args: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strings::Message;

    #[test]
    fn scan_messages_carry_msgids_plural_forms_counts_and_named_arguments() {
        assert_eq!(
            library_folder_not_mounted("/music"),
            Message {
                id: "{root} not mounted",
                plural_id: None,
                count: None,
                args: vec![("root", "/music".to_owned())],
            }
        );
        assert_eq!(
            moved_files_relinked(2),
            Message {
                id: "1 moved file relinked",
                plural_id: Some("{count} moved files relinked"),
                count: Some(2),
                args: vec![("count", "2".to_owned())],
            }
        );
        assert_eq!(
            failed_files_imported(3),
            Message {
                id: "1 previously failed file imported",
                plural_id: Some("{count} previously failed files imported"),
                count: Some(3),
                args: vec![("count", "3".to_owned())],
            }
        );
        assert_eq!(
            unavailable_title(),
            Message {
                id: "Library folder unavailable",
                plural_id: None,
                count: None,
                args: Vec::new(),
            }
        );
    }
}
