//! Release-date notification selection, independent of clocks and frontends.

use chrono::NaiveDate;

use crate::artist_news::{NewsKind, StoredRelease};
use crate::artist_news_parsing::{parse_partial_date, release_kind};

const UPDATE_NOTIFICATIONS_KEY: &str = "updates.notifications";

/// Which release-feed changes may surface as desktop notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateNotifications {
    Off,
    Releases,
    All,
}

impl UpdateNotifications {
    pub fn as_setting(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Releases => "releases",
            Self::All => "all",
        }
    }

    pub fn from_setting(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "releases" => Some(Self::Releases),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

/// Reads the notification scope, defaulting safely for missing or invalid data.
pub fn notification_preference(db: &crate::db::Db) -> Result<UpdateNotifications, rusqlite::Error> {
    let stored = crate::library::settings::get_setting(db, UPDATE_NOTIFICATIONS_KEY)?;
    Ok(stored
        .as_deref()
        .and_then(UpdateNotifications::from_setting)
        .unwrap_or(UpdateNotifications::Releases))
}

/// Persists the notification scope in Reprise's SQLite settings store.
pub fn set_notification_preference(
    db: &crate::db::Db,
    preference: UpdateNotifications,
) -> Result<(), rusqlite::Error> {
    crate::library::settings::set_setting(db, UPDATE_NOTIFICATIONS_KEY, preference.as_setting())
}

/// Returns releases whose date boundary is reached by this due-check run.
pub fn released_today_candidates(
    db: &crate::db::Db,
    run_started_at: i64,
    today: NaiveDate,
) -> Result<Vec<StoredRelease>, rusqlite::Error> {
    Ok(
        crate::artist_news_query::release_notification_candidates(db, run_started_at, today)?
            .into_iter()
            .filter(|release| release_reaches_today(release, today))
            .collect(),
    )
}

/// Records a successfully sent release-date notification.
pub fn mark_release_notified(
    db: &crate::db::Db,
    release_group_mbid: &str,
    notified_at: i64,
) -> Result<(), rusqlite::Error> {
    crate::artist_news_query::mark_release_notified_at(db, release_group_mbid, notified_at)
}

fn release_reaches_today(release: &StoredRelease, today: NaiveDate) -> bool {
    let Some(release_date) = parse_partial_date(&release.first_release_date) else {
        return false;
    };
    if release_date != today {
        return false;
    }
    // The catalog projection labels an Album/EP as Upcoming through its
    // release day and New on the following date. Evaluating that boundary's
    // successor proves this exact row is the transition into New, while the
    // explicit equality above keeps older rows out of today's notification.
    let Some(after_release_day) = today.succ_opt() else {
        return false;
    };
    release_kind(
        &release.release_type.to_ascii_lowercase(),
        &release.first_release_date,
        release_date,
        after_release_day,
    ) == Some(NewsKind::New)
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{
        mark_release_notified, notification_preference, released_today_candidates,
        set_notification_preference, UpdateNotifications,
    };

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 14).unwrap()
    }

    fn insert_release(db: &crate::db::Db, mbid: &str, fetched_at: i64) {
        db.conn()
            .execute(
                "INSERT INTO new_releases (
                   release_group_mbid, artist_name, artist_mbid, title, release_type,
                   first_release_date, fetched_at, first_seen
                 ) VALUES (?1, 'Fixture Artist', 'artist-id', 'Release Day', 'Album',
                           '2026-08-14', ?2, ?2)",
                rusqlite::params![mbid, fetched_at],
            )
            .unwrap();
    }

    #[test]
    fn os_6_the_first_fetch_announces_nothing() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let run_started_at = 1_000;
        insert_release(&db, "first-fetch", run_started_at);

        assert!(released_today_candidates(&db, run_started_at, today())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_release_already_stamped_as_notified_is_not_announced_again() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let run_started_at = 1_000;
        insert_release(&db, "known-before-run", run_started_at - 1);

        let first = released_today_candidates(&db, run_started_at, today()).unwrap();
        assert_eq!(
            first
                .iter()
                .map(|release| release.release_group_mbid.as_str())
                .collect::<Vec<_>>(),
            ["known-before-run"]
        );

        mark_release_notified(&db, "known-before-run", run_started_at + 1).unwrap();
        assert!(released_today_candidates(&db, run_started_at + 2, today())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_fresh_database_defaults_update_notifications_to_releases() {
        let db = crate::db::Db::open_in_memory().unwrap();

        assert_eq!(
            notification_preference(&db).unwrap(),
            UpdateNotifications::Releases
        );
    }

    #[test]
    fn update_notification_preferences_round_trip_every_stored_value() {
        let db = crate::db::Db::open_in_memory().unwrap();

        for expected in [
            UpdateNotifications::Off,
            UpdateNotifications::Releases,
            UpdateNotifications::All,
        ] {
            set_notification_preference(&db, expected).unwrap();
            assert_eq!(notification_preference(&db).unwrap(), expected);
        }
    }
}
