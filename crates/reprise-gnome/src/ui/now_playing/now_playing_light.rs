//! The panel's reactive-light fan-out: one reading in, every layer out.
//!
//! Split from `now_playing.rs` to keep both under the file cap. This is the
//! single place that turns a spectrum frame into `pressure` and `swell` and
//! hands them to the cover lift, the backdrop, the shimmer and the readout —
//! having two such places is how a duplicated predicate drifts.

use super::panel_state::{PanelTab, VISUAL_PAGE};
use super::surface::NowPlayingPanel;
use crate::ui::cover_lift::Source as CoverLiftSource;
use crate::ui::swell::Swell;

impl NowPlayingPanel {
    pub(super) fn advance_swell(&self, frame_time_us: i64) {
        if frame_time_us <= 0 {
            *self.swell.borrow_mut() = Swell::default();
            self.swell_pressure.set(0.0);
            self.cover_kick.set(0.0);
            self.swell_last_frame_us.set(0);
            self.widgets.cover_lift.feed(0.0, 0.0, 0.0);
            self.widgets.cover_lift.set_frame_time(0);
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
        self.widgets
            .cover_lift
            .feed(value, self.cover_kick.get(), pressure);
        self.widgets.cover_lift.set_frame_time(frame_time_us);
        self.widgets.bloom.set_light(pressure, value);
        self.widgets.shimmer.set_light(pressure, value);
        self.widgets.shimmer.set_frame_time(frame_time_us);
        // The readout names every value the reactive light runs on.
        self.widgets.visualizer.set_swell(value);
    }

    pub(super) fn sync_visual_activity(&self) {
        self.widgets.visualizer.set_active(
            self.song_visuals_enabled.get()
                && self.widgets.column.is_visible()
                && self.widgets.session.selected.get() == PanelTab::Visual,
        );
    }

    /// Recomputes the combined pin rather than letting the reasons race each
    /// other. Any one of them holds the bloom at rest and hides the shimmer;
    /// only when all clear may the current playback state take over.
    ///
    /// Panel visibility is one of them, for the same reason
    /// `sync_visual_activity` tracks it: the panel starts closed (NPP-12), and
    /// a pinned bloom runs no tick — without this the paused breath would keep
    /// redrawing a widget nobody can see, on most installs, forever.
    pub(super) fn sync_bloom_activity(&self) {
        let visualizer_visible = self.song_visuals_enabled.get()
            && self.widgets.column.is_visible()
            && self.widgets.tab_stack.visible_child_name().as_deref() == Some(VISUAL_PAGE);
        self.widgets.cover_lift.set_source(if visualizer_visible {
            CoverLiftSource::Kick
        } else {
            CoverLiftSource::Swell
        });
        let pinned = !self.song_visuals_enabled.get()
            || !self.widgets.column.is_visible()
            || visualizer_visible;
        self.widgets.bloom.set_pinned(pinned);
        self.widgets.shimmer.set_pinned(pinned);
        if !pinned {
            self.widgets
                .bloom
                .set_playback_state(self.playback_state.get());
        }
    }
}
