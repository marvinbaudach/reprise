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
    status.set_hexpand(true);
    status.set_can_target(false);
    root.append(track_list);
    root.append(status);
    root
}

pub(in crate::ui) fn css() -> String {
    ".reprise-list-status-bar { \
       background-color: @sidebar_bg_color; \
       color: @reprise_secondary_fg_color; \
       border-top: 1px solid rgba(255, 255, 255, 0.06); }"
        .into()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gtk4::prelude::*;

    use super::*;
    use crate::ui::status_bar::StatusBar;

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
        let status_surface = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        status_surface.add_css_class("reprise-list-status-bar");
        status_surface.append(&gtk4::Label::new(Some("1,674 tracks")));
        let root = build(&tracks, &status_surface);
        let window = gtk4::Window::builder()
            .default_width(600)
            .default_height(400)
            .child(&root)
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let track_bounds = tracks.compute_bounds(&root).unwrap();
        assert!(status_surface.has_css_class("reprise-list-status-bar"));
        let status_bounds = status_surface.compute_bounds(&root).unwrap();
        assert!(status_bounds.height() > 0.0);
        assert!(
            track_bounds.y() + track_bounds.height() <= status_bounds.y(),
            "track allocation {track_bounds:?} overlapped status allocation {status_bounds:?}"
        );
        assert!(!root.is::<gtk4::Overlay>());
        status_surface.set_visible(false);
        assert!(!status_surface.is_visible());

        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn contrast_2_status_bar_renders_with_content() {
        gtk4::init().unwrap();
        let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));
        conn.borrow()
            .execute(
                "INSERT INTO tracks (path, title, artist, duration_ms, added_at) \
                 VALUES ('/tmp/rendered-status.ogg', 'Rendered status', 'Artist', 90000, 0)",
                [],
            )
            .unwrap();

        let tracks = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        tracks.set_vexpand(true);
        let status = StatusBar::new();
        status.set_enabled(false);
        let root = build(&tracks, status.widget());
        let window = gtk4::Window::builder()
            .default_width(600)
            .default_height(400)
            .child(&root)
            .build();
        window.present();

        // The initial Library reload may finish before persisted layout is
        // applied. Enabling the persisted status setting must reveal that
        // result without requiring a second reload.
        status.refresh(&conn);
        status.set_enabled(true);
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert!(status.widget().is_mapped(), "status surface is mapped");
        assert!(status.widget().height() > 0, "status surface is allocated");
        assert!(
            !status.label().text().is_empty(),
            "status label carries rendered library statistics"
        );

        status.hide();
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(
            !status.widget().is_mapped(),
            "non-Library sources keep the status surface hidden"
        );

        conn.borrow().execute("DELETE FROM tracks", []).unwrap();
        status.refresh(&conn);
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(
            !status.widget().is_mapped(),
            "an empty Library keeps the status surface hidden"
        );

        window.close();
    }
}
