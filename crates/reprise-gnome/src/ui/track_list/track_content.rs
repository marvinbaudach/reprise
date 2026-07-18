//! Composes the track list with its compact statistics bar. The bar is a real
//! child below the list, so its surface stays deterministic and its allocation
//! never covers the final track row.

use gtk4::prelude::*;

pub(in crate::ui) fn build(
    track_list: &impl IsA<gtk4::Widget>,
    status: &impl IsA<gtk4::Widget>,
) -> gtk4::Box {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    track_list.set_vexpand(true);
    let status_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    status_bar.add_css_class("reprise-list-status-bar");
    status_bar.set_visible(status.is_visible());
    status.set_hexpand(true);
    status.set_halign(gtk4::Align::Fill);
    status.set_can_target(false);
    let status_bar_weak = status_bar.downgrade();
    status.connect_visible_notify(move |status| {
        if let Some(status_bar) = status_bar_weak.upgrade() {
            status_bar.set_visible(status.is_visible());
        }
    });
    status_bar.append(status);
    root.append(track_list);
    root.append(&status_bar);
    root
}

pub(in crate::ui) fn css() -> String {
    ".reprise-list-status-bar { \
       background-color: @sidebar_bg_color; \
       border-top: 1px solid rgba(255, 255, 255, 0.06); }"
        .into()
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    use super::*;

    #[test]
    fn contrast_2_status_bar_has_its_own_surface() {
        let css = css();

        assert!(css.contains(".reprise-list-status-bar"));
        assert!(css.contains("background-color: @sidebar_bg_color"));
        assert!(css.contains("border-top: 1px solid rgba(255, 255, 255, 0.06)"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn contrast_2_status_bar_reserves_space() {
        gtk4::init().unwrap();
        let tracks = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        tracks.set_vexpand(true);
        let status = gtk4::Label::new(Some("1,674 tracks"));
        let root = build(&tracks, &status);
        let window = gtk4::Window::builder()
            .default_width(600)
            .default_height(400)
            .child(&root)
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let track_bounds = tracks.compute_bounds(&root).unwrap();
        let status_surface = status.parent().unwrap();
        assert!(status_surface.has_css_class("reprise-list-status-bar"));
        assert_eq!(status_surface.parent().as_ref(), Some(root.upcast_ref()));
        let status_bounds = status_surface.compute_bounds(&root).unwrap();
        assert!(status_bounds.height() > 0.0);
        assert!(
            track_bounds.y() + track_bounds.height() <= status_bounds.y(),
            "track allocation {track_bounds:?} overlapped status allocation {status_bounds:?}"
        );
        assert!(!root.is::<gtk4::Overlay>());
        status.set_visible(false);
        assert!(!status_surface.is_visible());

        window.close();
    }
}
