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
#[ignore = "requires a display; run via xvfb-run"]
fn detected_rhythmdb_builds_the_import_row() {
    gtk4::init().unwrap();
    let dir = tempdir().unwrap();
    let rhythmdb = dir.path().join("rhythmdb.xml");
    assert!(build_import_row(&rhythmdb).is_none());

    fs::write(&rhythmdb, "<rhythmdb/>").unwrap();
    let surface = build_import_row(&rhythmdb).unwrap();
    assert_eq!(surface.row.title(), "Import from Rhythmbox");
    assert!(surface.row.is_activatable());
    let widgets = descendants(surface.row.upcast_ref());
    assert!(widgets
        .iter()
        .filter_map(|widget| widget.clone().downcast::<gtk4::Image>().ok())
        .any(|image| image.icon_name().as_deref() == Some("go-next-symbolic")));
    assert!(!widgets
        .iter()
        .any(gtk4::prelude::ObjectExt::is::<gtk4::Button>));
}

#[test]
fn statistics_are_selected_but_column_layout_requires_opt_in() {
    let options = import_option_specs();
    assert_eq!(options.len(), 5);
    assert_eq!(options[0].id, RhythmboxOption::ColumnLayout);
    assert!(!options[0].selected);
    assert_eq!(options[1].id, RhythmboxOption::Ratings);
    assert!(options[1].selected);
    assert_eq!(options[2].id, RhythmboxOption::PlayCountsAndLastPlayed);
    assert!(options[2].selected);
    assert_eq!(options[3].id, RhythmboxOption::DateAdded);
    assert!(options[3].selected);
    assert_eq!(options[4].id, RhythmboxOption::Playlists);
    assert!(options[4].selected);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn import_dialog_has_three_stack_states_and_five_option_rows() {
    gtk4::init().unwrap();
    let widgets = build_import_dialog();

    // Stack has three children
    assert!(widgets.stack.child_by_name("selection").is_some());
    assert!(widgets.stack.child_by_name("progress").is_some());
    assert!(widgets.stack.child_by_name("complete").is_some());

    // Five option rows with correct titles and defaults
    assert_eq!(widgets.rows.len(), 5);
    assert_eq!(widgets.rows[0].title(), "Column layout");
    assert_eq!(widgets.rows[1].title(), "Ratings");
    assert_eq!(widgets.rows[2].title(), "Play counts & last played");
    assert_eq!(widgets.rows[3].title(), "Date added");
    assert_eq!(widgets.rows[4].title(), "Playlists");
    assert!(!widgets.rows[0].is_active());
    assert!(widgets.rows[1].is_active());
    assert!(widgets.rows[2].is_active());
    assert!(widgets.rows[3].is_active());
    assert!(widgets.rows[4].is_active());

    // Import button starts insensitive (needs prescan)
    assert!(!widgets.import_button.is_sensitive());
}

fn descendants(root: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut found = Vec::new();
    collect_descendants(root, &mut found);
    found
}

fn collect_descendants(root: &gtk4::Widget, found: &mut Vec<gtk4::Widget>) {
    let mut child = root.first_child();
    while let Some(widget) = child {
        found.push(widget.clone());
        collect_descendants(&widget, found);
        child = widget.next_sibling();
    }
}
