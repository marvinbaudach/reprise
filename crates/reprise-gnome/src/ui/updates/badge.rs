//! The ✦ header-bar trigger and its unseen-count badge.

use gtk4::prelude::*;

use crate::ui::strings;

// The badge's arithmetic is shared application logic, not presentation: a
// headless surface reports the same counts. It lives in `reprise_core::updates`
// and is re-exported here so the widgets keep one import path.
pub(in crate::ui) use reprise_core::updates::{updates_badge, FeedBadge as FeedBadgeInput};

pub(in crate::ui) fn build_button() -> (gtk4::MenuButton, gtk4::Label) {
    let glyph = gtk4::Label::new(Some("✦"));
    glyph.add_css_class("title-3");
    let badge = gtk4::Label::new(None);
    badge.add_css_class("new-release-badge");
    badge.set_halign(gtk4::Align::End);
    badge.set_valign(gtk4::Align::Start);
    badge.set_visible(false);
    badge.set_can_target(false);
    let overlay = gtk4::Overlay::new();
    overlay.add_css_class("updates-trigger");
    overlay.set_size_request(32, 32);
    overlay.set_overflow(gtk4::Overflow::Visible);
    overlay.set_child(Some(&glyph));
    overlay.add_overlay(&badge);
    let button = gtk4::MenuButton::builder()
        .child(&overlay)
        .tooltip_text(strings::text(strings::UPDATES_HEADER))
        .css_classes(["flat"])
        .visible(false)
        .build();
    button.set_overflow(gtk4::Overflow::Visible);
    button.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::UPDATES_HEADER,
    ))]);
    (button, badge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::updates::badge_text as badge_presentation;

    #[test]
    fn nr_9a_updates_badge_sums_only_enabled_ready_feeds_and_caps_at_nine_plus() {
        assert_eq!(badge_presentation(0), None);
        assert_eq!(badge_presentation(1), Some("1".to_string()));
        assert_eq!(badge_presentation(9), Some("9".to_string()));
        assert_eq!(badge_presentation(10), Some("9+".to_string()));
        assert_eq!(badge_presentation(42), Some("9+".to_string()));
        assert_eq!(badge_presentation(-3), None);
        let ready = |unseen| FeedBadgeInput {
            enabled: true,
            ready: true,
            unseen,
        };
        let off = FeedBadgeInput {
            enabled: false,
            ready: true,
            unseen: 99,
        };
        let pending = FeedBadgeInput {
            enabled: true,
            ready: false,
            unseen: 99,
        };
        assert_eq!(updates_badge(ready(4), ready(5)), Some("9".into()));
        assert_eq!(updates_badge(ready(4), ready(6)), Some("9+".into()));
        assert_eq!(updates_badge(ready(3), off), Some("3".into()));
        assert_eq!(updates_badge(pending, ready(2)), Some("2".into()));
        assert_eq!(updates_badge(off, pending), None);
    }
}
