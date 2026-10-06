//! The in-memory snapshot of a stream that is still being decoded: a left
//! prefix of the final peaks and spectrogram, never a different picture.

use super::RenderDataSession;
use crate::spectrogram::{SPECTROGRAM_FRAME_RATE_HZ, SPECTROGRAM_SAMPLE_RATE_HZ};
use crate::waveform::STORED_PEAK_COUNT;

const SAMPLE_RATE_HZ: u32 = SPECTROGRAM_SAMPLE_RATE_HZ;
const FRAMES_PER_SECOND: usize = SPECTROGRAM_FRAME_RATE_HZ as usize;
const SAMPLES_PER_SECOND: usize = SAMPLE_RATE_HZ as usize;

/// `seconds` of a 440 Hz tone whose level steps up every second, so no two
/// seconds are equally loud and the loudest bucket sits at the end.
fn stepped_tone(seconds: usize) -> Vec<i16> {
    (0..seconds * SAMPLES_PER_SECOND)
        .map(|index| {
            let amplitude = 0.05 + 0.04 * (index / SAMPLES_PER_SECOND) as f64;
            let phase = std::f64::consts::TAU * 440.0 * index as f64 / f64::from(SAMPLE_RATE_HZ);
            (phase.sin() * amplitude * f64::from(i16::MAX)) as i16
        })
        .collect()
}

fn session_with(samples: &[i16]) -> RenderDataSession {
    let mut session = RenderDataSession::new();
    session
        .push_pcm_i16(samples, SAMPLE_RATE_HZ, 1)
        .expect("push must succeed");
    session
}

fn expected_frames(seconds: usize) -> usize {
    seconds * FRAMES_PER_SECOND
}

#[test]
fn partial_fills_buckets_from_the_left() {
    let tone = stepped_tone(10);
    let session = session_with(&tone[..tone.len() / 2]);

    let partial = session
        .partial(expected_frames(10))
        .expect("half a stream has complete buckets");

    assert!(
        (partial.waveform_peaks.len() as i64 - 500).abs() <= 2,
        "bucket count was {}",
        partial.waveform_peaks.len()
    );
    assert!(
        (partial.covered_fraction - 0.5).abs() < 0.005,
        "fraction was {}",
        partial.covered_fraction
    );
    assert_eq!(partial.spectrogram.frame_count(), expected_frames(5));
}

#[test]
fn partial_spectrogram_is_a_prefix_of_the_final() {
    let tone = stepped_tone(10);
    let session = session_with(&tone[..tone.len() * 3 / 5]);
    let partial = session.partial(expected_frames(10)).unwrap();

    let mut session = session;
    session
        .push_pcm_i16(&tone[tone.len() * 3 / 5..], SAMPLE_RATE_HZ, 1)
        .unwrap();
    let finished = session.finish().unwrap();

    assert!(partial.spectrogram.frame_count() > 0);
    let prefix = partial.spectrogram.cells();
    assert_eq!(prefix, &finished.spectrogram.cells()[..prefix.len()]);
}

#[test]
fn partial_is_none_before_one_bucket_is_complete() {
    let empty = RenderDataSession::new();
    assert!(empty.partial(expected_frames(10)).is_none());

    // One frame of a 10 s track (200 frames, 0.2 frame per bucket) covers
    // five buckets, so a stream shorter than one frame covers none.
    let session = session_with(&stepped_tone(1)[..SAMPLES_PER_SECOND / FRAMES_PER_SECOND - 1]);
    assert!(session.partial(expected_frames(10)).is_none());
    assert!(session.partial(0).is_none());
}

#[test]
fn partial_clamps_when_the_stream_outruns_the_expected_length() {
    let session = session_with(&stepped_tone(10));

    let partial = session
        .partial(expected_frames(4))
        .expect("a decoded stream has complete buckets");

    assert_eq!(partial.waveform_peaks.len(), STORED_PEAK_COUNT);
    assert!((partial.covered_fraction - 1.0).abs() < f32::EPSILON);
}

#[test]
fn partial_peaks_use_the_finish_normalisation() {
    let tone = stepped_tone(10);
    let session = session_with(&tone[..tone.len() * 7 / 10]);

    let partial = session.partial(expected_frames(10)).unwrap();

    assert_eq!(partial.waveform_peaks.iter().copied().max(), Some(255));
    assert_eq!(partial.waveform_peaks.last(), Some(&255));
}

#[test]
fn a_complete_partial_equals_the_final_peaks() {
    let tone = stepped_tone(10);
    let session = session_with(&tone);
    let partial = session.partial(expected_frames(10)).unwrap();

    let finished = session.finish().unwrap();

    assert_eq!(partial.waveform_peaks, finished.waveform_peaks);
    assert_eq!(partial.spectrogram, finished.spectrogram);
}

/// `seconds` of a 440 Hz tone that is loudest in its first second and much
/// quieter after, so the loudest bucket is emitted first and never replaced.
fn front_loaded_tone(seconds: usize) -> Vec<i16> {
    (0..seconds * SAMPLES_PER_SECOND)
        .map(|index| {
            let second = index / SAMPLES_PER_SECOND;
            let amplitude = if second == 0 {
                0.6
            } else {
                0.05 + 0.02 * second as f64
            };
            let phase = std::f64::consts::TAU * 440.0 * index as f64 / f64::from(SAMPLE_RATE_HZ);
            (phase.sin() * amplitude * f64::from(i16::MAX)) as i16
        })
        .collect()
}

#[test]
fn an_emitted_bucket_keeps_its_value_while_the_loudest_is_unchanged() {
    let tone = front_loaded_tone(10);
    let early = session_with(&tone[..tone.len() * 3 / 10])
        .partial(expected_frames(10))
        .unwrap();
    let later = session_with(&tone[..tone.len() * 6 / 10])
        .partial(expected_frames(10))
        .unwrap();

    assert!(later.waveform_peaks.len() > early.waveform_peaks.len());
    assert_eq!(
        early.waveform_peaks,
        later.waveform_peaks[..early.waveform_peaks.len()],
        "a bucket moved or changed once emitted"
    );
}

#[test]
fn partial_peaks_equal_a_finished_session_over_the_covered_frames() {
    // Ten buckets over a 10 s track: twenty frames each, so the five buckets
    // of the first half are exactly what a five-bucket session finishes to.
    let tone = stepped_tone(10);
    let half = &tone[..tone.len() / 2];
    let mut growing = RenderDataSession::with_peak_count(10);
    growing.push_pcm_i16(half, SAMPLE_RATE_HZ, 1).unwrap();
    let mut finished = RenderDataSession::with_peak_count(5);
    finished.push_pcm_i16(half, SAMPLE_RATE_HZ, 1).unwrap();

    let partial = growing.partial(expected_frames(10)).unwrap();

    assert_eq!(
        partial.waveform_peaks,
        finished.finish().unwrap().waveform_peaks
    );
}

#[test]
fn partial_spectrogram_covers_the_same_frames_as_its_peaks() {
    // 5.5 s of a 10 s track over ten buckets: five complete buckets cover the
    // first 5 s, and the colour read from the spectrogram must not be spread
    // over the half second the peaks leave out.
    let tone = stepped_tone(10);
    let mut session = RenderDataSession::with_peak_count(10);
    session
        .push_pcm_i16(&tone[..tone.len() * 11 / 20], SAMPLE_RATE_HZ, 1)
        .unwrap();

    let partial = session.partial(expected_frames(10)).unwrap();

    assert_eq!(partial.waveform_peaks.len(), 5);
    assert_eq!(partial.spectrogram.frame_count(), expected_frames(5));
}

#[test]
fn partial_survives_an_absurd_expected_length() {
    let session = session_with(&stepped_tone(10));

    assert!(session.partial(usize::MAX).is_none());
    assert!(session
        .partial(usize::MAX / STORED_PEAK_COUNT + 1)
        .is_none());
}
