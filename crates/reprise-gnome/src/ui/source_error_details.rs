use gtk4::prelude::*;
use reprise_core::source_error::SourceError;

use crate::ui::strings;

pub(super) fn details_text(error: &SourceError, occurred_at: &str) -> String {
    error.details(occurred_at).to_string()
}

pub(super) struct SourceErrorDetails {
    root: gtk4::Box,
    revealer: gtk4::Revealer,
    text: gtk4::Label,
}

impl SourceErrorDetails {
    pub(super) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let toggle = gtk4::Button::with_label(&strings::text(strings::SOURCE_DETAILS));
        toggle.add_css_class("flat");
        toggle.set_halign(gtk4::Align::Start);
        root.append(&toggle);

        let details_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let text = gtk4::Label::new(None);
        text.add_css_class("monospace");
        text.add_css_class("caption");
        text.set_selectable(true);
        text.set_wrap(true);
        text.set_xalign(0.0);
        details_box.append(&text);
        let copy = gtk4::Button::with_label(&strings::text(strings::SOURCE_COPY_DETAILS));
        copy.add_css_class("flat");
        copy.set_halign(gtk4::Align::Start);
        details_box.append(&copy);

        let revealer = gtk4::Revealer::new();
        revealer.set_reveal_child(false);
        revealer.set_child(Some(&details_box));
        root.append(&revealer);

        {
            let revealer = revealer.clone();
            toggle.connect_clicked(move |_| {
                revealer.set_reveal_child(!revealer.reveals_child());
            });
        }
        {
            let text = text.clone();
            copy.connect_clicked(move |_| {
                if let Some(display) = gtk4::gdk::Display::default() {
                    display.clipboard().set_text(text.text().as_str());
                }
            });
        }
        Self {
            root,
            revealer,
            text,
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(super) fn set_error(&self, error: &SourceError, occurred_at: &str) {
        self.text.set_text(&details_text(error, occurred_at));
        self.revealer.set_reveal_child(false);
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::source_error::{SourceError, SourceErrorKind};

    use super::details_text;

    #[test]
    fn net_3_d_details_are_available_only_in_the_explicit_details_block() {
        let error = SourceError::new(
            SourceErrorKind::Unreachable,
            "Fetch channel",
            "HTTP 503 from private.example",
        );

        let text = details_text(&error, "2026-07-30 12:00 UTC");

        assert_eq!(
            text,
            "Fetch channel\nHTTP 503 from private.example\n2026-07-30 12:00 UTC"
        );
        assert!(!error.to_string().contains("HTTP"));
        assert!(!error.to_string().contains("private.example"));
    }
}
