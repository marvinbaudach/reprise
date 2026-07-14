//! Last.fm preferences, desktop authorization, keyring storage, and bootstrap.
//!
//! Presents an inline `adw::ExpanderRow` with an enable switch, API key entry,
//! browser-based authorization, disconnect, and a live connection status subtitle.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::scrobbling::{LastFmClient, ScrobbleProvider, TransportError};
use rusqlite::Connection;

use crate::ui::lastfm_secret::{self, LastFmCredentials};
use crate::ui::scrobble_runtime::{ConnectionStatus, ScrobbleRuntime};
use crate::ui::strings;

use super::preferences::PreferencesContext;

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_CONTINUE: &str = "continue";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthorizationDecision {
    Configure,
    OpenBrowser,
    Exchange,
}

fn authorization_decision(
    api_key: &str,
    shared_secret: &str,
    browser_opened: bool,
) -> AuthorizationDecision {
    let has_credentials = !api_key.trim().is_empty() && !shared_secret.trim().is_empty();
    match (has_credentials, browser_opened) {
        (false, _) => AuthorizationDecision::Configure,
        (true, false) => AuthorizationDecision::OpenBrowser,
        (true, true) => AuthorizationDecision::Exchange,
    }
}

pub(super) fn status_text(status: &ConnectionStatus) -> String {
    let (base, submitted, pending) = match status {
        ConnectionStatus::Disabled => return strings::text(strings::LISTENBRAINZ_NOT_CONNECTED),
        ConnectionStatus::Connecting => return strings::text(strings::LISTENBRAINZ_CONNECTING),
        ConnectionStatus::Connected {
            user_name,
            pending,
            submitted,
        } => (strings::lastfm_connected(user_name), *submitted, *pending),
        ConnectionStatus::Offline { pending, submitted } => (
            strings::text(strings::LISTENBRAINZ_OFFLINE),
            *submitted,
            *pending,
        ),
        ConnectionStatus::Unauthorized => {
            return strings::text(strings::LASTFM_CREDENTIALS_REJECTED)
        }
        ConnectionStatus::Error { pending, submitted } => (
            strings::text(strings::LASTFM_CONNECTION_ERROR),
            *submitted,
            *pending,
        ),
    };
    match strings::scrobble_counts(submitted, pending) {
        Some(counts) => format!("{base} · {counts}"),
        None => base,
    }
}

struct LastFmExpanderSurface {
    expander: adw::ExpanderRow,
    api_key: adw::PasswordEntryRow,
    shared_secret: adw::PasswordEntryRow,
    open_browser: gtk4::Button,
    disconnect: gtk4::Button,
}

fn build_lastfm_expander(is_enabled: bool, connected: bool, status: &str) -> LastFmExpanderSurface {
    let description =
        crate::ui::preference_plugins::plugin_description(&reprise_core::modules::LASTFM_MODULE);
    let subtitle = if is_enabled {
        crate::ui::preference_dependencies::service_subtitle(&description, true, status)
    } else {
        description.clone()
    };

    let expander = adw::ExpanderRow::builder()
        .title(strings::text(strings::LASTFM))
        .subtitle(&subtitle)
        .show_enable_switch(true)
        .enable_expansion(is_enabled)
        .build();

    // Credential entry rows
    let api_key = adw::PasswordEntryRow::builder()
        .title(strings::text(strings::LASTFM_API_KEY))
        .build();
    expander.add_row(&api_key);

    let shared_secret = adw::PasswordEntryRow::builder()
        .title(strings::text(strings::LASTFM_SHARED_SECRET))
        .build();
    expander.add_row(&shared_secret);

    // Description hint
    let hint = adw::ActionRow::builder()
        .subtitle(strings::text(strings::LASTFM_DIALOG_BODY))
        .build();
    hint.add_css_class("property");
    expander.add_row(&hint);

    // Open Browser action row with suffix button
    let open_browser = gtk4::Button::builder()
        .label(strings::text(strings::OPEN_BROWSER))
        .valign(gtk4::Align::Center)
        .build();
    open_browser.add_css_class("suggested-action");
    open_browser.set_sensitive(false);
    let browser_row = adw::ActionRow::builder()
        .title(strings::text(strings::OPEN_BROWSER))
        .activatable_widget(&open_browser)
        .build();
    browser_row.add_suffix(&open_browser);
    expander.add_row(&browser_row);

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

    // Gate Open Browser on non-empty credentials
    for entry in [&api_key, &shared_secret] {
        entry.connect_changed({
            let open_browser = open_browser.clone();
            let api_key = api_key.clone();
            let shared_secret = shared_secret.clone();
            move |_| {
                open_browser.set_sensitive(
                    !api_key.text().trim().is_empty() && !shared_secret.text().trim().is_empty(),
                );
            }
        });
    }

    // Sensitivity gating for body rows when enable switch is off
    expander.connect_enable_expansion_notify({
        let api_key = api_key.downgrade();
        let shared_secret = shared_secret.downgrade();
        let browser_row = browser_row.downgrade();
        let hint = hint.downgrade();
        let disconnect_row = disconnect_row.downgrade();
        move |expander| {
            let enabled = expander.enables_expansion();
            if let Some(w) = api_key.upgrade() {
                w.set_sensitive(enabled);
            }
            if let Some(w) = shared_secret.upgrade() {
                w.set_sensitive(enabled);
            }
            if let Some(row) = browser_row.upgrade() {
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
    api_key.set_sensitive(body_sensitive);
    shared_secret.set_sensitive(body_sensitive);
    browser_row.set_sensitive(body_sensitive);
    hint.set_sensitive(body_sensitive);
    disconnect.set_sensitive(body_sensitive);

    LastFmExpanderSurface {
        expander,
        api_key,
        shared_secret,
        open_browser,
        disconnect,
    }
}

pub(super) fn bootstrap(conn: &Rc<RefCell<Connection>>, runtime: &Rc<ScrobbleRuntime>) {
    let enabled =
        reprise_core::modules::is_enabled(&conn.borrow(), &reprise_core::modules::LASTFM_MODULE)
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
        match lastfm_secret::load().await {
            Ok(Some(credentials)) => match client_for(&credentials) {
                Ok(client) => runtime.configure(credentials.session_key, Box::new(client)),
                Err(error) => {
                    tracing::warn!(%error, "stored Last.fm credentials are incomplete");
                    disable_module(&conn, &runtime);
                }
            },
            Ok(None) => disable_module(&conn, &runtime),
            Err(error) => {
                tracing::warn!(%error, "could not load Last.fm credentials from keyring");
                runtime.report_status(&ConnectionStatus::Error {
                    pending: pending_count(&conn),
                    submitted: 0,
                });
            }
        }
    });
}

fn client_for(
    credentials: &LastFmCredentials,
) -> Result<LastFmClient, reprise_core::scrobbling::MetadataError> {
    LastFmClient::new(&credentials.api_key, &credentials.shared_secret)
}

fn disable_module(conn: &Rc<RefCell<Connection>>, runtime: &ScrobbleRuntime) {
    if let Err(error) = reprise_core::modules::set_enabled(
        &conn.borrow(),
        &reprise_core::modules::LASTFM_MODULE,
        false,
    ) {
        tracing::warn!(%error, "could not disable Last.fm plugin");
    }
    runtime.disable();
}

impl PreferencesContext {
    /// Build the Last.fm expander row and wire up all controls.
    /// Returns the `ExpanderRow` to be added to the plugins group.
    pub(super) fn build_lastfm_row(self: &Rc<Self>) -> adw::ExpanderRow {
        let is_enabled = reprise_core::modules::is_enabled(
            &self.conn.borrow(),
            &reprise_core::modules::LASTFM_MODULE,
        )
        .unwrap_or(false);
        let connected = self.lastfm.is_active();
        let status = status_text(&self.lastfm.status());
        let surface = build_lastfm_expander(is_enabled, connected, &status);

        let description = crate::ui::preference_plugins::plugin_description(
            &reprise_core::modules::LASTFM_MODULE,
        );

        // Subscribe to runtime status changes for subtitle updates
        self.lastfm.subscribe(Rc::new({
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
        self.lastfm.subscribe(Rc::new({
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
                    expander.set_subtitle(&crate::ui::preference_dependencies::service_subtitle(
                        &description_for_toggle,
                        expander.enables_expansion(),
                        &status_text(&context.lastfm.status()),
                    ));
                    context.change_lastfm_activation(expander, expander.enables_expansion());
                }
            });

        // Open Browser button
        let weak = Rc::downgrade(self);
        let expander_for_browser = surface.expander.clone();
        let api_key = surface.api_key.clone();
        let shared_secret = surface.shared_secret.clone();
        surface.open_browser.connect_clicked(move |_| {
            if let Some(context) = weak.upgrade() {
                context.request_lastfm_authorization(
                    &expander_for_browser,
                    api_key.text().trim().to_string(),
                    shared_secret.text().trim().to_string(),
                );
            }
        });

        // Disconnect button
        let weak = Rc::downgrade(self);
        let expander_for_disconnect = surface.expander.clone();
        surface.disconnect.connect_clicked(move |_| {
            if let Some(context) = weak.upgrade() {
                context.disconnect_lastfm(&expander_for_disconnect);
            }
        });

        surface.expander
    }

    pub(super) fn change_lastfm_activation(
        self: &Rc<Self>,
        row: &adw::ExpanderRow,
        requested: bool,
    ) {
        if self.syncing_lastfm.get() {
            return;
        }
        if !requested {
            self.persist_lastfm_enabled(false);
            self.lastfm.disable();
            return;
        }
        if self.lastfm_activation_pending.replace(true) {
            return;
        }
        set_activation_pending(row, true);
        let weak = Rc::downgrade(self);
        let row = row.clone();
        glib::spawn_future_local(async move {
            let result = lastfm_secret::load().await;
            let Some(context) = weak.upgrade() else {
                return;
            };
            context.lastfm_activation_pending.set(false);
            set_activation_pending(&row, false);
            match result {
                Ok(Some(credentials)) if client_for(&credentials).is_ok() => {
                    context.enable_lastfm(&row, credentials);
                }
                Ok(_) => {
                    // No stored credentials: expand the row so the user sees
                    // the credential fields, but keep the enable switch on.
                    row.set_expanded(true);
                }
                Err(error) => {
                    tracing::warn!(%error, "could not access Last.fm credentials in keyring");
                    context.restore_lastfm_switch(&row);
                    context.show_lastfm_error(strings::LASTFM_KEYRING_ERROR);
                }
            }
        });
    }

    fn request_lastfm_authorization(
        self: &Rc<Self>,
        row: &adw::ExpanderRow,
        api_key: String,
        shared_secret: String,
    ) {
        if authorization_decision(&api_key, &shared_secret, false)
            != AuthorizationDecision::OpenBrowser
        {
            self.restore_lastfm_switch(row);
            self.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
            return;
        }
        self.lastfm.report_status(&ConnectionStatus::Connecting);
        let (sender, receiver) = async_channel::bounded(1);
        let worker_api_key = api_key.clone();
        let worker_secret = shared_secret.clone();
        let spawned = std::thread::Builder::new()
            .name("reprise-lastfm-token".to_string())
            .spawn(move || {
                let result = (|| {
                    let client = LastFmClient::new(&worker_api_key, &worker_secret)?;
                    let token = client.request_token()?;
                    let url = client.authorization_url(&token)?;
                    Ok::<_, TransportError>((token, url))
                })();
                let _ = sender.send_blocking(result);
            });
        if spawned.is_err() {
            self.restore_lastfm_switch(row);
            self.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
            return;
        }
        let weak = Rc::downgrade(self);
        let row = row.clone();
        glib::spawn_future_local(async move {
            let Some(context) = weak.upgrade() else {
                return;
            };
            match receiver.recv().await {
                Ok(Ok((token, url))) => {
                    match gio::AppInfo::launch_default_for_uri(&url, gio::AppLaunchContext::NONE) {
                        Ok(()) => {
                            context.present_lastfm_confirmation(
                                &row,
                                api_key,
                                shared_secret,
                                token,
                            );
                        }
                        Err(error) => {
                            tracing::warn!(%error, "could not open Last.fm authorization URL");
                            context.restore_lastfm_switch(&row);
                            context.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
                        }
                    }
                }
                Ok(Err(TransportError::Unauthorized)) => {
                    context
                        .lastfm
                        .report_status(&ConnectionStatus::Unauthorized);
                    context.restore_lastfm_switch(&row);
                    context.show_lastfm_error(strings::LASTFM_CREDENTIALS_REJECTED);
                }
                Ok(Err(error)) => {
                    tracing::warn!(%error, "could not request Last.fm authorization");
                    context.restore_lastfm_switch(&row);
                    context.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
                }
                Err(error) => {
                    tracing::warn!(%error, "Last.fm authorization worker ended unexpectedly");
                    context.restore_lastfm_switch(&row);
                    context.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
                }
            }
        });
    }

    fn present_lastfm_confirmation(
        self: &Rc<Self>,
        row: &adw::ExpanderRow,
        api_key: String,
        shared_secret: String,
        token: String,
    ) {
        if authorization_decision(&api_key, &shared_secret, true) != AuthorizationDecision::Exchange
        {
            self.restore_lastfm_switch(row);
            self.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
            return;
        }
        let dialog = adw::AlertDialog::builder()
            .heading(strings::text(strings::LASTFM_AUTHORIZE_HEADING))
            .body(strings::text(strings::LASTFM_AUTHORIZE_BODY))
            .default_response(RESPONSE_CONTINUE)
            .close_response(RESPONSE_CANCEL)
            .build();
        dialog.add_response(RESPONSE_CANCEL, &strings::text(strings::CANCEL));
        dialog.add_response(RESPONSE_CONTINUE, &strings::text(strings::LASTFM_CONTINUE));
        dialog.set_response_appearance(RESPONSE_CONTINUE, adw::ResponseAppearance::Suggested);
        let weak = Rc::downgrade(self);
        let row = row.clone();
        let parent = self.preferences_parent();
        dialog.choose(Some(&parent), gio::Cancellable::NONE, move |response| {
            if response == RESPONSE_CONTINUE {
                if let Some(context) = weak.upgrade() {
                    context.exchange_lastfm_token(&row, api_key, shared_secret, token);
                }
            }
        });
    }

    fn exchange_lastfm_token(
        self: &Rc<Self>,
        row: &adw::ExpanderRow,
        api_key: String,
        shared_secret: String,
        token: String,
    ) {
        let (sender, receiver) = async_channel::bounded(1);
        let worker_api_key = api_key.clone();
        let worker_secret = shared_secret.clone();
        let spawned = std::thread::Builder::new()
            .name("reprise-lastfm-session".to_string())
            .spawn(move || {
                let result = LastFmClient::new(&worker_api_key, &worker_secret)
                    .map_err(TransportError::from)
                    .and_then(|client| client.exchange_token(&token));
                let _ = sender.send_blocking(result);
            });
        if spawned.is_err() {
            self.restore_lastfm_switch(row);
            self.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
            return;
        }
        let weak = Rc::downgrade(self);
        let row = row.clone();
        glib::spawn_future_local(async move {
            let Some(context) = weak.upgrade() else {
                return;
            };
            match receiver.recv().await {
                Ok(Ok(session)) => {
                    let credentials = LastFmCredentials {
                        api_key,
                        shared_secret,
                        session_key: session.session_key,
                        user_name: session.user_name,
                    };
                    match lastfm_secret::store(&credentials).await {
                        Ok(()) => context.enable_lastfm(&row, credentials),
                        Err(error) => {
                            tracing::warn!(%error, "could not store Last.fm credentials");
                            context.restore_lastfm_switch(&row);
                            context.show_lastfm_error(strings::LASTFM_KEYRING_ERROR);
                        }
                    }
                }
                Ok(Err(TransportError::Unauthorized)) => {
                    context
                        .lastfm
                        .report_status(&ConnectionStatus::Unauthorized);
                    context.restore_lastfm_switch(&row);
                    context.show_lastfm_error(strings::LASTFM_CREDENTIALS_REJECTED);
                }
                Ok(Err(error)) => {
                    tracing::warn!(%error, "could not exchange Last.fm authorization token");
                    context.restore_lastfm_switch(&row);
                    context.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
                }
                Err(error) => {
                    tracing::warn!(%error, "Last.fm session worker ended unexpectedly");
                    context.restore_lastfm_switch(&row);
                    context.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
                }
            }
        });
    }

    fn enable_lastfm(&self, row: &adw::ExpanderRow, credentials: LastFmCredentials) {
        let Ok(client) = client_for(&credentials) else {
            self.restore_lastfm_switch(row);
            self.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
            return;
        };
        if self.persist_lastfm_enabled(true) {
            self.set_lastfm_switch(row, true);
            self.lastfm
                .configure(credentials.session_key, Box::new(client));
        } else {
            self.restore_lastfm_switch(row);
        }
    }

    fn disconnect_lastfm(self: &Rc<Self>, row: &adw::ExpanderRow) {
        let weak = Rc::downgrade(self);
        let row = row.clone();
        glib::spawn_future_local(async move {
            let Some(context) = weak.upgrade() else {
                return;
            };
            match lastfm_secret::delete().await {
                Ok(()) => {
                    if let Err(error) = reprise_core::scrobbling::clear_pending_for(
                        &context.conn.borrow(),
                        ScrobbleProvider::LastFm,
                    ) {
                        tracing::warn!(%error, "could not clear Last.fm queue on disconnect");
                    }
                    context.persist_lastfm_enabled(false);
                    context.lastfm.disable();
                    context.set_lastfm_switch(&row, false);
                }
                Err(error) => {
                    tracing::warn!(%error, "could not delete Last.fm credentials");
                    context.restore_lastfm_switch(&row);
                    context.show_lastfm_error(strings::LASTFM_DISCONNECT_ERROR);
                }
            }
        });
    }

    fn persist_lastfm_enabled(&self, enabled: bool) -> bool {
        reprise_core::modules::set_enabled(
            &self.conn.borrow(),
            &reprise_core::modules::LASTFM_MODULE,
            enabled,
        )
        .map_or_else(
            |error| {
                tracing::warn!(%error, "could not save Last.fm plugin state");
                false
            },
            |()| true,
        )
    }

    fn set_lastfm_switch(&self, row: &adw::ExpanderRow, active: bool) {
        self.syncing_lastfm.set(true);
        row.set_enable_expansion(active);
        self.syncing_lastfm.set(false);
    }

    fn restore_lastfm_switch(&self, row: &adw::ExpanderRow) {
        self.set_lastfm_switch(row, self.lastfm.is_active());
    }

    fn show_lastfm_error(&self, body: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading(strings::text(strings::LASTFM_ACCOUNT))
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
    reprise_core::scrobbling::pending_count_for(&conn.borrow(), ScrobbleProvider::LastFm)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_steps_require_credentials_browser_then_confirmation() {
        assert_eq!(
            authorization_decision("", "secret", false),
            AuthorizationDecision::Configure
        );
        assert_eq!(
            authorization_decision("key", "secret", false),
            AuthorizationDecision::OpenBrowser
        );
        assert_eq!(
            authorization_decision("key", "secret", true),
            AuthorizationDecision::Exchange
        );
    }

    #[test]
    fn connected_status_includes_lastfm_queued_count() {
        let text = status_text(&ConnectionStatus::Connected {
            user_name: "listener".to_string(),
            pending: 2,
            submitted: 0,
        });
        assert!(text.contains("listener"));
        assert!(text.contains('2'));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn expander_row_has_enable_switch_credentials_and_action_buttons() {
        gtk4::init().unwrap();
        let surface = build_lastfm_expander(false, false, "Not connected");
        assert!(surface.expander.shows_enable_switch());
        assert!(!surface.expander.enables_expansion());
        assert!(surface.api_key.is::<adw::PasswordEntryRow>());
        assert!(surface.shared_secret.is::<adw::PasswordEntryRow>());
        assert!(!surface.open_browser.is_sensitive());

        // Disconnect button's parent row is hidden when not connected
        assert!(surface.disconnect.parent().is_some_and(|p| !p.is_visible()));

        // Credentials gate Open Browser
        surface.api_key.set_text("key");
        assert!(!surface.open_browser.is_sensitive());
        surface.shared_secret.set_text("secret");
        assert!(surface.open_browser.is_sensitive());
        surface.api_key.set_text("  ");
        assert!(!surface.open_browser.is_sensitive());

        // When enabled + connected, body rows are sensitive and disconnect visible
        let enabled_surface = build_lastfm_expander(true, true, "Connected as listener");
        assert!(enabled_surface.expander.enables_expansion());
        assert!(enabled_surface.api_key.is_sensitive());
        assert!(enabled_surface
            .disconnect
            .parent()
            .is_some_and(|p| p.is_visible()));
    }
}
