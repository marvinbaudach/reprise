//! The fixed media slot shared by podcast and YouTube source rows.

use gtk4::prelude::*;

use super::skeleton::{MEDIA_HEIGHT, MEDIA_WIDTH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum MediaShape {
    /// 16:9 — YouTube thumbnails.
    Wide,
    /// 1:1 — podcast artwork and station logos.
    Square,
}

pub(in crate::ui) fn media_size(shape: MediaShape) -> (i32, i32) {
    match shape {
        MediaShape::Wide => (MEDIA_WIDTH, 36),
        MediaShape::Square => (36, 36),
    }
}

/// Places artwork in the source-row media slot without adding stateful
/// overlays. Selection belongs to the row tint and playback belongs beside
/// the title, so neither state can cover the image.
pub(in crate::ui) fn media(child: &impl IsA<gtk4::Widget>, shape: MediaShape) -> gtk4::Box {
    let (width, height) = media_size(shape);
    child.as_ref().set_size_request(width, height);
    child.as_ref().set_halign(gtk4::Align::Center);
    child.as_ref().set_valign(gtk4::Align::Center);

    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    root.add_css_class("reprise-source-row-media");
    root.set_size_request(MEDIA_WIDTH, MEDIA_HEIGHT);
    root.set_halign(gtk4::Align::Center);
    root.set_valign(gtk4::Align::Center);
    root.append(child);
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SRC-16`: 16:9 and square artwork occupy the same column, which is what
    /// puts the title at the same x position in both views.
    #[test]
    fn src_16_both_shapes_fit_the_same_column() {
        assert_eq!(media_size(MediaShape::Wide), (64, 36));
        assert_eq!(media_size(MediaShape::Square), (36, 36));
        for shape in [MediaShape::Wide, MediaShape::Square] {
            let (width, height) = media_size(shape);
            assert!(width <= MEDIA_WIDTH, "{shape:?} is wider than the column");
            assert!(
                height <= MEDIA_HEIGHT,
                "{shape:?} is taller than the column"
            );
        }
    }

    /// `SRC-12b`: the media slot contains artwork and no selection or
    /// playback overlay that can replace it.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_12b_the_media_slot_carries_only_the_playing_marker() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let artwork = gtk4::Image::new();
        let slot = media(&artwork, MediaShape::Wide);
        assert_eq!(slot.first_child().as_ref(), Some(artwork.upcast_ref()));
        assert!(
            artwork.next_sibling().is_none(),
            "selection and playback state must not cover the artwork"
        );
    }
}
