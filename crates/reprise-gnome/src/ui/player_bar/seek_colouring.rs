//! The player bar's side of the seek-bar colouring: applying the stored
//! choice, and deciding when the colour-scale legend is due.
//!
//! Split out of `player_bar.rs` to keep that file under the project's
//! 800-line cap.

use reprise_core::library::settings::SeekColouring;

use super::surface::PlayerBar;

impl PlayerBar {
    /// Applies the stored colouring to the bar and to the context-menu entry
    /// that explains it.
    pub(in crate::ui) fn set_seek_colouring(&self, colouring: SeekColouring) {
        self.seek_colouring.set(colouring);
        self.waveform.set_colouring(colouring);
        let spectral = colouring == SeekColouring::Frequency;
        self.explain_action.set_enabled(spectral);
        if !spectral {
            self.legend.hide();
        }
    }

    /// Whether the one-off colour legend should be offered for this track.
    ///
    /// True at most once per track, and never while the bar is drawn in a
    /// single colour — there is no scale to explain there, and the count in
    /// settings must not be spent on it either. The caller owns that count;
    /// this only answers "is this a new track whose bar has a scale".
    pub(in crate::ui) fn colour_legend_due_for(&self, track_id: i64) -> bool {
        if self.legend_track.replace(Some(track_id)) == Some(track_id) {
            return false;
        }
        self.seek_colouring.get() == SeekColouring::Frequency
    }

    pub(in crate::ui) fn show_colour_legend(&self) {
        if self.seek_colouring.get() != SeekColouring::Frequency {
            return;
        }
        self.legend.show();
    }
}
