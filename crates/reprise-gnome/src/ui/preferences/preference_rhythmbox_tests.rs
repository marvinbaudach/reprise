use std::fs;

use tempfile::tempdir;

use super::*;

#[test]
fn import_row_requires_a_detected_rhythmdb_file() {
    let dir = tempdir().unwrap();
    let rhythmdb = dir.path().join("rhythmdb.xml");

    assert!(!rhythmbox_data_available(&rhythmdb));
    fs::write(&rhythmdb, "<rhythmdb/>").unwrap();
    assert!(rhythmbox_data_available(&rhythmdb));
    assert!(!rhythmbox_data_available(dir.path()));
}

#[test]
fn only_supported_library_data_is_selected_for_import() {
    let options = import_option_specs();
    assert_eq!(options.len(), 4);
    assert_eq!(options[0].id, RhythmboxOption::Ratings);
    assert!(options[0].selected);
    assert_eq!(options[1].id, RhythmboxOption::PlayCountsAndLastPlayed);
    assert!(options[1].selected);
    assert_eq!(options[2].id, RhythmboxOption::DateAdded);
    assert!(options[2].selected);
    assert_eq!(options[3].id, RhythmboxOption::Playlists);
    assert!(options[3].selected);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn import_dialog_has_three_stack_states_and_four_readable_option_rows() {
    gtk4::init().unwrap();
    let widgets = build_import_dialog();

    // Stack has three children
    assert!(widgets.stack.child_by_name("selection").is_some());
    assert!(widgets.stack.child_by_name("progress").is_some());
    assert!(widgets.stack.child_by_name("complete").is_some());

    // Four supported data options with literal, readable titles.
    assert_eq!(widgets.rows.len(), 4);
    assert_eq!(widgets.rows[0].title(), "Ratings");
    assert_eq!(widgets.rows[1].title(), "Play counts & last played");
    assert_eq!(widgets.rows[2].title(), "Date added");
    assert_eq!(widgets.rows[3].title(), "Playlists");
    assert!(widgets.rows.iter().all(adw::SwitchRow::is_active));
    assert!(widgets
        .rows
        .iter()
        .all(|row| !adw::prelude::PreferencesRowExt::uses_markup(row)));

    // Import button starts insensitive (needs prescan)
    assert!(!widgets.import_button.is_sensitive());
}
