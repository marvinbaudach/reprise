use gtk4::prelude::*;
use reprise_core::podcasts::PodcastKind;

use crate::ui::strings;
use crate::ui::style::buttons;

pub(super) const REFRESH_LABEL_PAGE: &str = "label";
pub(super) const REFRESH_SPINNER_PAGE: &str = "spinner";

pub(super) struct PodcastsFooter {
    pub root: gtk4::Box,
    pub add: gtk4::Button,
    pub status: gtk4::Label,
    pub spinner: gtk4::Spinner,
    pub refresh_button: gtk4::Button,
    pub refresh_stack: gtk4::Stack,
    pub refresh_spinner: gtk4::Spinner,
}

pub(super) fn build(kind: PodcastKind) -> PodcastsFooter {
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    root.set_margin_top(6);
    root.set_margin_bottom(6);
    root.set_margin_start(12);
    root.set_margin_end(12);
    let add = gtk4::Button::builder()
        .label(strings::text(match kind {
            PodcastKind::Rss => strings::PODCAST_ADD,
            PodcastKind::Youtube => strings::YOUTUBE_ADD,
        }))
        .build();
    buttons::arm(&add, buttons::ADD_ACTION_CLASS);
    add.set_action_name(Some("podcasts.open-add"));
    root.append(&add);
    let spinner = gtk4::Spinner::new();
    root.append(&spinner);
    let status = gtk4::Label::new(None);
    status.add_css_class("caption");
    status.add_css_class("dim-label");
    status.set_hexpand(true);
    status.set_xalign(0.0);
    root.append(&status);

    let label = gtk4::Label::new(Some(&strings::text(strings::PODCAST_REFRESH_NOW)));
    let refresh_spinner = gtk4::Spinner::new();
    let refresh_stack = gtk4::Stack::new();
    refresh_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    refresh_stack.add_named(&label, Some(REFRESH_LABEL_PAGE));
    refresh_stack.add_named(&refresh_spinner, Some(REFRESH_SPINNER_PAGE));
    refresh_stack.set_visible_child_name(REFRESH_LABEL_PAGE);
    let refresh_button = gtk4::Button::new();
    refresh_button.set_child(Some(&refresh_stack));
    refresh_button.add_css_class("flat");
    root.append(&refresh_button);

    PodcastsFooter {
        root,
        add,
        status,
        spinner,
        refresh_button,
        refresh_stack,
        refresh_spinner,
    }
}
