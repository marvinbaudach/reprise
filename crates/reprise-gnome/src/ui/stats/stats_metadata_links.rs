//! Shared metadata-link payload and presentation for My Stats track rows.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

pub(super) type MetadataCallback = Rc<RefCell<Option<Rc<dyn Fn(StatsMetadataTarget)>>>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) enum StatsMetadataTarget {
    Track(i64),
    Album {
        track_id: i64,
        album: String,
        album_artist: String,
    },
    Artist {
        track_id: i64,
        artist: String,
    },
}

pub(super) fn button(
    text: &str,
    css_class: &str,
    target: StatsMetadataTarget,
    callback: &MetadataCallback,
) -> gtk4::Button {
    let button = gtk4::Button::with_label(text);
    button.set_halign(gtk4::Align::Start);
    button.add_css_class("flat");
    button.add_css_class("stats-metadata-link");
    button.add_css_class(css_class);
    let callback = callback.clone();
    button.connect_clicked(move |_| {
        let callback = callback.borrow().clone();
        if let Some(callback) = callback {
            callback(target.clone());
        }
    });
    button
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_14_metadata_links_are_plain_until_hover() {
        gtk4::init().unwrap();
        let callback: MetadataCallback = Rc::new(RefCell::new(None));
        let button = button(
            "Track",
            "stats-item-title",
            StatsMetadataTarget::Track(1),
            &callback,
        );

        assert!(button.has_css_class("stats-metadata-link"));
        assert!(!button.has_css_class("link"));
        assert!(button.is_focusable());
    }
}
