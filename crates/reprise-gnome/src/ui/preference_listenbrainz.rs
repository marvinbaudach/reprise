//! ListenBrainz-specific preferences, secure activation, and startup bootstrap.

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

struct ListenBrainzPageSurface {
    page: adw::NavigationPage,
    token: adw::PasswordEntryRow,
    connect: gtk4::Button,
    disconnect: Option<gtk4::Button>,
}

fn build_listenbrainz_page(connected: bool) -> ListenBrainzPageSurface {
    let account = adw::PreferencesGroup::builder()
        .description(strings::text(strings::LISTENBRAINZ_DIALOG_BODY))
        .build();
    let token = adw::PasswordEntryRow::builder()
        .title(strings::text(strings::LISTENBRAINZ_TOKEN))
        .build();
    account.add(&token);

    let content = adw::PreferencesPage::new();
    content.add(&account);
    let disconnect = connected.then(|| {
        let actions = adw::PreferencesGroup::new();
        let row = adw::ActionRow::builder()
            .title(strings::text(strings::LISTENBRAINZ))
            .build();
        let button = gtk4::Button::builder()
            .label(strings::text(strings::LISTENBRAINZ_DISCONNECT))
            .valign(gtk4::Align::Center)
            .build();
        button.add_css_class("destructive-action");
        row.add_suffix(&button);
        actions.add(&row);
        content.add(&actions);
        button
    });

    let connect = gtk4::Button::with_label(&strings::text(strings::LISTENBRAINZ_CONNECT));
    connect.add_css_class("suggested-action");
    connect.set_sensitive(false);
    token.connect_changed({
        let connect = connect.clone();
        move |token| connect.set_sensitive(!token.text().trim().is_empty())
    });
    let header = adw::HeaderBar::new();
    header.pack_end(&connect);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    let title = strings::text(strings::LISTENBRAINZ_ACCOUNT);
    let page = adw::NavigationPage::with_tag(&toolbar, &title, "listenbrainz-account");
    ListenBrainzPageSurface {
        page,
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
    pub(super) fn add_listenbrainz_controls(self: &Rc<Self>, switch: &adw::SwitchRow) {
        let description = switch.subtitle().unwrap_or_default().to_string();
        switch.set_subtitle(&crate::ui::preference_dependencies::service_subtitle(
            &description,
            switch.is_active(),
            &status_text(&self.listenbrainz.status()),
        ));
        self.listenbrainz.subscribe(Rc::new({
            let switch = switch.downgrade();
            let description = description.clone();
            move |status| {
                if let Some(switch) = switch.upgrade() {
                    switch.set_subtitle(&crate::ui::preference_dependencies::service_subtitle(
                        &description,
                        switch.is_active(),
                        &status_text(&status),
                    ));
                }
            }
        }));
        let runtime = self.listenbrainz.clone();
        let description_for_toggle = description.clone();
        switch.connect_active_notify(move |switch| {
            switch.set_subtitle(&crate::ui::preference_dependencies::service_subtitle(
                &description_for_toggle,
                switch.is_active(),
                &status_text(&runtime.status()),
            ));
        });
        let configure = crate::ui::preference_dependencies::add_configure_button(
            switch,
            &strings::text(strings::CONFIGURE),
        );
        let weak = Rc::downgrade(self);
        let switch = switch.downgrade();
        configure.connect_clicked(move |_| {
            let (Some(context), Some(switch)) = (weak.upgrade(), switch.upgrade()) else {
                return;
            };
            context.push_listenbrainz_page(&switch);
        });
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

        crate::ui::preference_dependencies::set_activation_pending(row, true);
        let weak = Rc::downgrade(self);
        let row = row.clone();
        glib::spawn_future_local(async move {
            let result = listenbrainz_secret::load().await;
            let Some(context) = weak.upgrade() else {
                return;
            };
            context.listenbrainz_activation_pending.set(false);
            crate::ui::preference_dependencies::set_activation_pending(&row, false);
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
                    _ => context.push_listenbrainz_page(&row),
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

    fn push_listenbrainz_page(self: &Rc<Self>, row: &adw::SwitchRow) {
        let Some(navigation) = self.preferences_navigation() else {
            tracing::warn!("ListenBrainz setup requested without preferences navigation");
            self.restore_listenbrainz_switch(row);
            return;
        };
        let surface = build_listenbrainz_page(self.listenbrainz.is_active());
        let weak = Rc::downgrade(self);
        let hiding_row = row.clone();
        surface.page.connect_hiding(move |_| {
            if let Some(context) = weak.upgrade() {
                context.restore_listenbrainz_switch(&hiding_row);
            }
        });
        let weak = Rc::downgrade(self);
        let connect_row = row.clone();
        let token = surface.token.clone();
        surface.connect.connect_clicked(move |_| {
            if let Some(context) = weak.upgrade() {
                context
                    .validate_and_save_listenbrainz(&connect_row, token.text().trim().to_string());
            }
        });
        if let Some(disconnect) = surface.disconnect {
            let weak = Rc::downgrade(self);
            let disconnect_row = row.clone();
            let navigation = navigation.downgrade();
            disconnect.connect_clicked(move |_| {
                let (Some(context), Some(navigation)) = (weak.upgrade(), navigation.upgrade())
                else {
                    return;
                };
                context.disconnect_listenbrainz(&disconnect_row);
                navigation.pop();
            });
        }
        navigation.push(&surface.page);
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

    fn enable_listenbrainz(&self, row: &adw::SwitchRow, token: String) {
        if self.persist_listenbrainz_enabled(true) {
            self.set_listenbrainz_switch(row, true);
            self.listenbrainz
                .configure(token, Box::new(ListenBrainzClient::new()));
        } else {
            self.restore_listenbrainz_switch(row);
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

    fn set_listenbrainz_switch(&self, row: &adw::SwitchRow, active: bool) {
        self.syncing_listenbrainz.set(true);
        row.set_active(active);
        self.syncing_listenbrainz.set(false);
    }

    fn restore_listenbrainz_switch(&self, row: &adw::SwitchRow) {
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
    fn setup_page_keeps_the_token_masked_and_gates_connect() {
        gtk4::init().unwrap();
        let surface = build_listenbrainz_page(false);
        assert_eq!(surface.page.title(), "ListenBrainz Account");
        assert!(surface.page.can_pop());
        assert!(surface.token.is::<adw::PasswordEntryRow>());
        assert!(!surface.connect.is_sensitive());
        assert!(surface.disconnect.is_none());

        surface.token.set_text("token");
        assert!(surface.connect.is_sensitive());
        surface.token.set_text("  ");
        assert!(!surface.connect.is_sensitive());
        assert!(build_listenbrainz_page(true).disconnect.is_some());

        let root =
            adw::NavigationPage::new(&gtk4::Box::new(gtk4::Orientation::Vertical, 0), "Plugins");
        let navigation = adw::NavigationView::new();
        navigation.add(&root);
        navigation.push(&surface.page);
        assert_eq!(navigation.visible_page().as_ref(), Some(&surface.page));
        assert!(navigation.pop());
        assert_eq!(navigation.visible_page().as_ref(), Some(&root));
    }
}
