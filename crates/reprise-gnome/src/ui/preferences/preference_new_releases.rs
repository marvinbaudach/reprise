//! New Releases plugin controls beyond the generic enable switch.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;

use crate::ui::artist_news_worker::ArtistNewsRuntime;

use super::strings;

#[derive(Clone)]
pub(in crate::ui) struct NewReleasePreferenceRows {
    rows: Rc<Vec<gtk4::Widget>>,
}

impl NewReleasePreferenceRows {
    pub(in crate::ui) fn add_to(&self, expander: &adw::ExpanderRow) {
        for row in self.rows.iter() {
            super::preference_plugin_chrome::add_nested_row(expander, row);
        }
    }

    pub(in crate::ui) fn set_sensitive(&self, enabled: bool) {
        for row in self.rows.iter() {
            row.set_sensitive(enabled);
        }
    }
}

pub(in crate::ui) fn build(
    conn: &Rc<Db>,
    runtime: &Rc<ArtistNewsRuntime>,
    enabled: bool,
) -> NewReleasePreferenceRows {
    let rows = NewReleasePreferenceRows {
        rows: Rc::new(vec![
            scope_row(conn, enabled).upcast(),
            notification_row(conn, enabled).upcast(),
        ]),
    };
    let alive = rows.rows[0].downgrade();
    let target = rows.clone();
    runtime.subscribe_enabled(
        move || alive.upgrade().is_some(),
        move |enabled| target.set_sensitive(enabled),
    );
    rows
}

pub(in crate::ui) fn scope_row(conn: &Rc<Db>, enabled: bool) -> adw::ComboRow {
    let selected = reprise_core::artist_news::configured_fetch_scope(conn).map_or(0, |scope| {
        u32::from(matches!(
            scope,
            reprise_core::artist_news::FetchScope::AllArtists
        ))
    });
    let model = gtk4::StringList::new(&[
        &strings::text(strings::TOP_ARTISTS_ONLY),
        &strings::text(strings::ALL_ARTISTS),
    ]);
    let row = adw::ComboRow::builder()
        .title(strings::text(strings::NEW_RELEASES_ARTISTS))
        .model(&model)
        .selected(selected)
        .sensitive(enabled)
        .build();
    let conn = conn.clone();
    row.connect_selected_notify(move |row| {
        if let Err(error) =
            reprise_core::artist_news::set_fetch_all_artists(&conn, row.selected() == 1)
        {
            tracing::warn!(%error, "could not save New Releases artist scope");
        }
    });
    row
}

fn notification_row(conn: &Rc<Db>, enabled: bool) -> adw::ComboRow {
    let selected = reprise_core::artist_news_notify::notification_preference(conn)
        .map_or(1, notification_preference_index);
    let model = gtk4::StringList::new(&[
        &strings::text(strings::NOTIFY_UPDATES_OFF),
        &strings::text(strings::NOTIFY_RELEASES_ONLY),
        &strings::text(strings::NOTIFY_ALL_UPDATES),
    ]);
    let row = adw::ComboRow::builder()
        .title(strings::text(strings::NOTIFY_ABOUT_UPDATES))
        .subtitle(strings::text(strings::NOTIFY_ALL_UPDATES_DESCRIPTION))
        .model(&model)
        .selected(selected)
        .sensitive(enabled)
        .build();
    let conn = conn.clone();
    row.connect_selected_notify(move |row| {
        let preference = notification_preference_at(row.selected());
        if let Err(error) =
            reprise_core::artist_news_notify::set_notification_preference(&conn, preference)
        {
            tracing::warn!(%error, "could not save update notification preference");
        }
    });
    row
}

fn notification_preference_index(
    preference: reprise_core::artist_news_notify::UpdateNotifications,
) -> u32 {
    match preference {
        reprise_core::artist_news_notify::UpdateNotifications::Off => 0,
        reprise_core::artist_news_notify::UpdateNotifications::Releases => 1,
        reprise_core::artist_news_notify::UpdateNotifications::All => 2,
    }
}

fn notification_preference_at(
    selected: u32,
) -> reprise_core::artist_news_notify::UpdateNotifications {
    match selected {
        0 => reprise_core::artist_news_notify::UpdateNotifications::Off,
        2 => reprise_core::artist_news_notify::UpdateNotifications::All,
        _ => reprise_core::artist_news_notify::UpdateNotifications::Releases,
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{notification_preference_at, notification_preference_index};

    #[test]
    fn os_7_all_updates_adds_the_concerts_delta() {
        let db = crate::test_db::open().unwrap();
        crate::test_db::connection(&db)
            .execute(
                "INSERT INTO concert_events (
                   id, artist_key, artist_name, starts_at, date_key, venue, city,
                   country, provider, fetched_at, dedupe_key
                 ) VALUES (1, 'artist', 'Castiel', '2026-08-20T19:00:00', '2026-08-20',
                           'Dynamo', 'Zürich', 'CH', 'fixture', 1, 'event-1')",
                [],
            )
            .unwrap();
        reprise_core::modules::set_enabled(&db, &reprise_core::modules::NEW_RELEASES_MODULE, true)
            .unwrap();
        reprise_core::modules::set_enabled(&db, &reprise_core::modules::CONCERTS_MODULE, true)
            .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 8, 14).unwrap();

        assert_eq!(
            crate::ui::notifications::updates::concert_delta_count(&db, today).unwrap(),
            0
        );
        let all = notification_preference_at(2);
        assert_eq!(notification_preference_index(all), 2);
        reprise_core::artist_news_notify::set_notification_preference(&db, all).unwrap();
        assert_eq!(
            crate::ui::notifications::updates::concert_delta_count(&db, today).unwrap(),
            1
        );

        reprise_core::modules::set_enabled(&db, &reprise_core::modules::CONCERTS_MODULE, false)
            .unwrap();
        assert_eq!(
            crate::ui::notifications::updates::concert_delta_count(&db, today).unwrap(),
            0
        );
    }
}
