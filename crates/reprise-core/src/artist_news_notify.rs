//! Release-date notification selection, independent of clocks and frontends.

use chrono::NaiveDate;

use crate::artist_news::{NewsKind, StoredRelease};
use crate::artist_news_parsing::{parse_partial_date, release_kind};

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

    use super::{mark_release_notified, released_today_candidates};

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
}
