//! Main-content stack sizing and source-transition policy.

use gtk4::prelude::*;

pub(super) fn build() -> gtk4::Stack {
    let stack = gtk4::Stack::new();
    super::library_player_bar::configure_content_stack(&stack);
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    stack.set_transition_duration(crate::ui::motion::STANDARD_MS);
    stack
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PageTransition {
    Crossfade,
    FadeThrough,
}

fn transition_for_switch(from: Option<&str>, to: &str) -> PageTransition {
    if matches!(
        (from, to),
        (Some("podcasts"), "library") | (Some("library"), "podcasts")
    ) {
        PageTransition::FadeThrough
    } else {
        PageTransition::Crossfade
    }
}

pub(in crate::ui) fn show_page(stack: &gtk4::Stack, name: &str) {
    let from = stack.visible_child_name();
    let transition = transition_for_switch(from.as_deref(), name);
    let Some(incoming) = stack.child_by_name(name) else {
        tracing::warn!(page = name, "content stack target is not installed");
        return;
    };
    // A dense source left transparent by an earlier fade-through becomes
    // fully visible before it is used as an incoming page again.
    incoming.set_opacity(1.0);
    if transition == PageTransition::FadeThrough {
        // MOT-8: retain the same Standard-duration surface transition as
        // other location switches without crossfading two readable tables.
        // Hiding only the outgoing child turns GtkStack's normal crossfade
        // into a single-surface fade-through; the incoming page still fades
        // in and the surrounding shell never hard-cuts.
        if let Some(outgoing) = from.as_deref().and_then(|name| stack.child_by_name(name)) {
            outgoing.set_opacity(0.0);
        }
    }
    stack.set_visible_child_full(name, gtk4::StackTransitionType::Crossfade);
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    #[test]
    fn mot_8_dense_source_switch_retains_standard_motion() {
        assert_eq!(
            super::transition_for_switch(Some("podcasts"), "library"),
            super::PageTransition::FadeThrough
        );
        assert_eq!(
            super::transition_for_switch(Some("library"), "podcasts"),
            super::PageTransition::FadeThrough
        );
        assert_eq!(
            super::transition_for_switch(Some("library"), "stats"),
            super::PageTransition::Crossfade
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
    fn mot_8_dense_source_switch_animates_without_overlapping_readable_tables() {
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
            podcasts.opacity() == 0.0,
            "the outgoing Podcast table must become visually unreadable before Music fades in"
        );
        assert!(
            stack.is_transition_running(),
            "dense source surfaces must retain the normal location-switch motion"
        );
        window.close();
    }
}
