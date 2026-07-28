//! Deterministic podcast download storage and cleanup.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::config::CleanupPolicy;

const PLAYED_RETENTION_SECONDS: i64 = 7 * 24 * 60 * 60;
const KEEP_EPISODES_PER_SHOW: usize = 5;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CleanupSummary {
    pub files_deleted: usize,
    pub bytes_deleted: u64,
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
                .is_some_and(|name| name.starts_with(&prefix))
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

pub fn enforce_cleanup(
    conn: &Connection,
    download_root: &Path,
    policy: CleanupPolicy,
    now: i64,
) -> Result<CleanupSummary, CleanupError> {
    let candidates = cleanup_candidates(conn, policy, now)?;
    let mut summary = CleanupSummary::default();
    for (episode_id, path) in candidates {
        let path = PathBuf::from(path);
        let canonical_path = match path.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                super::store::set_downloaded_path(conn, episode_id, None)?;
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let canonical_root = download_root.canonicalize()?;
        if !canonical_path.starts_with(canonical_root) {
            return Err(CleanupError::OutsideDownloadRoot);
        }
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
    #[error("download cleanup path is outside the configured root")]
    OutsideDownloadRoot,
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
    fn keep_all_never_deletes_downloads() {
        let conn = conn();
        let directory = tempfile::tempdir().unwrap();
        let show = add_show(&conn);
        let episode = add_download(&conn, directory.path(), show, 1, Some(1));

        assert_eq!(
            enforce_cleanup(&conn, directory.path(), CleanupPolicy::KeepAll, 1_000_000,).unwrap(),
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

        let summary = enforce_cleanup(
            &conn,
            directory.path(),
            CleanupPolicy::DeletePlayedAfter7Days,
            now,
        )
        .unwrap();

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
    fn cleanup_never_deletes_a_download_path_outside_its_root() {
        let conn = conn();
        let download_root = tempfile::tempdir().unwrap();
        let foreign_directory = tempfile::tempdir().unwrap();
        let show = add_show(&conn);
        let now = 1_000_000;
        let episode = add_download(
            &conn,
            download_root.path(),
            show,
            1,
            Some(now - PLAYED_RETENTION_SECONDS),
        );
        let foreign_path = foreign_directory.path().join("library-track.flac");
        std::fs::write(&foreign_path, b"must survive").unwrap();
        store::set_downloaded_path(&conn, episode, foreign_path.to_str()).unwrap();

        let result = enforce_cleanup(
            &conn,
            download_root.path(),
            CleanupPolicy::DeletePlayedAfter7Days,
            now,
        );

        assert!(result.is_err());
        assert_eq!(std::fs::read(&foreign_path).unwrap(), b"must survive");
        assert_eq!(
            store::episode(&conn, episode)
                .unwrap()
                .unwrap()
                .downloaded_path
                .as_deref(),
            foreign_path.to_str()
        );
    }

    #[test]
    fn keep_last_five_is_applied_per_show() {
        let conn = conn();
        let directory = tempfile::tempdir().unwrap();
        let show = add_show(&conn);
        for number in 1..=7 {
            add_download(&conn, directory.path(), show, number, None);
        }

        let summary =
            enforce_cleanup(&conn, directory.path(), CleanupPolicy::KeepLast5, 0).unwrap();

        assert_eq!(summary.files_deleted, 2);
        let remaining = super::super::query::episodes_for_subscription(&conn, show)
            .unwrap()
            .into_iter()
            .filter(|episode| episode.downloaded_path.is_some())
            .count();
        assert_eq!(remaining, 5);
    }
}
