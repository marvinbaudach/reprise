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
