//! Hosts the track list as the library content surface.

use gtk4::prelude::*;

pub(in crate::ui) fn build(track_list: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    track_list.set_vexpand(true);
    root.append(track_list);
    root
}

pub(in crate::ui) fn css() -> String {
    // CONTRAST-3's cross-surface inventory still parses this historical
    // selector. No widget carries the class now that CONTRAST-2b removed the
    // overlay, so this declaration cannot paint a surface.
    ".reprise-list-status-bar { color: @reprise_secondary_fg_color; }".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn contrast_2b_no_overlay_covers_a_row() {
        gtk4::init().unwrap();
        let tracks = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        tracks.set_vexpand(true);
        tracks.append(&gtk4::Label::new(Some("last track row")));
        let root = build(&tracks);
        let window = gtk4::Window::builder()
            .default_width(600)
            .default_height(400)
            .child(&root)
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let root_bounds = root.compute_bounds(&window).unwrap();
        let track_bounds = tracks.compute_bounds(&window).unwrap();
        assert_eq!(
            track_bounds, root_bounds,
            "track rows own the full content bounds"
        );
        assert!(
            root.observe_children().n_items() == 1,
            "no overlay sibling exists"
        );

        window.close();
    }
}
