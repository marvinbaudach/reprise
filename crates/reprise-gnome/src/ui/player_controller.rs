//! Bridges `Player` (whose events fire on GStreamer bus-watch and ticker
//! threads) to the GTK main thread and the `PlayerBar` widgets.
//!
//! Stage 3 Task 1 split this file's MPRIS mirror/command logic out into
//! `mpris_mirror.rs` and its fault-tolerance/auto-skip logic out into
//! `playback_faults.rs`. Both are `impl PlayerController` blocks living in
//! sibling modules under `ui` — not separate types — so `PlayerController`
//! still has exactly one owner for every field: this file. See each
//! module's own doc comment for what moved there and the `pub(super)` seam
//! (fields/methods marked visible to `ui` and its descendants) that makes
//! reaching into this struct from a sibling module possible. This file
//! remains the canonical description of the borrow-discipline invariant
//! itself (`## Queue borrow discipline` below), since `queue` is, and
//! stays, a field owned here.
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
//! player/GTK-facing call runs.** Every call site — in this file and in
//! `mpris_mirror.rs`/`playback_faults.rs`, which both borrow `queue` too —
//! follows the same shape: read the one `Option<i64>`/value it needs out of
//! the queue in a single expression (a `let x = queue.borrow_mut().method();`
//! statement, or an explicit `{ }` block when more than one queue call is
//! needed), which drops the borrow at the end of that statement/block —
//! *before* the next statement calls out. Unlike the two prior bugs, no
//! explicit `Rc::clone`-out step is needed here: `Queue`'s methods return
//! owned `Option<i64>`/`Repeat` values, not references into the `RefCell`'s
//! contents, so the borrow's temporary scope is inherently the only thing to
//! manage.
//!
//! ## Fault tolerance: toast, missing flag, and auto-skip (Stage 2 Task 5)
//!
//! Moved to `playback_faults.rs` (Stage 3 Task 1): `handle_unplayable_track`
//! and `skip_after_failure` are defined there as `impl PlayerController`
//! methods, called from `play_track_id`'s `Player::play` failure branch and
//! `apply_event`'s `PlayerEvent::Error` arm below exactly as before. See that
//! module's doc comment for the full "diagnose, mark/toast, skip" story.
//!
//! ### Toast + track-list-reload seam
//!
//! Both `handle_unplayable_track` (now in `playback_faults.rs`) and this
//! file need to reach widgets the controller doesn't own outright — this is
//! why the two fields below stay here (and their accessor methods are
//! `pub(super)`, so `playback_faults.rs` can call through them):
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
//! `mpris_state` (`Arc<Mutex<mpris::MprisState>>`, written by `mpris_mirror.
//! rs`'s `update_mpris_mirror`, read by that thread) and an `async_channel::
//! Receiver<MprisCommand>`, drained by a second `glib::spawn_future_local`
//! loop exactly parallel to the `PlayerEvent` drain loop already in `new`
//! (same `Weak`-controller-upgrade-or-break shape), calling `mpris_mirror.
//! rs`'s `handle_mpris_command` per command. `now_playing` is a small
//! `RefCell` cache of the currently-loaded track's title/artist/album/
//! duration (`current_track` already tracks id/duration but only as a
//! play-tracking high-water-mark key, not for display) — set alongside
//! `current_track` in `play_track_id`, cleared alongside `bar.clear_track()`
//! wherever that already runs. Both fields are `pub(super)` so `mpris_
//! mirror.rs` (a sibling module, not a descendant of this one) can reach
//! them — see that module's doc comment for the full mirror-update/command-
//! handling logic itself, which moved there in Stage 3 Task 1.
//!
//! Transport methods `toggle_pause`/`next`/`previous` stay here, `pub(super)`
//! so `mpris_mirror.rs`'s `handle_mpris_command` can call them too — not
//! inlined in the bar's button closures the way they used to be —
//! specifically so both `wire_bar_controls` and `handle_mpris_command` call
//! the same code (DRY): a physical media key and the on-screen button must
//! behave identically. MPRIS's `Play`/`Pause` are *not* the same as
//! `toggle_pause`, though (`PlayPause` is): the MPRIS spec has `Play`
//! start-or-resume and `Pause` pause, each a no-op if already in that state,
//! whereas the bar only ever has one button that alternates — see `mpris_
//! mirror.rs`'s `mpris_play`/`mpris_pause`, which consult `mpris_state`'s own
//! `status` (kept current by `update_mpris_mirror`) to decide whether
//! `toggle_pause`/`play_track_id` actually apply, rather than adding a new
//! `Player` query method just for this.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use libadwaita as adw;
use rusqlite::Connection;

use crate::ui::mpris_mirror::mpris_status_from_playback_state;
use crate::ui::player_bar::PlayerBar;
use crate::ui::player_controller_wiring;
use reprise_core::library::stats;
use reprise_core::media_integration::{MprisPlaybackStatus, SharedMprisState, DEFAULT_VOLUME};
use reprise_core::playback::{PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent};
use reprise_core::queries;
use reprise_core::queue::Queue;
use reprise_platform_linux::mpris;
use reprise_platform_linux::player::Player;

// `PlayerController::volume`'s initial value is `reprise_platform_linux::mpris::
// DEFAULT_VOLUME` (Stage-3 close-out: deduplicated from what used to be a
// second, separately-defined `const DEFAULT_VOLUME: f64 = 1.0` here — see
// that constant's doc comment in `mpris::state` for why it's now the single
// source of truth, and why `ui::player_bar`'s own `VOLUME_DEFAULT` stays a
// third, deliberately-separate constant).

/// Owns the `Player` and its `PlayerBar`, routing user input from the bar to
/// the player and `PlayerEvent`s from the player back onto the bar (on the
/// GTK main thread — see the module doc comment). Also owns play-count
/// tracking (see the module's `## Play tracking` section below).
pub struct PlayerController {
    /// `pub(super)` (Stage 3 Task 10) so `mpris_mirror.rs`'s `seek`/`mpris_
    /// set_volume` can reach `Player::seek_to`/`set_volume` directly — the
    /// same `pub(super)` sibling-module seam `queue`/`mpris_state` already
    /// use (see the module's `## Queue borrow discipline` doc section).
    pub(super) player: Box<dyn PlaybackBackend>,
    /// `pub(super)` (Stage 3 Task 10) so `mpris_mirror.rs`'s `mpris_set_
    /// shuffle`/`mpris_set_loop`/`mpris_set_volume` can reach `PlayerBar`'s
    /// `set_shuffle_indicator`/`set_repeat_indicator`/`set_volume_indicator`
    /// directly — same reasoning as `player` above.
    pub(super) bar: PlayerBar,
    /// The UI-owned database connection, shared with `track_list.rs` (see
    /// `window::build`) — used to write play-count updates via `library::
    /// stats::record_play`, and (via `playback_faults.rs`, `pub(super)` so
    /// that sibling module can reach it) to resolve/mark tracks on a
    /// playback failure.
    pub(super) conn: Rc<RefCell<Connection>>,
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
    /// previous/next buttons step through it. `pub(super)` so `mpris_mirror.
    /// rs` and `playback_faults.rs` can borrow it too — see the module's
    /// `## Queue borrow discipline` doc section for the rule every call site
    /// (in any of the three files) follows.
    pub(super) queue: RefCell<Queue>,
    /// See the module's `## Toast + track-list-reload seam` doc section.
    /// Empty (`WeakRef::new()`) until `set_toast_overlay` is called.
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    /// See the module's `## Toast + track-list-reload seam` doc section.
    /// `None` until `set_track_list_reload` is called.
    reload_track_list: RefCell<Option<Rc<dyn Fn()>>>,
    /// How many *consecutive* auto-skips (Stage 2 Task 5) have happened since
    /// the last successful playback start. Reset to 0 in `play_track_id` on
    /// every `Player::play` success; incremented by `playback_faults.rs`'s
    /// `skip_after_failure` (`pub(super)` so that sibling module can reach
    /// it), which consults `should_stop_skipping` against this value and the
    /// queue's length to bound the skip chain. See the module's `## Fault
    /// tolerance` doc section.
    pub(super) consecutive_skips: Cell<usize>,
    /// Shared with `mpris.rs`'s D-Bus thread — see the module's `## MPRIS`
    /// doc section. Written by `mpris_mirror.rs`'s `update_mpris_mirror`,
    /// never read directly here (the MPRIS thread is the only reader).
    /// `pub(super)` so that sibling module can reach it.
    pub(super) mpris_state: SharedMprisState,
    /// Title/artist/album/duration of the currently-loaded track, for
    /// `mpris_mirror.rs`'s `update_mpris_mirror` to build `mpris::MprisState`'s
    /// `Metadata` fields from — see the module's `## MPRIS` doc section for
    /// why this duplicates `current_track`'s id/duration rather than reusing
    /// it. `pub(super)` so that sibling module can reach it.
    pub(super) now_playing: RefCell<Option<NowPlaying>>,
    /// The last volume value applied via the bar's volume control or an
    /// MPRIS `Volume` write (Stage 3 Task 10). `Player::set_volume` is
    /// write-only (no getter), so this is the one source of truth `update_
    /// mpris_mirror`/`mpris_mirror.rs` read from to populate `mpris::
    /// MprisState::volume` — the same "controller owns the last-known
    /// value" shape `current_track`/`now_playing` already use. `pub(super)`
    /// so that sibling module can reach it.
    pub(super) volume: Cell<f64>,
    /// See `mpris::start`'s doc comment: the opposite direction from
    /// `mpris_receiver` (below, in `new`) — `mpris_mirror.rs`'s `notify_
    /// mpris_seek` sends the just-seeked position into this after every
    /// successful `seek`, and `mpris.rs`'s dedicated relay thread drains it
    /// to emit the `Seeked` signal. `pub(super)` so that sibling module can
    /// reach it.
    pub(super) mpris_seek_notify: async_channel::Sender<i64>,
}

/// See `PlayerController::now_playing`'s doc comment. Fields are `pub(super)`
/// (like `now_playing` itself) so `mpris_mirror.rs`'s `update_mpris_mirror`
/// can read them to build `mpris::MprisState`'s `Metadata` fields.
#[derive(Debug, Clone)]
pub(super) struct NowPlaying {
    pub(super) id: i64,
    pub(super) title: String,
    pub(super) artist: String,
    pub(super) album: String,
    pub(super) duration_ms: i64,
}

impl PlayerController {
    /// Builds the player, the bar, and the event bridge between them.
    /// `conn` is the same UI-owned database connection `track_list.rs`
    /// holds, used to record plays. Returns `Err` if GStreamer is
    /// unavailable (no playbin, bad `REPRISE_AUDIO_SINK` override, …) — the
    /// caller decides how to degrade (see `window::build`: library browsing
    /// keeps working without a bar).
    pub fn new(conn: Rc<RefCell<Connection>>) -> Result<Rc<Self>, PlaybackError> {
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
        let handles = mpris::start(crate::APP_ID);
        let mpris_state = handles.shared_state;
        let mpris_receiver = handles.commands;
        let mpris_seek_notify = handles.seek_notify;

        let controller = Rc::new(Self {
            player: Box::new(player),
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
            volume: Cell::new(DEFAULT_VOLUME),
            mpris_seek_notify,
        });

        player_controller_wiring::wire_bar_controls(&controller);
        player_controller_wiring::arm_smoke_repeat(&controller);

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
        // one `async_channel::Receiver`, one command applied per iteration
        // via `mpris_mirror.rs`'s `handle_mpris_command`.
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
    /// marked missing (see `playback_faults.rs`'s `handle_unplayable_track`),
    /// once the track list exists. `window::build` supplies a closure over a
    /// `Weak<TrackList>`, not a strong `Rc` — see the module's `## Toast +
    /// track-list-reload seam` doc section for why a strong reference here
    /// would leak.
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
    /// `playback_faults.rs`'s `skip_after_failure` auto-skip) so the
    /// "resolve, evaluate prior play tracking, start playback, handle
    /// failure" sequence exists exactly once (DRY). Ends the previous
    /// track's listening session first (`evaluate_play_tracking`, same as
    /// the old per-`Track` `play_track` did) — a queue step is still a track
    /// switch. On success, resets `consecutive_skips` to 0 (Stage 2 Task 5:
    /// a good track breaks any skip chain in progress). On a `Player::play`
    /// failure, hands off to `playback_faults.rs`'s `handle_unplayable_track`
    /// (diagnose missing-vs-corrupt, mark/toast, then auto-skip via `skip_
    /// after_failure`) rather than resetting outright — see that module's
    /// doc comment. A missing DB row or a query failure has no title/path to
    /// build a toast from, so those two cases just log and go straight to
    /// `skip_after_failure` (still counted against the skip-loop guard, so a
    /// queue of entirely-vanished rows can't spin forever either). `pub
    /// (super)` so `mpris_mirror.rs`'s `mpris_play` and `playback_faults.rs`'s
    /// `skip_after_failure` can call it too.
    pub(super) fn play_track_id(&self, id: i64) {
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
                        // Stage-2 close-out: `reset_to_stopped` disables the
                        // prev/next transport buttons, and MPRIS's
                        // `Previous`/`Play` commands can resume playback from
                        // Stopped straight through this arm (see
                        // `mpris_mirror.rs`'s `handle_mpris_command`) without
                        // ever going through `set_queue`/`play_from_view`,
                        // the only other call sites that re-enable them.
                        // Re-deriving and applying the enabled state here, on
                        // every successful playback start, keeps the
                        // on-screen buttons in sync with MPRIS-driven
                        // transitions too — hoisted into its own statement
                        // first so no `queue` borrow is alive across the
                        // `set_transport_enabled` call (see the module's
                        // `## Queue borrow discipline` doc section).
                        let queue_has_tracks = !self.queue.borrow().is_empty();
                        self.bar.set_transport_enabled(queue_has_tracks);
                        tracing::debug!(
                            queue_has_tracks,
                            "transport buttons re-enabled after playback start"
                        );
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

    /// Shows `text` as an `adw::Toast` on the window's toast overlay, if one
    /// has been wired via `set_toast_overlay` and is still alive — degrades
    /// to a warn log otherwise (never unwraps the `WeakRef` upgrade). See the
    /// module's `## Toast + track-list-reload seam` doc section. `pub(super)`
    /// so `playback_faults.rs`'s `handle_unplayable_track`/`skip_after_
    /// failure` can call it too.
    pub(super) fn show_toast(&self, text: &str) {
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
    /// same shape regardless). `pub(super)` so `playback_faults.rs`'s
    /// `handle_unplayable_track` can call it too.
    pub(super) fn reload_track_list(&self) {
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
                // Stage 3 Task 10: keeps MPRIS's `Position` current between
                // `update_mpris_mirror` rebuilds — see `update_mpris_
                // position`'s doc comment.
                self.update_mpris_position(position_ms);
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
                // already returned `Ok`) — see `playback_faults.rs`'s doc
                // comment. Only treat it as a per-track failure (diagnose +
                // toast + auto-skip) when there is a current track to
                // attribute it to; otherwise fall back to the pre-Task-5
                // behavior (log + reset) rather than guessing.
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
    /// stopped state even when stopping the pipeline errors out. `pub
    /// (super)` so `mpris_mirror.rs`'s `handle_mpris_command`/`mpris_play`
    /// and `playback_faults.rs`'s `skip_after_failure` can call it too.
    pub(super) fn reset_to_stopped(&self) {
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

    /// Toggles play/pause on the player — shared by the bar's play/pause
    /// button and MPRIS's `PlayPause` method (see the module's `## MPRIS`
    /// doc section). Logs and no-ops on failure, matching the prior inline
    /// button-closure behavior. `pub(super)` so `mpris_mirror.rs` can call it
    /// too.
    pub(super) fn toggle_pause(&self) {
        if let Err(error) = self.player.toggle_pause() {
            tracing::error!(%error, "toggle play/pause failed");
        }
    }

    /// Steps the queue to the previous track and plays it (or resets to
    /// stopped if there is none) — shared by the bar's previous button and
    /// MPRIS's `Previous` method. Borrow discipline: `previous()` runs
    /// inside this one `let` statement, so the borrow drops before
    /// `play_track_id`/`reset_to_stopped` run — see the module's `## Queue
    /// borrow discipline` doc section. `pub(super)` so `mpris_mirror.rs` can
    /// call it too.
    pub(super) fn previous(&self) {
        let previous = self.queue.borrow_mut().previous();
        match previous {
            Some(id) => self.play_track_id(id),
            None => self.reset_to_stopped(),
        }
    }

    /// Steps the queue to the next track and plays it (or resets to stopped
    /// if there is none) — shared by the bar's next button and MPRIS's
    /// `Next` method. Same borrow discipline as `previous`. `pub(super)` so
    /// `mpris_mirror.rs` can call it too.
    pub(super) fn next(&self) {
        let next = self.queue.borrow_mut().next_manual();
        match next {
            Some(id) => self.play_track_id(id),
            None => self.reset_to_stopped(),
        }
    }

    /// "Add to queue" context-menu action (Stage 3 Task 5): appends `ids` to
    /// the end of the current queue via `Queue::append_tracks` — see that
    /// method's doc comment for the exact append/no-auto-start semantics —
    /// without ever calling `play_track_id`. A no-op for an empty `ids`
    /// slice. If the queue was previously empty, the transport buttons are
    /// re-enabled to match its now-non-empty state (the same re-derivation
    /// `play_track_id` already does on every successful playback start), but
    /// no track starts playing: `ui::track_actions::queue_selected_ids`
    /// guards the empty case, and the queue itself only forms a `pos` of
    /// `Some(0)` for bookkeeping (see `Queue::append_tracks`) — playback
    /// stays exactly as it was. Borrow discipline: `append_tracks`/`is_
    /// empty` each run inside their own statement, so no `queue` borrow is
    /// alive across the `bar.set_transport_enabled` call (see the module's
    /// `## Queue borrow discipline` doc section).
    pub(super) fn append_to_queue(&self, ids: &[i64]) {
        if ids.is_empty() {
            tracing::debug!("append to queue: nothing to add; ignoring");
            return;
        }
        self.queue.borrow_mut().append_tracks(ids);
        let queue_len = self.queue.borrow().len();
        let queue_has_tracks = queue_len > 0;
        self.bar.set_transport_enabled(queue_has_tracks);
        tracing::info!(added = ids.len(), queue_len, "tracks added to queue");
    }

    /// Snapshot of every queued track id in current play order (Stage 3
    /// Task 3's `ViewSource::Queue` seam — see `queue::Queue::ids_in_order`'s
    /// doc comment). `track_list.rs`'s queue-ids provider closure (wired in
    /// `window::build`) calls this each time the track list reloads while
    /// showing the Queue source, so that view always reflects the queue's
    /// live state (including shuffle) rather than a stale copy. Hoisted
    /// into its own `let` statement even though nothing here calls back
    /// into `self` — consistent with every other `queue` access in this
    /// file (see the module's `## Queue borrow discipline` doc section).
    /// `pub(super)` so `track_list.rs` (a sibling module under `ui`) can
    /// call it. No explicit hoisting `let` is needed here (unlike most other
    /// `queue` accesses in this file): `ids_in_order()` returns an owned
    /// `Vec`, so the temporary `Ref` this creates already drops at the end
    /// of this one expression, before the function returns — there's no
    /// second statement left for it to still be alive across.
    pub(super) fn queue_ids_snapshot(&self) -> Vec<i64> {
        self.queue.borrow().ids_in_order()
    }

    /// Queue drag-reorder (Stage 3 Task 6): moves the queued track at index
    /// `from` to index `to` via `queue::Queue::move_item` — see that method's
    /// doc comment for the current-track-preservation contract and
    /// out-of-range/no-op handling (never panics). `ui::track_list_dnd`'s
    /// queue-reorder drop handler calls this via `TrackList::set_on_queue_
    /// reorder`, then reloads the track list itself so the Queue view picks
    /// up the new order — this method only mutates queue state, the same
    /// "state mutation only, caller decides what to refresh" contract as
    /// `append_to_queue`. Returns `Queue::move_item`'s own bool verbatim, so
    /// a no-op move (empty queue, out-of-range index, `from == to`) is
    /// reported as `false` rather than the caller assuming success just
    /// because a player was available to ask (Stage 3 Task 6 review finding
    /// #3).
    pub(super) fn move_queue_item(&self, from: usize, to: usize) -> bool {
        self.queue.borrow_mut().move_item(from, to)
    }

    /// Purges hard-deleted track ids from the queue (Stage-3 close-out):
    /// "Remove from library" (`queries::remove_missing_tracks`) deletes
    /// `tracks` rows outright — without this, a queued id that no longer
    /// resolves to a row desyncs `Queue::len`/`ids_in_order` from what
    /// `ViewSource::Queue`'s window query can actually render (see
    /// `queries.rs`'s module doc, `Queue` section, and `query_track_count`'s
    /// `Queue` arm). Called from `ui::track_list_context_menu::handle_
    /// remove_from_library` with exactly the ids `remove_missing_tracks`
    /// reports as actually deleted — never the raw requested selection,
    /// which could include ids that turned out not to be missing any more
    /// and so were never deleted. A no-op for an empty slice (no `queue`
    /// borrow taken at all). Hoisted borrow: `Queue::remove_ids` runs in its
    /// own statement, matching every other `queue` access in this file (see
    /// the module's `## Queue borrow discipline` doc section).
    pub(super) fn purge_queue_ids(&self, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        let changed = self.queue.borrow_mut().remove_ids(ids);
        if changed {
            tracing::info!(
                removed = ids.len(),
                queue_len = self.queue.borrow().len(),
                "queue purged of hard-deleted track ids"
            );
        }
    }
}

/// Current time as Unix seconds, for `record_play`'s `last_played_at`.
/// Mirrors `library::scanner`'s private `now_unix` helper (not made public
/// there — see that module's doc comment) rather than sharing it, since the
/// two callers are otherwise unrelated and the function is a single
/// `SystemTime` call.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
