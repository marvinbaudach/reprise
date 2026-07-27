//! Main-content stack sizing and source-transition policy.

pub(super) fn build() -> gtk4::Stack {
    let stack = gtk4::Stack::new();
    super::library_player_bar::configure_content_stack(&stack);
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    stack.set_transition_duration(crate::ui::motion::STANDARD_MS);
    stack
}

fn transition_for_switch(from: Option<&str>, to: &str) -> gtk4::StackTransitionType {
    // MOT-4: dense source tables must never remain simultaneously readable.
    // Keep the outer shell's Standard crossfade for other page switches, but
    // make the Music/Podcasts boundary atomic in both directions.
    if matches!(
        (from, to),
        (Some("podcasts"), "library") | (Some("library"), "podcasts")
    ) {
        gtk4::StackTransitionType::None
    } else {
        gtk4::StackTransitionType::Crossfade
    }
}

pub(super) fn show_page(stack: &gtk4::Stack, name: &str) {
    let from = stack.visible_child_name();
    stack.set_visible_child_full(name, transition_for_switch(from.as_deref(), name));
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    #[test]
    fn mot_4_dense_source_switch_uses_no_overlap_transition() {
        assert_eq!(
            super::transition_for_switch(Some("podcasts"), "library"),
            gtk4::StackTransitionType::None
        );
        assert_eq!(
            super::transition_for_switch(Some("library"), "podcasts"),
            gtk4::StackTransitionType::None
        );
        assert_eq!(
            super::transition_for_switch(Some("library"), "stats"),
            gtk4::StackTransitionType::Crossfade
        );
    }

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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mot_4_dense_source_switch_never_overlaps_readable_tables() {
        gtk4::init().unwrap();
        let stack = super::build();
        let library = gtk4::Label::new(Some("Music table"));
        let podcasts = gtk4::Label::new(Some("Podcast table"));
        stack.add_named(&library, Some("library"));
        stack.add_named(&podcasts, Some("podcasts"));
        stack.set_visible_child_name("podcasts");
        let window = gtk4::Window::builder().child(&stack).build();

        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(podcasts.is_mapped(), "precondition: Podcasts is visible");

        super::show_page(&stack, "library");
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert!(
            library.is_mapped(),
            "Music must be visible after the switch"
        );
        assert!(
            !podcasts.is_mapped(),
            "the outgoing Podcast table must not remain readable over Music"
        );
        assert!(
            !stack.is_transition_running(),
            "dense source surfaces must switch without an overlapping crossfade"
        );
        window.close();
    }
}
