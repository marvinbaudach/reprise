//! Display coverage for external library changes reaching a mapped device page.

use super::*;

fn widget_has_label(widget: &gtk4::Widget, expected: &str) -> bool {
    if widget
        .clone()
        .downcast::<gtk4::Label>()
        .is_ok_and(|label| label.text() == expected)
    {
        return true;
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if widget_has_label(&current, expected) {
            return true;
        }
        child = current.next_sibling();
    }
    false
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn external_changes_refreshes_the_switched_mapped_device_page_playlist_card() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let device_roots = tempfile::tempdir().unwrap();
    let first_root = device_roots.path().join("first");
    let second_root = device_roots.path().join("second");
    std::fs::create_dir_all(&first_root).unwrap();
    std::fs::create_dir_all(&second_root).unwrap();
    let conn = Rc::new(crate::test_db::open_fresh().unwrap());
    let backend = Rc::new(
        crate::ui::device_sync::device_sync_smoke::SimulatedMtpDeviceBackend::for_devices(vec![
            ("first".into(), "First phone".into(), first_root),
            ("second".into(), "Second phone".into(), second_root),
        ])
        .unwrap(),
    );
    let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
    let content_stack = gtk4::Stack::new();
    let window_title = adw::WindowTitle::new("Music", "");

    assert!(open(&content_stack, &window_title, "first", &runtime));
    let window = gtk4::Window::new();
    window.set_child(Some(&content_stack));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_eq!(mapped_device_id().as_deref(), Some("first"));

    let second = reprise_core::db::Db::open_migrated(conn.path().as_deref()).unwrap();
    reprise_core::library::playlists::create(&second, "Written before switching").unwrap();
    crate::ui::window::window_runtime_wiring::external_changes_wiring::refresh_device_sync(
        &runtime,
        &crate::ui::external_changes::RefreshPlan {
            sidebar: true,
            track_list: true,
            conversion: false,
        },
    );

    assert!(open(&content_stack, &window_title, "second", &runtime));
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_eq!(mapped_device_id().as_deref(), Some("second"));
    let page = content_stack.child_by_name("device-sync").unwrap();
    assert!(widget_has_label(&page, "Written before switching"));

    reprise_core::library::playlists::create(&second, "Written while second is mapped").unwrap();
    crate::ui::window::window_runtime_wiring::external_changes_wiring::refresh_device_sync(
        &runtime,
        &crate::ui::external_changes::RefreshPlan {
            sidebar: true,
            track_list: true,
            conversion: false,
        },
    );
    assert!(widget_has_label(&page, "Written while second is mapped"));
    window.close();
}
