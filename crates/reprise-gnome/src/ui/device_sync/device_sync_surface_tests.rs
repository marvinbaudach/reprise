use std::path::Path;

fn ui_source(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/ui")
        .join(relative);
    std::fs::read_to_string(path).expect("UI source should be readable")
}

fn core_source(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../reprise-core/src")
        .join(relative);
    std::fs::read_to_string(path).expect("core source should be readable")
}

#[test]
fn retired_device_browser_surfaces_stay_removed() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in [
        "src/ui/device_view/mod.rs",
        "src/ui/device_view/device_view.rs",
        "src/ui/preferences/preference_sync.rs",
        "src/ui/preferences/preference_sync_navigation.rs",
        "src/ui/preferences/preference_sync_planned.rs",
        "src/ui/device_sync/device_sync_actions.rs",
    ] {
        assert!(
            !manifest.join(relative).exists(),
            "retired sync surface still exists: {relative}"
        );
    }

    assert!(!ui_source("mod.rs").contains("mod device_view;"));
    assert!(!ui_source("preferences/mod.rs").contains("preference_sync"));
    assert!(!ui_source("preferences/preferences_window.rs").contains("Synchronization"));
    assert!(!ui_source("window/window.rs").contains("device_view"));
    assert!(!ui_source("device_sync/device_sync_runtime.rs").contains("pub fn enqueue("));
    assert!(!core_source("view_source.rs").contains("Device { serial"));
    assert!(!core_source("browser.rs").contains("Device { serial"));
}

#[test]
fn mtp_1_connected_devices_appear_without_automatic_navigation() {
    let feedback = ui_source("device_sync/device_sync_feedback.rs");
    let cards = ui_source("sidebar/sidebar_device_card.rs");

    assert!(feedback.contains("format!(\"{} connected\", device.name)"));
    assert!(cards.contains(".filter(|device| device.connected)"));
    assert!(!feedback.contains("content_stack"));
}

#[test]
fn mtp_2_primary_menu_routes_to_the_compact_device_launcher() {
    let wiring = ui_source("window/window_runtime_wiring.rs");
    assert!(wiring.contains("device_sync_launcher::present"));
    assert!(!wiring.contains("present_page(\"synchronization\")"));
}
