//! Translatable copy for sidebar issue-source cleanup.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}

fn plural(singular: &str, plural: &str, count: usize) -> String {
    let count_text = count.to_string();
    let count = u32::try_from(count).unwrap_or(u32::MAX);
    crate::i18n::format_message(
        &crate::i18n::ngettext(singular, plural, count),
        &[("count", &count_text)],
    )
}

pub const DISMISS_ALL_IMPORT_ERRORS: &str = N_!("Dismiss all import errors");
pub const REMOVE_ALL_MISSING_ENTRIES: &str = N_!("Remove all missing entries…");
pub const IMPORT_ERRORS_DISMISS_FAILED: &str = N_!("Could not dismiss import errors");
pub const REMOVE_MISSING_HEADING: &str = N_!("Remove All Missing Entries?");
pub const MISSING_ENTRIES_REMOVE_FAILED: &str = N_!("Could not remove missing entries");
pub const CANCEL: &str = N_!("Cancel");
pub const REMOVE: &str = N_!("Remove");

pub fn import_errors_dismissed(count: usize) -> String {
    plural(
        "Dismissed {count} import error",
        "Dismissed {count} import errors",
        count,
    )
}
