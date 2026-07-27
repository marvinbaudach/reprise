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
    split_view: &adw::OverlaySplitView,
    overlay: &adw::ToastOverlay,
    runtime: &Rc<DeviceSyncRuntime>,
) {
    let spinner = gtk4::Spinner::new();
    spinner.set_size_request(14, 14);
    spinner.set_visible(false);
    header.pack_end(&spinner);
    let active = Rc::new(Cell::new(false));
    // The header spinner mirrors "a sync runs while the sidebar card is not
    // visible". Card visibility tracks `show-sidebar` (an expanded overlay
    // shows the card too), so react to both notifications — a manual header
    // toggle in wide mode flips `show-sidebar` without touching `collapsed`,
    // and a sync-state event alone would otherwise leave the spinner stale.
    {
        let active = active.clone();
        let spinner = spinner.clone();
        split_view.connect_collapsed_notify(move |split_view| {
            sync_spinner_visibility(&spinner, split_view, active.get());
        });
    }
    {
        let active = active.clone();
        let spinner = spinner.clone();
        split_view.connect_show_sidebar_notify(move |split_view| {
            sync_spinner_visibility(&spinner, split_view, active.get());
        });
    }

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
    split_view: &adw::OverlaySplitView,
    active: &Cell<bool>,
    state: &DeviceSyncState,
) {
    let syncing = state.devices.iter().find(|device| is_syncing(device));
    active.set(syncing.is_some());
    sync_spinner_visibility(spinner, split_view, syncing.is_some());
    if let Some(device) = syncing {
        spinner.start();
        spinner.set_tooltip_text(Some(&sync_tooltip(device)));
    } else {
        spinner.stop();
        spinner.set_tooltip_text(None);
    }
}

/// The sidebar sync card is visible exactly while the sidebar itself is shown —
/// both as a permanent column and as an expanded overlay. The header spinner is
/// the fallback for when that card is hidden, so it must never double up with a
/// visible card: show it only while a sync runs and `show-sidebar` is off.
fn header_spinner_visible(syncing: bool, shows_sidebar: bool) -> bool {
    syncing && !shows_sidebar
}

fn sync_spinner_visibility(
    spinner: &gtk4::Spinner,
    split_view: &adw::OverlaySplitView,
    syncing: bool,
) {
    spinner.set_visible(header_spinner_visible(syncing, split_view.shows_sidebar()));
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
    super::device_sync_strings::syncing_spinner_tooltip(
        &device.name,
        phase_percent(&device.sync_phase),
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
    fn mtp_6_finishing_is_reported_as_complete_progress() {
        assert_eq!(phase_percent(&PlannedSyncPhase::Finishing), 100);
        assert!(include_str!("device_sync_feedback.rs").contains("Sync complete"));
        assert!(
            include_str!("../sidebar/sidebar_device_card.rs").contains("Synced ✓"),
            "the sidebar must project the completed idle state"
        );
    }

    #[test]
    fn header_spinner_only_fills_in_when_the_sidebar_card_is_hidden() {
        // No sync: never shown, regardless of the sidebar.
        assert!(!header_spinner_visible(false, false));
        assert!(!header_spinner_visible(false, true));
        // Sync + sidebar card visible (wide column or expanded overlay):
        // the card carries the status, so the header must not double up.
        assert!(!header_spinner_visible(true, true));
        // Sync + sidebar hidden (manually hidden while wide, or collapsed and
        // not expanded): the card is gone, so the header is the only surface.
        assert!(header_spinner_visible(true, false));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn header_spinner_shows_when_a_wide_sidebar_is_hidden_mid_sync() {
        gtk4::init().unwrap();
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let content = gtk4::Label::new(Some("Content"));
        // Wide mode: not collapsed. Hiding the sidebar keeps `collapsed` false,
        // so the old `is_collapsed()` gate would have left the spinner hidden.
        let split = adw::OverlaySplitView::builder()
            .sidebar(&sidebar)
            .content(&content)
            .sidebar_position(gtk4::PackType::Start)
            .collapsed(false)
            .show_sidebar(false)
            .build();
        let spinner = gtk4::Spinner::new();
        spinner.set_visible(false);

        assert!(!split.is_collapsed(), "the regressed gate hinges on this");
        sync_spinner_visibility(&spinner, &split, true);
        assert!(
            spinner.is_visible(),
            "a hidden wide sidebar must surface the header spinner"
        );

        // Re-showing the sidebar column hands the status back to its card.
        split.set_show_sidebar(true);
        sync_spinner_visibility(&spinner, &split, true);
        assert!(!spinner.is_visible());
    }
}
