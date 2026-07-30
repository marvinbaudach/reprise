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

pub(super) fn wire(
    podcasts: &Rc<crate::ui::podcasts::PodcastsView>,
    youtube: &Rc<crate::ui::podcasts::PodcastsView>,
    radio: &Rc<crate::ui::radio::RadioView>,
) {
    let monitor = gio::NetworkMonitor::default();
    let initial = connectivity_for(monitor.is_network_available());
    podcasts.set_connectivity(initial);
    youtube.set_connectivity(initial);
    radio.set_connectivity(initial);

    let podcasts = Rc::downgrade(podcasts);
    let youtube = Rc::downgrade(youtube);
    let radio = Rc::downgrade(radio);
    monitor.connect_network_changed(move |_, available| {
        let connectivity = connectivity_for(available);
        if let Some(view) = podcasts.upgrade() {
            view.set_connectivity(connectivity);
        }
        if let Some(view) = youtube.upgrade() {
            view.set_connectivity(connectivity);
        }
        if let Some(view) = radio.upgrade() {
            view.set_connectivity(connectivity);
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
