//! `MTP-37`: the Content section's size cap (`SyncTarget::cap_bytes`,
//! `MTP-38`) becomes editable via `DeviceSyncRuntime::set_target_cap`.
//! `MTP-39`/`MTP-25` already prove the pure eviction logic reacts to a cap;
//! what was missing until this rule was any way for a user to actually set
//! one. This test proves the new setter is genuinely wired into the live
//! plan, not merely persisted — the same failure mode as the three
//! render-but-never-read switches this branch shipped earlier.

use super::*;
use reprise_core::device_sync::{CategoryDiff, CategoryReading};

fn insert_show(db: &Db, id: i64) {
    crate::test_db::connection(db)
        .execute(
            "INSERT INTO podcast_subscriptions
         (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
         VALUES (?1, 'rss', 'https://example.test/show', 'Show', 0, 0, 1)",
            rusqlite::params![id],
        )
        .unwrap();
}

fn insert_episode(db: &Db, id: i64, subscription_id: i64, path: &std::path::Path) {
    crate::test_db::connection(db)
        .execute(
            "INSERT INTO podcast_episodes
         (id, subscription_id, guid, title, audio_url, downloaded_path,
          downloaded_bytes, first_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 5, 1)",
            rusqlite::params![
                id,
                subscription_id,
                format!("episode-{id}"),
                format!("Episode {id}"),
                format!("https://example.test/{id}.mp3"),
                path.to_string_lossy(),
            ],
        )
        .unwrap();
}

fn set_mtime(path: &std::path::Path, when: std::time::SystemTime) {
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(when)
        .unwrap();
}

fn podcast_files_to_copy(runtime: &Rc<DeviceSyncRuntime>, device_id: &str) -> usize {
    let device = runtime
        .devices()
        .into_iter()
        .find(|device| device.id == device_id)
        .unwrap_or_else(|| panic!("device {device_id} not found"));
    match device.category_readings[2] {
        CategoryReading::Diff(CategoryDiff { files_to_copy, .. }) => files_to_copy,
        other => panic!("expected a computed diff for podcast episodes, got {other:?}"),
    }
}

#[test]
fn mtp_37_lowering_the_podcast_cap_evicts_the_older_episode_from_the_next_plan() {
    run(async {
        let (downloads, conn) = fixture();
        let older = downloads.path().join("older.mp3");
        let newer = downloads.path().join("newer.mp3");
        std::fs::write(&older, b"aaaaa").unwrap();
        std::fs::write(&newer, b"bbbbb").unwrap();
        // The eviction order is by source file mtime, oldest first — set
        // `older`'s mtime a full day behind `newer`'s so ordering cannot
        // flake on filesystem timestamp granularity.
        let now = std::time::SystemTime::now();
        set_mtime(&older, now - std::time::Duration::from_secs(86_400));
        set_mtime(&newer, now);

        insert_show(&conn, 30);
        insert_episode(&conn, 300, 30, &older);
        insert_episode(&conn, 301, 30, &newer);
        disable_auto_start(&conn, "a");
        reprise_core::podcasts::phone_sync::set_device_enabled(&conn, 30, "a", true).unwrap();

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        assert_eq!(
            podcast_files_to_copy(&runtime, "a"),
            2,
            "both episodes fit comfortably under the default 4 GiB cap"
        );

        // Only one 5-byte episode fits under a 5-byte cap — the older one
        // must be evicted from the desired set before the diff is computed
        // (`MTP-25`).
        runtime
            .set_target_cap(
                "a",
                reprise_core::device_sync::SyncTargetKind::PodcastEpisodes,
                Some(5),
            )
            .unwrap();
        settle().await;

        assert_eq!(
            podcast_files_to_copy(&runtime, "a"),
            1,
            "lowering the cap via set_target_cap must actually change the next sync plan — \
             a cap that is stored but never enforced would leave this at 2"
        );

        // Raising the cap back up must restore the second episode — proves
        // the wiring reacts to the *current* persisted value on every
        // recompute, not just once at connect time.
        runtime
            .set_target_cap(
                "a",
                reprise_core::device_sync::SyncTargetKind::PodcastEpisodes,
                None,
            )
            .unwrap();
        settle().await;

        assert_eq!(
            podcast_files_to_copy(&runtime, "a"),
            2,
            "clearing the cap must restore both episodes to the plan"
        );
    });
}
