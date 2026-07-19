use gtk4::glib;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_11_switcher_is_centred_when_wide() {
    if gtk4::init().is_err() {
        return;
    }
    let window = adw::ApplicationWindow::builder()
        .default_width(1_000)
        .default_height(600)
        .build();
    let header = adw::HeaderBar::new();
    let title = adw::WindowTitle::new("Music", "");
    let views = test_view_stack();
    window.set_content(Some(&header));

    let library_title = build_library_title(&window, &header, &title, &views);
    window.present();
    drain_main_context();

    let switcher_bounds = library_title
        .switcher
        .compute_bounds(&header)
        .expect("wide switcher must be laid out inside the header");
    let switcher_centre = switcher_bounds.x() + switcher_bounds.width() / 2.0;
    let header_centre = header.width() as f32 / 2.0;
    assert!(
        (switcher_centre - header_centre).abs() <= 1.0,
        "switcher centre {switcher_centre} must match header centre {header_centre}"
    );
    assert_eq!(
        visible_descendants_of_type::<gtk4::Label>(&library_title.switcher).len(),
        3,
        "the wide switcher must visibly name every view"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_11_switcher_degrades_when_narrow() {
    if gtk4::init().is_err() {
        return;
    }
    let window = adw::ApplicationWindow::builder()
        .default_width(480)
        .default_height(600)
        .build();
    let header = adw::HeaderBar::new();
    let title = adw::WindowTitle::new("Music", "");
    let views = test_view_stack();
    window.set_content(Some(&header));

    let library_title = build_library_title(&window, &header, &title, &views);
    window.present();
    drain_main_context();

    assert!(
        visible_descendants_of_type::<gtk4::Label>(&library_title.switcher).is_empty(),
        "the narrow switcher must not render text labels"
    );
    assert_eq!(
        visible_descendants_of_type::<gtk4::Image>(&library_title.switcher).len(),
        3,
        "the narrow switcher must retain one visible icon per view"
    );
}

fn test_view_stack() -> adw::ViewStack {
    let stack = adw::ViewStack::new();
    stack.add_titled_with_icon(
        &gtk4::Label::new(Some("Tracks")),
        Some("tracks"),
        "Tracks",
        "view-list-symbolic",
    );
    stack.add_titled_with_icon(
        &gtk4::Label::new(Some("Albums")),
        Some("albums"),
        "Albums",
        "media-optical-cd-audio-symbolic",
    );
    stack.add_titled_with_icon(
        &gtk4::Label::new(Some("Artists")),
        Some("artists"),
        "Artists",
        "avatar-default-symbolic",
    );
    stack
}

fn visible_descendants_of_type<T: IsA<gtk4::Widget> + glib::types::StaticType>(
    root: &impl IsA<gtk4::Widget>,
) -> Vec<T> {
    let mut matches = Vec::new();
    let mut pending = root.as_ref().first_child().into_iter().collect::<Vec<_>>();
    while let Some(widget) = pending.pop() {
        if widget.is_visible() {
            if let Ok(found) = widget.clone().downcast::<T>() {
                matches.push(found);
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                pending.push(current.clone());
                child = current.next_sibling();
            }
        }
    }
    matches
}

fn drain_main_context() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}
