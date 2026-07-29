//! `MTP-21` selection-candidate query for podcasts/YouTube — split out of
//! `podcasts.rs` only to keep that file under the project's 800-line rule.
//! [`query_selection_candidates_for_device`] is still conceptually part of
//! `podcasts`'s public query surface (re-exported there), not a separate
//! concern.

use rusqlite::Connection;

use super::{source_from_kind, PodcastSyncSource};
use crate::connectivity::LocalAvailability;
use crate::device_sync::selection::EpisodeSelectionCandidate;

/// `MTP-21`: every episode candidate `selection::select_episodes` needs to
/// see for `device_id`, across both [`PodcastSyncSource`] kinds — downloaded
/// episodes at any played state (`select_episodes`'s rule decides which are
/// wanted, this query only supplies facts) plus explicitly wanted-but-
/// missing ones (`wanted_on_device`, `MTP-20`). Scoped to shows/channels
/// selected for this device exactly like
/// [`super::query_candidates_for_device`] — that join over
/// `podcast_subscription_devices` (`POD-12`) *is* this pipeline's notion of
/// "aktivierte Show/Kanal" (`MTP-21`).
///
/// A missing-file episode only becomes a candidate when `wanted_on_device`
/// is set: without that gate, enabling an old show for a device would flood
/// [`crate::device_sync::selection::EpisodeSelectionResult::waiting`] with
/// its entire undownloaded backlog the instant the subscription is
/// selected. This gate
/// applies uniformly to RSS and YouTube — YouTube's own "latest N per
/// channel, independent of download state" cap (design 6b) has no
/// persisted value yet (see the `MTP-36` `[geplant]` draft in
/// `docs/ux-rules.md`), so until that lands, the caller runs
/// `select_episodes` with an unbounded `latest`, which makes this same gate
/// the only thing keeping YouTube's `waiting` set finite too.
pub fn query_selection_candidates_for_device(
    conn: &Connection,
    device_id: &str,
) -> Result<Vec<(PodcastSyncSource, EpisodeSelectionCandidate)>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT e.id, e.subscription_id, s.kind,
                COALESCE(e.published_at, e.first_seen_at),
                e.played_at IS NOT NULL,
                e.downloaded_path IS NOT NULL
         FROM podcast_episodes e
         JOIN podcast_subscriptions s ON s.id = e.subscription_id
         JOIN podcast_subscription_devices d
           ON d.subscription_id = s.id AND d.device_id = ?1
         WHERE s.removed_at IS NULL
           AND e.removed_at IS NULL
           AND (e.downloaded_path IS NOT NULL OR e.wanted_on_device = 1)",
    )?;
    let rows = statement.query_map([device_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, bool>(4)?,
            row.get::<_, bool>(5)?,
        ))
    })?;
    let mut candidates = Vec::new();
    for row in rows {
        let (episode_id, group_id, kind, published_at, played, has_file) = row?;
        let Some(source) = source_from_kind(&kind) else {
            continue;
        };
        candidates.push((
            source,
            EpisodeSelectionCandidate {
                episode_id,
                group_id,
                published_at,
                played,
                local: if has_file {
                    LocalAvailability::Available
                } else {
                    LocalAvailability::Missing
                },
            },
        ));
    }
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;
    use crate::device_sync::selection::{
        select_episodes, EpisodeSelectionResult, EpisodeSelectionRule,
    };
    use std::collections::HashSet;

    fn migrated() -> Connection {
        crate::db::open_migrated(None).unwrap()
    }

    fn insert_subscription(conn: &Connection, id: i64, kind: &str, title: &str) {
        conn.execute(
            "INSERT INTO podcast_subscriptions
             (id, kind, feed_url, title, auto_download, added_at)
             VALUES (?1, ?2, ?3, ?4, 0, 1)",
            params![id, kind, format!("https://example.test/{id}"), title],
        )
        .unwrap();
    }

    fn insert_episode(conn: &Connection, id: i64, subscription_id: i64, path: &std::path::Path) {
        conn.execute(
            "INSERT INTO podcast_episodes
             (id, subscription_id, guid, title, audio_url, downloaded_path,
              downloaded_bytes, first_seen_at)
             VALUES (?1, ?2, ?3, 'Episode', 'https://example.test/e.mp3', ?4, 1, 1)",
            params![
                id,
                subscription_id,
                format!("episode-{id}"),
                path.to_string_lossy()
            ],
        )
        .unwrap();
    }

    fn insert_episode_without_file(conn: &Connection, id: i64, subscription_id: i64) {
        conn.execute(
            "INSERT INTO podcast_episodes
             (id, subscription_id, guid, title, audio_url, first_seen_at)
             VALUES (?1, ?2, ?3, 'Episode', 'https://example.test/e.mp3', 1)",
            params![id, subscription_id, format!("episode-{id}")],
        )
        .unwrap();
    }

    /// Runs the RSS podcast rule (`MTP-21`'s `UnplayedDownloadsOnly`) over
    /// just the RSS-sourced candidates, the way
    /// `device_sync_compact::recompute_delta_silent` does in the live
    /// pipeline.
    fn rss_selection_result(
        candidates: &[(PodcastSyncSource, EpisodeSelectionCandidate)],
    ) -> EpisodeSelectionResult {
        let rss = candidates
            .iter()
            .filter(|(source, _)| *source == PodcastSyncSource::Rss)
            .map(|(_, candidate)| candidate.clone())
            .collect::<Vec<_>>();
        let enabled_shows = rss.iter().map(|c| c.group_id).collect::<HashSet<_>>();
        select_episodes(
            &rss,
            &EpisodeSelectionRule::UnplayedDownloadsOnly { enabled_shows },
        )
    }

    // `MTP-21` end-to-end: `query_selection_candidates_for_device` feeding
    // `selection::select_episodes` — the live wiring this pipeline
    // previously lacked entirely (`query_candidates_for_device` only ever
    // returned downloaded episodes, with no played filter, and nothing
    // surfaced a wanted-but-missing episode at all).

    #[test]
    fn mtp_21_a_played_downloaded_episode_of_an_enabled_show_is_never_wanted() {
        let conn = migrated();
        let downloads = tempfile::tempdir().unwrap();
        let unplayed_path = downloads.path().join("unplayed.mp3");
        let played_path = downloads.path().join("played.mp3");
        std::fs::write(&unplayed_path, b"unplayed-audio").unwrap();
        std::fs::write(&played_path, b"played-audio").unwrap();
        insert_subscription(&conn, 1, "rss", "Show");
        crate::podcasts::phone_sync::set_device_enabled(&conn, 1, "mtp:pixel", true).unwrap();
        insert_episode(&conn, 11, 1, &unplayed_path);
        insert_episode(&conn, 12, 1, &played_path);
        conn.execute(
            "UPDATE podcast_episodes SET played_at = 1 WHERE id = 12",
            [],
        )
        .unwrap();

        let candidates = query_selection_candidates_for_device(&conn, "mtp:pixel").unwrap();
        let result = rss_selection_result(&candidates);

        assert_eq!(
            result.ready,
            [11],
            "the unplayed downloaded episode is ready to copy"
        );
        assert!(
            !result.ready.contains(&12) && !result.waiting.contains(&12),
            "a played episode is never wanted at all — not ready, not waiting, not copied"
        );
    }

    #[test]
    fn mtp_21_an_unplayed_downloaded_episode_is_ready_to_copy() {
        let conn = migrated();
        let downloads = tempfile::tempdir().unwrap();
        let path = downloads.path().join("fresh.mp3");
        std::fs::write(&path, b"fresh-audio").unwrap();
        insert_subscription(&conn, 1, "rss", "Show");
        crate::podcasts::phone_sync::set_device_enabled(&conn, 1, "mtp:pixel", true).unwrap();
        insert_episode(&conn, 11, 1, &path);

        let candidates = query_selection_candidates_for_device(&conn, "mtp:pixel").unwrap();
        let result = rss_selection_result(&candidates);

        assert_eq!(result.ready, [11]);
        assert!(result.waiting.is_empty());
    }

    #[test]
    fn mtp_21_a_wanted_episode_with_no_file_counts_as_waiting_never_as_ready() {
        let conn = migrated();
        insert_subscription(&conn, 1, "rss", "Show");
        crate::podcasts::phone_sync::set_device_enabled(&conn, 1, "mtp:pixel", true).unwrap();
        insert_episode_without_file(&conn, 11, 1);
        crate::podcasts::wanted_on_device::set_wanted_on_device(&conn, 11, true).unwrap();

        let candidates = query_selection_candidates_for_device(&conn, "mtp:pixel").unwrap();
        let result = rss_selection_result(&candidates);

        assert_eq!(
            result.waiting,
            [11],
            "a wanted episode without a local file counts as waiting"
        );
        assert!(
            result.ready.is_empty(),
            "it must never be treated as copyable while it has no file"
        );
    }

    #[test]
    fn mtp_21_an_unwanted_missing_episode_never_becomes_a_candidate_at_all() {
        let conn = migrated();
        insert_subscription(&conn, 1, "rss", "Show");
        crate::podcasts::phone_sync::set_device_enabled(&conn, 1, "mtp:pixel", true).unwrap();
        // No file, and nobody asked for it (`wanted_on_device` stays the
        // default `false`) — must not flood `waiting` with an untouched
        // backlog episode the instant its show is enabled for a device.
        insert_episode_without_file(&conn, 11, 1);

        let candidates = query_selection_candidates_for_device(&conn, "mtp:pixel").unwrap();

        assert!(
            candidates.is_empty(),
            "an episode nobody downloaded or wanted is not a candidate at all"
        );
    }

    #[test]
    fn mtp_21_selection_candidates_stay_scoped_to_shows_enabled_for_the_device() {
        let conn = migrated();
        let downloads = tempfile::tempdir().unwrap();
        let path = downloads.path().join("episode.mp3");
        std::fs::write(&path, b"audio").unwrap();
        insert_subscription(&conn, 1, "rss", "Enabled Show");
        insert_subscription(&conn, 2, "rss", "Other Show");
        crate::podcasts::phone_sync::set_device_enabled(&conn, 1, "mtp:pixel", true).unwrap();
        insert_episode(&conn, 11, 1, &path);
        insert_episode(&conn, 12, 2, &path);

        let candidates = query_selection_candidates_for_device(&conn, "mtp:pixel").unwrap();

        assert_eq!(
            candidates
                .iter()
                .map(|(_, candidate)| candidate.episode_id)
                .collect::<Vec<_>>(),
            [11],
            "an episode of a show never enabled for this device is not a candidate"
        );
    }
}
