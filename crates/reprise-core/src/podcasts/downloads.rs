//! Deterministic podcast download storage and cleanup.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use super::config::CleanupPolicy;
use super::PodcastError;

const PLAYED_RETENTION_SECONDS: i64 = 7 * 24 * 60 * 60;
const KEEP_EPISODES_PER_SHOW: usize = 5;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CleanupSummary {
    pub files_deleted: usize,
    pub bytes_deleted: u64,
}

pub fn set_downloaded_path(
    conn: &Connection,
    episode_id: i64,
    path: Option<&str>,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE podcast_episodes
         SET downloaded_path = ?2,
             downloaded_bytes = CASE WHEN ?2 IS NULL THEN NULL ELSE downloaded_bytes END
         WHERE id = ?1",
        params![episode_id, path],
    )?;
    Ok(())
}

pub fn set_downloaded_file(
    conn: &Connection,
    episode_id: i64,
    path: Option<&str>,
    downloaded_bytes: Option<i64>,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE podcast_episodes
         SET downloaded_path = ?2,
             downloaded_bytes = CASE
               WHEN ?2 IS NULL THEN NULL
               WHEN ?3 IS NULL THEN NULL
               ELSE MAX(?3, 0)
             END
         WHERE id = ?1",
        params![episode_id, path, downloaded_bytes],
    )?;
    Ok(())
}

pub fn persist_completed_if_active(
    conn: &Connection,
    episode_id: i64,
    path: &str,
    downloaded_bytes: u64,
) -> Result<bool, rusqlite::Error> {
    let downloaded_bytes = downloaded_bytes.min(i64::MAX as u64) as i64;
    Ok(conn.execute(
        "UPDATE podcast_episodes
         SET downloaded_path = ?2, downloaded_bytes = ?3
         WHERE id = ?1
           AND removed_at IS NULL
           AND EXISTS (
             SELECT 1 FROM podcast_subscriptions s
             WHERE s.id = podcast_episodes.subscription_id
               AND s.removed_at IS NULL
           )",
        params![episode_id, path, downloaded_bytes],
    )? != 0)
}

pub fn downloaded_paths_for_subscription(
    conn: &Connection,
    subscription_id: i64,
) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT downloaded_path FROM podcast_episodes
         WHERE subscription_id = ?1 AND downloaded_path IS NOT NULL
         ORDER BY id",
    )?;
    let rows = statement.query_map([subscription_id], |row| row.get(0))?;
    rows.collect::<Result<Vec<_>, _>>()
}

#[must_use]
pub fn default_download_root() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("reprise/podcasts")
}

#[must_use]
pub fn download_path(root: &Path, feed_url: &str, guid: &str, extension: &str) -> PathBuf {
    let extension = safe_extension(extension);
    root.join(format!("{:016x}", fnv1a_64(feed_url.as_bytes())))
        .join(format!("{:016x}.{extension}", fnv1a_64(guid.as_bytes())))
}

#[must_use]
pub fn extension_from_url(value: &str) -> &'static str {
    let extension = url::Url::parse(value).ok().and_then(|url| {
        Path::new(url.path())
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
    });
    match extension.as_deref() {
        Some("mp3") => "mp3",
        Some("m4a" | "mp4") => "m4a",
        Some("ogg" | "oga") => "ogg",
        Some("opus") => "opus",
        Some("flac") => "flac",
        Some("wav") => "wav",
        _ => "audio",
    }
}

pub fn reclaim_existing(
    root: &Path,
    feed_url: &str,
    guid: &str,
) -> std::io::Result<Option<PathBuf>> {
    let directory = root.join(format!("{:016x}", fnv1a_64(feed_url.as_bytes())));
    let prefix = format!("{:016x}.", fnv1a_64(guid.as_bytes()));
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut matches = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(&prefix) && !name.ends_with(".part"))
        {
            matches.push(entry.path());
        }
    }
    matches.sort();
    Ok(matches.into_iter().next())
}

pub fn prepare_destination(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[must_use]
pub fn partial_path(destination: &Path) -> PathBuf {
    let mut name = destination.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}

pub fn download_atomically(
    destination: &Path,
    operation: impl FnOnce(&Path) -> Result<(), PodcastError>,
) -> Result<u64, PodcastError> {
    prepare_destination(destination).map_err(|error| PodcastError::Body(error.to_string()))?;
    let temporary = partial_path(destination);
    remove_if_present(&temporary)?;
    if let Err(error) = operation(&temporary) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    let metadata = temporary
        .symlink_metadata()
        .map_err(|error| PodcastError::Body(error.to_string()))?;
    if !metadata.file_type().is_file() {
        let _ = std::fs::remove_file(&temporary);
        return Err(PodcastError::Body(
            "download did not produce a regular file".to_owned(),
        ));
    }
    if destination.exists() {
        let _ = std::fs::remove_file(&temporary);
        return Err(PodcastError::Body(format!(
            "download destination already exists: {}",
            destination.display()
        )));
    }
    if let Err(error) = std::fs::rename(&temporary, destination) {
        let _ = std::fs::remove_file(&temporary);
        return Err(PodcastError::Body(format!(
            "could not publish completed download: {error}"
        )));
    }
    Ok(metadata.len())
}

fn remove_if_present(path: &Path) -> Result<(), PodcastError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PodcastError::Body(error.to_string())),
    }
}

pub fn enforce_cleanup(
    conn: &Connection,
    policy: CleanupPolicy,
    now: i64,
) -> Result<CleanupSummary, CleanupError> {
    let candidates = cleanup_candidates(conn, policy, now)?;
    let mut summary = CleanupSummary::default();
    for (episode_id, path) in candidates {
        let path = PathBuf::from(path);
        let bytes = std::fs::metadata(&path)
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        match std::fs::remove_file(&path) {
            Ok(()) => {
                summary.files_deleted += 1;
                summary.bytes_deleted = summary.bytes_deleted.saturating_add(bytes);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        super::store::set_downloaded_path(conn, episode_id, None)?;
    }
    Ok(summary)
}

#[derive(Debug, thiserror::Error)]
pub enum CleanupError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("download cleanup failed: {0}")]
    Io(#[from] std::io::Error),
}

fn cleanup_candidates(
    conn: &Connection,
    policy: CleanupPolicy,
    now: i64,
) -> Result<Vec<(i64, String)>, rusqlite::Error> {
    match policy {
        CleanupPolicy::KeepAll => Ok(Vec::new()),
        CleanupPolicy::DeletePlayedAfter7Days => {
            let cutoff = now.saturating_sub(PLAYED_RETENTION_SECONDS);
            let mut statement = conn.prepare(
                "SELECT e.id, e.downloaded_path
                 FROM podcast_episodes e
                 JOIN podcast_subscriptions s ON s.id = e.subscription_id
                 WHERE s.removed_at IS NULL
                   AND e.downloaded_path IS NOT NULL
                   AND e.played_at IS NOT NULL
                   AND e.played_at <= ?1
                 ORDER BY e.id",
            )?;
            let rows = statement.query_map([cutoff], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect()
        }
        CleanupPolicy::KeepLast5 => {
            let mut statement = conn.prepare(
                "SELECT id, downloaded_path FROM (
                   SELECT e.id, e.downloaded_path,
                          ROW_NUMBER() OVER (
                            PARTITION BY e.subscription_id
                            ORDER BY e.published_at IS NULL, e.published_at DESC,
                                     e.first_seen_at DESC, e.id DESC
                          ) AS episode_rank
                   FROM podcast_episodes e
                   JOIN podcast_subscriptions s ON s.id = e.subscription_id
                   WHERE s.removed_at IS NULL
                 )
                 WHERE episode_rank > ?1 AND downloaded_path IS NOT NULL
                 ORDER BY id",
            )?;
            let rows = statement.query_map([KEEP_EPISODES_PER_SHOW as i64], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
            rows.collect()
        }
    }
}

fn safe_extension(extension: &str) -> &str {
    let extension = extension.trim().trim_start_matches('.');
    if !extension.is_empty()
        && extension.len() <= 8
        && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        extension
    } else {
        "audio"
    }
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::podcasts::feed::ParsedEpisode;
    use crate::podcasts::store::{self, NewSubscription};
    use crate::podcasts::PodcastError;
    use crate::podcasts::PodcastKind;

    fn conn() -> Connection {
        crate::db::open_migrated(None).unwrap()
    }

    fn add_show(conn: &Connection) -> i64 {
        store::add_or_restore(
            conn,
            &NewSubscription {
                kind: PodcastKind::Rss,
                feed_url: "https://example.test/feed".to_owned(),
                title: "Show".to_owned(),
                author: None,
                image_url: None,
                auto_download: false,
            },
            1,
        )
        .unwrap()
    }

    fn add_download(
        conn: &Connection,
        root: &Path,
        subscription_id: i64,
        number: i64,
        played_at: Option<i64>,
    ) -> i64 {
        let guid = format!("episode-{number}");
        let result = store::upsert_episode(
            conn,
            subscription_id,
            &ParsedEpisode {
                guid: guid.clone(),
                title: guid.clone(),
                audio_url: format!("https://example.test/{guid}.mp3"),
                page_url: None,
                published_at: Some(number),
                duration_secs: None,
            },
            number,
        )
        .unwrap()
        .expect("episode should be imported");
        let path = download_path(root, "https://example.test/feed", &guid, "mp3");
        prepare_destination(&path).unwrap();
        std::fs::write(&path, [0_u8; 4]).unwrap();
        store::set_downloaded_path(conn, result.episode_id, path.to_str()).unwrap();
        if let Some(played_at) = played_at {
            store::mark_played(conn, result.episode_id, played_at).unwrap();
        }
        result.episode_id
    }

    #[test]
    fn pod_5_paths_are_guid_keyed_and_reclaimable() {
        let directory = tempfile::tempdir().unwrap();
        let first = download_path(
            directory.path(),
            "https://example.test/feed",
            "stable-guid",
            ".mp3",
        );
        let second = download_path(
            directory.path(),
            "https://example.test/feed",
            "stable-guid",
            "mp3",
        );
        assert_eq!(first, second);
        prepare_destination(&first).unwrap();
        std::fs::write(&first, b"audio").unwrap();
        assert_eq!(
            reclaim_existing(directory.path(), "https://example.test/feed", "stable-guid").unwrap(),
            Some(first)
        );
    }

    #[test]
    fn pod_7_downloads_publish_only_complete_files_and_clean_failed_partials() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("episode.mp3");
        let part = partial_path(&destination);

        let bytes = download_atomically(&destination, |temporary| {
            assert_eq!(temporary, part);
            std::fs::write(temporary, b"complete")
                .map_err(|error| PodcastError::Body(error.to_string()))
        })
        .unwrap();
        assert_eq!(bytes, 8);
        assert_eq!(std::fs::read(&destination).unwrap(), b"complete");
        assert!(!part.exists());

        std::fs::remove_file(&destination).unwrap();
        let result = download_atomically(&destination, |temporary| {
            std::fs::write(temporary, b"partial").unwrap();
            Err(PodcastError::Transport("offline".to_owned()))
        });
        assert!(matches!(result, Err(PodcastError::Transport(_))));
        assert!(!destination.exists());
        assert!(!part.exists());
    }

    #[test]
    fn pod_7_reclaim_ignores_partial_files() {
        let directory = tempfile::tempdir().unwrap();
        let destination = download_path(
            directory.path(),
            "https://example.test/feed",
            "stable-guid",
            "mp3",
        );
        prepare_destination(&destination).unwrap();
        std::fs::write(partial_path(&destination), b"partial").unwrap();

        assert_eq!(
            reclaim_existing(directory.path(), "https://example.test/feed", "stable-guid").unwrap(),
            None
        );
    }

    #[test]
    fn pod_7_completed_file_is_not_persisted_after_episode_removal() {
        let conn = conn();
        let show = add_show(&conn);
        let result = store::upsert_episode(
            &conn,
            show,
            &ParsedEpisode {
                guid: "race".to_owned(),
                title: "Race".to_owned(),
                audio_url: "https://example.test/race.mp3".to_owned(),
                page_url: None,
                published_at: None,
                duration_secs: None,
            },
            1,
        )
        .unwrap()
        .unwrap();
        store::tombstone_episode(&conn, result.episode_id, 2).unwrap();

        assert!(
            !persist_completed_if_active(&conn, result.episode_id, "/podcasts/race.mp3", 128)
                .unwrap()
        );
    }

    #[test]
    fn keep_all_never_deletes_downloads() {
        let conn = conn();
        let directory = tempfile::tempdir().unwrap();
        let show = add_show(&conn);
        let episode = add_download(&conn, directory.path(), show, 1, Some(1));

        assert_eq!(
            enforce_cleanup(&conn, CleanupPolicy::KeepAll, 1_000_000).unwrap(),
            CleanupSummary::default()
        );
        assert!(store::episode(&conn, episode)
            .unwrap()
            .unwrap()
            .downloaded_path
            .is_some());
    }

    #[test]
    fn played_age_policy_deletes_only_old_played_downloads() {
        let conn = conn();
        let directory = tempfile::tempdir().unwrap();
        let show = add_show(&conn);
        let now = 1_000_000;
        let old = add_download(
            &conn,
            directory.path(),
            show,
            1,
            Some(now - PLAYED_RETENTION_SECONDS),
        );
        let recent = add_download(&conn, directory.path(), show, 2, Some(now - 10));
        let unplayed = add_download(&conn, directory.path(), show, 3, None);

        let summary = enforce_cleanup(&conn, CleanupPolicy::DeletePlayedAfter7Days, now).unwrap();

        assert_eq!(summary.files_deleted, 1);
        assert!(store::episode(&conn, old)
            .unwrap()
            .unwrap()
            .downloaded_path
            .is_none());
        for id in [recent, unplayed] {
            assert!(store::episode(&conn, id)
                .unwrap()
                .unwrap()
                .downloaded_path
                .is_some());
        }
    }

    #[test]
    fn keep_last_five_is_applied_per_show() {
        let conn = conn();
        let directory = tempfile::tempdir().unwrap();
        let show = add_show(&conn);
        for number in 1..=7 {
            add_download(&conn, directory.path(), show, number, None);
        }

        let summary = enforce_cleanup(&conn, CleanupPolicy::KeepLast5, 0).unwrap();

        assert_eq!(summary.files_deleted, 2);
        let remaining = super::super::query::episodes_for_subscription(&conn, show)
            .unwrap()
            .into_iter()
            .filter(|episode| episode.downloaded_path.is_some())
            .count();
        assert_eq!(remaining, 5);
    }
}
