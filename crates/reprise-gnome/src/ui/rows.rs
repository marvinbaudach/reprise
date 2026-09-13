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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gtk4::prelude::*;
    use libadwaita as adw;
    use libadwaita::prelude::*;

    use crate::ui::plain_text_display_tests::{rendered_label_texts, LabelSettle};

    fn rendered_row(row: &adw::ActionRow, settle: LabelSettle) -> (Vec<String>, Duration) {
        let group = adw::PreferencesGroup::new();
        group.add(row);
        rendered_label_texts(group.upcast_ref(), 480, 180, settle, || {})
    }

    // The control arm below expects the row's own title and subtitle to be
    // *absent* from the rendered labels — but an empty result also comes from
    // a harness that observed nothing at all (window never mapped, walk never
    // ran). A bare "absent" assertion cannot tell those apart, so a sentinel
    // label rides along in the same window: its presence proves the harness
    // was watching, keeping the negative assertions honest.
    const SENTINEL: &str = "rows harness sentinel";

    fn rendered_row_with_sentinel(
        row: &adw::ActionRow,
        settle: LabelSettle,
    ) -> (Vec<String>, Duration) {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let group = adw::PreferencesGroup::new();
        group.add(row);
        container.append(&group);
        container.append(&gtk4::Label::new(Some(SENTINEL)));
        rendered_label_texts(container.upcast_ref(), 480, 220, settle, || {})
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fb_13_row_plain_text_survives_markup_characters() {
        adw::init().expect("libadwaita must initialize under the display runner");
        let title = "Tom & Jerry <Live>";
        let subtitle = "AT&T > Radio";
        let banner_title = "Library & Radio <offline>";

        let plain_row = super::action_row().title(title).subtitle(subtitle).build();
        let (plain_labels, positive_wait) = rendered_row(&plain_row, LabelSettle::UntilText);
        assert!(plain_labels.iter().any(|label| label == title));
        assert!(plain_labels.iter().any(|label| label == subtitle));

        let banner = super::banner(banner_title);
        banner.set_revealed(true);
        let (banner_labels, _) =
            rendered_label_texts(banner.upcast_ref(), 480, 160, LabelSettle::UntilText, || {});
        assert!(banner_labels.iter().any(|label| label == banner_title));

        let markup_row = super::action_row()
            .use_markup(true)
            .title(title)
            .subtitle(subtitle)
            .build();
        let absence_wait = positive_wait.max(Duration::from_millis(100));
        let (markup_labels, _) =
            rendered_row_with_sentinel(&markup_row, LabelSettle::ObserveFor(absence_wait));
        assert!(
            markup_labels.iter().any(|label| label == SENTINEL),
            "the harness must still observe rendered labels while the markup arm swallows the title"
        );
        assert!(!markup_labels.iter().any(|label| label == title));
        assert!(!markup_labels.iter().any(|label| label == subtitle));
    }
}
