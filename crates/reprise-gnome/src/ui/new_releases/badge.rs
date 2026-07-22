//! The ✦ header-bar trigger and its unseen-count badge.

use gtk4::prelude::*;

use crate::ui::strings;

/// Text for the counter badge, or `None` when nothing should be shown.
/// `0` (or negative) → `None` (no empty element); `1..=9` → `"n"`;
/// `>= 10` → `"9+"`.
pub(in crate::ui) fn badge_presentation(unseen: i64) -> Option<String> {
    match unseen {
        n if n <= 0 => None,
        1..=9 => Some(unseen.to_string()),
        _ => Some("9+".to_string()),
    }
}

pub(in crate::ui) fn build_button() -> (gtk4::MenuButton, gtk4::Label) {
    let glyph = gtk4::Label::new(Some("✦"));
    glyph.add_css_class("title-3");
    let badge = gtk4::Label::new(None);
    badge.add_css_class("new-release-badge");
    badge.set_halign(gtk4::Align::End);
    badge.set_valign(gtk4::Align::Start);
    badge.set_visible(false);
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&glyph));
    overlay.add_overlay(&badge);
    let button = gtk4::MenuButton::builder()
        .child(&overlay)
        .tooltip_text(strings::text(strings::NEW_RELEASES))
        .css_classes(["flat"])
        .visible(false)
        .build();
    button.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::NEW_RELEASES,
    ))]);
    (button, badge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nr_9_badge_counts_unseen_and_caps_at_nine_plus() {
        assert_eq!(badge_presentation(0), None);
        assert_eq!(badge_presentation(1), Some("1".to_string()));
        assert_eq!(badge_presentation(9), Some("9".to_string()));
        assert_eq!(badge_presentation(10), Some("9+".to_string()));
        assert_eq!(badge_presentation(42), Some("9+".to_string()));
        assert_eq!(badge_presentation(-3), None);
    }
}
