use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::library::playlists;
use reprise_core::view_source::ViewSource;

use super::sidebar_playlist_creation;
use super::{find_row, rebuild, show_toast, Shared};
use crate::ui::strings;

#[derive(Clone, Copy, Default)]
enum CommitFocus {
    #[default]
    Preserve,
    Row,
    Next,
}

pub(super) fn placeholder_name() -> String {
    strings::text(strings::NEW_PLAYLIST_UNTITLED)
}

#[cfg(test)]
pub(super) fn create_placeholder(db: &Db) -> Result<i64, rusqlite::Error> {
    playlists::create(db, &placeholder_name())
}

pub(super) fn commit_name(db: &Db, id: i64, requested: &str) -> Result<bool, rusqlite::Error> {
    let requested = requested.trim();
    if requested.is_empty() {
        return Ok(false);
    }
    playlists::rename(db, id, requested).map(|changed| changed > 0)
}

pub(super) fn discard_placeholder(db: &Db, id: i64) -> Result<bool, rusqlite::Error> {
    playlists::delete(db, id, &placeholder_name())
}

pub(in crate::ui) fn begin(shared: &Rc<Shared>) {
    let name = placeholder_name();
    sidebar_playlist_creation::create_playlist_and_stay(shared, &name);
}

pub(in crate::ui) fn wire_editor(
    shared: &Rc<Shared>,
    row: &gtk4::ListBoxRow,
    playlist_id: i64,
    editor: &gtk4::EditableLabel,
) {
    let finalized = Rc::new(Cell::new(false));
    let commit_focus = Rc::new(Cell::new(CommitFocus::Preserve));

    let keys = gtk4::EventControllerKey::new();
    {
        let shared = shared.clone();
        let finalized = finalized.clone();
        let commit_focus = commit_focus.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if matches!(key, gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter) {
                commit_focus.set(CommitFocus::Row);
                return glib::Propagation::Proceed;
            }
            if matches!(key, gtk4::gdk::Key::Tab | gtk4::gdk::Key::ISO_Left_Tab) {
                commit_focus.set(CommitFocus::Next);
                return glib::Propagation::Proceed;
            }
            if key != gtk4::gdk::Key::Escape || finalized.replace(true) {
                return glib::Propagation::Proceed;
            }
            let shared = shared.clone();
            glib::idle_add_local_once(move || discard_and_rebuild(&shared, playlist_id));
            glib::Propagation::Stop
        });
    }
    editor.add_controller(keys);

    {
        let shared = shared.clone();
        let finalized = finalized.clone();
        let commit_focus = commit_focus.clone();
        editor.connect_editing_notify(move |editor| {
            if editor.is_editing() || finalized.replace(true) {
                return;
            }
            let requested = editor.text().to_string();
            let focus = commit_focus.get();
            let shared = shared.clone();
            glib::idle_add_local_once(move || {
                commit_and_rebuild(&shared, playlist_id, &requested, focus);
            });
        });
    }

    let focus = gtk4::EventControllerFocus::new();
    {
        let shared = shared.clone();
        let editor = editor.clone();
        let finalized = finalized.clone();
        let commit_focus = commit_focus.clone();
        focus.connect_leave(move |_| {
            if finalized.replace(true) {
                return;
            }
            let requested = editor.text().to_string();
            let focus = commit_focus.get();
            let shared = shared.clone();
            glib::idle_add_local_once(move || {
                commit_and_rebuild(&shared, playlist_id, &requested, focus);
            });
        });
    }
    editor.add_controller(focus);

    let editor = editor.clone();
    let row = row.clone();
    glib::idle_add_local_once(move || {
        row.grab_focus();
        editor.start_editing();
        editor.select_region(0, -1);
        editor.grab_focus();
    });
}

fn commit_and_rebuild(shared: &Rc<Shared>, playlist_id: i64, requested: &str, focus: CommitFocus) {
    match commit_name(&shared.conn, playlist_id, requested) {
        Ok(true) => super::notify_playlists_changed(shared),
        Ok(false) => {}
        Err(error) => {
            tracing::error!(%error, playlist_id, "failed to commit inline playlist name");
            show_toast(
                shared,
                &strings::playlist_create_failed_toast(requested.trim()),
            );
        }
    }
    shared.playlist_quick_edit_id.set(None);
    rebuild(
        shared,
        Some(ViewSource::Playlist(playlist_id)),
        "playlist inline name committed",
    );
    let row = find_row(shared, &ViewSource::Playlist(playlist_id));
    match (focus, row) {
        (CommitFocus::Row, Some(row)) => {
            glib::idle_add_local_once(move || {
                row.grab_focus();
            });
        }
        (CommitFocus::Next, Some(row)) => {
            glib::idle_add_local_once(move || {
                let mut sibling = row.next_sibling();
                while let Some(widget) = sibling {
                    if widget.is_focusable() {
                        widget.grab_focus();
                        return;
                    }
                    sibling = widget.next_sibling();
                }
            });
        }
        _ => {}
    }
}

fn discard_and_rebuild(shared: &Rc<Shared>, playlist_id: i64) {
    match discard_placeholder(&shared.conn, playlist_id) {
        Ok(true) => super::notify_playlists_changed(shared),
        Ok(false) => tracing::warn!(playlist_id, "fresh playlist changed before Escape discard"),
        Err(error) => {
            tracing::error!(%error, playlist_id, "failed to discard fresh playlist");
            show_toast(
                shared,
                &strings::playlist_delete_failed_toast(&placeholder_name()),
            );
        }
    }
    shared.playlist_quick_edit_id.set(None);
    rebuild(shared, None, "playlist inline creation discarded");
    let button = shared.playlist_add_button.borrow().clone();
    if let Some(button) = button {
        glib::idle_add_local_once(move || {
            button.grab_focus();
        });
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::db::Db;
    use reprise_core::library::playlists;

    #[test]
    fn nav_14_the_playlists_header_creates_a_playlist_in_place_without_a_dialog() {
        let db = Db::open_in_memory().unwrap();

        let id = super::create_placeholder(&db).unwrap();

        let playlist = playlists::get(&db, id).unwrap().unwrap();
        assert_eq!(playlist.name, super::placeholder_name());
    }

    #[test]
    fn nav_14_escape_discards_the_new_playlist_row_and_the_playlist() {
        let db = Db::open_in_memory().unwrap();
        let id = super::create_placeholder(&db).unwrap();

        assert!(super::discard_placeholder(&db, id).unwrap());
        assert!(playlists::get(&db, id).unwrap().is_none());
    }

    #[test]
    fn nav_14_an_empty_name_keeps_the_untitled_playlist() {
        let db = Db::open_in_memory().unwrap();
        let id = super::create_placeholder(&db).unwrap();

        assert!(!super::commit_name(&db, id, "  \n").unwrap());

        let playlist = playlists::get(&db, id).unwrap().unwrap();
        assert_eq!(playlist.name, super::placeholder_name());
    }

    #[test]
    fn nav_14_a_committed_name_reports_the_playlist_mutation() {
        let db = Db::open_in_memory().unwrap();
        let id = super::create_placeholder(&db).unwrap();

        assert!(super::commit_name(&db, id, "Road").unwrap());

        assert_eq!(playlists::get(&db, id).unwrap().unwrap().name, "Road");
    }
}
