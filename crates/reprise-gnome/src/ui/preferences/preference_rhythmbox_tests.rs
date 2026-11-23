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

fn rendered_texts(widget: &gtk4::Widget, out: &mut Vec<String>) {
    if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
        out.push(label.text().to_string());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        rendered_texts(&current, out);
        child = current.next_sibling();
    }
}

/// FB-13: building the import dialog parses no title as markup. Two titles
/// carry a literal `&`; a row that is handed such a title while it still parses
/// markup logs "Gtk-WARNING: Failed to set text ... from markup". The published
/// 0.1.346 Flatpak logged it three times while the first-run wizard opened
/// this dialog, even though the labels settled on the right text afterwards,
/// so the test reads both the log and the rendered labels.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_13_the_import_dialog_rows_render_their_ampersand_titles() {
    gtk4::init().unwrap();
    libadwaita::init().unwrap();
    let mark = crate::ui::gtk_log_capture::mark();
    let widgets = build_import_dialog();
    let skipped = strings::rhythmbox_skipped_warning(1, 2, 4, 3);
    widgets.warning_row.set_title(&skipped.title);
    widgets.warning_row.set_subtitle(&skipped.reasons);
    widgets.warning_row.set_visible(true);
    widgets
        .skipped_expander
        .set_title(&strings::rhythmbox_entries_skipped(10));
    widgets.skip_not_yet_scanned.set_subtitle("4");
    widgets.skip_non_song.set_subtitle("3");
    widgets
        .play_counts_result
        .set_subtitle(&strings::rhythmbox_result_play_counts(4, 6));

    widgets.stack.set_visible_child_name("complete");
    let parent = adw::Window::builder()
        .default_width(800)
        .default_height(900)
        .build();
    parent.present();
    widgets.dialog.present(Some(&parent));
    crate::ui::test_settle::settle_for(std::time::Duration::from_millis(500));
    let content = widgets.dialog.child().expect("dialog content");

    let mut texts = Vec::new();
    rendered_texts(content.upcast_ref(), &mut texts);
    widgets.dialog.force_close();
    parent.close();

    let warnings = crate::ui::gtk_log_capture::messages_since(mark);
    assert!(
        warnings
            .iter()
            .all(|message| !message.contains("from markup")),
        "GTK parsed a row title as markup: {warnings:?}"
    );

    // The option row (selection page) and the result row (complete page) both
    // carry this title; every other title appears on one row only.
    for (title, rows) in [
        (strings::RHYTHMBOX_PLAY_COUNTS_AND_LAST_PLAYED, 2),
        (strings::RHYTHMBOX_SKIP_NON_SONG, 1),
        (strings::RHYTHMBOX_SKIP_OUTSIDE_LIBRARY, 1),
        (strings::RHYTHMBOX_SKIP_MISSING_ON_DISK, 1),
        (strings::RHYTHMBOX_SKIP_NOT_YET_SCANNED, 1),
    ] {
        let rendered = texts.iter().filter(|text| *text == title).count();
        assert_eq!(
            rendered, rows,
            "{title:?} rendered on {rendered} rows, expected {rows}: {texts:?}"
        );
    }
}
