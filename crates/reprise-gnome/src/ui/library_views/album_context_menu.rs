//! Shared pointer/keyboard context menu for album grid cards.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use reprise_core::queries::AlbumSummary;

use crate::ui::album_card::{AlbumActionSlot, ArtistActivateSlot};
use crate::ui::popover_lifecycle;
use crate::ui::strings;

/// The action group name inserted on the parent widget — exported so
/// Task 7 (album view wiring) can reference it without reaching into
/// this module's private constants.
pub(in crate::ui) const ACTION_GROUP_NAME: &str = "albumctx";

const ACTION_PLAY: &str = "play";
const ACTION_PLAY_NEXT: &str = "play-next";
const ACTION_ADD_QUEUE: &str = "add-to-queue";
const ACTION_GO_TO_ARTIST: &str = "go-to-artist";
const ACTION_EDIT_TAGS: &str = "edit-tags";

/// Shared state captured by action closures.
pub(in crate::ui) struct AlbumMenuShared {
    /// The album under the cursor when the menu was opened.
    pub target_album: RefCell<Option<AlbumSummary>>,
    /// Callback: replace queue + play.
    pub on_play: AlbumActionSlot,
    /// Callback: prepend the album to Play Next.
    pub on_play_next: AlbumActionSlot,
    /// Callback: append to queue.
    pub on_queue: AlbumActionSlot,
    /// Callback: navigate to and select the album artist.
    pub on_artist: ArtistActivateSlot,
    /// Callback: open the batch tag editor for the album.
    pub on_edit_tags: AlbumActionSlot,
}

/// Builds the exact five-item model shared by pointer and keyboard openings.
fn build_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(
        Some(&strings::text(strings::ALBUM_MENU_PLAY)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_PLAY}")),
    );
    menu.append(
        Some(&strings::text(strings::ALBUM_MENU_PLAY_NEXT)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_PLAY_NEXT}")),
    );
    menu.append(
        Some(&strings::text(strings::ALBUM_MENU_ADD_QUEUE)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_ADD_QUEUE}")),
    );
    menu.append(
        Some(&strings::text(strings::ALBUM_MENU_GO_TO_ARTIST)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_GO_TO_ARTIST}")),
    );
    menu.append(
        Some(&strings::text(strings::ALBUM_MENU_EDIT_TAGS)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_EDIT_TAGS}")),
    );
    menu
}

/// Registers all context menu actions on the given action group and
/// returns it. Call `widget.insert_action_group(ACTION_GROUP_NAME, &group)`
/// on the album grid widget after wiring.
pub(in crate::ui) fn wire_actions(shared: &Rc<AlbumMenuShared>) -> gio::SimpleActionGroup {
    let group = gio::SimpleActionGroup::new();

    // Play.
    let play = gio::SimpleAction::new(ACTION_PLAY, None);
    {
        let shared = shared.clone();
        play.connect_activate(move |_, _| {
            let album = shared.target_album.borrow().clone();
            if let Some(album) = album {
                let cb = shared.on_play.borrow().clone();
                if let Some(cb) = cb {
                    cb(&album);
                }
            }
        });
    }
    group.add_action(&play);

    // Play next.
    let play_next = gio::SimpleAction::new(ACTION_PLAY_NEXT, None);
    {
        let shared = shared.clone();
        play_next.connect_activate(move |_, _| {
            let album = shared.target_album.borrow().clone();
            if let Some(album) = album {
                let cb = shared.on_play_next.borrow().clone();
                if let Some(cb) = cb {
                    cb(&album);
                }
            }
        });
    }
    group.add_action(&play_next);

    // Add to Queue.
    let add_queue = gio::SimpleAction::new(ACTION_ADD_QUEUE, None);
    {
        let shared = shared.clone();
        add_queue.connect_activate(move |_, _| {
            let album = shared.target_album.borrow().clone();
            if let Some(album) = album {
                let cb = shared.on_queue.borrow().clone();
                if let Some(cb) = cb {
                    cb(&album);
                }
            }
        });
    }
    group.add_action(&add_queue);

    // Go to artist.
    let go_to_artist = gio::SimpleAction::new(ACTION_GO_TO_ARTIST, None);
    {
        let shared = shared.clone();
        go_to_artist.connect_activate(move |_, _| {
            let album = shared.target_album.borrow().clone();
            if let Some(album) = album {
                let cb = shared.on_artist.borrow().clone();
                if let Some(cb) = cb {
                    cb(album.album_artist);
                }
            }
        });
    }
    group.add_action(&go_to_artist);

    // Edit tags.
    let edit_tags = gio::SimpleAction::new(ACTION_EDIT_TAGS, None);
    {
        let shared = shared.clone();
        edit_tags.connect_activate(move |_, _| {
            let album = shared.target_album.borrow().clone();
            if let Some(album) = album {
                let cb = shared.on_edit_tags.borrow().clone();
                if let Some(cb) = cb {
                    cb(&album);
                }
            }
        });
    }
    group.add_action(&edit_tags);

    group
}

/// Shows a context menu popover for the given album at the specified point
/// on `parent`. The menu model is rebuilt each time (for fresh playlist list).
pub(in crate::ui) fn show(
    parent: &impl IsA<gtk4::Widget>,
    shared: &Rc<AlbumMenuShared>,
    album: AlbumSummary,
    x: f64,
    y: f64,
) {
    *shared.target_album.borrow_mut() = Some(album);
    let menu_model = build_menu();
    let popover = gtk4::PopoverMenu::from_model(Some(&menu_model));
    popover.set_parent(parent.upcast_ref::<gtk4::Widget>());
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    popover.set_has_arrow(false);
    popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    popover.popup();
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn labels(model: &impl IsA<gio::MenuModel>) -> Vec<String> {
        let model = model.upcast_ref::<gio::MenuModel>();
        let mut result = Vec::new();
        for index in 0..model.n_items() {
            if let Some(label) = model
                .item_attribute_value(index, gio::MENU_ATTRIBUTE_LABEL, None)
                .and_then(|value| value.get::<String>())
            {
                result.push(label);
            }
            for link in [gio::MENU_LINK_SECTION, gio::MENU_LINK_SUBMENU] {
                if let Some(child) = model.item_link(index, link) {
                    result.extend(labels(&child));
                }
            }
        }
        result
    }

    #[test]
    fn build_menu_has_all_sections() {
        let menu = build_menu();
        assert_eq!(menu.n_items(), 5);
    }

    #[test]
    fn album_menu_contains_exactly_the_five_shared_actions_in_order() {
        assert_eq!(
            labels(&build_menu()),
            [
                "Play",
                "Play next",
                "Add to queue",
                "Go to artist",
                "Edit tags…",
            ]
        );
    }

    #[test]
    fn every_album_menu_item_invokes_its_shared_real_callback() {
        let album_calls = Rc::new(Cell::new(0));
        let artist_calls = Rc::new(Cell::new(0));
        let album_slot = || {
            let calls = album_calls.clone();
            Rc::new(RefCell::new(Some(Rc::new(move |_: &AlbumSummary| {
                calls.set(calls.get() + 1);
            })
                as crate::ui::album_card::AlbumAction)))
        };
        let artist_slot = {
            let calls = artist_calls.clone();
            Rc::new(RefCell::new(Some(Rc::new(move |artist: String| {
                assert_eq!(artist, "Artist");
                calls.set(calls.get() + 1);
            })
                as crate::ui::album_card::ArtistActivate)))
        };
        let shared = Rc::new(AlbumMenuShared {
            target_album: RefCell::new(Some(AlbumSummary {
                album: "Album".into(),
                album_artist: "Artist".into(),
                representative_path: String::new(),
                track_count: 1,
                year: None,
                total_duration_ms: 0,
                max_added_at: 0,
                total_play_count: 0,
            })),
            on_play: album_slot(),
            on_play_next: album_slot(),
            on_queue: album_slot(),
            on_artist: artist_slot,
            on_edit_tags: album_slot(),
        });
        let group = wire_actions(&shared);

        for action in [
            ACTION_PLAY,
            ACTION_PLAY_NEXT,
            ACTION_ADD_QUEUE,
            ACTION_EDIT_TAGS,
        ] {
            group.activate_action(action, None);
        }
        group.activate_action(ACTION_GO_TO_ARTIST, None);

        assert_eq!(album_calls.get(), 4);
        assert_eq!(artist_calls.get(), 1);
    }
}
