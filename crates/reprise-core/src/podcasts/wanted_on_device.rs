//! `wanted_on_device` (`MTP-20`): persistent "sync to phone" intent for a
//! single episode, independent of whether it has a local file yet (design
//! 7f).
//!
//! Today's phone sync only ever looks at episodes that already have a
//! downloaded file (`device_sync::podcasts::query_candidates_for_device`
//! filters on `downloaded_path IS NOT NULL`). Design 7f removes the mental
//! step this forces on the user — "download first, then select" — by
//! letting "Sync to phone" on an episode with **no** file mark it wanted
//! anyway; the download that satisfies it follows automatically. This
//! module models that persistent state and the transition fired the
//! instant it is set. It deliberately does **not** build the downloader
//! that reacts to a wanted-but-missing episode — that is E2/E4.

use rusqlite::{params, Connection, OptionalExtension};

use crate::connectivity::{
    deferrable_action_outcome, ActionOutcome, Connectivity, LocalAvailability,
};

/// What happens the instant an episode is marked wanted, given whether it
/// already has a local file. Composes with `connectivity::
/// deferrable_action_outcome` (`NET-3a`) instead of duplicating its
/// online/offline decision — phone sync of a local file is local work, so
/// it never needs to wait on the network either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WantOutcome {
    /// Already downloaded — nothing else has to happen before the episode
    /// is syncable.
    AlreadyLocal,
    /// No file yet — the download that will satisfy this want runs now or
    /// is queued for later, per `NET-3a`.
    Download(ActionOutcome),
}

/// `MTP-20`: the transition fired the instant "Sync to phone" is set on an
/// episode. Marking wanted while a file already exists needs no download
/// step; marking wanted without a file starts (or queues) the download
/// immediately — nobody has to think "download first, then select".
#[must_use]
pub fn want_episode(local: LocalAvailability, connectivity: Connectivity) -> WantOutcome {
    match local {
        LocalAvailability::Available => WantOutcome::AlreadyLocal,
        LocalAvailability::Missing => {
            WantOutcome::Download(deferrable_action_outcome(connectivity, local))
        }
    }
}

/// Persists the wanted flag for one episode. Returns `false` if no episode
/// with that id exists.
pub fn set_wanted_on_device(
    conn: &Connection,
    episode_id: i64,
    wanted: bool,
) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE podcast_episodes SET wanted_on_device = ?2 WHERE id = ?1",
        params![episode_id, wanted],
    )? != 0)
}

/// Reads the wanted flag for one episode, or `None` if it does not exist.
pub fn wanted_on_device(
    conn: &Connection,
    episode_id: i64,
) -> Result<Option<bool>, rusqlite::Error> {
    conn.query_row(
        "SELECT wanted_on_device FROM podcast_episodes WHERE id = ?1",
        [episode_id],
        |row| row.get(0),
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated() -> Connection {
        crate::db::open_migrated(None).unwrap()
    }

    fn insert_subscription(conn: &Connection, id: i64) {
        conn.execute(
            "INSERT INTO podcast_subscriptions
             (id, kind, feed_url, title, auto_download, added_at)
             VALUES (?1, 'rss', ?2, 'Show', 0, 1)",
            params![id, format!("https://example.test/{id}")],
        )
        .unwrap();
    }

    fn insert_episode(conn: &Connection, id: i64, subscription_id: i64) {
        conn.execute(
            "INSERT INTO podcast_episodes
             (id, subscription_id, guid, title, audio_url, first_seen_at)
             VALUES (?1, ?2, ?3, 'Episode', 'https://example.test/e.mp3', 1)",
            params![id, subscription_id, format!("guid-{id}")],
        )
        .unwrap();
    }

    #[test]
    fn mtp_20_wanted_on_device_defaults_to_false_and_persists_when_set() {
        let conn = migrated();
        insert_subscription(&conn, 1);
        insert_episode(&conn, 11, 1);

        assert_eq!(wanted_on_device(&conn, 11).unwrap(), Some(false));

        let updated = set_wanted_on_device(&conn, 11, true).unwrap();

        assert!(updated);
        assert_eq!(wanted_on_device(&conn, 11).unwrap(), Some(true));
    }

    #[test]
    fn mtp_20_setting_wanted_on_an_unknown_episode_reports_no_change() {
        let conn = migrated();

        assert!(!set_wanted_on_device(&conn, 999, true).unwrap());
        assert_eq!(wanted_on_device(&conn, 999).unwrap(), None);
    }

    #[test]
    fn mtp_20_wanting_an_already_downloaded_episode_needs_no_download() {
        assert_eq!(
            want_episode(LocalAvailability::Available, Connectivity::Online),
            WantOutcome::AlreadyLocal
        );
        assert_eq!(
            want_episode(LocalAvailability::Available, Connectivity::Offline),
            WantOutcome::AlreadyLocal
        );
    }

    #[test]
    fn mtp_20_wanting_an_episode_without_a_file_downloads_now_when_online() {
        assert_eq!(
            want_episode(LocalAvailability::Missing, Connectivity::Online),
            WantOutcome::Download(ActionOutcome::RunsNow)
        );
    }

    #[test]
    fn mtp_20_wanting_an_episode_without_a_file_queues_the_download_when_offline() {
        assert_eq!(
            want_episode(LocalAvailability::Missing, Connectivity::Offline),
            WantOutcome::Download(ActionOutcome::QueuedOffline)
        );
    }
}
