//! Deterministic podcast download storage and cleanup.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use super::config::CleanupPolicy;
use super::{PodcastError, PodcastKind};

const PLAYED_RETENTION_SECONDS: i64 = 7 * 24 * 60 * 60;

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

#[must_use]
pub fn extension_for(kind: PodcastKind, audio_url: &str) -> &'static str {
    match kind {
        PodcastKind::Rss => extension_from_url(audio_url),
        PodcastKind::Youtube => "opus",
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

/// `POD-5`: resolves one channel's effective "keep N downloaded" against the
/// global default (`podcasts::config::PodcastConfig::keep_downloaded_default`).
/// `None` (no persisted override for this channel) falls back to the
/// default; an explicit override — including `0` — always wins, because `0`
/// means unlimited for every numeric sync/cleanup setting since the owner
/// decision of 2026-07-29 (`E-9`). Pure, same shape as `device_sync::
/// selection::resolve_latest_per_channel` (`MTP-36`) — two quantity limits,
/// one mental model, deliberately (`O-5`).
#[must_use]
pub fn resolve_keep_downloaded(default_keep: usize, channel_override: Option<i64>) -> usize {
    match channel_override {
        Some(value) => usize::try_from(value).unwrap_or(0),
        None => default_keep,
    }
}

pub fn enforce_cleanup(
    conn: &Connection,
    download_root: &Path,
    policy: CleanupPolicy,
    default_keep_downloaded: usize,
    now: i64,
) -> Result<CleanupSummary, CleanupError> {
    let candidates = cleanup_candidates(conn, policy, default_keep_downloaded, now)?;
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
    default_keep_downloaded: usize,
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
        // `POD-5` / `O-5`: the per-show cap is no longer a fixed 5 — each
        // subscription's `keep_downloaded` override (`NULL` = no override)
        // is resolved against `default_keep_downloaded` via
        // `resolve_keep_downloaded`, one subscription at a time, because
        // SQLite window functions can't vary the rank cutoff per partition.
        // A resolved value of `0` means unlimited (`E-9`) and is excluded
        // from deletion entirely, never treated as "keep zero".
        CleanupPolicy::KeepLast5 => {
            let mut statement = conn.prepare(
                "SELECT id, downloaded_path, keep_downloaded, episode_rank FROM (
                   SELECT e.id, e.downloaded_path, s.keep_downloaded,
                          ROW_NUMBER() OVER (
                            PARTITION BY e.subscription_id
                            ORDER BY e.published_at IS NULL, e.published_at DESC,
                                     e.first_seen_at DESC, e.id DESC
                          ) AS episode_rank
                   FROM podcast_episodes e
                   JOIN podcast_subscriptions s ON s.id = e.subscription_id
                   WHERE s.removed_at IS NULL
                 )
                 WHERE downloaded_path IS NOT NULL
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?;
            let mut candidates = Vec::new();
            for row in rows {
                let (episode_id, path, keep_override, episode_rank) = row?;
                let keep = resolve_keep_downloaded(default_keep_downloaded, keep_override);
                if keep != 0 && episode_rank > keep as i64 {
                    candidates.push((episode_id, path));
                }
            }
            Ok(candidates)
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
#[path = "downloads_tests.rs"]
mod tests;
