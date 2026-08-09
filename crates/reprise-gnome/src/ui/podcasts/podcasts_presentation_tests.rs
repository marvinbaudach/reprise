//! Tests for the pure podcast projection in `podcasts_presentation`.
//! Split into their own file so that module stays under the
//! repository's 800-line source-size gate.

use super::*;

fn row(id: i64, published_at: Option<i64>, kind: PodcastKind) -> EpisodeRow {
    EpisodeRow {
        id,
        subscription_id: 1,
        guid: format!("g{id}"),
        title: format!("Episode {id}"),
        show: if id == 3 { "Other" } else { "Show" }.into(),
        show_image_url: None,
        image_url: None,
        kind,
        audio_url: "https://example.test/episode.mp3".into(),
        page_url: None,
        published_at,
        duration_secs: Some(4_533),
        downloaded_path: None,
        downloaded_bytes: None,
        played_at: None,
        position_ms: 0,
        first_seen_at: id,
        is_new: false,
        media_category: None,
    }
}

#[test]
fn pod_9_presentation_formats_date_length_source_and_status() {
    let today = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();
    let today_timestamp = today.and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp();
    assert_eq!(relative_date(Some(today_timestamp), today), "Today");
    assert_eq!(
        relative_date(Some(today_timestamp - 86_400), today),
        "Yesterday"
    );
    assert_eq!(duration(Some(4_533)), "1 h 15");
    assert_eq!(file_size(Some(41_943_040)), Some("40.0 MB".to_owned()));
    assert_eq!(source_pill(PodcastKind::Rss).label, "RSS");
    let mut episode = row(1, Some(today_timestamp), PodcastKind::Rss);
    assert_eq!(status_pill(&episode), None);
    episode.is_new = true;
    assert_eq!(status_pill(&episode).map(|pill| pill.label), Some("New"));
    episode.position_ms = 10;
    assert_eq!(status_pill(&episode).map(|pill| pill.label), Some("Resume"));
    episode.played_at = Some(1);
    assert_eq!(status_pill(&episode).map(|pill| pill.label), Some("Played"));
}

#[test]
fn duration_uses_unambiguous_minute_and_hour_boundaries() {
    let cases = [
        (None, ""),
        (Some(-1), ""),
        (Some(0), "< 1 min"),
        (Some(59), "< 1 min"),
        (Some(60), "1 min"),
        (Some(3_599), "59 min"),
        (Some(3_600), "1 h 00"),
        (Some(7_500), "2 h 05"),
    ];

    for (value, expected) in cases {
        assert_eq!(duration(value), expected, "duration {value:?}");
    }
}

#[test]
fn file_size_omits_unknown_zero_and_negative_values() {
    assert_eq!(file_size(None), None);
    assert_eq!(file_size(Some(-1)), None);
    assert_eq!(file_size(Some(0)), None);
    assert_eq!(file_size(Some(1_048_576)), Some("1.0 MB".to_owned()));
    assert_eq!(file_size(Some(1_073_741_824)), Some("1.0 GB".to_owned()));
}

#[test]
fn missing_dates_and_detail_parts_render_no_placeholders_or_empty_separators() {
    let today = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();

    assert_eq!(relative_date(None, today), "");
    assert_eq!(
        crate::ui::source_row::detail_line(["", "", strings::PODCAST_STATUS_NEW]),
        strings::PODCAST_STATUS_NEW
    );
    assert_eq!(
        crate::ui::source_row::detail_line(["Today", "", strings::PODCAST_STATUS_NEW]),
        "Today · New"
    );
    assert_eq!(
        strings::podcast_group_facts("15 episodes", 0, "", ""),
        "15 episodes · 0 new"
    );
}

#[test]
fn author_line_hides_title_prefixes_but_keeps_distinct_publishers() {
    assert_eq!(author_line("The Daily", Some("The Daily")), None);
    assert_eq!(
        author_line("The Daily – News Briefing", Some("The Daily")),
        None
    );
    assert_eq!(
        author_line("The Daily", Some("The New York Times")),
        Some("The New York Times")
    );
    assert_eq!(author_line("Artist Notes", Some("Art")), Some("Art"));
    assert_eq!(author_line("Show", Some("   ")), None);
}

#[test]
fn src_5_youtube_header_has_one_channel_name_while_rss_keeps_its_author() {
    assert_eq!(
        source_header(PodcastKind::Youtube, "Ferris Media", Some("Ferris Media")),
        SourceHeader {
            title: "Ferris Media",
            subtitle: None,
        }
    );
    assert_eq!(
        source_header(PodcastKind::Rss, "Systems Weekly", Some("Ada Lovelace")),
        SourceHeader {
            title: "Systems Weekly",
            subtitle: Some("Ada Lovelace"),
        }
    );
}

#[test]
fn filtering_composes_unplayed_downloaded_and_source() {
    let mut rows = vec![
        row(1, Some(10), PodcastKind::Rss),
        row(2, Some(20), PodcastKind::Youtube),
        row(3, Some(30), PodcastKind::Rss),
    ];
    rows[0].played_at = Some(100);
    rows[1].downloaded_path = Some("/music/ep2.mp3".into());
    let filtered = apply_filter(
        &rows,
        &PodcastFilter {
            unplayed_only: true,
            source: Some(PodcastKind::Youtube),
            downloaded_only: true,
            ..PodcastFilter::default()
        },
    );
    assert_eq!(filtered.iter().map(|row| row.id).collect::<Vec<_>>(), [2]);
}

/// `SRC-10` addendum (Block B2): the "Downloaded" filter matches only
/// episodes with a file on disk — would go red if `downloaded_only`
/// were ignored, since one row here has no `downloaded_path` at all.
#[test]
fn src_10_downloaded_only_filter_matches_files_on_disk_not_download_state() {
    let mut on_disk = row(1, Some(10), PodcastKind::Rss);
    on_disk.downloaded_path = Some("/music/ep1.mp3".into());
    let not_downloaded = row(2, Some(20), PodcastKind::Rss);
    let rows = vec![on_disk, not_downloaded];

    let filtered = apply_filter(
        &rows,
        &PodcastFilter {
            downloaded_only: true,
            ..PodcastFilter::default()
        },
    );

    assert_eq!(filtered.iter().map(|row| row.id).collect::<Vec<_>>(), [1]);
    assert!(active(&PodcastFilter {
        downloaded_only: true,
        ..PodcastFilter::default()
    }));
}

fn titled(id: i64, title: &str) -> EpisodeRow {
    EpisodeRow {
        title: title.to_owned(),
        ..row(id, Some(id * 10), PodcastKind::Rss)
    }
}

fn show(subscription_id: i64, title: &str, episodes: Vec<EpisodeRow>) -> SourceGroup {
    SourceGroup {
        subscription_id,
        title: title.into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Rss,
        sync_to_phone: false,
        episodes,
    }
}

/// UX POD-25: the query matches episode titles case-insensitively and
/// mid-word, renders only the matching episodes, drops shows without a
/// match, and auto-expands what is left. The per-group facts line keeps
/// the unfiltered counts (POD-9 / G2).
#[test]
fn pod_25_episode_title_query_narrows_shows_without_touching_their_facts() {
    let werkbank = show(
        1,
        "Werkbank",
        vec![
            titled(1, "Antwerpen: Wie ein Hafen wirklich funktioniert"),
            titled(2, "Werkzeuge, die wir viel zu selten benutzen"),
            titled(3, "Ein Nachmittag ohne Strom"),
        ],
    );
    let feldbericht = show(
        2,
        "Feldbericht",
        vec![titled(4, "Kartierung im Moor"), titled(5, "Nachtschicht")],
    );
    let filter = PodcastFilter {
        query: "wer".into(),
        ..PodcastFilter::default()
    };

    let rendered =
        rendered_source_groups(&[werkbank.clone(), feldbericht], &filter, &BTreeMap::new());

    assert_eq!(rendered.len(), 1, "a show without a match drops out");
    assert_eq!(rendered[0].group.title, "Werkbank");
    assert_eq!(
        rendered[0]
            .group
            .episodes
            .iter()
            .map(|episode| episode.id)
            .collect::<Vec<_>>(),
        [1, 2],
        "mid-word (Ant-wer-pen) and leading (Wer-kzeuge) matches, nothing else"
    );
    // POD-9 / G2: the facts line still describes the whole show.
    assert_eq!(rendered[0].summary.episode_count, 3);
    assert!(auto_expand_for_query(&filter.query));
    assert!(active(&filter));
    assert!(!auto_expand_for_query(&PodcastFilter::default().query));
    assert!(!active(&PodcastFilter::default()));
}

/// UX POD-25: the query reads episode titles alone — a show whose *name*
/// matches but whose episodes do not is not a hit.
#[test]
fn pod_25_query_reads_episode_titles_not_show_names() {
    let werkbank = show(1, "Werkbank", vec![titled(1, "Kartierung im Moor")]);

    let rendered = rendered_source_groups(
        &[werkbank],
        &PodcastFilter {
            query: "werk".into(),
            ..PodcastFilter::default()
        },
        &BTreeMap::new(),
    );

    assert!(rendered.is_empty());
}

/// UX POD-25: the query composes with the facet chips instead of
/// replacing them, and a jump to a hidden episode relaxes exactly the
/// facets that hide it — the query included.
#[test]
fn pod_25_query_composes_with_facets_and_relaxes_for_a_jump() {
    let mut played = titled(1, "Werkzeuge");
    played.played_at = Some(99);
    let unplayed = titled(2, "Werkbank am Abend");
    let filter = PodcastFilter {
        unplayed_only: true,
        query: "werk".into(),
        ..PodcastFilter::default()
    };

    let filtered = apply_filter(&[played.clone(), unplayed], &filter);
    assert_eq!(filtered.iter().map(|row| row.id).collect::<Vec<_>>(), [2]);

    // The played episode is hidden by the Unplayed facet alone, so only
    // that facet is relaxed and the typed query survives.
    let relaxed = filter_without_hiding(&played, &filter);
    assert!(!relaxed.unplayed_only);
    assert_eq!(relaxed.query, "werk");

    // An episode the query hides gives its query back instead.
    let elsewhere = titled(3, "Kartierung im Moor");
    let relaxed = filter_without_hiding(&elsewhere, &filter);
    assert_eq!(relaxed.query, "");
}

/// UX SEARCH-8a: the query is transient — it never travels into the
/// persisted facet config, and a restored config starts without one.
#[test]
fn search_8a_the_query_is_never_part_of_the_persisted_facets() {
    let filter = PodcastFilter {
        unplayed_only: true,
        downloaded_only: true,
        query: "wer".into(),
        ..PodcastFilter::default()
    };

    let facets = filter.facets();

    assert!(facets.unplayed_only);
    assert!(facets.downloaded_only);
    assert_eq!(PodcastFilter::from_facets(&facets).query, "");
    assert_eq!(filter.with_query("  neu  ").query, "neu");
}

#[test]
fn default_sort_is_date_descending_with_unknown_dates_last() {
    let mut rows = vec![
        row(1, None, PodcastKind::Rss),
        row(2, Some(10), PodcastKind::Rss),
        row(3, Some(30), PodcastKind::Rss),
    ];
    sort_newest_first(&mut rows);
    assert_eq!(rows.iter().map(|row| row.id).collect::<Vec<_>>(), [3, 2, 1]);
}

#[test]
fn src_5_source_summary_counts_new_downloads_and_latest_episode() {
    let mut first = row(1, Some(10), PodcastKind::Rss);
    first.is_new = true;
    first.downloaded_bytes = Some(2_000_000);
    let mut second = row(2, Some(20), PodcastKind::Rss);
    second.downloaded_bytes = Some(3_000_000);
    second.played_at = Some(30);
    let group = SourceGroup {
        subscription_id: 7,
        title: "Show".into(),
        author: Some("Publisher".into()),
        image_url: None,
        kind: PodcastKind::Rss,
        sync_to_phone: true,
        episodes: vec![first, second],
    };

    let states = BTreeMap::from([
        (1, DownloadState::Downloaded { bytes: 2_000_000 }),
        (2, DownloadState::Downloaded { bytes: 3_000_000 }),
    ]);
    assert_eq!(
        source_summary(&group, &states),
        SourceSummary {
            episode_count: 2,
            new_count: 1,
            downloaded_bytes: 5_000_000,
            latest_published_at: Some(20),
        }
    );
}

/// `G2` (design 6a): the header line's "new" figure must sum the same
/// discovery definition as the per-group facts (`is_new`) across every
/// group, independent of playback status.
#[test]
fn pod_9_library_summary_counts_shows_episodes_and_new_across_all_groups() {
    let mut played = row(1, Some(10), PodcastKind::Rss);
    played.played_at = Some(30);
    let mut unplayed = row(2, Some(20), PodcastKind::Rss);
    unplayed.is_new = true;
    let mut resuming = row(3, Some(15), PodcastKind::Rss);
    resuming.position_ms = 5_000;
    let group_a = SourceGroup {
        subscription_id: 1,
        title: "Show A".into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Rss,
        sync_to_phone: false,
        episodes: vec![played, unplayed],
    };
    let group_b = SourceGroup {
        subscription_id: 2,
        title: "Show B".into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Rss,
        sync_to_phone: false,
        episodes: vec![resuming],
    };

    let summary = library_summary(&[group_a, group_b]);

    assert_eq!(
        summary,
        LibrarySummary {
            shows: 2,
            episodes: 3,
            new: 1,
        }
    );
}

/// `G2`: an empty library must not fabricate counts.
#[test]
fn pod_9_library_summary_is_zero_for_no_subscriptions() {
    assert_eq!(library_summary(&[]), LibrarySummary::default());
}

#[test]
fn pod_9_filtered_children_keep_the_full_source_summary() {
    let mut played = row(1, Some(10), PodcastKind::Rss);
    played.played_at = Some(30);
    let mut unplayed = row(2, Some(20), PodcastKind::Rss);
    unplayed.is_new = true;
    let group = SourceGroup {
        subscription_id: 7,
        title: "Show".into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Rss,
        sync_to_phone: false,
        episodes: vec![played, unplayed],
    };

    let rendered = rendered_source_groups(
        &[group],
        &PodcastFilter {
            unplayed_only: true,
            ..PodcastFilter::default()
        },
        &BTreeMap::new(),
    );

    assert_eq!(rendered[0].group.episodes.len(), 1);
    assert_eq!(rendered[0].summary.episode_count, 2);
    assert_eq!(rendered[0].summary.new_count, 1);
    assert_eq!(rendered[0].summary.latest_published_at, Some(20));
}

/// `POD-12` / `D3`: the "On phone" indicator must track the selection
/// exactly — on the moment a connected device is added to the
/// selection, off the moment it is removed, and unaffected by devices
/// that are not currently connected.
#[test]
fn pod_12_on_phone_reflects_the_toggle() {
    let phone = PodcastSyncDevice {
        id: "mtp:phone".into(),
        name: "Phone".into(),
    };
    let tablet = PodcastSyncDevice {
        id: "mtp:tablet".into(),
        name: "Tablet".into(),
    };

    // Nothing selected yet.
    assert!(!on_phone(std::slice::from_ref(&phone), &[]));

    // Selected, but only for a device that is not currently connected.
    assert!(!on_phone(
        std::slice::from_ref(&phone),
        &["mtp:tablet".to_owned()]
    ));

    // Selected for the connected device: the toggle just turned on.
    assert!(on_phone(
        std::slice::from_ref(&phone),
        &["mtp:phone".to_owned(), "mtp:tablet".to_owned()]
    ));

    // A second connected device also counts.
    assert!(on_phone(
        &[phone.clone(), tablet],
        &["mtp:tablet".to_owned()]
    ));

    // Un-toggled again: back to false.
    assert!(!on_phone(&[phone], &[]));
}
