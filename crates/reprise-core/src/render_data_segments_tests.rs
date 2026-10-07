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
            last_in_file: false,
        },
        SegmentBounds {
            start_ms: 2_000,
            end_ms: 4_000,
            last_in_file: false,
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
        session.push_pcm_i16(chunk, RATE, 2, None).unwrap();
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
        last_in_file: false,
    };
    let mut session = SegmentedRenderDataSession::new(&[bounds[0], bounds[1], late], 100);

    session.push_pcm_i16(&stream, RATE, 2, None).unwrap();
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
    session
        .push_pcm_i16(&stream[..4_410], RATE, 2, None)
        .unwrap();

    assert_eq!(
        session.push_pcm_i16(&stream[..4_410], 48_000, 2, None),
        Err(RenderDataSessionError::RateOrChannelChanged)
    );
    assert_eq!(
        session.push_pcm_i16(&stream[..4_410], RATE, 1, None),
        Err(RenderDataSessionError::RateOrChannelChanged)
    );
    assert_eq!(
        session.push_pcm_i16(&stream[..4_410], 0, 2, None),
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

    session.push_pcm_f32(&floats, RATE, 2, None).unwrap();
    let data: Vec<_> = session.finish().into_iter().map(Result::unwrap).collect();

    assert_eq!(peak_band(&data[0]), expected_band(440.0));
    assert_eq!(peak_band(&data[1]), expected_band(1_000.0));
}

/// The timestamp of a chunk starting `frame` frames into the stream, truncated
/// to whole microseconds the way a decoder that counts in microseconds does.
fn truncated_us(frame: usize) -> i64 {
    i64::try_from(frame as u64 * 1_000_000 / u64::from(RATE)).unwrap()
}

/// Feeds `stream` in chunks of `chunk_frames`, each stamped with its start
/// time, skipping the chunks `drop` names (by index).
fn analyse_timed(
    stream: &[i16],
    bounds: &[SegmentBounds],
    chunk_frames: usize,
    drop: &[usize],
) -> Vec<Result<TrackRenderData, RenderDataSessionError>> {
    let mut session = SegmentedRenderDataSession::new(bounds, 100);
    for (index, chunk) in stream.chunks(chunk_frames * 2).enumerate() {
        if drop.contains(&index) {
            continue;
        }
        session
            .push_pcm_i16(chunk, RATE, 2, Some(truncated_us(index * chunk_frames)))
            .unwrap();
    }
    session.finish()
}

fn assert_same(left: &TrackRenderData, right: &TrackRenderData) {
    assert_eq!(left.waveform_peaks, right.waveform_peaks);
    assert_eq!(left.spectrogram.cells(), right.spectrogram.cells());
    assert_eq!(left.loudness, right.loudness);
}

#[test]
fn timestamps_a_decoder_rounds_to_microseconds_place_chunks_exactly() {
    let (stream, bounds) = two_track_stream();

    let counted = analyse(&stream, &bounds, 997);
    let timed = analyse_timed(&stream, &bounds, 997, &[]);

    for (left, right) in counted.iter().zip(&timed) {
        assert_same(left, right.as_ref().unwrap());
    }
}

#[test]
fn a_dropped_chunk_does_not_shift_the_later_tracks() {
    let (stream, bounds) = two_track_stream();
    let whole = analyse_timed(&stream, &bounds, 4_410, &[]);

    // Chunk 5 is half a second into the first track.
    let dropped = analyse_timed(&stream, &bounds, 4_410, &[5]);

    assert_same(whole[1].as_ref().unwrap(), dropped[1].as_ref().unwrap());
    assert_ne!(
        whole[0].as_ref().unwrap().spectrogram.frame_count(),
        dropped[0].as_ref().unwrap().spectrogram.frame_count(),
        "the first track is short by the dropped chunk, not padded with silence"
    );
}

#[test]
fn a_chunk_without_a_timestamp_continues_from_the_running_frame_count() {
    let (stream, bounds) = two_track_stream();
    let counted = analyse(&stream, &bounds, 4_410);
    let mut session = SegmentedRenderDataSession::new(&bounds, 100);

    for (index, chunk) in stream.chunks(4_410 * 2).enumerate() {
        let stamp = (index % 2 == 0).then(|| truncated_us(index * 4_410));
        session.push_pcm_i16(chunk, RATE, 2, stamp).unwrap();
    }

    for (left, right) in counted.iter().zip(session.finish()) {
        assert_same(left, &right.unwrap());
    }
}

#[test]
fn audio_that_arrives_again_is_dropped_and_the_first_copy_wins() {
    let (stream, bounds) = two_track_stream();
    let counted = analyse(&stream, &bounds, 4_410);
    let mut session = SegmentedRenderDataSession::new(&bounds, 100);
    let chunks: Vec<&[i16]> = stream.chunks(4_410 * 2).collect();

    for (index, chunk) in chunks.iter().enumerate() {
        session
            .push_pcm_i16(chunk, RATE, 2, Some(truncated_us(index * 4_410)))
            .unwrap();
        if index == 3 {
            // The same chunk again, then a louder copy of it: both are late.
            session
                .push_pcm_i16(chunk, RATE, 2, Some(truncated_us(index * 4_410)))
                .unwrap();
            let louder: Vec<i16> = chunk
                .iter()
                .map(|sample| sample.saturating_mul(4))
                .collect();
            session
                .push_pcm_i16(&louder, RATE, 2, Some(truncated_us(index * 4_410)))
                .unwrap();
        }
    }

    for (left, right) in counted.iter().zip(session.finish()) {
        assert_same(left, &right.unwrap());
    }
}

#[test]
fn audio_stamped_before_the_start_of_the_stream_is_dropped() {
    let (stream, bounds) = two_track_stream();
    let counted = analyse(&stream, &bounds, 4_410);
    let mut session = SegmentedRenderDataSession::new(&bounds, 100);
    let priming = vec![i16::MAX; 441 * 2];

    session
        .push_pcm_i16(&priming, RATE, 2, Some(-10_000))
        .unwrap();
    for (index, chunk) in stream.chunks(4_410 * 2).enumerate() {
        session
            .push_pcm_i16(chunk, RATE, 2, Some(truncated_us(index * 4_410)))
            .unwrap();
    }

    for (left, right) in counted.iter().zip(session.finish()) {
        assert_same(left, &right.unwrap());
    }
}

/// A quiet 440 Hz track of two seconds, then a loud 1000 Hz one of four, the
/// last of the file; `claimed_end_ms` is where the file's metadata ends it.
fn album_with_a_last_track_ending_at(claimed_end_ms: i64) -> (Vec<i16>, [SegmentBounds; 2]) {
    let mut stream = tone(440.0, 2, 0.1);
    stream.extend(tone(1_000.0, 4, 0.5));
    let bounds = [
        SegmentBounds {
            start_ms: 0,
            end_ms: 2_000,
            last_in_file: false,
        },
        SegmentBounds {
            start_ms: 2_000,
            end_ms: claimed_end_ms,
            last_in_file: true,
        },
    ];
    (stream, bounds)
}

#[test]
fn the_last_track_runs_to_the_decoded_end_whatever_the_metadata_claims() {
    for claimed_end_ms in [4_000, 6_000, 8_000] {
        let (stream, bounds) = album_with_a_last_track_ending_at(claimed_end_ms);
        let split = RATE as usize * 2 * 2;

        let data = analyse(&stream, &bounds, 997);

        for (index, part) in [&stream[..split], &stream[split..]].into_iter().enumerate() {
            let mut alone = RenderDataSession::with_peak_count(100);
            alone.push_pcm_i16(part, RATE, 2).unwrap();
            assert_same(&data[index], &alone.finish().unwrap());
        }
        assert_eq!(
            data[1].spectrogram.frame_count(),
            80,
            "claimed end {claimed_end_ms}"
        );
    }
}

#[test]
fn only_the_cut_decides_whether_two_bounds_are_the_same_cut() {
    let cut = SegmentBounds {
        start_ms: 2_000,
        end_ms: 6_000,
        last_in_file: true,
    };

    assert!(cut.same_cut(&SegmentBounds {
        last_in_file: false,
        ..cut
    }));
    assert!(!cut.same_cut(&SegmentBounds {
        end_ms: 6_001,
        ..cut
    }));
}

#[test]
fn a_tracks_partial_picture_covers_only_its_own_stretch() {
    let (stream, bounds) = two_track_stream();
    let mut session = SegmentedRenderDataSession::new(&bounds, 100);
    let three_seconds = RATE as usize * 3 * 2;

    session
        .push_pcm_i16(&stream[..three_seconds], RATE, 2, None)
        .unwrap();

    let first = session.partial_source(0).unwrap().render(40).unwrap();
    let second = session.partial_source(1).unwrap().render(40).unwrap();
    assert_eq!(first.covered_fraction, 1.0);
    assert!(
        (0.45..=0.55).contains(&second.covered_fraction),
        "{}",
        second.covered_fraction
    );
    assert!(session.partial_source(2).is_none());
}
