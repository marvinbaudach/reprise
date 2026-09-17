use super::*;

fn point(frequency_hz: f64, gain_db: f64) -> AndroidEqualizerPoint {
    AndroidEqualizerPoint {
        frequency_hz,
        gain_db,
    }
}

fn capability(
    frequency_hz: f64,
    minimum_gain_db: f64,
    maximum_gain_db: f64,
) -> AndroidEqualizerBandCapability {
    AndroidEqualizerBandCapability {
        frequency_hz,
        minimum_gain_db,
        maximum_gain_db,
    }
}

/// The case the Kotlin copy of this arithmetic used to assert on its own: a
/// two-point curve read by a three-band device, sampled in log frequency. The
/// midpoint between 100 Hz and 10 kHz is 1 kHz, so a -6/+6 curve reads 0 there.
#[test]
fn the_device_contributes_its_bands_and_the_core_does_the_sampling() {
    let projected = project_equalizer_curve(
        vec![point(100.0, -6.0), point(10_000.0, 6.0)],
        vec![
            capability(100.0, -12.0, 12.0),
            capability(1_000.0, -12.0, 12.0),
            capability(10_000.0, -12.0, 12.0),
        ],
    )
    .unwrap();

    assert_eq!(
        projected
            .iter()
            .map(|band| band.frequency_hz)
            .collect::<Vec<_>>(),
        vec![100.0, 1_000.0, 10_000.0],
    );
    for (band, expected) in projected.iter().zip([-6.0, 0.0, 6.0]) {
        assert!(
            (band.gain_db - expected).abs() < 1e-9,
            "{} Hz read {} dB, expected {expected} dB",
            band.frequency_hz,
            band.gain_db,
        );
    }
    // The device's own limits travel back out with each band, so the surface
    // showing them never has to guess at the range of a slider.
    assert!(projected
        .iter()
        .all(|band| band.minimum_gain_db == -12.0 && band.maximum_gain_db == 12.0));
}

/// The authored curve outreaches this device. The projection is clamped to what
/// the hardware can render — and the curve itself is untouched, because a
/// projection is a picture of it, never a write back to it.
#[test]
fn a_level_the_device_cannot_reach_is_clamped_to_what_it_can() {
    let curve = vec![point(60.0, -12.0), point(14_000.0, 12.0)];
    let projected = project_equalizer_curve(
        curve.clone(),
        vec![capability(60.0, -3.0, 3.0), capability(14_000.0, -3.0, 3.0)],
    )
    .unwrap();

    assert_eq!(
        projected
            .iter()
            .map(|band| band.gain_db)
            .collect::<Vec<_>>(),
        vec![-3.0, 3.0],
    );
    assert_eq!(curve[0].gain_db, -12.0);
    assert_eq!(curve[1].gain_db, 12.0);
}

/// Nothing here may panic across the FFI: a device that reports an unusable band
/// list, or a curve that never should have got this far, has to come back as an
/// error the caller can handle.
#[test]
fn an_unusable_curve_or_band_list_comes_back_as_an_error() {
    assert!(matches!(
        project_equalizer_curve(Vec::new(), vec![capability(60.0, -3.0, 3.0)]),
        Err(LibraryError::InvalidPlaybackSetting { .. }),
    ));
    assert!(matches!(
        project_equalizer_curve(
            vec![point(60.0, 0.0)],
            vec![capability(60.0, -3.0, 3.0), capability(60.0, -3.0, 3.0)],
        ),
        Err(LibraryError::InvalidPlaybackSetting { .. }),
    ));
    assert!(matches!(
        project_equalizer_curve(vec![point(60.0, 0.0)], vec![capability(60.0, 3.0, -3.0)],),
        Err(LibraryError::InvalidPlaybackSetting { .. }),
    ));
    // No bands at all is not a failure — it is a device with nothing to show.
    assert_eq!(
        project_equalizer_curve(vec![point(60.0, 0.0)], Vec::new()).unwrap(),
        Vec::new(),
    );
}

#[test]
fn android_receives_the_standard_presets_in_desktop_order() {
    let presets = standard_equalizer_presets();
    let expected_presets = EqualizerPreset::ALL
        .into_iter()
        .map(AndroidEqualizerPreset::from)
        .collect::<Vec<_>>();
    let expected_curves = EqualizerPreset::ALL
        .into_iter()
        .map(|preset| preset.ten_band_levels().to_vec())
        .collect::<Vec<_>>();

    assert_eq!(
        presets
            .iter()
            .map(|definition| definition.preset)
            .collect::<Vec<_>>(),
        expected_presets,
    );
    assert_eq!(
        presets
            .iter()
            .map(|definition| {
                definition
                    .curve
                    .iter()
                    .map(|point| point.gain_db)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        expected_curves,
    );
    assert!(presets.iter().all(|definition| {
        definition
            .curve
            .iter()
            .map(|point| point.frequency_hz)
            .eq(reprise_core::equalizer::GSTREAMER_EQUALIZER_CENTRES_HZ)
    }));
}

#[test]
fn the_bridge_offers_every_shared_preset() {
    let presets = standard_equalizer_presets();

    assert_eq!(presets.len(), EqualizerPreset::ALL.len());
    assert!(presets
        .iter()
        .zip(EqualizerPreset::ALL)
        .all(|(definition, preset)| definition.preset == preset.into()));
}
