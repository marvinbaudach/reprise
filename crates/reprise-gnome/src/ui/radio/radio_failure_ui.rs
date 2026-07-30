//! Radio dead-stream actions and the explicit directory URL refresh.

use std::rc::Rc;

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
