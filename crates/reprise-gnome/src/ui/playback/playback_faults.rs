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
//! - `handle_unplayable_track`: starts the diagnosis of a playback failure for
//!   a track id (file missing vs. file exists but won't play) and hands off to
//!   `skip_after_failure` without waiting for it.
//! - `probe_fault` / `apply_probe_answer`: the diagnosis and its report (mark +
//!   toast, or toast-only), see "Where the work runs" below.
//! - `skip_after_failure`: the one shared skip-loop guard — advances pending
//!   Up Next and the playback context, or gives up at their fixed combined
//!   bound.
//! - the fixed-bound skip loop, consulting Core's `should_stop_skipping` rule.
//!
//! ## Where the work runs
//!
//! A track on a dead hard NFS or SMB mount blocks every call that touches its
//! path in the kernel, so the main thread never does:
//!
//! - **Main thread:** the `query_track_summary` read of the local database, the
//!   skip (`skip_after_failure`), the overdue timer, applying the answer
//!   (`reload_track_list`, the toast).
//! - **`gio::spawn_blocking` worker:** `is_file()` and
//!   `mark_track_missing_if_current`, together, in one job over owned values
//!   and a database handle the worker opens itself (`playback_fault_probe.rs`,
//!   injectable for tests).
//!
//! ## One toast per fault
//!
//! The skip does not wait for the probe, so the toast follows its answer, with
//! today's wording. If the probe has not answered within [`FAULT_PROBE_WAIT`],
//! the neutral "could not play" toast is shown at that moment instead, and the
//! answer, when it arrives, only marks and reloads: whichever comes first sets
//! the shared flag, so a fault never shows two toasts. A probe that never
//! returns leaves the track unmarked and holds one worker thread for as long as
//! the kernel call lasts, but freezes nothing. A fault whose row is already gone
//! starts no probe and shows no toast.
//!
//! The run's closing toasts come last: the skip-limit stop and the summary of
//! skipped unplayable episodes. The faults of a run are skipped at once, so
//! these can be due while the fault toasts are still pending; they are held
//! back, in order, until every fault toast has been shown
//! (`pending_fault_toasts`, `deferred_toasts`), at most [`FAULT_PROBE_WAIT`]
//! later.
//!
//! ## When the skip count resets
//!
//! `Player::play` returns `Ok` before an asynchronous fault (a start that
//! never leaves READY, a sink that never opens) has had its say, so a start is
//! no evidence that anything works. The count therefore survives the starts the
//! skip loop makes itself (`failure_advance_in_flight`) and resets only when a
//! position tick shows the playhead moving (`note_playback_progress`), when the
//! user starts something (`play_track_id`), or when the run ends. Otherwise a
//! wedged sink would skip a repeating queue for ever, one fault per start.
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

use gtk4::{gio, glib};

use crate::ui::player_controller::PlayerController;
use crate::ui::strings;
use crate::ui::up_next_transport::AdvanceReason;
use reprise_core::playback::{playback_fault_policy, should_stop_skipping, PlaybackFaultNotice};
use reprise_core::queries;

use super::playback_fault_probe::{FaultProbeAnswer, FaultProbeRequest};

#[cfg(test)]
thread_local! {
    static RECORDED_FAULT_TOASTS: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Takes every toast the fault path has shown on this thread since the last
/// call. Test threads are one per test, so tests never see each other's.
#[cfg(test)]
pub(super) fn recorded_fault_toasts() -> Vec<String> {
    RECORDED_FAULT_TOASTS.with(|toasts| std::mem::take(&mut *toasts.borrow_mut()))
}

/// How long the handler waits for the probe before it shows the neutral
/// "could not play" toast itself.
pub(in crate::ui) const FAULT_PROBE_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

/// How many probes may be in flight at once. Each holds a thread of GLib's
/// shared task pool for as long as its kernel call blocks, and covers and the
/// other background work share that pool, so a dead mount must not be able to
/// fill it. A fault past the bound starts no probe and gets the neutral toast.
pub(in crate::ui) const MAX_FAULT_PROBES_IN_FLIGHT: usize = 2;

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
            self.show_after_pending_fault_toasts(strings::skipped_unplayable_episodes(count));
        }
    }

    /// Diagnoses and reports a playback failure for `id` (shared by
    /// `player_controller.rs`'s `play_track_id` `Player::play` failure
    /// branch and its `apply_event`'s `PlayerEvent::Error` arm — see this
    /// module's doc comment) and always calls `skip_after_failure` to move
    /// on at once, without waiting for the file probe (`probe_fault`). Re-resolves `id`'s `TrackSummary` independently (rather than
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
            Ok(Some(summary)) => self.probe_fault(id, summary.path, summary.title),
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

    /// Starts the probe of a faulted track's file on a blocking worker and
    /// arranges the one toast this fault is allowed. Whichever comes first
    /// shows it: the probe's answer (today's wording) or, once the probe is
    /// `FAULT_PROBE_WAIT` overdue, the neutral "could not play" toast. The
    /// other then only does the probe's non-toast work.
    ///
    /// Nothing here waits: the callers skip to the next track right after.
    /// Both callbacks hold only a `Weak`, so a late answer after the controller
    /// is gone does nothing.
    fn probe_fault(self: &std::rc::Rc<Self>, id: i64, path: String, title: String) {
        let probe = self.fault_probe.borrow().clone();
        let request = FaultProbeRequest {
            track_id: id,
            path: std::path::PathBuf::from(&path),
            db_path: self.conn.path(),
        };
        let toast_pending = std::rc::Rc::new(std::cell::Cell::new(true));
        self.pending_fault_toasts
            .set(self.pending_fault_toasts.get() + 1);

        let overdue = {
            let (this, toast_pending, title) = (
                std::rc::Rc::downgrade(self),
                toast_pending.clone(),
                title.clone(),
            );
            move || {
                let Some(this) = this.upgrade() else { return };
                if toast_pending.replace(false) {
                    tracing::warn!(
                        track_id = id,
                        "the file probe has not answered; showing the neutral toast"
                    );
                    this.settle_fault_toast(&strings::could_not_play_toast(&title));
                }
            }
        };
        glib::timeout_add_local_once(FAULT_PROBE_WAIT, overdue);

        // A fault past the bound, or for a track already being probed, starts
        // no job: the timer above shows its neutral toast.
        if !self.claim_probe_slot(id) {
            tracing::warn!(
                track_id = id,
                "no probe slot free or the track is already being probed; the neutral toast follows"
            );
            return;
        }
        let this = std::rc::Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let answer = gio::spawn_blocking(move || probe(request)).await;
            let Some(this) = this.upgrade() else { return };
            // Returned or panicked: either way the worker thread is free again.
            this.release_probe_slot(id);
            match answer {
                Ok(answer) => {
                    let toast_due = toast_pending.replace(false);
                    this.apply_probe_answer(id, &path, &title, answer, toast_due);
                }
                // The timer above still shows the neutral toast.
                Err(_) => tracing::error!(track_id = id, "the file probe panicked"),
            }
        });
    }

    /// Takes a probe slot for `id`; false when [`MAX_FAULT_PROBES_IN_FLIGHT`]
    /// probes are out or `id` already has one. The set is only touched in its
    /// own statements, so no borrow is alive across a GTK call.
    fn claim_probe_slot(&self, id: i64) -> bool {
        let mut in_flight = self.probes_in_flight.borrow_mut();
        if in_flight.len() >= MAX_FAULT_PROBES_IN_FLIGHT {
            return false;
        }
        in_flight.insert(id)
    }

    fn release_probe_slot(&self, id: i64) {
        self.probes_in_flight.borrow_mut().remove(&id);
    }

    fn apply_probe_answer(
        &self,
        id: i64,
        path: &str,
        title: &str,
        answer: FaultProbeAnswer,
        toast_due: bool,
    ) {
        let policy = playback_fault_policy(answer == FaultProbeAnswer::FilePresent);
        if policy.mark_missing {
            tracing::error!(
                track_id = id,
                path,
                title,
                diagnostic = strings::file_missing_toast(title),
                "file no longer exists on disk; marked missing and skipped"
            );
        } else {
            tracing::error!(
                track_id = id,
                path,
                title,
                "playback failed for a file that still exists; skipping"
            );
        }
        if answer == (FaultProbeAnswer::FileMissing { marked: true }) {
            self.reload_track_list();
        }
        if toast_due {
            self.settle_fault_toast(&notice_text(policy.notices[0], title));
        }
    }

    /// Shows a fault's one toast and, when it was the last one pending, the
    /// closing toasts that were waiting behind it. They leave their cell in
    /// their own statement, so no borrow is alive when a toast reaches GTK.
    fn settle_fault_toast(&self, text: &str) {
        self.show_fault_toast(text);
        let pending = self.pending_fault_toasts.get().saturating_sub(1);
        self.pending_fault_toasts.set(pending);
        if pending > 0 {
            return;
        }
        let deferred = std::mem::take(&mut *self.deferred_toasts.borrow_mut());
        for text in deferred {
            self.show_fault_toast(&text);
        }
    }

    /// A toast that closes a run (the skip-limit stop, the skipped-episodes
    /// summary): now when no fault toast is pending, else after the last of
    /// them (`settle_fault_toast`), so it is never overtaken by one.
    fn show_after_pending_fault_toasts(&self, text: String) {
        if self.pending_fault_toasts.get() == 0 {
            self.show_fault_toast(&text);
        } else {
            self.deferred_toasts.borrow_mut().push(text);
        }
    }

    /// [`Self::show_toast`] for the toasts of the fault path. A display test
    /// reads what was said from [`recorded_fault_toasts`]: a toast overlay
    /// cannot be asked which toasts it has shown.
    fn show_fault_toast(&self, text: &str) {
        #[cfg(test)]
        RECORDED_FAULT_TOASTS.with(|toasts| toasts.borrow_mut().push(text.to_owned()));
        self.show_toast(text);
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
                self.show_after_pending_fault_toasts(strings::skipped_unplayable_episodes(
                    episode_skips,
                ));
            } else {
                self.show_after_pending_fault_toasts(strings::text(
                    strings::PLAYBACK_STOPPED_TOO_MANY_UNPLAYABLE,
                ));
            }
            return;
        }

        let outer = self.failure_advance_in_flight.replace(true);
        self.advance_playback(AdvanceReason::Manual);
        self.failure_advance_in_flight.set(outer);
    }

    /// Playback demonstrably moved (a position tick past zero): whatever
    /// skipping led here is over, so the next fault starts a fresh run. Until
    /// then every asynchronous fault counts towards `should_stop_skipping` —
    /// otherwise a sink that stalls every start would skip a repeating queue
    /// for ever.
    pub(in crate::ui) fn note_playback_progress(&self, position_ms: i64) {
        if position_ms > 0 {
            self.consecutive_skips.set(0);
            self.failure_skip_limit.set(0);
        }
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
