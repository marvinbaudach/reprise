//! Connected-device projection and per-device podcast sync actions.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use gtk4::gio;
use gtk4::gio::prelude::ActionMapExt;
use gtk4::glib;
use gtk4::glib::variant::StaticVariantType;
use reprise_core::podcasts::{self, SourceGroup};
use rusqlite::Connection;

use super::podcasts_context_menu::{self, PodcastSyncDevice};
use super::podcasts_view::PodcastsView;

#[derive(Default)]
pub(super) struct PodcastDeviceSyncState {
    connected: RefCell<Vec<PodcastSyncDevice>>,
    selected: RefCell<BTreeMap<i64, Vec<String>>>,
    runtime: RefCell<Weak<crate::ui::device_sync_runtime::DeviceSyncRuntime>>,
}

impl PodcastDeviceSyncState {
    pub(super) fn connected(&self) -> Vec<PodcastSyncDevice> {
        self.connected.borrow().clone()
    }

    pub(super) fn update_connected(&self, devices: Vec<PodcastSyncDevice>) -> bool {
        if *self.connected.borrow() == devices {
            return false;
        }
        self.connected.replace(devices);
        true
    }

    pub(super) fn is_connected(&self, device_id: &str) -> bool {
        self.connected
            .borrow()
            .iter()
            .any(|device| device.id == device_id)
    }

    pub(super) fn selected(&self) -> BTreeMap<i64, Vec<String>> {
        self.selected.borrow().clone()
    }

    pub(super) fn replace_selected(&self, selected: BTreeMap<i64, Vec<String>>) {
        self.selected.replace(selected);
    }

    pub(super) fn selected_for_groups(
        conn: &Connection,
        groups: &[SourceGroup],
    ) -> Result<BTreeMap<i64, Vec<String>>, rusqlite::Error> {
        groups
            .iter()
            .map(|group| {
                podcasts::phone_sync::selected_device_ids(conn, group.subscription_id)
                    .map(|devices| (group.subscription_id, devices))
            })
            .collect()
    }
}

pub(super) fn bind(
    view: &Rc<PodcastsView>,
    runtime: &Rc<crate::ui::device_sync_runtime::DeviceSyncRuntime>,
) {
    view.device_sync.runtime.replace(Rc::downgrade(runtime));
    let weak = Rc::downgrade(view);
    let subscription = runtime.subscribe(Rc::new(move |state| {
        let Some(view) = weak.upgrade() else {
            return;
        };
        let devices = state
            .devices
            .into_iter()
            .filter(|device| device.connected)
            .map(|device| PodcastSyncDevice {
                id: device.id,
                name: device.name,
            })
            .collect::<Vec<_>>();
        if !view.device_sync.update_connected(devices) {
            return;
        }
        view.render();
    }));
    subscription.retain_for_widget(view.root());
}

pub(super) fn install_action(view: &Rc<PodcastsView>, group: &gio::SimpleActionGroup) {
    let sync = gio::SimpleAction::new(
        podcasts_context_menu::ACTION_TOGGLE_PHONE_SYNC,
        Some(&<(i64, String)>::static_variant_type()),
    );
    let weak = Rc::downgrade(view);
    sync.connect_activate(move |_, target| {
        let Some(view) = weak.upgrade() else {
            return;
        };
        let Some((subscription_id, device_id)) =
            target.and_then(glib::Variant::get::<(i64, String)>)
        else {
            return;
        };
        if !view.device_sync.is_connected(&device_id) {
            return;
        }
        let enabled = !view
            .device_sync
            .selected
            .borrow()
            .get(&subscription_id)
            .is_some_and(|devices| devices.contains(&device_id));
        let result = {
            let conn = view.conn.borrow();
            podcasts::phone_sync::set_device_enabled(&conn, subscription_id, &device_id, enabled)
        };
        if let Err(error) = result {
            view.show_error(&error.to_string());
            return;
        }
        view.refresh();
        let runtime = view.device_sync.runtime.borrow().upgrade();
        if let Some(runtime) = runtime {
            if let Err(error) = runtime.recompute_delta(&device_id) {
                tracing::warn!(%error, %device_id, "could not refresh device podcast plan");
            }
        }
    });
    group.add_action(&sync);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pod_8_connected_device_choices_replace_stale_disconnected_devices() {
        let state = PodcastDeviceSyncState::default();
        let phone = PodcastSyncDevice {
            id: "mtp:phone".into(),
            name: "Phone".into(),
        };

        assert!(state.update_connected(vec![phone.clone()]));
        assert_eq!(state.connected(), [phone]);
        assert!(state.is_connected("mtp:phone"));
        assert!(!state.is_connected("mtp:tablet"));
        assert!(state.update_connected(Vec::new()));
        assert!(state.connected().is_empty());
        assert!(!state.update_connected(Vec::new()));
    }
}
