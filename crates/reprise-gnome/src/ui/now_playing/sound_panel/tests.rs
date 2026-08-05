use gtk4::gio::prelude::MenuModelExt;
use gtk4::prelude::Cast;
use reprise_core::sound_features::SoundFeatures;
use reprise_core::sound_neighbours::{SoundNeighbour, SoundNeighbourResult};
use reprise_core::sound_stats::compute_sound_stats;

use super::{profile_positions, ready_for_matches, shown_track_ids, MIN_READY_FEATURES};

fn menu_labels(model: &gtk4::gio::MenuModel) -> Vec<String> {
    let mut labels = Vec::new();
    for index in 0..model.n_items() {
        if let Some(label) = model
            .item_attribute_value(index, gtk4::gio::MENU_ATTRIBUTE_LABEL, None)
            .and_then(|value| value.get::<String>())
        {
            labels.push(label);
        }
        if let Some(section) = model.item_link(index, gtk4::gio::MENU_LINK_SECTION) {
            labels.extend(menu_labels(&section));
        }
    }
    labels
}

fn feature(timbre: f32, dynamics: f32, tempo: Option<f32>) -> SoundFeatures {
    SoundFeatures {
        band_mean: [0.0; reprise_core::spectrogram::SPECTROGRAM_BAND_COUNT],
        centroid_mean: timbre,
        centroid_var: 0.0,
        frame_crest_db: dynamics,
        tempo,
    }
}

#[test]
fn sim_4_panel_requires_fifty_profiles_and_the_current_track() {
    assert!(!ready_for_matches(MIN_READY_FEATURES - 1, true));
    assert!(!ready_for_matches(MIN_READY_FEATURES, false));
    assert!(ready_for_matches(MIN_READY_FEATURES, true));
}

#[test]
fn sim_4_profile_uses_library_percentiles_and_disables_tempo_when_excluded() {
    let values = [
        feature(1.0, 10.0, Some(60.0)),
        feature(2.0, 20.0, Some(90.0)),
        feature(3.0, 30.0, Some(120.0)),
    ];
    let stats = compute_sound_stats(&values);

    let positions = profile_positions(&values[1], &stats, false);

    assert_eq!(positions.timbre, 50.0);
    assert_eq!(positions.dynamics, 50.0);
    assert_eq!(positions.tempo, None);
}

#[test]
fn sim_5_queue_action_preserves_the_rendered_neighbour_order() {
    assert_eq!(super::SoundPanelOptions::default().limit, 7);
    let result = SoundNeighbourResult {
        library_count: 42,
        matches: vec![
            SoundNeighbour {
                track_id: 9,
                path: "/9.flac".into(),
                title: "Nine".into(),
                artist: "A".into(),
                album: "Ninth".into(),
                album_artist: "A".into(),
                distance: 0.1,
                percentile: 99.0,
            },
            SoundNeighbour {
                track_id: 3,
                path: "/3.flac".into(),
                title: "Three".into(),
                artist: "B".into(),
                album: "Third".into(),
                album_artist: "B".into(),
                distance: 0.2,
                percentile: 95.0,
            },
        ],
    };

    assert_eq!(shown_track_ids(&result), vec![9, 3]);
}

#[test]
fn sound_result_uses_the_standard_compact_track_context_menu() {
    assert_eq!(
        menu_labels(super::list::build_context_menu_model().upcast_ref()),
        ["Play next", "Add to queue", "Go to album"]
    );
}
