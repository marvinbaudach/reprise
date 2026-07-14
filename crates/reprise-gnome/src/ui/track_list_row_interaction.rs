use gtk4::prelude::*;

const REORDER_TARGET_CSS_CLASS: &str = "reprise-reorder-target";

/// Row drop-position indicator; installed app-wide by [`super::style`].
pub(super) fn css() -> String {
    use super::style::tokens::DROP_INDICATOR_THICKNESS;
    format!(
        ".{REORDER_TARGET_CSS_CLASS}:drop(active) {{ \
         box-shadow: inset 0 {DROP_INDICATOR_THICKNESS} @accent_color; }}"
    )
}

pub(super) fn set_reorder_indicator(widget: &impl IsA<gtk4::Widget>, active: bool) {
    if active {
        widget.add_css_class(REORDER_TARGET_CSS_CLASS);
    } else {
        widget.remove_css_class(REORDER_TARGET_CSS_CLASS);
    }
}

/// Marks and expands each app-owned cell child. The marker is the stable
/// target for live density sizing; filling the cell also lets attached
/// gestures work beyond the text or icon's natural-size pixels.
pub(super) fn expand_to_cell(widget: &impl IsA<gtk4::Widget>) {
    widget.add_css_class("reprise-track-cell");
    widget.set_hexpand(true);
    widget.set_halign(gtk4::Align::Fill);
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn interaction_surface_expands_to_the_whole_cell() {
        if gtk4::init().is_err() {
            return;
        }
        let label = gtk4::Label::new(Some("Short title"));
        label.set_halign(gtk4::Align::Start);
        super::expand_to_cell(&label);
        assert!(label.hexpands());
        assert_eq!(label.halign(), gtk4::Align::Fill);
    }

    #[test]
    fn reorder_indicator_uses_the_drop_active_state_and_accent_line() {
        let css = super::css();
        assert!(css.contains(":drop(active)"));
        assert!(css.contains("box-shadow"));
        assert!(css.contains("@accent_color"));
    }
}
