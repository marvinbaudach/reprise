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
fn import_offer_treats_unknown_as_not_absent() {
    use reprise_core::library::source::{LibraryPathMetadata, LibraryPathPresence};

    let file = || {
        LibraryPathPresence::Present(LibraryPathMetadata {
            is_file: true,
            is_directory: false,
            size: None,
            modified: None,
            identity: None,
        })
    };
    let directory = LibraryPathPresence::Present(LibraryPathMetadata {
        is_file: false,
        is_directory: true,
        size: None,
        modified: None,
        identity: None,
    });

    assert!(!should_offer_rhythmbox_import(LibraryPathPresence::Absent));
    assert!(!should_offer_rhythmbox_import(directory));
    assert!(should_offer_rhythmbox_import(LibraryPathPresence::Unknown));
    assert!(should_offer_rhythmbox_import(file()));
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

/// A stand-in environment: the tests never read or write the process's own.
fn variables<'a>(
    pairs: &'a [(&'a str, &'a str)],
) -> impl Fn(&str) -> Option<std::ffi::OsString> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| std::ffi::OsString::from(*value))
    }
}

const FLATHUB_DATABASE: &str =
    "/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox/rhythmdb.xml";

#[test]
fn start_7_the_importer_finds_the_database_through_the_resolver() {
    let environment = [("HOME", "/home/tester")];
    let native = || std::path::PathBuf::from("/home/tester/.local/share");

    // Nothing exists: the native directory is where a missing library is reported.
    assert_eq!(
        rhythmdb_path_for(false, native(), variables(&environment), |_| false),
        std::path::PathBuf::from("/home/tester/.local/share/rhythmbox/rhythmdb.xml")
    );
    // Only the Flathub Rhythmbox has a database: the old native-only join missed it.
    assert_eq!(
        rhythmdb_path_for(false, native(), variables(&environment), |path| path
            == std::path::Path::new(FLATHUB_DATABASE)),
        std::path::PathBuf::from(FLATHUB_DATABASE)
    );
    // The override wins untouched, even when a database exists elsewhere.
    assert_eq!(
        rhythmdb_path_for(
            false,
            native(),
            variables(&[
                ("HOME", "/home/tester"),
                ("REPRISE_RHYTHMDB_PATH", "/chosen/db.xml")
            ]),
            |_| true
        ),
        std::path::PathBuf::from("/chosen/db.xml")
    );
}

#[test]
fn start_7_the_importer_takes_its_playlists_file_from_the_resolver() {
    let rhythmdb = std::path::Path::new("/found/rhythmbox/rhythmdb.xml");

    assert_eq!(
        playlists_path_for(rhythmdb, variables(&[])),
        std::path::PathBuf::from("/found/rhythmbox/playlists.xml")
    );
    assert_eq!(
        playlists_path_for(
            rhythmdb,
            variables(&[("REPRISE_RHYTHMBOX_PLAYLISTS_PATH", "/chosen/playlists.xml")])
        ),
        std::path::PathBuf::from("/chosen/playlists.xml")
    );
    assert_eq!(
        playlists_path_for(
            rhythmdb,
            variables(&[("REPRISE_RHYTHMBOX_PLAYLISTS_PATH", "")])
        ),
        std::path::PathBuf::from("/found/rhythmbox/playlists.xml")
    );
}
