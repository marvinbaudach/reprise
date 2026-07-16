//! Connected-device cards shown below the scrolling navigation rows.

use std::rc::Rc;

use gtk4::prelude::*;

use super::device_sync_runtime::{
    DeviceSyncRuntime, DeviceSyncState, DeviceView, PlannedSyncPhase,
};
use super::device_sync_strings;

type OpenCallback = Rc<dyn Fn(String, String)>;

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

    let subscription = runtime.subscribe(Rc::new({
        let section = section.clone();
        move |state| render(&section, &state, &on_open)
    }));
    subscription.retain_for_widget(&section);
}

fn render(section: &gtk4::Box, state: &DeviceSyncState, on_open: &OpenCallback) {
    while let Some(child) = section.first_child() {
        section.remove(&child);
    }
    let devices = state
        .devices
        .iter()
        .filter(|device| device.connected)
        .collect::<Vec<_>>();
    section.set_visible(!devices.is_empty());
    if devices.is_empty() {
        return;
    }
    let heading = gtk4::Label::new(Some("DEVICES"));
    heading.add_css_class("caption");
    heading.add_css_class("dim-label");
    heading.set_xalign(0.0);
    heading.set_margin_start(8);
    section.append(&heading);
    for device in devices {
        section.append(&device_card(device, on_open));
    }
}

fn device_card(device: &DeviceView, on_open: &OpenCallback) -> gtk4::Box {
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
    card.add_css_class("card");
    card.set_margin_bottom(3);
    card.set_margin_start(2);
    card.set_margin_end(2);
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
    let name = gtk4::Label::new(Some(&card_title(device)));
    name.add_css_class("heading");
    name.set_xalign(0.0);
    let detail = gtk4::Label::new(Some(&card_subtitle(device)));
    detail.add_css_class("caption");
    detail.add_css_class("dim-label");
    detail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    detail.set_xalign(0.0);
    labels.append(&name);
    labels.append(&detail);
    top.append(&labels);

    let syncing = matches!(
        device.sync_phase,
        PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
    );
    let action = gtk4::Button::with_label(if syncing { "Cancel" } else { "Sync" });
    action.add_css_class(if syncing { "flat" } else { "suggested-action" });
    action.set_valign(gtk4::Align::Center);
    action.set_sensitive(
        syncing
            || device
                .delta
                .as_ref()
                .is_some_and(|delta| !delta.to_copy.is_empty() || !delta.to_remove.is_empty()),
    );
    action.set_action_name(Some("app.sync-device"));
    action.set_action_target_value(Some(&device.id.to_variant()));
    top.append(&action);
    card.append(&top);

    if let PlannedSyncPhase::Syncing {
        bytes_done,
        bytes_total,
        ..
    } = device.sync_phase
    {
        let progress = gtk4::ProgressBar::new();
        progress.set_fraction(if bytes_total == 0 {
            0.0
        } else {
            bytes_done as f64 / bytes_total as f64
        });
        card.append(&progress);
    }

    let open = on_open.clone();
    let id = device.id.clone();
    let name = device.name.clone();
    let click = gtk4::GestureClick::new();
    click.set_button(1);
    click.connect_released(move |_, _, _, _| open(id.clone(), name.clone()));
    card.add_controller(click);
    card
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
