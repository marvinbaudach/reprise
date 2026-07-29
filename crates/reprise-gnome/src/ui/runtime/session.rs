//! [`RuntimeSession`]: owns the client, the mirror, and the fan-out a
//! surface subscribes to. See the module doc comment in `mod.rs` for the
//! full picture; this file is the construction, the event pump, and the
//! read side. `commands.rs` is the write side (an `impl RuntimeSession`
//! block in a sibling file — same split `mpris_mirror.rs` uses against
//! `player_controller.rs`, and the same `pub(super)` seam: `send` below is
//! `pub(super)` purely so that sibling module can reach it, `client` itself
//! stays private).
//!
//! Nothing in the crate calls any of this yet (see `mod.rs`'s doc comment
//! for why that is deliberate). `#![allow(dead_code)]` keeps the surface
//! compilable in the meantime, the same way `ui::playback::external_media`
//! does while its own callers are still landing.
#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;

use reprise_runtime_client::{
    ClientError, ClientEvent, RuntimeClient, RuntimeCommand, RuntimeEvents, RuntimeMirror,
};
use reprise_runtime_protocol::device_run::DeviceRunSnapshot;
use reprise_runtime_protocol::jobs::JobSnapshot;
use reprise_runtime_protocol::playback::PlaybackSnapshot;
use reprise_runtime_protocol::queue::QueueSnapshot;

type StateChangedCallback = Rc<dyn Fn()>;
type CommandFailedCallback = Rc<dyn Fn(&RuntimeCommand, &ClientError)>;

/// The GTK-side owner of one runtime connection.
///
/// Cheap to hold as an `Rc<RuntimeSession>`: every accessor takes `&self`
/// and every mutation goes through the interior `RefCell`s below, the same
/// shape `PlayerController` uses for `queue`/`up_next` — see that struct's
/// module doc for the borrow-discipline rule this type follows too (this
/// module's own doc comment restates it for the fields declared here).
pub(crate) struct RuntimeSession {
    client: RuntimeClient,
    mirror: RefCell<RuntimeMirror>,
    state_changed: RefCell<Vec<StateChangedCallback>>,
    command_failed: RefCell<Vec<CommandFailedCallback>>,
}

impl RuntimeSession {
    /// Starts a client for `capabilities` against the well-known runtime
    /// name and returns a session that owns it, its event pump already
    /// running.
    ///
    /// Never fails to construct: `reprise_runtime_client::start`'s own doc
    /// comment is explicit that a runtime which is not there yet is an
    /// ordinary, later-connecting state, not a construction-time error —
    /// this constructor inherits that guarantee unchanged.
    pub(crate) fn start(capabilities: Vec<String>) -> Rc<Self> {
        let (client, events) = reprise_runtime_client::start(capabilities);
        let session = Self::from_client(client);
        Self::spawn_pump(&session, &events);
        session
    }

    /// The shared constructor body: a session with nothing known yet (the
    /// same "disconnected, nothing known" starting point
    /// [`RuntimeMirror::new`] describes), no pump running.
    ///
    /// Split out from [`Self::start`] so tests can build a session around a
    /// client without also starting the `glib` pump — see `session_tests.rs`
    /// for why that matters: those tests drive [`Self::apply`] directly with
    /// synthetic events, and a live pump racing real (empty/disconnected)
    /// events from the same client's worker thread onto the same session
    /// would make that nondeterministic.
    fn from_client(client: RuntimeClient) -> Rc<Self> {
        Rc::new(Self {
            client,
            mirror: RefCell::new(RuntimeMirror::new()),
            state_changed: RefCell::new(Vec::new()),
            command_failed: RefCell::new(Vec::new()),
        })
    }

    /// Drains `events` onto the GTK main context and folds each one into
    /// the session via [`Self::apply`].
    ///
    /// This is the same `async_channel` + `glib::spawn_future_local` bridge
    /// `ui::mpris_mirror`'s `spawn_command_drain` uses for the MPRIS command
    /// channel (see that function's doc comment for the pattern this
    /// mirrors and why it is the frontend's established shape rather than
    /// `glib::idle_add` per event): a `Weak` reference into the future, so
    /// the pump never keeps a session alive past whatever owns the `Rc`,
    /// upgraded once per iteration, the loop ending the first time that
    /// upgrade fails. `RuntimeEvents::receiver()` is documented as existing
    /// exactly for this — "the GTK frontend spawns it onto the main
    /// context" — so this is that call site.
    fn spawn_pump(session: &Rc<Self>, events: &RuntimeEvents) {
        let weak = Rc::downgrade(session);
        let receiver = events.receiver();
        glib::spawn_future_local(async move {
            while let Ok(event) = receiver.recv().await {
                let Some(session) = weak.upgrade() else {
                    break;
                };
                session.apply(&event);
            }
        });
    }

    /// Folds one event into the mirror and notifies subscribers.
    ///
    /// Takes `event` by reference, matching [`RuntimeMirror::apply`]'s own
    /// signature: nothing here needs ownership either, only to read it once
    /// (as itself, for [`Self::notify_command_failed`], and as the
    /// [`RuntimeMirror::apply`] argument).
    ///
    /// Kept as a plain, synchronous method — no `glib`, no `async` — so it
    /// is the one real seam under test: `session_tests.rs` drives it
    /// directly with synthetic [`ClientEvent`]s, exercising exactly the
    /// same code the pump above calls, without a session bus or a spawned
    /// future.
    ///
    /// Borrow discipline: [`RuntimeMirror::apply`] runs and returns inside
    /// its own `let` statement, dropping the `RefMut` before
    /// [`Self::notify_state_changed`] (which can call back into a
    /// subscriber) runs on the next line — see the module doc comment's
    /// `## Borrow discipline` section.
    fn apply(&self, event: &ClientEvent) {
        if let ClientEvent::CommandFailed { command, error, .. } = event {
            self.notify_command_failed(command, error);
        }
        let changed = self.mirror.borrow_mut().apply(event);
        if changed {
            self.notify_state_changed();
        }
    }

    fn notify_state_changed(&self) {
        let callbacks = self.state_changed.borrow().clone();
        for callback in callbacks {
            callback();
        }
    }

    fn notify_command_failed(&self, command: &RuntimeCommand, error: &ClientError) {
        let callbacks = self.command_failed.borrow().clone();
        for callback in callbacks {
            callback(command, error);
        }
    }

    /// Subscribes to "the runtime-bound state changed": fired whenever
    /// [`RuntimeMirror::apply`] reports that something a surface renders
    /// changed — a fresh snapshot on (re)connect, a disconnection or
    /// refusal (RUN-2/RUN-3), or a playback/queue/device/job delta.
    ///
    /// Same `add_on_*` fan-out shape
    /// `ui::playback::queue_transport::PlayerController::add_on_queue_changed`
    /// uses: every callback is kept, called in subscription order, and the
    /// list is cloned out of its `RefCell` before any callback runs (see
    /// [`Self::notify_state_changed`]) so a callback that subscribes again
    /// or issues a command cannot hit a live borrow.
    pub(crate) fn add_on_state_changed(&self, callback: impl Fn() + 'static) {
        self.state_changed.borrow_mut().push(Rc::new(callback));
    }

    /// Subscribes to "a command this session sent did not succeed" — the
    /// [`RuntimeClient::send`] counterpart of [`Self::add_on_state_changed`].
    /// `send` never blocks and never returns an error itself (that is the
    /// whole reason a UI thread uses it instead of `::call` — see that
    /// method's doc comment), so this is the only way a caller learns that
    /// a command it issued failed.
    pub(crate) fn add_on_command_failed(
        &self,
        callback: impl Fn(&RuntimeCommand, &ClientError) + 'static,
    ) {
        self.command_failed.borrow_mut().push(Rc::new(callback));
    }

    /// Whether the runtime is currently reachable. `false` until the first
    /// `Connected` event, and again after every disconnection or refusal —
    /// see the module doc comment's `## Disconnected state` section (RUN-2).
    pub(crate) fn is_connected(&self) -> bool {
        self.mirror.borrow().is_connected()
    }

    /// The current playback snapshot, owned — never a `Ref` out of the
    /// `RefCell` (that is the mistake `player_controller.rs`'s borrow
    /// discipline exists to prevent). `None` while disconnected, per RUN-2;
    /// never a guess built from a stale value.
    pub(crate) fn playback(&self) -> Option<PlaybackSnapshot> {
        self.mirror.borrow().playback().cloned()
    }

    /// The current queue snapshot, owned. `None` while disconnected, for
    /// the same reason as [`Self::playback`].
    pub(crate) fn queue(&self) -> Option<QueueSnapshot> {
        self.mirror.borrow().queue().cloned()
    }

    /// Every live device run, owned and sorted by device name. Empty while
    /// disconnected.
    pub(crate) fn device_runs(&self) -> Vec<DeviceRunSnapshot> {
        self.mirror.borrow().device_runs().to_vec()
    }

    /// Every live background job, owned and sorted by job id. Empty while
    /// disconnected.
    pub(crate) fn jobs(&self) -> Vec<JobSnapshot> {
        self.mirror.borrow().jobs().to_vec()
    }

    /// Sends `command` without waiting — see [`RuntimeClient::send`]'s doc
    /// comment for why this, and never `::call`, is correct on the GTK main
    /// thread. `pub(super)` so `commands.rs`'s thin per-command methods can
    /// reach it without `client` itself needing any wider visibility.
    pub(super) fn send(&self, command: RuntimeCommand) {
        self.client.send(command);
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod session_tests;
