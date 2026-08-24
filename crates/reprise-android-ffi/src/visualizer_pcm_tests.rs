use super::LiveAudioState;

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
