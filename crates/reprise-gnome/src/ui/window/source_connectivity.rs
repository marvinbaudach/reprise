use std::cell::Cell;
use std::rc::{Rc, Weak};

#[cfg(feature = "test-fixtures")]
use std::time::Duration;

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
    state: Rc<Cell<Connectivity>>,
    concerts: super::content_stack::DeferredPage<crate::ui::concerts::ConcertsView>,
    releases: Weak<crate::ui::releases::ReleasesView>,
    podcasts: super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    youtube: super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    radio: super::content_stack::DeferredPage<crate::ui::radio::RadioView>,
    preferences: Weak<crate::ui::preferences::PreferencesContext>,
}

impl ConnectivityTargets {
    fn new(
        concerts: &super::content_stack::DeferredPage<crate::ui::concerts::ConcertsView>,
        releases: &Rc<crate::ui::releases::ReleasesView>,
        podcasts: &super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
        youtube: &super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
        radio: &super::content_stack::DeferredPage<crate::ui::radio::RadioView>,
        preferences: &Rc<crate::ui::preferences::PreferencesContext>,
    ) -> Self {
        let state = Rc::new(Cell::new(Connectivity::Online));
        for page in [podcasts, youtube] {
            let state = state.clone();
            page.on_materialized(move |view| view.set_connectivity(state.get()));
        }
        {
            let state = state.clone();
            concerts.on_materialized(move |view| view.set_connectivity(state.get()));
        }
        {
            let state = state.clone();
            radio.on_materialized(move |view| view.set_connectivity(state.get()));
        }
        Self {
            state,
            concerts: concerts.clone(),
            releases: Rc::downgrade(releases),
            podcasts: podcasts.clone(),
            youtube: youtube.clone(),
            radio: radio.clone(),
            preferences: Rc::downgrade(preferences),
        }
    }

    fn project(&self, connectivity: Connectivity) {
        self.state.set(connectivity);
        self.concerts
            .if_materialized(|view| view.set_connectivity(connectivity));
        if let Some(view) = self.releases.upgrade() {
            view.set_connectivity(connectivity);
        }
        self.podcasts
            .if_materialized(|view| view.set_connectivity(connectivity));
        self.youtube
            .if_materialized(|view| view.set_connectivity(connectivity));
        self.radio
            .if_materialized(|view| view.set_connectivity(connectivity));
        if let Some(preferences) = self.preferences.upgrade() {
            preferences.set_connectivity(connectivity);
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
    targets.project(initial);
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
        targets.project(next);
        gtk4::glib::ControlFlow::Continue
    });
    true
}

/// Projects `gio::NetworkMonitor` at the window boundary and pushes the result
/// into every seam that needs it. `NET-3a` requires *connectivity* to be an
/// explicitly set value rather than something each consumer infers where it
/// stands, and this is the only place that derives it.
///
/// One direct reader remains and is deliberate: `podcast_refresh_scheduler`
/// asks for `is_network_metered()` at the moment it decides whether to start an
/// automatic refresh. That is a different fact from connectivity, it can change
/// between two refreshes, and it has no state to keep in sync — so pushing it
/// would buy nothing.
pub(super) fn wire(
    concerts: &super::content_stack::DeferredPage<crate::ui::concerts::ConcertsView>,
    releases: &Rc<crate::ui::releases::ReleasesView>,
    podcasts: &super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    youtube: &super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    radio: &super::content_stack::DeferredPage<crate::ui::radio::RadioView>,
    preferences: &Rc<crate::ui::preferences::PreferencesContext>,
) {
    let targets =
        ConnectivityTargets::new(concerts, releases, podcasts, youtube, radio, preferences);
    #[cfg(feature = "test-fixtures")]
    if wire_test_connectivity(targets.clone()) {
        return;
    }
    let monitor = gio::NetworkMonitor::default();
    let initial = connectivity_for(monitor.is_network_available());
    targets.project(initial);
    monitor.connect_network_changed(move |_monitor, available| {
        let connectivity = connectivity_for(available);
        targets.project(connectivity);
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
