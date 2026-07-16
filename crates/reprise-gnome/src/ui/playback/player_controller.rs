//! Bridges `Player` (whose events fire on GStreamer bus-watch and ticker
//! threads) to the GTK main thread and the `PlayerBar` widgets.
//!
//! Stage 3 Task 1 split this file's MPRIS mirror/command logic out into
//! `mpris_mirror.rs` and its fault-tolerance/auto-skip logic out into
//! `playback_faults.rs`. Task 8 split its queue-driven transport methods
//! (`previous`/`next`/`toggle_pause`/`append_to_queue`/`queue_ids_snapshot`/
//! `move_queue_item`/`purge_queue_ids`) out into `queue_transport.rs`, and
//! the Now-Playing full view's construction, wiring, and bar/page fan-out
//! (`sync_*`) out into `now_playing_wiring.rs`. All four are `impl
//! PlayerController` blocks living in sibling modules under `ui` — not
//! separate types — so `PlayerController` still has exactly one owner for
//! every field: this file. See each module's own doc comment for what moved
//! there and the `pub(super)` seam (fields/methods marked visible to `ui`
//! and its descendants) that makes reaching into this struct from a sibling
//! module possible. This file remains the canonical description of the
//! borrow-discipline invariant itself (`## Queue borrow discipline` below),
//! since `queue` is, and stays, a field owned here.
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
//! The completion implementation now lives in `play_tracking.rs`, which also
//! feeds the optional ListenBrainz runtime without adding another session-end
//! path to this edge-tight controller module.
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
//! `mpris_mirror.rs`/`playback_faults.rs`/`queue_transport.rs`, which all
//! borrow `queue` too — follows the same shape: read the one `Option<i64>`/value it needs out of
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
//!   window), so it can't be a constructor parameter; `set_toast_overlay`
//!   injects it once `window::build` has it. A `WeakRef`, not a strong
//!   reference, so the controller can never keep the window alive past its
//!   natural lifetime; `show_toast` degrades to a log line if the upgrade
//!   ever fails rather than panicking or silently dropping the toast.
//! - `reload_track_list: RefCell<Option<Rc<dyn Fn()>>>` — similarly injected
//!   post-construction via `set_track_list_reload`, since `TrackList` is
//!   also built after the controller. `window::build` supplies a closure
//!   over a `Weak<TrackList>`, never a strong `Rc`: a strong reference back
//!   would be an `Rc` cycle with `TrackList`'s own `Shared.on_activate`,
//!   which already holds a strong `Rc<PlayerController>`. `Rc<dyn Fn()>`
//!   (not `Box`) so `reload_track_list()` can clone it out of the `RefCell`
//!   in one `let` statement before calling it — same hoist-before-calling-
//!   out shape the queue borrows above use, kept for consistency even though
//!   this `RefCell` isn't actually subject to the `## Queue borrow
//!   discipline` hazard: nothing reachable from `reload_track_list()`'s call
//!   can currently call back into it re-entrantly, so there's no live bug
//!   here today, just the same defensive shape.
//!
//! ## MPRIS (Stage 2 Task 6)
//!
//! `mpris::start()` is called once, right after the controller's own `Rc`
//! exists (see `new`), spawning `mpris.rs`'s dedicated D-Bus thread and
//! handing back two things this struct holds for its whole life: `mpris_
//! state` (written by `mpris_mirror.rs`'s `update_mpris_mirror`, read by
//! that thread) and an `async_channel::Receiver<MprisCommand>`, drained by
//! `mpris_mirror.rs`'s `spawn_command_drain` (called from `new` — see that
//! function's doc comment for the drain loop). `now_playing` is a small
//! `RefCell` cache of the currently-loaded track's display fields, set
//! alongside `current_track` in `play_track_id` and cleared alongside
//! `bar.clear_track()`; both are `pub(super)` so `mpris_mirror.rs` can reach
//! them — see that module's doc comment for the mirror-update/command-
//! handling logic. The two fields look alike but differ: `current_track` is
//! only `evaluate_play_tracking`'s high-water-mark key (id/duration, never
//! rendered), while `now_playing` is the display cache MPRIS's `Metadata` is
//! built from.
//!
//! Transport methods `toggle_pause`/`next`/`previous` stay here, `pub(super)`
//! so `mpris_mirror.rs`'s `handle_mpris_command` can call them too, keeping
//! one code path for a physical media key and the on-screen button (DRY).
//! MPRIS's `Play`/`Pause` are distinct from `toggle_pause` — see
//! `mpris_mirror.rs`'s `mpris_play`/`mpris_pause` doc comments.
//!
//! ## Track-change notification (GUI-A)
//!
//! `play_track_id` asks `notifications.rs` to send a `gio::Notification`
//! (title/body from the track summary, icon from a Bar-size cover thumbnail)
//! through the `application` field's `WeakRef`. Cover resolve/decode/cache work
//! runs off the main loop, so a cold image can never delay `Player::play`.
//! Greenfield: no notification existed before this. Two rules keep it from
//! being annoying or fragile:
//!
//! - **Change, not state.** The id compared is the *previous* `now_playing`
//!   id (read before `play_track_id` overwrites it), so pause/resume of the
//!   same track — which re-enters `play_track_id` only via `play_from_view`
//!   with the same id, never on its own — never re-fires the notification.
//! - **Never fatal.** Every step (`resolve_source`, `thumbnail`, the
//!   `WeakRef` upgrade) is an `if let`/`Option` chain; a decode failure,
//!   missing cover, or missing application handle silently skips the icon or
//!   the whole send, never panics, and is never logged above `debug` (a
//!   headless/no-portal environment hits this on every track).
//! - **Never stale.** The async result carries the bar-cover generation from
//!   the same track change and is discarded if a newer track has superseded it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use libadwaita as adw;
use rusqlite::Connection;

use crate::ui::compact_player::CompactPlayer;
use crate::ui::cover_download_worker::CoverDownloadRuntime;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::mpris_mirror::{self, mpris_status_from_playback_state};
use crate::ui::player_bar::PlayerBar;
use crate::ui::player_controller_wiring;
use crate::ui::player_lyrics::{lyrics_query_for, start_track_for_lyrics, PlayerLyrics};
use crate::ui::style::cover_accent::Rgb as AccentRgb;
use reprise_core::media_integration::{MprisPlaybackStatus, SharedMprisState, DEFAULT_VOLUME};
use reprise_core::playback::{PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent};
use reprise_core::queries;
use reprise_core::queue::Queue;
use reprise_core::up_next::UpNextQueue;
use reprise_platform_linux::mpris;
use reprise_platform_linux::player::Player;

use super::scrobble_runtime::ScrobbleRuntime;
use super::scrobble_session::ScrobbleSession;

/// Whether `present_track` should start the pipeline (`Yes` — ordinary path)
/// or leave it running because `playbin3` already handed off gaplessly to the
/// pre-fed URI (`No` — see `advance_gaplessly`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartPlayback {
    Yes,
    No,
}

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
    pub(super) active_audio_effects: RefCell<reprise_core::playback::AudioEffects>,
    /// `pub(super)` (Stage 3 Task 10) so `mpris_mirror.rs`'s `mpris_set_
    /// shuffle`/`mpris_set_loop`/`mpris_set_volume` can reach `PlayerBar`'s
    /// `set_shuffle_indicator`/`set_repeat_indicator`/`set_volume_indicator`
    /// directly — same reasoning as `player` above.
    pub(super) bar: PlayerBar,
    pub(super) compact_player: CompactPlayer,
    /// The UI-owned database connection, shared with `track_list.rs` (see
    /// `window::build`) — used to write play-count updates via `library::
    /// stats::record_play`, and (via `playback_faults.rs`, `pub(super)` so
    /// that sibling module can reach it) to resolve/mark tracks on a
    /// playback failure.
    pub(super) conn: Rc<RefCell<Connection>>,
    /// `(track_id, duration_ms)` of the track currently loaded, set by
    /// `play_track_id` and cleared once play tracking has been evaluated for
    /// it (see `evaluate_play_tracking`). `None` when no track is loaded.
    pub(super) current_track: Cell<Option<(i64, i64)>>,
    /// The highest playback position observed for `current_track` via
    /// `Position` events — not the most recent one, so seeking backward
    /// near the end of a track can't cost a listener credit for having
    /// already passed the 50% mark. Reset to 0 whenever a new track starts.
    pub(super) max_position_ms: Cell<i64>,
    pub(super) listenbrainz: Rc<ScrobbleRuntime>,
    pub(super) lastfm: Rc<ScrobbleRuntime>,
    pub(super) scrobble_session: RefCell<ScrobbleSession>,
    /// The playback queue (Stage 2 Task 3/4): track order, shuffle, and
    /// repeat mode. `play_from_view` seeds it; `TrackFinished`/the
    /// previous/next buttons step through it. `pub(super)` so `mpris_mirror.
    /// rs` and `playback_faults.rs` can borrow it too — see the module's
    /// `## Queue borrow discipline` doc section for the rule every call site
    /// (in any of the three files) follows.
    pub(super) queue: RefCell<Queue>,
    pub(super) up_next: RefCell<UpNextQueue>,
    pub(super) current_up_next: Cell<Option<i64>>,
    /// See the module's `## Toast + track-list-reload seam` doc section.
    /// Empty (`WeakRef::new()`) until `set_toast_overlay` is called.
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    /// See the module's `## Toast + track-list-reload seam` doc section.
    /// `None` until `set_track_list_reload` is called.
    reload_track_list: RefCell<Option<Rc<dyn Fn()>>>,
    pub(super) queue_changed: RefCell<Option<Rc<dyn Fn()>>>,
    pub(super) current_track_changed:
        RefCell<Option<super::current_track_selection::OnCurrentTrackChanged>>,
    /// Fans coarse playback-state changes to the track list's now-playing
    /// equaliser (freeze on pause, drop the marker on stop) — see `current_
    /// track_selection.rs`. Same callback seam as `current_track_changed`,
    /// invoked from `now_playing_wiring.rs`'s `sync_state`.
    pub(super) playback_state_changed:
        RefCell<Option<super::current_track_selection::OnPlaybackStateChanged>>,
    /// Fans now-playing album identity changes to the album grid's EQ markers.
    /// Fired with `Some((album, artist))` when a new track starts and `None`
    /// when playback stops. `pub(super)` field so sibling modules can fire it;
    /// public setter so `window.rs` can register the album-view callback.
    pub(super) now_playing_album_changed: RefCell<Option<Rc<dyn Fn(Option<(String, String)>)>>>,
    /// Same seam as `playback_state_changed`, but for the album grid's
    /// now-playing equaliser (freeze on pause). Kept as a separate named slot
    /// so the track-list and album-view consumers stay independent.
    pub(super) playback_state_changed_album:
        RefCell<Option<super::current_track_selection::OnPlaybackStateChanged>>,
    /// Supplies the currently visible view's track ids for the transport's
    /// end-of-queue refill (see `up_next_transport.rs`'s `advance_common`):
    /// when a manual "next" runs off the end of an exhausted queue
    /// (`Repeat::Off`), the queue is rebuilt from these ids instead of going
    /// silent. `window.rs` wires it to `TrackList::transport_refill_ids`;
    /// returns an empty vec when refilling makes no sense (Queue view).
    pub(super) view_refill_ids: RefCell<Option<Rc<dyn Fn() -> Vec<i64>>>>,
    /// How many *consecutive* auto-skips (Stage 2 Task 5) have happened since
    /// the last successful playback start. Reset to 0 in `play_track_id` on
    /// every `Player::play` success; incremented by `playback_faults.rs`'s
    /// `skip_after_failure` (`pub(super)` so that sibling module can reach
    /// it), which consults `should_stop_skipping` against this value and the
    /// queue's length to bound the skip chain. See the module's `## Fault
    /// tolerance` doc section.
    pub(super) consecutive_skips: Cell<usize>,
    pub(super) failure_skip_limit: Cell<usize>,
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
    pub(super) now_playing: Rc<RefCell<Option<NowPlaying>>>,
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
    /// Off-thread cover decode/cache substrate (Task 4); `play_track_id`
    /// feeds the bar's and the Now-Playing page's cover widgets through this
    /// one shared instance (see `now_playing_wiring.rs`'s `sync_cover`) —
    /// same loader, two sizes, no second cache.
    pub(super) cover_loader: Rc<CoverLoader>,
    /// Generation token for the bar's cover widget (see `cover_loader.rs`):
    /// bumped per `play_track_id` call so a stale in-flight load can't
    /// clobber a newer one.
    pub(super) bar_cover_generation: Rc<Cell<u64>>,
    pub(super) compact_cover_generation: Rc<Cell<u64>>,
    /// Shared off-main lyrics runtime and weak target for the Information
    /// panel's Lyrics page. Playback position is fanned into this same owner;
    /// it never starts a second timer.
    pub(super) lyrics: Rc<PlayerLyrics>,
    /// Generation token for the seek waveform's off-main peak load, so a
    /// rapid track change can't paint a stale waveform.
    pub(super) waveform_generation: Rc<Cell<u64>>,
    /// Generation token for the cover-accent off-main extraction, so a rapid
    /// track change can't apply a stale album accent.
    pub(super) cover_accent_generation: Rc<Cell<u64>>,
    /// The accent most recently applied (or `None` for no-cover / fallback).
    /// Read by `reset_cover_accent` and `apply_cover_accent` to supply the
    /// "from" color for the 400 ms cross-fade; written back once each new
    /// accent is committed. `pub(super)` so `now_playing_wiring.rs` can
    /// borrow it.
    pub(super) cover_accent_last: Rc<RefCell<Option<AccentRgb>>>,
    /// The owning `gio::Application`, for `play_track_id`'s track-change
    /// notification (Task 9: `app.send_notification`). Passed into `new` from
    /// `window::build`, which already holds the `&adw::Application` it builds
    /// the window on — the cleanest seam, since the controller is otherwise
    /// never handed a window/application reference (see the module's `##
    /// Track-change notification` doc section). A `WeakRef`, like `toast_
    /// overlay` above, so the controller can never keep the application alive
    /// past its natural lifetime; `notify_now_playing` degrades to a no-op if
    /// the upgrade ever fails.
    pub(super) application: glib::WeakRef<gio::Application>,
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
    /// File URI for the resolved cached cover. It starts empty while the
    /// off-thread cover pipeline runs and is retained here so later status
    /// changes keep MPRIS metadata complete.
    pub(super) art_url: Option<String>,
    pub(super) duration_ms: i64,
    /// On-disk path of the currently-loaded track. Not read yet (`play_
    /// track_id` feeds `now_playing_wiring.rs`'s `sync_cover` from
    /// `summary.path` directly) — kept for parity with the other display
    /// fields above, and as the natural home for a future caller (e.g. a
    /// re-applied MPRIS mirror) that only has this cache to work from.
    #[allow(dead_code)]
    pub(super) path: String,
}

impl PlayerController {
    /// Builds the player, the bar, and the event bridge between them.
    /// `conn` is the same UI-owned database connection `track_list.rs`
    /// holds, used to record plays. Returns `Err` if GStreamer is
    /// unavailable (no playbin, bad `REPRISE_AUDIO_SINK` override, …) — the
    /// caller decides how to degrade (see `window::build`: library browsing
    /// keeps working without a bar).
    pub(super) fn new(
        conn: Rc<RefCell<Connection>>,
        cover_download: CoverDownloadRuntime,
        listenbrainz: Rc<ScrobbleRuntime>,
        lastfm: Rc<ScrobbleRuntime>,
        app: &adw::Application,
    ) -> Result<Rc<Self>, PlaybackError> {
        let (sender, receiver) = async_channel::unbounded::<PlayerEvent>();

        let player = Player::new(Box::new(move |event| {
            if let Err(error) = sender.try_send(event) {
                // Unbounded channel: try_send only fails once the receiver
                // (the drain loop below) is gone, i.e. during app teardown.
                tracing::warn!(%error, "player event dropped: UI receiver is gone");
            }
        }))?;
        let initial_effects = super::audio_effects::apply_initial(&player, &conn);
        {
            // Apply the stored transition mode to the backend up front so
            // Gapless/Crossfade is active from the first track (feed_next then
            // pre-feeds once playback starts).
            let conn_ref = conn.borrow();
            player.set_transition(
                reprise_core::library::settings::get_track_transition(&conn_ref),
                reprise_core::library::settings::get_crossfade_seconds(&conn_ref),
            );
        }

        // Stage 2 Task 6: `mpris::start()` never fails outright (see its own
        // doc comment's `## Failure is never fatal` section) — it always
        // hands back a working mirror + command receiver, spawning the
        // actual D-Bus thread in the background. Started here (right before
        // the fields it feeds), not in `window::build`, since nothing
        // outside this controller needs either handle — see the module's
        // `## MPRIS` doc section.
        //
        // MPRIS is always on (no user toggle): the redesign integrates media
        // keys / lock-screen unconditionally.
        let handles = mpris::start(crate::APP_ID);
        let mpris_state = handles.shared_state;
        let mpris_receiver = handles.commands;
        let mpris_seek_notify = handles.seek_notify;

        let controller = Rc::new(Self {
            player: Box::new(player),
            active_audio_effects: RefCell::new(initial_effects),
            bar: PlayerBar::new(),
            compact_player: CompactPlayer::new(),
            conn,
            current_track: Cell::new(None),
            max_position_ms: Cell::new(0),
            listenbrainz,
            lastfm,
            scrobble_session: RefCell::new(ScrobbleSession::default()),
            queue: RefCell::new(Queue::new()),
            up_next: RefCell::new(UpNextQueue::default()),
            current_up_next: Cell::new(None),
            toast_overlay: glib::WeakRef::new(),
            reload_track_list: RefCell::new(None),
            queue_changed: RefCell::new(None),
            current_track_changed: RefCell::new(None),
            playback_state_changed: RefCell::new(None),
            now_playing_album_changed: RefCell::new(None),
            playback_state_changed_album: RefCell::new(None),
            view_refill_ids: RefCell::new(None),
            consecutive_skips: Cell::new(0),
            failure_skip_limit: Cell::new(0),
            mpris_state,
            now_playing: Rc::new(RefCell::new(None)),
            volume: Cell::new(DEFAULT_VOLUME),
            mpris_seek_notify,
            cover_loader: CoverLoader::new(cover_download),
            bar_cover_generation: Rc::new(Cell::new(0)),
            compact_cover_generation: Rc::new(Cell::new(0)),
            lyrics: PlayerLyrics::new(),
            waveform_generation: Rc::new(Cell::new(0)),
            cover_accent_generation: Rc::new(Cell::new(0)),
            cover_accent_last: Rc::new(RefCell::new(None)),
            application: {
                let weak = glib::WeakRef::new();
                weak.set(Some(app.upcast_ref::<gio::Application>()));
                weak
            },
        });

        player_controller_wiring::wire_bar_controls(&controller);
        player_controller_wiring::wire_compact_controls(&controller);
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

        // MPRIS-command drain: see `mpris_mirror.rs`'s `spawn_command_drain`
        // doc comment (moved there — Stage-3 close-out — to keep this file's
        // line count comfortably under the split-file gate).
        mpris_mirror::spawn_command_drain(&controller, mpris_receiver);

        Ok(controller)
    }

    /// The bottom-bar widget for the library overlay shell.
    pub fn bar_widget(&self) -> &gtk4::Box {
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
    pub fn set_on_title_click(&self, f: impl Fn() + 'static) {
        self.bar.set_on_title_click(f);
    }

    /// Registers the provider for the transport's end-of-queue refill — see
    /// the `view_refill_ids` field doc.
    pub fn set_view_refill_provider(&self, provider: impl Fn() -> Vec<i64> + 'static) {
        *self.view_refill_ids.borrow_mut() = Some(Rc::new(provider));
    }

    /// The *effective album artist* of the currently-playing track, or `None`
    /// when nothing is playing / the resolved artist is blank. Used by the
    /// player-bar artist deep-link to select the right Artists-tab master row.
    ///
    /// The now-playing display cache (`NowPlaying`) only carries the *track*
    /// artist, but the Artists view groups by album artist, so this resolves
    /// the effective album artist from the DB by id using the same
    /// `EFFECTIVE_ALBUM_ARTIST` fallback (album artist when tagged, else track
    /// artist) that the Artists view groups by — see `queries::
    /// query_track_album_artist`. Borrow discipline: the `now_playing` borrow
    /// drops at the end of its own `let` statement before `conn` is borrowed.
    pub fn current_track_album_artist(&self) -> Option<String> {
        let id = self.now_playing.borrow().as_ref().map(|track| track.id)?;
        let artist = {
            let conn = self.conn.borrow();
            reprise_core::queries::query_track_album_artist(&conn, id)
                .inspect_err(|error| {
                    tracing::warn!(%error, id, "album-artist deep-link lookup failed");
                })
                .ok()
                .flatten()?
        };
        let trimmed = artist.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }

    /// Wires the cover-image click gesture — see `PlayerBar::connect_cover_clicked`.
    pub fn connect_cover_clicked(&self, f: impl Fn() + 'static) {
        self.bar.connect_cover_clicked(f);
    }

    /// Wires the artist-label click gesture — see `PlayerBar::connect_artist_clicked`.
    pub fn connect_artist_clicked(&self, f: impl Fn() + 'static) {
        self.bar.connect_artist_clicked(f);
    }

    pub fn set_track_list_reload(&self, reload: impl Fn() + 'static) {
        *self.reload_track_list.borrow_mut() = Some(Rc::new(reload));
    }

    /// Registers a callback that receives the now-playing album identity
    /// whenever it changes. Called with `Some((album, artist))` when a new
    /// track starts and `None` when playback stops. Used by `window.rs` to
    /// forward now-playing state to the album-grid EQ markers.
    pub fn set_on_now_playing_album_changed(
        &self,
        callback: impl Fn(Option<(String, String)>) + 'static,
    ) {
        *self.now_playing_album_changed.borrow_mut() = Some(Rc::new(callback));
    }

    /// Fires the now-playing-album-changed callback.
    pub(super) fn notify_now_playing_album_changed(&self, album: Option<(String, String)>) {
        let callback = self.now_playing_album_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(album);
        }
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
        self.current_up_next.set(None);

        let queue_len = self.queue.borrow().len();
        tracing::info!(queue_len, start_index, "queue set from view");

        let has_transport = !self.queue.borrow().is_empty() || !self.up_next.borrow().is_empty();
        self.sync_transport_enabled(has_transport);

        let current = self.queue.borrow().current();
        match current {
            Some(id) => self.play_track_id(id),
            None => self.reset_to_stopped(),
        }
    }

    /// Resolves `id` via `queries::query_track_summary` and starts its
    /// playback — the one place that actually calls `Player::play`, shared
    /// by `play_from_view` and every queue-stepping call site so the
    /// "resolve, evaluate prior play tracking, start playback, handle
    /// failure" sequence exists exactly once (DRY). Ends the previous
    /// track's listening session first (`evaluate_play_tracking`) — a queue
    /// step is still a track switch. On success, resets `consecutive_skips`
    /// to 0 (a good track breaks any skip chain in progress). On a `Player::
    /// play` failure, hands off to `playback_faults.rs`'s `handle_unplayable_
    /// track` (diagnose missing-vs-corrupt, mark/toast, then auto-skip)
    /// rather than resetting outright. A missing DB row or a query failure
    /// has no title/path to toast from, so those cases just log and go
    /// straight to `skip_after_failure`. `pub(super)` so `mpris_mirror.rs`
    /// and `playback_faults.rs` can call it too.
    pub(super) fn play_track_id(&self, id: i64) {
        self.present_track(id, StartPlayback::Yes);
    }

    /// Loads `id` as the now-playing track and reflects it across every
    /// surface (bar, Now-Playing, cover, lyrics, scrobble, MPRIS). The single
    /// difference `start` makes: `Yes` starts the pipeline via `play()` (the
    /// ordinary path — a fresh selection, manual skip, or `TrackFinished`
    /// advance); `No` means the audio is *already* rolling because `playbin3`
    /// handed off gaplessly to this track's pre-fed URI (see `advance_
    /// gaplessly`), so only the metadata/UI catch up — no `play()`, no gap.
    pub(super) fn present_track(&self, id: i64, start: StartPlayback) {
        self.evaluate_play_tracking();
        self.sync_lyrics_track(None);

        let summary = {
            let conn = self.conn.borrow();
            queries::query_track_summary(&conn, id)
        };

        match summary {
            Ok(Some(summary)) => {
                // Read out before `now_playing` is overwritten below — the
                // one comparison that makes the notification below fire only
                // on an actual track change, never on pause/resume of the
                // same id (see the module's `## Track-change notification`
                // doc section).
                let previous_id = self.now_playing.borrow().as_ref().map(|np| np.id);

                self.current_track.set(Some((id, summary.duration_ms)));
                self.max_position_ms.set(0);
                *self.now_playing.borrow_mut() = Some(NowPlaying {
                    id,
                    title: summary.title.clone(),
                    artist: summary.artist.clone(),
                    album: summary.album.clone(),
                    art_url: None,
                    duration_ms: summary.duration_ms,
                    path: summary.path.clone(),
                });

                // Notify the album grid so it can display the EQ marker on
                // the now-playing album card. Use the effective album artist
                // (album_artist when non-empty, artist otherwise) to match the
                // key `AlbumSummary::album_artist` uses — they must agree so
                // `rebind_in_store`'s `eq_ignore_ascii_case` comparison hits.
                self.notify_now_playing_album_changed(Some((
                    summary.album.clone(),
                    summary.effective_album_artist().to_owned(),
                )));

                // Feeds the bar AND the Now-Playing page from this one call
                // — see `now_playing_wiring.rs`'s `sync_track`/`sync_cover`
                // doc comments for why this is the single state path both
                // widgets are ever fed from.
                self.sync_track(
                    &summary.title,
                    &summary.artist,
                    &summary.album,
                    summary.year,
                );
                self.sync_cover(&summary.path);
                if previous_id != Some(id) {
                    self.notify_now_playing(
                        &summary.title,
                        &summary.artist,
                        &summary.album,
                        &summary.path,
                    );
                }
                let lyrics_result = match start {
                    StartPlayback::Yes => start_track_for_lyrics(self.player.as_ref(), &summary),
                    // Gapless: the pipeline is already playing this track, so
                    // don't restart it — just build the lyrics key.
                    StartPlayback::No => Ok(lyrics_query_for(&summary)),
                };
                match lyrics_result {
                    Ok(lyrics_query) => {
                        self.sync_lyrics_track(Some(lyrics_query));
                        tracing::info!(
                            track_id = id,
                            gapless = matches!(start, StartPlayback::No),
                            from_up_next = self.current_up_next.get() == Some(id),
                            "playback started"
                        );
                        self.begin_scrobble(reprise_core::scrobbling::TrackMetadata {
                            artist_name: summary.artist.clone(),
                            track_name: summary.title.clone(),
                            release_name: (!summary.album.trim().is_empty())
                                .then(|| summary.album.clone()),
                            duration_ms: summary.duration_ms,
                        });
                        self.notify_current_track_changed(id, None, true);
                        self.consecutive_skips.set(0);
                        self.failure_skip_limit.set(0);
                        // `reset_to_stopped` disables prev/next, and MPRIS
                        // can resume from Stopped through this arm without
                        // going through `play_from_view` — re-derive and
                        // apply the enabled state here too, hoisted into its
                        // own statement so no `queue` borrow is alive across
                        // `sync_transport_enabled` (see `## Queue borrow
                        // discipline`).
                        let queue_has_tracks = !self.queue.borrow().is_empty()
                            || !self.up_next.borrow().is_empty()
                            || self.current_up_next.get().is_some();
                        self.sync_transport_enabled(queue_has_tracks);
                        tracing::debug!(
                            queue_has_tracks,
                            "transport buttons re-enabled after playback start"
                        );
                        // Reflect the new track's metadata immediately
                        // rather than waiting for the async `StateChanged
                        // (Playing)` event — that event's arrival still
                        // triggers a second, idempotent mirror update.
                        self.update_mpris_mirror(MprisPlaybackStatus::Playing);
                        // Pre-feed the following track so the backend can hand
                        // off to it gaplessly when this one is about to finish.
                        self.feed_next();
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
            Some(overlay) => crate::ui::toasts::show(&overlay, text),
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

    /// Applies one marshalled `PlayerEvent` to the bar. Runs on the GTK main
    /// thread (called only from the drain loop in `new`).
    fn apply_event(&self, event: PlayerEvent) {
        match event {
            PlayerEvent::StateChanged(state) => {
                tracing::info!(?state, "player bar: applying state change");
                self.sync_state(state);
                if state == PlaybackState::Stopped {
                    self.sync_clear_track();
                    // Defensive, like the `sync_clear_track()` above:
                    // `reset_to_stopped` (the only caller of `Player::stop`)
                    // already clears `now_playing` itself before this event
                    // even has a chance to drain, but a stray `Stopped` from
                    // elsewhere must not leave stale metadata mirrored.
                    *self.now_playing.borrow_mut() = None;
                    // Now-playing observers (the track table's + the Artists
                    // view's mini-EQ) are turned off via the
                    // `playback_state_changed(Stopped)` fan-out that
                    // `sync_state` above already fires — see
                    // `current_track_selection::wire`.
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
                self.sync_position(position_ms, duration_ms);
                // Stage 3 Task 10: keeps MPRIS's `Position` current between
                // `update_mpris_mirror` rebuilds — see `update_mpris_
                // position`'s doc comment.
                self.update_mpris_position(position_ms);
            }
            PlayerEvent::TrackFinished => {
                tracing::info!("track finished: advancing queue");
                self.advance_playback(super::up_next_transport::AdvanceReason::Automatic);
            }
            PlayerEvent::AdvancedToNext => {
                // Gapless hand-off: the pre-fed next track is already playing.
                // Advance the queue model and reflect the new track WITHOUT
                // restarting the pipeline. (Real handler wired in below.)
                tracing::info!("gapless hand-off: advancing queue model without restart");
                self.advance_gaplessly();
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
    /// state. Evaluates play tracking for whatever track was loaded first —
    /// every path that ends a listening session (`TrackFinished`, a player
    /// error, a future explicit stop) funnels through here (`play_track_id`
    /// calls it separately for the track-switch case, since that path never
    /// calls `reset_to_stopped`). On success this relies on the `StateChanged
    /// (Stopped)` event `stop()` emits, routed back through `apply_event`, so
    /// the bar isn't reset twice; if `stop()` fails, that event never fires,
    /// so the bar is reset directly here instead. `pub(super)` so `mpris_
    /// mirror.rs` and `playback_faults.rs` can call it too.
    pub(super) fn reset_to_stopped(&self) {
        self.evaluate_play_tracking();
        self.consecutive_skips.set(0);
        self.failure_skip_limit.set(0);
        let queue_can_resume = self.current_up_next.get().is_some()
            || self.queue.borrow().current().is_some()
            || !self.up_next.borrow().is_empty();
        self.sync_transport_enabled(queue_can_resume);
        // Stage 2 Task 6: cleared and mirrored unconditionally, before
        // `player.stop()` even runs — unlike the bar (which relies on the
        // `StateChanged(Stopped)` event on the success path, only resetting
        // directly here on failure), the MPRIS mirror has no such event
        // fallback of its own to lean on, so it's simplest and safest to
        // just always bring it in sync with "stopped, nothing loaded" right
        // here regardless of how `player.stop()` turns out.
        *self.now_playing.borrow_mut() = None;
        self.notify_now_playing_album_changed(None);
        self.update_mpris_mirror(MprisPlaybackStatus::Stopped);
        match self.player.stop() {
            Ok(()) => {}
            Err(error) => {
                tracing::error!(%error, "failed to stop player during reset");
                self.sync_state(PlaybackState::Stopped);
                self.sync_clear_track();
            }
        }
    }
}
