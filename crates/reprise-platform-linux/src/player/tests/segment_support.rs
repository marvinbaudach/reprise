//! Shared fixtures for the CUE-track playback tests: a generated file made of
//! tone and silence regions, a recorder of what leaves the gain element, and
//! an event pump for the headless pipeline.
//!
//! The regions are the markers: a test plays a CUE track whose stretch starts
//! where a tone starts after silence, so "the first buffer heard is audible
//! and starts at the track's start" proves the decoder really seeked there.

use std::path::Path;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use super::*;
use crate::player_pipeline::AUDIO_SINK_ENV_VAR;

const SAMPLE_RATE: u32 = 44_100;
const TONE_HZ: f64 = 440.0;
const TONE_AMPLITUDE: f64 = 8_000.0;
/// A buffer whose peak reaches this fraction of full scale carries the tone;
/// the tone peaks at about a quarter of full scale, silence at zero.
const AUDIBLE_PEAK: f64 = 0.01;

/// Writes a mono 16-bit WAV made of `regions`, each `(milliseconds, tone)`.
pub(super) fn write_regions_wav(path: &Path, regions: &[(u32, bool)]) {
    let samples: Vec<i16> = regions
        .iter()
        .flat_map(|&(ms, tone)| {
            let count = (u64::from(SAMPLE_RATE) * u64::from(ms) / 1000) as usize;
            (0..count).map(move |index| {
                if !tone {
                    return 0;
                }
                let seconds = index as f64 / f64::from(SAMPLE_RATE);
                ((seconds * TONE_HZ * std::f64::consts::TAU).sin() * TONE_AMPLITUDE) as i16
            })
        })
        .collect();
    let data_size = u32::try_from(samples.len() * 2).unwrap();
    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, wav).unwrap();
}

/// Encodes the WAV at `wav` to a FLAC at `flac` without a seek table. It runs
/// as fast as the pipeline can consume the file; it never uses the audio sink
/// or the wall clock.
pub(super) fn encode_flac(wav: &Path, flac: &Path) {
    let pipeline = gst::Pipeline::new();
    let source = gst::ElementFactory::make("filesrc")
        .property("location", wav.to_str().unwrap())
        .build()
        .unwrap();
    let wav_parse = gst::ElementFactory::make("wavparse").build().unwrap();
    let convert = gst::ElementFactory::make("audioconvert").build().unwrap();
    let caps_filter = gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::Caps::builder("audio/x-raw")
                .field("format", "S16LE")
                .field("channels", 2_i32)
                .build(),
        )
        .build()
        .unwrap();
    let encoder = gst::ElementFactory::make("flacenc")
        .property("seekpoints", 0_i32)
        .property("blocksize", 4_096_u32)
        .property("padding", 8_192_u32)
        .build()
        .unwrap();
    let sink = gst::ElementFactory::make("filesink")
        .property("location", flac.to_str().unwrap())
        .build()
        .unwrap();
    pipeline
        .add_many([&source, &wav_parse, &convert, &caps_filter, &encoder, &sink])
        .unwrap();
    gst::Element::link_many([&source, &wav_parse, &convert, &caps_filter, &encoder, &sink])
        .unwrap();

    pipeline.set_state(gst::State::Playing).unwrap();
    let message = pipeline
        .bus()
        .unwrap()
        .timed_pop_filtered(
            gst::ClockTime::from_seconds(20),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        )
        .expect("FLAC fixture encoding must finish");
    if let gst::MessageView::Error(error) = message.view() {
        panic!("FLAC fixture encoding failed: {error:?}");
    }
    pipeline.set_state(gst::State::Null).unwrap();
}

/// One buffer as it leaves the gain element.
#[derive(Clone, Copy, Debug)]
pub(super) struct HeardBuffer {
    /// Stream time of the buffer's first sample, in the file's own clock.
    pub(super) start_ms: i64,
    pub(super) audible: bool,
    /// The linear gain the element held while it processed the buffer.
    pub(super) linear_gain: f64,
}

pub(super) fn gain_element(player: &Player) -> gst::Element {
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

/// What left the gain element since the last `SEGMENT` event, and how many
/// segment events arrived — a flushing seek is over once a new one has.
#[derive(Clone, Default)]
pub(super) struct HeardLog {
    buffers: Arc<Mutex<Vec<HeardBuffer>>>,
    segments: Arc<std::sync::atomic::AtomicUsize>,
    stream_starts: Arc<std::sync::atomic::AtomicUsize>,
    landings: Arc<Mutex<Landings>>,
}

/// Where each distinct seek of the stream first became audible.
///
/// One flushing seek can reach the gain element as several `SEGMENT` events
/// with the same seqnum, a few milliseconds apart — the seek went to more than
/// one sink of `playbin3`'s bin and each copy restarted the stream. They are
/// one seek, so `segments` and `buffers` (which restart on every copy) cannot
/// tell a test whether a new seek has landed; the seqnum can.
#[derive(Default)]
struct Landings {
    last_seqnum: Option<gst::Seqnum>,
    awaiting_buffer: bool,
    heard: Vec<HeardBuffer>,
}

impl Landings {
    fn segment(&mut self, seqnum: gst::Seqnum) {
        if self.last_seqnum != Some(seqnum) {
            self.last_seqnum = Some(seqnum);
            self.awaiting_buffer = true;
        }
    }

    fn buffer(&mut self, buffer: HeardBuffer) {
        if std::mem::take(&mut self.awaiting_buffer) {
            self.heard.push(buffer);
        }
    }
}

impl HeardLog {
    pub(super) fn buffers(&self) -> Vec<HeardBuffer> {
        self.buffers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(super) fn segments(&self) -> usize {
        self.segments.load(Ordering::SeqCst)
    }

    /// How many `STREAM_START` events left the gain element.
    pub(super) fn stream_starts(&self) -> usize {
        self.stream_starts.load(Ordering::SeqCst)
    }

    /// Whether at least `segments` segment events arrived and a buffer
    /// followed the last of them.
    pub(super) fn heard_after(&self, segments: usize) -> bool {
        self.segments() >= segments && !self.buffers().is_empty()
    }

    /// How many distinct seeks (the file's own start counts as one) have a
    /// buffer heard behind them.
    pub(super) fn landings(&self) -> usize {
        self.landings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .heard
            .len()
    }

    /// The first buffer heard after the most recent distinct seek.
    pub(super) fn latest_landing(&self) -> HeardBuffer {
        *self
            .landings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .heard
            .last()
            .unwrap_or_else(|| {
                panic!(
                    "expected a seek to land at the gain element (segments {}, stream starts {})",
                    self.segments(),
                    self.stream_starts()
                )
            })
    }
}

/// Records every buffer leaving the gain element since the last `SEGMENT`
/// event — so after a flushing seek only what the seek produced is kept.
pub(super) fn record_heard(player: &Player) -> HeardLog {
    let heard = HeardLog::default();
    let recorded = heard.buffers.clone();
    let segments = heard.segments.clone();
    let gain = gain_element(player);
    let element = gain.clone();
    let stream_starts = heard.stream_starts.clone();
    let landings = heard.landings.clone();
    let stream_segment = Mutex::new(None::<gst::FormattedSegment<gst::ClockTime>>);
    gain.static_pad("src").unwrap().add_probe(
        gst::PadProbeType::BUFFER | gst::PadProbeType::EVENT_DOWNSTREAM,
        move |pad, info| {
            match &info.data {
                Some(gst::PadProbeData::Event(event)) => {
                    if event.type_() == gst::EventType::StreamStart {
                        stream_starts.fetch_add(1, Ordering::SeqCst);
                    }
                    if let gst::EventView::Segment(event) = event.view() {
                        *stream_segment
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner) =
                            event.segment().downcast_ref::<gst::ClockTime>().cloned();
                        recorded
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .clear();
                        segments.fetch_add(1, Ordering::SeqCst);
                        landings
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .segment(event.seqnum());
                    }
                }
                Some(gst::PadProbeData::Buffer(buffer)) => {
                    let stream_time = buffer.pts().and_then(|pts| {
                        stream_segment
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .as_ref()
                            .and_then(|segment| segment.to_stream_time(pts))
                    });
                    let format = pad.current_caps().and_then(|caps| {
                        caps.structure(0)
                            .and_then(|structure| structure.get::<String>("format").ok())
                    });
                    if let Some(stream_time) = stream_time {
                        let heard = HeardBuffer {
                            start_ms: stream_time.mseconds() as i64,
                            audible: peak(buffer, format.as_deref()) >= AUDIBLE_PEAK,
                            linear_gain: element.property::<f64>("volume"),
                        };
                        recorded
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .push(heard);
                        landings
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .buffer(heard);
                    }
                }
                _ => {}
            }
            gst::PadProbeReturn::Ok
        },
    );
    heard
}

/// The buffer's peak as a fraction of full scale, for the raw formats the
/// filter negotiates; an unknown format reads as silent.
fn peak(buffer: &gst::BufferRef, format: Option<&str>) -> f64 {
    let Ok(map) = buffer.map_readable() else {
        return 0.0;
    };
    let bytes = map.as_slice();
    match format {
        Some("S16LE") => bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|chunk| f64::from(i16::from_le_bytes(*chunk)).abs() / f64::from(i16::MAX))
            .fold(0.0, f64::max),
        Some("S32LE") => bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| f64::from(i32::from_le_bytes(*chunk)).abs() / f64::from(i32::MAX))
            .fold(0.0, f64::max),
        Some("F32LE") => bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| f64::from(f32::from_le_bytes(*chunk)).abs())
            .fold(0.0, f64::max),
        Some("F64LE") => bytes
            .as_chunks::<8>()
            .0
            .iter()
            .map(|chunk| f64::from_le_bytes(*chunk).abs())
            .fold(0.0, f64::max),
        _ => 0.0,
    }
}

/// The end, in the file's own clock, of the last buffer a sink has rendered.
#[derive(Clone, Default)]
pub(super) struct RenderedLog {
    end_ms: Arc<std::sync::atomic::AtomicI64>,
}

impl RenderedLog {
    pub(super) fn end_ms(&self) -> i64 {
        self.end_ms.load(Ordering::SeqCst)
    }
}

/// Latency of the emulated ring buffer, comfortably above what a real audio
/// sink holds, so a boundary that does not wait for the sink misses by it.
const SINK_BACKLOG_NANOS: u64 = 400_000_000;

/// Swaps the player's audio sink for a queue ahead of a clock-synced
/// `fakesink`: like a real sink's ring buffer, the queue accepts audio well
/// before it is rendered, and an end-of-stream drains through it. A plain
/// `fakesink` has no such lead, so it cannot tell a track that was heard to its
/// end from one that was cut off while the sink still held its tail. What the
/// returned log reports was rendered on the clock, not merely written.
pub(super) fn slow_sink(player: &Player) -> RenderedLog {
    let rendered = RenderedLog::default();
    slow_sink_into(player, &rendered);
    rendered
}

/// [`slow_sink`], reporting into a log the caller made beforehand — so an
/// event observer built before the player can read it.
pub(super) fn slow_sink_into(player: &Player, rendered: &RenderedLog) {
    let sink = gst::parse::bin_from_description(
        &format!(
            "queue max-size-time={SINK_BACKLOG_NANOS} max-size-buffers=0 max-size-bytes=0 \
             ! fakesink name=rendered sync=true signal-handoffs=true"
        ),
        true,
    )
    .unwrap();
    let end_ms = rendered.end_ms.clone();
    sink.by_name("rendered")
        .unwrap()
        .connect("handoff", false, move |values| {
            let buffer = values[1].get::<gst::Buffer>().unwrap();
            if let Some(pts) = buffer.pts() {
                let end = pts + buffer.duration().unwrap_or(gst::ClockTime::ZERO);
                end_ms.store(end.mseconds() as i64, Ordering::SeqCst);
            }
            None
        });
    player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .set_property("audio-sink", &sink);
}

/// How long a pipeline may sit in READY with its first state change pending
/// before the start counts as hung. A healthy start leaves READY within
/// milliseconds, so this is far beyond any load.
const HUNG_START_PATIENCE: Duration = Duration::from_secs(5);
/// Starts tried before a test gives up; every one of them hanging is not luck.
const START_ATTEMPTS: usize = 5;

/// [`Harness::start`] for a test that builds its own [`Player`]: runs `begin`
/// and starts it over, up to [`START_ATTEMPTS`] times, while the pipeline
/// hangs in READY.
pub(super) fn start_over_when_hung(player: &Player, begin: impl Fn()) {
    for attempt in 1..=START_ATTEMPTS {
        begin();
        if leaves_ready(player, HUNG_START_PATIENCE) {
            return;
        }
        eprintln!("the pipeline hung in READY on start {attempt}; starting over");
    }
    panic!("playbin3 hung in READY on all {START_ATTEMPTS} starts");
}

/// Whether the player's pipeline gets past READY within `patience`.
fn leaves_ready(player: &Player, patience: Duration) -> bool {
    let main_context = gst::glib::MainContext::default();
    let deadline = Instant::now() + patience;
    while Instant::now() < deadline {
        let playbin = player
            .playbin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if playbin.current_state() != gst::State::Ready {
            return true;
        }
        main_context.iteration(false);
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

/// A headless player on `fakesink`, its events, and the sink lock held for
/// the harness's whole life.
pub(super) struct Harness {
    pub(super) player: Player,
    events: Receiver<PlayerEvent>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Harness {
    pub(super) fn new() -> Self {
        Self::observing(|_| {})
    }

    /// [`Self::new`], with `observe` called on every event at the instant the
    /// player emits it — before the event pump, which can lag under load,
    /// gets to it.
    pub(super) fn observing(observe: impl Fn(&PlayerEvent) + Send + Sync + 'static) -> Self {
        let guard = AUDIO_SINK_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");
        let (tx, events) = std::sync::mpsc::channel::<PlayerEvent>();
        let player = Player::new(Box::new(move |event| {
            observe(&event);
            let _ = tx.send(event);
        }))
        .unwrap();
        Self {
            player,
            events,
            _guard: guard,
        }
    }

    /// Runs `begin` — the `play` of a track and whatever the test does at once
    /// after it — and starts it over if the pipeline hangs in READY.
    ///
    /// GStreamer 1.28's `playbin3` sometimes never leaves READY on a start:
    /// the stream arrives while `decodebin3` is still being brought up and the
    /// state change then never completes, whatever the file (a bare `playbin3`
    /// on a `fakesink` does it too, one start in 20 to 100 here). Nothing a
    /// test could wait for ends that, so a start that has not left READY after
    /// [`HUNG_START_PATIENCE`] is begun again from `Null`, which `play` does.
    pub(super) fn start(&self, begin: impl Fn()) {
        start_over_when_hung(&self.player, begin);
    }

    /// Pumps the main context (which dispatches the bus watch) and collects
    /// events until `done` holds for the collected list or `timeout` passes.
    pub(super) fn pump_until(
        &self,
        timeout: Duration,
        done: impl Fn(&[PlayerEvent]) -> bool,
    ) -> Vec<PlayerEvent> {
        let main_context = gst::glib::MainContext::default();
        let deadline = Instant::now() + timeout;
        let mut collected = Vec::new();
        while Instant::now() < deadline {
            main_context.iteration(false);
            collected.extend(self.events.try_iter());
            if done(&collected) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        collected
    }

    /// Pumps for exactly `duration`, collecting every event.
    pub(super) fn pump_for(&self, duration: Duration) -> Vec<PlayerEvent> {
        self.pump_until(duration, |_| false)
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.player.stop();
        std::env::remove_var(AUDIO_SINK_ENV_VAR);
    }
}

pub(super) fn ticks(events: &[PlayerEvent]) -> Vec<(i64, i64)> {
    events
        .iter()
        .filter_map(|event| match event {
            PlayerEvent::Position {
                position_ms,
                duration_ms,
            } => Some((*position_ms, *duration_ms)),
            _ => None,
        })
        .collect()
}

pub(super) fn count(events: &[PlayerEvent], wanted: fn(&PlayerEvent) -> bool) -> usize {
    events.iter().filter(|event| wanted(event)).count()
}

pub(super) fn linear(gain_db: f64) -> f64 {
    10_f64.powf(gain_db / 20.0)
}

pub(super) fn cue_item(path: &Path, segment: (i64, i64), gain_db: f64) -> PlaybackItem<'_> {
    PlaybackItem {
        path: path.to_str().unwrap(),
        gain_db,
        segment: Some(segment),
    }
}
