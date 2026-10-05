use super::*;

fn inputs(mode: ReplayGainMode) -> GainInputs {
    GainInputs {
        mode,
        tags: ReplayGainTags::default(),
        measured: None,
        album_measured: None,
    }
}

#[test]
fn album_loudness_is_a_duration_weighted_energy_mean() {
    let result = album_loudness(&[(-10.0, 1_000), (-20.0, 3_000)]).unwrap();
    assert!((result - -14.881_166).abs() < 0.000_001);
    assert_eq!(album_loudness(&[]), None);
}

#[test]
fn play_18_gain_resolution_covers_every_source_and_fallback() {
    let measured = MeasuredLoudness {
        integrated_lufs: -20.0,
        true_peak: 0.5,
    };
    let cases = [
        (inputs(ReplayGainMode::Off), 0.0, GainSource::None),
        (
            GainInputs {
                mode: ReplayGainMode::Track,
                tags: ReplayGainTags {
                    track_gain_db: Some(2.5),
                    track_peak: Some(0.5),
                    ..ReplayGainTags::default()
                },
                measured: Some(measured),
                album_measured: None,
            },
            2.5,
            GainSource::Tag,
        ),
        (
            GainInputs {
                measured: Some(measured),
                ..inputs(ReplayGainMode::Track)
            },
            2.0,
            GainSource::Measured,
        ),
        (
            GainInputs {
                mode: ReplayGainMode::Album,
                tags: ReplayGainTags {
                    album_gain_db: Some(-1.5),
                    album_peak: Some(0.8),
                    ..ReplayGainTags::default()
                },
                measured: Some(measured),
                album_measured: Some((-21.0, 0.4)),
            },
            -1.5,
            GainSource::Tag,
        ),
        (
            GainInputs {
                measured: Some(measured),
                album_measured: Some((-21.0, 0.4)),
                ..inputs(ReplayGainMode::Album)
            },
            3.0,
            GainSource::Measured,
        ),
        (
            GainInputs {
                measured: Some(measured),
                ..inputs(ReplayGainMode::Album)
            },
            2.0,
            GainSource::Measured,
        ),
        (
            GainInputs {
                mode: ReplayGainMode::Album,
                tags: ReplayGainTags {
                    track_gain_db: Some(-3.0),
                    ..ReplayGainTags::default()
                },
                measured: Some(measured),
                album_measured: None,
            },
            -3.0,
            GainSource::Tag,
        ),
        (inputs(ReplayGainMode::Track), 0.0, GainSource::None),
    ];

    for (case, expected_gain, expected_source) in cases {
        let result = resolve_gain(case);
        assert!((result.gain_db - expected_gain).abs() < f64::EPSILON);
        assert_eq!(result.source, expected_source);
    }
}

#[test]
fn play_18_peak_caps_positive_gain_for_the_chosen_source() {
    let result = resolve_gain(GainInputs {
        mode: ReplayGainMode::Track,
        tags: ReplayGainTags {
            track_gain_db: Some(8.0),
            track_peak: Some(0.8),
            ..ReplayGainTags::default()
        },
        measured: None,
        album_measured: None,
    });

    assert!((result.gain_db - 1.938_200_260_161_128).abs() < 0.000_000_001);
    assert_eq!(result.source, GainSource::Tag);

    let measured = resolve_gain(GainInputs {
        measured: Some(MeasuredLoudness {
            integrated_lufs: -30.0,
            true_peak: 0.5,
        }),
        ..inputs(ReplayGainMode::Track)
    });
    assert!((measured.gain_db - 6.020_599_913_279_624).abs() < 0.000_000_001);
    assert_eq!(measured.source, GainSource::Measured);
}

#[test]
fn play_18_silence_has_no_measured_gain() {
    let result = resolve_gain(GainInputs {
        measured: Some(MeasuredLoudness {
            integrated_lufs: f64::NEG_INFINITY,
            true_peak: 0.0,
        }),
        ..inputs(ReplayGainMode::Track)
    });

    assert_eq!(
        result,
        ResolvedGain {
            gain_db: 0.0,
            source: GainSource::None
        }
    );
}

fn tagged_track(gain_db: f64) -> GainInputs {
    GainInputs {
        tags: ReplayGainTags {
            track_gain_db: Some(gain_db),
            ..ReplayGainTags::default()
        },
        ..inputs(ReplayGainMode::Track)
    }
}

#[test]
fn play_18_gain_stays_inside_the_audible_safe_range() {
    assert_eq!(resolve_gain(tagged_track(40.0)).gain_db, MAX_GAIN_DB);
    assert_eq!(resolve_gain(tagged_track(-60.0)).gain_db, MIN_GAIN_DB);
    assert_eq!(resolve_gain(tagged_track(-3.5)).gain_db, -3.5);

    let very_quiet = resolve_gain(GainInputs {
        measured: Some(MeasuredLoudness {
            integrated_lufs: -80.0,
            true_peak: 0.0,
        }),
        ..inputs(ReplayGainMode::Track)
    });
    assert_eq!(very_quiet.gain_db, MAX_GAIN_DB);
}

#[test]
fn play_18_a_non_finite_gain_is_unity() {
    for hostile in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let result = resolve_gain(tagged_track(hostile));
        assert_eq!(result.gain_db, 0.0, "{hostile} must not reach the player");
    }
}
