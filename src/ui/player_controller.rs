//! Bridges `Player` (whose events fire on GStreamer bus-watch and ticker
//! threads) to the GTK main thread and the `PlayerBar` widgets.
//!
//! ## Event marshalling: `async-channel` + `glib::spawn_future_local`
//!
//! Every GTK widget call must happen on the main thread, but `Player::new`'s
//! callback is invoked from non-GTK threads. The bridge used here is the
//! pattern the gtk4-rs book recommends: the player callback does a
//! non-blocking `try_send` into an unbounded `async_channel`, and a single
//! future spawned on the default (main) `glib::MainContext` drains the
//! receiver and applies each event to the bar. Compared to the alternative —
//! `glib::idle_add` from the callback thread per event — this keeps one
//! long-lived drain loop with strict FIFO ordering instead of allocating a
//! GSource per event, and it makes the thread boundary explicit in the
//! types: only `PlayerEvent` (plain `Send` data) crosses it, exactly the
//! runtime-safety property the player was designed around.
//!
//! ## Lifetime
//!
//! The drain future holds only a `Weak` reference to the controller, so the
//! controller's lifetime stays tied to the window (via the track-list
//! activation closure that owns the `Rc`), not to the future. Once the
//! controller drops, the next event breaks the loop, the receiver closes,
//! and the player callback's `try_send` starts failing (logged at warn —
//! that only happens during teardown).
//!
//! ## Play tracking
//!
//! `current_track`/`max_position_ms` track, per loaded track, the furthest
//! playback position observed via `Position` events. Whenever a listening
//! session ends — the track finishes, playback switches to a different
//! track, or the player is reset to stopped after an error — an idempotent
//! `evaluate_play_tracking` call checks `library::stats::should_count_play`
//! against that high-water mark and `library::stats::record_play`s a play if
//! it crosses the 50%-listened threshold, then clears the tracked state.
//!
//! ## Queue borrow discipline
//!
//! `queue` is a `RefCell<Queue>` (Stage 2 Task 3/4). Two prior bugs in this
//! codebase — a `RefCell` borrow held across a re-entrant GTK callback in
//! `player_bar.rs`'s seek scale, and the same class of bug in
//! `ui::rating::RatingWidget` (see that module's doc comment) — were both
//! "hold a `Ref`/`RefMut` across a call that can synchronously call back
//! into the same object". `play_track_id` (the one function that actually
//! starts playback) can synchronously trigger `PlayerEvent`s and eventually
//! another `apply_event`/queue call, so **no `queue` borrow may still be
//! alive when `play_track_id`, `reset_to_stopped`, or any other
//! player/GTK-facing call runs.** Every call site here follows the same
//! shape: read the one `Option<i64>`/value it needs out of the queue in a
//! single expression (a `let x = queue.borrow_mut().method();` statement, or
//! an explicit `{ }` block when more than one queue call is needed), which
//! drops the borrow at the end of that statement/block — *before* the next
//! statement calls out. Unlike the two prior bugs, no explicit `Rc::clone`-
//! out step is needed here: `Queue`'s methods return owned `Option<i64>`/
//! `Repeat` values, not references into the `RefCell`'s contents, so the
//! borrow's temporary scope is inherently the only thing to manage.
//!
//! ## Fault tolerance: toast, missing flag, and auto-skip (Stage 2 Task 5)
//!
//! A physically deleted or otherwise unplayable queued file must never crash
//! or dead-end the app. `play_track_id`'s `Player::play` failure branch and
//! `apply_event`'s `PlayerEvent::Error` arm (which can fire asynchronously,
//! after `play_track_id` already returned `Ok`, for the *currently loaded*
//! track — GStreamer resolves most "file not found"-class errors as an async
//! bus message, not a synchronous `set_state` failure) both funnel into two
//! shared helpers instead of duplicating the "diagnose, mark/toast, skip"
//! sequence (DRY):
//!
//! - `handle_unplayable_track(id)` re-resolves `id`'s `TrackSummary` (title +
//!   path) and decides, via a cheap `std::path::Path::exists` check on the
//!   resolved path, which of the two fault classes this is: **file missing**
//!   (marks the row via `queries::mark_track_missing`, toasts
//!   `strings::file_missing_toast`, then refreshes the track list — see the
//!   reload seam below — so the row disappears from view) or **file exists
//!   but won't play** (corrupt/unsupported content: toasts
//!   `strings::could_not_play_toast`, does *not* mark anything missing). An
//!   `Ok(None)`/`Err` resolution (the row itself is already gone, or the
//!   query failed) has no title/path to show a toast with, so those cases
//!   log and skip without a toast or a redundant `mark_track_missing` call.
//! - `skip_after_failure()` is the one shared skip-loop-guard: it increments
//!   `consecutive_skips` (a `Cell<usize>`, reset to 0 on every *successful*
//!   `play_track_id` playback start — including a successful skip landing on
//!   a good track), then consults the pure `should_stop_skipping` (bounded by
//!   the queue's own length, so a queue of N broken tracks can chain-skip at
//!   most N times before giving up) to decide between `next_manual()` →
//!   `play_track_id` (hoisted borrow, same discipline as every other queue
//!   call site above) and `reset_to_stopped` + the "too many unplayable"
//!   toast.
//!
//! ### Toast + track-list-reload seam
//!
//! Both require reaching widgets the controller doesn't own outright:
//!
//! - `toast_overlay: glib::WeakRef<adw::ToastOverlay>` — the overlay is built
//!   in `window::build` *after* `PlayerController::new` (it wraps the whole
//!   window, including the player bar this controller owns), so it can't be
//!   a constructor parameter; `set_toast_overlay` injects it once
//!   `window::build` has it. A `WeakRef`, not a strong reference, so the
//!   controller can never keep the overlay (and thus the window) alive past
//!   its natural lifetime; `show_toast` degrades to a log line if the
//!   upgrade ever fails (e.g. very late shutdown) rather than panicking or
//!   silently dropping the toast's *reason* from the logs.
//! - `reload_track_list: RefCell<Option<Rc<dyn Fn()>>>` — similarly injected
//!   post-construction via `set_track_list_reload`, since `track_list.rs`'s
//!   `TrackList` is also built after the controller (its `on_activate`
//!   closure needs `Rc<PlayerController>` to already exist). `window::build`
//!   supplies a closure over a `Weak<TrackList>` (never a strong `Rc`): a
//!   strong reference back from the controller to the track list would be an
//!   `Rc` cycle with `TrackList`'s own `Shared.on_activate`, which already
//!   holds a strong `Rc<PlayerController>` — neither side would ever free.
//!   Stored as `Rc<dyn Fn()>` rather than `Box<dyn Fn()>` specifically so
//!   `reload_track_list()` can clone it out of the `RefCell` in one `let`
//!   statement before calling it — the same hoist-before-calling-out shape
//!   the queue borrows above use, even though (unlike `queue`) nothing here
//!   can currently call back into this particular `RefCell` re-entrantly.
//!
//! ## MPRIS (Stage 2 Task 6)
//!
//! `mpris::start()` is called once, right after the controller's own `Rc`
//! exists (see `new`), spawning `mpris.rs`'s dedicated D-Bus thread and
//! handing back two things this struct holds for the rest of its life:
//! `mpris_state` (`Arc<Mutex<mpris::MprisState>>`, written here, read by
//! that thread — see `update_mpris_mirror`) and an `async_channel::
//! Receiver<MprisCommand>`, drained by a second `glib::spawn_future_local`
//! loop exactly parallel to the `PlayerEvent` drain loop already in `new`
//! (same `Weak`-controller-upgrade-or-break shape). `update_mpris_mirror` is
//! the one place that writes `mpris_state`: called from `play_track_id`
//! (a new track just started), `reset_to_stopped` (playback ended), and
//! `apply_event`'s `StateChanged` arm (status actually changed) — every real
//! transition, without needing a call in `TrackFinished` too, since that arm
//! only ever delegates to `play_track_id`/`reset_to_stopped`, which already
//! cover it. `now_playing` is a small `RefCell` cache of the currently-
//! loaded track's title/artist/album/duration (`current_track` already
//! tracks id/duration but only as a play-tracking high-water-mark key, not
//! for display) — set alongside `current_track` in `play_track_id`, cleared
//! alongside `bar.clear_track()` wherever that already runs.
//!
//! Transport methods `toggle_pause`/`next`/`previous` are shared, named
//! methods — not inlined in the bar's button closures the way they used to
//! be — specifically so both `wire_bar_controls` and `handle_mpris_command`
//! call the same code (DRY): a physical media key and the on-screen button
//! must behave identically. MPRIS's `Play`/`Pause` are *not* the same as
//! `toggle_pause`, though (`PlayPause` is): the MPRIS spec has `Play`
//! start-or-resume and `Pause` pause, each a no-op if already in that state,
//! whereas the bar only ever has one button that alternates — see
//! `mpris_play`/`mpris_pause`, which consult `mpris_state`'s own `status`
//! (already kept current by `update_mpris_mirror`) to decide whether
//! `toggle_pause`/`play_track_id` actually apply, rather than adding a new
//! `Player` query method just for this.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use libadwaita as adw;
use rusqlite::Connection;

use crate::library::stats;
use crate::mpris::{self, MprisCommand, MprisPlaybackStatus, MprisState, SharedMprisState};
use crate::player::{PlaybackState, Player, PlayerError, PlayerEvent};
use crate::queries;
use crate::queue::{Queue, Repeat};
use crate::ui::player_bar::PlayerBar;
use crate::ui::strings;

/// Dev/verification hook (permanent, like `REPRISE_SCAN_DIR`/`REPRISE_
/// SMOKE_QUIT`/`REPRISE_SMOKE_ACTIVATE`): when set to `"all"`, forces
/// `Repeat::All` right after the controller (and its queue) are built, so a
/// headless E2E can observe auto-advance wrapping from the last track back
/// to the first without a human toggling the repeat button.
///
/// Usage: `REPRISE_SMOKE_REPEAT=all REPRISE_SCAN_DIR=… REPRISE_SMOKE_ACTIVATE=1
///  REPRISE_AUDIO_SINK=fakesink REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`.
const SMOKE_REPEAT_ENV_VAR: &str = "REPRISE_SMOKE_REPEAT";
const SMOKE_REPEAT_ALL_VALUE: &str = "all";

/// Owns the `Player` and its `PlayerBar`, routing user input from the bar to
/// the player and `PlayerEvent`s from the player back onto the bar (on the
/// GTK main thread — see the module doc comment). Also owns play-count
/// tracking (see the module's `## Play tracking` section below).
pub struct PlayerController {
    player: Player,
    bar: PlayerBar,
    /// The UI-owned database connection, shared with `track_list.rs` (see
    /// `window::build`) — used only to write play-count updates via
    /// `library::stats::record_play`.
    conn: Rc<RefCell<Connection>>,
    /// `(track_id, duration_ms)` of the track currently loaded, set by
    /// `play_track_id` and cleared once play tracking has been evaluated for
    /// it (see `evaluate_play_tracking`). `None` when no track is loaded.
    current_track: Cell<Option<(i64, i64)>>,
    /// The highest playback position observed for `current_track` via
    /// `Position` events — not the most recent one, so seeking backward
    /// near the end of a track can't cost a listener credit for having
    /// already passed the 50% mark. Reset to 0 whenever a new track starts.
    max_position_ms: Cell<i64>,
    /// The playback queue (Stage 2 Task 3/4): track order, shuffle, and
    /// repeat mode. `play_from_view` seeds it; `TrackFinished`/the
    /// previous/next buttons step through it. See the module's `## Queue
    /// borrow discipline` doc section for the rule every call site here
    /// follows.
    queue: RefCell<Queue>,
    /// See the module's `## Toast + track-list-reload seam` doc section.
    /// Empty (`WeakRef::new()`) until `set_toast_overlay` is called.
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    /// See the module's `## Toast + track-list-reload seam` doc section.
    /// `None` until `set_track_list_reload` is called.
    reload_track_list: RefCell<Option<Rc<dyn Fn()>>>,
    /// How many *consecutive* auto-skips (Stage 2 Task 5) have happened since
    /// the last successful playback start. Reset to 0 in `play_track_id` on
    /// every `Player::play` success; incremented by `skip_after_failure`,
    /// which consults `should_stop_skipping` against this value and the
    /// queue's length to bound the skip chain. See the module's `## Fault
    /// tolerance` doc section.
    consecutive_skips: Cell<usize>,
    /// Shared with `mpris.rs`'s D-Bus thread — see the module's `## MPRIS`
    /// doc section. Written by `update_mpris_mirror`, never read directly
    /// here (the MPRIS thread is the only reader).
    mpris_state: SharedMprisState,
    /// Title/artist/album/duration of the currently-loaded track, for
    /// `update_mpris_mirror` to build `mpris::MprisState`'s `Metadata`
    /// fields from — see the module's `## MPRIS` doc section for why this
    /// duplicates `current_track`'s id/duration rather than reusing it.
    now_playing: RefCell<Option<NowPlaying>>,
}

/// See `PlayerController::now_playing`'s doc comment.
#[derive(Debug, Clone)]
struct NowPlaying {
    id: i64,
    title: String,
    artist: String,
    album: String,
    duration_ms: i64,
}

impl PlayerController {
    /// Builds the player, the bar, and the event bridge between them.
    /// `conn` is the same UI-owned database connection `track_list.rs`
    /// holds, used to record plays. Returns `Err` if GStreamer is
    /// unavailable (no playbin, bad `REPRISE_AUDIO_SINK` override, …) — the
    /// caller decides how to degrade (see `window::build`: library browsing
    /// keeps working without a bar).
    pub fn new(conn: Rc<RefCell<Connection>>) -> Result<Rc<Self>, PlayerError> {
        let (sender, receiver) = async_channel::unbounded::<PlayerEvent>();

        let player = Player::new(Box::new(move |event| {
            if let Err(error) = sender.try_send(event) {
                // Unbounded channel: try_send only fails once the receiver
                // (the drain loop below) is gone, i.e. during app teardown.
                tracing::warn!(%error, "player event dropped: UI receiver is gone");
            }
        }))?;

        // Stage 2 Task 6: `mpris::start()` never fails outright (see its own
        // doc comment's `## Failure is never fatal` section) — it always
        // hands back a working mirror + command receiver, spawning the
        // actual D-Bus thread in the background. Started here (right before
        // the fields it feeds), not in `window::build`, since nothing
        // outside this controller needs either handle — see the module's
        // `## MPRIS` doc section.
        let (mpris_state, mpris_receiver) = mpris::start();

        let controller = Rc::new(Self {
            player,
            bar: PlayerBar::new(),
            conn,
            current_track: Cell::new(None),
            max_position_ms: Cell::new(0),
            queue: RefCell::new(Queue::new()),
            toast_overlay: glib::WeakRef::new(),
            reload_track_list: RefCell::new(None),
            consecutive_skips: Cell::new(0),
            mpris_state,
            now_playing: RefCell::new(None),
        });

        wire_bar_controls(&controller);
        arm_smoke_repeat(&controller);

        let weak = Rc::downgrade(&controller);
        glib::spawn_future_local(async move {
            while let Ok(event) = receiver.recv().await {
                let Some(controller) = weak.upgrade() else {
                    break;
                };
                controller.apply_event(event);
            }
        });

        // Mirrors the drain loop above exactly (see the module's `## MPRIS`
        // doc section): a `Weak` controller reference, broken FIFO drain of
        // one `async_channel::Receiver`, one event/command applied per
        // iteration.
        let weak = Rc::downgrade(&controller);
        glib::spawn_future_local(async move {
            while let Ok(command) = mpris_receiver.recv().await {
                let Some(controller) = weak.upgrade() else {
                    break;
                };
                controller.handle_mpris_command(command);
            }
        });

        Ok(controller)
    }

    /// The bottom-bar widget for `ToolbarView::add_bottom_bar`.
    pub fn bar_widget(&self) -> &gtk4::ActionBar {
        self.bar.widget()
    }

    /// Injects the window's toast overlay, once it exists (see the module's
    /// `## Toast + track-list-reload seam` doc section for why this can't be
    /// a constructor parameter). Stored as a `WeakRef` — `show_toast`
    /// degrades to log-only if the upgrade ever fails.
    pub fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.toast_overlay.set(Some(overlay));
    }

    /// Injects the callback that refreshes the track list after a track is
    /// marked missing (see `handle_unplayable_track`), once the track list
    /// exists. `window::build` supplies a closure over a `Weak<TrackList>`,
    /// not a strong `Rc` — see the module's `## Toast + track-list-reload
    /// seam` doc section for why a strong reference here would leak.
    pub fn set_track_list_reload(&self, reload: impl Fn() + 'static) {
        *self.reload_track_list.borrow_mut() = Some(Rc::new(reload));
    }

    /// Starts playback of `ids[start_index]` and loads the rest of `ids` into
    /// the queue as what auto-advance/previous/next step through. Row
    /// activation lands here — see `ui::track_list`'s `queue_ids_for_
    /// activation` for how `ids`/`start_index` are built from the currently
    /// visible sort/filter view. An empty `ids` (nothing to play) resets to
    /// stopped instead of calling `play_track_id`.
    ///
    /// Borrow discipline: `set_tracks` and `current()` each run inside their
    /// own statement, so their `queue` borrows drop before `play_track_id`/
    /// `reset_to_stopped` run — see the module's `## Queue borrow
    /// discipline` doc section.
    pub fn play_from_view(&self, ids: Vec<i64>, start_index: usize) {
        self.queue.borrow_mut().set_tracks(ids, start_index);

        let queue_len = self.queue.borrow().len();
        tracing::info!(queue_len, start_index, "queue set from view");

        let queue_is_empty = self.queue.borrow().is_empty();
        self.bar.set_transport_enabled(!queue_is_empty);

        let current = self.queue.borrow().current();
        match current {
            Some(id) => self.play_track_id(id),
            None => self.reset_to_stopped(),
        }
    }

    /// Resolves `id` via `queries::query_track_summary` and starts its
    /// playback — the one place that actually calls `Player::play`, shared
    /// by `play_from_view` and every queue-stepping call site
    /// (`TrackFinished`'s auto-advance, the previous/next buttons,
    /// `skip_after_failure`'s auto-skip) so the "resolve, evaluate prior play
    /// tracking, start playback, handle failure" sequence exists exactly once
    /// (DRY). Ends the previous track's listening session first
    /// (`evaluate_play_tracking`, same as the old per-`Track` `play_track`
    /// did) — a queue step is still a track switch. On success, resets
    /// `consecutive_skips` to 0 (Stage 2 Task 5: a good track breaks any
    /// skip chain in progress). On a `Player::play` failure, hands off to
    /// `handle_unplayable_track` (diagnose missing-vs-corrupt, mark/toast,
    /// then auto-skip via `skip_after_failure`) rather than resetting
    /// outright — see the module's `## Fault tolerance` doc section. A
    /// missing DB row or a query failure has no title/path to build a toast
    /// from, so those two cases just log and go straight to
    /// `skip_after_failure` (still counted against the skip-loop guard, so a
    /// queue of entirely-vanished rows can't spin forever either).
    fn play_track_id(&self, id: i64) {
        self.evaluate_play_tracking();

        let summary = {
            let conn = self.conn.borrow();
            queries::query_track_summary(&conn, id)
        };

        match summary {
            Ok(Some(summary)) => {
                self.current_track.set(Some((id, summary.duration_ms)));
                self.max_position_ms.set(0);
                *self.now_playing.borrow_mut() = Some(NowPlaying {
                    id,
                    title: summary.title.clone(),
                    artist: summary.artist.clone(),
                    album: summary.album.clone(),
                    duration_ms: summary.duration_ms,
                });

                self.bar.set_track(&summary.title, &summary.artist);
                match self.player.play(&summary.path) {
                    Ok(()) => {
                        self.consecutive_skips.set(0);
                        // Stage 2 Task 6: reflect the new track's metadata
                        // immediately rather than waiting for the
                        // `StateChanged(Playing)` event `play()` also just
                        // enqueued to drain asynchronously (see the module's
                        // `## MPRIS` doc section) — that event's own arrival
                        // still triggers a second, idempotent mirror update.
                        self.update_mpris_mirror(MprisPlaybackStatus::Playing);
                    }
                    Err(error) => {
                        tracing::error!(%error, path = %summary.path, track_id = id, "failed to start playback");
                        self.handle_unplayable_track(id);
                    }
                }
            }
            Ok(None) => {
                tracing::warn!(
                    track_id = id,
                    "queue: track id has no matching database row; skipping playback"
                );
                self.skip_after_failure();
            }
            Err(error) => {
                tracing::error!(%error, track_id = id, "failed to resolve track for playback");
                self.skip_after_failure();
            }
        }
    }

    /// Diagnoses and reports a playback failure for `id` (shared by
    /// `play_track_id`'s `Player::play` failure branch and `apply_event`'s
    /// `PlayerEvent::Error` arm — see the module's `## Fault tolerance` doc
    /// section), then always calls `skip_after_failure` to move on. Re-
    /// resolves `id`'s `TrackSummary` independently (rather than requiring
    /// callers to pass one in) so both call sites can share this one
    /// function even though only `play_track_id` already has a summary in
    /// hand — one extra small `SELECT` on the failure path is a non-issue
    /// next to never crashing.
    fn handle_unplayable_track(&self, id: i64) {
        let summary = {
            let conn = self.conn.borrow();
            queries::query_track_summary(&conn, id)
        };

        match summary {
            Ok(Some(summary)) => {
                if std::path::Path::new(&summary.path).exists() {
                    tracing::error!(
                        track_id = id,
                        path = %summary.path,
                        title = %summary.title,
                        "playback failed for a file that still exists; skipping"
                    );
                    self.show_toast(&strings::could_not_play_toast(&summary.title));
                } else {
                    tracing::error!(
                        track_id = id,
                        path = %summary.path,
                        title = %summary.title,
                        "file no longer exists on disk; marking missing and skipping"
                    );
                    let mark_result = {
                        let conn = self.conn.borrow();
                        queries::mark_track_missing(&conn, id)
                    };
                    match mark_result {
                        Ok(()) => {
                            self.show_toast(&strings::file_missing_toast(&summary.title));
                            self.reload_track_list();
                        }
                        Err(error) => {
                            // The row still shows in the list, but playback
                            // already failed and is about to be skipped — the
                            // user still needs to know *something* went
                            // wrong, so fall back to the generic toast rather
                            // than showing nothing.
                            tracing::error!(%error, track_id = id, "failed to mark track missing");
                            self.show_toast(&strings::could_not_play_toast(&summary.title));
                        }
                    }
                }
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

    /// The one shared skip-loop guard (Stage 2 Task 5 — see the module's
    /// `## Fault tolerance` doc section): increments `consecutive_skips`,
    /// then either advances the queue and plays the next track, or — once
    /// `should_stop_skipping` says the chain is bounded by the queue's own
    /// length — gives up, toasts, and resets to stopped instead of spinning
    /// through an entirely-broken queue forever. Borrow discipline: `len()`
    /// and (further down) `next_manual()` each run inside their own `let`
    /// statement, so no `queue` borrow is alive when `play_track_id`/
    /// `reset_to_stopped` run — see the module's `## Queue borrow
    /// discipline` doc section.
    fn skip_after_failure(&self) {
        let queue_len = self.queue.borrow().len();
        let skips = self.consecutive_skips.get() + 1;
        self.consecutive_skips.set(skips);

        if should_stop_skipping(skips, queue_len) {
            tracing::error!(
                skips,
                queue_len,
                "too many consecutive unplayable tracks; stopping playback"
            );
            self.consecutive_skips.set(0);
            self.reset_to_stopped();
            self.show_toast(strings::PLAYBACK_STOPPED_TOO_MANY_UNPLAYABLE);
            return;
        }

        let next = self.queue.borrow_mut().next_manual();
        match next {
            Some(next_id) => self.play_track_id(next_id),
            None => {
                self.consecutive_skips.set(0);
                self.reset_to_stopped();
            }
        }
    }

    /// Shows `text` as an `adw::Toast` on the window's toast overlay, if one
    /// has been wired via `set_toast_overlay` and is still alive — degrades
    /// to a warn log otherwise (never unwraps the `WeakRef` upgrade). See the
    /// module's `## Toast + track-list-reload seam` doc section.
    fn show_toast(&self, text: &str) {
        match self.toast_overlay.upgrade() {
            Some(overlay) => overlay.add_toast(adw::Toast::new(text)),
            None => {
                tracing::warn!(text, "toast overlay is gone; degrading to log-only");
            }
        }
    }

    /// Calls the track-list reload callback wired via `set_track_list_reload`,
    /// if any — used after `queries::mark_track_missing` so the now-missing
    /// row disappears from the view. Degrades to a warn log if no callback is
    /// wired yet. Borrow discipline: the `Rc<dyn Fn()>` is cloned out of the
    /// `RefCell` in its own `let` statement before being called, mirroring
    /// the `queue` borrow discipline elsewhere in this file (see the
    /// module's `## Toast + track-list-reload seam` doc section for why this
    /// one field can currently never be re-entered, but the hoist keeps the
    /// same shape regardless).
    fn reload_track_list(&self) {
        let reload = self.reload_track_list.borrow().clone();
        match reload {
            Some(reload) => reload(),
            None => {
                tracing::warn!("track list reload requested but no callback is wired yet");
            }
        }
    }

    /// Evaluates whether the currently loaded track (if any) crossed the
    /// 50%-listened threshold (`library::stats::should_count_play`) and, if
    /// so, records a play — then clears the tracked state either way.
    /// Idempotent: called with no current track, it's a no-op, so every
    /// call site below (track switch, finish, error/stop) can call it
    /// unconditionally without risking a double-count.
    fn evaluate_play_tracking(&self) {
        let Some((track_id, duration_ms)) = self.current_track.take() else {
            return;
        };
        let max_position_ms = self.max_position_ms.replace(0);

        if !stats::should_count_play(max_position_ms, duration_ms) {
            return;
        }

        let conn = self.conn.borrow();
        match stats::record_play(&conn, track_id, now_unix()) {
            Ok(()) => {
                tracing::debug!(track_id, max_position_ms, duration_ms, "play recorded");
            }
            Err(error) => {
                tracing::error!(%error, track_id, "failed to record play");
            }
        }
    }

    /// Applies one marshalled `PlayerEvent` to the bar. Runs on the GTK main
    /// thread (called only from the drain loop in `new`).
    fn apply_event(&self, event: PlayerEvent) {
        match event {
            PlayerEvent::StateChanged(state) => {
                tracing::info!(?state, "player bar: applying state change");
                self.bar.set_state(state);
                if state == PlaybackState::Stopped {
                    self.bar.clear_track();
                    // Defensive, like the `bar.clear_track()` above:
                    // `reset_to_stopped` (the only caller of `Player::stop`)
                    // already clears `now_playing` itself before this event
                    // even has a chance to drain, but a stray `Stopped` from
                    // elsewhere must not leave stale metadata mirrored.
                    *self.now_playing.borrow_mut() = None;
                }
                self.update_mpris_mirror(mpris_status_from_playback_state(state));
            }
            PlayerEvent::Position {
                position_ms,
                duration_ms,
            } => {
                // Debug, not info: fires every 500 ms during playback.
                tracing::debug!(
                    position_ms,
                    duration_ms,
                    "player bar: applying position tick"
                );
                self.max_position_ms
                    .set(self.max_position_ms.get().max(position_ms));
                self.bar.set_position(position_ms, duration_ms);
            }
            PlayerEvent::TrackFinished => {
                tracing::info!("track finished: advancing queue");
                // Borrow discipline: `advance_auto()` runs inside this one
                // `let` statement, so the `queue` borrow drops before
                // `play_track_id`/`reset_to_stopped` run below — see the
                // module's `## Queue borrow discipline` doc section.
                let next = self.queue.borrow_mut().advance_auto();
                match next {
                    Some(id) => self.play_track_id(id),
                    None => {
                        tracing::info!("queue exhausted: resetting player to stopped");
                        self.reset_to_stopped();
                    }
                }
            }
            PlayerEvent::Error(message) => {
                // Stage 2 Task 5: this can fire asynchronously for the
                // *currently loaded* queue track (e.g. GStreamer resolving a
                // "file not found"/decode error after `play_track_id`
                // already returned `Ok`) — see the module's `## Fault
                // tolerance` doc section. Only treat it as a per-track
                // failure (diagnose + toast + auto-skip) when there is a
                // current track to attribute it to; otherwise fall back to
                // the pre-Task-5 behavior (log + reset) rather than guessing.
                match self.current_track.get() {
                    Some((id, _)) => {
                        tracing::error!(%message, track_id = id, "player error during queue track playback");
                        self.handle_unplayable_track(id);
                    }
                    None => {
                        tracing::error!(%message, "player error with no current track; resetting to stopped");
                        self.reset_to_stopped();
                    }
                }
            }
        }
    }

    /// Stops the pipeline and ensures the bar lands in the stopped/empty
    /// state. Evaluates play tracking for whatever track was loaded first
    /// (see `evaluate_play_tracking`) — every path that ends a listening
    /// session (`TrackFinished`, a player error, and a future explicit
    /// stop) funnels through here, so this is the one place that needs to
    /// call it for those cases (`play_track_id` calls it separately, for the
    /// track-switch case, since that path never calls `reset_to_stopped`).
    /// On success the rest of this relies entirely on the
    /// `StateChanged(Stopped)` event `stop()` emits — routed back here
    /// through `apply_event` — so the bar isn't reset twice. If `stop()`
    /// itself fails, though, that event never fires, so the bar is reset
    /// directly right here instead: the UI must still land in a consistent
    /// stopped state even when stopping the pipeline errors out.
    fn reset_to_stopped(&self) {
        self.evaluate_play_tracking();
        self.bar.set_transport_enabled(false);
        // Stage 2 Task 6: cleared and mirrored unconditionally, before
        // `player.stop()` even runs — unlike the bar (which relies on the
        // `StateChanged(Stopped)` event on the success path, only resetting
        // directly here on failure), the MPRIS mirror has no such event
        // fallback of its own to lean on, so it's simplest and safest to
        // just always bring it in sync with "stopped, nothing loaded" right
        // here regardless of how `player.stop()` turns out.
        *self.now_playing.borrow_mut() = None;
        self.update_mpris_mirror(MprisPlaybackStatus::Stopped);
        match self.player.stop() {
            Ok(()) => {}
            Err(error) => {
                tracing::error!(%error, "failed to stop player during reset");
                self.bar.set_state(PlaybackState::Stopped);
                self.bar.clear_track();
            }
        }
    }

    /// Recomputes the MPRIS mirror from current controller state and writes
    /// it into the shared `Arc<Mutex<mpris::MprisState>>` — see the module's
    /// `## MPRIS` doc section for the full list of call sites and why they
    /// cover every real transition. `status` is passed in rather than read
    /// from anywhere: callers always already know it more directly (the
    /// `PlayerEvent::StateChanged` payload, or the status `play_track_id`/
    /// `reset_to_stopped` are about to put the player into) than this
    /// function could re-derive it.
    ///
    /// `can_next`/`can_prev` both mirror `queue.is_empty()`'s negation — the
    /// same granularity `PlayerBar::set_transport_enabled` already uses
    /// (see `play_from_view`): this app doesn't compute a finer "not at the
    /// first/last track" distinction anywhere else either, so MPRIS clients
    /// see exactly the same enabled/disabled transport state the on-screen
    /// buttons do, rather than inventing new semantics here.
    ///
    /// Borrow discipline: `queue.borrow()` is read and dropped inside this
    /// one statement; nothing after it calls back into `queue`, `player`, or
    /// GTK, so there's no re-entrancy hazard here the way there is at the
    /// other call sites documented in the module's `## Queue borrow
    /// discipline` section — the shape is kept consistent anyway.
    fn update_mpris_mirror(&self, status: MprisPlaybackStatus) {
        let can_control = !self.queue.borrow().is_empty();
        let now_playing = self.now_playing.borrow().clone();

        let new_state = match now_playing {
            Some(track) => MprisState {
                status,
                track_id: Some(track.id),
                title: track.title,
                artist: track.artist,
                album: track.album,
                duration_ms: track.duration_ms,
                can_next: can_control,
                can_prev: can_control,
            },
            None => MprisState {
                status,
                track_id: None,
                title: String::new(),
                artist: String::new(),
                album: String::new(),
                duration_ms: 0,
                can_next: can_control,
                can_prev: can_control,
            },
        };

        let mut mirror = self
            .mpris_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *mirror = new_state;
    }

    /// Toggles play/pause on the player — shared by the bar's play/pause
    /// button and MPRIS's `PlayPause` method (see the module's `## MPRIS`
    /// doc section). Logs and no-ops on failure, matching the prior inline
    /// button-closure behavior.
    fn toggle_pause(&self) {
        if let Err(error) = self.player.toggle_pause() {
            tracing::error!(%error, "toggle play/pause failed");
        }
    }

    /// Steps the queue to the previous track and plays it (or resets to
    /// stopped if there is none) — shared by the bar's previous button and
    /// MPRIS's `Previous` method. Borrow discipline: `previous()` runs
    /// inside this one `let` statement, so the borrow drops before
    /// `play_track_id`/`reset_to_stopped` run — see the module's `## Queue
    /// borrow discipline` doc section.
    fn previous(&self) {
        let previous = self.queue.borrow_mut().previous();
        match previous {
            Some(id) => self.play_track_id(id),
            None => self.reset_to_stopped(),
        }
    }

    /// Steps the queue to the next track and plays it (or resets to stopped
    /// if there is none) — shared by the bar's next button and MPRIS's
    /// `Next` method. Same borrow discipline as `previous`.
    fn next(&self) {
        let next = self.queue.borrow_mut().next_manual();
        match next {
            Some(id) => self.play_track_id(id),
            None => self.reset_to_stopped(),
        }
    }

    /// Dispatches one command received from `mpris.rs`'s D-Bus thread (see
    /// the module's `## MPRIS` doc section) — the MPRIS drain loop's only
    /// caller. `Stop` maps directly to `reset_to_stopped` (MPRIS has no
    /// weaker "pause and forget position" stop semantics to preserve here);
    /// `Next`/`Previous` map directly to the same named methods the bar
    /// buttons call. `Play`/`Pause`/`PlayPause` need their own small
    /// handling — see `mpris_play`/`mpris_pause`'s doc comments for why they
    /// aren't just `toggle_pause`.
    fn handle_mpris_command(&self, command: MprisCommand) {
        match command {
            MprisCommand::Play => self.mpris_play(),
            MprisCommand::Pause => self.mpris_pause(),
            MprisCommand::PlayPause => self.toggle_pause(),
            MprisCommand::Stop => self.reset_to_stopped(),
            MprisCommand::Next => self.next(),
            MprisCommand::Previous => self.previous(),
        }
    }

    /// MPRIS `Play`: per spec, starts or resumes playback — unlike
    /// `PlayPause`, it must not stop an already-playing track. Reads the
    /// current status from the MPRIS mirror (kept current by `update_mpris_
    /// mirror`) rather than adding a new `Player` query method purely for
    /// this: paused resumes via `toggle_pause`; stopped starts the queue's
    /// current track via `play_track_id`, if there is one; already playing
    /// is a no-op.
    fn mpris_play(&self) {
        match self.mpris_status() {
            MprisPlaybackStatus::Playing => {}
            MprisPlaybackStatus::Paused => self.toggle_pause(),
            MprisPlaybackStatus::Stopped => {
                let current = self.queue.borrow().current();
                match current {
                    Some(id) => self.play_track_id(id),
                    None => {
                        tracing::debug!("MPRIS Play: queue is empty; nothing to play");
                    }
                }
            }
        }
    }

    /// MPRIS `Pause`: per spec, pauses — a no-op unless currently playing
    /// (unlike `PlayPause`, must not *resume* a paused track). See `mpris_
    /// play`'s doc comment for why this reads the mirror rather than adding
    /// a new `Player` query method.
    fn mpris_pause(&self) {
        if self.mpris_status() == MprisPlaybackStatus::Playing {
            self.toggle_pause();
        }
    }

    /// The MPRIS mirror's current `status` field — poisoned-recovery lock,
    /// same pattern `player.rs` uses everywhere it locks a mutex.
    fn mpris_status(&self) -> MprisPlaybackStatus {
        self.mpris_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .status
    }
}

/// Wires the bar's user-input signals to player calls. Each closure holds a
/// `Weak` controller reference: the bar is owned *by* the controller, so a
/// strong reference here would be a leak-guaranteeing Rc cycle.
fn wire_bar_controls(controller: &Rc<PlayerController>) {
    let weak = Rc::downgrade(controller);
    controller.bar.connect_play_pause(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.toggle_pause();
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_seek(move |position_ms| {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        if let Err(error) = controller.player.seek_to(position_ms) {
            tracing::error!(%error, position_ms, "seek failed");
        }
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_volume_changed(move |volume| {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.player.set_volume(volume);
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_previous(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.previous();
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_next(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.next();
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_shuffle_toggled(move |active| {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.queue.borrow_mut().set_shuffle(active);
        // Read back the queue's own idea of shuffle state (rather than just
        // logging `active`) so a log line always reflects what `Queue`
        // actually did, not just what the button asked for.
        let is_shuffled = controller.queue.borrow().is_shuffled();
        tracing::debug!(is_shuffled, "shuffle toggled");
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_repeat_clicked(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        // Explicit block (not a single statement): reading the current mode
        // and setting the new one both need the same borrow, so they're
        // scoped together here — still dropped before `set_repeat_
        // indicator` (a GTK call) runs after the block. See the module's
        // `## Queue borrow discipline` doc section.
        let next_repeat = {
            let mut queue = controller.queue.borrow_mut();
            let next_repeat = cycle_repeat(queue.repeat());
            queue.set_repeat(next_repeat);
            next_repeat
        };
        controller.bar.set_repeat_indicator(next_repeat);
    });
}

/// Maps `player::PlaybackState` to `mpris::MprisPlaybackStatus` — the one
/// explicit conversion between the two (see the module's `## MPRIS` doc
/// section for why they're deliberately separate types). Pure, so it's
/// unit-testable directly like `cycle_repeat`/`should_stop_skipping` below.
fn mpris_status_from_playback_state(state: PlaybackState) -> MprisPlaybackStatus {
    match state {
        PlaybackState::Playing => MprisPlaybackStatus::Playing,
        PlaybackState::Paused => MprisPlaybackStatus::Paused,
        PlaybackState::Stopped => MprisPlaybackStatus::Stopped,
    }
}

/// Cycles the repeat mode in the mockup's button order: Off -> All -> One ->
/// Off. Pure (no `Queue`/GTK access) so it's unit-testable directly.
fn cycle_repeat(current: Repeat) -> Repeat {
    match current {
        Repeat::Off => Repeat::All,
        Repeat::All => Repeat::One,
        Repeat::One => Repeat::Off,
    }
}

/// The skip-loop guard's pure decision (Stage 2 Task 5 — see the module's
/// `## Fault tolerance` doc section): whether `skip_after_failure` should
/// give up rather than skip to yet another track. `true` once
/// `consecutive_skips` has reached `queue_len` (an upper bound; if Repeat::Off
/// and playback started mid-queue, `next_manual` reaching the physical end
/// may trigger an earlier stop) or when the queue is empty to begin with
/// (nothing to skip to at all). Pure (no `Queue`/GTK/DB access) so it's
/// unit-testable directly.
fn should_stop_skipping(consecutive_skips: usize, queue_len: usize) -> bool {
    queue_len == 0 || consecutive_skips >= queue_len
}

/// Arms `REPRISE_SMOKE_REPEAT=all` (see the const's doc comment above):
/// forces the queue into `Repeat::All` right after construction and syncs
/// the bar's repeat indicator to match, so a headless E2E run can observe
/// auto-advance wrapping from the last queued track back to the first.
fn arm_smoke_repeat(controller: &Rc<PlayerController>) {
    let Ok(value) = std::env::var(SMOKE_REPEAT_ENV_VAR) else {
        return;
    };
    if value != SMOKE_REPEAT_ALL_VALUE {
        tracing::warn!(
            value,
            "{SMOKE_REPEAT_ENV_VAR} set to an unrecognized value; ignoring (expected \"{SMOKE_REPEAT_ALL_VALUE}\")"
        );
        return;
    }
    tracing::info!(
        "{SMOKE_REPEAT_ENV_VAR}={SMOKE_REPEAT_ALL_VALUE} set: forcing Repeat::All for headless wrap-around E2E"
    );
    controller.queue.borrow_mut().set_repeat(Repeat::All);
    controller.bar.set_repeat_indicator(Repeat::All);
}

/// Current time as Unix seconds, for `record_play`'s `last_played_at`.
/// Mirrors `library::scanner`'s private `now_unix` helper (not made public
/// there — see that module's doc comment) rather than sharing it, since the
/// two callers are otherwise unrelated and the function is a single
/// `SystemTime` call.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpris_status_mirrors_playback_state() {
        assert_eq!(
            mpris_status_from_playback_state(PlaybackState::Playing),
            MprisPlaybackStatus::Playing
        );
        assert_eq!(
            mpris_status_from_playback_state(PlaybackState::Paused),
            MprisPlaybackStatus::Paused
        );
        assert_eq!(
            mpris_status_from_playback_state(PlaybackState::Stopped),
            MprisPlaybackStatus::Stopped
        );
    }

    #[test]
    fn cycle_repeat_goes_off_all_one_off() {
        assert_eq!(cycle_repeat(Repeat::Off), Repeat::All);
        assert_eq!(cycle_repeat(Repeat::All), Repeat::One);
        assert_eq!(cycle_repeat(Repeat::One), Repeat::Off);
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
}
