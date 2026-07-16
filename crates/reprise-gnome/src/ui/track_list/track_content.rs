//! Composes the track list with its compact statistics label. The label is
//! an overlay aligned to the content's bottom-right corner, so it does not
//! reserve a full row between the list and the player bar.

use gtk4::prelude::*;

pub(in crate::ui) fn build(
    track_list: &impl IsA<gtk4::Widget>,
    status: &impl IsA<gtk4::Widget>,
) -> gtk4::Overlay {
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(track_list));
    status.set_halign(gtk4::Align::End);
    status.set_valign(gtk4::Align::End);
    status.set_can_target(false);
    overlay.add_overlay(status);
    overlay
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn status_is_a_compact_bottom_right_overlay() {
        gtk4::init().unwrap();
        let tracks = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let status = gtk4::Label::new(Some("1,674 tracks"));

        let overlay = build(&tracks, &status);

        assert_eq!(overlay.child().as_ref(), Some(tracks.upcast_ref()));
        assert_eq!(status.parent().as_ref(), Some(overlay.upcast_ref()));
        assert_eq!(status.halign(), gtk4::Align::End);
        assert_eq!(status.valign(), gtk4::Align::End);
        assert!(!status.can_target());
    }
}
