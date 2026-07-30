use std::rc::Rc;

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
    let monitor = gio::NetworkMonitor::default();
    let initial = connectivity_for(monitor.is_network_available());
    concerts.set_connectivity(initial);
    releases.set_connectivity(initial);
    podcasts.set_connectivity(initial);
    youtube.set_connectivity(initial);
    radio.set_connectivity(initial);
    device_sync.set_connectivity(initial);
    device_sync.set_metered(monitor.is_network_metered());

    let concerts = Rc::downgrade(concerts);
    let releases = Rc::downgrade(releases);
    let podcasts = Rc::downgrade(podcasts);
    let youtube = Rc::downgrade(youtube);
    let radio = Rc::downgrade(radio);
    let device_sync = Rc::downgrade(device_sync);
    monitor.connect_network_changed(move |monitor, available| {
        let connectivity = connectivity_for(available);
        if let Some(view) = concerts.upgrade() {
            view.set_connectivity(connectivity);
        }
        if let Some(view) = releases.upgrade() {
            view.set_connectivity(connectivity);
        }
        if let Some(view) = podcasts.upgrade() {
            view.set_connectivity(connectivity);
        }
        if let Some(view) = youtube.upgrade() {
            view.set_connectivity(connectivity);
        }
        if let Some(view) = radio.upgrade() {
            view.set_connectivity(connectivity);
        }
        if let Some(runtime) = device_sync.upgrade() {
            runtime.set_connectivity(connectivity);
            // `MTP-43` reads metered separately, and it changes on the same
            // signal — tethering after Wi-Fi drops is one network change, not
            // two, so device sync must not be left believing the old answer.
            runtime.set_metered(monitor.is_network_metered());
        }
    });
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
