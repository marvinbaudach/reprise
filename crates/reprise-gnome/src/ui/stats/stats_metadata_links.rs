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
    button.add_css_class("link");
    button.add_css_class(css_class);
    let callback = callback.clone();
    button.connect_clicked(move |_| {
        if let Some(callback) = callback.borrow().clone() {
            callback(target.clone());
        }
    });
    button
}
