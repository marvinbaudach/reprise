//! Full-text tooltips for labels only while their rendered text is ellipsized.

use gtk4::prelude::*;

pub(in crate::ui) fn arm(label: &gtk4::Label) {
    label.set_has_tooltip(true);
    label.connect_query_tooltip(|label, _x, _y, _keyboard, tooltip| {
        let (_, natural, _, _) = label.measure(gtk4::Orientation::Horizontal, -1);
        let text = label.text();
        if tooltip_text(&text, natural, label.width()).is_some() {
            tooltip.set_text(Some(&text));
            true
        } else {
            false
        }
    });
}

fn tooltip_text(text: &str, natural_width: i32, allocated_width: i32) -> Option<&str> {
    (natural_width > allocated_width).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tip_1c_ellipsis_tooltip_returns_exact_full_text_only_when_truncated() {
        assert_eq!(
            tooltip_text("Complete title", 120, 80),
            Some("Complete title")
        );
        assert_eq!(tooltip_text("Complete title", 80, 120), None);
        assert_eq!(tooltip_text("Complete title", 80, 80), None);
    }
}
