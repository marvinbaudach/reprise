//! GTK renderer for scan messages described by `reprise-view`.

use reprise_view::strings::scan as messages;

#[allow(unused_imports)]
pub use messages::{
    FAILED_FILES_IMPORTED, FAILED_FILE_IMPORTED_ONE, LIBRARY_FOLDER_NOT_MOUNTED,
    LIBRARY_FOLDER_UNAVAILABLE, MOVED_FILES_RELINKED, MOVED_FILE_RELINKED_ONE, RETRY,
};

fn borrowed<'a>(args: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    args.iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect()
}

pub(super) fn render(message: &reprise_view::strings::Message) -> String {
    let template = match (message.plural_id, message.count) {
        (Some(plural_id), Some(count)) => crate::i18n::ngettext(
            message.id,
            plural_id,
            u32::try_from(count).unwrap_or(u32::MAX),
        ),
        _ => crate::i18n::gettext(message.id),
    };
    crate::i18n::format_message(&template, &borrowed(&message.args))
}

pub fn library_folder_not_mounted(root: &str) -> String {
    render(&messages::library_folder_not_mounted(root))
}

pub fn moved_files_relinked(count: u32) -> String {
    render(&messages::moved_files_relinked(count))
}

pub fn failed_files_imported(count: u32) -> String {
    render(&messages::failed_files_imported(count))
}

pub fn unavailable_title() -> String {
    render(&messages::unavailable_title())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_folder_not_mounted_preserves_the_rendered_copy() {
        assert_eq!(library_folder_not_mounted("/music"), "/music not mounted");
    }

    #[test]
    fn moved_files_relinked_preserves_singular_and_plural_copy() {
        assert_eq!(moved_files_relinked(1), "1 moved file relinked");
        assert_eq!(moved_files_relinked(2), "2 moved files relinked");
    }

    #[test]
    fn failed_files_imported_preserves_singular_and_plural_copy() {
        assert_eq!(
            failed_files_imported(1),
            "1 previously failed file imported"
        );
        assert_eq!(
            failed_files_imported(2),
            "2 previously failed files imported"
        );
    }

    #[test]
    fn unavailable_title_preserves_the_rendered_copy() {
        assert_eq!(unavailable_title(), "Library folder unavailable");
    }
}
