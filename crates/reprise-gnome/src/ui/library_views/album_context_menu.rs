//! Right-click context menu for album grid cards. Builds a GMenu model
//! and a `gio::SimpleActionGroup` with handlers for Play, Shuffle,
//! Add to Queue, Add to Playlist (submenu), Edit Tags, Go to Folder.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library::playlists;
use reprise_core::queries::AlbumSummary;
use rusqlite::Connection;

use crate::ui::album_card_actions;
use crate::ui::popover_lifecycle;
use crate::ui::strings;
use crate::ui::track_actions;

/// The action group name inserted on the parent widget — exported so
/// Task 7 (album view wiring) can reference it without reaching into
/// this module's private constants.
pub(in crate::ui) const ACTION_GROUP_NAME: &str = "albumctx";

const ACTION_PLAY: &str = "play";
const ACTION_SHUFFLE: &str = "shuffle";
const ACTION_ADD_QUEUE: &str = "add-to-queue";
const ACTION_ADD_PLAYLIST: &str = "add-to-playlist";
const ACTION_NEW_PLAYLIST: &str = "new-playlist";
const ACTION_EDIT_TAGS: &str = "edit-tags";
const ACTION_GO_TO_FOLDER: &str = "go-to-folder";

/// Shared state captured by action closures.
pub(in crate::ui) struct AlbumMenuShared {
    pub conn: Rc<RefCell<Connection>>,
    /// The album under the cursor when the menu was opened.
    pub target_album: RefCell<Option<AlbumSummary>>,
    /// Callback: replace queue + play.
    pub on_play: Rc<RefCell<Option<Rc<dyn Fn(&AlbumSummary)>>>>,
    /// Callback: append to queue.
    pub on_queue: Rc<RefCell<Option<Rc<dyn Fn(&AlbumSummary)>>>>,
    /// Callback: shuffle + play.
    pub on_shuffle: Rc<RefCell<Option<Rc<dyn Fn(&AlbumSummary)>>>>,
    /// Callback: show toast after playlist add.
    pub on_toast: Rc<RefCell<Option<Rc<dyn Fn(String)>>>>,
}

/// Builds the full GMenu model (rebuilt on each show to refresh playlists).
fn build_menu(conn: &Connection) -> gio::Menu {
    let menu = gio::Menu::new();

    // Section 1 — primary: Play, Shuffle, Add to Queue.
    let primary = gio::Menu::new();
    primary.append(
        Some(&strings::text(strings::ALBUM_MENU_PLAY)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_PLAY}")),
    );
    primary.append(
        Some(&strings::text(strings::ALBUM_MENU_SHUFFLE)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_SHUFFLE}")),
    );
    primary.append(
        Some(&strings::text(strings::ALBUM_MENU_ADD_QUEUE)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_ADD_QUEUE}")),
    );
    menu.append_section(None, &primary);

    // Section 2 — playlist: Add to Playlist submenu (with New Playlist leaf).
    let playlist_section = gio::Menu::new();
    let playlist_sub = gio::Menu::new();
    if let Ok(lists) = playlists::list(conn) {
        for pl in lists {
            let item = gio::MenuItem::new(Some(&pl.name), None);
            item.set_action_and_target_value(
                Some(&format!("{ACTION_GROUP_NAME}.{ACTION_ADD_PLAYLIST}")),
                Some(&pl.id.to_variant()),
            );
            playlist_sub.append_item(&item);
        }
    }
    playlist_sub.append(
        Some(&strings::text(strings::ALBUM_MENU_NEW_PLAYLIST)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_NEW_PLAYLIST}")),
    );
    playlist_section.append_submenu(
        Some(&strings::text(strings::ALBUM_MENU_ADD_PLAYLIST)),
        &playlist_sub,
    );
    menu.append_section(None, &playlist_section);

    // Section 3 — utility: Edit Tags, Go to Folder.
    let utility = gio::Menu::new();
    utility.append(
        Some(&strings::text(strings::ALBUM_MENU_EDIT_TAGS)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_EDIT_TAGS}")),
    );
    utility.append(
        Some(&strings::text(strings::ALBUM_MENU_GO_TO_FOLDER)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_GO_TO_FOLDER}")),
    );
    menu.append_section(None, &utility);

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

    // Shuffle.
    let shuffle = gio::SimpleAction::new(ACTION_SHUFFLE, None);
    {
        let shared = shared.clone();
        shuffle.connect_activate(move |_, _| {
            let album = shared.target_album.borrow().clone();
            if let Some(album) = album {
                let cb = shared.on_shuffle.borrow().clone();
                if let Some(cb) = cb {
                    cb(&album);
                }
            }
        });
    }
    group.add_action(&shuffle);

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

    // Add to Playlist (with playlist id parameter).
    let add_playlist =
        gio::SimpleAction::new(ACTION_ADD_PLAYLIST, Some(glib::VariantTy::INT64));
    {
        let shared = shared.clone();
        add_playlist.connect_activate(move |_, param| {
            let Some(playlist_id) = param.and_then(|v| v.get::<i64>()) else {
                tracing::warn!("album context menu: add-to-playlist fired with no playlist id");
                return;
            };
            let album = shared.target_album.borrow().clone();
            let Some(album) = album else { return };
            // Fetch ids first, drop the borrow, then call add_selected_to_playlist.
            let ids = {
                let conn = shared.conn.borrow();
                album_card_actions::album_track_ids(&conn, &album)
            };
            if ids.is_empty() {
                return;
            }
            match track_actions::add_selected_to_playlist(&shared.conn, playlist_id, &ids) {
                Ok(count) => {
                    tracing::info!(playlist_id, count, "album context menu: tracks added to playlist");
                    let cb = shared.on_toast.borrow().clone();
                    if let Some(cb) = cb {
                        cb(strings::tracks_added_to_playlist_toast(count as usize, &playlist_id_name(&shared.conn, playlist_id)));
                    }
                }
                Err(error) => {
                    tracing::error!(%error, playlist_id, "album context menu: failed to add tracks to playlist");
                }
            }
        });
    }
    group.add_action(&add_playlist);

    // New Playlist — uses album title as playlist name (v1 simplest approach).
    let new_playlist = gio::SimpleAction::new(ACTION_NEW_PLAYLIST, None);
    {
        let shared = shared.clone();
        new_playlist.connect_activate(move |_, _| {
            let album = shared.target_album.borrow().clone();
            let Some(album) = album else { return };
            let ids = {
                let conn = shared.conn.borrow();
                album_card_actions::album_track_ids(&conn, &album)
            };
            if ids.is_empty() {
                return;
            }
            match track_actions::create_playlist_and_add(&shared.conn, &album.album, &ids) {
                Ok((_id, count)) => {
                    tracing::info!(
                        name = %album.album,
                        count,
                        "album context menu: playlist created and tracks added"
                    );
                    let cb = shared.on_toast.borrow().clone();
                    if let Some(cb) = cb {
                        cb(strings::tracks_added_to_playlist_toast(count as usize, &album.album));
                    }
                }
                Err(error) => {
                    tracing::error!(%error, name = %album.album, "album context menu: failed to create playlist");
                }
            }
        });
    }
    group.add_action(&new_playlist);

    // Edit Tags — logs intent; full tag-editor wiring done in album view.
    let edit_tags = gio::SimpleAction::new(ACTION_EDIT_TAGS, None);
    {
        let shared = shared.clone();
        edit_tags.connect_activate(move |_, _| {
            let album = shared.target_album.borrow().clone();
            let Some(album) = album else { return };
            let ids = {
                let conn = shared.conn.borrow();
                album_card_actions::album_track_ids(&conn, &album)
            };
            tracing::info!(count = ids.len(), album = %album.album, "album context menu: edit tags requested");
        });
    }
    group.add_action(&edit_tags);

    // Go to Folder.
    let go_folder = gio::SimpleAction::new(ACTION_GO_TO_FOLDER, None);
    {
        let shared = shared.clone();
        go_folder.connect_activate(move |_, _| {
            let album = shared.target_album.borrow().clone();
            if let Some(album) = album {
                album_card_actions::open_folder(&album.representative_path);
            }
        });
    }
    group.add_action(&go_folder);

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
    let menu_model = {
        let conn = shared.conn.borrow();
        build_menu(&conn)
    };
    let popover = gtk4::PopoverMenu::from_model(Some(&menu_model));
    popover.set_parent(parent.upcast_ref::<gtk4::Widget>());
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
        x as i32,
        y as i32,
        1,
        1,
    )));
    popover.set_has_arrow(false);
    popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    popover.popup();
}

/// Resolves a playlist id to its display name for toast messages.
/// Falls back to `"playlist {id}"` if the lookup fails.
fn playlist_id_name(conn: &Rc<RefCell<Connection>>, playlist_id: i64) -> String {
    let conn = conn.borrow();
    playlists::list(&conn)
        .unwrap_or_default()
        .into_iter()
        .find(|p| p.id == playlist_id)
        .map_or_else(|| format!("playlist {playlist_id}"), |p| p.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_menu_has_all_sections() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let menu = build_menu(&conn);
        // 3 sections: primary (play/shuffle/queue), playlist, utility (tags/folder).
        assert_eq!(menu.n_items(), 3);
    }
}
