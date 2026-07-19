//! NAV-5 capture/restore wiring at the Tracks/Albums/Artists stack boundary.

use std::cell::RefCell;
use std::rc::Rc;

use super::library_shell::{
    LibraryViews, LIBRARY_VIEW_ALBUMS, LIBRARY_VIEW_ARTISTS, LIBRARY_VIEW_TRACKS,
};
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
    let track_list = Rc::downgrade(track_list);
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
                if let Some(track_list) = track_list.upgrade() {
                    track_list.remember_current_view_state();
                }
            }
            LIBRARY_VIEW_ALBUMS => remember_album(),
            LIBRARY_VIEW_ARTISTS => remember_artist(),
            _ => {}
        }
        match current.as_str() {
            LIBRARY_VIEW_TRACKS => {
                if let Some(track_list) = track_list.upgrade() {
                    track_list.restore_current_view_state();
                }
            }
            LIBRARY_VIEW_ALBUMS => restore_album(),
            LIBRARY_VIEW_ARTISTS => restore_artist(),
            _ => {}
        }
    });
}
