use gtk4::prelude::*;

/// Makes each cell's interactive child fill the cell allocation. Gestures
/// attached to the child then work across the row cell instead of only on
/// the text or icon's natural-size pixels.
pub(super) fn expand_to_cell(widget: &impl IsA<gtk4::Widget>) {
    widget.set_hexpand(true);
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
        super::expand_to_cell(&label);
        assert!(label.hexpands());
    }
}
