//! Pure Android planning for explicitly selected RSS podcast downloads.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::Connection;

use super::safe_component;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PodcastSyncSource {
    Rss,
    Youtube,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PodcastSyncCandidate {
    pub episode_id: i64,
    pub source: PodcastSyncSource,
    pub source_path: PathBuf,
    pub device_path: String,
    pub title: String,
    pub show: String,
    pub size_bytes: u64,
    pub source_mtime: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PodcastDeviceFile {
    pub device_path: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PodcastSyncPlan {
    pub selected: usize,
    pub to_copy: Vec<PodcastSyncCandidate>,
    pub to_remove: Vec<String>,
    pub bytes: u64,
}

pub fn query_candidates(conn: &Connection) -> Result<Vec<PodcastSyncCandidate>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT e.id, e.title, s.title, e.downloaded_path, e.downloaded_bytes
         FROM podcast_episodes e
         JOIN podcast_subscriptions s ON s.id = e.subscription_id
         WHERE s.removed_at IS NULL
           AND s.kind = 'rss'
           AND s.sync_to_phone = 1
           AND e.removed_at IS NULL
           AND e.downloaded_path IS NOT NULL
         ORDER BY s.title COLLATE NOCASE, e.published_at DESC, e.id DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            PathBuf::from(row.get::<_, String>(3)?),
            row.get::<_, Option<i64>>(4)?,
        ))
    })?;
    let rows = rows.collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let mut candidates = Vec::new();
    for row in rows {
        let (episode_id, title, show, source_path, recorded_bytes) = row;
        let Ok(metadata) = std::fs::metadata(&source_path) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() == 0 {
            continue;
        }
        let recorded_bytes = match recorded_bytes {
            Some(bytes) => {
                let Ok(bytes) = u64::try_from(bytes) else {
                    continue;
                };
                if metadata.len() != bytes {
                    continue;
                }
                bytes
            }
            None => {
                let bytes = metadata.len();
                crate::podcasts::store::set_downloaded_file(
                    conn,
                    episode_id,
                    source_path.to_str(),
                    Some(bytes.min(i64::MAX as u64) as i64),
                )?;
                bytes
            }
        };
        candidates.push(PodcastSyncCandidate {
            episode_id,
            source: PodcastSyncSource::Rss,
            device_path: device_path(episode_id, &show, &title, &source_path),
            title,
            show,
            size_bytes: recorded_bytes,
            source_mtime: metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .and_then(|duration| i64::try_from(duration.as_secs()).ok())
                .unwrap_or(0),
            source_path,
        });
    }
    Ok(candidates)
}

pub fn build_plan(
    candidates: Vec<PodcastSyncCandidate>,
    inventory: &[PodcastDeviceFile],
    remove_deleted: bool,
) -> PodcastSyncPlan {
    let candidates = candidates
        .into_iter()
        .filter(|candidate| {
            candidate.source == PodcastSyncSource::Rss && safe_relative_path(&candidate.device_path)
        })
        .collect::<Vec<_>>();
    let desired = candidates
        .iter()
        .map(|candidate| candidate.device_path.clone())
        .collect::<std::collections::HashSet<_>>();
    let existing = inventory
        .iter()
        .filter(|file| safe_relative_path(&file.device_path))
        .map(|file| (file.device_path.as_str(), file.size_bytes))
        .collect::<std::collections::HashMap<_, _>>();
    let to_copy = candidates
        .into_iter()
        .filter(|candidate| {
            existing.get(candidate.device_path.as_str()).copied() != Some(candidate.size_bytes)
        })
        .collect::<Vec<_>>();
    let bytes = to_copy.iter().map(|candidate| candidate.size_bytes).sum();
    let to_remove = if remove_deleted {
        inventory
            .iter()
            .filter(|file| {
                safe_relative_path(&file.device_path)
                    && !desired.contains(file.device_path.as_str())
            })
            .map(|file| file.device_path.clone())
            .collect()
    } else {
        Vec::new()
    };
    PodcastSyncPlan {
        selected: desired.len(),
        to_copy,
        to_remove,
        bytes,
    }
}

pub fn safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.chars().any(char::is_control)
        && std::path::Path::new(path)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn device_path(episode_id: i64, show: &str, title: &str, source: &Path) -> String {
    let show = safe_component(show, "Unknown Podcast");
    let title = safe_component(title, "Untitled Episode");
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| {
            !extension.is_empty()
                && extension.len() <= 8
                && extension
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
        .map(|extension| format!(".{}", extension.to_ascii_lowercase()))
        .unwrap_or_default();
    format!("{show}/{episode_id}-{title}{extension}")
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;

    fn migrated() -> rusqlite::Connection {
        crate::db::open_migrated(None).unwrap()
    }

    fn insert_subscription(
        conn: &rusqlite::Connection,
        id: i64,
        kind: &str,
        sync_to_phone: bool,
        title: &str,
    ) {
        conn.execute(
            "INSERT INTO podcast_subscriptions
             (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, 1)",
            params![
                id,
                kind,
                format!("https://example.test/{id}"),
                title,
                sync_to_phone
            ],
        )
        .unwrap();
    }

    fn insert_episode(
        conn: &rusqlite::Connection,
        id: i64,
        subscription_id: i64,
        path: &std::path::Path,
        downloaded_bytes: i64,
    ) {
        conn.execute(
            "INSERT INTO podcast_episodes
             (id, subscription_id, guid, title, audio_url, downloaded_path,
              downloaded_bytes, first_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
            params![
                id,
                subscription_id,
                format!("episode-{id}"),
                format!("Episode {id}: /?*"),
                format!("https://example.test/{id}.mp3"),
                path.to_string_lossy(),
                downloaded_bytes
            ],
        )
        .unwrap();
    }

    #[test]
    fn pod_8_candidates_are_complete_selected_rss_downloads_only() {
        let conn = migrated();
        let downloads = tempfile::tempdir().unwrap();
        let eligible = downloads.path().join("eligible.mp3");
        let youtube = downloads.path().join("youtube.mp3");
        let unselected = downloads.path().join("unselected.mp3");
        let partial = downloads.path().join("partial.mp3");
        std::fs::write(&eligible, b"complete").unwrap();
        std::fs::write(&youtube, b"youtube").unwrap();
        std::fs::write(&unselected, b"unselected").unwrap();
        std::fs::write(&partial, b"short").unwrap();

        insert_subscription(&conn, 1, "rss", true, "Show: One / Daily");
        insert_subscription(&conn, 2, "youtube", true, "Video Channel");
        insert_subscription(&conn, 3, "rss", false, "Other Show");
        insert_episode(&conn, 11, 1, &eligible, 8);
        insert_episode(&conn, 12, 2, &youtube, 7);
        insert_episode(&conn, 13, 3, &unselected, 10);
        insert_episode(&conn, 14, 1, &partial, 99);
        insert_episode(&conn, 15, 1, &eligible, 8);
        conn.execute(
            "UPDATE podcast_episodes SET removed_at = 1 WHERE id = 15",
            [],
        )
        .unwrap();

        let candidates = super::query_candidates(&conn).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].episode_id, 11);
        assert_eq!(candidates[0].source_path, eligible);
        assert_eq!(candidates[0].size_bytes, 8);
        assert_eq!(
            candidates[0].device_path,
            "Show One Daily/11-Episode 11.mp3"
        );
    }

    #[test]
    fn pod_8_legacy_downloads_backfill_size_before_phone_sync() {
        let conn = migrated();
        let downloads = tempfile::tempdir().unwrap();
        let legacy = downloads.path().join("legacy.mp3");
        std::fs::write(&legacy, b"legacy").unwrap();
        insert_subscription(&conn, 1, "rss", true, "Legacy Show");
        insert_episode(&conn, 11, 1, &legacy, 6);
        conn.execute(
            "UPDATE podcast_episodes SET downloaded_bytes = NULL WHERE id = 11",
            [],
        )
        .unwrap();

        let candidates = query_candidates(&conn).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].size_bytes, 6);
        assert_eq!(
            crate::podcasts::store::episode(&conn, 11)
                .unwrap()
                .unwrap()
                .downloaded_bytes,
            Some(6)
        );
    }

    #[test]
    fn pod_8_plan_copies_and_removes_only_inside_the_rss_podcast_tree() {
        let source = PodcastSyncCandidate {
            episode_id: 1,
            source: PodcastSyncSource::Rss,
            source_path: "/downloads/one.mp3".into(),
            device_path: "Show/1-One.mp3".into(),
            title: "One".into(),
            show: "Show".into(),
            size_bytes: 100,
            source_mtime: 1,
        };
        let youtube = PodcastSyncCandidate {
            source: PodcastSyncSource::Youtube,
            device_path: "Channel/2-Video.webm".into(),
            ..source.clone()
        };
        let inventory = vec![
            PodcastDeviceFile {
                device_path: source.device_path.clone(),
                size_bytes: 50,
            },
            PodcastDeviceFile {
                device_path: "Old/9-Old.mp3".into(),
                size_bytes: 20,
            },
            PodcastDeviceFile {
                device_path: "../Music/Reprise/Album/track.mp3".into(),
                size_bytes: 20,
            },
            PodcastDeviceFile {
                device_path: "/Podcasts/Other App/keep.mp3".into(),
                size_bytes: 20,
            },
        ];

        let plan = build_plan(vec![source.clone(), youtube], &inventory, true);

        assert_eq!(plan.to_copy, [source]);
        assert_eq!(plan.to_remove, ["Old/9-Old.mp3".to_string()]);
        assert_eq!(plan.bytes, 100);
    }

    #[test]
    fn pod_8_managed_paths_reject_absolute_parent_and_control_components() {
        assert!(safe_relative_path("Show/1-Episode.mp3"));
        assert!(!safe_relative_path("../Music/Reprise/track.mp3"));
        assert!(!safe_relative_path("Show/../../track.mp3"));
        assert!(!safe_relative_path("/Podcasts/Reprise/track.mp3"));
        assert!(!safe_relative_path("Show/\ntrack.mp3"));
    }
}
