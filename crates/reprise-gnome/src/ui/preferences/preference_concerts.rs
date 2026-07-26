//! Concerts plugin preferences and pure location-apply decisions.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use crate::ui::{one_shot_task, strings};

#[derive(Clone, Debug, PartialEq)]
enum LocationDecision {
    Store {
        latitude: f64,
        longitude: f64,
        name: String,
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

struct ConcertPreferenceRowsInner {
    rows: Vec<gtk4::Widget>,
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

pub(in crate::ui) fn build(conn: &Rc<RefCell<Connection>>, enabled: bool) -> ConcertPreferenceRows {
    let bandsintown = password_row(
        conn,
        reprise_core::concerts::config::BANDSINTOWN_APP_ID_KEY,
        strings::CONCERTS_BANDSINTOWN_APP_ID,
    );
    let ticketmaster = password_row(
        conn,
        reprise_core::concerts::config::TICKETMASTER_API_KEY,
        strings::CONCERTS_TICKETMASTER_API_KEY,
    );
    let (city, location_status) = location_rows(conn);
    let radius = radius_row(conn);
    let window_days = window_days_row(conn);
    let similar = reprise_core::concerts::config::similar_config(&conn.borrow()).unwrap_or(
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
        similar_enabled.connect_active_notify(move |row| {
            save_setting(
                &conn,
                reprise_core::concerts::config::SIMILAR_ENABLED_KEY,
                if row.is_active() { "1" } else { "0" },
            );
        });
    }
    let similar_count = adw::SpinRow::with_range(1.0, 25.0, 1.0);
    similar_count.set_title(&strings::text(strings::CONCERTS_SIMILAR_COUNT));
    similar_count.set_value(similar.count as f64);
    {
        let conn = conn.clone();
        similar_count.connect_value_notify(move |row| {
            save_setting(
                &conn,
                reprise_core::concerts::config::SIMILAR_COUNT_KEY,
                &row.value().round().to_string(),
            );
        });
    }
    let rows = vec![
        bandsintown.upcast(),
        ticketmaster.upcast(),
        city.upcast(),
        location_status.upcast(),
        radius.upcast(),
        window_days.upcast(),
        similar_enabled.clone().upcast(),
        similar_count.clone().upcast(),
    ];
    let preferences = ConcertPreferenceRows {
        inner: Rc::new(ConcertPreferenceRowsInner {
            rows,
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
    conn: &Rc<RefCell<Connection>>,
    key: &'static str,
    title: &'static str,
) -> adw::PasswordEntryRow {
    let value = reprise_core::library::settings::get_setting(&conn.borrow(), key)
        .ok()
        .flatten()
        .unwrap_or_default();
    let row = adw::PasswordEntryRow::builder()
        .title(strings::text(title))
        .text(value)
        .build();
    let conn = conn.clone();
    row.connect_changed(move |row| save_setting(&conn, key, row.text().as_str()));
    row
}

fn location_rows(conn: &Rc<RefCell<Connection>>) -> (adw::EntryRow, adw::ActionRow) {
    let stored = reprise_core::concerts::config::location(&conn.borrow())
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
        let status = status.clone();
        city.connect_apply(move |row| {
            let query = row.text().trim().to_owned();
            if query.is_empty() {
                apply_location(
                    &conn,
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
            receive_location(receiver, conn.clone(), status.clone(), None);
        });
    }
    {
        let conn = conn.clone();
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
        let status = status.clone();
        let city = city.clone();
        clear.connect_clicked(move |_| {
            clear_location(&conn);
            city.set_text("");
            status.set_visible(false);
        });
    }
    (city, status)
}

fn receive_location(
    receiver: std::io::Result<async_channel::Receiver<LocationDecision>>,
    conn: Rc<RefCell<Connection>>,
    status: adw::ActionRow,
    on_complete: Option<Box<dyn FnOnce()>>,
) {
    let Ok(receiver) = receiver else {
        apply_location(
            &conn,
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
        apply_location(&conn, &status, decision);
        if let Some(on_complete) = on_complete {
            on_complete();
        }
    });
}

fn apply_location(
    conn: &Rc<RefCell<Connection>>,
    status: &adw::ActionRow,
    decision: LocationDecision,
) {
    match decision {
        LocationDecision::Store {
            latitude,
            longitude,
            name,
        } => {
            for (key, value) in [
                (
                    reprise_core::concerts::config::LOCATION_LAT_KEY,
                    latitude.to_string(),
                ),
                (
                    reprise_core::concerts::config::LOCATION_LON_KEY,
                    longitude.to_string(),
                ),
                (
                    reprise_core::concerts::config::LOCATION_NAME_KEY,
                    name.clone(),
                ),
            ] {
                save_setting(conn, key, &value);
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

fn clear_location(conn: &Rc<RefCell<Connection>>) {
    for key in [
        reprise_core::concerts::config::LOCATION_LAT_KEY,
        reprise_core::concerts::config::LOCATION_LON_KEY,
        reprise_core::concerts::config::LOCATION_NAME_KEY,
    ] {
        save_setting(conn, key, "");
    }
}

fn radius_row(conn: &Rc<RefCell<Connection>>) -> adw::ComboRow {
    let radii = [None, Some(50_u32), Some(100), Some(250), Some(500)];
    let labels = radii.map(|radius| {
        radius.map_or_else(
            || strings::text(strings::CONCERTS_OFF),
            strings::concerts_radius_km,
        )
    });
    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    let model = gtk4::StringList::new(&label_refs);
    let stored = reprise_core::library::settings::get_setting(
        &conn.borrow(),
        reprise_core::concerts::config::DEFAULT_RADIUS_KEY,
    )
    .ok()
    .flatten()
    .and_then(|value| value.parse::<u32>().ok());
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
    row.connect_selected_notify(move |row| {
        let value = radii
            .get(row.selected() as usize)
            .copied()
            .flatten()
            .map_or_else(String::new, |radius| radius.to_string());
        save_setting(
            &conn,
            reprise_core::concerts::config::DEFAULT_RADIUS_KEY,
            &value,
        );
    });
    row
}

fn window_days_row(conn: &Rc<RefCell<Connection>>) -> adw::SpinRow {
    let row = adw::SpinRow::with_range(30.0, 365.0, 1.0);
    row.set_title(&strings::text(strings::CONCERTS_PLAY_WINDOW));
    row.set_value(reprise_core::concerts::config::window_days(&conn.borrow()).unwrap_or(90) as f64);
    let conn = conn.clone();
    row.connect_value_notify(move |row| {
        save_setting(
            &conn,
            reprise_core::concerts::config::WINDOW_DAYS_KEY,
            &row.value().round().to_string(),
        );
    });
    row
}

fn save_setting(conn: &Rc<RefCell<Connection>>, key: &str, value: &str) {
    if let Err(error) = reprise_core::library::settings::set_setting(&conn.borrow(), key, value) {
        tracing::warn!(%error, setting = key, "could not save Concerts preference");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_apply_decisions_store_success_and_keep_errors_visible() {
        assert_eq!(
            geocode_decision(Ok(Some(reprise_core::concerts::GeocodedLocation {
                lat: 48.137,
                lon: 11.575,
                display_name: "Munich, Bavaria".into(),
            }))),
            LocationDecision::Store {
                latitude: 48.137,
                longitude: 11.575,
                name: "Munich, Bavaria".into(),
            }
        );
        assert!(matches!(
            geocode_decision(Ok(None)),
            LocationDecision::Error(_)
        ));
        assert_eq!(
            portal_decision(&Ok(reprise_platform_linux::location::PortalLocation {
                latitude: 47.376,
                longitude: 8.541,
                accuracy_m: Some(1_000.0),
            })),
            LocationDecision::Store {
                latitude: 47.376,
                longitude: 8.541,
                name: crate::ui::strings::text(crate::ui::strings::CONCERTS_CURRENT_LOCATION),
            }
        );
        assert!(matches!(
            portal_decision(&Err("denied".into())),
            LocationDecision::Error(error)
                if error == crate::ui::strings::text(
                    crate::ui::strings::CONCERTS_LOCATION_NOT_FOUND
                )
        ));
    }

    #[test]
    fn current_location_button_is_disabled_with_pending_feedback() {
        assert_eq!(
            current_location_button_state(false),
            CurrentLocationButtonState {
                sensitive: true,
                show_spinner: false,
            }
        );
        assert_eq!(
            current_location_button_state(true),
            CurrentLocationButtonState {
                sensitive: false,
                show_spinner: true,
            }
        );
    }

    #[test]
    fn stored_credentials_are_preferred_and_similar_count_clamps() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        reprise_core::library::settings::set_setting(
            &conn,
            reprise_core::concerts::config::BANDSINTOWN_APP_ID_KEY,
            "stored-app",
        )
        .unwrap();
        reprise_core::library::settings::set_setting(
            &conn,
            reprise_core::concerts::config::SIMILAR_COUNT_KEY,
            "99",
        )
        .unwrap();

        let credentials = reprise_core::concerts::config::credentials(&conn).unwrap();
        let similar = reprise_core::concerts::config::similar_config(&conn).unwrap();

        assert_eq!(
            credentials.bandsintown_app_id.as_deref(),
            Some("stored-app")
        );
        assert_eq!(similar.count, 25);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn concerts_preferences_use_protected_credentials_and_link_similar_sensitivity() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        if gtk4::init().is_err() {
            return;
        }
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let preferences = build(&Rc::new(RefCell::new(conn)), true);

        assert!(
            preferences.inner.rows[0].is::<adw::PasswordEntryRow>()
                && preferences.inner.rows[1].is::<adw::PasswordEntryRow>()
        );
        assert!(!preferences.inner.similar_count.is_sensitive());
        preferences.inner.similar_enabled.set_active(true);
        assert!(preferences.inner.similar_count.is_sensitive());
        preferences.set_sensitive(false);
        assert!(!preferences.inner.similar_count.is_sensitive());
    }
}
