//! Now-playing marking and play/pause activation for the My Stats song rows.
//!
//! Split out of `stats_songs_card.rs` so the card keeps rendering rows and this
//! module keeps the single answer to "is *this* row the loaded track, and what
//! does activating it do". Both the glyph a row shows and the callback it fires
//! are derived here from the same [`TrackMark`] — a row can therefore never
//! show a pause glyph while its click restarts the track, which is the
//! duplicate-predicate drift this project has already paid for twice.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::playing_marker;
use crate::ui::strings;

const PLAY_ICON: &str = "media-playback-start-symbolic";
const PAUSE_ICON: &str = "media-playback-pause-symbolic";

/// The loaded track as the stats page knows it: which track, and whether it is
/// running rather than paused. `None` means nothing is loaded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct TrackMark {
    pub(super) track_id: i64,
    pub(super) playing: bool,
}

/// What activating a song row's transport button does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Activation {
    /// The row is the loaded track — pause it, or resume it if paused.
    TogglePause,
    /// Any other row — start it from the stats page.
    Start,
}

/// The one predicate behind both the glyph and the click. Only the loaded
/// track toggles; everything else starts, whether or not something else is
/// currently playing.
pub(super) fn activation_for(mark: Option<TrackMark>, target: i64) -> Activation {
    match mark {
        Some(mark) if mark.track_id == target => Activation::TogglePause,
        _ => Activation::Start,
    }
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

/// The playback-visible widgets of one song row. `refresh` is the only writer:
/// every input (mark, hover, focus) lands in a `Cell` and then re-derives the
/// full visual state, so no two paths can disagree about what the row shows.
///
/// `play` is `None` for the expanded "all top tracks" list, which carries the
/// marker (NAV-10a wants every visible instance marked) but keeps its rows
/// purely navigational — no transport affordance was ever offered there.
pub(super) struct SongRowPlayback {
    row: gtk4::Box,
    marker: gtk4::Box,
    play: Option<gtk4::Button>,
    track_id: i64,
    mark: Cell<Option<TrackMark>>,
    revealed: Cell<bool>,
}

impl SongRowPlayback {
    pub(super) fn new(
        row: &gtk4::Box,
        overlay: &gtk4::Overlay,
        play: Option<gtk4::Button>,
        track_id: i64,
        mark: Option<TrackMark>,
    ) -> Rc<Self> {
        let marker = playing_marker::build();
        marker.add_css_class("stats-song-marker");
        overlay.add_overlay(&marker);
        let playback = Rc::new(Self {
            row: row.clone(),
            marker,
            play,
            track_id,
            mark: Cell::new(mark),
            revealed: Cell::new(false),
        });
        playback.refresh();
        playback
    }

    pub(super) fn set_mark(&self, mark: Option<TrackMark>) {
        self.mark.set(mark);
        self.refresh();
    }

    /// Hover or keyboard focus reached (or left) the row: the transport button
    /// is offered, and the equaliser steps aside for it because both want the
    /// same 40×40 cover. The row's persistent accent tint stays put, so the
    /// loaded row is still marked while the pointer is on it.
    fn set_revealed(&self, revealed: bool) {
        self.revealed.set(revealed);
        self.refresh();
    }

    fn refresh(&self) {
        let mark = self.mark.get();
        let loaded = mark.is_some_and(|mark| mark.track_id == self.track_id);
        let playing = loaded && mark.is_some_and(|mark| mark.playing);
        let revealed = self.revealed.get();

        if loaded {
            self.row.add_css_class("now-playing");
        } else {
            self.row.remove_css_class("now-playing");
        }

        playing_marker::set_playing(&self.marker, playing);
        self.marker.set_visible(loaded && !revealed);

        if let Some(play) = &self.play {
            play.set_visible(revealed);
            let (icon, tooltip) = if playing {
                (PAUSE_ICON, strings::STATS_PAUSE_TRACK)
            } else {
                (PLAY_ICON, strings::STATS_PLAY_TRACK)
            };
            play.set_icon_name(icon);
            play.set_tooltip_text(Some(&strings::text(tooltip)));
        }
    }
}

/// Reveals the transport button on hover and on keyboard focus, hiding the
/// equaliser underneath it for as long as it shows. Replaces the card's former
/// standalone visibility wiring so button and marker are switched by one
/// writer instead of two.
pub(super) fn install_reveal(row: &gtk4::Box, playback: &Rc<SongRowPlayback>) {
    let hovered = Rc::new(Cell::new(false));
    let focused = Rc::new(Cell::new(false));

    let motion = gtk4::EventControllerMotion::new();
    motion.connect_enter({
        let playback = playback.clone();
        let hovered = hovered.clone();
        move |_, _, _| {
            hovered.set(true);
            playback.set_revealed(true);
        }
    });
    motion.connect_leave({
        let playback = playback.clone();
        let hovered = hovered.clone();
        let focused = focused.clone();
        move |_| {
            hovered.set(false);
            playback.set_revealed(focused.get());
        }
    });
    row.add_controller(motion);

    let focus = gtk4::EventControllerFocus::new();
    focus.connect_contains_focus_notify({
        let playback = playback.clone();
        let hovered = hovered.clone();
        move |focus| {
            focused.set(focus.contains_focus());
            playback.set_revealed(focus.contains_focus() || hovered.get());
        }
    });
    row.add_controller(focus);
}

#[cfg(test)]
#[path = "stats_songs_playback_tests.rs"]
mod tests;
