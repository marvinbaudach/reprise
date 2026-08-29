use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::test_support::{fake_binary, short_timeouts};
use super::{YtDlp, YtDlpChannel};

fn channel(id: &str) -> YtDlpChannel {
    YtDlpChannel {
        id: id.to_owned(),
        title: id.to_owned(),
        url: format!("https://www.youtube.com/channel/{id}"),
        image_url: None,
        matching_video_count: 1,
        matching_video_ids: Vec::new(),
        follower_count: None,
    }
}

#[test]
fn src_9_channel_head_uses_the_shared_runner_and_returns_the_published_count() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("args");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "printf '%s\\n' \"$@\" > '{}'\nprintf '%s\\n' '{{\"channel_follower_count\":67200000}}'",
            log.display()
        ),
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts())
        .with_browser_session("brave")
        .with_metadata_language(Some("de"));

    assert_eq!(
        runner
            .channel_follower_count("https://www.youtube.com/channel/UC-visible")
            .unwrap(),
        Some(67_200_000)
    );
    assert_eq!(
        fs::read_to_string(log).unwrap().lines().collect::<Vec<_>>(),
        [
            "--cookies-from-browser",
            "brave",
            "--no-warnings",
            "--flat-playlist",
            "-I",
            "0",
            "--extractor-args",
            "youtube:lang=de",
            "-J",
            "https://www.youtube.com/channel/UC-visible",
        ]
    );
}

#[test]
fn src_9_enrichment_keeps_partial_success_and_swallows_channel_failures() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(
        directory.path(),
        r#"
case "$*" in
  "--no-warnings --flat-playlist -I 0 -J https://www.youtube.com/channel/UC-visible")
    printf '%s\n' '{"channel_follower_count":62400}' ;;
  "--no-warnings --flat-playlist -I 0 -J https://www.youtube.com/channel/UC-failed")
    exit 2 ;;
  *) printf '%s\n' "unexpected arguments: $*" >&2; exit 3 ;;
esac
"#,
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());
    let mut channels = vec![channel("UC-visible"), channel("UC-failed")];

    runner.enrich_follower_counts(&mut channels, &AtomicBool::new(false));

    assert_eq!(channels[0].follower_count, Some(62_400));
    assert_eq!(channels[1].follower_count, None);
}

#[test]
fn follower_enrichment_never_dispatches_a_non_youtube_channel_url() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("invoked");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "touch '{}'\nprintf '%s\\n' '{{\"channel_follower_count\":99}}'",
            log.display()
        ),
    );
    let runner =
        YtDlp::with_binary_and_timeouts(binary, short_timeouts()).with_browser_session("brave");
    let mut channels = vec![channel("UC-untrusted"), channel("UC-wrong-scheme")];
    channels[0].url = "https://attacker.example/channel/UC-untrusted".to_owned();
    channels[1].url = "file://youtube.com/channel/UC-wrong-scheme".to_owned();

    runner.enrich_follower_counts(&mut channels, &AtomicBool::new(false));

    assert!(
        !log.exists(),
        "the untrusted URL reached the yt-dlp boundary"
    );
    assert!(channels
        .iter()
        .all(|channel| channel.follower_count.is_none()));
}

#[test]
fn src_9_channel_head_never_invents_a_hidden_or_malformed_count() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(
        directory.path(),
        r#"
for argument in "$@"; do url=$argument; done
case "$url" in
  *UC-absent) printf '%s\n' '{}' ;;
  *UC-null) printf '%s\n' '{"channel_follower_count":null}' ;;
  *UC-float) printf '%s\n' '{"channel_follower_count":1200000.0}' ;;
esac
"#,
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());

    assert_eq!(
        runner
            .channel_follower_count("https://www.youtube.com/channel/UC-absent")
            .unwrap(),
        None
    );
    assert_eq!(
        runner
            .channel_follower_count("https://www.youtube.com/channel/UC-null")
            .unwrap(),
        None
    );
    assert_eq!(
        runner
            .channel_follower_count("https://www.youtube.com/channel/UC-float")
            .unwrap(),
        Some(1_200_000)
    );
}

#[test]
fn follower_enrichment_returns_at_its_budget_and_leaves_timed_out_counts_absent() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(
        directory.path(),
        "sleep 5\nprintf '%s\\n' '{\"channel_follower_count\":99}'",
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());
    let mut channels = vec![channel("UC-slow")];
    let started = Instant::now();

    runner.enrich_follower_counts_with_budget(
        &mut channels,
        &AtomicBool::new(false),
        Duration::from_millis(120),
    );

    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the whole pass must own its deadline: {:?}",
        started.elapsed()
    );
    assert_eq!(channels[0].follower_count, None);
    assert_eq!(channels[0].matching_video_count, 1);
}

#[test]
fn follower_enrichment_cancellation_stops_before_another_channel_is_picked_up() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("urls");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "for argument in \"$@\"; do url=$argument; done\nprintf '%s\\n' \"$url\" >> '{}'\nsleep 0.3\nprintf '%s\\n' '{{\"channel_follower_count\":7}}'",
            log.display()
        ),
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());
    let mut channels = (0..12)
        .map(|index| channel(&format!("UC-{index}")))
        .collect::<Vec<_>>();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel_from_thread = cancelled.clone();
    let cancel_thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        cancel_from_thread.store(true, Ordering::Release);
    });

    runner.enrich_follower_counts(&mut channels, &cancelled);
    cancel_thread.join().unwrap();

    let fetched = fs::read_to_string(log).unwrap().lines().count();
    assert!(fetched > 0);
    assert!(
        fetched <= 4,
        "only the already in-flight workers may finish after cancellation: {fetched}"
    );
}

#[test]
fn follower_enrichment_runs_no_more_than_four_channel_heads_at_once() {
    let directory = tempfile::tempdir().unwrap();
    let active = directory.path().join("active");
    fs::create_dir(&active).unwrap();
    let overlap = directory.path().join("overlap");
    let binary = fake_binary(
        directory.path(),
        &format!(
            "for argument in \"$@\"; do url=$argument; done\nid=${{url##*/}}\ntouch '{}/'$id\nfind '{}' -type f | wc -l >> '{}'\nsleep 0.2\nrm '{}/'$id\nprintf '%s\\n' '{{\"channel_follower_count\":7}}'",
            active.display(),
            active.display(),
            overlap.display(),
            active.display()
        ),
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());
    let mut channels = (0..12)
        .map(|index| channel(&format!("UC-{index}")))
        .collect::<Vec<_>>();

    runner.enrich_follower_counts(&mut channels, &AtomicBool::new(false));

    let peak = fs::read_to_string(overlap)
        .unwrap()
        .lines()
        .map(|line| line.trim().parse::<usize>().unwrap())
        .max()
        .unwrap();
    assert_eq!(peak, 4);
    assert!(channels
        .iter()
        .all(|channel| channel.follower_count == Some(7)));
}

#[test]
fn follower_enrichment_fetches_only_the_first_twenty_missing_counts() {
    let directory = tempfile::tempdir().unwrap();
    let binary = fake_binary(
        directory.path(),
        "printf '%s\\n' '{\"channel_follower_count\":7}'",
    );
    let runner = YtDlp::with_binary_and_timeouts(binary, short_timeouts());
    let mut channels = (0..26)
        .map(|index| channel(&format!("UC-{index}")))
        .collect::<Vec<_>>();
    channels[0].follower_count = Some(99);

    runner.enrich_follower_counts(&mut channels, &AtomicBool::new(false));

    assert_eq!(channels[0].follower_count, Some(99));
    assert!(channels[1..=20]
        .iter()
        .all(|channel| channel.follower_count == Some(7)));
    assert!(channels[21..]
        .iter()
        .all(|channel| channel.follower_count.is_none()));
}
