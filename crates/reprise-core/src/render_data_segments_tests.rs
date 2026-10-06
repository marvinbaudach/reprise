use super::*;
use crate::spectrogram::{SPECTROGRAM_BAND_COUNT, SPECTROGRAM_HIGH_HZ, SPECTROGRAM_LOW_HZ};

const RATE: u32 = 44_100;

fn tone(frequency_hz: f64, seconds: u32, amplitude: f64) -> Vec<i16> {
    (0..RATE * seconds)
        .flat_map(|index| {
            let phase = std::f64::consts::TAU * frequency_hz * f64::from(index) / f64::from(RATE);
            let sample = (phase.sin() * amplitude * f64::from(i16::MAX)) as i16;
            [sample, sample]
        })
        .collect()
}

/// A quiet 440 Hz second, then a loud 1000 Hz second and a half.
fn two_track_stream() -> (Vec<i16>, [SegmentBounds; 2]) {
    let mut stream = tone(440.0, 2, 0.1);
    stream.extend(tone(1_000.0, 2, 0.5));
    let bounds = [
        SegmentBounds {
            start_ms: 0,
            end_ms: 2_000,
        },
        SegmentBounds {
            start_ms: 2_000,
            end_ms: 4_000,
        },
    ];
    (stream, bounds)
}

fn peak_band(data: &TrackRenderData) -> usize {
    let frame = data
        .spectrogram
        .frame(data.spectrogram.frame_count() - 1)
        .expect("a finished stream has a frame");
    frame
        .iter()
        .enumerate()
        .max_by_key(|(_, level)| **level)
        .map(|(index, _)| index)
        .expect("a frame has bands")
}

fn expected_band(frequency_hz: f64) -> usize {
    let low = f64::from(SPECTROGRAM_LOW_HZ).log2();
    let high = f64::from(SPECTROGRAM_HIGH_HZ).log2();
    let step = (high - low) / SPECTROGRAM_BAND_COUNT as f64;
    (((frequency_hz.log2() - low) / step).floor() as usize).min(SPECTROGRAM_BAND_COUNT - 1)
}

fn analyse(stream: &[i16], bounds: &[SegmentBounds], chunk_frames: usize) -> Vec<TrackRenderData> {
    let mut session = SegmentedRenderDataSession::new(bounds, 100);
    for chunk in stream.chunks(chunk_frames * 2) {
        session.push_pcm_i16(chunk, RATE, 2).unwrap();
    }
    session
        .finish()
        .into_iter()
        .map(|data| data.expect("both tracks have audio"))
        .collect()
}

#[test]
fn each_track_is_analysed_from_its_own_stretch_of_the_one_stream() {
    let (stream, bounds) = two_track_stream();

    let data = analyse(&stream, &bounds, 4_410);

    assert_eq!(peak_band(&data[0]), expected_band(440.0));
    assert_eq!(peak_band(&data[1]), expected_band(1_000.0));
    assert_eq!(data[0].spectrogram.frame_count(), 40);
    assert_eq!(data[1].spectrogram.frame_count(), 40);
}

#[test]
fn a_track_gets_what_analysing_its_stretch_alone_would_give() {
    let (stream, bounds) = two_track_stream();
    let split = RATE as usize * 2 * 2;

    let together = analyse(&stream, &bounds, 4_410);

    for (index, part) in [&stream[..split], &stream[split..]].into_iter().enumerate() {
        let mut alone = RenderDataSession::with_peak_count(100);
        alone.push_pcm_i16(part, RATE, 2).unwrap();
        let alone = alone.finish().unwrap();
        assert_eq!(
            together[index].waveform_peaks, alone.waveform_peaks,
            "track {index}"
        );
        assert_eq!(
            together[index].spectrogram.cells(),
            alone.spectrogram.cells(),
            "track {index}"
        );
        assert_eq!(together[index].loudness, alone.loudness, "track {index}");
    }
}

#[test]
fn the_result_does_not_depend_on_how_the_stream_was_chunked() {
    let (stream, bounds) = two_track_stream();

    let coarse = analyse(&stream, &bounds, 1_000_000);
    let awkward = analyse(&stream, &bounds, 997);

    for (left, right) in coarse.iter().zip(&awkward) {
        assert_eq!(left.waveform_peaks, right.waveform_peaks);
        assert_eq!(left.spectrogram.cells(), right.spectrogram.cells());
        assert_eq!(left.loudness, right.loudness);
    }
}

#[test]
fn loudness_is_measured_per_track() {
    let (stream, bounds) = two_track_stream();

    let data = analyse(&stream, &bounds, 4_410);
    let quiet = data[0].loudness.expect("measured").integrated_lufs;
    let loud = data[1].loudness.expect("measured").integrated_lufs;

    // Five times the amplitude is 14 dB, and K-weighting favours 1 kHz a little.
    assert!((13.0..16.0).contains(&(loud - quiet)), "{quiet} vs {loud}");
}

#[test]
fn a_track_the_stream_never_reaches_is_empty_and_the_others_are_not() {
    let (stream, bounds) = two_track_stream();
    let late = SegmentBounds {
        start_ms: 9_000,
        end_ms: 10_000,
    };
    let mut session = SegmentedRenderDataSession::new(&[bounds[0], bounds[1], late], 100);

    session.push_pcm_i16(&stream, RATE, 2).unwrap();
    let results = session.finish();

    assert!(results[0].is_ok() && results[1].is_ok());
    assert_eq!(
        results[2].as_ref().err(),
        Some(&RenderDataSessionError::EmptyStream)
    );
}

#[test]
fn a_change_of_rate_or_channels_is_refused() {
    let (stream, bounds) = two_track_stream();
    let mut session = SegmentedRenderDataSession::new(&bounds, 100);
    session.push_pcm_i16(&stream[..4_410], RATE, 2).unwrap();

    assert_eq!(
        session.push_pcm_i16(&stream[..4_410], 48_000, 2),
        Err(RenderDataSessionError::RateOrChannelChanged)
    );
    assert_eq!(
        session.push_pcm_i16(&stream[..4_410], RATE, 1),
        Err(RenderDataSessionError::RateOrChannelChanged)
    );
    assert_eq!(
        session.push_pcm_i16(&stream[..4_410], 0, 2),
        Err(RenderDataSessionError::InvalidStreamConfig)
    );
}

#[test]
fn float_pcm_is_routed_the_same_way() {
    let (stream, bounds) = two_track_stream();
    let floats: Vec<f32> = stream
        .iter()
        .map(|sample| f32::from(*sample) / 32_768.0)
        .collect();
    let mut session = SegmentedRenderDataSession::new(&bounds, 100);

    session.push_pcm_f32(&floats, RATE, 2).unwrap();
    let data: Vec<_> = session.finish().into_iter().map(Result::unwrap).collect();

    assert_eq!(peak_band(&data[0]), expected_band(440.0));
    assert_eq!(peak_band(&data[1]), expected_band(1_000.0));
}
