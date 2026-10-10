use super::segment_support::start_over_when_hung;
use super::*;
use reprise_core::library::settings::TrackTransition;

/// Crossfade Phase B backend proof (headless, fakesink): with `Crossfade`
/// selected, the position ticker must spin up a second playbin for the
/// pre-fed successor and promote it without stopping the primary pipeline.
#[test]
fn play_20b_crossfade_promotion_carries_the_next_gain_from_the_first_sample() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

    let (tx, rx) = std::sync::mpsc::channel::<PlayerEvent>();
    let player = Player::new(Box::new(move |event| {
        let _ = tx.send(event);
    }))
    .unwrap();

    player.set_transition(TrackTransition::Crossfade, 1);

    let first = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    let second = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/blip.flac");
    start_over_when_hung(&player, || {
        player.play(item(first)).unwrap();
        player.set_next(Some(PlaybackItem {
            segment: None,
            path: second,
            gain_db: 6.0,
        }));
    });

    let main_context = gst::glib::MainContext::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut advanced = 0usize;
    let mut finished = 0usize;
    let mut stopped = 0usize;
    let mut saw_crossfading = false;
    while std::time::Instant::now() < deadline {
        main_context.iteration(false);
        if player.crossfading.load(Ordering::SeqCst) {
            saw_crossfading = true;
        }
        while let Ok(event) = rx.try_recv() {
            match event {
                PlayerEvent::AdvancedToNext => advanced += 1,
                PlayerEvent::TrackFinished => finished += 1,
                PlayerEvent::StateChanged(PlaybackState::Stopped) => stopped += 1,
                _ => {}
            }
        }
        if advanced > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    main_context.iteration(false);
    while let Ok(event) = rx.try_recv() {
        match event {
            PlayerEvent::AdvancedToNext => advanced += 1,
            PlayerEvent::TrackFinished => finished += 1,
            PlayerEvent::StateChanged(PlaybackState::Stopped) => stopped += 1,
            _ => {}
        }
    }

    assert!(saw_crossfading, "expected the second pipeline to start");
    assert_eq!(
        advanced, 1,
        "expected exactly one AdvancedToNext from the crossfade promotion"
    );
    assert_eq!(stopped, 0, "the primary pipeline must not stop mid-fade");
    assert_eq!(
        finished, 0,
        "the outgoing EOS must not surface as TrackFinished"
    );

    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let current_uri = playbin.property::<Option<String>>("current-uri");
    assert!(
        current_uri
            .as_deref()
            .is_some_and(|uri| uri.ends_with("blip.flac")),
        "the promoted pipeline should contain the second track"
    );
    let track_gain = playbin
        .property::<Option<gst::Element>>("audio-filter")
        .unwrap()
        .downcast::<gst::Bin>()
        .unwrap()
        .by_name("reprise-track-gain")
        .unwrap()
        .property::<f64>("volume");
    assert!((track_gain - 10_f64.powf(6.0 / 20.0)).abs() < 1e-6);
    drop(playbin);

    assert!(
        !player.crossfading.load(Ordering::SeqCst),
        "the crossfade guard must be cleared after promotion"
    );
    assert!(
        player
            .incoming
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none(),
        "the incoming-pipeline slot must be empty after promotion"
    );

    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

mod cue {
    //! No crossfade into or out of a CUE track (PLAY-24). Two tracks of one
    //! file play through (PLAY-23a, `segment_boundary_tests`); every other
    //! change with a CUE track on either side is a hard change.

    use super::super::segment_support::{count, cue_item, write_regions_wav, Harness};
    use super::*;
    use crate::crossfade::CrossfadeEngine;
    use crate::gapless::QueuedTrack;
    use crate::player_pipeline::path_to_uri;

    const CROSSFADE_SECONDS: u8 = 1;
    const HANG_GUARD: Duration = Duration::from_secs(20);

    fn finished(event: &PlayerEvent) -> bool {
        matches!(event, PlayerEvent::TrackFinished)
    }

    fn advanced(event: &PlayerEvent) -> bool {
        matches!(event, PlayerEvent::AdvancedToNext)
    }

    fn tone(directory: &tempfile::TempDir, name: &str, ms: u32) -> std::path::PathBuf {
        let path = directory.path().join(name);
        write_regions_wav(&path, &[(ms, true)]);
        path
    }

    /// Pumps until the playing track finishes, noting whether a second
    /// pipeline ever started on the way.
    fn pump_to_finish(harness: &Harness) -> (Vec<PlayerEvent>, bool) {
        let crossfaded = std::cell::Cell::new(false);
        let events = harness.pump_until(HANG_GUARD, |events| {
            if harness.player.crossfading.load(Ordering::SeqCst) {
                crossfaded.set(true);
            }
            count(events, finished) > 0
        });
        (events, crossfaded.get())
    }

    #[test]
    fn play_24_a_cue_track_changes_hard_to_a_whole_file() {
        let harness = Harness::new();
        harness
            .player
            .set_transition(TrackTransition::Crossfade, CROSSFADE_SECONDS);
        let directory = tempfile::tempdir().unwrap();
        let album = tone(&directory, "album.wav", 6_000);
        let single = tone(&directory, "single.wav", 3_000);

        harness.start(|| {
            harness
                .player
                .play(cue_item(&album, (1_000, 3_000), 0.0))
                .unwrap();
            harness
                .player
                .set_next(Some(item(single.to_str().unwrap())));
        });
        let (events, crossfaded) = pump_to_finish(&harness);

        assert!(
            !crossfaded,
            "no second pipeline may fade out of a CUE track"
        );
        assert_eq!(count(&events, finished), 1);
        assert_eq!(count(&events, advanced), 0);
        harness.player.play(item(single.to_str().unwrap())).unwrap();
        let current_uri = harness
            .player
            .playbin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .property::<Option<String>>("current-uri");
        assert!(current_uri.is_some_and(|uri| uri.ends_with("single.wav")));
    }

    #[test]
    fn play_24_a_whole_file_changes_hard_to_a_cue_track() {
        let harness = Harness::new();
        harness
            .player
            .set_transition(TrackTransition::Crossfade, CROSSFADE_SECONDS);
        let directory = tempfile::tempdir().unwrap();
        let single = tone(&directory, "single.wav", 2_500);
        let album = tone(&directory, "album.wav", 6_000);

        harness.start(|| {
            harness.player.play(item(single.to_str().unwrap())).unwrap();
            harness
                .player
                .set_next(Some(cue_item(&album, (1_000, 3_000), 0.0)));
        });
        let (events, crossfaded) = pump_to_finish(&harness);

        assert!(!crossfaded, "no second pipeline may fade into a CUE track");
        assert_eq!(count(&events, finished), 1);
        assert_eq!(count(&events, advanced), 0);
    }

    /// The fade the promotion tests run: two seconds gives the 500 ms
    /// position ticker four chances to start it even on a loaded machine.
    const PROMOTION_FADE_SECONDS: u8 = 2;

    /// Fades `first` into `second` and returns once the crossfade promoted
    /// the pipeline playing `second`.
    fn crossfade_into(harness: &Harness, first: &std::path::Path, second: &std::path::Path) {
        harness
            .player
            .set_transition(TrackTransition::Crossfade, PROMOTION_FADE_SECONDS);
        harness.start(|| {
            harness.player.play(item(first.to_str().unwrap())).unwrap();
            harness
                .player
                .set_next(Some(item(second.to_str().unwrap())));
        });
        let events = harness.pump_until(HANG_GUARD, |events| count(events, advanced) > 0);
        assert_eq!(count(&events, advanced), 1, "the crossfade must promote");
        assert_eq!(count(&events, finished), 0);
    }

    /// The everyday path into a CUE track under Crossfade: the track a
    /// crossfade promoted ends, finishes, and the CUE track after it starts
    /// hard and plays to its own end.
    #[test]
    fn play_24_a_track_promoted_by_a_crossfade_finishes_and_hands_on_to_a_cue_track() {
        let harness = Harness::new();
        let directory = tempfile::tempdir().unwrap();
        let first = tone(&directory, "first.wav", 4_000);
        let second = tone(&directory, "second.wav", 3_000);
        let album = tone(&directory, "album.wav", 4_000);
        harness.player.set_spectrum_enabled(true).unwrap();
        crossfade_into(&harness, &first, &second);

        harness
            .player
            .set_next(Some(cue_item(&album, (1_000, 2_000), 0.0)));
        let (events, crossfaded) = pump_to_finish(&harness);
        assert_eq!(
            count(&events, finished),
            1,
            "the promoted track must reach its end and finish"
        );
        assert!(!crossfaded, "no second pipeline may fade into a CUE track");
        assert!(
            count(&events, |event| matches!(event, PlayerEvent::Spectrum(_))) > 0,
            "the promoted pipeline must feed the visualizer"
        );

        harness.start(|| {
            harness
                .player
                .play(cue_item(&album, (1_000, 2_000), 0.0))
                .unwrap();
        });
        let (events, _) = pump_to_finish(&harness);
        assert_eq!(
            count(&events, finished),
            1,
            "the CUE track after it must start and finish"
        );
    }

    /// Crossfades between whole files are unchanged: the last track, promoted
    /// by a crossfade with nothing after it, still finishes.
    #[test]
    fn play_24_the_last_track_promoted_by_a_crossfade_finishes() {
        let harness = Harness::new();
        let directory = tempfile::tempdir().unwrap();
        let first = tone(&directory, "first.wav", 4_000);
        let second = tone(&directory, "second.wav", 3_000);
        crossfade_into(&harness, &first, &second);

        let (events, _) = pump_to_finish(&harness);
        assert_eq!(
            count(&events, finished),
            1,
            "the promoted last track must reach its end and finish"
        );
    }

    /// The trigger itself refuses while a CUE track plays, even with a whole
    /// file in the slot — the pre-feed rules keep it empty, this guard does
    /// not rely on them.
    #[test]
    fn play_24_the_crossfade_trigger_refuses_while_a_cue_track_plays() {
        let harness = Harness::new();
        harness
            .player
            .set_transition(TrackTransition::Crossfade, CROSSFADE_SECONDS);
        let directory = tempfile::tempdir().unwrap();
        let album = tone(&directory, "album.wav", 6_000);
        let single = tone(&directory, "single.wav", 3_000);
        harness.start(|| {
            harness
                .player
                .play(cue_item(&album, (1_000, 3_000), 0.0))
                .unwrap();
        });
        *harness
            .player
            .next_uri
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(QueuedTrack {
            uri: path_to_uri(single.to_str().unwrap()).unwrap(),
            gain_db: 0.0,
            segment: None,
        });

        let player = &harness.player;
        CrossfadeEngine {
            playbin: player.playbin.clone(),
            bus_watch: player.bus_watch.clone(),
            on_event: player.on_event.clone(),
            effects: player.effects.clone(),
            next_uri: player.next_uri.clone(),
            pending_gain: player.pending_gain.clone(),
            handoff_pending: player.handoff_pending.clone(),
            transition: player.transition.clone(),
            crossfading: player.crossfading.clone(),
            user_volume: player.user_volume.clone(),
            generation: player.fade_generation.clone(),
            incoming: player.incoming.clone(),
            spectrum_enabled: player.spectrum_enabled.clone(),
            cava_stream_generation: player.cava_stream_generation.clone(),
            stream_generation: player.stream_generation.clone(),
            segments: player.segments.clone(),
        }
        .maybe_start(1_500, 2_000);

        assert!(!player.crossfading.load(Ordering::SeqCst));
        assert!(player
            .next_uri
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some());
    }
}
