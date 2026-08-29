//! Display-dependent proof that a playlist which disappeared from the library
//! also leaves the sync card's `GtkListBox`. The card's own `rows` vector is
//! rebuilt correctly on every source change, so asserting on it alone cannot
//! see an orphaned button that stayed in the list — only walking the list's
//! children can. Its siblings in `device_sync_page_display_tests.rs` live in
//! their own file to keep that one under the project's 800-line limit.

use super::*;

fn named_row(source: SelectionSource, name: &str, smart: bool) -> SyncPlaylistRow {
    SyncPlaylistRow {
        source,
        name: Some(name.into()),
        smart,
        ..row()
    }
}

fn list_children(list: &gtk4::ListBox) -> Vec<gtk4::Widget> {
    let mut children = Vec::new();
    let mut next = list.first_child();
    while let Some(child) = next {
        next = child.next_sibling();
        children.push(child);
    }
    children
}

fn page_actions() -> PageActions {
    PageActions {
        set_profile: Rc::new(|_| {}),
        set_playlist: Rc::new(|_, _| {}),
        start: Rc::new(|| {}),
        cancel: Rc::new(|| {}),
        eject: Rc::new(|| {}),
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_deleted_playlist_takes_its_row_widget_with_it() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");

    let mut before = device();
    before.page.playlists = vec![
        named_row(
            SelectionSource::Playlist(1),
            "Like Immortal Disfigurement",
            false,
        ),
        named_row(SelectionSource::Playlist(2), "Lorna Shore & Similar", false),
        named_row(SelectionSource::Smart(3), "Recently added", true),
    ];

    let (surface, _root) = DeviceSyncPage::new(&before, page_actions(), &no_op_content_actions());
    assert_eq!(
        list_children(&surface.playlist_card.list).len(),
        3,
        "the card must start with one list child per projected playlist"
    );

    let mut after = before.clone();
    after.page.playlists.remove(0);
    surface.update(&after);

    assert_eq!(
        surface.playlist_card.rows.borrow().len(),
        2,
        "the card's row vector must shrink with the projected source list"
    );
    assert_eq!(
        list_children(&surface.playlist_card.list).len(),
        2,
        "the deleted playlist's button must leave the list, not merely the row vector"
    );
    assert!(
        !surface.root_text().contains("Like Immortal Disfigurement"),
        "a playlist deleted from the library is still shown on the device page: {}",
        surface.root_text()
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_selection_summary_still_names_its_count() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");

    let mut device = device();
    device.page.unique_track_count = 741;

    let (surface, _root) = DeviceSyncPage::new(&device, page_actions(), &no_op_content_actions());

    let text = surface.root_text();
    assert!(
        text.contains("741 unique tracks"),
        "the selection summary dropped its count — a plural form without a \
         `{{count}}` placeholder renders as a bare noun: {text}"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn remembered_rows_verification_and_preview_render_without_live_measurement_claims() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");

    let mut empty = device();
    empty.connected = false;
    empty.session_state = reprise_core::device_sync::DeviceSessionState::Remembered;
    empty.storage = DeviceStorageSnapshot::default();
    empty.storage_measured = false;
    empty.page.playlists.clear();
    empty.page.unique_track_count = 0;
    empty.page.target_bytes = 0;
    empty.page.changes = SyncChangeSummary::default();
    empty.page.controls = SyncPageControls {
        editable: true,
        can_start: false,
        can_cancel: false,
        can_eject: false,
    };
    let (surface, _root) = DeviceSyncPage::new(&empty, page_actions(), &no_op_content_actions());
    assert!(surface.playlist_card.rows.borrow().is_empty());
    assert!(surface.root_text().contains("0 unique tracks"));

    let verified_at = chrono::Utc::now() - chrono::Duration::days(1);
    let mut remembered = empty.clone();
    remembered.last_sync = Some(verified_at);
    remembered.contents_state =
        reprise_core::device_sync::device_view::DeviceContentsState::VerifiedEarlier(verified_at);
    remembered.page.playlists = vec![
        named_row(SelectionSource::Playlist(2), "Road", false),
        named_row(SelectionSource::Smart(2), "Recently added", true),
    ];
    remembered.page.unique_track_count = 3;
    remembered.page.target_bytes = 32 * 1_024;
    remembered.page.changes = SyncChangeSummary {
        additions: 2,
        replacements: 1,
        removals: 7,
        playlist_writes: 2,
        transfer_bytes: 32 * 1_024,
        ..Default::default()
    };
    remembered.page.storage = reprise_core::device_sync::project_storage(
        &remembered.storage,
        &reprise_core::device_sync::MirrorPlan {
            transfer_bytes: remembered.page.changes.transfer_bytes,
            ..Default::default()
        },
    );
    surface.update(&remembered);

    let rows = surface.playlist_card.rows.borrow();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.button.is_active()));
    assert!(rows.iter().all(|row| row.button.is_sensitive()));
    let text = surface.root_text();
    assert!(text.contains("Road"));
    assert!(text.contains("Recently added"));
    assert!(text.contains("3 unique tracks"));
    assert!(text.contains("2 files to copy"));
    assert!(text.contains("2 playlist writes"));
    assert!(text.contains("Files to remove are settled when the device is next inspected."));
    assert!(!text.contains("7 removed"));
    let storage_summary = surface.dashboard.storage_summary.text();
    assert_eq!(
        storage_summary,
        "Write access unknown · Storage projection is unavailable until the selection is valid."
    );
    assert!(
        !storage_summary.contains("Music ") && !text.contains("Reprise music "),
        "an unmeasured storage snapshot must not render a music byte figure: {text}"
    );
    assert!(!text.contains("Device contents never verified"));
    assert!(!surface.on_device.check_button_is_sensitive());
    drop(rows);

    let mut active = device();
    let mut after = composition(Some(48 * 1_024));
    after.reprise_music_bytes = 48 * 1_024;
    active.page.storage.after_sync = Some(after);
    active.page.storage.transfer_bytes = 16 * 1_024;
    surface.update(&active);
    let active_text = surface.root_text();
    assert!(active_text.contains(
        "Writable · Music 48.0 KiB · after sync +16.0 KiB · Other 16.0 KiB · Free 48.0 KiB"
    ));
    assert!(active_text
        .contains("Reprise music 48.0 KiB · this run +16.0 KiB · Other 16.0 KiB · 48.0 KiB free"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn active_and_inert_controls_keep_their_hardware_boundaries() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");

    let active = device();
    let (surface, _root) = DeviceSyncPage::new(&active, page_actions(), &no_op_content_actions());
    let active_text = surface.root_text();
    assert!(surface.dashboard.profile.is_sensitive());
    assert!(surface.dashboard.eject.is_sensitive());
    assert!(surface.dashboard.dock.primary.is_sensitive());

    let mut inert = active.clone();
    inert.session_state = reprise_core::device_sync::DeviceSessionState::Inert {
        active_device_name: "Other phone".into(),
    };
    inert.page.controls = SyncPageControls {
        editable: true,
        can_start: false,
        can_cancel: false,
        can_eject: false,
    };
    surface.update(&inert);
    assert!(surface.dashboard.profile.is_sensitive());
    assert!(surface
        .playlist_card
        .rows
        .borrow()
        .iter()
        .all(|row| row.button.is_sensitive()));
    assert!(!surface.dashboard.eject.is_sensitive());
    assert!(!surface.dashboard.dock.primary.is_sensitive());

    surface.update(&active);
    assert_eq!(surface.root_text(), active_text);
    assert!(surface.dashboard.eject.is_sensitive());
    assert!(surface.dashboard.dock.primary.is_sensitive());
}
