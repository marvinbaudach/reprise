//! Recognising the successor whose hand-off is already under way.
//!
//! Split out of `player.rs` to keep it under the file-size cap; it reaches
//! into `Player`'s own fields, which a child module may.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::PoisonError;

use crate::gapless::QueuedTrack;
use crate::player_effects::set_playbin_track_gain;

use super::Player;

impl Player {
    /// A re-fed `next` that names the track whose hand-off is already under way
    /// (a live ReplayGain change re-feeds the unchanged next track) must update
    /// the gain that hand-off will apply: the gapless gain pending for the next
    /// stream start, the gain of the pre-built crossfade secondary, or — for a
    /// CUE track the boundary probe already handed over to, heard or not yet —
    /// the gain playing now. Returns `true` when it did, so the track is not queued again.
    ///
    /// A track is identified by URI *and* segment: two tracks of one CUE file
    /// share the URI, and a whole-file hand-off is never one of them.
    pub(super) fn refresh_in_flight_gain(&self, queued: &QueuedTrack) -> bool {
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(segment) = queued.segment {
            if !self
                .segments
                .refresh_in_flight_gain(&queued.uri, segment, queued.gain_db)
            {
                return false;
            }
            if let Err(error) = set_playbin_track_gain(&playbin, queued.gain_db) {
                tracing::warn!(%error, "could not refresh the handed-over CUE track's gain");
            }
            return true;
        }
        {
            let mut pending = self
                .pending_gain
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if pending.is_some() && playbin_uri(&playbin).as_deref() == Some(&queued.uri) {
                *pending = Some(queued.gain_db);
                return true;
            }
        }
        let incoming = self
            .incoming
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let Some(secondary) = incoming else {
            return false;
        };
        if playbin_uri(&secondary).as_deref() != Some(&queued.uri) {
            return false;
        }
        if let Err(error) = set_playbin_track_gain(&secondary, queued.gain_db) {
            tracing::warn!(%error, "could not refresh the crossfade secondary's gain");
        }
        true
    }
}

fn playbin_uri(playbin: &gst::Element) -> Option<String> {
    playbin.property::<Option<String>>("uri")
}
