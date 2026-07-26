//! Navigation policy for the Android synchronization preferences.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::device_sync_runtime::DeviceSyncRuntime;
use super::device_sync_runtime::DeviceView;

fn single_device_id(devices: &[DeviceView]) -> Option<&str> {
    let mut connected = devices.iter().filter(|device| device.connected);
    let device = connected.next()?;
    connected.next().is_none().then_some(device.id.as_str())
}

pub(super) fn install_single_device_shortcut(
    page: &libadwaita::PreferencesPage,
    runtime: &Rc<DeviceSyncRuntime>,
    selected_ids: &Rc<Vec<i64>>,
) {
    let runtime = runtime.clone();
    let selected_ids = selected_ids.clone();
    let shortcut_used = Rc::new(Cell::new(false));
    let shortcut_scheduled = Rc::new(Cell::new(false));
    page.connect_map(move |page| {
        if shortcut_used.get() || shortcut_scheduled.get() {
            return;
        }
        let Some(device_id) = single_device_id(&runtime.devices()).map(str::to_owned) else {
            return;
        };
        shortcut_scheduled.set(true);
        let page = page.downgrade();
        let runtime = runtime.clone();
        let selected_ids = selected_ids.clone();
        let shortcut_used = shortcut_used.clone();
        let shortcut_scheduled = shortcut_scheduled.clone();
        gtk4::glib::idle_add_local_once(move || {
            shortcut_scheduled.set(false);
            let Some(page) = page.upgrade().filter(WidgetExt::is_mapped) else {
                return;
            };
            if shortcut_used.replace(true) {
                return;
            }
            super::preference_sync::present_device(&page, &device_id, &runtime, &selected_ids);
        });
    });
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use libadwaita as adw;
    use libadwaita::prelude::*;

    fn device(id: &str) -> DeviceView {
        DeviceView {
            id: id.into(),
            name: id.into(),
            icon: gtk4::gio::ThemedIcon::new("phone-symbolic").upcast(),
            connected: true,
            storage: Default::default(),
            scanning: false,
            scan_error: None,
            draft_playlists: Vec::new(),
            last_enqueue: None,
            snapshot: reprise_core::device_sync::DeviceQueue::new().snapshot(),
            settings: reprise_core::device_sync::DeviceSettings {
                device_serial: id.into(),
                device_name: id.into(),
                selection: reprise_core::device_sync::DeviceSelection::default(),
                profile: reprise_core::device_sync::TransferProfile::default(),
                opus_bitrate: 0,
                ratings_back: false,
                remove_deleted: true,
            },
            delta: None,
            sync_phase: crate::ui::device_sync_runtime::PlannedSyncPhase::Idle,
            sync_error: None,
            last_sync: None,
            tracks: Vec::new(),
            selected_track_count: 0,
            bytes_per_second: 0,
        }
    }

    #[test]
    fn exactly_one_device_skips_the_device_chooser() {
        let phone = device("phone");

        assert_eq!(single_device_id(&[phone]), Some("phone"));
    }

    #[test]
    fn zero_or_multiple_devices_keep_the_device_chooser() {
        let phone = device("phone");
        let tablet = device("tablet");

        assert_eq!(single_device_id(&[]), None);
        assert_eq!(single_device_id(&[phone, tablet]), None);
    }

    #[test]
    fn shortcut_counts_only_connected_devices() {
        let phone = device("phone");
        let mut old_phone = device("old-phone");
        old_phone.connected = false;

        assert_eq!(single_device_id(&[old_phone.clone()]), None);
        assert_eq!(single_device_id(&[phone, old_phone]), Some("phone"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn device_row_pushes_a_navigation_subpage_instead_of_a_dialog() {
        gtk4::init().unwrap();
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let runtime = DeviceSyncRuntime::new(
            &Rc::new(RefCell::new(conn)),
            reprise_platform_linux::device_sync::DeviceMonitor::new(),
        );
        let navigation = adw::NavigationView::new();
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        navigation.add(&adw::NavigationPage::new(&content, "Preferences"));

        super::super::preference_sync::present_device(
            &content,
            "unknown-device",
            &runtime,
            &Rc::new(Vec::new()),
        );

        let visible = navigation.visible_page().expect("a page is visible");
        assert_eq!(
            visible.title(),
            super::super::device_sync_strings::text(
                super::super::device_sync_strings::DISCONNECTED,
            )
        );
    }
}
