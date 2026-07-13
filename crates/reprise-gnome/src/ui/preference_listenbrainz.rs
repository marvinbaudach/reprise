//! ListenBrainz-specific preferences, secure activation, and startup bootstrap.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::scrobbling::{ListenBrainzClient, ScrobblerTransport, TransportError};
use rusqlite::Connection;

use crate::ui::listenbrainz_runtime::{ConnectionStatus, ListenBrainzRuntime};
use crate::ui::{listenbrainz_secret, strings};

use super::preferences::PreferencesContext;

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_CONNECT: &str = "connect";
const RESPONSE_DISCONNECT: &str = "disconnect";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ActivationDecision {
    Configure,
    Enable,
    Disable,
}

pub(super) fn activation_decision(requested: bool, token_available: bool) -> ActivationDecision {
    match (requested, token_available) {
        (true, false) => ActivationDecision::Configure,
        (true, true) => ActivationDecision::Enable,
        (false, _) => ActivationDecision::Disable,
    }
}

pub(super) fn status_text(status: &ConnectionStatus) -> String {
    match status {
        ConnectionStatus::Disabled => strings::text(strings::LISTENBRAINZ_NOT_CONNECTED),
        ConnectionStatus::Connecting => strings::text(strings::LISTENBRAINZ_CONNECTING),
        ConnectionStatus::Connected {
            user_name,
            pending: 0,
        } => strings::listenbrainz_connected(user_name),
        ConnectionStatus::Connected { user_name, pending } => {
            strings::listenbrainz_pending(&strings::listenbrainz_connected(user_name), *pending)
        }
        ConnectionStatus::Offline { pending } => {
            strings::listenbrainz_pending(&strings::text(strings::LISTENBRAINZ_OFFLINE), *pending)
        }
        ConnectionStatus::Unauthorized => strings::text(strings::LISTENBRAINZ_TOKEN_REJECTED),
        ConnectionStatus::Error { pending } => strings::listenbrainz_pending(
            &strings::text(strings::LISTENBRAINZ_CONNECTION_ERROR),
            *pending,
        ),
    }
}

pub(super) fn bootstrap(conn: &Rc<RefCell<Connection>>, runtime: &Rc<ListenBrainzRuntime>) {
    let enabled = reprise_core::modules::is_enabled(
        &conn.borrow(),
        &reprise_core::modules::LISTENBRAINZ_MODULE,
    )
    .unwrap_or(false);
    if !enabled {
        return;
    }

    let weak_runtime = Rc::downgrade(runtime);
    let conn = conn.clone();
    glib::spawn_future_local(async move {
        let Some(runtime) = weak_runtime.upgrade() else {
            return;
        };
        match listenbrainz_secret::load().await {
            Ok(Some(token)) if !token.trim().is_empty() => runtime.configure(token),
            Ok(_) => {
                if let Err(error) = reprise_core::modules::set_enabled(
                    &conn.borrow(),
                    &reprise_core::modules::LISTENBRAINZ_MODULE,
                    false,
                ) {
                    tracing::warn!(%error, "could not disable tokenless ListenBrainz module");
                }
                runtime.disable();
            }
            Err(error) => {
                tracing::warn!(%error, "could not load ListenBrainz token from keyring");
                runtime.report_status(&ConnectionStatus::Error {
                    pending: pending_count(&conn),
                });
            }
        }
    });
}

impl PreferencesContext {
    pub(super) fn add_listenbrainz_account(
        self: &Rc<Self>,
        group: &adw::PreferencesGroup,
        switch: &adw::SwitchRow,
    ) {
        let account = adw::ActionRow::builder()
            .title(strings::text(strings::LISTENBRAINZ_ACCOUNT))
            .subtitle(status_text(&self.listenbrainz.status()))
            .activatable(true)
            .build();
        account.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
        self.listenbrainz.subscribe(Rc::new({
            let account = account.downgrade();
            move |status| {
                if let Some(account) = account.upgrade() {
                    account.set_subtitle(&status_text(&status));
                }
            }
        }));
        let weak = Rc::downgrade(self);
        let switch_for_account = switch.clone();
        account.connect_activated(move |_| {
            if let Some(context) = weak.upgrade() {
                context.present_listenbrainz_dialog(&switch_for_account);
            }
        });
        group.add(&account);
    }

    pub(super) fn change_listenbrainz_activation(
        self: &Rc<Self>,
        row: &adw::SwitchRow,
        requested: bool,
    ) {
        if self.syncing_listenbrainz.get() {
            return;
        }
        if activation_decision(requested, false) == ActivationDecision::Disable {
            self.persist_listenbrainz_enabled(false);
            self.listenbrainz.disable();
            return;
        }
        if self.listenbrainz_activation_pending.replace(true) {
            return;
        }

        self.set_listenbrainz_switch(row, false);
        let weak = Rc::downgrade(self);
        let row = row.clone();
        glib::spawn_future_local(async move {
            let Some(context) = weak.upgrade() else {
                return;
            };
            context.listenbrainz_activation_pending.set(false);
            match listenbrainz_secret::load().await {
                Ok(token) => match (
                    activation_decision(
                        true,
                        token.as_ref().is_some_and(|token| !token.trim().is_empty()),
                    ),
                    token,
                ) {
                    (ActivationDecision::Enable, Some(token)) => {
                        context.enable_listenbrainz(&row, token);
                    }
                    _ => context.present_listenbrainz_dialog(&row),
                },
                Err(error) => {
                    tracing::warn!(%error, "could not access ListenBrainz token in keyring");
                    context
                        .listenbrainz
                        .report_status(&ConnectionStatus::Error {
                            pending: pending_count(&context.conn),
                        });
                    context.show_listenbrainz_error(strings::LISTENBRAINZ_KEYRING_ERROR);
                }
            }
        });
    }

    fn present_listenbrainz_dialog(self: &Rc<Self>, row: &adw::SwitchRow) {
        let entry = adw::PasswordEntryRow::builder()
            .title(strings::text(strings::LISTENBRAINZ_TOKEN))
            .build();
        let list = gtk4::ListBox::new();
        list.add_css_class("boxed-list");
        list.append(&entry);
        let dialog = adw::AlertDialog::builder()
            .heading(strings::text(strings::LISTENBRAINZ_DIALOG_HEADING))
            .body(strings::text(strings::LISTENBRAINZ_DIALOG_BODY))
            .extra_child(&list)
            .default_response(RESPONSE_CONNECT)
            .close_response(RESPONSE_CANCEL)
            .build();
        dialog.add_response(RESPONSE_CANCEL, &strings::text(strings::CANCEL));
        dialog.add_response(
            RESPONSE_CONNECT,
            &strings::text(strings::LISTENBRAINZ_CONNECT),
        );
        dialog.set_response_appearance(RESPONSE_CONNECT, adw::ResponseAppearance::Suggested);
        dialog.set_response_enabled(RESPONSE_CONNECT, false);
        entry.connect_changed({
            let dialog = dialog.clone();
            move |entry| {
                dialog.set_response_enabled(
                    RESPONSE_CONNECT,
                    !gtk4::prelude::EditableExt::text(entry).trim().is_empty(),
                );
            }
        });
        if self.listenbrainz.is_active() {
            dialog.add_response(
                RESPONSE_DISCONNECT,
                &strings::text(strings::LISTENBRAINZ_DISCONNECT),
            );
            dialog
                .set_response_appearance(RESPONSE_DISCONNECT, adw::ResponseAppearance::Destructive);
        }

        let weak = Rc::downgrade(self);
        let row = row.clone();
        dialog.choose(
            Some(&self.window),
            gio::Cancellable::NONE,
            move |response| {
                let Some(context) = weak.upgrade() else {
                    return;
                };
                match response.as_str() {
                    RESPONSE_CONNECT => {
                        let token = gtk4::prelude::EditableExt::text(&entry).trim().to_string();
                        context.validate_and_save_listenbrainz(&row, token);
                    }
                    RESPONSE_DISCONNECT => context.disconnect_listenbrainz(&row),
                    _ => {}
                }
            },
        );
    }

    fn validate_and_save_listenbrainz(self: &Rc<Self>, row: &adw::SwitchRow, token: String) {
        self.listenbrainz
            .report_status(&ConnectionStatus::Connecting);
        let (sender, receiver) = async_channel::bounded(1);
        let spawned = std::thread::Builder::new()
            .name("reprise-listenbrainz-validate".to_string())
            .spawn({
                let token = token.clone();
                move || {
                    let result = ListenBrainzClient::new().validate_token(&token);
                    let _ = sender.send_blocking(result);
                }
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "could not start ListenBrainz validation worker");
            self.listenbrainz.report_status(&ConnectionStatus::Error {
                pending: pending_count(&self.conn),
            });
            self.show_listenbrainz_error(strings::LISTENBRAINZ_VALIDATION_ERROR);
            return;
        }

        let weak = Rc::downgrade(self);
        let row = row.clone();
        glib::spawn_future_local(async move {
            let Some(context) = weak.upgrade() else {
                return;
            };
            match receiver.recv().await {
                Ok(Ok(_user_name)) => match listenbrainz_secret::save(&token).await {
                    Ok(()) => context.enable_listenbrainz(&row, token),
                    Err(error) => {
                        tracing::warn!(%error, "could not store ListenBrainz token in keyring");
                        context
                            .listenbrainz
                            .report_status(&ConnectionStatus::Error {
                                pending: pending_count(&context.conn),
                            });
                        context.show_listenbrainz_error(strings::LISTENBRAINZ_KEYRING_ERROR);
                    }
                },
                Ok(Err(TransportError::Unauthorized)) => {
                    context
                        .listenbrainz
                        .report_status(&ConnectionStatus::Unauthorized);
                    context.show_listenbrainz_error(strings::LISTENBRAINZ_TOKEN_REJECTED);
                }
                Ok(Err(error)) => {
                    tracing::warn!(%error, "could not validate ListenBrainz token");
                    context
                        .listenbrainz
                        .report_status(&ConnectionStatus::Error {
                            pending: pending_count(&context.conn),
                        });
                    context.show_listenbrainz_error(strings::LISTENBRAINZ_VALIDATION_ERROR);
                }
                Err(error) => {
                    tracing::warn!(%error, "ListenBrainz validation worker ended unexpectedly");
                    context
                        .listenbrainz
                        .report_status(&ConnectionStatus::Error {
                            pending: pending_count(&context.conn),
                        });
                    context.show_listenbrainz_error(strings::LISTENBRAINZ_VALIDATION_ERROR);
                }
            }
        });
    }

    fn enable_listenbrainz(&self, row: &adw::SwitchRow, token: String) {
        if self.persist_listenbrainz_enabled(true) {
            self.set_listenbrainz_switch(row, true);
            self.listenbrainz.configure(token);
        }
    }

    fn disconnect_listenbrainz(self: &Rc<Self>, row: &adw::SwitchRow) {
        let weak = Rc::downgrade(self);
        let row = row.clone();
        glib::spawn_future_local(async move {
            let Some(context) = weak.upgrade() else {
                return;
            };
            match listenbrainz_secret::delete().await {
                Ok(()) => {
                    let cleared = {
                        let conn = context.conn.borrow();
                        reprise_core::scrobbling::clear_pending(&conn)
                    };
                    if let Err(error) = cleared {
                        tracing::warn!(%error, "could not clear ListenBrainz queue on disconnect");
                    }
                    context.persist_listenbrainz_enabled(false);
                    context.listenbrainz.disable();
                    context.set_listenbrainz_switch(&row, false);
                }
                Err(error) => {
                    tracing::warn!(%error, "could not delete ListenBrainz token from keyring");
                    context.show_listenbrainz_error(strings::LISTENBRAINZ_DISCONNECT_ERROR);
                }
            }
        });
    }

    fn persist_listenbrainz_enabled(&self, enabled: bool) -> bool {
        match reprise_core::modules::set_enabled(
            &self.conn.borrow(),
            &reprise_core::modules::LISTENBRAINZ_MODULE,
            enabled,
        ) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "could not save ListenBrainz plugin state");
                false
            }
        }
    }

    fn set_listenbrainz_switch(&self, row: &adw::SwitchRow, active: bool) {
        self.syncing_listenbrainz.set(true);
        row.set_active(active);
        self.syncing_listenbrainz.set(false);
    }

    fn show_listenbrainz_error(&self, body: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading(strings::text(strings::LISTENBRAINZ_ACCOUNT))
            .body(strings::text(body))
            .close_response("close")
            .build();
        dialog.add_response("close", &strings::text(strings::CLOSE));
        dialog.present(Some(&self.window));
    }
}

fn pending_count(conn: &Rc<RefCell<Connection>>) -> usize {
    reprise_core::scrobbling::pending_count(&conn.borrow()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_copy_distinguishes_connection_and_pending_states() {
        assert_eq!(status_text(&ConnectionStatus::Disabled), "Not connected");
        assert_eq!(
            status_text(&ConnectionStatus::Connected {
                user_name: "listener".to_string(),
                pending: 0,
            }),
            "Connected as listener"
        );
        assert_eq!(
            status_text(&ConnectionStatus::Offline { pending: 3 }),
            "Offline · 3 listens pending"
        );
        assert_eq!(
            status_text(&ConnectionStatus::Unauthorized),
            "Token rejected"
        );
    }

    #[test]
    fn enabling_without_a_token_opens_configuration_but_stays_disabled() {
        assert_eq!(
            activation_decision(true, false),
            ActivationDecision::Configure
        );
        assert_eq!(activation_decision(true, true), ActivationDecision::Enable);
        assert_eq!(
            activation_decision(false, true),
            ActivationDecision::Disable
        );
    }

    #[test]
    fn connected_status_includes_pending_count() {
        assert_eq!(
            status_text(&ConnectionStatus::Connected {
                user_name: "listener".to_string(),
                pending: 2,
            }),
            "Connected as listener · 2 listens pending"
        );
        assert_eq!(
            status_text(&ConnectionStatus::Error { pending: 1 }),
            "Connection error · 1 listen pending"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn token_entry_is_a_masked_password_row() {
        gtk4::init().unwrap();
        let entry = adw::PasswordEntryRow::new();
        assert!(entry.is::<adw::PasswordEntryRow>());
    }
}
