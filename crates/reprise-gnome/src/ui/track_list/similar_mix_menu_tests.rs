use gtk4::gio;
use gtk4::prelude::*;

use super::track_menu::{
    action_states, build_track_menu, MenuContext, MenuInputs, SelectionSummary,
};

fn labels(model: &gio::MenuModel, out: &mut Vec<String>) {
    for item in 0..model.n_items() {
        if let Some(value) = model.item_attribute_value(item, "label", None) {
            if let Some(label) = value.str() {
                out.push(label.to_string());
            }
        }
        for link in ["section", "submenu"] {
            if let Some(child) = model.item_link(item, link) {
                labels(&child, out);
            }
        }
    }
}

#[test]
fn ac_12_similar_mix_is_a_playable_selection_action() {
    let playable = SelectionSummary {
        count: 2,
        any_missing: true,
        all_missing: false,
        same_album: false,
        same_artist: false,
        same_folder: false,
    };
    assert!(action_states(MenuContext::LibraryTracks, &playable).similar_mix);

    let menu = build_track_menu(&MenuInputs {
        context: MenuContext::LibraryTracks,
        selection: &playable,
        playlists: &[],
        is_missing_view: false,
    });
    let mut menu_labels = Vec::new();
    labels(menu.upcast_ref(), &mut menu_labels);
    assert!(menu_labels
        .iter()
        .any(|label| label == "Create similar mix…"));

    let all_missing = SelectionSummary {
        all_missing: true,
        ..playable
    };
    assert!(!action_states(MenuContext::LibraryTracks, &all_missing).similar_mix);
}
