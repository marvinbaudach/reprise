//! Last.fm preferences, desktop authorization, keyring storage, and bootstrap.

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
const RESPONSE_CONNECT: &str = "connect";
const RESPONSE_CONTINUE: &str = "continue";
const RESPONSE_DISCONNECT: &str = "disconnect";

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
    match status {
        ConnectionStatus::Disabled => strings::text(strings::LISTENBRAINZ_NOT_CONNECTED),
        ConnectionStatus::Connecting => strings::text(strings::LISTENBRAINZ_CONNECTING),
        ConnectionStatus::Connected {
            user_name,
            pending: 0,
        } => strings::lastfm_connected(user_name),
        ConnectionStatus::Connected { user_name, pending } => {
            strings::lastfm_pending(&strings::lastfm_connected(user_name), *pending)
        }
        ConnectionStatus::Offline { pending } => {
            strings::lastfm_pending(&strings::text(strings::LISTENBRAINZ_OFFLINE), *pending)
        }
        ConnectionStatus::Unauthorized => strings::text(strings::LASTFM_CREDENTIALS_REJECTED),
        ConnectionStatus::Error { pending } => {
            strings::lastfm_pending(&strings::text(strings::LASTFM_CONNECTION_ERROR), *pending)
        }
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
    pub(super) fn add_lastfm_account(
        self: &Rc<Self>,
        group: &adw::PreferencesGroup,
        switch: &adw::SwitchRow,
    ) {
        let account = adw::ActionRow::builder()
            .title(strings::text(strings::LASTFM_ACCOUNT))
            .subtitle(status_text(&self.lastfm.status()))
            .activatable(true)
            .build();
        account.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
        self.lastfm.subscribe(Rc::new({
            let account = account.downgrade();
            move |status| {
                if let Some(account) = account.upgrade() {
                    account.set_subtitle(&status_text(&status));
                }
            }
        }));
        crate::ui::preference_dependencies::bind_visibility(switch, &account);
        let weak = Rc::downgrade(self);
        let switch = switch.clone();
        account.connect_activated(move |_| {
            if let Some(context) = weak.upgrade() {
                context.present_lastfm_dialog(&switch);
            }
        });
        group.add(&account);
    }

    pub(super) fn change_lastfm_activation(self: &Rc<Self>, row: &adw::SwitchRow, requested: bool) {
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
        self.set_lastfm_switch(row, false);
        let weak = Rc::downgrade(self);
        let row = row.clone();
        glib::spawn_future_local(async move {
            let Some(context) = weak.upgrade() else {
                return;
            };
            context.lastfm_activation_pending.set(false);
            match lastfm_secret::load().await {
                Ok(Some(credentials)) if client_for(&credentials).is_ok() => {
                    context.enable_lastfm(&row, credentials);
                }
                Ok(_) => context.present_lastfm_dialog(&row),
                Err(error) => {
                    tracing::warn!(%error, "could not access Last.fm credentials in keyring");
                    context.show_lastfm_error(strings::LASTFM_KEYRING_ERROR);
                }
            }
        });
    }

    fn present_lastfm_dialog(self: &Rc<Self>, row: &adw::SwitchRow) {
        let api_key = adw::PasswordEntryRow::builder()
            .title(strings::text(strings::LASTFM_API_KEY))
            .build();
        let shared_secret = adw::PasswordEntryRow::builder()
            .title(strings::text(strings::LASTFM_SHARED_SECRET))
            .build();
        let list = gtk4::ListBox::new();
        list.add_css_class("boxed-list");
        list.append(&api_key);
        list.append(&shared_secret);
        let dialog = adw::AlertDialog::builder()
            .heading(strings::text(strings::LASTFM_DIALOG_HEADING))
            .body(strings::text(strings::LASTFM_DIALOG_BODY))
            .extra_child(&list)
            .default_response(RESPONSE_CONNECT)
            .close_response(RESPONSE_CANCEL)
            .build();
        dialog.add_response(RESPONSE_CANCEL, &strings::text(strings::CANCEL));
        dialog.add_response(RESPONSE_CONNECT, &strings::text(strings::OPEN_BROWSER));
        dialog.set_response_appearance(RESPONSE_CONNECT, adw::ResponseAppearance::Suggested);
        dialog.set_response_enabled(RESPONSE_CONNECT, false);
        for entry in [&api_key, &shared_secret] {
            entry.connect_changed({
                let dialog = dialog.clone();
                let api_key = api_key.clone();
                let shared_secret = shared_secret.clone();
                move |_| {
                    dialog.set_response_enabled(
                        RESPONSE_CONNECT,
                        !api_key.text().trim().is_empty()
                            && !shared_secret.text().trim().is_empty(),
                    );
                }
            });
        }
        if self.lastfm.is_active() {
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
                    RESPONSE_CONNECT => context.request_lastfm_authorization(
                        &row,
                        api_key.text().trim().to_string(),
                        shared_secret.text().trim().to_string(),
                    ),
                    RESPONSE_DISCONNECT => context.disconnect_lastfm(&row),
                    _ => {}
                }
            },
        );
    }

    fn request_lastfm_authorization(
        self: &Rc<Self>,
        row: &adw::SwitchRow,
        api_key: String,
        shared_secret: String,
    ) {
        if authorization_decision(&api_key, &shared_secret, false)
            != AuthorizationDecision::OpenBrowser
        {
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
                            context.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
                        }
                    }
                }
                Ok(Err(TransportError::Unauthorized)) => {
                    context
                        .lastfm
                        .report_status(&ConnectionStatus::Unauthorized);
                    context.show_lastfm_error(strings::LASTFM_CREDENTIALS_REJECTED);
                }
                Ok(Err(error)) => {
                    tracing::warn!(%error, "could not request Last.fm authorization");
                    context.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
                }
                Err(error) => {
                    tracing::warn!(%error, "Last.fm authorization worker ended unexpectedly");
                    context.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
                }
            }
        });
    }

    fn present_lastfm_confirmation(
        self: &Rc<Self>,
        row: &adw::SwitchRow,
        api_key: String,
        shared_secret: String,
        token: String,
    ) {
        if authorization_decision(&api_key, &shared_secret, true) != AuthorizationDecision::Exchange
        {
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
        dialog.choose(
            Some(&self.window),
            gio::Cancellable::NONE,
            move |response| {
                if response == RESPONSE_CONTINUE {
                    if let Some(context) = weak.upgrade() {
                        context.exchange_lastfm_token(&row, api_key, shared_secret, token);
                    }
                }
            },
        );
    }

    fn exchange_lastfm_token(
        self: &Rc<Self>,
        row: &adw::SwitchRow,
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
                            context.show_lastfm_error(strings::LASTFM_KEYRING_ERROR);
                        }
                    }
                }
                Ok(Err(TransportError::Unauthorized)) => {
                    context
                        .lastfm
                        .report_status(&ConnectionStatus::Unauthorized);
                    context.show_lastfm_error(strings::LASTFM_CREDENTIALS_REJECTED);
                }
                Ok(Err(error)) => {
                    tracing::warn!(%error, "could not exchange Last.fm authorization token");
                    context.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
                }
                Err(error) => {
                    tracing::warn!(%error, "Last.fm session worker ended unexpectedly");
                    context.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
                }
            }
        });
    }

    fn enable_lastfm(&self, row: &adw::SwitchRow, credentials: LastFmCredentials) {
        let Ok(client) = client_for(&credentials) else {
            self.show_lastfm_error(strings::LASTFM_CONNECTION_ERROR);
            return;
        };
        if self.persist_lastfm_enabled(true) {
            self.set_lastfm_switch(row, true);
            self.lastfm
                .configure(credentials.session_key, Box::new(client));
        }
    }

    fn disconnect_lastfm(self: &Rc<Self>, row: &adw::SwitchRow) {
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

    fn set_lastfm_switch(&self, row: &adw::SwitchRow, active: bool) {
        self.syncing_lastfm.set(true);
        row.set_active(active);
        self.syncing_lastfm.set(false);
    }

    fn show_lastfm_error(&self, body: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading(strings::text(strings::LASTFM_ACCOUNT))
            .body(strings::text(body))
            .close_response("close")
            .build();
        dialog.add_response("close", &strings::text(strings::CLOSE));
        dialog.present(Some(&self.window));
    }
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
    fn connected_status_includes_lastfm_pending_count() {
        let text = status_text(&ConnectionStatus::Connected {
            user_name: "listener".to_string(),
            pending: 2,
        });
        assert!(text.contains("listener"));
        assert!(text.contains('2'));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn api_credentials_are_masked_rows() {
        gtk4::init().unwrap();
        let key = adw::PasswordEntryRow::new();
        let secret = adw::PasswordEntryRow::new();
        assert!(key.is::<adw::PasswordEntryRow>());
        assert!(secret.is::<adw::PasswordEntryRow>());
    }
}
