//! Concerts plugin preferences and pure location-apply decisions.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;

use crate::ui::concerts::ConcertsRuntime;
use crate::ui::{one_shot_task, strings};

#[derive(Clone, Debug, PartialEq)]
enum LocationDecision {
    Store {
        latitude: f64,
        longitude: f64,
        name: String,
        /// `RAD-5`/`O-4`: only ever set from city search's Nominatim
        /// `addressdetails` — never from a reverse-geocoding call, so the
        /// XDG-portal "Use current location" path always stores `None`.
        country_code: Option<String>,
    },
    Error(String),
}

fn geocode_decision(
    result: Result<Option<reprise_core::concerts::GeocodedLocation>, String>,
) -> LocationDecision {
    match result {
        Ok(Some(location)) => LocationDecision::Store {
            latitude: location.lat,
            longitude: location.lon,
            name: location.display_name,
            country_code: location.country_code,
        },
        Ok(None) => LocationDecision::Error(strings::text(strings::CONCERTS_LOCATION_NOT_FOUND)),
        Err(_) => LocationDecision::Error(strings::text(strings::CONCERTS_LOCATION_NOT_FOUND)),
    }
}

fn portal_decision(
    result: &Result<reprise_platform_linux::location::PortalLocation, String>,
) -> LocationDecision {
    match result {
        Ok(location) => LocationDecision::Store {
            latitude: location.latitude,
            longitude: location.longitude,
            name: strings::text(strings::CONCERTS_CURRENT_LOCATION),
            // The portal returns only coordinates — no address text at
            // all — so there is nothing honest to derive a country from
            // without a new reverse-geocoding call, which `O-4` forbids.
            country_code: None,
        },
        Err(_) => LocationDecision::Error(strings::text(strings::CONCERTS_LOCATION_NOT_FOUND)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CurrentLocationButtonState {
    sensitive: bool,
    show_spinner: bool,
}

fn current_location_button_state(pending: bool) -> CurrentLocationButtonState {
    CurrentLocationButtonState {
        sensitive: !pending,
        show_spinner: pending,
    }
}

fn set_current_location_pending(button: &gtk4::Button, pending: bool) {
    let state = current_location_button_state(pending);
    button.set_sensitive(state.sensitive);
    if state.show_spinner {
        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let spinner = gtk4::Spinner::new();
        spinner.start();
        content.append(&spinner);
        content.append(&gtk4::Label::new(Some(&strings::text(
            strings::CONCERTS_USE_CURRENT_LOCATION,
        ))));
        button.set_child(Some(&content));
    } else {
        button.set_label(&strings::text(strings::CONCERTS_USE_CURRENT_LOCATION));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CredentialApplyDecision {
    Reset,
    Verify,
    CouldNotVerify,
}

fn credential_apply_decision(credential: &str, saved: bool) -> CredentialApplyDecision {
    if credential.trim().is_empty() {
        return CredentialApplyDecision::Reset;
    }
    if !saved {
        return CredentialApplyDecision::CouldNotVerify;
    }
    CredentialApplyDecision::Verify
}

fn credential_feedback_message(
    verification: reprise_core::concerts::CredentialVerification,
) -> Option<&'static str> {
    match verification {
        reprise_core::concerts::CredentialVerification::Empty => None,
        reprise_core::concerts::CredentialVerification::Valid => {
            Some(strings::CONCERTS_CREDENTIAL_VALID)
        }
        reprise_core::concerts::CredentialVerification::Rejected => {
            Some(strings::CONCERTS_CREDENTIAL_REJECTED)
        }
        reprise_core::concerts::CredentialVerification::CouldNotVerify => {
            Some(strings::CONCERTS_CREDENTIAL_UNVERIFIED)
        }
    }
}

fn apply_credential_feedback(
    status: &gtk4::Label,
    verification: reprise_core::concerts::CredentialVerification,
) {
    let Some(message) = credential_feedback_message(verification) else {
        status.set_visible(false);
        return;
    };
    status.set_label(&strings::text(message));
    status.set_visible(true);
}

#[derive(Clone)]
struct CredentialPreferenceRow {
    row: adw::PasswordEntryRow,
    #[cfg(test)]
    status: gtk4::Label,
}

#[derive(Clone, Copy)]
struct CredentialPreferenceSpec {
    provider: reprise_core::concerts::ProviderKind,
    key: &'static str,
    title: &'static str,
}

fn credential_preference_specs() -> [CredentialPreferenceSpec; 1] {
    [CredentialPreferenceSpec {
        provider: reprise_core::concerts::ProviderKind::Bandsintown,
        key: reprise_core::concerts::config::BANDSINTOWN_APP_ID_KEY,
        title: strings::CONCERTS_BANDSINTOWN_APP_ID,
    }]
}

struct ConcertPreferenceRowsInner {
    rows: Vec<gtk4::Widget>,
    #[cfg(test)]
    credentials: Vec<CredentialPreferenceRow>,
    similar_enabled: adw::SwitchRow,
    similar_count: adw::SpinRow,
    module_enabled: Cell<bool>,
}

#[derive(Clone)]
pub(in crate::ui) struct ConcertPreferenceRows {
    inner: Rc<ConcertPreferenceRowsInner>,
}

impl ConcertPreferenceRows {
    pub(in crate::ui) fn add_to(&self, group: &adw::PreferencesGroup) {
        for row in &self.inner.rows {
            group.add(row);
        }
    }

    pub(in crate::ui) fn set_sensitive(&self, enabled: bool) {
        self.inner.module_enabled.set(enabled);
        for row in &self.inner.rows {
            row.set_sensitive(enabled);
        }
        self.inner
            .similar_count
            .set_sensitive(enabled && self.inner.similar_enabled.is_active());
    }
}

pub(in crate::ui) fn build_page(
    conn: &Rc<Db>,
    runtime: &Rc<ConcertsRuntime>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(strings::text(strings::CONCERTS))
        .icon_name("x-office-calendar-symbolic")
        .build();
    let group = adw::PreferencesGroup::new();
    let rows = build(conn, runtime, runtime.enabled.get());
    rows.add_to(&group);
    page.add(&group);

    let alive = page.downgrade();
    runtime.subscribe_enabled(
        move || alive.upgrade().is_some(),
        move |enabled| rows.set_sensitive(enabled),
    );
    page
}

pub(in crate::ui) fn build(
    conn: &Rc<Db>,
    runtime: &Rc<ConcertsRuntime>,
    enabled: bool,
) -> ConcertPreferenceRows {
    let credentials = credential_preference_specs()
        .into_iter()
        .map(|spec| password_row(conn, runtime, spec.provider, spec.key, spec.title))
        .collect::<Vec<_>>();
    let (city, location_status) = location_rows(conn, runtime);
    let radius = radius_row(conn, runtime);
    let window_days = window_days_row(conn, runtime);
    let similar = reprise_core::concerts::config::similar_config(conn).unwrap_or(
        reprise_core::concerts::config::SimilarConfig {
            enabled: false,
            count: 10,
        },
    );
    let similar_enabled = adw::SwitchRow::builder()
        .title(strings::text(strings::CONCERTS_SIMILAR_ENABLED))
        .active(similar.enabled)
        .build();
    {
        let conn = conn.clone();
        let runtime = runtime.clone();
        similar_enabled.connect_active_notify(move |row| {
            if save_setting(
                &conn,
                reprise_core::concerts::config::SIMILAR_ENABLED_KEY,
                if row.is_active() { "1" } else { "0" },
            ) {
                runtime.notify_settings_changed();
            }
        });
    }
    let similar_count = adw::SpinRow::with_range(1.0, 25.0, 1.0);
    similar_count.set_title(&strings::text(strings::CONCERTS_SIMILAR_COUNT));
    similar_count.set_value(similar.count as f64);
    {
        let conn = conn.clone();
        let runtime = runtime.clone();
        similar_count.connect_value_notify(move |row| {
            if save_setting(
                &conn,
                reprise_core::concerts::config::SIMILAR_COUNT_KEY,
                &row.value().round().to_string(),
            ) {
                runtime.notify_settings_changed();
            }
        });
    }
    let mut rows = credentials
        .iter()
        .map(|credential| credential.row.clone().upcast())
        .collect::<Vec<_>>();
    rows.extend([
        city.upcast(),
        location_status.upcast(),
        radius.upcast(),
        window_days.upcast(),
        similar_enabled.clone().upcast(),
        similar_count.clone().upcast(),
    ]);
    let preferences = ConcertPreferenceRows {
        inner: Rc::new(ConcertPreferenceRowsInner {
            rows,
            #[cfg(test)]
            credentials,
            similar_enabled: similar_enabled.clone(),
            similar_count: similar_count.clone(),
            module_enabled: Cell::new(enabled),
        }),
    };
    {
        let preferences = preferences.clone();
        similar_enabled.connect_active_notify(move |row| {
            preferences
                .inner
                .similar_count
                .set_sensitive(preferences.inner.module_enabled.get() && row.is_active());
        });
    }
    preferences.set_sensitive(enabled);
    preferences
}

fn password_row(
    conn: &Rc<Db>,
    runtime: &Rc<ConcertsRuntime>,
    provider: reprise_core::concerts::ProviderKind,
    key: &'static str,
    title: &'static str,
) -> CredentialPreferenceRow {
    let value = reprise_core::library::settings::get_setting(conn, key)
        .ok()
        .flatten()
        .unwrap_or_default();
    let row = adw::PasswordEntryRow::builder()
        .title(strings::text(title))
        .text(value)
        .show_apply_button(true)
        .build();
    let status = gtk4::Label::builder()
        .accessible_role(gtk4::AccessibleRole::Status)
        .css_classes(["caption", "dim-label"])
        .visible(false)
        .build();
    row.add_suffix(&status);
    let generation = Rc::new(Cell::new(0_u64));
    {
        let conn = conn.clone();
        let runtime = runtime.clone();
        let status = status.clone();
        let generation = generation.clone();
        row.connect_changed(move |row| {
            generation.set(generation.get().wrapping_add(1));
            status.set_visible(false);
            if save_setting(&conn, key, row.text().as_str()) {
                runtime.notify_settings_changed();
            }
        });
    }
    {
        let conn = conn.clone();
        let runtime = runtime.clone();
        let status = status.clone();
        let generation = generation.clone();
        row.connect_apply(move |row| {
            let credential = row.text().trim().to_owned();
            let saved = save_setting(&conn, key, &credential);
            if saved {
                runtime.notify_settings_changed();
            }
            match credential_apply_decision(&credential, saved) {
                CredentialApplyDecision::Reset => {
                    generation.set(generation.get().wrapping_add(1));
                    apply_credential_feedback(
                        &status,
                        reprise_core::concerts::CredentialVerification::Empty,
                    );
                }
                CredentialApplyDecision::CouldNotVerify => {
                    generation.set(generation.get().wrapping_add(1));
                    apply_credential_feedback(
                        &status,
                        reprise_core::concerts::CredentialVerification::CouldNotVerify,
                    );
                }
                CredentialApplyDecision::Verify => {
                    start_credential_verification(&status, &generation, provider, credential);
                }
            }
        });
    }
    CredentialPreferenceRow {
        row,
        #[cfg(test)]
        status,
    }
}

fn start_credential_verification(
    status: &gtk4::Label,
    generation: &Rc<Cell<u64>>,
    provider: reprise_core::concerts::ProviderKind,
    credential: String,
) {
    let current = generation.get().wrapping_add(1);
    generation.set(current);
    status.set_label(&strings::text(strings::CONCERTS_CREDENTIAL_CHECKING));
    status.set_visible(true);
    let receiver = match one_shot_task::spawn("reprise-concert-credential", move || {
        reprise_core::concerts::verify_credential(provider, &credential)
    }) {
        Ok(receiver) => receiver,
        Err(_) => {
            apply_credential_feedback(
                status,
                reprise_core::concerts::CredentialVerification::CouldNotVerify,
            );
            return;
        }
    };
    let status = status.downgrade();
    let generation = generation.clone();
    glib::spawn_future_local(async move {
        let verification = receiver
            .recv()
            .await
            .unwrap_or(reprise_core::concerts::CredentialVerification::CouldNotVerify);
        if generation.get() != current {
            return;
        }
        if let Some(status) = status.upgrade() {
            apply_credential_feedback(&status, verification);
        }
    });
}

fn location_rows(conn: &Rc<Db>, runtime: &Rc<ConcertsRuntime>) -> (adw::EntryRow, adw::ActionRow) {
    let stored = reprise_core::concerts::config::location(conn)
        .ok()
        .flatten();
    let city = adw::EntryRow::builder()
        .title(strings::text(strings::CONCERTS_CITY_ENTRY))
        .text(
            stored
                .as_ref()
                .map_or("", |location| location.name.as_str()),
        )
        .show_apply_button(true)
        .build();
    let current = gtk4::Button::builder()
        .label(strings::text(strings::CONCERTS_USE_CURRENT_LOCATION))
        .valign(gtk4::Align::Center)
        .build();
    let clear = gtk4::Button::builder()
        .label(strings::text(strings::CONCERTS_CLEAR_LOCATION))
        .valign(gtk4::Align::Center)
        .build();
    city.add_suffix(&current);
    city.add_suffix(&clear);
    let status = adw::ActionRow::builder().visible(false).build();
    let current_pending = Rc::new(Cell::new(false));

    {
        let conn = conn.clone();
        let runtime = runtime.clone();
        let status = status.clone();
        city.connect_apply(move |row| {
            let query = row.text().trim().to_owned();
            if query.is_empty() {
                apply_location(
                    &conn,
                    &runtime,
                    &status,
                    LocationDecision::Error(strings::text(strings::CONCERTS_LOCATION_NOT_FOUND)),
                );
                return;
            }
            let receiver = one_shot_task::spawn("reprise-geocode", move || {
                geocode_decision(
                    reprise_core::concerts::geocode(&query).map_err(|error| error.to_string()),
                )
            });
            receive_location(
                receiver,
                conn.clone(),
                runtime.clone(),
                status.clone(),
                None,
            );
        });
    }
    {
        let conn = conn.clone();
        let runtime = runtime.clone();
        let status = status.clone();
        let pending = current_pending.clone();
        current.connect_clicked(move |button| {
            if pending.replace(true) {
                return;
            }
            set_current_location_pending(button, true);
            let receiver = one_shot_task::spawn("reprise-location", || {
                portal_decision(&reprise_platform_linux::location::current_location(
                    reprise_platform_linux::location::DEFAULT_TIMEOUT,
                ))
            });
            let button = button.clone();
            let pending = pending.clone();
            receive_location(
                receiver,
                conn.clone(),
                runtime.clone(),
                status.clone(),
                Some(Box::new(move || {
                    pending.set(false);
                    set_current_location_pending(&button, false);
                })),
            );
        });
    }
    {
        let conn = conn.clone();
        let runtime = runtime.clone();
        let status = status.clone();
        let city = city.clone();
        clear.connect_clicked(move |_| {
            clear_location(&conn, &runtime);
            city.set_text("");
            status.set_visible(false);
        });
    }
    (city, status)
}

fn receive_location(
    receiver: std::io::Result<async_channel::Receiver<LocationDecision>>,
    conn: Rc<Db>,
    runtime: Rc<ConcertsRuntime>,
    status: adw::ActionRow,
    on_complete: Option<Box<dyn FnOnce()>>,
) {
    let Ok(receiver) = receiver else {
        apply_location(
            &conn,
            &runtime,
            &status,
            LocationDecision::Error(strings::text(strings::CONCERTS_LOCATION_NOT_FOUND)),
        );
        if let Some(on_complete) = on_complete {
            on_complete();
        }
        return;
    };
    glib::spawn_future_local(async move {
        let decision = receiver.recv().await.unwrap_or_else(|_| {
            LocationDecision::Error(strings::text(strings::CONCERTS_LOCATION_NOT_FOUND))
        });
        apply_location(&conn, &runtime, &status, decision);
        if let Some(on_complete) = on_complete {
            on_complete();
        }
    });
}

fn apply_location(
    conn: &Rc<Db>,
    runtime: &ConcertsRuntime,
    status: &adw::ActionRow,
    decision: LocationDecision,
) {
    match decision {
        LocationDecision::Store {
            latitude,
            longitude,
            name,
            country_code,
        } => {
            // `O-4`: this is the single write path for the app-level,
            // consented location — Radio's "Near you" chip (`RAD-5`) reads
            // straight through `reprise_core::location`, not a copy.
            let saved = reprise_core::location::store(
                conn,
                latitude,
                longitude,
                &name,
                country_code.as_deref(),
            );
            match saved {
                Ok(()) => runtime.notify_settings_changed(),
                Err(error) => tracing::warn!(%error, "could not save Concerts location"),
            }
            status.set_subtitle(&name);
            status.set_visible(true);
        }
        LocationDecision::Error(error) => {
            status.set_subtitle(&error);
            status.set_visible(true);
        }
    }
}

fn clear_location(conn: &Rc<Db>, runtime: &ConcertsRuntime) {
    match reprise_core::location::clear(conn) {
        Ok(()) => runtime.notify_settings_changed(),
        Err(error) => tracing::warn!(%error, "could not clear Concerts location"),
    }
}

fn radius_row(conn: &Rc<Db>, runtime: &Rc<ConcertsRuntime>) -> adw::ComboRow {
    let radii = std::iter::once(None)
        .chain(
            reprise_core::concerts::config::RADIUS_PRESETS_KM
                .into_iter()
                .map(Some),
        )
        .collect::<Vec<_>>();
    let labels = radii
        .iter()
        .map(|radius| {
            radius.map_or_else(
                || strings::text(strings::CONCERTS_OFF),
                strings::concerts_radius_km,
            )
        })
        .collect::<Vec<_>>();
    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    let model = gtk4::StringList::new(&label_refs);
    let stored = match reprise_core::library::settings::get_setting(
        conn,
        reprise_core::concerts::config::DEFAULT_RADIUS_KEY,
    )
    .ok()
    .flatten()
    {
        Some(value) => value.parse::<u32>().ok(),
        None => Some(reprise_core::concerts::config::DEFAULT_RADIUS_KM as u32),
    };
    let selected = radii
        .iter()
        .position(|radius| *radius == stored)
        .unwrap_or_default() as u32;
    let row = adw::ComboRow::builder()
        .title(strings::text(strings::CONCERTS_DEFAULT_RADIUS))
        .model(&model)
        .selected(selected)
        .build();
    let conn = conn.clone();
    let runtime = runtime.clone();
    row.connect_selected_notify(move |row| {
        let value = radii
            .get(row.selected() as usize)
            .copied()
            .flatten()
            .map_or_else(String::new, |radius| radius.to_string());
        if save_setting(
            &conn,
            reprise_core::concerts::config::DEFAULT_RADIUS_KEY,
            &value,
        ) {
            runtime.notify_settings_changed();
        }
    });
    row
}

fn window_days_row(conn: &Rc<Db>, runtime: &Rc<ConcertsRuntime>) -> adw::SpinRow {
    let row = adw::SpinRow::with_range(30.0, 365.0, 1.0);
    row.set_title(&strings::text(strings::CONCERTS_PLAY_WINDOW));
    row.set_value(reprise_core::concerts::config::window_days(conn).unwrap_or(90) as f64);
    let conn = conn.clone();
    let runtime = runtime.clone();
    row.connect_value_notify(move |row| {
        if save_setting(
            &conn,
            reprise_core::concerts::config::WINDOW_DAYS_KEY,
            &row.value().round().to_string(),
        ) {
            runtime.notify_settings_changed();
        }
    });
    row
}

fn save_setting(conn: &Rc<Db>, key: &str, value: &str) -> bool {
    if let Err(error) = reprise_core::library::settings::set_setting(conn, key, value) {
        tracing::warn!(%error, setting = key, "could not save Concerts preference");
        return false;
    }
    true
}

#[cfg(test)]
#[path = "preference_concerts_tests.rs"]
mod tests;
