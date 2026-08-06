//! The panel's reactive-light fan-out: one reading in, every layer out.
//!
//! Split from `now_playing.rs` to keep both under the file cap. This is the
//! single place that turns a spectrum frame into `pressure` and `swell` and
//! hands them to the cover lift, the backdrop, the shimmer and the readout —
//! having two such places is how a duplicated predicate drifts.

use super::panel_state::{PanelTab, UP_NEXT_PAGE};
use super::surface::NowPlayingPanel;
use crate::ui::playback::external_media::ExternalPlaybackSnapshot;
use crate::ui::swell::Swell;

impl NowPlayingPanel {
    fn carries_music_or_has_no_external_session(&self) -> bool {
        self.external_snapshot
            .borrow()
            .as_ref()
            .is_none_or(ExternalPlaybackSnapshot::carries_music)
    }

    pub(super) fn song_visuals_active_for_media(&self) -> bool {
        self.song_visuals_enabled.get() && self.carries_music_or_has_no_external_session()
    }

    pub(super) fn sync_visual_page_visibility(&self) {
        let visible = self.song_visuals_active_for_media();
        self.widgets.visual_page.set_visible(visible);
        if !visible && self.widgets.session.selected.get() == PanelTab::Visual {
            self.widgets.tab_stack.set_visible_child_name(UP_NEXT_PAGE);
        }
    }

    pub(super) fn sync_media_presence(&self) {
        let has_media =
            self.loaded_track.borrow().is_some() || self.external_snapshot.borrow().is_some();
        self.widgets.visualizer.set_has_track(has_media);
    }

    pub(super) fn advance_swell(&self, frame_time_us: i64) {
        if frame_time_us <= 0 {
            *self.swell.borrow_mut() = Swell::default();
            self.swell_pressure.set(0.0);
            self.swell_last_frame_us.set(0);
            self.widgets.cover_lift.feed(0.0, 0.0);
            self.widgets.bloom.set_light(0.0, 0.0);
            self.widgets.shimmer.set_light(0.0, 0.0);
            self.widgets.shimmer.set_frame_time(0);
            self.widgets.visualizer.set_swell(0.0);
            return;
        }

        let previous = self.swell_last_frame_us.replace(frame_time_us);
        let dt_s = if previous > 0 {
            frame_time_us.saturating_sub(previous) as f64 / 1_000_000.0
        } else {
            0.0
        };
        let pressure = self.swell_pressure.get();
        let value = {
            let mut swell = self.swell.borrow_mut();
            swell.advance(pressure, dt_s);
            if crate::ui::motion::animations_enabled() {
                swell.value()
            } else {
                swell.value_without_motion()
            }
        };
        self.widgets.cover_lift.feed(value, pressure);
        self.widgets.bloom.set_light(pressure, value);
        self.widgets.shimmer.set_light(pressure, value);
        self.widgets.shimmer.set_frame_time(frame_time_us);
        // The readout names every value the reactive light runs on.
        self.widgets.visualizer.set_swell(value);
    }

    pub(super) fn sync_visual_activity(&self) {
        self.widgets.visualizer.set_active(
            self.song_visuals_active_for_media()
                && self.widgets.column.is_visible()
                && self.widgets.session.selected.get() == PanelTab::Visual,
        );
    }

    /// Recomputes the combined pin rather than letting the reasons race each
    /// other. Either reason holds the bloom at rest and hides the shimmer;
    /// only when both clear may the current playback state take over.
    ///
    /// Panel visibility is one of them, for the same reason
    /// `sync_visual_activity` tracks it: the panel starts closed (NPP-12), and
    /// a pinned bloom runs no tick — without this the paused breath would keep
    /// redrawing a widget nobody can see, on most installs, forever.
    ///
    /// **The Visual tab is deliberately not a reason.** It used to be: the tab
    /// held the backdrop at rest, hid the turning disc and switched the cover's
    /// shadow to the beat, on the theory that two light languages in one panel
    /// fight each other. Looked at in use, the plain treatment — blurred cover,
    /// moving — was simply nicer there too, and the beat-driven shadow read as
    /// the cover twitching. The head of the panel now looks the same whichever
    /// tab is open.
    pub(super) fn sync_bloom_activity(&self) {
        let pinned = !self.song_visuals_active_for_media() || !self.widgets.column.is_visible();
        self.widgets.bloom.set_pinned(pinned);
        self.widgets.shimmer.set_pinned(pinned);
        if !pinned {
            self.widgets
                .bloom
                .set_playback_state(self.playback_state.get());
        }
    }
}
