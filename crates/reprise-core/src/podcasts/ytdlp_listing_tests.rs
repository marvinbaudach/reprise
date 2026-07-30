use super::test_support::{fake_binary, short_timeouts};
use super::YtDlp;

#[test]
fn pod_18_a_youtube_listing_asks_yt_dlp_for_upload_dates() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(
        directory.path(),
        r#"
test "$*" = "--no-warnings --flat-playlist --extractor-args youtubetab:approximate_date -J https://youtube.test/@show"
printf '%s\n' '{"title":"Channel","entries":[]}'
"#,
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());

    runner.list("https://youtube.test/@show").unwrap();
}

#[test]
fn pod_18_a_youtube_episode_keeps_the_upload_timestamp_from_the_listing() {
    let playlist = super::parse_playlist(
        "list",
        r#"{"entries":[{"id":"dated","title":"Dated","timestamp":1785225600}]}"#,
    )
    .unwrap();

    let episode =
        crate::podcasts::youtube::project_video(playlist.entries.into_iter().next().unwrap());

    assert_eq!(episode.published_at, Some(1_785_225_600));
}

#[test]
fn pod_18_a_listing_without_a_timestamp_still_yields_episodes() {
    let playlist = super::parse_playlist(
        "list",
        r#"{"entries":[
          {"id":"missing","title":"Missing"},
          {"id":"null","title":"Null","timestamp":null},
          {"id":"negative","title":"Negative","timestamp":-1},
          {"id":"string","title":"String","timestamp":"1785225600"}
        ]}"#,
    )
    .unwrap();

    // No date may cost an episode: every entry survives, dated or not.
    assert_eq!(playlist.entries.len(), 4);
    assert_eq!(playlist.entries[0].timestamp, None);
    assert_eq!(playlist.entries[1].timestamp, None);
    assert_eq!(playlist.entries[2].timestamp, None);
    // yt-dlp sometimes renders numbers as strings; that is a date, not a defect.
    assert_eq!(playlist.entries[3].timestamp, Some(1_785_225_600));
}
