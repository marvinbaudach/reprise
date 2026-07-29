//! The single-owner application runtime.
//!
//! ## What this crate owns, and why it exists
//!
//! `docs/plans/multi-frontend-core.md` §9.1 draws a sharp line through
//! Reprise's state. Everything in SQLite — library, playlists, settings,
//! modules, subscriptions, concerts, releases — stays embedded: every
//! surface links `reprise-core` directly, WAL carries many readers, and the
//! change log makes foreign writes visible. That half gets no IPC and no
//! daemon, and the hot query path stays a function call.
//!
//! The other half is the state that is *not* in the database: the audio
//! pipeline, the in-memory queue, a device run in flight, a background job's
//! progress. Until now it lived inside the GTK process, which meant an agent
//! could only reach it through MPRIS, and only while a window happened to be
//! open. This crate is that half, extracted into one owner:
//!
//! | State | Owner |
//! | --- | --- |
//! | Player pipeline, position, volume | this crate |
//! | Queue — order, current item, explicit "play next" | this crate |
//! | Device runs | this crate |
//! | Background jobs | this crate (rows in SQLite) |
//! | Database writes during a runtime effect | this crate, serialized |
//! | Library, playlists, settings, … | SQLite, read directly by every surface |
//!
//! ## Shape
//!
//! A [`Runtime`] is a synchronous reducer with ports. Commands come in from
//! connected clients, state changes, and the facets that changed go out as
//! events under one global sequence. Everything that touches the outside
//! world — audio, a device's filesystem, the clock — sits behind a trait in
//! [`ports`], which is why the whole thing runs in a unit test with no
//! display, no audio and no media files.
//!
//! What this crate deliberately does *not* contain:
//!
//! * **The lease and the D-Bus service.** Single-owner enforcement (§9.3), the
//!   activatable unit and the idle shutdown are platform packaging and land
//!   with Task 3.2 in `reprise-platform-linux`. [`Runtime::is_idle`] answers
//!   the question; it does not act on the answer.
//! * **Real ports.** The GStreamer backend and the Linux device effects
//!   already exist; wiring them to these traits is Task 3.2/3.3. Task 3.1
//!   ships the runtime and its fakes.

pub mod client;
pub mod error;
pub mod event;
#[cfg(any(test, feature = "fakes"))]
pub mod fakes;
pub mod lifecycle;
pub mod ports;

mod devices;
mod effects;
mod jobs;
mod runtime;
mod transport;

pub use client::{ClientHandshake, ClientId};
pub use error::{Capability, Failed, Refused, Rejected, RuntimeError, Unavailable};
pub use event::{Delivery, RuntimeEvent, RuntimeSnapshot, SequencedEvent};
pub use lifecycle::{Lifecycle, LifecycleChange, LifecycleMachine, RefusalCause, IDLE_GRACE};
pub use ports::{Clock, DeviceEffects, LibraryPort, PlayableTrack, Ports, TrackLocation};
pub use runtime::{Command, Connected, DeviceCommand, Runtime};

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod runtime_tests;
