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
//! - `should_stop_skipping`: the pure decision the guard consults.
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
use reprise_core::playback::{playback_fault_policy, PlaybackFaultNotice};
use reprise_core::queries;

fn notice_text(notice: PlaybackFaultNotice, title: &str) -> String {
    match notice {
        PlaybackFaultNotice::TrackUnavailableSkipped => {
            strings::text(strings::TRACK_UNAVAILABLE_SKIPPED)
        }
        PlaybackFaultNotice::CouldNotPlaySkipped => strings::could_not_play_toast(title),
    }
}

impl PlayerController {
    /// Diagnoses and reports a playback failure for `id` (shared by
    /// `player_controller.rs`'s `play_track_id` `Player::play` failure
    /// branch and its `apply_event`'s `PlayerEvent::Error` arm — see this
    /// module's doc comment), then always calls `skip_after_failure` to move
    /// on. Re-resolves `id`'s `TrackSummary` independently (rather than
    /// requiring callers to pass one in) so both call sites can share this
    /// one function even though only `play_track_id` already has a summary
    /// in hand — one extra small `SELECT` on the failure path is a non-issue
    /// next to never crashing.
    pub(in crate::ui) fn handle_unplayable_track(&self, id: i64) {
        let summary = {
            let conn = self.conn.borrow();
            queries::query_track_summary(&conn, id)
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
                        let conn = self.conn.borrow();
                        queries::mark_track_missing_if_current(
                            &conn,
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
    pub(in crate::ui) fn skip_after_failure(&self) {
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
            self.reset_to_stopped();
            self.show_toast(&strings::text(
                strings::PLAYBACK_STOPPED_TOO_MANY_UNPLAYABLE,
            ));
            return;
        }

        self.advance_playback(AdvanceReason::Manual);
    }
}

/// The skip-loop guard's pure decision (Stage 2 Task 5 — see this module's
/// doc comment): whether `skip_after_failure` should give up rather than
/// skip to yet another track. `true` once `consecutive_skips` has reached
/// `queue_len`, which is the fixed context plus manual candidate count
/// captured at the first failure, or when there was nothing to skip to. Pure
/// (no Queue/GTK/DB access) so it is unit-testable directly.
fn should_stop_skipping(consecutive_skips: usize, queue_len: usize) -> bool {
    queue_len == 0 || consecutive_skips >= queue_len
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
    fn missing_fault_notice_matches_fb_6_copy_exactly() {
        assert_eq!(
            notice_text(PlaybackFaultNotice::TrackUnavailableSkipped, "Ignored"),
            "Track unavailable — skipped"
        );
    }

    #[test]
    fn should_stop_skipping_table() {
        // (consecutive_skips, queue_len, expected)
        let cases = [
            (0, 0, true),  // empty queue: nothing to skip to, stop immediately
            (1, 0, true),  // empty queue always stops, regardless of skips
            (0, 3, false), // no skips yet: keep going
            (1, 3, false), // fewer skips than the queue is long: keep going
            (2, 3, false), // still fewer than queue_len: keep going
            (3, 3, true),  // skips == queue_len: bounded, stop
            (4, 3, true),  // skips > queue_len: definitely stop
            (1, 1, true),  // single-track queue: one skip already exhausts it
        ];
        for (skips, queue_len, expected) in cases {
            assert_eq!(
                should_stop_skipping(skips, queue_len),
                expected,
                "should_stop_skipping({skips}, {queue_len}) should be {expected}"
            );
        }
    }

    #[test]
    fn failure_limit_stays_fixed_while_pending_tracks_are_consumed() {
        assert_eq!(failure_limit(0, 4, 2, false), 6);
        assert_eq!(failure_limit(6, 4, 1, true), 6);
        assert_eq!(failure_limit(6, 4, 0, false), 6);
    }
}
