//! Full-page per-device surface for Android playlist mirroring.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::device_sync::TransferProfile;

#[cfg(test)]
use super::device_sync_on_device::storage_legend;
use super::device_sync_on_device::{OnDeviceActions, OnDeviceSection};
use super::device_sync_page_actions::PageActions;
use super::device_sync_page_copy::{
    change_summary, device_last_sync_copy, eject_sensitive, offline_change_preview, profile_label,
};
// Only named directly by this module's own `#[cfg(test)]` child (below) —
// a plain `cargo build` never compiles that module, so these would
// otherwise warn as unused.
#[cfg(test)]
use super::device_sync_page_copy::{
    blocker_summary, format_local_date_time, playlist_subtitle, verification_summary,
    warning_summary,
};
use super::device_sync_page_layout;
use super::device_sync_page_layout::DeviceDashboard;
use super::device_sync_playlist_card::{PlaylistCard, PlaylistCardActions};
use super::device_sync_remembered;
use super::device_sync_runtime::{DeviceSyncRuntime, DeviceSyncState, DeviceView};
#[cfg(test)]
use super::device_sync_storage_copy::storage_access_notice;
use super::device_sync_storage_copy::storage_summary;
use super::device_sync_strings;

struct DeviceSyncPage {
    root: gtk4::glib::WeakRef<gtk4::Stack>,
    dashboard: DeviceDashboard,
    playlist_card: Rc<PlaylistCard>,
    on_device: OnDeviceSection,
    updating: Rc<Cell<bool>>,
}

impl DeviceSyncPage {
    fn new(
        device: &DeviceView,
        actions: PageActions,
        on_device_actions: &OnDeviceActions,
    ) -> (Rc<Self>, gtk4::Stack) {
        let PageActions {
            set_profile,
            set_playlist,
            start,
            cancel,
            eject,
        } = actions;
        let labels = device_sync_page_layout::profile_labels(profile_label);
        let playlist_card = Rc::new(PlaylistCard::new(PlaylistCardActions {
            set_playlist,
            open_picker: on_device_actions.open_playlist_picker.clone(),
        }));
        let dashboard = device_sync_page_layout::build(device, &labels, &playlist_card);
        debug_assert!(dashboard.scroller.vexpands());
        dashboard
            .eject
            .set_tooltip_text(Some(&device_sync_strings::eject_tooltip(false)));
        let review_playlists = {
            let scroller = dashboard.scroller.clone();
            let playlist_card = playlist_card.clone();
            Rc::new(move || {
                if let Some(bounds) = playlist_card.root().compute_bounds(&scroller) {
                    let adjustment = scroller.vadjustment();
                    let maximum =
                        (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
                    adjustment.set_value(
                        (adjustment.value() + f64::from(bounds.y()))
                            .clamp(adjustment.lower(), maximum),
                    );
                }
                playlist_card.focus_first();
            }) as Rc<dyn Fn()>
        };
        let on_device = OnDeviceSection::new(on_device_actions, review_playlists);
        dashboard.on_device.append(on_device.root());
        debug_assert_eq!(
            dashboard.content.last_child(),
            Some(dashboard.on_device.clone().upcast()),
            "the dashboard owns the complete section order"
        );

        let disconnected = adw::StatusPage::builder()
            .icon_name("phone-symbolic")
            .title("Device disconnected")
            .description("Reconnect the device to continue synchronization.")
            .build();
        let root = gtk4::Stack::new();
        root.add_named(&dashboard.root, Some("connected"));
        root.add_named(&disconnected, Some("disconnected"));
        root.set_visible_child_name("connected");
        let root_ref = gtk4::glib::WeakRef::new();
        root_ref.set(Some(&root));

        let updating = Rc::new(Cell::new(false));
        {
            let updating = updating.clone();
            dashboard.profile.connect_selected_notify(move |row| {
                if updating.get() {
                    return;
                }
                let Some(profile) = TransferProfile::ALL.get(row.selected() as usize).copied()
                else {
                    return;
                };
                set_profile(profile);
            });
        }
        dashboard.dock.connect_actions(start, cancel);
        {
            dashboard.eject.connect_clicked(move |_| eject());
        }

        let surface = Rc::new(Self {
            root: root_ref,
            dashboard,
            playlist_card,
            on_device,
            updating,
        });
        surface.update(device);
        // The widget tree owns its controller, while the controller keeps only
        // a weak root reference. Dropping the removed root therefore releases
        // both the controller and its runtime callback without a cycle.
        let keepalive = surface.clone();
        root.connect_unrealize(move |_| {
            let _ = &keepalive;
        });
        (surface, root)
    }

    fn update(&self, device: &DeviceView) {
        self.updating.set(true);
        self.dashboard.device_name.set_label(&device.name);
        self.dashboard
            .device_name
            .set_sensitive(device.rememberable);
        // The name itself leads, because it is what an ellipsized title hides;
        // the action or the reason it is unavailable follows on its own line.
        if device.rememberable {
            let action = device_sync_strings::text(device_sync_strings::RENAME_DEVICE);
            self.dashboard
                .device_name
                .set_tooltip_text(Some(&format!("{}\n{action}", device.name)));
            self.dashboard
                .device_name
                .update_property(&[gtk4::accessible::Property::Label(&action)]);
        } else {
            let reason = device_sync_strings::rename_requires_durable_identity();
            self.dashboard
                .device_name
                .set_tooltip_text(Some(&format!("{}\n{reason}", device.name)));
            // A disabled button that still announces "Rename device" tells a
            // screen-reader user the state but not the reason.
            self.dashboard
                .device_name
                .update_property(&[gtk4::accessible::Property::Label(&reason)]);
        }
        self.dashboard.connection.remove_css_class("success");
        self.dashboard.connection.remove_css_class("warning");
        self.dashboard.connection.remove_css_class("dim-label");
        match &device.session_state {
            reprise_core::device_sync::DeviceSessionState::Active => {
                if let Some(status) = &device.memory_status {
                    self.dashboard.connection.set_label(status);
                    self.dashboard.connection.add_css_class("warning");
                } else {
                    self.dashboard.connection.set_label("MTP connected");
                    self.dashboard.connection.add_css_class("success");
                }
            }
            reprise_core::device_sync::DeviceSessionState::Inert { active_device_name } => {
                self.dashboard
                    .connection
                    .set_label(&device_sync_strings::inert_device_status(
                        active_device_name,
                    ));
                self.dashboard.connection.add_css_class("warning");
            }
            reprise_core::device_sync::DeviceSessionState::Remembered => {
                self.dashboard.connection.set_label(
                    &reprise_core::device_sync::remembered_device_status(
                        device.last_sync,
                        chrono::Utc::now(),
                    ),
                );
                self.dashboard.connection.add_css_class("dim-label");
            }
        }
        self.dashboard
            .device_last_sync
            .set_label(&device_last_sync_copy(device));
        if let Some(root) = self.root.upgrade() {
            root.set_visible_child_name(
                if device_sync_remembered::apply(&self.dashboard, device) {
                    "connected"
                } else {
                    "disconnected"
                },
            );
        }
        let selected = TransferProfile::ALL
            .iter()
            .position(|profile| profile == &device.page.profile)
            .unwrap_or(0);
        self.dashboard.profile.set_selected(selected as u32);
        self.dashboard
            .profile
            .set_sensitive(device.page.controls.editable);
        self.playlist_card.update(device);
        let changes = if device.session_state.shows_diff() {
            change_summary(&device.page.changes)
        } else {
            offline_change_preview(
                device.page.changes.additions,
                device.page.changes.replacements,
                device.page.changes.playlist_writes,
                device.page.changes.transfer_bytes,
            )
        };
        self.dashboard.changes.set_label(&changes);
        self.dashboard.storage_name.set_label(
            device
                .page
                .storage
                .target_name
                .as_deref()
                .unwrap_or("Device storage"),
        );
        self.dashboard.storage_summary.set_label(&storage_summary(
            &device.page.storage,
            device.storage_measured,
        ));
        self.dashboard.storage_bar.update(&device.page.storage);
        self.update_actions(device);
        self.on_device.update(device);
        self.updating.set(false);
    }

    fn update_actions(&self, device: &DeviceView) {
        self.dashboard.dock.update(device);
        self.dashboard.eject.set_sensitive(eject_sensitive(device));
        self.dashboard
            .eject
            .set_tooltip_text(Some(&device_sync_strings::eject_tooltip(
                !self.dashboard.eject.is_sensitive(),
            )));
    }

    #[cfg(test)]
    fn root_text(&self) -> String {
        fn append(widget: &gtk4::Widget, output: &mut String) {
            if let Ok(label) = widget.clone().downcast::<gtk4::Label>() {
                output.push_str(&label.text());
                output.push('\n');
            }
            if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
                if let Some(label) = button.label() {
                    output.push_str(&label);
                    output.push('\n');
                }
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                append(&current, output);
                child = current.next_sibling();
            }
        }
        let mut output = String::new();
        if let Some(root) = self.root.upgrade() {
            append(root.upcast_ref(), &mut output);
        }
        output
    }
}

fn page_state_callback(
    surface: Weak<DeviceSyncPage>,
    device_id: String,
) -> Rc<dyn Fn(DeviceSyncState)> {
    Rc::new(move |state| {
        let Some(surface) = surface.upgrade() else {
            return;
        };
        if let Some(device) = state.devices.iter().find(|device| device.id == device_id) {
            surface.update(device);
        } else if let Some(root) = surface.root.upgrade() {
            root.set_visible_child_name("disconnected");
        }
    })
}

pub(in crate::ui) fn open(
    content_stack: &gtk4::Stack,
    window_title: &adw::WindowTitle,
    device_id: &str,
    runtime: &Rc<DeviceSyncRuntime>,
) -> bool {
    let device = runtime
        .devices()
        .into_iter()
        .find(|device| device.id == device_id);
    let Some(device) = device else {
        return false;
    };
    let (surface, root) = DeviceSyncPage::new(
        &device,
        PageActions::for_runtime(runtime, device_id),
        &OnDeviceActions::for_runtime(runtime, device_id),
    );
    {
        let runtime = runtime.clone();
        let device_id = device_id.to_string();
        surface
            .dashboard
            .device_name
            .connect_clicked(move |button| {
                super::device_sync_rename::prompt(button, &runtime, &device_id);
            });
    }
    if let Some(previous) = content_stack.child_by_name("device-sync") {
        content_stack.remove(&previous);
    }
    content_stack.add_named(&root, Some("device-sync"));
    window_title.set_title(&device.name);

    let subscription = runtime.subscribe(page_state_callback(
        Rc::downgrade(&surface),
        device_id.to_string(),
    ));
    subscription.retain_for_widget(&root);
    let focus = surface
        .playlist_card
        .focus_widget()
        .or_else(|| {
            surface.dashboard.dock.primary.is_sensitive().then(|| {
                surface
                    .dashboard
                    .dock
                    .primary
                    .clone()
                    .upcast::<gtk4::Widget>()
            })
        })
        .unwrap_or_else(|| surface.dashboard.eject.clone().upcast::<gtk4::Widget>());
    crate::ui::window::content_stack::show_page(content_stack, "device-sync");
    gtk4::glib::idle_add_local_once(move || {
        focus.grab_focus();
    });
    true
}

#[cfg(test)]
#[path = "device_sync_page_tests.rs"]
mod tests;
