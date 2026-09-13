//! Shared construction points for libadwaita rows and banners. Their titles
//! and subtitles are plain text, so markup characters cannot discard them.

use libadwaita as adw;

pub(in crate::ui) fn action_row() -> adw::builders::ActionRowBuilder {
    adw::ActionRow::builder().use_markup(false)
}

pub(in crate::ui) fn expander_row() -> adw::builders::ExpanderRowBuilder {
    adw::ExpanderRow::builder().use_markup(false)
}

pub(in crate::ui) fn switch_row() -> adw::builders::SwitchRowBuilder {
    adw::SwitchRow::builder().use_markup(false)
}

pub(in crate::ui) fn combo_row() -> adw::builders::ComboRowBuilder {
    adw::ComboRow::builder().use_markup(false)
}

pub(in crate::ui) fn entry_row() -> adw::builders::EntryRowBuilder {
    adw::EntryRow::builder().use_markup(false)
}

pub(in crate::ui) fn password_entry_row() -> adw::builders::PasswordEntryRowBuilder {
    adw::PasswordEntryRow::builder().use_markup(false)
}

pub(in crate::ui) fn banner(title: &str) -> adw::Banner {
    let banner = adw::Banner::new(title);
    banner.set_use_markup(false);
    banner
}
