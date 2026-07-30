//! `podcasts.rs`'s tests, split out only to keep that file under the
//! project's 800-line rule — the same reason
//! [`super::query_selection_candidates_for_device`] lives in its own
//! sibling. Declared inside `podcasts.rs`, so `super` is still that
//! module and every test reads exactly as it did inline.

use rusqlite::params;

use super::*;

/// Both source modules ship **off** (`NET-1a`), and `MTP-46` makes an off
/// module contribute nothing. Every test below is about what a device
/// receives once the user actually uses these features, so switching them
/// on is their precondition, not their subject — `MTP-46`'s own tests are
/// the ones that flip them back.
fn migrated() -> crate::db::Db {
    let db = crate::db::Db::open_in_memory().unwrap();
    crate::modules::set_enabled(&db, &crate::modules::PODCASTS_MODULE, true).unwrap();
    crate::modules::set_enabled(&db, &crate::modules::YOUTUBE_MODULE, true).unwrap();
    db
}

fn insert_subscription(db: &crate::db::Db, id: i64, kind: &str, sync_to_phone: bool, title: &str) {
    let conn = db.conn();
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
    db: &crate::db::Db,
    id: i64,
    subscription_id: i64,
    path: &std::path::Path,
    downloaded_bytes: i64,
    published_at: Option<i64>,
) {
    let conn = db.conn();
    conn.execute(
        "INSERT INTO podcast_episodes
         (id, subscription_id, guid, title, audio_url, downloaded_path,
          downloaded_bytes, published_at, first_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1)",
        params![
            id,
            subscription_id,
            format!("episode-{id}"),
            format!("Episode {id}: /?*"),
            format!("https://example.test/{id}.mp3"),
            path.to_string_lossy(),
            downloaded_bytes,
            published_at
        ],
    )
    .unwrap();
}

#[test]
fn pod_12_candidates_are_complete_and_explicitly_selected_for_the_device() {
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
    crate::podcasts::phone_sync::set_device_enabled(&conn, 1, "mtp:pixel", true).unwrap();
    crate::podcasts::phone_sync::set_device_enabled(&conn, 3, "mtp:tablet", true).unwrap();
    insert_episode(&conn, 11, 1, &eligible, 8, Some(1_785_225_600));
    insert_episode(&conn, 12, 2, &youtube, 7, None);
    insert_episode(&conn, 13, 3, &unselected, 10, None);
    insert_episode(&conn, 14, 1, &partial, 99, None);
    insert_episode(&conn, 15, 1, &eligible, 8, None);
    conn.conn()
        .execute(
            "UPDATE podcast_episodes SET removed_at = 1 WHERE id = 15",
            [],
        )
        .unwrap();

    let candidates = super::query_candidates_for_device(&conn, "mtp:pixel").unwrap();

    // Episode 12 belongs to a YouTube subscription that was never
    // explicitly selected for "mtp:pixel" (only subscription 1 was), so
    // it stays out — not because of its kind, but because selection is
    // per subscription and per device (POD-12), same as RSS.
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].episode_id, 11);
    assert_eq!(candidates[0].source, PodcastSyncSource::Rss);
    assert_eq!(candidates[0].source_path, eligible);
    assert_eq!(candidates[0].size_bytes, 8);
    assert_eq!(
        candidates[0].device_path,
        "Show One Daily/2026-07-28 - Episode 11.mp3"
    );
}

#[test]
fn mtp_47_the_device_path_names_the_publication_day_not_the_database_id() {
    let db = migrated();
    let downloads = tempfile::tempdir().unwrap();
    let episode = downloads.path().join("episode.mp3");
    std::fs::write(&episode, b"episode").unwrap();
    insert_subscription(&db, 1, "rss", true, "Show");
    crate::podcasts::phone_sync::set_device_enabled(&db, 1, "mtp:pixel", true).unwrap();
    insert_episode(&db, 11, 1, &episode, 7, Some(1_785_225_600));

    let candidates = query_candidates_for_device(&db, "mtp:pixel").unwrap();

    assert_eq!(
        candidates[0].device_path,
        "Show/2026-07-28 - Episode 11.mp3"
    );
}

#[test]
fn mtp_47_an_episode_without_a_publication_date_uses_the_day_it_was_first_seen() {
    let db = migrated();
    let downloads = tempfile::tempdir().unwrap();
    let episode = downloads.path().join("episode.mp3");
    std::fs::write(&episode, b"episode").unwrap();
    insert_subscription(&db, 1, "rss", true, "Show");
    crate::podcasts::phone_sync::set_device_enabled(&db, 1, "mtp:pixel", true).unwrap();
    insert_episode(&db, 11, 1, &episode, 7, None);
    db.conn()
        .execute(
            "UPDATE podcast_episodes SET first_seen_at = ?2 WHERE id = ?1",
            params![11, 1_785_225_600_i64],
        )
        .unwrap();

    let candidates = query_candidates_for_device(&db, "mtp:pixel").unwrap();

    assert_eq!(
        candidates[0].device_path,
        "Show/2026-07-28 - Episode 11.mp3"
    );
}

#[test]
fn mtp_47_two_episodes_that_would_share_a_name_are_told_apart_by_their_episode_id() {
    let db = migrated();
    let downloads = tempfile::tempdir().unwrap();
    let first = downloads.path().join("first.mp3");
    let second = downloads.path().join("second.mp3");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    insert_subscription(&db, 1, "rss", true, "Show");
    crate::podcasts::phone_sync::set_device_enabled(&db, 1, "mtp:pixel", true).unwrap();
    insert_episode(&db, 11, 1, &first, 5, Some(1_785_225_600));
    insert_episode(&db, 12, 1, &second, 6, Some(1_785_225_600));
    db.conn()
        .execute(
            "UPDATE podcast_episodes SET title = 'Same title' WHERE id = 11",
            [],
        )
        .unwrap();
    db.conn()
        .execute(
            "UPDATE podcast_episodes SET title = 'same TITLE' WHERE id = 12",
            [],
        )
        .unwrap();

    let candidates = query_candidates_for_device(&db, "mtp:pixel").unwrap();

    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().any(|candidate| {
        candidate.episode_id == 11
            && candidate.device_path == "Show/2026-07-28 - Same title [11].mp3"
    }));
    assert!(candidates.iter().any(|candidate| {
        candidate.episode_id == 12
            && candidate.device_path == "Show/2026-07-28 - same TITLE [12].mp3"
    }));
    let plan = build_plan(
        candidates,
        &[],
        true,
        PodcastSyncSource::Rss,
        None,
        EnabledSyncSources {
            rss: true,
            youtube: true,
        },
    );
    assert_eq!(plan.selected, 2);
}

/// `MTP-47`: the disambiguator is appended to the *name*, so uniqueness has
/// to be judged on the composed path and not on the raw title. An episode
/// literally called `Weekly Update [42]` occupies exactly the name episode 42
/// receives the moment a namesake forces it to be disambiguated — and two
/// candidates on one device path do not collide loudly: both are copied, and
/// one silently overwrites the other on the phone.
#[test]
fn mtp_47_a_title_that_already_ends_in_brackets_cannot_take_a_disambiguated_name() {
    let db = migrated();
    let downloads = tempfile::tempdir().unwrap();
    let impostor = downloads.path().join("impostor.mp3");
    let first = downloads.path().join("first.mp3");
    let second = downloads.path().join("second.mp3");
    std::fs::write(&impostor, b"impostor").unwrap();
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    insert_subscription(&db, 1, "rss", true, "Show");
    crate::podcasts::phone_sync::set_device_enabled(&db, 1, "mtp:pixel", true).unwrap();
    insert_episode(&db, 7, 1, &impostor, 8, Some(1_785_225_600));
    insert_episode(&db, 42, 1, &first, 5, Some(1_785_225_600));
    insert_episode(&db, 43, 1, &second, 6, Some(1_785_225_600));
    db.conn()
        .execute(
            "UPDATE podcast_episodes SET title = 'Weekly Update [42]' WHERE id = 7",
            [],
        )
        .unwrap();
    db.conn()
        .execute(
            "UPDATE podcast_episodes SET title = 'Weekly Update' WHERE id IN (42, 43)",
            [],
        )
        .unwrap();

    let candidates = query_candidates_for_device(&db, "mtp:pixel").unwrap();

    let device_path = |episode_id: i64| {
        candidates
            .iter()
            .find(|candidate| candidate.episode_id == episode_id)
            .unwrap()
            .device_path
            .to_lowercase()
    };
    assert_eq!(candidates.len(), 3);
    assert_ne!(
        device_path(7),
        device_path(42),
        "the verbatim title must not land on the disambiguated episode's name"
    );
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.device_path.to_lowercase())
            .collect::<HashSet<_>>()
            .len(),
        3,
        "every episode selected for a device needs its own path, or a sync \
         copies both and keeps whichever landed last"
    );
}

#[test]
fn mtp_47_an_episode_without_a_namesake_carries_no_disambiguator() {
    let db = migrated();
    let downloads = tempfile::tempdir().unwrap();
    let episode = downloads.path().join("episode.mp3");
    std::fs::write(&episode, b"episode").unwrap();
    insert_subscription(&db, 1, "rss", true, "Show");
    crate::podcasts::phone_sync::set_device_enabled(&db, 1, "mtp:pixel", true).unwrap();
    insert_episode(&db, 11, 1, &episode, 7, Some(1_785_225_600));

    let candidates = query_candidates_for_device(&db, "mtp:pixel").unwrap();

    assert_eq!(
        candidates[0].device_path,
        "Show/2026-07-28 - Episode 11.mp3"
    );
    assert!(!candidates[0].device_path.contains('['));
}

#[test]
fn mtp_47_a_very_long_episode_title_stays_within_the_component_byte_limit() {
    let db = migrated();
    let downloads = tempfile::tempdir().unwrap();
    let episode = downloads.path().join("episode.mp3");
    std::fs::write(&episode, b"episode").unwrap();
    insert_subscription(&db, 1, "rss", true, "Show");
    crate::podcasts::phone_sync::set_device_enabled(&db, 1, "mtp:pixel", true).unwrap();
    insert_episode(&db, 11, 1, &episode, 7, Some(1_785_225_600));
    let long_title = "界".repeat(140);
    db.conn()
        .execute(
            "UPDATE podcast_episodes SET title = ?2 WHERE id = ?1",
            params![11, long_title],
        )
        .unwrap();

    let candidates = query_candidates_for_device(&db, "mtp:pixel").unwrap();
    let path = &candidates[0].device_path;
    let (folder, file_name) = path.split_once('/').unwrap();
    let title = file_name
        .strip_suffix(".mp3")
        .unwrap()
        .strip_prefix("2026-07-28 - ")
        .unwrap();

    assert!(folder.len() <= MAX_COMPONENT_BYTES);
    assert!(title.len() <= MAX_COMPONENT_BYTES);
    assert!(std::str::from_utf8(path.as_bytes()).is_ok());
    assert!(path.ends_with(".mp3"));
}

#[test]
fn mtp_47_the_device_path_separator_stays_ascii() {
    let db = migrated();
    let downloads = tempfile::tempdir().unwrap();
    let episode = downloads.path().join("episode.mp3");
    std::fs::write(&episode, b"episode").unwrap();
    insert_subscription(&db, 1, "rss", true, "Show");
    crate::podcasts::phone_sync::set_device_enabled(&db, 1, "mtp:pixel", true).unwrap();
    insert_episode(&db, 11, 1, &episode, 7, Some(1_785_225_600));

    let candidates = query_candidates_for_device(&db, "mtp:pixel").unwrap();

    assert!(candidates[0].device_path.contains(" - "));
    assert!(candidates[0].device_path.is_ascii());
}

#[test]
fn pod_12_a_selected_youtube_subscription_is_queried_just_like_rss() {
    let conn = migrated();
    let downloads = tempfile::tempdir().unwrap();
    let video = downloads.path().join("video.webm");
    std::fs::write(&video, b"video-bytes").unwrap();

    insert_subscription(&conn, 1, "youtube", false, "Channel");
    crate::podcasts::phone_sync::set_device_enabled(&conn, 1, "mtp:pixel", true).unwrap();
    insert_episode(&conn, 11, 1, &video, 11, None);

    let candidates = query_candidates_for_device(&conn, "mtp:pixel").unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].source, PodcastSyncSource::Youtube);
    assert_eq!(candidates[0].episode_id, 11);
}

/// `MTP-46`. Both halves run against one identical database; the switch
/// is the only thing that differs, so the change in the result cannot be
/// attributed to anything else.
#[test]
fn mtp_46_switching_youtube_off_removes_its_episodes_from_the_device_sync() {
    let conn = migrated();
    let downloads = tempfile::tempdir().unwrap();
    let video = downloads.path().join("video.webm");
    let episode = downloads.path().join("episode.mp3");
    std::fs::write(&video, b"video-bytes").unwrap();
    std::fs::write(&episode, b"episode").unwrap();

    insert_subscription(&conn, 1, "youtube", false, "Channel");
    insert_subscription(&conn, 2, "rss", false, "Show");
    crate::podcasts::phone_sync::set_device_enabled(&conn, 1, "mtp:pixel", true).unwrap();
    crate::podcasts::phone_sync::set_device_enabled(&conn, 2, "mtp:pixel", true).unwrap();
    insert_episode(&conn, 11, 1, &video, 11, None);
    insert_episode(&conn, 12, 2, &episode, 7, None);

    let on = query_candidates_for_device(&conn, "mtp:pixel").unwrap();
    assert_eq!(
        on.iter()
            .filter(|c| c.source == PodcastSyncSource::Youtube)
            .count(),
        1,
        "with YouTube on, its selected episode is a candidate"
    );

    crate::modules::set_enabled(&conn, &crate::modules::YOUTUBE_MODULE, false).unwrap();
    let off = query_candidates_for_device(&conn, "mtp:pixel").unwrap();

    assert!(
        off.iter().all(|c| c.source != PodcastSyncSource::Youtube),
        "switching YouTube off must take its episodes out of the sync entirely"
    );
    assert_eq!(
        off.iter()
            .filter(|c| c.source == PodcastSyncSource::Rss)
            .count(),
        1,
        "and must not touch Podcasts, which is a peer module (issue #96)"
    );
    assert_eq!(
        conn.conn()
            .query_row(
                "SELECT COUNT(*) FROM podcast_subscriptions WHERE removed_at IS NULL",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        2,
        "`SET-9` keeps the subscription; only its syncing stops"
    );
}

/// The mirror of the test above, so neither module can be gated by the
/// other's switch.
#[test]
fn mtp_46_switching_podcasts_off_removes_its_episodes_and_leaves_youtube_alone() {
    let conn = migrated();
    let downloads = tempfile::tempdir().unwrap();
    let video = downloads.path().join("video.webm");
    let episode = downloads.path().join("episode.mp3");
    std::fs::write(&video, b"video-bytes").unwrap();
    std::fs::write(&episode, b"episode").unwrap();

    insert_subscription(&conn, 1, "youtube", false, "Channel");
    insert_subscription(&conn, 2, "rss", false, "Show");
    crate::podcasts::phone_sync::set_device_enabled(&conn, 1, "mtp:pixel", true).unwrap();
    crate::podcasts::phone_sync::set_device_enabled(&conn, 2, "mtp:pixel", true).unwrap();
    insert_episode(&conn, 11, 1, &video, 11, None);
    insert_episode(&conn, 12, 2, &episode, 7, None);

    crate::modules::set_enabled(&conn, &crate::modules::PODCASTS_MODULE, false).unwrap();
    let candidates = query_candidates_for_device(&conn, "mtp:pixel").unwrap();

    assert!(
        candidates
            .iter()
            .all(|c| c.source != PodcastSyncSource::Rss),
        "switching Podcasts off must take its episodes out of the sync"
    );
    assert_eq!(
        candidates
            .iter()
            .filter(|c| c.source == PodcastSyncSource::Youtube)
            .count(),
        1,
        "YouTube is a peer, not a child of Podcasts — it stays"
    );
}

/// The global gate sits above both switches: `SET-9` promises "off makes
/// this a local player only", and a phone still filling up with feed
/// downloads would not be one.
#[test]
fn mtp_46_the_global_online_sources_gate_empties_the_sync_even_with_both_modules_on() {
    let conn = migrated();
    let downloads = tempfile::tempdir().unwrap();
    let episode = downloads.path().join("episode.mp3");
    std::fs::write(&episode, b"episode").unwrap();

    insert_subscription(&conn, 1, "rss", false, "Show");
    crate::podcasts::phone_sync::set_device_enabled(&conn, 1, "mtp:pixel", true).unwrap();
    insert_episode(&conn, 11, 1, &episode, 7, None);

    assert_eq!(
        query_candidates_for_device(&conn, "mtp:pixel")
            .unwrap()
            .len(),
        1
    );

    crate::online_sources::set_enabled(&conn, false).unwrap();

    assert!(
        query_candidates_for_device(&conn, "mtp:pixel")
            .unwrap()
            .is_empty(),
        "the global gate must empty the sync regardless of the module switches"
    );
}

#[test]
fn pod_12_legacy_downloads_backfill_size_before_phone_sync() {
    let conn = migrated();
    let downloads = tempfile::tempdir().unwrap();
    let legacy = downloads.path().join("legacy.mp3");
    std::fs::write(&legacy, b"legacy").unwrap();
    insert_subscription(&conn, 1, "rss", true, "Legacy Show");
    crate::podcasts::phone_sync::set_device_enabled(&conn, 1, "mtp:pixel", true).unwrap();
    insert_episode(&conn, 11, 1, &legacy, 6, None);
    conn.conn()
        .execute(
            "UPDATE podcast_episodes SET downloaded_bytes = NULL WHERE id = 11",
            [],
        )
        .unwrap();

    let candidates = query_candidates_for_device(&conn, "mtp:pixel").unwrap();

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
fn pod_12_plan_copies_and_removes_only_inside_the_rss_podcast_tree() {
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

    let plan = build_plan(
        vec![source.clone(), youtube],
        &inventory,
        true,
        PodcastSyncSource::Rss,
        None,
        EnabledSyncSources {
            rss: true,
            youtube: true,
        },
    );

    assert_eq!(plan.to_copy, [source]);
    assert_eq!(plan.to_remove, ["Old/9-Old.mp3".to_string()]);
    assert_eq!(plan.bytes, 100);
    assert_eq!(
        plan.bytes_freed, 20,
        "bytes_freed sums the removed inventory files, not the whole inventory"
    );
}

#[test]
fn mtp_45_youtube_source_selects_youtube_candidates_the_same_way_rss_does() {
    let rss = PodcastSyncCandidate {
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
        size_bytes: 200,
        ..rss.clone()
    };

    let plan = build_plan(
        vec![rss, youtube.clone()],
        &[],
        true,
        PodcastSyncSource::Youtube,
        None,
        EnabledSyncSources {
            rss: true,
            youtube: true,
        },
    );

    assert_eq!(plan.to_copy, [youtube]);
    assert_eq!(plan.bytes, 200);
}

#[test]
fn mtp_25_a_cap_evicts_the_oldest_candidates_before_copying_or_removing() {
    let old = PodcastSyncCandidate {
        episode_id: 1,
        source: PodcastSyncSource::Youtube,
        source_path: "/downloads/old.webm".into(),
        device_path: "Channel/1-Old.webm".into(),
        title: "Old".into(),
        show: "Channel".into(),
        size_bytes: 60,
        source_mtime: 1,
    };
    let newer = PodcastSyncCandidate {
        episode_id: 2,
        device_path: "Channel/2-Newer.webm".into(),
        title: "Newer".into(),
        size_bytes: 60,
        source_mtime: 2,
        ..old.clone()
    };
    // `old` is already resident on the device; `newer` is not yet
    // copied. A 60-byte cap only has room for one of them, so `old`
    // (the smaller age) must leave — evicted before copying, not
    // copied then immediately evicted.
    let inventory = vec![PodcastDeviceFile {
        device_path: old.device_path.clone(),
        size_bytes: 60,
    }];

    let plan = build_plan(
        vec![old.clone(), newer.clone()],
        &inventory,
        true,
        PodcastSyncSource::Youtube,
        Some(60),
        EnabledSyncSources {
            rss: true,
            youtube: true,
        },
    );

    assert_eq!(plan.to_copy, [newer]);
    assert_eq!(plan.to_remove, [old.device_path]);
    assert_eq!(plan.bytes, 60);
    assert_eq!(plan.bytes_freed, 60);
}

/// `MTP-46`, the destructive reading it exists to rule out. Gating only
/// the candidate query leaves `build_plan` with an empty desired set,
/// and an empty desired set means "remove everything" — so switching
/// YouTube off would have wiped every YouTube file off the phone on the
/// next sync with `remove_deleted` on. Both halves of this test run the
/// same inventory through the same call; only the switch differs.
#[test]
fn mtp_46_switching_a_source_off_never_deletes_what_is_already_on_the_phone() {
    let resident = PodcastDeviceFile {
        device_path: "Channel/11-Video.webm".into(),
        size_bytes: 40,
    };

    // Off: nothing is copied *and* nothing is removed.
    let off = build_plan(
        Vec::new(),
        std::slice::from_ref(&resident),
        true,
        PodcastSyncSource::Youtube,
        None,
        EnabledSyncSources {
            rss: true,
            youtube: false,
        },
    );
    assert!(
        off.to_remove.is_empty(),
        "switching YouTube off must not delete what it already put on the phone"
    );
    assert!(off.to_copy.is_empty());
    assert_eq!(off.bytes_freed, 0);

    // On, with the episode genuinely gone from the library: the ordinary
    // `remove_deleted` cleanup still works. Without this half the test
    // would also pass if the removal path had simply been deleted.
    let on = build_plan(
        Vec::new(),
        std::slice::from_ref(&resident),
        true,
        PodcastSyncSource::Youtube,
        None,
        EnabledSyncSources {
            rss: true,
            youtube: true,
        },
    );
    assert_eq!(
        on.to_remove,
        [resident.device_path],
        "an unsubscribed episode of an *enabled* source is still removed"
    );
    assert_eq!(on.bytes_freed, 40);
}

#[test]
fn mtp_25_a_cap_that_already_fits_evicts_nothing() {
    let candidate = PodcastSyncCandidate {
        episode_id: 1,
        source: PodcastSyncSource::Rss,
        source_path: "/downloads/one.mp3".into(),
        device_path: "Show/1-One.mp3".into(),
        title: "One".into(),
        show: "Show".into(),
        size_bytes: 10,
        source_mtime: 1,
    };

    let plan = build_plan(
        vec![candidate.clone()],
        &[],
        true,
        PodcastSyncSource::Rss,
        Some(100),
        EnabledSyncSources {
            rss: true,
            youtube: true,
        },
    );

    assert_eq!(plan.to_copy, [candidate]);
    assert!(plan.to_remove.is_empty());
}

#[test]
fn pod_12_each_device_receives_only_its_selected_subscriptions() {
    let conn = migrated();
    let downloads = tempfile::tempdir().unwrap();
    let phone_episode = downloads.path().join("phone.mp3");
    let tablet_episode = downloads.path().join("tablet.mp3");
    std::fs::write(&phone_episode, b"phone").unwrap();
    std::fs::write(&tablet_episode, b"tablet").unwrap();
    insert_subscription(&conn, 1, "rss", false, "Phone Show");
    insert_subscription(&conn, 2, "rss", false, "Tablet Show");
    insert_episode(&conn, 11, 1, &phone_episode, 5, None);
    insert_episode(&conn, 12, 2, &tablet_episode, 6, None);
    crate::podcasts::phone_sync::set_device_enabled(&conn, 1, "mtp:phone", true).unwrap();
    crate::podcasts::phone_sync::set_device_enabled(&conn, 2, "mtp:tablet", true).unwrap();

    let phone = query_candidates_for_device(&conn, "mtp:phone").unwrap();
    let tablet = query_candidates_for_device(&conn, "mtp:tablet").unwrap();
    let unselected = query_candidates_for_device(&conn, "mtp:other").unwrap();

    assert_eq!(
        phone
            .iter()
            .map(|episode| episode.episode_id)
            .collect::<Vec<_>>(),
        [11]
    );
    assert_eq!(
        tablet
            .iter()
            .map(|episode| episode.episode_id)
            .collect::<Vec<_>>(),
        [12]
    );
    assert!(unselected.is_empty());
}

#[test]
fn pod_12_managed_paths_reject_absolute_parent_and_control_components() {
    assert!(safe_relative_path("Show/1-Episode.mp3"));
    assert!(safe_relative_path("Show/2026-07-28 - Episode.mp3"));
    assert!(!safe_relative_path("../Music/Reprise/track.mp3"));
    assert!(!safe_relative_path("Show/../../track.mp3"));
    assert!(!safe_relative_path("/Podcasts/Reprise/track.mp3"));
    assert!(!safe_relative_path("Show/\ntrack.mp3"));
}
