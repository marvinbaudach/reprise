//! Main-content stack sizing policy.

pub(super) fn build() -> gtk4::Stack {
    let stack = gtk4::Stack::new();
    stack.set_hhomogeneous(false);
    stack.set_vhomogeneous(false);
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    stack.set_transition_duration(crate::ui::motion::STANDARD_MS);
    stack
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn hidden_device_page_cannot_expand_the_visible_library_page() {
        gtk4::init().unwrap();
        let stack = super::build();

        assert!(!stack.is_hhomogeneous());
        assert!(
            !stack.is_vhomogeneous(),
            "hidden tall pages must not determine the visible page height"
        );
    }
}
