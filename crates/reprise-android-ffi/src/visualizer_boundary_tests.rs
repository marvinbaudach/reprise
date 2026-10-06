// AC-29 at the Android engine seam: Media3's burst-fed PCM and the 60 Hz
// vsync tick, across the events that restart the live processor. Every run is
// judged against the frames the same audio draws on an engine that has been
// playing it for long enough to have settled.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use reprise_core::playback::boundary_fixture::{
    assert_no_dip, dimming_complaints, FADE_LEVEL_FLOOR, INTRO_LEVEL_FLOOR, frame_mean, judge_boundary, pinning_complaints, settled_seconds, Boundary, Frame, Measure,
    Opening, SyntheticMusic, BOUNDARY_OFFSETS_SECONDS, FRAMES_PER_SECOND, JUDGED_FRAMES,
    SETTLE_FRAMES,
};

use super::{AndroidVisualEngine, MonotonicClock};

const LOUD_GAIN: f32 = 0.9;
const LEVEL_STEP_DB: f32 = 14.0;
const SAMPLE_RATE_HZ: u32 = 48_000;
const PREVIOUS_RATE_HZ: u32 = 44_100;
const WARM_SECONDS: f32 = 12.0;
/// How long the engine an opening is judged against has played: past the span
/// in which a boundary brakes, at the rate it is fed.
fn settled_for() -> f32 {
    settled_seconds(SAMPLE_RATE_HZ) as f32
}
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
        self.play_with(music.rate_hz(), from_seconds, seconds, |start, frames| {
            music.stereo_i16_bytes(start, frames)
        })
    }

    /// Like [`Self::play`] for a song whose PCM is rendered by `render`, given
    /// a start sample and a frame count.
    fn play_with(
        &mut self,
        rate_hz: u32,
        from_seconds: f32,
        seconds: f32,
        render: impl Fn(usize, usize) -> Vec<u8>,
    ) -> Vec<Frame> {
        let rate = rate_hz as usize;
        let mut position = (from_seconds * rate as f32) as usize;
        let end = position + (seconds * rate as f32) as usize;
        let mut next_burst = self.now;
        let mut burst_ms = FIRST_BURST_MS;
        let stop = self.now + Duration::from_secs_f32(seconds);
        let mut frames = Vec::new();
        while self.now < stop {
            let vsync = self.next_vsync();
            while next_burst <= vsync && position < end {
                let burst_frames = (burst_ms as usize * rate / 1_000).min(end - position);
                self.feed(rate_hz, position, burst_frames, &render);
                position += burst_frames;
                burst_ms = BURST_PERIOD_MS;
                let wobble = self.jitter.next() * BURST_JITTER_MS;
                next_burst += Duration::from_secs_f32((BURST_PERIOD_MS as f32 + wobble) / 1_000.0);
            }
            if let Some(frame) = self.tick_at(vsync) {
                frames.push(frame);
            }
        }
        frames
    }

    /// Vsyncs with no PCM arriving, as while paused.
    fn idle(&mut self, seconds: f32) {
        let stop = self.now + Duration::from_secs_f32(seconds);
        while self.now < stop {
            let vsync = self.next_vsync();
            self.tick_at(vsync);
        }
    }

    fn next_vsync(&mut self) -> Duration {
        let period_ms = 1_000.0 / FRAMES_PER_SECOND as f32;
        self.now
            + Duration::from_secs_f32((period_ms + self.jitter.next() * VSYNC_JITTER_MS) / 1_000.0)
    }

    fn tick_at(&mut self, vsync: Duration) -> Option<Frame> {
        self.clock.set(vsync);
        self.now = vsync;
        self.vsyncs += 1;
        if self.vsyncs.is_multiple_of(MISSED_VSYNC_EVERY) {
            return None;
        }
        self.engine.tick();
        Some(self.frame())
    }

    fn feed(
        &self,
        rate_hz: u32,
        start: usize,
        frames: usize,
        render: &impl Fn(usize, usize) -> Vec<u8>,
    ) {
        let mut done = 0;
        while done < frames {
            let take = (frames - done).min(CHUNK_FRAMES);
            let bytes = render(start + done, take);
            let byte_count = bytes.len() as u32;
            self.engine.ingest_pcm_i16(bytes, byte_count, rate_hz, 2);
            done += take;
        }
    }

    /// The live processor's gain.
    fn gain(&self) -> f32 {
        self.engine
            .lock_live_audio()
            .as_ref()
            .expect("the engine has a live processor")
            .processor
            .sensitivity()
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

    /// What the Kotlin side does on `onIsPlayingChanged(true)` after a pause:
    /// the intent and playing flags flip and the decoder history is dropped.
    fn pause_for(&mut self, seconds: f32) {
        self.engine.set_playing(false);
        self.engine.set_playback_intended(false);
        self.idle(seconds);
    }

    fn resume(&self) {
        self.engine.set_playback_intended(true);
        self.engine.reset_audio_history();
        self.engine.set_playing(true);
    }
}

fn loud() -> SyntheticMusic {
    SyntheticMusic::new(1, SAMPLE_RATE_HZ, LOUD_GAIN)
}

fn quiet() -> SyntheticMusic {
    loud().louder_by(-LEVEL_STEP_DB)
}

/// Where in a song a boundary lands, `offset_seconds` past `BOUNDARY_SECONDS`.
fn boundary_at(offset_seconds: f32) -> f32 {
    BOUNDARY_SECONDS + offset_seconds
}

/// The frames `music` draws on an engine that has been playing it all along.
fn settled_reference(music: &SyntheticMusic, offset_seconds: f32) -> Vec<Frame> {
    let at = boundary_at(offset_seconds);
    let mut phone = Phone::new();
    phone.play(music, at - WARM_SECONDS, WARM_SECONDS);
    let frames = phone.play(music, at, RECORDED_SECONDS);
    assert!(frames.len() >= RECORDED_FRAMES);
    frames
}

/// `previous` plays up to `previous_until_seconds`, the track changes, `next`
/// plays from the boundary on.
fn after_track_change_at(
    previous: &SyntheticMusic,
    previous_until_seconds: f32,
    next: &SyntheticMusic,
    offset_seconds: f32,
) -> Vec<Frame> {
    let mut phone = Phone::new();
    phone.play(
        previous,
        previous_until_seconds - WARM_SECONDS,
        WARM_SECONDS,
    );
    phone.change_track();
    let frames = phone.play(next, boundary_at(offset_seconds), RECORDED_SECONDS);
    assert!(frames.len() >= RECORDED_FRAMES);
    frames
}

fn after_track_change(
    previous: &SyntheticMusic,
    next: &SyntheticMusic,
    offset_seconds: f32,
) -> Vec<Frame> {
    after_track_change_at(previous, WARM_SECONDS, next, offset_seconds)
}

#[test]
fn ac_29_the_first_pcm_is_measured_instead_of_swelling_from_a_cold_start() {
    for offset in BOUNDARY_OFFSETS_SECONDS {
        for music in [loud(), quiet()] {
            let frames = Phone::new().play(&music, boundary_at(offset), RECORDED_SECONDS);

            judge_boundary(
                &format!("first pcm at +{offset}"),
                &frames,
                &settled_reference(&music, offset),
                Boundary::Fresh,
            );
        }
    }
}

#[test]
fn ac_29_a_swiped_to_song_14_db_louder_does_not_hit_the_ceiling() {
    for offset in BOUNDARY_OFFSETS_SECONDS {
        let frames = after_track_change(&quiet(), &loud(), offset);

        judge_boundary(
            &format!("quiet to loud at +{offset}"),
            &frames,
            &settled_reference(&loud(), offset),
            Boundary::Different,
        );
    }
}

#[test]
fn ac_29_a_swiped_to_song_14_db_quieter_is_not_left_dim() {
    for offset in BOUNDARY_OFFSETS_SECONDS {
        let frames = after_track_change(&loud(), &quiet(), offset);

        judge_boundary(
            &format!("loud to quiet at +{offset}"),
            &frames,
            &settled_reference(&quiet(), offset),
            Boundary::Different,
        );
    }
}

#[test]
fn ac_29_a_swiped_to_song_of_the_same_loudness_keeps_its_height() {
    let other_song = SyntheticMusic::new(2, SAMPLE_RATE_HZ, LOUD_GAIN);
    for offset in BOUNDARY_OFFSETS_SECONDS {
        let frames = after_track_change(&other_song, &loud(), offset);

        judge_boundary(
            &format!("same loudness at +{offset}"),
            &frames,
            &settled_reference(&loud(), offset),
            Boundary::Continuing,
        );
    }
}

#[test]
fn ac_29_a_sample_rate_change_is_measured_like_a_fresh_start() {
    let before = quiet().at_rate(PREVIOUS_RATE_HZ);
    for offset in BOUNDARY_OFFSETS_SECONDS {
        let mut phone = Phone::new();
        phone.play(&before, 0.0, WARM_SECONDS);
        phone.change_track();

        let frames = phone.play(&quiet(), boundary_at(offset), RECORDED_SECONDS);

        // The new rate builds a new processor, which starts from nothing like a
        // fresh start.
        judge_boundary(
            &format!("rate change at +{offset}"),
            &frames,
            &settled_reference(&quiet(), offset),
            Boundary::Fresh,
        );
    }
}

#[test]
fn ac_29_a_swipe_to_the_same_song_does_not_shrink_the_frame_it_continues() {
    for offset in BOUNDARY_OFFSETS_SECONDS {
        let frames = after_track_change_at(&loud(), boundary_at(offset), &loud(), offset);
        let reference = settled_reference(&loud(), offset);

        judge_boundary(
            &format!("same song at +{offset}"),
            &frames,
            &reference,
            Boundary::Continuing,
        );
        assert_no_dip(&format!("same song at +{offset}"), &frames, &reference);
    }
}

// A swipe seeds the shape on screen into the new stream's processor while the
// new song's level moves the gain five-fold. The seeded bars carry on from
// where they stood: they neither jump up nor vanish while the window fills.
#[test]
fn ac_29_a_seeded_shape_survives_a_gain_that_moves_more_than_threefold() {
    const SEED_STAYS_WITHIN: (f32, f32) = (0.3, 2.2);
    const MOVES_MORE_THAN: f32 = 3.0;
    for (previous, next) in [(quiet(), loud()), (loud(), quiet())] {
        let mut phone = Phone::new();
        phone.play(&previous, 0.0, WARM_SECONDS);
        let seed_level = frame_mean(&phone.frame());
        let gain_before = phone.gain();

        phone.change_track();
        let frames = phone.play(&next, BOUNDARY_SECONDS, RECORDED_SECONDS);

        let moved = (phone.gain() / gain_before).max(gain_before / phone.gain());
        assert!(
            moved > MOVES_MORE_THAN,
            "the fixture moved the gain only {moved:.1}-fold, so it proved nothing"
        );

        for (index, frame) in frames.iter().take(SETTLE_FRAMES).enumerate() {
            let ratio = frame_mean(frame) / seed_level;
            assert!(
                (SEED_STAYS_WITHIN.0..=SEED_STAYS_WITHIN.1).contains(&ratio),
                "frame {index} after the swipe is {ratio:.2} times the seeded shape"
            );
        }
    }
}

/// A song that is loud, drops to a quiet passage between `QUIET_FROM` and
/// `QUIET_TO` seconds, and is loud again.
const QUIET_FROM_SECONDS: f32 = 8.0;
const QUIET_TO_SECONDS: f32 = 16.0;
const QUIET_PASSAGE_DB: f32 = 20.0;

fn with_a_quiet_passage(start: usize, frames: usize) -> Vec<u8> {
    let at = start as f32 / SAMPLE_RATE_HZ as f32;
    let music = if (QUIET_FROM_SECONDS..QUIET_TO_SECONDS).contains(&at) {
        loud().louder_by(-QUIET_PASSAGE_DB)
    } else {
        loud()
    };
    music.stereo_i16_bytes(start, frames)
}

// `onIsPlayingChanged` false to true calls `reset_audio_history`, buffering
// stalls included, and nothing about the music changed. A resume inside a quiet
// passage must leave the passage as quiet as it is on a phone that never
// paused: the gain it was drawn at stays, and is not measured again from the
// quiet window, which would draw it at full height.
#[test]
fn ac_29_a_resume_keeps_the_gain_and_does_not_redraw_a_quiet_passage_loud() {
    const RESUME_AT_SECONDS: f32 = 9.0;
    const PAUSE_SECONDS: f32 = 2.0;
    const FIRST_FRAME: usize = 20;
    const LAST_FRAME: usize = 80;

    let mut continuing = Phone::new();
    let through = continuing.play_with(
        SAMPLE_RATE_HZ,
        0.0,
        RESUME_AT_SECONDS + 3.0,
        with_a_quiet_passage,
    );
    let continuing_from = (RESUME_AT_SECONDS * FRAMES_PER_SECOND as f32) as usize;

    let mut resumed = Phone::new();
    resumed.play_with(SAMPLE_RATE_HZ, 0.0, RESUME_AT_SECONDS, with_a_quiet_passage);
    resumed.pause_for(PAUSE_SECONDS);
    resumed.resume();
    let after = resumed.play_with(SAMPLE_RATE_HZ, RESUME_AT_SECONDS, 3.0, with_a_quiet_passage);

    let level = |frames: &[Frame]| {
        frames[FIRST_FRAME..LAST_FRAME]
            .iter()
            .map(frame_mean)
            .sum::<f32>()
            / (LAST_FRAME - FIRST_FRAME) as f32
    };
    let ratio = level(&after) / level(&through[continuing_from..]);
    assert!(
        (0.67..=1.5).contains(&ratio),
        "a resume inside a quiet passage draws it {ratio:.2} times as tall as without the pause"
    );
}

#[test]
fn ac_29_a_resume_inside_a_loud_song_does_not_dim_or_swell_it() {
    const RESUME_AT_SECONDS: f32 = 6.0;
    const FIRST_FRAME: usize = 20;
    const LAST_FRAME: usize = 100;

    let mut continuing = Phone::new();
    let through = continuing.play(&loud(), 0.0, RESUME_AT_SECONDS + 3.0);
    let continuing_from = (RESUME_AT_SECONDS * FRAMES_PER_SECOND as f32) as usize;

    let mut resumed = Phone::new();
    resumed.play(&loud(), 0.0, RESUME_AT_SECONDS);
    resumed.pause_for(1.0);
    resumed.resume();
    let after = resumed.play(&loud(), RESUME_AT_SECONDS, 3.0);

    let level = |frames: &[Frame]| {
        frames[FIRST_FRAME..LAST_FRAME]
            .iter()
            .map(frame_mean)
            .sum::<f32>()
            / (LAST_FRAME - FIRST_FRAME) as f32
    };
    let ratio = level(&after) / level(&through[continuing_from..]);
    assert!(
        (0.9..=1.1).contains(&ratio),
        "a resume left the song at {ratio:.2} times its level without the pause"
    );
}

/// Frames judged after a quiet intro ends, three seconds of them.
const BODY_FRAMES: usize = 3 * FRAMES_PER_SECOND;
/// Frames judged from the start of a fade-in, ten seconds of them.
const FADE_FRAMES: usize = 10 * FRAMES_PER_SECOND;
/// Frames more than the settled engine that may touch full height.
const PINNED_SLACK: usize = 6;
/// Vsyncs that miss shift the frame that holds the drop by up to this many.
const MISSED_VSYNC_MARGIN: usize = 8;

/// Plays the loud song with `opening`, from the first start or after a track
/// change from the same loud song, and returns `seconds` of frames.
fn opened_run(opening: Opening, track_change: bool, seconds: f32) -> Vec<Frame> {
    let song = loud().opened_by(opening, (boundary_at(0.0) * SAMPLE_RATE_HZ as f32) as usize);
    let mut phone = Phone::new();
    if track_change {
        phone.play(&loud(), boundary_at(0.0) - settled_for(), settled_for());
        phone.change_track();
    }
    phone.play(&song, boundary_at(0.0), seconds)
}

/// The frames an engine settled on the loud song draws for `song`, which it
/// goes on playing without a boundary.
fn settled_on_the_loud_song(song: &SyntheticMusic, from_seconds: f32, seconds: f32) -> Vec<Frame> {
    let mut phone = Phone::new();
    phone.play(&loud(), boundary_at(0.0) - settled_for(), settled_for());
    phone.play(song, from_seconds, seconds)
}

// A long, quiet intro: the gain was measured on it, and the body has to be
// caught however late it comes, through Media3's bursts and the vsync tick.
#[test]
fn ac_29_a_loud_body_after_a_long_quiet_intro_does_not_pin_the_swiped_to_songs_bars() {
    let mut complaints = Vec::new();
    for (seconds, db) in [(8, 30.0), (10, 14.0)] {
        let opening = Opening::Intro { seconds, db };
        let first = seconds * FRAMES_PER_SECOND - MISSED_VSYNC_MARGIN;
        let reference = settled_on_the_loud_song(
            &loud(),
            boundary_at(0.0) + seconds as f32,
            BODY_FRAMES as f32 / FRAMES_PER_SECOND as f32 + 0.5,
        );
        let reference = Measure::of(&reference[..BODY_FRAMES]);
        let allowed = reference.pinned_frames + PINNED_SLACK;
        for track_change in [false, true] {
            let frames = opened_run(opening, track_change, seconds as f32 + 3.6);
            let label = format!("a {seconds} s intro {db} dB down, track change {track_change}");
            complaints.extend(pinning_complaints(
                &label,
                Measure::of(&frames[first..first + BODY_FRAMES]),
                allowed,
            ));
            // The level is judged from the frame the drop has surely reached, so
            // a few frames of the intro are not counted as dim.
            let body = first + MISSED_VSYNC_MARGIN;
            complaints.extend(dimming_complaints(
                &label,
                Measure::of(&frames[body..body + BODY_FRAMES]),
                reference,
                INTRO_LEVEL_FLOOR,
            ));
        }
    }
    assert!(complaints.is_empty(), "{complaints:#?}");
}

// A song that rises out of silence, judged against the engine that has been
// playing the loud song and goes on into the fade.
#[test]
fn ac_29_a_fade_in_does_not_pin_the_swiped_to_songs_bars() {
    let mut complaints = Vec::new();
    for opening in [
        Opening::LinearFade { seconds: 3 },
        Opening::DbFade { seconds: 5 },
    ] {
        let song = loud().opened_by(opening, (boundary_at(0.0) * SAMPLE_RATE_HZ as f32) as usize);
        let reference = settled_on_the_loud_song(&song, boundary_at(0.0), 10.5);
        let reference = Measure::of(&reference[..FADE_FRAMES]);
        let allowed = reference.pinned_frames + PINNED_SLACK;
        for track_change in [false, true] {
            let frames = opened_run(opening, track_change, 10.5);
            let label = format!("a {opening:?}, track change {track_change}");
            let measured = Measure::of(&frames[..FADE_FRAMES]);
            complaints.extend(pinning_complaints(&label, measured, allowed));
            complaints.extend(dimming_complaints(&label, measured, reference, FADE_LEVEL_FLOOR));
        }
    }
    assert!(complaints.is_empty(), "{complaints:#?}");
}
