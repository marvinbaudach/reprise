//! Release-date and concert update notifications.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use chrono::NaiveDate;
use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::glib::variant::ToVariant;
use reprise_core::artist_news::StoredRelease;
use reprise_core::db::Db;

const COLLECT_RELEASES_AT: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ConcertAnnouncementState {
    observed_unseen: usize,
}

impl ConcertAnnouncementState {
    pub(super) fn observe(self, unseen: usize) -> (Self, bool) {
        (
            Self {
                observed_unseen: unseen,
            },
            unseen > self.observed_unseen,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NotificationTarget {
    Link(String),
    View(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NotificationSpec {
    id: String,
    title: String,
    body: String,
    target: NotificationTarget,
    cover_mbid: Option<String>,
}

fn release_notification_specs(releases: &[StoredRelease]) -> Vec<NotificationSpec> {
    if releases.len() >= COLLECT_RELEASES_AT {
        let body = releases
            .iter()
            .take(3)
            .map(|release| release.artist_name.as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        return vec![NotificationSpec {
            id: "updates-releases".into(),
            title: crate::ui::strings::update_releases_title(releases.len()),
            body,
            target: NotificationTarget::View("releases"),
            cover_mbid: Some(releases[0].release_group_mbid.clone()),
        }];
    }

    releases
        .iter()
        .map(|release| NotificationSpec {
            id: format!("updates-release-{}", release.release_group_mbid),
            title: release.title.clone(),
            body: crate::ui::strings::update_release_body(
                &release.artist_name,
                &release.release_type,
            ),
            target: NotificationTarget::Link(
                reprise_core::artist_news_links::announce_url_or_fallback(
                    release.announce_url.as_deref(),
                    &release.release_group_mbid,
                ),
            ),
            cover_mbid: Some(release.release_group_mbid.clone()),
        })
        .collect()
}

fn concert_notification_spec(
    count: usize,
    concert: &reprise_core::concerts::ConcertRow,
    formatted_date: &str,
) -> NotificationSpec {
    NotificationSpec {
        id: "updates-concerts".into(),
        title: crate::ui::strings::update_concerts_title(count),
        body: crate::ui::strings::update_concert_body(
            &concert.artist_name,
            &concert.city,
            formatted_date,
        ),
        target: NotificationTarget::View("concerts"),
        cover_mbid: None,
    }
}

/// Sends the full unseen Concerts delta once for an `All updates` run.
pub(in crate::ui) fn concert_delta_count(
    db: &Db,
    today: NaiveDate,
) -> Result<usize, rusqlite::Error> {
    if !reprise_core::modules::is_enabled(db, &reprise_core::modules::NEW_RELEASES_MODULE)?
        || !reprise_core::modules::is_enabled(db, &reprise_core::modules::CONCERTS_MODULE)?
        || reprise_core::artist_news_notify::notification_preference(db)?
            != reprise_core::artist_news_notify::UpdateNotifications::All
    {
        return Ok(0);
    }
    let filter = reprise_core::concerts::config::persisted_filter(db)?;
    let location = reprise_core::concerts::config::location(db)?;
    let count = reprise_core::concerts::count_unseen(db, &filter, location.as_ref(), today)?;
    Ok(usize::try_from(count).unwrap_or(usize::MAX))
}

pub(super) fn send_due_concerts(
    application: &gio::Application,
    db: &Db,
    today: NaiveDate,
    count: usize,
) -> Result<usize, rusqlite::Error> {
    if count == 0 {
        return Ok(0);
    }
    let filter = reprise_core::concerts::config::persisted_filter(db)?;
    let location = reprise_core::concerts::config::location(db)?;
    let Some(first) =
        reprise_core::concerts::query_unseen(db, &filter, location.as_ref(), today, 1)?
            .into_iter()
            .next()
    else {
        return Ok(0);
    };
    let date =
        crate::ui::concerts::concerts_presentation::format_event_date(&first.date_key, today);
    send_notification(
        application,
        &concert_notification_spec(count, &first, &date),
        None,
    );
    Ok(count)
}

/// Sends and stamps every release that became due before this check began.
pub(super) fn send_due_releases(
    application: &gio::Application,
    db: &Db,
    run_started_at: i64,
    now: i64,
    today: NaiveDate,
    cover_generation: &Rc<Cell<u64>>,
) -> Result<usize, rusqlite::Error> {
    let generation = cover_generation.get().wrapping_add(1);
    cover_generation.set(generation);
    if !reprise_core::modules::is_enabled(db, &reprise_core::modules::NEW_RELEASES_MODULE)? {
        return Ok(0);
    }
    if reprise_core::artist_news_notify::notification_preference(db)?
        == reprise_core::artist_news_notify::UpdateNotifications::Off
    {
        return Ok(0);
    }
    let releases =
        reprise_core::artist_news_notify::released_today_candidates(db, run_started_at, today)?;
    let specs = deliver_release_candidates(db, &releases, now, |spec| {
        send_notification(application, spec, None);
    })?;
    for spec in specs {
        send_with_cover_when_available(application, spec, generation, cover_generation);
    }
    Ok(releases.len())
}

fn deliver_release_candidates(
    db: &Db,
    releases: &[StoredRelease],
    now: i64,
    mut send: impl FnMut(&NotificationSpec),
) -> Result<Vec<NotificationSpec>, rusqlite::Error> {
    let specs = release_notification_specs(releases);
    if releases.len() >= COLLECT_RELEASES_AT {
        if let Some(spec) = specs.first() {
            send(spec);
            for release in releases {
                reprise_core::artist_news_notify::mark_release_notified(
                    db,
                    &release.release_group_mbid,
                    now,
                )?;
            }
        }
    } else {
        for (release, spec) in releases.iter().zip(&specs) {
            send(spec);
            reprise_core::artist_news_notify::mark_release_notified(
                db,
                &release.release_group_mbid,
                now,
            )?;
        }
    }
    Ok(specs)
}

fn send_notification(application: &gio::Application, spec: &NotificationSpec, icon: Option<&Path>) {
    let notification = gio::Notification::new(&spec.title);
    notification.set_body(Some(&spec.body));
    match &spec.target {
        NotificationTarget::Link(url) => notification
            .set_default_action_and_target_value("app.open-updates-link", Some(&url.to_variant())),
        NotificationTarget::View(target) => notification.set_default_action_and_target_value(
            "app.open-updates-view",
            Some(&target.to_variant()),
        ),
    }
    if let Some(icon) = icon {
        notification.set_icon(&gio::FileIcon::new(&gio::File::for_path(icon)));
    }
    application.send_notification(Some(&spec.id), &notification);
}

fn send_with_cover_when_available(
    application: &gio::Application,
    spec: NotificationSpec,
    expected_generation: u64,
    current_generation: &Rc<Cell<u64>>,
) {
    let application = application.clone();
    let current_generation = current_generation.clone();
    let Some(mbid) = spec.cover_mbid.clone() else {
        return;
    };
    glib::spawn_future_local(async move {
        let cover = gio::spawn_blocking(move || {
            reprise_core::cover_download::fetch_release_group_cover(&mbid)
        })
        .await
        .ok();
        if !super::generation_is_current(expected_generation, current_generation.get()) {
            return;
        }
        let Some(reprise_core::cover_download::ReleaseGroupCover::Image(path)) = cover else {
            return;
        };
        send_notification(&application, &spec, Some(&path));
    });
}

#[cfg(test)]
mod tests {
    use reprise_core::artist_news::{LibraryPresence, StoredRelease};
    use reprise_core::concerts::{ConcertRow, TicketAvailability};

    use super::{
        concert_notification_spec, deliver_release_candidates, release_notification_specs,
        ConcertAnnouncementState,
    };

    fn release(mbid: &str, artist: &str, title: &str) -> StoredRelease {
        StoredRelease {
            release_group_mbid: mbid.into(),
            artist_name: artist.into(),
            artist_mbid: format!("artist-{mbid}"),
            title: title.into(),
            release_type: "Album".into(),
            first_release_date: "2026-08-14".into(),
            fetched_at: 1,
            seen_at: None,
            hidden: false,
            presence: LibraryPresence::Absent,
            announce_url: Some(format!("https://{artist}.bandcamp.com/album/{mbid}")),
            track_count: Some(10),
            local_track_count: 0,
        }
    }

    fn concert() -> ConcertRow {
        ConcertRow {
            id: 1,
            availability: TicketAvailability::OnSale,
            date_key: "2026-08-20".into(),
            starts_at: "2026-08-20T20:00:00".into(),
            artist_name: "Castiel".into(),
            venue: "Dynamo".into(),
            city: "Zürich".into(),
            region: None,
            country: Some("CH".into()),
            latitude: None,
            longitude: None,
            distance_km: None,
            ticket_url: None,
            ticket_source: None,
            event_url: None,
            provider: "fixture".into(),
            is_similar: false,
            similar_to: None,
        }
    }

    #[test]
    fn concerts_use_the_full_unseen_count_and_first_row_example() {
        let notification = concert_notification_spec(12, &concert(), "20.08.2026");

        assert_eq!(notification.id, "updates-concerts");
        assert_eq!(notification.title, "12 new concerts");
        assert_eq!(notification.body, "Castiel · Zürich · 20.08.2026");
        assert_eq!(
            notification.target,
            super::NotificationTarget::View("concerts")
        );
    }

    #[test]
    fn concert_notifications_follow_unseen_stack_growth_and_reset() {
        let (state, should_send) = ConcertAnnouncementState::default().observe(2);
        assert!(should_send, "the first non-empty stack should be announced");

        let (state, should_send) = state.observe(2);
        assert!(!should_send, "an unchanged stack should stay silent");

        let (state, should_send) = state.observe(3);
        assert!(should_send, "a grown stack should be announced again");

        let (state, should_send) = state.observe(0);
        assert!(!should_send, "emptying the stack should stay silent");

        let (_, should_send) = state.observe(1);
        assert!(
            should_send,
            "a new concert after the stack was emptied should be announced"
        );
    }

    #[test]
    fn one_to_three_releases_keep_stable_per_release_notifications() {
        let releases = [
            release("one", "First", "First Record"),
            release("two", "Second", "Second Record"),
        ];

        let notifications = release_notification_specs(&releases);

        assert_eq!(notifications.len(), 2);
        assert_eq!(notifications[0].id, "updates-release-one");
        assert_eq!(notifications[0].title, "First Record");
        assert_eq!(notifications[0].body, "First · Album · out today");
        assert_eq!(
            notifications[0].target,
            super::NotificationTarget::Link("https://First.bandcamp.com/album/one".into())
        );
    }

    #[test]
    fn notification_link_matches_the_release_result_value() {
        let release = release("same-target", "Artist", "Record");
        let expected = reprise_core::artist_news_links::announce_url_or_fallback(
            release.announce_url.as_deref(),
            &release.release_group_mbid,
        );

        let notifications = release_notification_specs(&[release]);

        assert_eq!(
            notifications[0].target,
            super::NotificationTarget::Link(expected)
        );
    }

    #[test]
    fn four_releases_collapse_into_one_collected_notification() {
        let releases = [
            release("one", "First", "One"),
            release("two", "Second", "Two"),
            release("three", "Third", "Three"),
            release("four", "Fourth", "Four"),
        ];

        let notifications = release_notification_specs(&releases);

        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].id, "updates-releases");
        assert_eq!(notifications[0].title, "4 releases are out");
        assert_eq!(notifications[0].body, "First · Second · Third");
        assert_eq!(
            notifications[0].target,
            super::NotificationTarget::View("releases")
        );
    }

    #[test]
    fn the_release_stamp_is_written_only_after_the_notification_is_sent() {
        let db = crate::test_db::open().unwrap();
        crate::test_db::connection(&db)
            .execute(
                "INSERT INTO new_releases (
                   release_group_mbid, artist_name, artist_mbid, title, release_type,
                   first_release_date, fetched_at, first_seen
                 ) VALUES ('known', 'Artist', 'artist-id', 'Record', 'Album',
                           '2026-08-14', 1, 1)",
                [],
            )
            .unwrap();
        let today = chrono::NaiveDate::from_ymd_opt(2026, 8, 14).unwrap();
        let releases =
            reprise_core::artist_news_notify::released_today_candidates(&db, 2, today).unwrap();
        let mut sends = 0;

        deliver_release_candidates(&db, &releases, 3, |_| {
            sends += 1;
            assert_eq!(
                reprise_core::artist_news_notify::released_today_candidates(&db, 2, today)
                    .unwrap()
                    .len(),
                1
            );
        })
        .unwrap();

        assert_eq!(sends, 1);
        assert!(
            reprise_core::artist_news_notify::released_today_candidates(&db, 4, today)
                .unwrap()
                .is_empty()
        );
    }
}
