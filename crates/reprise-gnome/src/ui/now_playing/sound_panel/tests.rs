use gtk4::gio::prelude::MenuModelExt;
use gtk4::prelude::Cast;
use reprise_core::sound_neighbours::{SoundNeighbour, SoundNeighbourResult};
use reprise_core::sound_snapshot::SoundSnapshotOptions;

use super::shown_track_ids;

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

#[test]
fn sim_5_queue_action_preserves_the_rendered_neighbour_order() {
    assert_eq!(SoundSnapshotOptions::default().limit, 7);
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
