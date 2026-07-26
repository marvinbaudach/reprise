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

pub(super) fn link(
    text: &str,
    css_class: &str,
    target: StatsMetadataTarget,
    callback: &MetadataCallback,
) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_halign(gtk4::Align::Start);
    label.set_xalign(0.0);
    label.add_css_class("stats-metadata-link");
    label.add_css_class(css_class);
    let callback = callback.clone();
    crate::ui::link_activation::arm(
        &label,
        text,
        Rc::new(move || {
            let callback = callback.borrow().clone();
            if let Some(callback) = callback {
                callback(target.clone());
            }
        }),
    );
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_14_metadata_links_are_compact_keyboard_links() {
        gtk4::init().unwrap();
        let callback: MetadataCallback = Rc::new(RefCell::new(None));
        let link = link(
            "Track",
            "stats-item-title",
            StatsMetadataTarget::Track(1),
            &callback,
        );

        assert!(link.has_css_class("stats-metadata-link"));
        assert!(link.is_focusable());
        assert_eq!(link.accessible_role(), gtk4::AccessibleRole::Link);
    }
}
