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
//! there and the `pub(in crate::ui)` seam (fields/methods marked visible to `ui`
//! and its descendants) that makes reaching into this struct from a sibling
//! module possible. This file remains the canonical description of the
//! borrow-discipline invariant itself (`## Queue borrow discipline` below),
//! since `queue` is, and stays, a field owned here.
//!
//! ## Event marshalling: `async-channel` + `glib::spawn_future_local`
//!
//! Every GTK widget call must happen on the main thread, but the injected
//! playback backend's callback is invoked from non-GTK threads. The bridge is
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
//! `pub(in crate::ui)`, so `playback_faults.rs` can call through them):
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
//! The window composition root starts media integration once and injects its
//! handles into `new`, including two things this struct holds for its lifetime: `mpris_
//! state` (written by `mpris_mirror.rs`'s `update_mpris_mirror`, read by
//! that thread) and an `async_channel::Receiver<MprisCommand>`, drained by
//! `mpris_mirror.rs`'s `spawn_command_drain` (called from `new` — see that
//! function's doc comment for the drain loop). `now_playing` is a small
//! `RefCell` cache of the currently-loaded track's display fields, set
//! alongside `current_track` in `play_track_id` and cleared alongside
//! `bar.clear_track()`; both are `pub(in crate::ui)` so `mpris_mirror.rs` can reach
//! them — see that module's doc comment for the mirror-update/command-
//! handling logic. The two fields look alike but differ: `current_track` is
//! only `evaluate_play_tracking`'s high-water-mark key (id/duration, never
//! rendered), while `now_playing` is the display cache MPRIS's `Metadata` is
//! built from.
//!
//! Transport methods `toggle_pause`/`next`/`previous` stay here, `pub(in crate::ui)`
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
use std::sync::Arc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use libadwaita as adw;
use rusqlite::Connection;

use crate::ui::compact_player::CompactPlayer;
use crate::ui::cover_download_worker::CoverDownloadRuntime;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::mpris_mirror;
use crate::ui::player_bar::PlayerBar;
use crate::ui::player_controller_wiring;
use crate::ui::player_lyrics::{lyrics_query_for, start_track_for_lyrics, PlayerLyrics};
use crate::ui::style::cover_accent::Rgb as AccentRgb;
use reprise_core::media_integration::{
    MediaIntegrationHandles, MprisPlaybackStatus, SharedMprisState, DEFAULT_VOLUME,
};
use reprise_core::playback::{PlaybackBackend, PlaybackState, PlayerEvent};
use reprise_core::queries;
use reprise_core::queue::Queue;
use reprise_core::up_next::UpNextQueue;
use reprise_core::waveform::WaveformBackend;

type ViewRefillIds = Rc<dyn Fn() -> Vec<i64>>;

use super::scrobble_runtime::ScrobbleRuntime;
use super::scrobble_session::ScrobbleSession;

/// Whether `present_track` should start the pipeline (`Yes` — ordinary path)
/// or leave it running because `playbin3` already handed off gaplessly to the
/// pre-fed URI (`No` — see `advance_gaplessly`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum StartPlayback {
    Yes,
    No,
}

/// Contract-only platform resources assembled by the window composition root.
/// Feature modules consume this bundle without naming a concrete OS backend.
pub(in crate::ui) struct PlayerControllerBackends {
    pub(in crate::ui) playback: Box<dyn PlaybackBackend>,
    pub(in crate::ui) playback_events: async_channel::Receiver<PlayerEvent>,
    pub(in crate::ui) media: MediaIntegrationHandles,
    pub(in crate::ui) waveform: Arc<dyn WaveformBackend>,
}

// `PlayerController::volume`'s initial value is the core media-integration
// `DEFAULT_VOLUME` (Stage-3 close-out: deduplicated from what used to be a
// second, separately-defined `const DEFAULT_VOLUME: f64 = 1.0` here — see
// that constant's doc comment in `mpris::state` for why it's now the single
// source of truth, and why `ui::player_bar`'s own `VOLUME_DEFAULT` stays a
// third, deliberately-separate constant).

/// Owns the `Player` and its `PlayerBar`, routing user input from the bar to
/// the player and `PlayerEvent`s from the player back onto the bar (on the
/// GTK main thread — see the module doc comment). Also owns play-count
/// tracking (see the module's `## Play tracking` section below).
pub struct PlayerController {
    /// `pub(in crate::ui)` (Stage 3 Task 10) so `mpris_mirror.rs`'s `seek`/`mpris_
    /// set_volume` can reach `Player::seek_to`/`set_volume` directly — the
    /// same `pub(in crate::ui)` sibling-module seam `queue`/`mpris_state` already
    /// use (see the module's `## Queue borrow discipline` doc section).
    pub(in crate::ui) player: Box<dyn PlaybackBackend>,
    pub(in crate::ui) active_audio_effects: RefCell<reprise_core::playback::AudioEffects>,
    /// `pub(in crate::ui)` (Stage 3 Task 10) so `mpris_mirror.rs`'s `mpris_set_
    /// shuffle`/`mpris_set_loop`/`mpris_set_volume` can reach `PlayerBar`'s
    /// `set_shuffle_indicator`/`set_repeat_indicator`/`set_volume_indicator`
    /// directly — same reasoning as `player` above.
    pub(in crate::ui) bar: PlayerBar,
    pub(in crate::ui) compact_player: CompactPlayer,
    /// The UI-owned database connection, shared with `track_list.rs` (see
    /// `window::build`) — used to write play-count updates via `library::
    /// stats::record_play`, and (via `playback_faults.rs`, `pub(in crate::ui)` so
    /// that sibling module can reach it) to resolve/mark tracks on a
    /// playback failure.
    pub(in crate::ui) conn: Rc<RefCell<Connection>>,
    /// `(track_id, duration_ms)` of the track currently loaded, set by
    /// `play_track_id` and cleared once play tracking has been evaluated for
    /// it (see `evaluate_play_tracking`). `None` when no track is loaded.
    pub(in crate::ui) current_track: Cell<Option<(i64, i64)>>,
    /// The highest playback position observed for `current_track` via
    /// `Position` events — not the most recent one, so seeking backward
    /// near the end of a track can't cost a listener credit for having
    /// already passed the 50% mark. Reset to 0 whenever a new track starts.
    pub(in crate::ui) max_position_ms: Cell<i64>,
    pub(in crate::ui) listenbrainz: Rc<ScrobbleRuntime>,
    pub(in crate::ui) lastfm: Rc<ScrobbleRuntime>,
    pub(in crate::ui) scrobble_session: RefCell<ScrobbleSession>,
    /// The playback queue (Stage 2 Task 3/4): track order, shuffle, and
    /// repeat mode. `play_from_view` seeds it; `TrackFinished`/the
    /// previous/next buttons step through it. `pub(in crate::ui)` so `mpris_mirror.
    /// rs` and `playback_faults.rs` can borrow it too — see the module's
    /// `## Queue borrow discipline` doc section for the rule every call site
    /// (in any of the three files) follows.
    pub(in crate::ui) queue: RefCell<Queue>,
    pub(in crate::ui) up_next: RefCell<UpNextQueue>,
    pub(in crate::ui) current_up_next: Cell<Option<i64>>,
    /// Where the `queue` snapshot was seeded from (`play_from_view`) — the
    /// Queue view's named virtual context tail and NAV-9b's jump target.
    /// `None` before the first play and after a stop cleared the context.
    pub(in crate::ui) play_origin: RefCell<Option<super::play_origin::PlayOrigin>>,
    /// See the module's `## Toast + track-list-reload seam` doc section.
    /// Empty (`WeakRef::new()`) until `set_toast_overlay` is called.
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    /// See the module's `## Toast + track-list-reload seam` doc section.
    /// `None` until `set_track_list_reload` is called.
    reload_track_list: RefCell<Option<Rc<dyn Fn()>>>,
    /// Refreshes an already-open My Stats page after a real listen event is
    /// committed. Kept separate from the track-list reload seam because a
    /// listen changes statistics, not library membership.
    pub(in crate::ui) listen_event_recorded: RefCell<Option<Rc<dyn Fn()>>>,
    /// Queue-change fan-out for the sidebar/Queue view and the Now Playing
    /// panel. Callbacks are cloned out before invocation for reentrancy.
    pub(in crate::ui) queue_changed: RefCell<Vec<Rc<dyn Fn()>>>,
    pub(in crate::ui) current_track_changed:
        RefCell<Option<super::current_track_selection::OnCurrentTrackChanged>>,
    /// Fans coarse playback-state changes to the track list's now-playing
    /// equaliser (freeze on pause, drop the marker on stop) — see `current_
    /// track_selection.rs`. Same callback seam as `current_track_changed`,
    /// invoked from `now_playing_wiring.rs`'s `sync_state`.
    pub(in crate::ui) playback_state_changed:
        RefCell<Option<super::current_track_selection::OnPlaybackStateChanged>>,
    /// Independent loaded-track feed for the right Now Playing panel. This
    /// follows the player's cache, never the library selection.
    pub(in crate::ui) now_playing_panel_track_changed:
        RefCell<Option<OnNowPlayingPanelTrackChanged>>,
    /// Playback-state feed for the right panel. Pausing never clears the
    /// separately delivered loaded-track snapshot.
    pub(in crate::ui) now_playing_panel_state_changed:
        RefCell<Option<OnNowPlayingPanelStateChanged>>,
    /// Supplies the current view's ids when an exhausted queue needs refill.
    pub(in crate::ui) view_refill_ids: RefCell<Option<ViewRefillIds>>,
    /// How many *consecutive* auto-skips (Stage 2 Task 5) have happened since
    /// the last successful playback start. Reset to 0 in `play_track_id` on
    /// every `Player::play` success; incremented by `playback_faults.rs`'s
    /// `skip_after_failure` (`pub(in crate::ui)` so that sibling module can reach
    /// it), which consults `should_stop_skipping` against this value and the
    /// queue's length to bound the skip chain. See the module's `## Fault
    /// tolerance` doc section.
    pub(in crate::ui) consecutive_skips: Cell<usize>,
    pub(in crate::ui) failure_skip_limit: Cell<usize>,
    /// Shared with `mpris.rs`'s D-Bus thread — see the module's `## MPRIS`
    /// doc section. Written by `mpris_mirror.rs`'s `update_mpris_mirror`,
    /// never read directly here (the MPRIS thread is the only reader).
    /// `pub(in crate::ui)` so that sibling module can reach it.
    pub(in crate::ui) mpris_state: SharedMprisState,
    /// Title/artist/album/duration of the currently-loaded track, for
    /// `mpris_mirror.rs`'s `update_mpris_mirror` to build `mpris::MprisState`'s
    /// `Metadata` fields from — see the module's `## MPRIS` doc section for
    /// why this duplicates `current_track`'s id/duration rather than reusing
    /// it. `pub(in crate::ui)` so that sibling module can reach it.
    pub(in crate::ui) now_playing: Rc<RefCell<Option<NowPlaying>>>,
    /// The last volume value applied via the bar's volume control or an
    /// MPRIS `Volume` write (Stage 3 Task 10). `Player::set_volume` is
    /// write-only (no getter), so this is the one source of truth `update_
    /// mpris_mirror`/`mpris_mirror.rs` read from to populate `mpris::
    /// MprisState::volume` — the same "controller owns the last-known
    /// value" shape `current_track`/`now_playing` already use. `pub(in crate::ui)`
    /// so that sibling module can reach it.
    pub(in crate::ui) volume: Cell<f64>,
    /// See `mpris::start`'s doc comment: the opposite direction from
    /// `mpris_receiver` (below, in `new`) — `mpris_mirror.rs`'s `notify_
    /// mpris_seek` sends the just-seeked position into this after every
    /// successful `seek`, and `mpris.rs`'s dedicated relay thread drains it
    /// to emit the `Seeked` signal. `pub(in crate::ui)` so that sibling module can
    /// reach it.
    pub(in crate::ui) mpris_seek_notify: async_channel::Sender<i64>,
    /// Off-thread cover decode/cache substrate (Task 4); `play_track_id`
    /// feeds the bar's and the Now-Playing page's cover widgets through this
    /// one shared instance (see `now_playing_wiring.rs`'s `sync_cover`) —
    /// same loader, two sizes, no second cache.
    pub(in crate::ui) cover_loader: Rc<CoverLoader>,
    /// Generation token for the bar's cover widget (see `cover_loader.rs`):
    /// bumped per `play_track_id` call so a stale in-flight load can't
    /// clobber a newer one.
    pub(in crate::ui) bar_cover_generation: Rc<Cell<u64>>,
    pub(in crate::ui) compact_cover_generation: Rc<Cell<u64>>,
    /// Shared off-main lyrics runtime and weak target for the Now Playing
    /// panel's Lyrics view. Playback position is fanned into this same owner;
    /// it never starts a second timer.
    pub(in crate::ui) lyrics: Rc<PlayerLyrics>,
    /// Generation token for the seek waveform's off-main peak load, so a
    /// rapid track change can't paint a stale waveform.
    pub(in crate::ui) waveform_generation: Rc<Cell<u64>>,
    pub(in crate::ui) waveform_backend: Arc<dyn WaveformBackend>,
    /// Generation token for the cover-accent off-main extraction, so a rapid
    /// track change can't apply a stale album accent.
    pub(in crate::ui) cover_accent_generation: Rc<Cell<u64>>,
    /// The accent most recently applied (or `None` for no-cover / fallback).
    /// Read by `reset_cover_accent` and `apply_cover_accent` to supply the
    /// "from" color for the 400 ms cross-fade; written back once each new
    /// accent is committed. `pub(in crate::ui)` so `now_playing_wiring.rs` can
    /// borrow it.
    pub(in crate::ui) cover_accent_last: Rc<RefCell<Option<AccentRgb>>>,
    /// The owning `gio::Application`, for `play_track_id`'s track-change
    /// notification (Task 9: `app.send_notification`). Passed into `new` from
    /// `window::build`, which already holds the `&adw::Application` it builds
    /// the window on — the cleanest seam, since the controller is otherwise
    /// never handed a window/application reference (see the module's `##
    /// Track-change notification` doc section). A `WeakRef`, like `toast_
    /// overlay` above, so the controller can never keep the application alive
    /// past its natural lifetime; `notify_now_playing` degrades to a no-op if
    /// the upgrade ever fails.
    pub(in crate::ui) application: glib::WeakRef<gio::Application>,
}

/// See `PlayerController::now_playing`'s doc comment. Fields are `pub(in crate::ui)`
/// (like `now_playing` itself) so `mpris_mirror.rs`'s `update_mpris_mirror`
/// can read them to build `mpris::MprisState`'s `Metadata` fields.
#[derive(Debug, Clone)]
pub(in crate::ui) struct NowPlaying {
    pub(in crate::ui) id: i64,
    pub(in crate::ui) title: String,
    pub(in crate::ui) artist: String,
    pub(in crate::ui) album: String,
    pub(in crate::ui) album_artist: String,
    pub(in crate::ui) genre: String,
    pub(in crate::ui) artist_mbid: Option<String>,
    /// File URI for the resolved cached cover. It starts empty while the
    /// off-thread cover pipeline runs and is retained here so later status
    /// changes keep MPRIS metadata complete.
    pub(in crate::ui) art_url: Option<String>,
    pub(in crate::ui) duration_ms: i64,
    /// On-disk path of the currently-loaded track. The Now Playing panel uses
    /// this player-owned snapshot to load the same cover as the transport bar.
    pub(in crate::ui) path: String,
}

type OnNowPlayingPanelTrackChanged = Rc<dyn Fn(Option<NowPlaying>)>;
type OnNowPlayingPanelStateChanged = Rc<dyn Fn(PlaybackState)>;

impl PlayerController {
    /// Builds the controller and the event bridge around injected platform
    /// backends assembled by the window composition root.
    /// `conn` is the same UI-owned database connection `track_list.rs`
    /// holds, used to record plays. Platform construction failures are handled
    /// before this function is called so feature code only sees core contracts.
    pub(in crate::ui) fn new(
        conn: Rc<RefCell<Connection>>,
        cover_download: CoverDownloadRuntime,
        listenbrainz: Rc<ScrobbleRuntime>,
        lastfm: Rc<ScrobbleRuntime>,
        backends: PlayerControllerBackends,
        app: &adw::Application,
    ) -> Rc<Self> {
        let PlayerControllerBackends {
            playback: player,
            playback_events: receiver,
            media: handles,
            waveform,
        } = backends;
        let initial_effects = super::audio_effects::apply_initial(player.as_ref(), &conn);
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

        // Media integration is always on. Its platform handles are assembled
        // by the window composition root and remain failure-tolerant.
        let mpris_state = handles.shared_state;
        let mpris_receiver = handles.commands;
        let mpris_seek_notify = handles.seek_notify;

        let lyrics = PlayerLyrics::new(&conn.borrow());
        let controller = Rc::new(Self {
            player,
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
            play_origin: RefCell::new(None),
            toast_overlay: glib::WeakRef::new(),
            reload_track_list: RefCell::new(None),
            listen_event_recorded: RefCell::new(None),
            queue_changed: RefCell::new(Vec::new()),
            current_track_changed: RefCell::new(None),
            playback_state_changed: RefCell::new(None),
            now_playing_panel_track_changed: RefCell::new(None),
            now_playing_panel_state_changed: RefCell::new(None),
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
            lyrics,
            waveform_generation: Rc::new(Cell::new(0)),
            waveform_backend: waveform,
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

        controller
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

    pub fn set_view_refill_provider(&self, provider: impl Fn() -> Vec<i64> + 'static) {
        *self.view_refill_ids.borrow_mut() = Some(Rc::new(provider));
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

    pub(in crate::ui) fn set_on_listen_event_recorded(&self, callback: impl Fn() + 'static) {
        *self.listen_event_recorded.borrow_mut() = Some(Rc::new(callback));
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
    /// straight to `skip_after_failure`. `pub(in crate::ui)` so `mpris_mirror.rs`
    /// and `playback_faults.rs` can call it too.
    pub(in crate::ui) fn play_track_id(&self, id: i64) {
        self.play_track_id_with_change(
            id,
            super::current_track_selection::CurrentTrackChange::PlaybackStarted,
        );
    }

    pub(in crate::ui) fn play_track_id_with_change(
        &self,
        id: i64,
        change: super::current_track_selection::CurrentTrackChange,
    ) {
        self.present_track(id, StartPlayback::Yes, change);
    }

    /// Loads `id` as the now-playing track and reflects it across every
    /// surface (bar, Now-Playing, cover, lyrics, scrobble, MPRIS). The single
    /// difference `start` makes: `Yes` starts the pipeline via `play()` (the
    /// ordinary path — a fresh selection, manual skip, or `TrackFinished`
    /// advance); `No` means the audio is *already* rolling because `playbin3`
    /// handed off gaplessly to this track's pre-fed URI (see `advance_
    /// gaplessly`), so only the metadata/UI catch up — no `play()`, no gap.
    pub(in crate::ui) fn present_track(
        &self,
        id: i64,
        start: StartPlayback,
        change: super::current_track_selection::CurrentTrackChange,
    ) {
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
                    album_artist: summary.album_artist.clone(),
                    genre: summary.genre.clone(),
                    artist_mbid: summary.artist_mbid.clone(),
                    art_url: None,
                    duration_ms: summary.duration_ms,
                    path: summary.path.clone(),
                });

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
                        self.notify_current_track_changed(id, None, change);
                        // The composite Queue view keys its Now Playing row
                        // and Up Next tail off the playhead — every track
                        // change re-partitions it (QUE-1) and shrinks the
                        // QUE-5 counter.
                        self.notify_queue_changed();
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
    /// module's `## Toast + track-list-reload seam` doc section. `pub(in crate::ui)`
    /// so `playback_faults.rs`'s `handle_unplayable_track`/`skip_after_
    /// failure` can call it too.
    pub(in crate::ui) fn show_toast(&self, text: &str) {
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
    /// same shape regardless). `pub(in crate::ui)` so `playback_faults.rs`'s
    /// `handle_unplayable_track` can call it too.
    pub(in crate::ui) fn reload_track_list(&self) {
        let reload = self.reload_track_list.borrow().clone();
        match reload {
            Some(reload) => reload(),
            None => {
                tracing::warn!("track list reload requested but no callback is wired yet");
            }
        }
    }
}
