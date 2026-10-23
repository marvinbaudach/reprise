use super::*;

fn inputs(mode: ReplayGainMode) -> GainInputs {
    GainInputs {
        mode,
        tags: ReplayGainTags::default(),
        measured: None,
        album_measured: None,
        target: LoudnessTarget::REPLAYGAIN,
    }
}

#[test]
fn album_loudness_is_a_duration_weighted_energy_mean() {
    let result = album_loudness(&[(-10.0, 1_000), (-20.0, 3_000)]).unwrap();
    assert!((result - -14.881_166).abs() < 0.000_001);
    assert_eq!(album_loudness(&[]), None);
}

#[test]
fn play_19_gain_resolution_covers_every_source_and_fallback() {
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
                target: LoudnessTarget::REPLAYGAIN,
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
                target: LoudnessTarget::REPLAYGAIN,
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
                target: LoudnessTarget::REPLAYGAIN,
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
fn play_19_peak_caps_positive_gain_for_the_chosen_source() {
    let result = resolve_gain(GainInputs {
        mode: ReplayGainMode::Track,
        tags: ReplayGainTags {
            track_gain_db: Some(8.0),
            track_peak: Some(0.8),
            ..ReplayGainTags::default()
        },
        measured: None,
        album_measured: None,
        target: LoudnessTarget::REPLAYGAIN,
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
fn play_19_silence_has_no_measured_gain() {
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
fn play_19_gain_stays_inside_the_audible_safe_range() {
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
fn play_19_a_non_finite_gain_is_unity() {
    for hostile in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let result = resolve_gain(tagged_track(hostile));
        assert_eq!(result.gain_db, 0.0, "{hostile} must not reach the player");
    }
}

fn assert_gain(result: ResolvedGain, expected_db: f64) {
    assert!(
        (result.gain_db - expected_db).abs() < 0.000_000_001,
        "expected {expected_db} dB, got {}",
        result.gain_db
    );
}

#[test]
fn play_25_a_measured_track_aims_at_the_streaming_target() {
    let measured = Some(MeasuredLoudness {
        integrated_lufs: -20.0,
        true_peak: 0.5,
    });
    let at = |target| {
        resolve_gain(GainInputs {
            measured,
            target,
            ..inputs(ReplayGainMode::Track)
        })
    };

    assert_gain(at(LoudnessTarget::STREAMING), 6.0);
    assert_gain(at(LoudnessTarget::REPLAYGAIN), 2.0);
    assert_eq!(at(LoudnessTarget::STREAMING).source, GainSource::Measured);
}

#[test]
fn play_25_a_tag_gain_is_raised_by_the_target_offset() {
    let tagged = |target| {
        resolve_gain(GainInputs {
            tags: ReplayGainTags {
                track_gain_db: Some(-3.5),
                track_peak: Some(0.5),
                ..ReplayGainTags::default()
            },
            target,
            ..inputs(ReplayGainMode::Track)
        })
    };

    assert_gain(tagged(LoudnessTarget::STREAMING), 0.5);
    assert_gain(tagged(LoudnessTarget::REPLAYGAIN), -3.5);
    assert_eq!(tagged(LoudnessTarget::STREAMING).source, GainSource::Tag);
}

#[test]
fn play_25_the_peak_cap_applies_after_the_raise() {
    let result = resolve_gain(GainInputs {
        tags: ReplayGainTags {
            track_gain_db: Some(1.0),
            track_peak: Some(0.9),
            ..ReplayGainTags::default()
        },
        target: LoudnessTarget::STREAMING,
        ..inputs(ReplayGainMode::Track)
    });

    // 1.0 + 4.0 = 5.0 dB would clip a 0.9 peak; the cap is -20 * log10(0.9).
    assert_gain(result, -20.0 * 0.9_f64.log10());
    assert!(result.gain_db < 1.0);
}

#[test]
fn play_25_album_mode_raises_album_tags_and_measurements() {
    let album_tagged = resolve_gain(GainInputs {
        tags: ReplayGainTags {
            album_gain_db: Some(-6.0),
            album_peak: Some(0.5),
            ..ReplayGainTags::default()
        },
        target: LoudnessTarget::STREAMING,
        ..inputs(ReplayGainMode::Album)
    });
    assert_gain(album_tagged, -2.0);
    assert_eq!(album_tagged.source, GainSource::Tag);

    let album_measured = resolve_gain(GainInputs {
        album_measured: Some((-20.0, 0.5)),
        target: LoudnessTarget::STREAMING,
        ..inputs(ReplayGainMode::Album)
    });
    assert_gain(album_measured, 6.0);
    assert_eq!(album_measured.source, GainSource::Measured);

    let off = resolve_gain(GainInputs {
        album_measured: Some((-20.0, 0.5)),
        target: LoudnessTarget::STREAMING,
        ..inputs(ReplayGainMode::Off)
    });
    assert_eq!(off.gain_db, 0.0);
}

fn tag_only(mode: ReplayGainMode, tags: ReplayGainTags, target: LoudnessTarget) -> GainInputs {
    GainInputs {
        tags,
        target,
        ..inputs(mode)
    }
}

fn track_tag(gain_db: f64) -> ReplayGainTags {
    ReplayGainTags {
        track_gain_db: Some(gain_db),
        ..ReplayGainTags::default()
    }
}

#[test]
fn play_25_a_tag_without_a_peak_is_capped_by_the_measured_peak() {
    let result = resolve_gain(GainInputs {
        measured: Some(MeasuredLoudness {
            integrated_lufs: -20.0,
            true_peak: 0.9,
        }),
        ..tag_only(
            ReplayGainMode::Track,
            track_tag(-3.0),
            LoudnessTarget::STREAMING,
        )
    });

    // -3.0 + 4.0 = +1.0 dB would clip the measured 0.9 peak.
    assert_gain(result, -20.0 * 0.9_f64.log10());
    assert_eq!(result.source, GainSource::Tag);
}

#[test]
fn play_25_a_tag_without_any_known_peak_is_raised_at_most_to_unity() {
    let at = |gain_db| {
        resolve_gain(tag_only(
            ReplayGainMode::Track,
            track_tag(gain_db),
            LoudnessTarget::STREAMING,
        ))
    };

    assert_gain(at(-3.0), 0.0);
    assert_gain(at(-9.0), -5.0);
    assert_gain(at(2.0), 2.0);
    assert_eq!(at(-3.0).source, GainSource::Tag);
}

#[test]
fn play_25_the_desktop_target_leaves_a_peakless_tag_exactly_as_tagged() {
    for gain_db in [-3.0, -9.0, 2.0] {
        let result = resolve_gain(tag_only(
            ReplayGainMode::Track,
            track_tag(gain_db),
            LoudnessTarget::REPLAYGAIN,
        ));
        assert_eq!(result.gain_db, gain_db);
    }
}

#[test]
fn play_25_an_album_tag_without_a_peak_falls_back_to_the_track_peak() {
    let album_tag = |track_peak, measured| {
        resolve_gain(GainInputs {
            measured,
            ..tag_only(
                ReplayGainMode::Album,
                ReplayGainTags {
                    album_gain_db: Some(-3.0),
                    track_peak,
                    ..ReplayGainTags::default()
                },
                LoudnessTarget::STREAMING,
            )
        })
    };
    let measured = Some(MeasuredLoudness {
        integrated_lufs: -20.0,
        true_peak: 0.95,
    });

    // The track tag's peak wins over the measured one, and both bound the raise.
    assert_gain(album_tag(Some(0.9), measured), -20.0 * 0.9_f64.log10());
    assert_gain(album_tag(None, measured), -20.0 * 0.95_f64.log10());
    // With no peak anywhere the raise stops at unity.
    assert_gain(album_tag(None, None), 0.0);
}

#[test]
fn play_25_the_album_peak_wins_over_the_track_peak_for_an_album_tag() {
    let result = resolve_gain(tag_only(
        ReplayGainMode::Album,
        ReplayGainTags {
            album_gain_db: Some(-3.0),
            album_peak: Some(0.9),
            track_peak: Some(0.5),
            ..ReplayGainTags::default()
        },
        LoudnessTarget::STREAMING,
    ));

    // The raised +1.0 dB only clips the album peak; the looser track peak would let it pass.
    assert_gain(result, -20.0 * 0.9_f64.log10());
}

#[test]
fn play_25_the_album_peak_cap_applies_after_the_raise() {
    let album_tagged = resolve_gain(tag_only(
        ReplayGainMode::Album,
        ReplayGainTags {
            album_gain_db: Some(-2.0),
            album_peak: Some(0.9),
            ..ReplayGainTags::default()
        },
        LoudnessTarget::STREAMING,
    ));
    // -2.0 + 4.0 = +2.0 dB would clip; capping first and raising after would give +2.0.
    assert_gain(album_tagged, -20.0 * 0.9_f64.log10());

    let album_measured = resolve_gain(GainInputs {
        album_measured: Some((-20.0, 0.9)),
        target: LoudnessTarget::STREAMING,
        ..inputs(ReplayGainMode::Album)
    });
    assert_gain(album_measured, -20.0 * 0.9_f64.log10());
}

#[test]
fn play_25_the_safe_range_clamp_applies_after_the_raise() {
    let measured = resolve_gain(GainInputs {
        measured: Some(MeasuredLoudness {
            integrated_lufs: -30.0,
            true_peak: 0.001,
        }),
        target: LoudnessTarget::STREAMING,
        ..inputs(ReplayGainMode::Track)
    });
    assert_eq!(measured.gain_db, MAX_GAIN_DB);

    let tagged = resolve_gain(tag_only(
        ReplayGainMode::Track,
        ReplayGainTags {
            track_gain_db: Some(10.0),
            track_peak: Some(0.001),
            ..ReplayGainTags::default()
        },
        LoudnessTarget::STREAMING,
    ));
    assert_eq!(tagged.gain_db, MAX_GAIN_DB);
}
