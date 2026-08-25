//! Concerts plugin preferences and pure location-apply decisions.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;

use crate::ui::concerts::ConcertsRuntime;
use crate::ui::location_broadcast::LocationBroadcast;
use crate::ui::strings;

pub(in crate::ui) const LOCATION_REFERENCE_CLASS: &str = "reprise-location-reference";

type OnLocation = Rc<dyn Fn()>;

fn location_reference_copy(
    location: Option<&reprise_core::location::AppLocation>,
    radius_km: u32,
) -> (String, String) {
    match location {
        Some(location) => {
            let name = strings::concerts_location_name(&location.name, location.country.as_deref());
            (
                strings::location_reference(&name, radius_km),
                strings::text(strings::LOCATION_CHANGE_IN_LOCATION),
            )
        }
        None => (
            strings::text(strings::LOCATION_REFERENCE_NOT_SET),
            strings::text(strings::LOCATION_SET_LOCATION),
        ),
    }
}

fn refresh_location_reference(conn: &Db, row: &adw::ActionRow, action: &gtk4::Label) {
    let location = reprise_core::location::app_location(conn).ok().flatten();
    let radius = reprise_core::location::default_radius_km(conn)
        .unwrap_or(reprise_core::location::DEFAULT_RADIUS_KM)
        .round() as u32;
    let (title, action_text) = location_reference_copy(location.as_ref(), radius);
    row.set_title(&title);
    action.set_label(&action_text);
}

fn location_reference_row(
    conn: &Rc<Db>,
    broadcast: &Rc<LocationBroadcast>,
    on_location: &OnLocation,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder().activatable(true).build();
    row.set_sensitive(true);
    row.add_css_class(LOCATION_REFERENCE_CLASS);
    row.add_css_class("dim-label");
    let pin = gtk4::Image::from_icon_name("find-location-symbolic");
    pin.set_accessible_role(gtk4::AccessibleRole::Presentation);
    row.add_prefix(&pin);
    let action = gtk4::Label::new(None);
    action.add_css_class("caption");
    row.add_suffix(&action);
    refresh_location_reference(conn, &row, &action);
    {
        let on_location = on_location.clone();
        row.connect_activated(move |_| on_location());
    }
    {
        let alive = row.downgrade();
        let target = alive.clone();
        let action = action.downgrade();
        let conn = conn.clone();
        broadcast.subscribe(
            move || alive.upgrade().is_some(),
            move || {
                let (Some(row), Some(action)) = (target.upgrade(), action.upgrade()) else {
                    return;
                };
                refresh_location_reference(&conn, &row, &action);
            },
        );
    }
    row
}

struct ConcertPreferenceRowsInner {
    rows: Vec<gtk4::Widget>,
    module_rows: Vec<gtk4::Widget>,
    #[cfg(test)]
    location_reference: adw::ActionRow,
    similar_enabled: adw::SwitchRow,
    similar_count: adw::SpinRow,
    module_enabled: Cell<bool>,
}

#[derive(Clone)]
pub(in crate::ui) struct ConcertPreferenceRows {
    inner: Rc<ConcertPreferenceRowsInner>,
}

impl ConcertPreferenceRows {
    pub(in crate::ui) fn add_to(&self, expander: &adw::ExpanderRow) {
        for row in &self.inner.rows {
            super::preference_plugin_chrome::add_nested_row(expander, row);
        }
    }

    pub(in crate::ui) fn set_sensitive(&self, enabled: bool) {
        self.inner.module_enabled.set(enabled);
        for row in &self.inner.module_rows {
            row.set_sensitive(enabled);
        }
        self.inner
            .similar_count
            .set_sensitive(enabled && self.inner.similar_enabled.is_active());
    }
}

pub(in crate::ui) fn build(
    conn: &Rc<Db>,
    runtime: &Rc<ConcertsRuntime>,
    broadcast: &Rc<LocationBroadcast>,
    on_location: &OnLocation,
    enabled: bool,
) -> ConcertPreferenceRows {
    let location_reference = location_reference_row(conn, broadcast, on_location);
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
    let module_rows = vec![
        window_days.upcast(),
        similar_enabled.clone().upcast(),
        similar_count.clone().upcast(),
    ];
    let mut rows = vec![location_reference.clone().upcast()];
    rows.extend(module_rows.iter().cloned());
    let preferences = ConcertPreferenceRows {
        inner: Rc::new(ConcertPreferenceRowsInner {
            rows,
            module_rows,
            #[cfg(test)]
            location_reference,
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
