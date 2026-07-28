//! The Linux packaging of the runtime: lease, bus service, activation.
//!
//! `reprise-runtime` is the toolkit-neutral owner of playback, the queue,
//! device runs and jobs. It knows nothing about buses, units or file locks —
//! deliberately, because none of that is portable. This module is where the
//! Linux answers live (§9.3, §9.4):
//!
//! * [`lease`] — the single-owner lock under `XDG_RUNTIME_DIR`;
//! * [`service`] — the `org.reprise.Reprise1` interface on the session bus;
//! * the activation metadata in `data/`, installed alongside the binary and
//!   checked by `scripts/check-runtime-service-install.sh` so activation
//!   cannot be green on a development machine and dead on a user's.
//!
//! Clients never see any of it. A client knows "connect" and "error"; it
//! does not know systemd (§9.4/3).

pub mod composition;
pub mod lease;
pub mod service;

mod interface;

pub use composition::{compose, ComposeError, Composition};
pub use interface::Error as RuntimeDBusError;
pub use lease::{LeaseError, RuntimeLease};
pub use service::{
    Request, RuntimeService, ServeOptions, ServiceError, ServiceInbox, BUS_NAME, OBJECT_PATH,
};
