//! NAV-5 capture/restore wiring at the Tracks/Albums/Artists stack boundary.

use std::cell::RefCell;
use std::rc::Rc;

use super::library_shell::{
    LibraryViews, LIBRARY_VIEW_ALBUMS, LIBRARY_VIEW_ARTISTS, LIBRARY_VIEW_TRACKS,
};
use super::navigation_context::{ContextAnchorPolicy, LibraryMode};
use crate::ui::album_view::AlbumView;
use crate::ui::artist_view::ArtistView;
use crate::ui::track_list::TrackList;

pub(in crate::ui) fn wire(
    views: &LibraryViews,
    album_view: &AlbumView,
    artist_view: &ArtistView,
    track_list: &Rc<TrackList>,
) {
    let previous = Rc::new(RefCell::new(LIBRARY_VIEW_TRACKS.to_owned()));
    let remember_album = album_view.remember_view_state_callback();
    let restore_album = album_view.restore_view_state_callback();
    let remember_artist = artist_view.remember_view_state_callback();
    let restore_artist = artist_view.restore_view_state_callback();
    let reveal_album = album_view.reveal_playing_context_callback();
    let reveal_artist = artist_view.reveal_playing_context_callback();
    let policy = Rc::new(RefCell::new(ContextAnchorPolicy::default()));
    let track_list = Rc::downgrade(track_list);
    let track_list_for_stack = track_list.clone();
    let policy_for_stack = policy.clone();
    let reveal_album_for_stack = reveal_album.clone();
    let reveal_artist_for_stack = reveal_artist.clone();
    views.stack.connect_visible_child_name_notify(move |stack| {
        let Some(current) = stack.visible_child_name().map(|name| name.to_string()) else {
            return;
        };
        let leaving = previous.replace(current.clone());
        if leaving == current {
            return;
        }
        match leaving.as_str() {
            LIBRARY_VIEW_TRACKS => {
                if let Some(track_list) = track_list_for_stack.upgrade() {
                    track_list.remember_current_view_state();
                }
            }
            LIBRARY_VIEW_ALBUMS => remember_album(),
            LIBRARY_VIEW_ARTISTS => remember_artist(),
            _ => {}
        }
        let Some(mode) = mode_for_name(&current) else {
            return;
        };
        if policy_for_stack.borrow().has_visited(mode) {
            match mode {
                LibraryMode::Tracks => {
                    if let Some(track_list) = track_list_for_stack.upgrade() {
                        track_list.restore_current_view_state();
                    }
                }
                LibraryMode::Albums => restore_album(),
                LibraryMode::Artists => restore_artist(),
            }
            return;
        }
        let revealed = reveal_mode(
            mode,
            &track_list_for_stack,
            &reveal_album_for_stack,
            &reveal_artist_for_stack,
        );
        policy_for_stack.borrow_mut().enter(mode, revealed);
    });
    let revealed = reveal_mode(
        LibraryMode::Tracks,
        &track_list,
        &reveal_album,
        &reveal_artist,
    );
    policy.borrow_mut().enter(LibraryMode::Tracks, revealed);
}

fn mode_for_name(name: &str) -> Option<LibraryMode> {
    match name {
        LIBRARY_VIEW_TRACKS => Some(LibraryMode::Tracks),
        LIBRARY_VIEW_ALBUMS => Some(LibraryMode::Albums),
        LIBRARY_VIEW_ARTISTS => Some(LibraryMode::Artists),
        _ => None,
    }
}

fn reveal_mode(
    mode: LibraryMode,
    track_list: &std::rc::Weak<TrackList>,
    reveal_album: &Rc<dyn Fn() -> bool>,
    reveal_artist: &Rc<dyn Fn() -> bool>,
) -> bool {
    match mode {
        LibraryMode::Tracks => track_list
            .upgrade()
            .is_some_and(|track_list| track_list.reveal_playing_context()),
        LibraryMode::Albums => reveal_album(),
        LibraryMode::Artists => reveal_artist(),
    }
}
