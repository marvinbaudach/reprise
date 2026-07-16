//! Finished, centered placeholders for Information-panel contexts that
//! cannot request Artist News.

use libadwaita as adw;

use super::info_panel_state::PanelContext;
use super::strings;

pub(in crate::ui) fn build(context: &PanelContext) -> Option<adw::StatusPage> {
    let (icon, title, description) = match context {
        PanelContext::Empty => (
            "folder-music-symbolic",
            strings::text(strings::NEWS_SELECT_TRACK),
            None,
        ),
        PanelContext::Multiple(count) => (
            "edit-select-all-symbolic",
            strings::tracks_selected(*count),
            Some(strings::text(strings::NEWS_MULTIPLE_SELECTION)),
        ),
        PanelContext::Track(track) if track.artist.trim().is_empty() => (
            "avatar-default-symbolic",
            strings::text(strings::NEWS_NO_ARTIST),
            None,
        ),
        PanelContext::Track(_) => return None,
    };

    let page = adw::StatusPage::builder()
        .icon_name(icon)
        .title(title)
        .vexpand(true);
    let page = match description {
        Some(description) => page.description(description),
        None => page,
    };
    Some(page.build())
}
