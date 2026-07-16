use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::models::Track;

use super::info_panel_state::PanelContext;
use super::{Shared, TrackList};

#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::ui) fn context_from_tracks(tracks: Vec<Track>) -> PanelContext {
    match tracks.len() {
        0 => PanelContext::Empty,
        1 => PanelContext::Track(tracks.into_iter().next().expect("one track")),
        count => PanelContext::Multiple(count),
    }
}

pub(in crate::ui) fn wire(shared: &Rc<Shared>) {
    let shared_weak = Rc::downgrade(shared);
    shared.selection.connect_selection_changed(move |_, _, _| {
        if let Some(shared) = shared_weak.upgrade() {
            notify(&shared);
        }
    });
}

fn selection_positions(shared: &Shared) -> Vec<u32> {
    let bitset = shared.selection.selection();
    let Some((mut iter, first)) = gtk4::BitsetIter::init_first(&bitset) else {
        return Vec::new();
    };
    let mut positions = vec![first];
    positions.extend(iter.by_ref());
    positions
}

fn current_context(shared: &Shared) -> PanelContext {
    let positions = selection_positions(shared);
    match positions.as_slice() {
        [] => PanelContext::Empty,
        [position] => shared
            .model
            .track_at(*position)
            .map_or(PanelContext::Empty, PanelContext::Track),
        positions => PanelContext::Multiple(positions.len()),
    }
}

fn notify(shared: &Rc<Shared>) {
    let context = current_context(shared);
    let callback = shared.on_selection_changed.borrow().clone();
    if let Some(callback) = callback {
        callback(context);
    }
}

impl TrackList {
    pub(in crate::ui) fn set_on_selection_changed(
        &self,
        callback: impl Fn(PanelContext) + 'static,
    ) {
        *self.shared.on_selection_changed.borrow_mut() = Some(Rc::new(callback));
        notify(&self.shared);
    }

    pub(in crate::ui) fn shared_cover_loader(&self) -> Rc<super::cover_loader::CoverLoader> {
        self.shared.cover_loader.clone()
    }

    pub(in crate::ui) fn select_for_smoke(&self, position: u32) {
        self.shared.selection.unselect_all();
        self.shared.selection.select_item(position, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::models::Track;

    fn track(id: i64) -> Track {
        Track {
            id,
            path: format!("/{id}.flac"),
            title: format!("Track {id}"),
            artist: "Artist".into(),
            album: "Album".into(),
            album_artist: "Artist".into(),
            year: None,
            track_no: None,
            genre: String::new(),
            duration_ms: 0,
            bitrate_kbps: None,
            rating: 0,
            play_count: 0,
            last_played_at: None,
            added_at: 0,
            file_mtime: 0,
            missing: false,
            file_size: 0,
            device: None,
            inode: None,
            playlist_position: None,
        }
    }

    #[test]
    fn zero_one_and_multiple_tracks_map_to_panel_context() {
        assert_eq!(context_from_tracks(Vec::new()), PanelContext::Empty);
        assert_eq!(
            context_from_tracks(vec![track(1)]),
            PanelContext::Track(track(1))
        );
        assert_eq!(
            context_from_tracks(vec![track(1), track(2)]),
            PanelContext::Multiple(2)
        );
    }
}
