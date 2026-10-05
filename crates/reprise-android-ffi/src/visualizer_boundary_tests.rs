// AC-29 at the Android engine seam: Media3's burst-fed PCM and the 60 Hz
// vsync tick, across the events that restart the live processor. Every run is
// judged against the frames the same audio draws on an engine that has been
// playing it for long enough to have settled.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use reprise_core::playback::boundary_fixture::{
    assert_no_dip, judge_boundary, Frame, SyntheticMusic, FRAMES_PER_SECOND, JUDGED_FRAMES,
    SETTLE_FRAMES,
};

use super::{AndroidVisualEngine, MonotonicClock};

const LOUD_GAIN: f32 = 0.9;
const LEVEL_STEP_DB: f32 = 14.0;
const SAMPLE_RATE_HZ: u32 = 48_000;
const PREVIOUS_RATE_HZ: u32 = 44_100;
const WARM_SECONDS: f32 = 12.0;
const BOUNDARY_SECONDS: f32 = 30.0;
/// Seconds recorded after the boundary: enough for every judged frame at 60 Hz
/// even when vsyncs are missed.
const RECORDED_SECONDS: f32 = 2.8;
const RECORDED_FRAMES: usize = SETTLE_FRAMES + JUDGED_FRAMES + FRAMES_PER_SECOND;
/// Media3 hands the tap PCM in bursts: the first after a (re)start fills the
/// whole AudioTrack buffer, later ones about every 250 ms.
const FIRST_BURST_MS: u64 = 500;
const BURST_PERIOD_MS: u64 = 250;
const BURST_JITTER_MS: f32 = 20.0;
const VSYNC_JITTER_MS: f32 = 1.5;
/// Every n-th vsync is missed; the next tick then spans two periods.
const MISSED_VSYNC_EVERY: usize = 97;
const CHUNK_FRAMES: usize = 1_024;

struct TestClock(AtomicU64);

impl TestClock {
    fn set(&self, now: Duration) {
        self.0.store(
            now.as_nanos().try_into().expect("fits u64"),
            Ordering::Relaxed,
        );
    }
}

impl MonotonicClock for TestClock {
    fn now(&self) -> Duration {
        Duration::from_nanos(self.0.load(Ordering::Relaxed))
    }
}

/// Deterministic jitter in `-1..1`.
struct Jitter(u64);

impl Jitter {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32) / (1_u64 << 23) as f32 - 1.0
    }
}

/// One engine on a fake clock, fed like the phone feeds it.
struct Phone {
    engine: AndroidVisualEngine,
    clock: Arc<TestClock>,
    now: Duration,
    vsyncs: usize,
    jitter: Jitter,
}

impl Phone {
    fn new() -> Self {
        let clock = Arc::new(TestClock(AtomicU64::new(1_000_000_000)));
        let engine = AndroidVisualEngine::with_clock(clock.clone());
        engine.set_playing(true);
        engine.set_playback_intended(true);
        Self {
            engine,
            now: clock.now(),
            clock,
            vsyncs: 0,
            jitter: Jitter(11),
        }
    }

    /// Plays `music` from `from_seconds` for `seconds`, returning one frame per
    /// vsync that fired.
    fn play(&mut self, music: &SyntheticMusic, from_seconds: f32, seconds: f32) -> Vec<Frame> {
        let rate = music.rate_hz() as usize;
        let mut position = (from_seconds * rate as f32) as usize;
        let end = position + (seconds * rate as f32) as usize;
        let mut next_burst = self.now;
        let mut burst_ms = FIRST_BURST_MS;
        let stop = self.now + Duration::from_secs_f32(seconds);
        let mut frames = Vec::new();
        while self.now < stop {
            let period_ms = 1_000.0 / FRAMES_PER_SECOND as f32;
            let vsync = self.now
                + Duration::from_secs_f32(
                    (period_ms + self.jitter.next() * VSYNC_JITTER_MS) / 1_000.0,
                );
            while next_burst <= vsync && position < end {
                let burst_frames = (burst_ms as usize * rate / 1_000).min(end - position);
                self.feed(music, position, burst_frames);
                position += burst_frames;
                burst_ms = BURST_PERIOD_MS;
                let wobble = self.jitter.next() * BURST_JITTER_MS;
                next_burst += Duration::from_secs_f32((BURST_PERIOD_MS as f32 + wobble) / 1_000.0);
            }
            self.clock.set(vsync);
            self.now = vsync;
            self.vsyncs += 1;
            if !self.vsyncs.is_multiple_of(MISSED_VSYNC_EVERY) {
                self.engine.tick();
                frames.push(self.frame());
            }
        }
        frames
    }

    fn feed(&self, music: &SyntheticMusic, start: usize, frames: usize) {
        let mut done = 0;
        while done < frames {
            let take = (frames - done).min(CHUNK_FRAMES);
            let bytes = music.stereo_i16_bytes(start + done, take);
            let byte_count = bytes.len() as u32;
            self.engine
                .ingest_pcm_i16(bytes, byte_count, music.rate_hz(), 2);
            done += take;
        }
    }

    fn frame(&self) -> Frame {
        self.engine
            .current_bands()
            .try_into()
            .expect("the engine draws 64 bars")
    }

    /// What the Kotlin side does when a swipe or a skip starts another song:
    /// Media3 flushes, the panel notes the change, and the bars on screen are
    /// handed back as the shape to continue from.
    fn change_track(&self) {
        let shown = self.engine.current_bands();
        self.engine.reset_audio_stream();
        self.engine.note_track_changed();
        self.engine.adopt_shape(shown);
    }
}

fn loud() -> SyntheticMusic {
    SyntheticMusic::new(1, SAMPLE_RATE_HZ, LOUD_GAIN)
}

fn quiet() -> SyntheticMusic {
    loud().louder_by(-LEVEL_STEP_DB)
}

/// The frames `music` draws on an engine that has been playing it all along.
fn settled_reference(music: &SyntheticMusic) -> Vec<Frame> {
    let mut phone = Phone::new();
    phone.play(music, BOUNDARY_SECONDS - WARM_SECONDS, WARM_SECONDS);
    let frames = phone.play(music, BOUNDARY_SECONDS, RECORDED_SECONDS);
    assert!(frames.len() >= RECORDED_FRAMES);
    frames
}

/// `previous` plays up to `previous_until_seconds`, the track changes, `next`
/// plays from the boundary on.
fn after_track_change_at(
    previous: &SyntheticMusic,
    previous_until_seconds: f32,
    next: &SyntheticMusic,
) -> Vec<Frame> {
    let mut phone = Phone::new();
    phone.play(
        previous,
        previous_until_seconds - WARM_SECONDS,
        WARM_SECONDS,
    );
    phone.change_track();
    let frames = phone.play(next, BOUNDARY_SECONDS, RECORDED_SECONDS);
    assert!(frames.len() >= RECORDED_FRAMES);
    frames
}

fn after_track_change(previous: &SyntheticMusic, next: &SyntheticMusic) -> Vec<Frame> {
    after_track_change_at(previous, WARM_SECONDS, next)
}

#[test]
fn ac_29_the_first_pcm_is_measured_instead_of_swelling_from_a_cold_start() {
    for music in [loud(), quiet()] {
        let frames = Phone::new().play(&music, BOUNDARY_SECONDS, RECORDED_SECONDS);

        judge_boundary("first pcm", &frames, &settled_reference(&music), false);
    }
}

#[test]
fn ac_29_a_swiped_to_song_14_db_louder_does_not_hit_the_ceiling() {
    let frames = after_track_change(&quiet(), &loud());

    judge_boundary("quiet to loud", &frames, &settled_reference(&loud()), true);
}

#[test]
fn ac_29_a_swiped_to_song_14_db_quieter_is_not_left_dim() {
    let frames = after_track_change(&loud(), &quiet());

    judge_boundary("loud to quiet", &frames, &settled_reference(&quiet()), true);
}

#[test]
fn ac_29_a_swiped_to_song_of_the_same_loudness_keeps_its_height() {
    let other_song = SyntheticMusic::new(2, SAMPLE_RATE_HZ, LOUD_GAIN);
    let frames = after_track_change(&other_song, &loud());

    judge_boundary("same loudness", &frames, &settled_reference(&loud()), true);
}

#[test]
fn ac_29_a_sample_rate_change_is_measured_like_a_fresh_start() {
    let before = quiet().at_rate(PREVIOUS_RATE_HZ);
    let mut phone = Phone::new();
    phone.play(&before, 0.0, WARM_SECONDS);
    phone.change_track();

    let frames = phone.play(&quiet(), BOUNDARY_SECONDS, RECORDED_SECONDS);

    // The new rate builds a new processor, which starts from nothing like a
    // fresh start; only the height it settles at is judged.
    judge_boundary("rate change", &frames, &settled_reference(&quiet()), false);
}

#[test]
fn ac_29_a_swipe_to_the_same_song_does_not_shrink_the_frame_it_continues() {
    let frames = after_track_change_at(&loud(), BOUNDARY_SECONDS, &loud());

    judge_boundary("same song", &frames, &settled_reference(&loud()), true);
    assert_no_dip("same song", &frames, &settled_reference(&loud()));
}
