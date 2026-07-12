use gtk4::prelude::*;

const REORDER_TARGET_CSS_CLASS: &str = "reprise-reorder-target";

pub(super) fn reorder_indicator_css() -> String {
    format!(
        ".{REORDER_TARGET_CSS_CLASS}:drop(active) {{ \
         box-shadow: inset 0 2px @accent_color; }}"
    )
}

pub(super) fn install_reorder_indicator_style(widget: &impl IsA<gtk4::Widget>) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&reorder_indicator_css());
    gtk4::style_context_add_provider_for_display(
        &widget.display(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

pub(super) fn set_reorder_indicator(widget: &impl IsA<gtk4::Widget>, active: bool) {
    if active {
        widget.add_css_class(REORDER_TARGET_CSS_CLASS);
    } else {
        widget.remove_css_class(REORDER_TARGET_CSS_CLASS);
    }
}

/// Makes each cell's interactive child fill the cell allocation. Gestures
/// attached to the child then work across the row cell instead of only on
/// the text or icon's natural-size pixels.
pub(super) fn expand_to_cell(widget: &impl IsA<gtk4::Widget>) {
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
        let css = super::reorder_indicator_css();
        assert!(css.contains(":drop(active)"));
        assert!(css.contains("box-shadow"));
        assert!(css.contains("@accent_color"));
    }
}
