//! ListenBrainz-specific preferences, secure activation, and startup bootstrap.
//!
//! Presents an inline `adw::ExpanderRow` with an enable switch, token entry,
//! connect / disconnect controls, and a live connection status subtitle.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::scrobbling::{ListenBrainzClient, ScrobblerTransport, TransportError};
use rusqlite::Connection;

use crate::ui::scrobble_runtime::{ConnectionStatus, ScrobbleRuntime};
use crate::ui::{listenbrainz_secret, strings};

use super::preferences::PreferencesContext;

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

struct ListenBrainzExpanderSurface {
    expander: adw::ExpanderRow,
    token: adw::PasswordEntryRow,
    connect: gtk4::Button,
    disconnect: gtk4::Button,
}

fn build_listenbrainz_expander(
    is_enabled: bool,
    connected: bool,
    status: &str,
) -> ListenBrainzExpanderSurface {
    let description = crate::ui::preference_plugins::plugin_description(
        &reprise_core::modules::LISTENBRAINZ_MODULE,
    );
    let subtitle = if is_enabled {
        crate::ui::preference_dependencies::service_subtitle(&description, true, status)
    } else {
        description.clone()
    };

    let expander = adw::ExpanderRow::builder()
        .title(strings::text(strings::LISTENBRAINZ))
        .subtitle(&subtitle)
        .show_enable_switch(true)
        .enable_expansion(is_enabled)
        .build();

    // Token entry row
    let token = adw::PasswordEntryRow::builder()
        .title(strings::text(strings::LISTENBRAINZ_TOKEN))
        .build();
    expander.add_row(&token);

    // Description hint
    let hint = adw::ActionRow::builder()
        .subtitle(strings::text(strings::LISTENBRAINZ_DIALOG_BODY))
        .build();
    hint.add_css_class("property");
    expander.add_row(&hint);

    // Connect action row with suffix button
    let connect = gtk4::Button::builder()
        .label(strings::text(strings::LISTENBRAINZ_CONNECT))
        .valign(gtk4::Align::Center)
        .build();
    connect.add_css_class("suggested-action");
    connect.set_sensitive(false);
    let connect_row = adw::ActionRow::builder()
        .title(strings::text(strings::LISTENBRAINZ_CONNECT))
        .activatable_widget(&connect)
        .build();
    connect_row.add_suffix(&connect);
    expander.add_row(&connect_row);

    // Disconnect action row with destructive button
    let disconnect = gtk4::Button::builder()
        .label(strings::text(strings::LISTENBRAINZ_DISCONNECT))
        .valign(gtk4::Align::Center)
        .build();
    disconnect.add_css_class("destructive-action");
    let disconnect_row = adw::ActionRow::builder()
        .title(strings::text(strings::LISTENBRAINZ_DISCONNECT))
        .activatable_widget(&disconnect)
        .build();
    disconnect_row.add_suffix(&disconnect);
    disconnect_row.set_visible(connected);
    expander.add_row(&disconnect_row);

    // Gate connect button on non-empty token
    token.connect_changed({
        let connect = connect.clone();
        move |token| connect.set_sensitive(!token.text().trim().is_empty())
    });

    // Decouple enable switch from expandability: when the switch is toggled
    // off, adw::ExpanderRow collapses the body by default. Re-expand
    // immediately so the user can still see (greyed-out) contents.
    expander.connect_enable_expansion_notify({
        let token = token.downgrade();
        let connect_row = connect_row.downgrade();
        let hint = hint.downgrade();
        let disconnect_row = disconnect_row.downgrade();
        move |expander| {
            let enabled = expander.enables_expansion();
            // Keep body rows sensitive only when enabled
            if let Some(w) = token.upgrade() {
                w.set_sensitive(enabled);
            }
            if let Some(row) = connect_row.upgrade() {
                row.set_sensitive(enabled);
            }
            if let Some(row) = hint.upgrade() {
                row.set_sensitive(enabled);
            }
            if let Some(row) = disconnect_row.upgrade() {
                row.set_sensitive(enabled);
            }
        }
    });

    // Apply initial sensitivity
    let body_sensitive = is_enabled;
    token.set_sensitive(body_sensitive);
    connect_row.set_sensitive(body_sensitive);
    hint.set_sensitive(body_sensitive);
    disconnect.set_sensitive(body_sensitive);

    ListenBrainzExpanderSurface {
        expander,
        token,
        connect,
        disconnect,
    }
}

pub(super) fn bootstrap(conn: &Rc<RefCell<Connection>>, runtime: &Rc<ScrobbleRuntime>) {
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
            Ok(Some(token)) if !token.trim().is_empty() => {
                runtime.configure(token, Box::new(ListenBrainzClient::new()));
            }
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
    /// Build the ListenBrainz expander row and wire up all controls.
    /// Returns the `ExpanderRow` to be added to the plugins group.
    pub(super) fn build_listenbrainz_row(self: &Rc<Self>) -> adw::ExpanderRow {
        let is_enabled = reprise_core::modules::is_enabled(
            &self.conn.borrow(),
            &reprise_core::modules::LISTENBRAINZ_MODULE,
        )
        .unwrap_or(false);
        let connected = self.listenbrainz.is_active();
        let status = status_text(&self.listenbrainz.status());
        let surface = build_listenbrainz_expander(is_enabled, connected, &status);

        let description = crate::ui::preference_plugins::plugin_description(
            &reprise_core::modules::LISTENBRAINZ_MODULE,
        );

        // Subscribe to runtime status changes for subtitle updates
        self.listenbrainz.subscribe(Rc::new({
            let expander = surface.expander.downgrade();
            let description = description.clone();
            move |status| {
                if let Some(expander) = expander.upgrade() {
                    expander.set_subtitle(&crate::ui::preference_dependencies::service_subtitle(
                        &description,
                        expander.enables_expansion(),
                        &status_text(&status),
                    ));
                }
            }
        }));

        // Disconnect button visibility tracks connection state
        let disconnect_button = surface.disconnect.clone();
        self.listenbrainz.subscribe(Rc::new({
            let disconnect = disconnect_button.downgrade();
            move |status| {
                if let Some(button) = disconnect.upgrade() {
                    let is_connected = !matches!(status, ConnectionStatus::Disabled);
                    if let Some(parent) = button.parent() {
                        parent.set_visible(is_connected);
                    }
                }
            }
        }));

        // Enable switch toggle
        let weak = Rc::downgrade(self);
        let description_for_toggle = description.clone();
        surface
            .expander
            .connect_enable_expansion_notify(move |expander| {
                if let Some(context) = weak.upgrade() {
                    // Update subtitle immediately
                    expander.set_subtitle(&crate::ui::preference_dependencies::service_subtitle(
                        &description_for_toggle,
                        expander.enables_expansion(),
                        &status_text(&context.listenbrainz.status()),
                    ));
                    context.change_listenbrainz_activation(expander, expander.enables_expansion());
                }
            });

        // Connect button
        let weak = Rc::downgrade(self);
        let expander_for_connect = surface.expander.clone();
        let token_for_connect = surface.token.clone();
        surface.connect.connect_clicked(move |_| {
            if let Some(context) = weak.upgrade() {
                context.validate_and_save_listenbrainz(
                    &expander_for_connect,
                    token_for_connect.text().trim().to_string(),
                );
            }
        });

        // Disconnect button
        let weak = Rc::downgrade(self);
        let expander_for_disconnect = surface.expander.clone();
        surface.disconnect.connect_clicked(move |_| {
            if let Some(context) = weak.upgrade() {
                context.disconnect_listenbrainz(&expander_for_disconnect);
            }
        });

        surface.expander
    }

    pub(super) fn change_listenbrainz_activation(
        self: &Rc<Self>,
        row: &adw::ExpanderRow,
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

        set_activation_pending(row, true);
        let weak = Rc::downgrade(self);
        let row = row.clone();
        glib::spawn_future_local(async move {
            let result = listenbrainz_secret::load().await;
            let Some(context) = weak.upgrade() else {
                return;
            };
            context.listenbrainz_activation_pending.set(false);
            set_activation_pending(&row, false);
            match result {
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
                    _ => {
                        // No stored token: expand the row so the user sees the
                        // token field, but keep the enable switch on (it will
                        // revert if the user closes without connecting).
                        row.set_expanded(true);
                    }
                },
                Err(error) => {
                    tracing::warn!(%error, "could not access ListenBrainz token in keyring");
                    context
                        .listenbrainz
                        .report_status(&ConnectionStatus::Error {
                            pending: pending_count(&context.conn),
                        });
                    context.restore_listenbrainz_switch(&row);
                    context.show_listenbrainz_error(strings::LISTENBRAINZ_KEYRING_ERROR);
                }
            }
        });
    }

    fn validate_and_save_listenbrainz(self: &Rc<Self>, row: &adw::ExpanderRow, token: String) {
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
            self.restore_listenbrainz_switch(row);
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
                        context.restore_listenbrainz_switch(&row);
                        context.show_listenbrainz_error(strings::LISTENBRAINZ_KEYRING_ERROR);
                    }
                },
                Ok(Err(TransportError::Unauthorized)) => {
                    context
                        .listenbrainz
                        .report_status(&ConnectionStatus::Unauthorized);
                    context.restore_listenbrainz_switch(&row);
                    context.show_listenbrainz_error(strings::LISTENBRAINZ_TOKEN_REJECTED);
                }
                Ok(Err(error)) => {
                    tracing::warn!(%error, "could not validate ListenBrainz token");
                    context
                        .listenbrainz
                        .report_status(&ConnectionStatus::Error {
                            pending: pending_count(&context.conn),
                        });
                    context.restore_listenbrainz_switch(&row);
                    context.show_listenbrainz_error(strings::LISTENBRAINZ_VALIDATION_ERROR);
                }
                Err(error) => {
                    tracing::warn!(%error, "ListenBrainz validation worker ended unexpectedly");
                    context
                        .listenbrainz
                        .report_status(&ConnectionStatus::Error {
                            pending: pending_count(&context.conn),
                        });
                    context.restore_listenbrainz_switch(&row);
                    context.show_listenbrainz_error(strings::LISTENBRAINZ_VALIDATION_ERROR);
                }
            }
        });
    }

    fn enable_listenbrainz(&self, row: &adw::ExpanderRow, token: String) {
        if self.persist_listenbrainz_enabled(true) {
            self.set_listenbrainz_switch(row, true);
            self.listenbrainz
                .configure(token, Box::new(ListenBrainzClient::new()));
        } else {
            self.restore_listenbrainz_switch(row);
        }
    }

    fn disconnect_listenbrainz(self: &Rc<Self>, row: &adw::ExpanderRow) {
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
                    context.restore_listenbrainz_switch(&row);
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

    fn set_listenbrainz_switch(&self, row: &adw::ExpanderRow, active: bool) {
        self.syncing_listenbrainz.set(true);
        row.set_enable_expansion(active);
        self.syncing_listenbrainz.set(false);
    }

    fn restore_listenbrainz_switch(&self, row: &adw::ExpanderRow) {
        self.set_listenbrainz_switch(row, self.listenbrainz.is_active());
    }

    fn show_listenbrainz_error(&self, body: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading(strings::text(strings::LISTENBRAINZ_ACCOUNT))
            .body(strings::text(body))
            .close_response("close")
            .build();
        dialog.add_response("close", &strings::text(strings::CLOSE));
        dialog.present(Some(&self.preferences_parent()));
    }
}

/// Mark the expander row as pending (sensitive = false, switch forced on).
fn set_activation_pending(row: &adw::ExpanderRow, pending: bool) {
    if pending {
        row.set_enable_expansion(true);
    }
    row.set_sensitive(!pending);
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
    fn expander_row_has_enable_switch_token_entry_and_action_buttons() {
        gtk4::init().unwrap();
        let surface = build_listenbrainz_expander(false, false, "Not connected");
        assert!(surface.expander.shows_enable_switch());
        assert!(!surface.expander.enables_expansion());
        assert!(surface.token.is::<adw::PasswordEntryRow>());
        assert!(!surface.connect.is_sensitive());

        // Disconnect button's parent row is hidden when not connected
        assert!(surface.disconnect.parent().is_some_and(|p| !p.is_visible()));

        // Token gates Connect
        surface.token.set_text("token");
        assert!(surface.connect.is_sensitive());
        surface.token.set_text("  ");
        assert!(!surface.connect.is_sensitive());

        // When enabled, body rows become sensitive
        let enabled_surface = build_listenbrainz_expander(true, true, "Connected as listener");
        assert!(enabled_surface.expander.enables_expansion());
        assert!(enabled_surface.token.is_sensitive());
        assert!(enabled_surface
            .disconnect
            .parent()
            .is_some_and(|p| p.is_visible()));
    }
}
