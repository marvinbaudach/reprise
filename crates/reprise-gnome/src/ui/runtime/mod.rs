//! The GTK-side owner of the headless-runtime connection (thin-core stage 3
//! migration, first brick — `docs/ux-rules.md` section AG, RUN-1..RUN-5).
//!
//! Today `PlayerController` (`ui::playback::player_controller`) owns the
//! audio backend and both queues directly. The runtime is taking that job
//! over: playback, the queue, background jobs, and device runs will belong
//! to one `org.reprise.Reprise1` process, and every surface — this frontend
//! included — becomes a client of it (RUN-1). `crates/reprise-runtime-client`
//! is that client's transport and its snapshot-folding mirror, written once
//! for every surface (MCP, CLI, this one) to share; see that crate's module
//! doc for the wire contract. This module is *not* that migration — nothing
//! here is wired into `PlayerController` or the window yet, on purpose: it
//! is the additive foundation the later stages build the actual cutover on.
//!
//! ## What lives here
//!
//! [`RuntimeSession`] (`session.rs`) owns a `RuntimeClient` and a
//! `RuntimeMirror` behind a `RefCell`, pumps the client's event stream onto
//! the GTK main context, and exposes:
//!
//! - read accessors that return owned snapshots (`playback`, `queue`,
//!   `device_runs`, `jobs`, `is_connected`) — never a `Ref` out of the
//!   `RefCell`, and never a guessed value while disconnected (RUN-2);
//! - `add_on_state_changed`/`add_on_command_failed` subscriptions, in the
//!   same `RefCell<Vec<Rc<dyn Fn(..)>>>` fan-out shape
//!   `PlayerController::add_on_queue_changed` already uses;
//! - a thin command surface (`commands.rs`) — one method per
//!   [`reprise_runtime_client::RuntimeCommand`] variant, all forwarding to
//!   `RuntimeClient::send`.
//!
//! ## Borrow discipline
//!
//! Same invariant `player_controller.rs`'s `## Queue borrow discipline`
//! documents for `queue`: no `mirror`/`state_changed`/`command_failed`
//! `Ref`/`RefMut` is ever alive while a callback runs. Every accessor reads
//! the one owned value it needs and lets the borrow drop at the end of that
//! expression; every notification clones the callback list out of its
//! `RefCell` before calling any of them, so a callback that subscribes
//! again or sends a command cannot hit a live borrow. See `session.rs`'s
//! `apply`/`notify_state_changed`/`notify_command_failed` for where this is
//! enforced.
//!
//! ## Disconnected state (RUN-2/RUN-3)
//!
//! [`RuntimeMirror`](reprise_runtime_client::RuntimeMirror) already carries
//! this guarantee and `RuntimeSession` does nothing to weaken it: a session
//! with no connection yet, and one that just lost its connection, both
//! report `is_connected() == false`, `playback()`/`queue()` as `None`, and
//! `device_runs()`/`jobs()` as empty — never a dummy built from the last
//! known state. A reconnect delivers a complete snapshot that *replaces*
//! everything the mirror holds; nothing here merges an old value with a
//! new one.

mod commands;
mod session;

pub(crate) use session::RuntimeSession;
