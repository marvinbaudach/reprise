//! External-media state regressions split from `external_media_state.rs` for the code-file size gate.

use super::*;

fn podcast_session(
    neighbours: Option<NeighbourContext>,
    automatic_advance: Option<AutomaticAdvance>,
) -> PodcastSession {
    PodcastSession {
        media: ExternalMedia::Podcast {
            episode_id: 7,
            title: "Episode".into(),
            show: "Show".into(),
            source: EpisodeSource::Url("https://example.test/episode.mp3".into()),
            resume_ms: 0,
            duration_ms: None,
        },
        neighbours,
        automatic_advance,
        subscription_id: 42,
        kind: PodcastKind::Rss,
        published_at: None,
        art_url: None,
        phase: PodcastPhase::Playing,
        restored: false,
        origin: PodcastOrigin::Direct,
        resume: ResumePolicy::new(0),
        position_ms: 0,
        last_persisted_ms: 0,
        duration_known: false,
        error: None,
    }
}

#[test]
fn pod_21_a_stream_that_dies_before_playing_stays_on_the_advance_chain() {
    let neighbours = NeighbourContext::for_episode(&[1, 2, 3], 2).unwrap();
    let chain = AutomaticAdvance::new(NeighbourDirection::Next);
    let mut session = podcast_session(Some(neighbours), Some(chain));

    // `play_uri` accepted the URI, so the session is nominally "playing",
    // but nothing has streamed yet — this is the 403-after-start case.
    session.note_playback_progress(0);
    assert!(
        matches!(session.failure_action(), PodcastFailureAction::Automatic(_)),
        "a stream that never advanced must keep skipping, not strand the user"
    );

    // Once it genuinely advances, a later break is a mid-playback break.
    session.note_playback_progress(4_000);
    assert_eq!(session.failure_action(), PodcastFailureAction::Direct);
}

#[test]
fn automatic_advance_skips_two_failures_and_stops_on_the_third() {
    let first_target = NeighbourContext::for_episode(&[1, 2, 3, 4, 5], 2).unwrap();
    let chain = AutomaticAdvance::new(NeighbourDirection::Next);

    let AdvanceFailure::Retry {
        neighbours: second_target,
        chain,
    } = chain.after_failure(&first_target)
    else {
        panic!("the first failure should skip to the next neighbour");
    };
    assert_eq!(second_target.current_id(), 3);

    let AdvanceFailure::Retry {
        neighbours: third_target,
        chain,
    } = chain.after_failure(&second_target)
    else {
        panic!("the second failure should skip to the next neighbour");
    };
    assert_eq!(third_target.current_id(), 4);
    assert_eq!(chain.after_failure(&third_target), AdvanceFailure::Stop);
}

#[test]
fn direct_failure_stops_on_the_clicked_episode_without_skipping() {
    let neighbours = NeighbourContext::for_episode(&[6, 7, 8], 7).unwrap();
    let session = podcast_session(Some(neighbours), None);

    assert_eq!(session.failure_action(), PodcastFailureAction::Direct);
}

#[test]
fn queue_navigation_is_selected_only_by_the_session_not_the_open_view() {
    let queue = ExternalPlaybackState::default();
    assert_eq!(
        queue.transport_target(NeighbourDirection::Next),
        NeighbourTransport::Queue
    );

    let neighbours = NeighbourContext::for_episode(&[6, 7, 8], 7).unwrap();
    let podcast = ExternalPlaybackState {
        session: Some(ExternalSession::Podcast(podcast_session(
            Some(neighbours),
            None,
        ))),
        ..ExternalPlaybackState::default()
    };
    assert!(matches!(
        podcast.transport_target(NeighbourDirection::Next),
        NeighbourTransport::Item {
            neighbours: context,
            origin: PodcastOrigin::Direct,
        } if context.current_id() == 8
    ));
}

#[test]
fn failed_early_resume_is_retried_once_after_duration_arrives() {
    let mut resume = ResumePolicy::new(42_000);
    resume.initial_seek_finished(false);

    assert_eq!(resume.position_tick(0), None);
    assert_eq!(resume.position_tick(180_000), Some(42_000));
    assert_eq!(resume.position_tick(180_000), None);
}

#[test]
fn successful_early_resume_needs_no_retry() {
    let mut resume = ResumePolicy::new(42_000);
    resume.initial_seek_finished(true);

    assert_eq!(resume.position_tick(180_000), None);
}

#[test]
fn rad_2_pause_is_disconnect_presented_as_paused() {
    let mut state = RadioPresentation::connected();
    state.on_stream_title(Some("A song".into()));

    assert_eq!(state.pause(), Some(RadioCommand::Disconnect));
    assert_eq!(state.phase(), RadioPhase::Paused);
    assert_eq!(state.last_title(), Some("A song"));
    assert_eq!(state.table_now_playing(), None);
    assert_eq!(state.play(), Some(RadioCommand::Reconnect));

    state.reconnect_failed("station unavailable".into());
    assert_eq!(state.phase(), RadioPhase::Paused);
    assert_eq!(state.inline_error(), Some("station unavailable"));
    assert!(!state.is_empty());
}

/// `PLAY-12`: stopping external playback ends the session outright —
/// nothing takes over, so the projection the player bar and the panel
/// follow goes away with it and they have to drop to their empty state.
/// (`leave_external_for_queue` is the other exit: there a queue track
/// takes over and keeps the surfaces loaded.)
#[test]
fn play_12_a_stopped_radio_session_leaves_nothing_to_project() {
    let mut state = radio_state();
    assert_eq!(
        state.snapshot().map(|snapshot| snapshot.mode),
        Some(PlaybackMode::Radio)
    );

    state.clear_session();
    state.clear_preview();

    assert!(state.snapshot().is_none());
}

#[test]
fn reconnecting_radio_can_be_paused_before_audio_starts() {
    let mut state = RadioPresentation {
        phase: RadioPhase::Reconnecting,
        last_title: None,
        inline_error: None,
    };

    assert_eq!(state.pause(), Some(RadioCommand::Disconnect));
    assert_eq!(state.phase(), RadioPhase::Paused);
}

#[test]
fn downloaded_youtube_episode_plays_without_resolution() {
    assert!(!podcast_source_requires_resolution(
        PodcastKind::Youtube,
        &EpisodeSource::File("/data/episode.webm".into()),
    ));
    assert!(podcast_source_requires_resolution(
        PodcastKind::Youtube,
        &EpisodeSource::Url("https://youtube.example/watch?v=1".into()),
    ));
}

#[test]
fn pausing_an_async_resolution_invalidates_its_generation() {
    let mut state = ExternalPlaybackState::default();
    let previous = state.generation;

    state.invalidate_pending();

    assert_ne!(state.generation, previous);
}

#[test]
fn stream_tags_only_belong_to_a_connected_radio() {
    let mut state = RadioPresentation::connected();
    assert!(state.accepts_stream_tags());
    state.pause();
    assert!(!state.accepts_stream_tags());
    state.play();
    assert!(!state.accepts_stream_tags());
}

#[test]
fn radio_retry_is_consumed_until_a_user_reconnect_resets_it() {
    let mut guard = reprise_core::radio::click::ReresolveGuard::default();
    assert!(guard.take_retry(Some("station")));
    assert!(!guard.take_retry(Some("station")));

    guard = reprise_core::radio::click::ReresolveGuard::default();
    assert!(guard.take_retry(Some("station")));
}

#[test]
fn playing_radio_activation_stops_instead_of_pausing() {
    let state = RadioPresentation::connected();
    assert_eq!(state.activation(), RadioCommand::Stop);
}

#[test]
fn removing_a_show_matches_only_its_active_podcast_session() {
    let session = podcast_session(None, None);
    let state = ExternalPlaybackState {
        session: Some(ExternalSession::Podcast(session)),
        ..ExternalPlaybackState::default()
    };
    assert!(state.plays_podcast_subscription(42));
    assert!(!state.plays_podcast_subscription(41));
}

#[test]
fn ac_26_external_snapshots_classify_youtube_and_radio_as_music() {
    let mut rss_session = podcast_session(None, None);
    rss_session.kind = PodcastKind::Rss;
    let rss = podcast_state(rss_session).snapshot().unwrap();
    assert!(!rss.carries_music(), "an RSS podcast is speech");

    let mut youtube_session = podcast_session(None, None);
    youtube_session.kind = PodcastKind::Youtube;
    let youtube = podcast_state(youtube_session).snapshot().unwrap();
    assert!(youtube.carries_music(), "YouTube carries Song Visuals");

    let radio = radio_state().snapshot().unwrap();
    assert!(radio.carries_music(), "radio carries Song Visuals");
}

fn podcast_state(session: PodcastSession) -> ExternalPlaybackState {
    ExternalPlaybackState {
        session: Some(ExternalSession::Podcast(session)),
        ..ExternalPlaybackState::default()
    }
}

fn radio_state() -> ExternalPlaybackState {
    ExternalPlaybackState {
        session: Some(ExternalSession::Radio(RadioSession {
            media: ExternalMedia::Radio {
                station_id: 5,
                name: "Station".into(),
                stream_url: "https://example.test/stream".into(),
                uuid: None,
            },
            art_url: None,
            presentation: RadioPresentation::connected(),
            retry_guard: reprise_core::radio::click::ReresolveGuard::default(),
        })),
        ..ExternalPlaybackState::default()
    }
}
