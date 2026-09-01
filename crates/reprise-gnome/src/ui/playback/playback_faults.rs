//! Fault-tolerance and auto-skip logic for `PlayerController` (Stage 2 Task
//! 5; split out of `player_controller.rs` in Stage 3 Task 1 — see that
//! module's `## Fault tolerance` and `## Toast + track-list-reload seam` doc
//! sections for the parts of the story that stayed there: the two call sites
//! that funnel into `handle_unplayable_track` below (`play_track_id`'s
//! `Player::play` failure branch and `apply_event`'s `PlayerEvent::Error`
//! arm), and the toast-overlay/track-list-reload fields and methods this
//! module calls through rather than owning itself).
//!
//! ## What lives here
//!
//! - `handle_unplayable_track`: diagnoses a playback failure for a track id
//!   (file missing vs. file exists but won't play) and reports it (mark +
//!   toast, or toast-only), then always hands off to `skip_after_failure`.
//! - `skip_after_failure`: the one shared skip-loop guard — advances pending
//!   Up Next and the playback context, or gives up at their fixed combined
//!   bound.
//! - the fixed-bound skip loop, consulting Core's `should_stop_skipping` rule.
//!
//! ## Seam: `pub(in crate::ui)`
//!
//! Sibling of `player_controller` under `ui` — same reasoning as `mpris_
//! mirror.rs`'s doc comment (read it for the full rationale). This module
//! reaches into `PlayerController`'s `conn`, `queue`, and `consecutive_skips`
//! fields (all `pub(in crate::ui)` on the struct in `player_controller.rs`) and
//! calls its `play_track_id`, `show_toast`, `reload_track_list`, and `reset_
//! to_stopped` methods (all `pub(in crate::ui)` there too). `player_controller.rs`
//! still owns every field; this module only ever borrows `&self`.
//!
//! ## Queue borrow discipline
//!
//! Same invariant as `player_controller.rs`'s `## Queue borrow discipline`
//! doc section. `skip_after_failure` reads `queue.borrow().len()` and later
//! `queue.borrow_mut().next_manual()` each inside their own `let` statement,
//! so no borrow is alive when `play_track_id`/`reset_to_stopped` — both of
//! which can synchronously trigger further `PlayerEvent`s — run afterward.

use crate::ui::player_controller::PlayerController;
use crate::ui::strings;
use crate::ui::up_next_transport::AdvanceReason;
use reprise_core::playback::{playback_fault_policy, should_stop_skipping, PlaybackFaultNotice};
use reprise_core::queries;

pub(super) fn note_episode_skip(run: &std::cell::Cell<usize>) {
    run.set(run.get().saturating_add(1));
}

pub(super) fn take_episode_skip_count(run: &std::cell::Cell<usize>) -> usize {
    run.replace(0)
}

fn notice_text(notice: PlaybackFaultNotice, title: &str) -> String {
    match notice {
        PlaybackFaultNotice::TrackUnavailableSkipped => {
            strings::text(strings::TRACK_UNAVAILABLE_SKIPPED)
        }
        PlaybackFaultNotice::CouldNotPlaySkipped => strings::could_not_play_toast(title),
    }
}

impl PlayerController {
    pub(in crate::ui) fn flush_episode_skip_toast(&self) {
        let count = take_episode_skip_count(&self.consecutive_episode_skips);
        if count > 0 {
            self.show_toast(&strings::skipped_unplayable_episodes(count));
        }
    }

    /// Diagnoses and reports a playback failure for `id` (shared by
    /// `player_controller.rs`'s `play_track_id` `Player::play` failure
    /// branch and its `apply_event`'s `PlayerEvent::Error` arm — see this
    /// module's doc comment), then always calls `skip_after_failure` to move
    /// on. Re-resolves `id`'s `TrackSummary` independently (rather than
    /// requiring callers to pass one in) so both call sites can share this
    /// one function even though only `play_track_id` already has a summary
    /// in hand — one extra small `SELECT` on the failure path is a non-issue
    /// next to never crashing.
    pub(in crate::ui) fn handle_unplayable_track(self: &std::rc::Rc<Self>, id: i64) {
        let summary = {
            let conn = &self.conn;
            queries::query_track_summary(conn, id)
        };

        match summary {
            Ok(Some(summary)) => {
                let policy = playback_fault_policy(std::path::Path::new(&summary.path).is_file());
                if !policy.mark_missing {
                    tracing::error!(
                        track_id = id,
                        path = %summary.path,
                        title = %summary.title,
                        "playback failed for a file that still exists; skipping"
                    );
                } else {
                    let diagnostic = strings::file_missing_toast(&summary.title);
                    tracing::error!(
                        track_id = id,
                        path = %summary.path,
                        title = %summary.title,
                        diagnostic,
                        "file no longer exists on disk; marking missing and skipping"
                    );
                    let mark_result = {
                        let conn = &self.conn;
                        queries::mark_track_missing_if_current(
                            conn,
                            id,
                            std::path::Path::new(&summary.path),
                        )
                    };
                    match mark_result {
                        Ok(true) => {
                            self.reload_track_list();
                        }
                        Ok(false) => tracing::info!(
                            track_id = id,
                            "stale playback fault did not mark a reconciled track missing"
                        ),
                        Err(error) => {
                            tracing::error!(%error, track_id = id, "failed to mark track missing");
                        }
                    }
                }
                self.show_toast(&notice_text(policy.notices[0], &summary.title));
                if policy.skip {
                    self.skip_after_failure();
                }
                return;
            }
            Ok(None) => {
                tracing::warn!(
                    track_id = id,
                    "playback failed and the track's row is already gone; skipping without marking"
                );
            }
            Err(error) => {
                tracing::error!(%error, track_id = id, "failed to resolve track after a playback failure");
            }
        }

        self.skip_after_failure();
    }

    /// The one shared skip-loop guard (Stage 2 Task 5 — see this module's
    /// doc comment): increments `consecutive_skips`, then either advances the
    /// next candidate, or — once `should_stop_skipping` reaches the fixed
    /// combined context/Up Next bound — gives up, toasts, and resets to
    /// stopped instead of spinning through entirely broken candidates. All
    /// queue borrows end before advancing playback.
    pub(in crate::ui) fn skip_after_failure(self: &std::rc::Rc<Self>) {
        let queue_len = failure_limit(
            self.failure_skip_limit.get(),
            self.queue.borrow().len(),
            self.up_next.borrow().len(),
            self.current_up_next.get().is_some(),
        );
        self.failure_skip_limit.set(queue_len);
        let skips = self.consecutive_skips.get() + 1;
        self.consecutive_skips.set(skips);

        if should_stop_skipping(skips, queue_len) {
            tracing::error!(
                skips,
                queue_len,
                "too many consecutive unplayable tracks; stopping playback"
            );
            self.consecutive_skips.set(0);
            self.failure_skip_limit.set(0);
            let episode_skips = take_episode_skip_count(&self.consecutive_episode_skips);
            self.reset_to_stopped();
            if episode_skips > 0 {
                self.show_toast(&strings::skipped_unplayable_episodes(episode_skips));
            } else {
                self.show_toast(&strings::text(
                    strings::PLAYBACK_STOPPED_TOO_MANY_UNPLAYABLE,
                ));
            }
            return;
        }

        self.advance_playback(AdvanceReason::Manual);
    }
}

fn failure_limit(
    existing: usize,
    context_len: usize,
    pending_len: usize,
    has_current_pending: bool,
) -> usize {
    if existing > 0 {
        existing
    } else {
        context_len + pending_len + usize::from(has_current_pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fb_6_consecutive_episode_faults_collapse_to_one_toast_count() {
        let run = std::cell::Cell::new(0);
        super::note_episode_skip(&run);
        super::note_episode_skip(&run);
        super::note_episode_skip(&run);

        assert_eq!(super::take_episode_skip_count(&run), 3);
        assert_eq!(super::take_episode_skip_count(&run), 0);
    }

    #[test]
    fn missing_fault_notice_matches_fb_6_copy_exactly() {
        assert_eq!(
            notice_text(PlaybackFaultNotice::TrackUnavailableSkipped, "Ignored"),
            "Track unavailable — skipped"
        );
    }

    #[test]
    fn failure_limit_stays_fixed_while_pending_tracks_are_consumed() {
        assert_eq!(failure_limit(0, 4, 2, false), 6);
        assert_eq!(failure_limit(6, 4, 1, true), 6);
        assert_eq!(failure_limit(6, 4, 0, false), 6);
    }
}
