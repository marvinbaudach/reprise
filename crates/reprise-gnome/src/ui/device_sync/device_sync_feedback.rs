//! Shared headerbar progress and lifecycle toasts for device synchronization.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use super::device_sync_runtime::{
    DeviceSyncRuntime, DeviceSyncState, DeviceView, PlannedSyncPhase,
};

#[derive(Clone)]
struct PreviousDevice {
    name: String,
    syncing: bool,
    progress: Option<(u32, u32)>,
    last_sync: Option<chrono::DateTime<chrono::Utc>>,
}

pub(in crate::ui) fn install(
    header: &adw::HeaderBar,
    split_view: &adw::NavigationSplitView,
    overlay: &adw::ToastOverlay,
    runtime: &Rc<DeviceSyncRuntime>,
) {
    let spinner = gtk4::Spinner::new();
    spinner.set_size_request(14, 14);
    spinner.set_visible(false);
    header.pack_end(&spinner);
    let active = Rc::new(Cell::new(false));
    let active_for_collapse = active.clone();
    let spinner_for_collapse = spinner.clone();
    split_view.connect_collapsed_notify(move |split_view| {
        spinner_for_collapse.set_visible(split_view.is_collapsed() && active_for_collapse.get());
    });

    let previous = Rc::new(RefCell::new(HashMap::<String, PreviousDevice>::new()));
    let split_view = split_view.clone();
    let overlay = overlay.clone();
    let subscription = runtime.subscribe(Rc::new(move |state| {
        update_header(&spinner, &split_view, &active, &state);
        show_transitions(&overlay, &previous, &state);
    }));
    subscription.retain_for_widget(header);
}

fn update_header(
    spinner: &gtk4::Spinner,
    split_view: &adw::NavigationSplitView,
    active: &Cell<bool>,
    state: &DeviceSyncState,
) {
    let syncing = state.devices.iter().find(|device| is_syncing(device));
    active.set(syncing.is_some());
    spinner.set_visible(split_view.is_collapsed() && syncing.is_some());
    if let Some(device) = syncing {
        spinner.start();
        spinner.set_tooltip_text(Some(&sync_tooltip(device)));
    } else {
        spinner.stop();
        spinner.set_tooltip_text(None);
    }
}

fn show_transitions(
    overlay: &adw::ToastOverlay,
    previous: &RefCell<HashMap<String, PreviousDevice>>,
    state: &DeviceSyncState,
) {
    let old = previous.borrow().clone();
    let current = state
        .devices
        .iter()
        .filter(|device| device.connected)
        .map(|device| {
            (
                device.id.clone(),
                previous_device(device, old.get(&device.id)),
            )
        })
        .collect::<HashMap<_, _>>();
    for device in state.devices.iter().filter(|device| device.connected) {
        match old.get(&device.id) {
            None => super::toasts::show(overlay, &format!("{} connected", device.name)),
            Some(before) if before.syncing && !is_syncing(device) => {
                let text = if device.last_sync != before.last_sync {
                    let copied = before.progress.map_or(0, |(_, total)| total);
                    format!("Sync complete · {copied} copied")
                } else if let Some(error) = &device.sync_error {
                    format!("Sync finished with errors · {}", error.message)
                } else {
                    "Sync cancelled".to_string()
                };
                super::toasts::show(overlay, &text);
            }
            _ => {}
        }
    }
    for (id, before) in &old {
        if current.contains_key(id) {
            continue;
        }
        let suffix = before.progress.map_or_else(String::new, |(done, total)| {
            format!(" — sync incomplete ({done} of {total})")
        });
        super::toasts::show(overlay, &format!("{} disconnected{suffix}", before.name));
    }
    previous.replace(current);
}

fn previous_device(device: &DeviceView, before: Option<&PreviousDevice>) -> PreviousDevice {
    let progress = match device.sync_phase {
        PlannedSyncPhase::Syncing {
            step: super::device_sync_runtime::SyncStep::Copying,
            done,
            total,
            ..
        } => Some((done, total)),
        _ => before.and_then(|before| before.progress),
    };
    PreviousDevice {
        name: device.name.clone(),
        syncing: is_syncing(device),
        progress,
        last_sync: device.last_sync,
    }
}

fn is_syncing(device: &DeviceView) -> bool {
    matches!(
        device.sync_phase,
        PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
    )
}

fn sync_tooltip(device: &DeviceView) -> String {
    format!(
        "Syncing {} · {}%",
        device.name,
        phase_percent(&device.sync_phase)
    )
}

fn phase_percent(phase: &PlannedSyncPhase) -> u64 {
    match phase {
        PlannedSyncPhase::Syncing {
            bytes_done,
            bytes_total,
            ..
        } if *bytes_total > 0 => bytes_done.saturating_mul(100) / bytes_total,
        PlannedSyncPhase::Finishing => 100,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finishing_is_reported_as_complete_progress() {
        assert_eq!(phase_percent(&PlannedSyncPhase::Finishing), 100);
    }
}
