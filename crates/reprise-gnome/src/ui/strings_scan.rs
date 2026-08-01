//! GTK renderer for scan messages described by `reprise-view`.

use reprise_view::strings::scan as messages;

// `RETRY` is the only msgid the frontend still names directly — it labels a
// button rather than arriving through a `Message`. The other six were either
// private before the move or publicly exported with no consumer at all; a
// blanket `#[allow(unused_imports)]` was keeping the latter alive, so they go
// rather than the lint.
pub use messages::RETRY;

fn borrowed<'a>(args: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    args.iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect()
}

pub(super) fn render(message: &reprise_view::strings::Message) -> String {
    let template = match &message.plural {
        // `ngettext` takes the count as a `u32`; saturating is the same
        // narrowing the pre-move `strings::plural` did, and no scan count
        // comes anywhere near the boundary.
        Some(plural) => crate::i18n::ngettext(
            message.id,
            plural.id,
            u32::try_from(plural.count).unwrap_or(u32::MAX),
        ),
        None => crate::i18n::gettext(message.id),
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
    fn a_zero_count_renders_the_plural_copy() {
        // English takes the plural at zero. A renderer that dropped the count
        // on its way through `Message` would show the singular here.
        assert_eq!(moved_files_relinked(0), "0 moved files relinked");
        assert_eq!(
            failed_files_imported(0),
            "0 previously failed files imported"
        );
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
