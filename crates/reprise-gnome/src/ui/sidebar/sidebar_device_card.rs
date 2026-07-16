//! Connected-device cards shown below the scrolling navigation rows.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;

use super::device_sync_runtime::{
    DeviceSyncRuntime, DeviceSyncState, DeviceView, PlannedSyncPhase,
};
use super::device_sync_strings;

type OpenCallback = Rc<dyn Fn(String, String)>;

/// Live card widgets, keyed by device id, so a state update can refresh them
/// in place. Rebuilding the section on every update destroyed the card
/// between a click's press and release — during a sync `notify` fires on
/// every progress callback, which made the card permanently unclickable —
/// and re-cloned every widget many times a second for nothing.
type CardRegistry = Rc<RefCell<HashMap<String, DeviceCard>>>;

struct DeviceCard {
    root: gtk4::Box,
    icon: gtk4::Image,
    name: gtk4::Label,
    detail: gtk4::Label,
    action: gtk4::Button,
    progress: gtk4::ProgressBar,
    /// Read by the click gesture, which outlives any single state update.
    open_name: Rc<RefCell<String>>,
}

pub(super) fn bind(
    sidebar_root: &gtk4::Box,
    runtime: &Rc<DeviceSyncRuntime>,
    on_open: OpenCallback,
) {
    let section = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    section.set_margin_start(10);
    section.set_margin_end(10);
    section.set_margin_top(8);
    section.set_margin_bottom(8);
    section.set_visible(false);
    let first = sidebar_root.first_child();
    sidebar_root.insert_child_after(&section, first.as_ref());

    let heading = gtk4::Label::new(Some("DEVICES"));
    heading.add_css_class("caption");
    heading.add_css_class("dim-label");
    heading.set_xalign(0.0);
    heading.set_margin_start(8);
    section.append(&heading);

    let cards: CardRegistry = Rc::new(RefCell::new(HashMap::new()));
    let subscription = runtime.subscribe(Rc::new({
        let section = section.clone();
        let cards = cards.clone();
        move |state| render(&section, &cards, &state, &on_open)
    }));
    subscription.retain_for_widget(&section);
}

fn render(
    section: &gtk4::Box,
    cards: &CardRegistry,
    state: &DeviceSyncState,
    on_open: &OpenCallback,
) {
    let devices = state
        .devices
        .iter()
        .filter(|device| device.connected)
        .collect::<Vec<_>>();
    section.set_visible(!devices.is_empty());

    let mut registry = cards.borrow_mut();
    // Drop cards for devices that went away.
    registry.retain(|id, card| {
        let keep = devices.iter().any(|device| &device.id == id);
        if !keep {
            section.remove(&card.root);
        }
        keep
    });
    // Update in place, appending only genuinely new devices.
    for device in devices {
        match registry.get(&device.id) {
            Some(card) => card.update(device),
            None => {
                let card = DeviceCard::new(device, on_open);
                section.append(&card.root);
                card.update(device);
                registry.insert(device.id.clone(), card);
            }
        }
    }
}

impl DeviceCard {
    fn new(device: &DeviceView, on_open: &OpenCallback) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        root.add_css_class("card");
        root.set_margin_bottom(3);
        root.set_margin_start(2);
        root.set_margin_end(2);
        let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        top.set_margin_top(8);
        top.set_margin_bottom(8);
        top.set_margin_start(10);
        top.set_margin_end(8);
        let icon = gtk4::Image::from_gicon(&device.icon);
        icon.set_pixel_size(24);
        top.append(&icon);
        let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
        labels.set_hexpand(true);
        let name = gtk4::Label::new(None);
        name.add_css_class("heading");
        name.set_xalign(0.0);
        let detail = gtk4::Label::new(None);
        detail.add_css_class("caption");
        detail.add_css_class("dim-label");
        detail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        detail.set_xalign(0.0);
        labels.append(&name);
        labels.append(&detail);
        top.append(&labels);
        let action = gtk4::Button::new();
        action.set_valign(gtk4::Align::Center);
        action.set_action_name(Some("app.sync-device"));
        action.set_action_target_value(Some(&device.id.to_variant()));
        top.append(&action);
        root.append(&top);
        let progress = gtk4::ProgressBar::new();
        progress.set_visible(false);
        root.append(&progress);

        // The gesture lives as long as the card, so opening the device view
        // works mid-sync; the name is read fresh on click because it can
        // change (GVfs settles a generic "mtp" into the real model name).
        let open_name = Rc::new(RefCell::new(device.name.clone()));
        let open = on_open.clone();
        let id = device.id.clone();
        let click_name = open_name.clone();
        let click = gtk4::GestureClick::new();
        click.set_button(1);
        click.connect_released(move |_, _, _, _| {
            let name = click_name.borrow().clone();
            open(id.clone(), name);
        });
        root.add_controller(click);

        Self {
            root,
            icon,
            name,
            detail,
            action,
            progress,
            open_name,
        }
    }

    fn update(&self, device: &DeviceView) {
        self.icon.set_from_gicon(&device.icon);
        self.name.set_text(&card_title(device));
        self.detail.set_text(&card_subtitle(device));
        *self.open_name.borrow_mut() = device.name.clone();

        let syncing = matches!(
            device.sync_phase,
            PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
        );
        self.action
            .set_label(if syncing { "Cancel" } else { "Sync" });
        if syncing {
            self.action.remove_css_class("suggested-action");
            self.action.add_css_class("flat");
        } else {
            self.action.remove_css_class("flat");
            self.action.add_css_class("suggested-action");
        }
        self.action.set_sensitive(
            syncing
                || device
                    .delta
                    .as_ref()
                    .is_some_and(|delta| !delta.to_copy.is_empty() || !delta.to_remove.is_empty()),
        );

        if let PlannedSyncPhase::Syncing {
            bytes_done,
            bytes_total,
            ..
        } = device.sync_phase
        {
            self.progress.set_visible(true);
            self.progress.set_fraction(if bytes_total == 0 {
                0.0
            } else {
                (bytes_done as f64 / bytes_total as f64).clamp(0.0, 1.0)
            });
        } else {
            self.progress.set_visible(false);
        }
    }
}

fn card_title(device: &DeviceView) -> String {
    if matches!(
        device.sync_phase,
        PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
    ) {
        format!("Syncing {}", device.name)
    } else {
        device.name.clone()
    }
}

fn card_subtitle(device: &DeviceView) -> String {
    match &device.sync_phase {
        PlannedSyncPhase::ComputingDelta => "Checking…".into(),
        PlannedSyncPhase::Syncing {
            current_track,
            bytes_done,
            bytes_total,
            ..
        } => {
            let percent = if *bytes_total == 0 {
                0
            } else {
                bytes_done.saturating_mul(100) / bytes_total
            };
            format!("{percent}% · {current_track}")
        }
        PlannedSyncPhase::Finishing => "Finishing…".into(),
        PlannedSyncPhase::Idle => {
            let queued = device.delta.as_ref().map_or(0, |delta| delta.to_copy.len());
            format!(
                "{queued} queued · {}",
                device_sync_strings::available_space(device.available_bytes)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syncing_title_is_explicit() {
        assert_eq!(
            card_title(&DeviceView {
                id: "pixel".into(),
                name: "Pixel 8".into(),
                icon: gtk4::gio::ThemedIcon::new("phone-symbolic").upcast(),
                connected: true,
                available_bytes: None,
                contents: Default::default(),
                scanning: false,
                scan_error: None,
                draft_playlists: Vec::new(),
                last_enqueue: None,
                snapshot: reprise_core::device_sync::DeviceQueue::new().snapshot(),
                settings: reprise_core::device_sync::DeviceSettings {
                    device_serial: "pixel".into(),
                    device_name: "Pixel 8".into(),
                    selection: Default::default(),
                    opus_bitrate: 0,
                    ratings_back: false,
                    remove_deleted: true,
                },
                delta: None,
                sync_phase: PlannedSyncPhase::Finishing,
                sync_error: None,
                last_sync: None,
                tracks: Vec::new(),
                selected_track_count: 0,
            }),
            "Syncing Pixel 8"
        );
    }
}
