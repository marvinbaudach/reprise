//! Radio dead-stream actions and the explicit directory URL refresh.

use std::rc::Rc;

use reprise_core::connectivity::Connectivity;
use reprise_core::radio::{self, StationRow};
use reprise_core::source_error::{FailureAction, SourceErrorKind};

use super::{activate_station, present_add_dialog, refresh_shared, show_radio_failure, Shared};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RadioFailureAction {
    RetryPlayback,
    ReresolveDirectoryUrl,
    OpenAddDialog,
    None,
}

pub(super) fn radio_failure_action(
    action: FailureAction,
    uuid: Option<&str>,
) -> RadioFailureAction {
    match action {
        FailureAction::TryAgain => RadioFailureAction::RetryPlayback,
        FailureAction::FindNewUrl if uuid.is_some_and(|uuid| !uuid.is_empty()) => {
            RadioFailureAction::ReresolveDirectoryUrl
        }
        FailureAction::FindNewUrl => RadioFailureAction::OpenAddDialog,
        FailureAction::CheckSubscription
        | FailureAction::Unsubscribe
        | FailureAction::OpenPreferences => RadioFailureAction::None,
    }
}

pub(super) fn should_clear_radio_failure(
    connectivity: Connectivity,
    failure_kind: Option<&SourceErrorKind>,
) -> bool {
    connectivity == Connectivity::Online && matches!(failure_kind, Some(SourceErrorKind::Offline))
}

/// Whether going offline may post its notice — see the podcast twin of this
/// guard. A station that is not broadcasting stays not broadcasting while the
/// connection is down, and "No connection" would both hide that and make the
/// notice look transient enough for reconnect to clear it.
pub(super) fn should_show_offline_radio_notice(
    connectivity: Connectivity,
    has_stations: bool,
    failure_kind: Option<&SourceErrorKind>,
) -> bool {
    connectivity == Connectivity::Offline
        && has_stations
        && matches!(failure_kind, None | Some(SourceErrorKind::Offline))
}

pub(super) fn reresolve_station_url(shared: &Rc<Shared>, station: &StationRow) {
    let Some(uuid) = station.uuid.clone().filter(|uuid| !uuid.is_empty()) else {
        present_add_dialog(shared);
        return;
    };
    let receiver = match crate::ui::one_shot_task::spawn("reprise-radio-find-url", move || {
        radio::click::click_and_resolve(&uuid)
    }) {
        Ok(receiver) => receiver,
        Err(error) => {
            show_radio_failure(shared, SourceErrorKind::Unreachable, error.to_string());
            return;
        }
    };
    let station_id = station.id;
    let weak = Rc::downgrade(shared);
    gtk4::glib::spawn_future_local(async move {
        let Ok(result) = receiver.recv().await else {
            return;
        };
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let stream_url = match result {
            Ok(stream_url) => stream_url,
            Err(error) => {
                let kind = SourceErrorKind::from(&error);
                show_radio_failure(&shared, kind, error.to_string());
                return;
            }
        };
        if let Err(error) = radio::station::update_stream_url(&shared.conn, station_id, &stream_url)
        {
            show_radio_failure(&shared, SourceErrorKind::Unreachable, error.to_string());
            return;
        }
        refresh_shared(&shared);
        let Some(station) = radio::station::get(&shared.conn, station_id).ok().flatten() else {
            return;
        };
        activate_station(&shared, &station);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_3_b_online_connectivity_clears_only_the_offline_radio_notice() {
        assert!(should_clear_radio_failure(
            Connectivity::Online,
            Some(&SourceErrorKind::Offline),
        ));
        assert!(!should_clear_radio_failure(
            Connectivity::Online,
            Some(&SourceErrorKind::Unreachable),
        ));
        assert!(!should_clear_radio_failure(
            Connectivity::Offline,
            Some(&SourceErrorKind::Offline),
        ));
    }

    #[test]
    fn net_3_b_going_offline_never_replaces_a_dead_station_notice() {
        // A station that is not broadcasting stays that way while the network
        // is down. Rewriting the notice as "No connection" both hid that and
        // made it transient enough for reconnect to clear it entirely.
        assert!(!should_show_offline_radio_notice(
            Connectivity::Offline,
            true,
            Some(&SourceErrorKind::Unreachable),
        ));
        assert!(!should_show_offline_radio_notice(
            Connectivity::Offline,
            true,
            Some(&SourceErrorKind::SourceGone),
        ));
        assert!(should_show_offline_radio_notice(
            Connectivity::Offline,
            true,
            None
        ));
        assert!(should_show_offline_radio_notice(
            Connectivity::Offline,
            true,
            Some(&SourceErrorKind::Offline),
        ));
        assert!(!should_show_offline_radio_notice(
            Connectivity::Offline,
            false,
            None
        ));
    }
}
