//! Starting a track: a fresh pipeline, the start queued on it, and a bounded
//! wait for its answer. See `Player::play`'s doc comment for the why.

use reprise_core::playback::{PlaybackError, PlaybackState, PlayerEvent};

use super::pipeline_swap::StartRecord;
use super::{start_watchdog, Player, START_ANSWER_WAIT};

impl Player {
    pub(super) fn play_resolved_uri(
        &self,
        uri: &str,
        source: &str,
        live: bool,
        gain_db: f64,
        segment: Option<(i64, i64)>,
    ) -> Result<(), PlaybackError> {
        // A manual jump invalidates every gapless/crossfade transition. This
        // applies equally to local paths and external media.
        self.reset_transition();
        self.start_watchdog.disarm();
        let record = StartRecord {
            uri: uri.to_owned(),
            live,
            gain_db,
            segment,
        };
        // Nothing is swapped if the pipeline cannot be built.
        let (fresh, watch) = self.swap.build_for(&record)?;
        let started = self.swap.begin_start(fresh, watch, record);
        if let Some(Err(error)) = started.answer.wait(START_ANSWER_WAIT) {
            tracing::warn!(%error, source, "playback failed");
            self.segments.reset();
            if let Err(error) = self.swap.replace_with_idle(Some(&started.element)) {
                tracing::warn!(%error, "no idle pipeline after a failed start");
            }
            return Err(PlaybackError::Backend(format!("GStreamer: {error}")));
        }
        // A CUE start's `StateChanged(Playing)` comes from `complete_start`
        // once the file has prerolled.
        if segment.is_none() {
            (self.on_event)(PlayerEvent::StateChanged(PlaybackState::Playing));
        }
        if start_watchdog::covers(uri, live) {
            self.start_watchdog.arm(started.generation);
        }
        Ok(())
    }
}
