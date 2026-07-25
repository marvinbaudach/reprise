use super::*;

#[test]
fn ac_20_spectrum_frame_normalizes_decibels_and_rejects_non_finite_input() {
    let mut decibels = [-80.0_f32; SPECTRUM_BAND_COUNT];
    decibels[..16].copy_from_slice(&[
        -80.0,
        -72.0,
        -64.0,
        -56.0,
        -48.0,
        -40.0,
        -32.0,
        -24.0,
        -16.0,
        -8.0,
        0.0,
        -120.0,
        12.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ]);
    let frame = SpectrumFrame::from_decibels(decibels);

    assert_eq!(frame.bands()[0], 0.0);
    assert_eq!(frame.bands()[5], 0.5);
    assert_eq!(frame.bands()[10], 1.0);
    assert_eq!(frame.bands()[11], 0.0);
    assert_eq!(frame.bands()[12], 1.0);
    assert_eq!(&frame.bands()[13..16], &[0.0, 0.0, 0.0]);
    // Bands beyond the explicit prefix sit at the floor.
    assert!(frame.bands()[16..].iter().all(|&value| value == 0.0));
}

#[test]
fn ac_20_coalescing_uses_the_freshest_spectrum_but_retains_one_recent_hit() {
    let mut analyzer = SpectrumAnalyzer::new();
    for _ in 0..20 {
        analyzer.ingest([-80.0; SPECTRUM_ANALYSIS_BAND_COUNT]);
    }
    let hit = analyzer.ingest([0.0; SPECTRUM_ANALYSIS_BAND_COUNT]);
    let latest = analyzer.ingest([-80.0; SPECTRUM_ANALYSIS_BAND_COUNT]);
    assert!(hit.beat().fired);
    assert!(!latest.beat().fired);

    let coalesced = hit.coalesce_latest(latest);

    assert_eq!(coalesced.bands(), latest.bands());
    assert_eq!(coalesced.level(), latest.level());
    assert_eq!(coalesced.bass(), latest.bass());
    assert!(
        coalesced.beat().fired,
        "a hit one skipped analyzer frame ago must remain visible"
    );
    assert!(
        coalesced.beat().strength < hit.beat().strength,
        "a carried hit must age while skipped frames are collapsed"
    );
}

#[test]
fn ac_20_beat_strength_tracks_absolute_transient_energy() {
    fn kick_strength(peak_db: f32) -> f32 {
        let wall = [-50.0; SPECTRUM_ANALYSIS_BAND_COUNT];
        let mut analyzer = SpectrumAnalyzer::new();
        for _ in 0..120 {
            analyzer.ingest(wall);
        }
        let mut kick = wall;
        kick[..FLUX_LOW_BANDS].fill(peak_db);
        let beat = analyzer.ingest(kick).beat();
        assert!(beat.fired, "{peak_db} dB kick must be detected");
        beat.strength
    }

    let moderate = kick_strength(-38.0);
    let huge = kick_strength(0.0);
    assert!(
        huge >= moderate + 0.25,
        "a huge transient must visibly outrank a moderate one: moderate={moderate}, huge={huge}"
    );
}

#[test]
fn ac_20_sparse_fft_spikes_cannot_impersonate_a_full_energy_beat() {
    let wall = [-50.0; SPECTRUM_ANALYSIS_BAND_COUNT];
    let mut analyzer = SpectrumAnalyzer::new();
    for _ in 0..120 {
        analyzer.ingest(wall);
    }
    let edges = log_band_edges();
    let mut sparse = wall;
    for band in 32..SPECTRUM_BAND_COUNT {
        sparse[edges[band]] = 0.0;
    }

    let beat = analyzer.ingest(sparse).beat();
    assert!(beat.fired, "the sensitive onset path may detect the spike");
    assert!(
        beat.strength < 0.5,
        "sparse max-pooled bins must not look like a full-energy beat, got {}",
        beat.strength
    );
}
