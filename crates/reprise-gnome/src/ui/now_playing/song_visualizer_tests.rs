use super::*;

const BANDS: [f32; 16] = [
    0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.8, 0.6, 0.4, 0.3, 0.2, 0.1,
];

#[test]
fn ac_10_rings_flow_and_pulse_have_distinct_bounded_geometry() {
    let rings = scene(VisualPreset::Rings, &BANDS, 240.0, 220.0, 0.25);
    let flow = scene(VisualPreset::Flow, &BANDS, 240.0, 220.0, 0.25);
    let pulse = scene(VisualPreset::Pulse, &BANDS, 240.0, 220.0, 0.25);

    assert_eq!(rings.circles.len(), 4);
    assert_eq!(rings.bars.len(), 16);
    assert_eq!(flow.strokes.len(), 3);
    assert!(flow.circles.is_empty());
    assert_eq!(pulse.circles.len(), 2);
    assert_eq!(pulse.strokes.len(), 16);
    for scene in [&rings, &flow, &pulse] {
        assert!(scene.is_finite_and_bounded(240.0, 220.0));
    }
}

#[test]
fn ac_10_visual_presets_are_stable_keyboard_labels() {
    assert_eq!(
        VisualPreset::ALL.map(VisualPreset::label),
        ["Rings", "Flow", "Pulse"]
    );
}

#[test]
fn ac_10_louder_spectrum_changes_geometry_without_changing_cardinality() {
    let quiet = scene(VisualPreset::Rings, &[0.0; 16], 240.0, 220.0, 0.0);
    let loud = scene(VisualPreset::Rings, &[1.0; 16], 240.0, 220.0, 0.0);

    assert_eq!(quiet.bars.len(), loud.bars.len());
    assert!(
        loud.bars.iter().map(|bar| bar.length).sum::<f64>()
            > quiet.bars.iter().map(|bar| bar.length).sum::<f64>()
    );
}

#[test]
fn ac_10_visual_chrome_uses_the_shared_cover_accent_and_press_vocabulary() {
    let css = css();
    assert!(css.matches("@reprise_player_accent").count() >= 4);
    assert!(css.contains(".reprise-song-visual-preset:checked"));
    assert!(css.contains(".reprise-song-visual-fullscreen-canvas"));

    let buttons = crate::ui::style::buttons::css();
    assert!(buttons.contains(".reprise-btn-toggle:active"));
    assert!(buttons.contains(".reprise-btn-toggle:focus-visible"));
}

#[test]
fn ac_11_playing_moves_while_pause_settles_to_the_static_profile() {
    let mut state = RenderState {
        current: [0.0; SPECTRUM_BAND_COUNT],
        target: [1.0; SPECTRUM_BAND_COUNT],
        static_profile: [0.2; SPECTRUM_BAND_COUNT],
        phase: 0.0,
        preset: VisualPreset::Rings,
        playback: PlaybackState::Playing,
    };

    assert!(!advance_state(&mut state));
    assert!(state.phase > 0.0);
    assert!(state.current.iter().all(|band| *band > 0.0));

    let phase = state.phase;
    state.playback = PlaybackState::Paused;
    state.target = state.static_profile;
    assert!(!advance_state(&mut state));
    assert_eq!(state.phase, phase);
    assert!(state.current.iter().all(|band| *band < 0.2));
}

#[test]
fn ac_11_stop_then_track_clear_settles_instead_of_snapping() {
    let mut state = RenderState {
        current: [0.8; SPECTRUM_BAND_COUNT],
        target: [0.2; SPECTRUM_BAND_COUNT],
        static_profile: [0.2; SPECTRUM_BAND_COUNT],
        phase: 0.5,
        preset: VisualPreset::Rings,
        playback: PlaybackState::Stopped,
    };

    clear_static_profile(&mut state, true);
    assert_eq!(state.current, [0.8; SPECTRUM_BAND_COUNT]);
    assert_eq!(state.target, NEUTRAL_PROFILE);
    assert!(!advance_state(&mut state));
    assert!(state
        .current
        .iter()
        .all(|band| *band < 0.8 && *band > NEUTRAL_PROFILE[0]));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_10_visual_widget_exposes_a_labeled_canvas_and_three_keyboard_presets() {
    gtk4::init().unwrap();
    let visualizer = SongVisualizer::new();

    assert_eq!(visualizer.area.accessible_role(), gtk4::AccessibleRole::Img);
    assert!(gtk4::test_accessible_has_property(
        &visualizer.area,
        gtk4::AccessibleProperty::Label
    ));
    let presets = visualizer
        .root
        .last_child()
        .expect("preset row")
        .downcast::<gtk4::Box>()
        .unwrap();
    let mut child = presets.first_child();
    let mut labels = Vec::new();
    while let Some(widget) = child {
        let button = widget.clone().downcast::<gtk4::ToggleButton>().unwrap();
        assert!(button.is_focusable());
        labels.push(button.label().unwrap().to_string());
        child = widget.next_sibling();
    }
    assert_eq!(labels, ["Rings", "Flow", "Pulse"]);
}
