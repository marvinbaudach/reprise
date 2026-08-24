use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{
    live_processor_for_stream, AndroidVisualEngine, LiveAudioState, MonotonicClock,
    TARGET_PCM_BUFFER_DURATION,
};

#[test]
fn pcm_ring_overflow_discards_the_oldest_samples() {
    let mut live_audio = LiveAudioState::new(0, 8_000).expect("supported sample rate");
    let capacity = 16_000;
    let samples = (0..capacity + 2).map(|sample| sample as f32).collect::<Vec<_>>();

    live_audio.pcm_buffer.append(&samples);

    assert_eq!(live_audio.pcm_buffer.samples.len(), capacity);
    assert_eq!(live_audio.pcm_buffer.samples.front(), Some(&2.0));
    assert_eq!(
        live_audio.pcm_buffer.samples.back(),
        Some(&((capacity + 1) as f32))
    );
}

#[test]
fn live_audio_reset_empties_the_pcm_ring() {
    let mut live_audio = LiveAudioState::new(0, 48_000).expect("supported sample rate");
    live_audio.pcm_buffer.append(&[0.25, -0.5, 0.75]);

    live_audio.reset();

    assert!(live_audio.pcm_buffer.samples.is_empty());
}

#[test]
fn pcm_ring_capacity_is_two_seconds_at_the_stream_sample_rate() {
    let forty_eight_k = LiveAudioState::new(0, 48_000).expect("supported sample rate");
    let forty_four_one_k = LiveAudioState::new(0, 44_100).expect("supported sample rate");

    assert_eq!(forty_eight_k.pcm_buffer.capacity, 96_000);
    assert_eq!(forty_four_one_k.pcm_buffer.capacity, 88_200);
}

#[test]
fn pcm_ingest_buffers_downmixed_samples_without_publishing_bands() {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);
    let pcm = stereo_pcm(&[(16_384, 0), (-16_384, 16_384)]);

    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));

    {
        let live_audio = engine.live_audio.lock().expect("live audio lock");
        let live_audio = live_audio.as_ref().expect("PCM initialized the stream");
        assert_eq!(
            live_audio.pcm_buffer.samples.iter().copied().collect::<Vec<_>>(),
            vec![0.25, 0.0]
        );
        assert!(live_audio.bands.iter().all(|band| *band == 0.0));
    }
    assert!(!engine.has_live_audio());
    assert!(engine.scene(272.0, 272.0).is_empty());
}

#[test]
fn pcm_ingest_rejects_samples_from_a_replaced_stream_generation() {
    let mut live_audio = Some(LiveAudioState::new(3, 48_000).expect("supported sample rate"));
    live_audio
        .as_mut()
        .expect("live audio state")
        .pcm_buffer
        .append(&[0.25, 0.5]);

    let current = live_processor_for_stream(&mut live_audio, 4, 48_000)
        .expect("same sample rate keeps a processor");

    assert_eq!(current.stream_generation, 4);
    assert!(current.pcm_buffer.samples.is_empty());
}

#[test]
fn pcm_ingest_keeps_its_frame_validation_contract() {
    let engine = AndroidVisualEngine::new();
    let one_stereo_frame = stereo_pcm(&[(1, -1)]);

    assert!(!engine.ingest_pcm_i16(Vec::new(), 0, 48_000, 2));
    assert!(!engine.ingest_pcm_i16(one_stereo_frame.clone(), 5, 48_000, 2));
    assert!(!engine.ingest_pcm_i16(one_stereo_frame.clone(), 3, 48_000, 2));
    assert!(!engine.ingest_pcm_i16(one_stereo_frame.clone(), 4, 48_000, 0));
    assert!(!engine.ingest_pcm_i16(one_stereo_frame, 4, 48_000, 33));
}

#[test]
fn tick_consumes_pcm_in_proportion_to_monotonic_elapsed_time() {
    let clock = Arc::new(PcmTestClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playing(true);
    let pcm = stereo_pcm(&vec![(8_192, -4_096); 12_000]);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));

    clock.advance(Duration::from_millis(10));
    engine.tick();
    assert_eq!(buffered_sample_count(&engine), 11_520);

    let refill = stereo_pcm(&vec![(8_192, -4_096); 480]);
    assert!(engine.ingest_pcm_i16(refill.clone(), refill.len() as u32, 48_000, 2));

    clock.advance(Duration::from_millis(20));
    engine.tick();
    assert_eq!(buffered_sample_count(&engine), 11_040);
}

#[test]
fn every_tick_with_buffered_pcm_publishes_a_band_set() {
    let clock = Arc::new(PcmTestClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playing(true);
    let pcm = stereo_sine_pcm(2_000.0, 48_000, 12_000);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));

    let mut previous_bands = engine.live_bands_for_testing();
    for _ in 0..3 {
        clock.advance(Duration::from_millis(10));
        assert!(engine.tick(), "a newly published band set is a change");
        let bands = engine.live_bands_for_testing();
        assert_ne!(bands, previous_bands, "each PCM tick publishes fresh bars");
        previous_bands = bands;
    }

    assert!(engine.has_live_audio());
}

#[test]
fn tick_without_buffered_pcm_does_not_change_live_bands() {
    let clock = Arc::new(PcmTestClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    let before = engine.live_bands_for_testing();

    clock.advance(Duration::from_millis(10));
    engine.tick();

    assert_eq!(engine.live_bands_for_testing(), before);
    assert!(!engine.has_live_audio());
}

#[test]
fn pcm_controller_reduces_sustained_overfill_within_the_ten_percent_cap() {
    let mut live_audio = LiveAudioState::new(0, 48_000).expect("supported sample rate");
    let nominal = 480;
    live_audio.pcm_buffer.append(&vec![0.25; 16_800]);
    let initial_fill = live_audio.pcm_buffer.samples.len();

    for _ in 0..60 {
        live_audio.pcm_buffer.append(&vec![0.25; nominal]);
        let before = live_audio.pcm_buffer.samples.len();
        assert!(live_audio.analyze_elapsed(Duration::from_millis(10)).is_some());
        let consumed = before - live_audio.pcm_buffer.samples.len();
        assert!((432..=528).contains(&consumed));
    }

    assert!(live_audio.pcm_buffer.samples.len() < initial_fill);
}

#[test]
fn pcm_controller_hard_trims_above_twice_the_target() {
    let mut live_audio = LiveAudioState::new(0, 48_000).expect("supported sample rate");
    let target = samples_for_test_duration(TARGET_PCM_BUFFER_DURATION, 48_000);
    live_audio.pcm_buffer.append(&vec![0.25; target * 2 + 1]);

    assert!(live_audio.analyze_elapsed(Duration::from_millis(10)).is_none());

    assert_eq!(live_audio.pcm_buffer.samples.len(), target);
}

#[test]
fn pcm_controller_does_not_catch_up_through_underflow() {
    let mut live_audio = LiveAudioState::new(0, 48_000).expect("supported sample rate");
    live_audio.pcm_buffer.append(&vec![0.25; 100]);

    assert!(live_audio.analyze_elapsed(Duration::from_millis(10)).is_some());
    assert!(live_audio.pcm_buffer.samples.is_empty());
    for _ in 0..10 {
        assert!(live_audio.analyze_elapsed(Duration::from_millis(10)).is_none());
    }

    live_audio.pcm_buffer.append(&vec![0.25; 480]);
    assert!(live_audio.analyze_elapsed(Duration::from_millis(10)).is_some());
    assert_eq!(live_audio.pcm_buffer.samples.len(), 48);
}

#[test]
fn pcm_controller_converges_to_target_under_constant_supply() {
    let mut live_audio = LiveAudioState::new(0, 48_000).expect("supported sample rate");
    let target = samples_for_test_duration(TARGET_PCM_BUFFER_DURATION, 48_000);
    live_audio.pcm_buffer.append(&vec![0.25; target + 4_800]);

    for _ in 0..300 {
        live_audio.pcm_buffer.append(&vec![0.25; 480]);
        assert!(live_audio.analyze_elapsed(Duration::from_millis(10)).is_some());
    }
    live_audio.pcm_buffer.append(&vec![0.25; 480]);

    assert!(live_audio.pcm_buffer.samples.len().abs_diff(target) <= 16);
}

fn buffered_sample_count(engine: &AndroidVisualEngine) -> usize {
    engine
        .live_audio
        .lock()
        .expect("live audio lock")
        .as_ref()
        .map_or(0, |live_audio| live_audio.pcm_buffer.samples.len())
}

fn samples_for_test_duration(duration: Duration, sample_rate_hz: u32) -> usize {
    (duration.as_secs_f64() * f64::from(sample_rate_hz)).round() as usize
}

#[derive(Default)]
struct PcmTestClock {
    now_nanos: AtomicU64,
}

impl PcmTestClock {
    fn advance(&self, duration: Duration) {
        self.now_nanos.fetch_add(
            duration
                .as_nanos()
                .try_into()
                .expect("test duration fits u64"),
            Ordering::Relaxed,
        );
    }
}

impl MonotonicClock for PcmTestClock {
    fn now(&self) -> Duration {
        Duration::from_nanos(self.now_nanos.load(Ordering::Relaxed))
    }
}

fn stereo_pcm(frames: &[(i16, i16)]) -> Vec<u8> {
    frames
        .iter()
        .flat_map(|(left, right)| left.to_le_bytes().into_iter().chain(right.to_le_bytes()))
        .collect()
}

fn stereo_sine_pcm(frequency_hz: f32, sample_rate_hz: u32, frame_count: usize) -> Vec<u8> {
    let frames = (0..frame_count)
        .map(|frame| {
            let sample = (std::f32::consts::TAU * frequency_hz * frame as f32
                / sample_rate_hz as f32)
                .sin();
            let sample = (sample * 20_000.0).round() as i16;
            (sample, sample)
        })
        .collect::<Vec<_>>();
    stereo_pcm(&frames)
}
