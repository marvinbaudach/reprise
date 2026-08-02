//! Scan completion and unavailable-root messages.

use super::{Message, Plural};

/// The one msgid a surface still names directly: it labels a Retry button
/// rather than arriving through a [`Message`].
pub const RETRY: &str = N_!("Retry");

// Everything below is the catalog's own vocabulary. Surfaces receive finished
// `Message` values from the functions, never these literals, so none of them
// needs to be public — that is also what keeps `xgettext` as the only reader
// of the raw text.
const LIBRARY_FOLDER_UNAVAILABLE: &str = N_!("Library folder unavailable");
const LIBRARY_FOLDER_NOT_MOUNTED: &str = N_!("{root} not mounted");
const MOVED_FILE_RELINKED_ONE: &str = N_!("1 moved file relinked");
const MOVED_FILES_RELINKED: &str = N_!("{count} moved files relinked");
const FAILED_FILE_IMPORTED_ONE: &str = N_!("1 previously failed file imported");
const FAILED_FILES_IMPORTED: &str = N_!("{count} previously failed files imported");

pub fn library_folder_not_mounted(root: &str) -> Message {
    Message {
        id: LIBRARY_FOLDER_NOT_MOUNTED,
        plural: None,
        args: vec![("root", root.to_owned())],
    }
}

pub fn moved_files_relinked(count: u32) -> Message {
    Message {
        id: MOVED_FILE_RELINKED_ONE,
        plural: Some(Plural {
            id: MOVED_FILES_RELINKED,
            count: u64::from(count),
        }),
        args: vec![("count", count.to_string())],
    }
}

pub fn failed_files_imported(count: u32) -> Message {
    Message {
        id: FAILED_FILE_IMPORTED_ONE,
        plural: Some(Plural {
            id: FAILED_FILES_IMPORTED,
            count: u64::from(count),
        }),
        args: vec![("count", count.to_string())],
    }
}

pub fn unavailable_title() -> Message {
    Message {
        id: LIBRARY_FOLDER_UNAVAILABLE,
        plural: None,
        args: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_messages_carry_msgids_plural_forms_counts_and_named_arguments() {
        assert_eq!(
            library_folder_not_mounted("/music"),
            Message {
                id: "{root} not mounted",
                plural: None,
                args: vec![("root", "/music".to_owned())],
            }
        );
        assert_eq!(
            moved_files_relinked(2),
            Message {
                id: "1 moved file relinked",
                plural: Some(Plural {
                    id: "{count} moved files relinked",
                    count: 2,
                }),
                args: vec![("count", "2".to_owned())],
            }
        );
        assert_eq!(
            failed_files_imported(3),
            Message {
                id: "1 previously failed file imported",
                plural: Some(Plural {
                    id: "{count} previously failed files imported",
                    count: 3,
                }),
                args: vec![("count", "3".to_owned())],
            }
        );
        assert_eq!(
            unavailable_title(),
            Message {
                id: "Library folder unavailable",
                plural: None,
                args: Vec::new(),
            }
        );
    }

    #[test]
    fn a_zero_count_still_describes_the_plural_form() {
        // English selects the plural at zero, so a count of none must not
        // collapse into the singular msgid on its way through the boundary.
        let message = moved_files_relinked(0);
        assert_eq!(message.plural.as_ref().map(|plural| plural.count), Some(0));
        assert_eq!(message.args, vec![("count", "0".to_owned())]);
    }
}
