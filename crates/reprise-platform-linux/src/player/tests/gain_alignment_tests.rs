//! Proves the track gain changes exactly at the audible track boundary.
//!
//! The filter bin buffers up to a second of audio in its playback queue. A gain
//! switch keyed to the bin's *sink* pad would therefore fire a second before
//! the old track's tail is played, so the tail would be scaled by the new
//! track's gain. The switch has to ride the `STREAM_START` event after the
//! queue, where it is serialised with the data.

use std::path::Path;

use super::*;
use crate::player_pipeline::AUDIO_SINK_ENV_VAR;

const FIRST_GAIN_DB: f64 = -6.0;
const SECOND_GAIN_DB: f64 = 6.0;
const FIRST_TRACK_SECONDS: u32 = 3;
const SECOND_TRACK_SECONDS: u32 = 2;
const DEADLINE: Duration = Duration::from_secs(15);
const QUEUE_THREAD_STALL: Duration = Duration::from_millis(200);

/// One buffer as it leaves the gain element: which stream it belongs to and the
/// linear gain the element held while it processed that buffer.
#[derive(Clone, Copy, Debug)]
struct GainSample {
    stream_index: usize,
    linear_gain: f64,
}

fn linear(gain_db: f64) -> f64 {
    10_f64.powf(gain_db / 20.0)
}

fn gain_element(player: &Player) -> gst::Element {
    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    playbin
        .property::<Option<gst::Element>>("audio-filter")
        .unwrap()
        .downcast::<gst::Bin>()
        .unwrap()
        .by_name("reprise-track-gain")
        .unwrap()
}

/// Records every buffer leaving the gain element together with the stream it
/// belongs to (counted from the `STREAM_START` events on the same pad).
fn record_gain_per_buffer(gain: &gst::Element) -> Arc<Mutex<Vec<GainSample>>> {
    let samples = Arc::new(Mutex::new(Vec::new()));
    let recorded = samples.clone();
    let element = gain.clone();
    let streams_seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    gain.static_pad("src").unwrap().add_probe(
        gst::PadProbeType::BUFFER | gst::PadProbeType::EVENT_DOWNSTREAM,
        move |_, info| {
            match &info.data {
                Some(gst::PadProbeData::Event(event))
                    if event.type_() == gst::EventType::StreamStart =>
                {
                    streams_seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
                Some(gst::PadProbeData::Buffer(_)) => {
                    let seen = streams_seen.load(std::sync::atomic::Ordering::SeqCst);
                    recorded
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(GainSample {
                            stream_index: seen.saturating_sub(1),
                            linear_gain: element.property::<f64>("volume"),
                        });
                }
                _ => {}
            }
            gst::PadProbeReturn::Ok
        },
    );
    samples
}

fn pump_until_finished(rx: &std::sync::mpsc::Receiver<PlayerEvent>) {
    let main_context = gst::glib::MainContext::default();
    let deadline = std::time::Instant::now() + DEADLINE;
    while std::time::Instant::now() < deadline {
        main_context.iteration(false);
        while let Ok(event) = rx.try_recv() {
            if matches!(event, PlayerEvent::TrackFinished) {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the second track never finished");
}

fn write_wavs(directory: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let first = directory.join("first.wav");
    let second = directory.join("second.wav");
    handoff_duration_tests::write_sine_wav(&first, FIRST_TRACK_SECONDS);
    handoff_duration_tests::write_sine_wav(&second, SECOND_TRACK_SECONDS);
    (first, second)
}

#[test]
fn play_19a_every_buffer_of_each_track_carries_that_tracks_gain() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");
    let (tx, rx) = std::sync::mpsc::channel::<PlayerEvent>();
    let player = Player::new(Box::new(move |event| {
        let _ = tx.send(event);
    }))
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (first, second) = write_wavs(directory.path());
    let samples = record_gain_per_buffer(&gain_element(&player));

    player
        .play(PlaybackItem {
            path: first.to_str().unwrap(),
            gain_db: FIRST_GAIN_DB,
        })
        .unwrap();
    player.set_next(Some(PlaybackItem {
        path: second.to_str().unwrap(),
        gain_db: SECOND_GAIN_DB,
    }));
    pump_until_finished(&rx);

    let samples = samples.lock().unwrap_or_else(PoisonError::into_inner);
    let of_stream = |index: usize| {
        samples
            .iter()
            .filter(|sample| sample.stream_index == index)
            .collect::<Vec<_>>()
    };
    let (first_buffers, second_buffers) = (of_stream(0), of_stream(1));
    assert!(!first_buffers.is_empty(), "no buffers of the first track");
    assert!(!second_buffers.is_empty(), "no buffers of the second track");
    let misfits = |buffers: &[&GainSample], expected_db: f64| {
        buffers
            .iter()
            .filter(|sample| (sample.linear_gain - linear(expected_db)).abs() > 1e-6)
            .count()
    };
    assert_eq!(
        misfits(&first_buffers, FIRST_GAIN_DB),
        0,
        "the tail of the first track was scaled by the wrong gain \
         ({} of {} buffers; last {:?})",
        misfits(&first_buffers, FIRST_GAIN_DB),
        first_buffers.len(),
        first_buffers.last()
    );
    assert_eq!(
        misfits(&second_buffers, SECOND_GAIN_DB),
        0,
        "the head of the second track was scaled by the wrong gain \
         (first {:?})",
        second_buffers.first()
    );

    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

const SYNTHETIC_RATE: i32 = 44_100;
const SYNTHETIC_AMPLITUDE: f32 = 0.25;
const SYNTHETIC_BUFFERS_PER_TRACK: usize = 4;
const SYNTHETIC_FRAMES_PER_BUFFER: usize = 441;

fn constant_buffer() -> gst::Buffer {
    let samples = vec![SYNTHETIC_AMPLITUDE; SYNTHETIC_FRAMES_PER_BUFFER];
    gst::Buffer::from_slice(
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect::<Vec<u8>>(),
    )
}

/// Deterministic proof that the switch rides the data. The audible side of the
/// filter is held shut while track A's tail and track B's `STREAM_START` are
/// pushed in, so A's tail waits in the playback queue exactly as it does when
/// the sink is slower than the decoder. Measured on the actual samples: every
/// buffer of A must still be scaled by A's gain once the gate opens.
#[test]
fn play_19a_the_gain_switch_waits_for_the_tail_queued_ahead_of_it() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    gst::init().unwrap();
    let filter = build_audio_filter(&AudioEffects::default())
        .unwrap()
        .unwrap();
    let pending: crate::gapless::PendingGain = Arc::new(Mutex::new(None));
    let gain = filter
        .clone()
        .downcast::<gst::Bin>()
        .unwrap()
        .by_name("reprise-track-gain")
        .unwrap();
    // The queue thread that feeds the gain element is asynchronous to the
    // thread that pushes into the bin. Holding the first STREAM_START back
    // makes the slowest schedule the only one: the pending gain must be set
    // only once track A's STREAM_START has passed the switch, as it is in
    // playback, where `about-to-finish` fires long after the stream began.
    let gain_sink = gain.static_pad("sink").unwrap();
    let first_stream_start_held = std::sync::atomic::AtomicBool::new(false);
    gain_sink.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
        let is_stream_start = info
            .event()
            .is_some_and(|event| event.type_() == gst::EventType::StreamStart);
        if is_stream_start
            && !first_stream_start_held.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            std::thread::sleep(QUEUE_THREAD_STALL);
        }
        gst::PadProbeReturn::Ok
    });
    crate::player_effects::install_filter_gain_switch(&filter, pending.clone()).unwrap();
    // Added after the switch, so it runs after the switch on the same event.
    let a_stream_started = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let a_stream_started = a_stream_started.clone();
        gain_sink.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
            if info
                .event()
                .is_some_and(|event| event.type_() == gst::EventType::StreamStart)
            {
                a_stream_started.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            gst::PadProbeReturn::Ok
        });
    }
    gain.set_property("volume", linear(FIRST_GAIN_DB));

    let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let heard = Arc::new(Mutex::new(Vec::<(usize, f32)>::new()));
    let streams_seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    {
        let (gate, heard, streams_seen) = (gate.clone(), heard.clone(), streams_seen.clone());
        gain.static_pad("src").unwrap().add_probe(
            gst::PadProbeType::BUFFER | gst::PadProbeType::EVENT_DOWNSTREAM,
            move |_, info| {
                let (open, signal) = &*gate;
                let mut open = open.lock().unwrap_or_else(PoisonError::into_inner);
                while !*open {
                    open = signal.wait(open).unwrap_or_else(PoisonError::into_inner);
                }
                match &info.data {
                    Some(gst::PadProbeData::Event(event))
                        if event.type_() == gst::EventType::StreamStart =>
                    {
                        streams_seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                    Some(gst::PadProbeData::Buffer(buffer)) => {
                        let map = buffer.map_readable().unwrap();
                        let first = f32::from_le_bytes(map[..4].try_into().unwrap());
                        let seen = streams_seen.load(std::sync::atomic::Ordering::SeqCst);
                        heard
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .push((seen.saturating_sub(1), first));
                    }
                    _ => {}
                }
                gst::PadProbeReturn::Ok
            },
        );
    }

    let pipeline = gst::Pipeline::new();
    let sink = gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .build()
        .unwrap();
    pipeline.add_many([&filter, &sink]).unwrap();
    filter.link(&sink).unwrap();
    let source = gst::Pad::builder(gst::PadDirection::Src).build();
    source.link(&filter.static_pad("sink").unwrap()).unwrap();
    source.set_active(true).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();

    let caps = gst::Caps::builder("audio/x-raw")
        .field("format", "F32LE")
        .field("layout", "interleaved")
        .field("rate", SYNTHETIC_RATE)
        .field("channels", 1_i32)
        .build();
    let push_track = |stream_id: &str| {
        assert!(source.push_event(gst::event::StreamStart::new(stream_id)));
        assert!(source.push_event(gst::event::Caps::new(&caps)));
        assert!(
            source.push_event(gst::event::Segment::new(&gst::FormattedSegment::<
                gst::ClockTime,
            >::new()))
        );
        for _ in 0..SYNTHETIC_BUFFERS_PER_TRACK {
            source.push(constant_buffer()).unwrap();
        }
    };
    push_track("track-a");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !a_stream_started.load(std::sync::atomic::Ordering::SeqCst) {
        assert!(
            std::time::Instant::now() < deadline,
            "track A's STREAM_START never reached the gain element"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    *pending.lock().unwrap_or_else(PoisonError::into_inner) = Some(SECOND_GAIN_DB);
    push_track("track-b");
    {
        let (open, signal) = &*gate;
        *open.lock().unwrap_or_else(PoisonError::into_inner) = true;
        signal.notify_all();
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while heard.lock().unwrap_or_else(PoisonError::into_inner).len()
        < 2 * SYNTHETIC_BUFFERS_PER_TRACK
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    pipeline.set_state(gst::State::Null).unwrap();

    let heard = heard.lock().unwrap_or_else(PoisonError::into_inner);
    let first_gain = SYNTHETIC_AMPLITUDE * linear(FIRST_GAIN_DB) as f32;
    let second_gain = SYNTHETIC_AMPLITUDE * linear(SECOND_GAIN_DB) as f32;
    assert_eq!(heard.len(), 2 * SYNTHETIC_BUFFERS_PER_TRACK);
    for (stream, level) in heard.iter() {
        let expected = if *stream == 0 {
            first_gain
        } else {
            second_gain
        };
        assert!(
            (level - expected).abs() < 1e-5,
            "stream {stream} buffer carried level {level}, expected {expected}; \
             full run: {heard:?}"
        );
    }
}

#[test]
fn play_18_the_gain_element_never_receives_an_unsafe_factor() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    use crate::player_effects::linear_gain;
    use reprise_core::library::loudness::{MAX_GAIN_DB, MIN_GAIN_DB};

    assert_eq!(linear_gain(f64::NAN), 1.0);
    assert_eq!(linear_gain(f64::INFINITY), 1.0);
    assert_eq!(linear_gain(f64::NEG_INFINITY), 1.0);
    // The same -24..+12 dB window Core resolves into and the phone clamps to.
    assert!((linear_gain(400.0) - linear(MAX_GAIN_DB)).abs() < 1e-9);
    assert!((linear_gain(-400.0) - linear(MIN_GAIN_DB)).abs() < 1e-9);
    assert!((linear_gain(MAX_GAIN_DB) - 3.981).abs() < 1e-3);
    assert!((linear_gain(MIN_GAIN_DB) - 0.0631).abs() < 1e-4);
    assert!((linear_gain(6.0) - linear(6.0)).abs() < 1e-12);

    gst::init().unwrap();
    let filter = build_audio_filter(&AudioEffects::default())
        .unwrap()
        .unwrap();
    let gain = filter
        .clone()
        .downcast::<gst::Bin>()
        .unwrap()
        .by_name("reprise-track-gain")
        .unwrap();
    gain.set_property("volume", linear_gain(400.0));
    assert!((gain.property::<f64>("volume") - linear(MAX_GAIN_DB)).abs() < 1e-6);
}
