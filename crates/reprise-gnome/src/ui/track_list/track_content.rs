//! Composes the track list with its compact statistics overlay. The pill is
//! aligned to the content pane rather than the window, so opening the
//! end-positioned Now Playing column keeps it pinned beside that column.

use gtk4::prelude::*;

const STATUS_EDGE_INSET: i32 = 16;

pub(in crate::ui) fn build(
    track_list: &impl IsA<gtk4::Widget>,
    status: &impl IsA<gtk4::Widget>,
) -> gtk4::Overlay {
    let root = gtk4::Overlay::new();
    track_list.set_vexpand(true);
    root.set_child(Some(track_list));
    status.set_halign(gtk4::Align::End);
    status.set_valign(gtk4::Align::End);
    status.set_margin_end(STATUS_EDGE_INSET);
    status.set_margin_bottom(STATUS_EDGE_INSET);
    status.set_can_target(false);
    root.add_overlay(status);
    root
}

pub(in crate::ui) fn css() -> String {
    ".reprise-list-status-bar { \
       background-color: @sidebar_bg_color; \
       color: @reprise_secondary_fg_color; \
       border: 1px solid rgba(255, 255, 255, 0.10); \
       border-radius: 999px; }"
        .into()
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use gtk4::prelude::*;
    use libadwaita as adw;

    use super::*;
    use crate::ui::status_bar::StatusBar;

    #[test]
    fn contrast_2a_status_overlay_has_a_compact_surface() {
        let css = css();

        assert!(css.contains(".reprise-list-status-bar"));
        assert!(css.contains("background-color: @sidebar_bg_color"));
        assert!(css.contains("border: 1px solid rgba(255, 255, 255, 0.10)"));
        assert!(css.contains("border-radius: 999px"));
        assert!(!css.contains("border-top:"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn status_overlay_stays_at_the_table_edge_when_the_right_panel_opens() {
        gtk4::init().unwrap();
        let tracks = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        tracks.set_vexpand(true);
        let status_surface = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        status_surface.add_css_class("reprise-list-status-bar");
        status_surface.append(&gtk4::Label::new(Some("1,674 tracks")));
        let root = build(&tracks, &status_surface);
        fn require_overlay(_: &gtk4::Overlay) {}
        require_overlay(&root);

        let panel = adw::ToolbarView::new();
        panel.set_width_request(crate::ui::now_playing_column::PANEL_WIDTH);
        let split = adw::OverlaySplitView::builder()
            .content(&root)
            .sidebar(&panel)
            .sidebar_position(gtk4::PackType::End)
            .show_sidebar(true)
            .collapsed(false)
            .min_sidebar_width(f64::from(crate::ui::now_playing_column::PANEL_WIDTH))
            .max_sidebar_width(f64::from(crate::ui::now_playing_column::PANEL_WIDTH))
            .build();
        let window = gtk4::Window::builder()
            .default_width(900)
            .default_height(400)
            .child(&split)
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let root_bounds = root.compute_bounds(&split).unwrap();
        assert!(status_surface.has_css_class("reprise-list-status-bar"));
        let status_bounds = status_surface.compute_bounds(&split).unwrap();
        assert!(status_bounds.height() > 0.0);
        assert!(
            status_bounds.width() < root_bounds.width() / 2.0,
            "status surface {status_bounds:?} expanded across content {root_bounds:?}"
        );
        assert!(
            (root_bounds.x() + root_bounds.width()
                - status_bounds.x()
                - status_bounds.width()
                - STATUS_EDGE_INSET as f32)
                .abs()
                <= 1.0,
            "status right edge {status_bounds:?} did not follow content edge {root_bounds:?}"
        );
        assert!(root.is::<gtk4::Overlay>());
        assert_eq!(status_surface.halign(), gtk4::Align::End);
        assert_eq!(status_surface.valign(), gtk4::Align::End);
        status_surface.set_visible(false);
        assert!(!status_surface.is_visible());

        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn contrast_2a_status_overlay_renders_with_content() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        crate::test_db::connection(&conn)
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
        crate::ui::test_settle::settle_until_mapped(status.widget());

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

        crate::test_db::connection(&conn)
            .execute("DELETE FROM tracks", [])
            .unwrap();
        status.refresh(&conn);
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(
            !status.widget().is_mapped(),
            "an empty Library keeps the status surface hidden"
        );

        window.close();
    }
}
