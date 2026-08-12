use super::super::device_sync_runtime::{PickerPlaylistRow, PickerSnapshot};
use super::*;
use reprise_core::device_sync::SelectionSource;

fn send_escape_key() {
    let status = std::process::Command::new("xdotool")
        .args(["key", "--clearmodifiers", "Escape"])
        .status()
        .expect("xdotool is required by the X11 display test");
    assert!(status.success(), "xdotool could not send Escape");
}

fn settle_until(label: &str, condition: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !condition() {
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(std::time::Instant::now() < deadline, "timed out: {label}");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn row(source: SelectionSource, selected: bool) -> PickerPlaylistRow {
    PickerPlaylistRow {
        source,
        name: "List".into(),
        smart: false,
        selected,
        track_count: 1,
        size_bytes: 1,
    }
}

#[test]
fn mtp_51_everything_and_named_playlist_rows_are_one_exclusive_selection() {
    let mut snapshot = PickerSnapshot {
        rows: vec![
            row(EVERYTHING_SOURCE, false),
            row(SelectionSource::Playlist(7), true),
        ],
        keep_smart_updated: true,
    };

    set_playlist_row_selected(&mut snapshot, 0, true);
    assert_eq!(
        snapshot
            .rows
            .iter()
            .map(|row| row.selected)
            .collect::<Vec<_>>(),
        [true, false]
    );

    set_playlist_row_selected(&mut snapshot, 1, true);
    assert_eq!(
        snapshot
            .rows
            .iter()
            .map(|row| row.selected)
            .collect::<Vec<_>>(),
        [false, true]
    );
}

#[test]
fn mtp_51_select_all_selects_every_named_playlist_without_selecting_everything() {
    let mut snapshot = PickerSnapshot {
        rows: vec![
            row(EVERYTHING_SOURCE, false),
            row(SelectionSource::Playlist(7), false),
            row(SelectionSource::Smart(8), false),
        ],
        keep_smart_updated: true,
    };

    select_all_rows(&mut snapshot);

    assert_eq!(
        snapshot
            .rows
            .iter()
            .map(|row| row.selected)
            .collect::<Vec<_>>(),
        [false, true, true]
    );
}

#[test]
fn mtp_51_escape_clears_a_filter_before_the_dialog_can_close() {
    let cleared = std::cell::Cell::new(false);
    assert_eq!(
        picker_escape_propagation(
            gtk4::gdk::Key::Escape,
            gtk4::gdk::ModifierType::empty(),
            "rock",
            &|| cleared.set(true),
        ),
        gtk4::glib::Propagation::Stop
    );
    assert!(cleared.get());

    assert_eq!(
        picker_escape_propagation(
            gtk4::gdk::Key::Escape,
            gtk4::gdk::ModifierType::empty(),
            "",
            &|| panic!("an empty picker filter must leave Escape to the dialog"),
        ),
        gtk4::glib::Propagation::Proceed
    );
}

#[test]
#[ignore = "requires a display and xdotool; run via xvfb-run"]
fn mtp_51_filtered_picker_escape_leaves_the_dialog_open() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let parent = gtk4::Window::new();
    parent.present();
    let filter = gtk4::SearchEntry::new();
    filter.set_text("rock");
    wire_picker_filter_escape(&filter);
    let dialog = libadwaita::Dialog::new();
    dialog.set_child(Some(&filter));
    dialog.present(Some(&parent));
    while gtk4::glib::MainContext::default().iteration(false) {}
    filter.grab_focus();

    send_escape_key();
    settle_until("the first Escape clears the picker filter", || {
        filter.text().is_empty()
    });
    assert!(
        dialog.is_visible(),
        "clearing the filter must not close the picker"
    );

    send_escape_key();
    settle_until("the second Escape reaches the dialog close", || {
        !dialog.is_visible()
    });
    parent.close();
}
