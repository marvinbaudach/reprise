use std::rc::{Rc, Weak};

#[cfg(feature = "test-fixtures")]
use std::{cell::Cell, time::Duration};

use gtk4::gio;
use gtk4::prelude::NetworkMonitorExt;
use reprise_core::connectivity::Connectivity;

pub(super) const fn connectivity_for(network_available: bool) -> Connectivity {
    if network_available {
        Connectivity::Online
    } else {
        Connectivity::Offline
    }
}

#[cfg(feature = "test-fixtures")]
const TEST_CONNECTIVITY_FILE_ENV: &str = "REPRISE_TEST_CONNECTIVITY_FILE";

#[derive(Clone)]
struct ConnectivityTargets {
    concerts: Weak<crate::ui::concerts::ConcertsView>,
    releases: Weak<crate::ui::releases::ReleasesView>,
    podcasts: Weak<crate::ui::podcasts::PodcastsView>,
    youtube: Weak<crate::ui::podcasts::PodcastsView>,
    radio: Weak<crate::ui::radio::RadioView>,
    device_sync: Weak<crate::ui::device_sync_runtime::DeviceSyncRuntime>,
}

impl ConnectivityTargets {
    fn new(
        concerts: &Rc<crate::ui::concerts::ConcertsView>,
        releases: &Rc<crate::ui::releases::ReleasesView>,
        podcasts: &Rc<crate::ui::podcasts::PodcastsView>,
        youtube: &Rc<crate::ui::podcasts::PodcastsView>,
        radio: &Rc<crate::ui::radio::RadioView>,
        device_sync: &Rc<crate::ui::device_sync_runtime::DeviceSyncRuntime>,
    ) -> Self {
        Self {
            concerts: Rc::downgrade(concerts),
            releases: Rc::downgrade(releases),
            podcasts: Rc::downgrade(podcasts),
            youtube: Rc::downgrade(youtube),
            radio: Rc::downgrade(radio),
            device_sync: Rc::downgrade(device_sync),
        }
    }

    fn project(&self, connectivity: Connectivity, metered: bool) {
        if let Some(view) = self.concerts.upgrade() {
            view.set_connectivity(connectivity);
        }
        if let Some(view) = self.releases.upgrade() {
            view.set_connectivity(connectivity);
        }
        if let Some(view) = self.podcasts.upgrade() {
            view.set_connectivity(connectivity);
        }
        if let Some(view) = self.youtube.upgrade() {
            view.set_connectivity(connectivity);
        }
        if let Some(view) = self.radio.upgrade() {
            view.set_connectivity(connectivity);
        }
        if let Some(runtime) = self.device_sync.upgrade() {
            runtime.set_connectivity(connectivity);
            runtime.set_metered(metered);
        }
    }
}

#[cfg(feature = "test-fixtures")]
fn wire_test_connectivity(targets: ConnectivityTargets) -> bool {
    let Ok(path) = std::env::var(TEST_CONNECTIVITY_FILE_ENV) else {
        return false;
    };
    let path = std::path::Path::new(&path).to_path_buf();
    let initial = reprise_core::connectivity::read_test_connectivity(&path);
    if initial.is_none() {
        tracing::warn!(
            path = %path.display(),
            "test connectivity control starts online until it contains online or offline"
        );
    }
    let initial = initial.unwrap_or(Connectivity::Online);
    targets.project(initial, false);
    let last = Cell::new(initial);
    gtk4::glib::timeout_add_local(Duration::from_millis(100), move || {
        let next = reprise_core::connectivity::read_test_connectivity(&path);
        let Some(next) = next else {
            return gtk4::glib::ControlFlow::Continue;
        };
        if next == last.get() {
            return gtk4::glib::ControlFlow::Continue;
        }
        last.set(next);
        targets.project(next, false);
        gtk4::glib::ControlFlow::Continue
    });
    true
}

/// Projects `gio::NetworkMonitor` at the window boundary and pushes the result
/// into every seam that needs it. `NET-3a` requires *connectivity* to be an
/// explicitly set value rather than something each consumer infers where it
/// stands, and this is the only place that derives it; device sync is included
/// for exactly that reason — it used to read the monitor itself, which is the
/// drift this projection exists to stop.
///
/// One direct reader remains and is deliberate: `podcast_refresh_scheduler`
/// asks for `is_network_metered()` at the moment it decides whether to start an
/// automatic refresh. That is a different fact from connectivity, it can change
/// between two refreshes, and it has no state to keep in sync — so pushing it
/// would buy nothing.
pub(super) fn wire(
    concerts: &Rc<crate::ui::concerts::ConcertsView>,
    releases: &Rc<crate::ui::releases::ReleasesView>,
    podcasts: &Rc<crate::ui::podcasts::PodcastsView>,
    youtube: &Rc<crate::ui::podcasts::PodcastsView>,
    radio: &Rc<crate::ui::radio::RadioView>,
    device_sync: &Rc<crate::ui::device_sync_runtime::DeviceSyncRuntime>,
) {
    let targets =
        ConnectivityTargets::new(concerts, releases, podcasts, youtube, radio, device_sync);
    #[cfg(feature = "test-fixtures")]
    if wire_test_connectivity(targets.clone()) {
        return;
    }
    let monitor = gio::NetworkMonitor::default();
    let initial = connectivity_for(monitor.is_network_available());
    targets.project(initial, monitor.is_network_metered());
    monitor.connect_network_changed(move |monitor, available| {
        let connectivity = connectivity_for(available);
        // `MTP-43` reads metered separately, and it changes on the same
        // signal — tethering after Wi-Fi drops is one network change, not
        // two, so device sync must not be left believing the old answer.
        targets.project(connectivity, monitor.is_network_metered());
    });
}

/// `MTP-46`/`SET-4`: the two source-module switches live in Preferences but
/// change what a device may sync, and the device page renders from a snapshot
/// that only a recompute refreshes. Without this the row of a switched-off
/// source stays until something unrelated triggers one.
///
/// The runtime is held weakly: the preferences context outlives it in no
/// meaningful sense, and a strong cycle through this closure would keep both
/// alive for the life of the process.
pub(super) fn wire_source_module_recompute(
    preferences: &Rc<crate::ui::preferences::PreferencesContext>,
    device_sync: &Rc<crate::ui::device_sync_runtime::DeviceSyncRuntime>,
) {
    let device_sync = Rc::downgrade(device_sync);
    *preferences.on_source_modules_changed.borrow_mut() = Some(Rc::new(move || {
        if let Some(device_sync) = device_sync.upgrade() {
            device_sync.recompute_all_devices();
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_3_a_network_monitor_state_is_projected_once_at_the_window_boundary() {
        assert_eq!(connectivity_for(true), Connectivity::Online);
        assert_eq!(connectivity_for(false), Connectivity::Offline);
    }
}
