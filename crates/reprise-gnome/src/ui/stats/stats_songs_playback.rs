//! Now-playing marking for the My Stats song rows.
//!
//! Split out of `stats_songs_card.rs` so the card keeps rendering rows and
//! this module keeps the single answer to "is *this* row the loaded track".
//! Every visual consequence of that answer — rank slot, title, bar, row tint —
//! is applied from one function, so no two of them can disagree about which
//! row is playing.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::playing_marker;

/// Set on the row, the title and the bar while that row is the loaded track.
const NOW_PLAYING_CLASS: &str = "now-playing";

/// The loaded track as the stats page knows it: which track, and whether it is
/// running rather than paused. `None` means nothing is loaded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct TrackMark {
    pub(super) track_id: i64,
    pub(super) playing: bool,
}

/// Folds a coarse playback-state change into the current mark. Pausing keeps
/// the track marked and only freezes it — dropping the mark on pause would
/// make a paused track indistinguishable from a stopped one. Stopping clears
/// it, which is also how an external podcast or radio session takes the marker
/// away from the music rows: the same signal the track table already acts on,
/// not a second predicate about external media.
pub(super) fn mark_for_state(
    current: Option<TrackMark>,
    state: reprise_core::playback::PlaybackState,
) -> Option<TrackMark> {
    use reprise_core::playback::PlaybackState;
    match state {
        PlaybackState::Stopped => None,
        PlaybackState::Playing | PlaybackState::Paused => current.map(|mark| TrackMark {
            playing: state == PlaybackState::Playing,
            ..mark
        }),
    }
}

/// The playback-visible widgets of one song row.
///
/// The marker takes the rank slot rather than overlaying the cover: a row
/// whose number is replaced by a moving equaliser reads as playing at a
/// glance, and the cover stays a cover. `apply` is the only writer, so the
/// four affected widgets are always in one consistent state.
pub(super) struct SongRowPlayback {
    row: gtk4::Widget,
    rank_slot: gtk4::Box,
    rank: gtk4::Label,
    title: gtk4::Widget,
    bar: gtk4::Widget,
    marker: gtk4::Box,
    track_id: i64,
    mark: Cell<Option<TrackMark>>,
}

impl SongRowPlayback {
    pub(super) fn new(
        row: &impl IsA<gtk4::Widget>,
        rank_slot: &gtk4::Box,
        rank: &gtk4::Label,
        title: &impl IsA<gtk4::Widget>,
        bar: &impl IsA<gtk4::Widget>,
        track_id: i64,
    ) -> Rc<Self> {
        let marker = playing_marker::build();
        marker.add_css_class("stats-song-marker");
        marker.set_visible(false);
        rank_slot.append(&marker);
        let playback = Rc::new(Self {
            row: row.clone().upcast(),
            rank_slot: rank_slot.clone(),
            rank: rank.clone(),
            title: title.clone().upcast(),
            bar: bar.clone().upcast(),
            marker,
            track_id,
            mark: Cell::new(None),
        });
        playback.apply();
        playback
    }

    pub(super) fn set_mark(&self, mark: Option<TrackMark>) {
        self.mark.set(mark);
        self.apply();
    }

    fn apply(&self) {
        let mark = self.mark.get();
        let loaded = mark.is_some_and(|mark| mark.track_id == self.track_id);
        let playing = loaded && mark.is_some_and(|mark| mark.playing);

        for widget in [&self.row, &self.title, &self.bar] {
            if loaded {
                widget.add_css_class(NOW_PLAYING_CLASS);
            } else {
                widget.remove_css_class(NOW_PLAYING_CLASS);
            }
        }
        if loaded {
            self.rank_slot.add_css_class(NOW_PLAYING_CLASS);
        } else {
            self.rank_slot.remove_css_class(NOW_PLAYING_CLASS);
        }

        // The number and the equaliser share one slot: exactly one of them is
        // ever visible, so the row's width never shifts when playback moves.
        playing_marker::set_playing(&self.marker, playing);
        self.marker.set_visible(loaded);
        self.rank.set_visible(!loaded);
    }
}

#[cfg(test)]
#[path = "stats_songs_playback_tests.rs"]
mod tests;
