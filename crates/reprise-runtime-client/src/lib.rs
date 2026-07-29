//! Talking to the runtime, from any surface.
//!
//! A client is stateless towards the runtime (§9.5): the runtime is the
//! truth and the client holds a mirror it replaces wholesale whenever it
//! (re)connects. This module is that mirror's plumbing, and it is
//! deliberately the only place any surface learns what a bus is.
//!
//! ## What a caller sees
//!
//! * [`start`] returns a [`RuntimeClient`] to send commands with and a
//!   stream of [`ClientEvent`]s to follow state with. It does not fail: a
//!   runtime that is not there yet is an ordinary state, reported as
//!   [`ClientEvent::Disconnected`], not an error at construction time.
//! * Commands come in two shapes on purpose. [`RuntimeClient::send`] returns
//!   immediately and reports a failure as an event — the only correct choice
//!   on a UI thread, where a bus round trip is a visible stall.
//!   [`RuntimeClient::call`] waits for the answer, which is what a tool call
//!   needs, because "did it work" *is* its result.
//! * A dropped connection is expected behaviour, not a fault. The client
//!   reconnects with a bounded backoff and emits a fresh
//!   [`ClientEvent::Connected`] carrying a complete snapshot. Nothing is
//!   replayed: a surface refreshes its state, it does not re-apply
//!   operations it missed.
//! * A command sent while disconnected is **not** buffered. It fails
//!   structurally and the surface decides whether to offer it again —
//!   executing an old intention against state it never saw is the more
//!   dangerous failure (§9.5).

mod client;
mod events;
mod mirror;

pub use client::{start, start_with_bus_name, RuntimeClient, RuntimeEvents};
pub use events::{ClientError, ClientEvent, RuntimeCommand};
pub use mirror::RuntimeMirror;
