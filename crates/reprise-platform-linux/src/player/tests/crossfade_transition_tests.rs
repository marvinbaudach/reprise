use super::segment_support::{count, write_regions_wav, Harness};
use super::*;
use crate::crossfade::{test_seam, CrossfadeEngine};
use reprise_core::library::settings::TrackTransition;
use std::sync::atomic::AtomicUsize;

/// Length of the track the promotion test fades into.
const SECOND_TRACK_MS: u32 = 3_000;
pub(super) const HANG_GUARD: Duration = Duration::from_secs(20);
/// How often a crossfade is begun, the first time included, when its
/// secondary hangs in READY. A natural hang is rare even under load, so a test
/// that needs more than one retry points at the fade giving way, not at
/// `playbin3`.
const CROSSFADE_START_ATTEMPTS: usize = 3;
/// How long a given-way crossfade must stay without a promotion before it
/// counts as given way: the promotion clears the guard a moment before it
/// reports `AdvancedToNext`.
const GAVE_WAY_SETTLE: Duration = Duration::from_millis(100);

/// The engine the position ticker drives, built from `player`'s handles, for
/// a test that plays the ticker's part itself.
pub(super) fn engine_of(player: &Player) -> CrossfadeEngine {
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
}

/// What goes wrong with the crossfade secondary.
#[derive(Clone, Copy)]
pub(super) enum Trouble {
    /// Nothing: the probe only counts the buffers that reach the secondary.
    Watch,
    /// The start never completes: the first buffer is held until `Null`.
    Hang,
    /// The start is late but completes: the first buffer waits this long.
    Late(Duration),
}

/// The injector installed on the next crossfade secondary. `installed`
/// counts the secondaries it reached, `reached` how often the probe fired
/// on one of them: the control arms that prove the trouble was injected,
/// and the tell between a secondary that hung in READY on its own (no
/// buffer ever reaches its gain element) and one that was merely slow.
pub(super) struct SecondaryTrouble {
    installed: Arc<AtomicUsize>,
    reached: Arc<AtomicUsize>,
    _hook: test_seam::HookGuard,
}

impl SecondaryTrouble {
    pub(super) fn installed(&self) -> usize {
        self.installed.load(Ordering::SeqCst)
    }

    pub(super) fn reached(&self) -> usize {
        self.reached.load(Ordering::SeqCst)
    }

    /// Forgets what an earlier attempt of the same test counted.
    fn reset(&self) {
        self.installed.store(0, Ordering::SeqCst);
        self.reached.store(0, Ordering::SeqCst);
        test_seam::forget_gave_up();
    }
}

/// Hooks the next secondary the crossfade builds, before it plays, and
/// puts a probe on the sink pad of its track-gain element, the same
/// technique as the start watchdog tests use on the main playbin.
pub(super) fn trouble_on_secondary(trouble: Trouble) -> SecondaryTrouble {
    let installed = Arc::new(AtomicUsize::new(0));
    let hooked = installed.clone();
    let reached = Arc::new(AtomicUsize::new(0));
    let counted = reached.clone();
    let hook = test_seam::on_secondary_built(Arc::new(move |secondary| {
        hooked.fetch_add(1, Ordering::SeqCst);
        let gain = secondary
            .property::<Option<gst::Element>>("audio-filter")
            .unwrap()
            .downcast::<gst::Bin>()
            .unwrap()
            .by_name("reprise-track-gain")
            .unwrap();
        let pad = gain.static_pad("sink").unwrap();
        let counted = counted.clone();
        match trouble {
            Trouble::Watch => {
                pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
                    counted.fetch_add(1, Ordering::SeqCst);
                    gst::PadProbeReturn::Ok
                });
            }
            Trouble::Hang => {
                pad.add_probe(gst::PadProbeType::BLOCK_DOWNSTREAM, move |_, _| {
                    counted.fetch_add(1, Ordering::SeqCst);
                    // Stay blocked: the pad is released by the next `Null`.
                    gst::PadProbeReturn::Ok
                });
            }
            Trouble::Late(delay) => {
                pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
                    counted.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(delay);
                    gst::PadProbeReturn::Remove
                });
            }
        }
    }));
    SecondaryTrouble {
        installed,
        reached,
        _hook: hook,
    }
}

/// Whether a secondary that the fade gave up on had hung in READY on its own,
/// as `playbin3` sometimes does on a start: no buffer ever reached its gain
/// element. `gave_up_in` is its (current, pending) state when the fade gave
/// up. Only that case is begun again; a secondary that was moving, or that
/// had left READY, was given up on too early, and the test must fail on it.
fn hung_in_ready(reached: usize, gave_up_in: Option<(gst::State, gst::State)>) -> bool {
    reached == 0 && gave_up_in == Some((gst::State::Ready, gst::State::Playing))
}

/// Runs a crossfade scenario until `done` holds for the events, and returns
/// them together with whether a second pipeline was ever seen starting.
///
/// The crossfade secondary is a `playbin3` too, and GStreamer 1.28's sometimes
/// never leaves READY on a start (see `Harness::start`). Since the crossfade
/// gives way to a plain track change when its incoming track does not preroll
/// (PLAY-25), such a secondary is no longer promoted, so a test that needs the
/// fade begins again, as `Harness::start` does for the main pipeline.
///
/// Only a secondary that hung on its own is begun again: `secondary` watches
/// the buffers reaching its gain element, and a hung one never gets one. A fade
/// that gave way although the secondary was moving is a failure of the fade,
/// and the events are returned for the test to fail on.
pub(super) fn crossfade_over_when_hung(
    harness: &Harness,
    secondary: &SecondaryTrouble,
    begin: impl Fn(),
    done: impl Fn(&[PlayerEvent]) -> bool,
) -> (Vec<PlayerEvent>, bool) {
    let player = &harness.player;
    for attempt in 1..=CROSSFADE_START_ATTEMPTS {
        secondary.reset();
        harness.start(&begin);
        let saw_crossfading = std::cell::Cell::new(false);
        let gave_way = || {
            saw_crossfading.get()
                && !player.crossfading.load(Ordering::SeqCst)
                && player
                    .incoming
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .is_none()
        };
        let promoted = |events: &[PlayerEvent]| {
            count(events, |event| matches!(event, PlayerEvent::AdvancedToNext)) > 0
        };
        let mut events = harness.pump_until(HANG_GUARD, |events| {
            if player.crossfading.load(Ordering::SeqCst) {
                saw_crossfading.set(true);
            }
            done(events) || (gave_way() && !promoted(events))
        });
        if !done(&events) && gave_way() {
            events.extend(harness.pump_for(GAVE_WAY_SETTLE));
        }
        let hung_on_its_own = !done(&events)
            && gave_way()
            && hung_in_ready(secondary.reached(), test_seam::gave_up_in());
        if !hung_on_its_own {
            return (events, saw_crossfading.get());
        }
        eprintln!("crossfade-retry: the secondary hung in READY on start {attempt}; starting over");
    }
    panic!("the crossfade secondary hung in READY on all {CROSSFADE_START_ATTEMPTS} starts");
}

/// Crossfade Phase B backend proof (headless, fakesink): with `Crossfade`
/// selected, the position ticker must spin up a second playbin for the
/// pre-fed successor and promote it without stopping the primary pipeline.
#[test]
fn play_20b_crossfade_promotion_carries_the_next_gain_from_the_first_sample() {
    let harness = Harness::new();
    let player = &harness.player;
    player.set_transition(TrackTransition::Crossfade, 1);

    let first = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    // Longer than the whole fade: a fixture that ends within it would post its
    // own end-of-stream right after the promotion, and that is a finished
    // track, not the outgoing one leaking. (`blip.flac` is 0.46 s.)
    let directory = tempfile::tempdir().unwrap();
    let second_path = directory.path().join("second.wav");
    write_regions_wav(&second_path, &[(SECOND_TRACK_MS, true)]);
    let second = second_path.to_str().unwrap();
    let watcher = trouble_on_secondary(Trouble::Watch);
    let (events, saw_crossfading) = crossfade_over_when_hung(
        &harness,
        &watcher,
        || {
            player.play(item(first)).unwrap();
            player.set_next(Some(PlaybackItem {
                segment: None,
                path: second,
                gain_db: 6.0,
            }));
        },
        |events| count(events, |event| matches!(event, PlayerEvent::AdvancedToNext)) > 0,
    );
    // Let the promotion's own events settle before counting them.
    let mut events = events;
    events.extend(harness.pump_for(Duration::from_millis(50)));
    let advanced = count(&events, |event| {
        matches!(event, PlayerEvent::AdvancedToNext)
    });
    let finished = count(&events, |event| matches!(event, PlayerEvent::TrackFinished));
    let stopped = count(&events, |event| {
        matches!(event, PlayerEvent::StateChanged(PlaybackState::Stopped))
    });

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
            .is_some_and(|uri| uri.ends_with("second.wav")),
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
}

mod cue {
    //! No crossfade into or out of a CUE track (PLAY-24). Two tracks of one
    //! file play through (PLAY-23a, `segment_boundary_tests`); every other
    //! change with a CUE track on either side is a hard change.

    use super::super::segment_support::{count, cue_item, write_regions_wav, Harness};
    use super::*;
    use crate::gapless::QueuedTrack;
    use crate::player_pipeline::path_to_uri;

    const CROSSFADE_SECONDS: u8 = 1;

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
        let watcher = super::trouble_on_secondary(super::Trouble::Watch);
        let (events, _) = super::crossfade_over_when_hung(
            harness,
            &watcher,
            || {
                harness.player.play(item(first.to_str().unwrap())).unwrap();
                harness
                    .player
                    .set_next(Some(item(second.to_str().unwrap())));
            },
            |events| count(events, advanced) > 0,
        );
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

        engine_of(&harness.player).maybe_start(1_500, 2_000);

        let player = &harness.player;
        assert!(!player.crossfading.load(Ordering::SeqCst));
        assert!(player
            .next_uri
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some());
    }
}

mod hung_in_ready_rule {
    use super::*;

    const HUNG: Option<(gst::State, gst::State)> = Some((gst::State::Ready, gst::State::Playing));

    #[test]
    fn a_secondary_that_never_left_ready_and_got_no_buffer_hung_on_its_own() {
        assert!(hung_in_ready(0, HUNG));
    }

    #[test]
    fn a_secondary_that_had_left_ready_was_given_up_on_too_early() {
        for state in [
            (gst::State::Paused, gst::State::Playing),
            (gst::State::Paused, gst::State::VoidPending),
            (gst::State::Playing, gst::State::VoidPending),
        ] {
            assert!(!hung_in_ready(0, Some(state)), "{state:?}");
        }
    }

    #[test]
    fn a_secondary_that_got_a_buffer_was_not_hung() {
        assert!(!hung_in_ready(1, HUNG));
    }

    #[test]
    fn a_fade_that_did_not_record_its_secondary_is_not_retried() {
        assert!(!hung_in_ready(0, None));
    }
}
