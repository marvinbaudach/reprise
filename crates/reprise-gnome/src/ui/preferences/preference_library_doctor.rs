//! Library Doctor plugin controls shared with the main-window result page.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::gio;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::{strings, PreferencesContext};

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_ENABLE: &str = "enable";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteToggleAction {
    Disable,
    Enable,
    Confirm,
}

pub(super) const fn remote_toggle_action(
    requested_active: bool,
    preference: reprise_core::library_doctor::RemoteSuggestionPreference,
) -> RemoteToggleAction {
    if !requested_active {
        RemoteToggleAction::Disable
    } else if preference.consent_required {
        RemoteToggleAction::Confirm
    } else {
        RemoteToggleAction::Enable
    }
}

pub(super) fn remote_suggestions_row(
    context: &Rc<PreferencesContext>,
    module_enabled: bool,
) -> adw::SwitchRow {
    let preference =
        reprise_core::library_doctor::remote_suggestion_preference(&context.conn.borrow())
            .unwrap_or(reprise_core::library_doctor::RemoteSuggestionPreference {
                enabled: false,
                consent_required: true,
            });
    let row = adw::SwitchRow::builder()
        .title(strings::text(strings::LIBRARY_DOCTOR_REMOTE))
        .subtitle(strings::text(strings::LIBRARY_DOCTOR_REMOTE_DESCRIPTION))
        .use_markup(false)
        .active(preference.enabled)
        .sensitive(module_enabled)
        .build();
    let syncing = Rc::new(Cell::new(false));
    let weak = Rc::downgrade(context);
    let syncing_notify = syncing.clone();
    row.connect_active_notify(move |row| {
        if syncing_notify.get() {
            return;
        }
        let Some(context) = weak.upgrade() else {
            return;
        };
        let preference =
            reprise_core::library_doctor::remote_suggestion_preference(&context.conn.borrow())
                .unwrap_or(reprise_core::library_doctor::RemoteSuggestionPreference {
                    enabled: false,
                    consent_required: true,
                });
        match remote_toggle_action(row.is_active(), preference) {
            RemoteToggleAction::Disable => {
                let result = {
                    let conn = context.conn.borrow();
                    reprise_core::library_doctor::disable_remote_suggestions(&conn)
                };
                if let Err(error) = result {
                    tracing::warn!(%error, "could not disable Library Doctor remote suggestions");
                    set_active_without_notify(row, &syncing_notify, true);
                }
            }
            RemoteToggleAction::Enable => {
                let result = {
                    let conn = context.conn.borrow();
                    reprise_core::library_doctor::accept_remote_suggestions(&conn)
                };
                if let Err(error) = result {
                    tracing::warn!(%error, "could not enable Library Doctor remote suggestions");
                    set_active_without_notify(row, &syncing_notify, false);
                }
            }
            RemoteToggleAction::Confirm => {
                set_active_without_notify(row, &syncing_notify, false);
                present_remote_confirmation(&context, row, &syncing_notify);
            }
        }
    });
    row
}

fn set_active_without_notify(row: &adw::SwitchRow, syncing: &Cell<bool>, active: bool) {
    syncing.set(true);
    row.set_active(active);
    syncing.set(false);
}

fn present_remote_confirmation(
    context: &Rc<PreferencesContext>,
    row: &adw::SwitchRow,
    syncing: &Rc<Cell<bool>>,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(strings::text(strings::LIBRARY_DOCTOR_REMOTE_HEADING))
        .body(strings::text(strings::LIBRARY_DOCTOR_REMOTE_BODY))
        .default_response(RESPONSE_CANCEL)
        .close_response(RESPONSE_CANCEL)
        .build();
    dialog.add_response(RESPONSE_CANCEL, &strings::text(strings::CANCEL));
    dialog.add_response(
        RESPONSE_ENABLE,
        &strings::text(strings::LIBRARY_DOCTOR_REMOTE_ENABLE),
    );
    dialog.set_response_appearance(RESPONSE_ENABLE, adw::ResponseAppearance::Suggested);
    let weak = Rc::downgrade(context);
    let row = row.clone();
    let syncing = syncing.clone();
    let parent = context.preferences_parent();
    dialog.choose(Some(&parent), gio::Cancellable::NONE, move |response| {
        if response != RESPONSE_ENABLE {
            return;
        }
        let Some(context) = weak.upgrade() else {
            return;
        };
        let result = {
            let conn = context.conn.borrow();
            reprise_core::library_doctor::accept_remote_suggestions(&conn)
        };
        match result {
            Ok(()) => set_active_without_notify(&row, &syncing, true),
            Err(error) => {
                tracing::warn!(%error, "could not record Library Doctor remote consent");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_1d_first_remote_enable_requires_confirmation_and_cancel_stays_off() {
        let consent_required = reprise_core::library_doctor::RemoteSuggestionPreference {
            enabled: false,
            consent_required: true,
        };
        let consented = reprise_core::library_doctor::RemoteSuggestionPreference {
            enabled: false,
            consent_required: false,
        };

        assert_eq!(
            remote_toggle_action(true, consent_required),
            RemoteToggleAction::Confirm
        );
        assert_eq!(
            remote_toggle_action(true, consented),
            RemoteToggleAction::Enable
        );
        assert_eq!(
            remote_toggle_action(false, consented),
            RemoteToggleAction::Disable
        );
    }
}
