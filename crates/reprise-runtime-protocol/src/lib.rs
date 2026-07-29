//! The versioned command and snapshot contract between the Reprise runtime
//! and its clients (GTK, MCP, CLI, later frontends).
//!
//! ## Why this crate exists
//!
//! The device-sync surface used to be a sixteen-field positional tuple —
//! `(String, bool, String, u64, u64, u64, …)` — declared **twice**: once in
//! `reprise-platform-linux` as the D-Bus interface's return type, once in
//! `reprise-mcp` as the client's decode type. Seven of those fields were
//! `u64` and two were `String`, so swapping a pair on one side and not the
//! other compiles, passes the signature check, and silently reports the
//! wrong numbers. Nested tuples made it worse: `DeviceSyncChangesRow` was
//! seven consecutive `u64`s.
//!
//! Every wire type therefore lives here exactly once, with names. A field
//! cannot drift between the two sides because there is only one side.
//!
//! ## Encoding
//!
//! Snapshots cross D-Bus as dictionaries (`a{sv}`) keyed by field name, not
//! as positional structs. Two consequences are deliberate:
//!
//! - Reordering fields is not observable on the wire, so it cannot corrupt a
//!   client.
//! - Adding an optional field is backwards compatible: an older client
//!   ignores the unknown key, a newer client sees `None` when an older
//!   service omits it. `Option` fields are simply absent when unset, which
//!   is also why no `(bool, T)` "optional" pairs survive from the tuples.
//!
//! Commands stay ordinary typed method arguments; they are short, named at
//! the call site, and a wrong one fails the signature check.
//!
//! ## Versioning
//!
//! [`PROTOCOL_VERSION`] is the contract's version, not the application's. A
//! client checks it during the handshake and refuses a mismatched major
//! version instead of decoding a payload it does not understand — the
//! `Refused` case of the runtime's error semantics (see section 9.7 of
//! `docs/plans/multi-frontend-core.md`).
//!
//! ## Path freedom
//!
//! No snapshot in this crate carries a local filesystem path. Entities are
//! named by opaque ids, devices by their display name, failures by a short
//! diagnostic kind. `tests/schema.rs` enforces that against fully populated
//! fixtures rather than trusting review.

pub mod device_run;
pub mod device_sync;
pub mod endpoint;
pub mod jobs;
pub mod playback;
pub mod queue;
pub mod runtime;

pub use endpoint::{BUS_NAME, INTERFACE_NAME, OBJECT_PATH};

/// The protocol contract's version.
///
/// Bump `major` for any change a current client cannot survive: removing or
/// renaming a field, changing a field's type, changing the meaning of an
/// existing value. Bump `minor` for additive change: a new optional field, a
/// new command, a new enum-like string value that older clients may ignore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

impl ProtocolVersion {
    /// Whether a peer advertising `self` can serve a client built against
    /// `expected`.
    ///
    /// Only the major version decides. A lower minor is explicitly fine: a
    /// minor bump may only *add* optional fields and commands, so an older
    /// peer simply omits a key the client then reads as `None`. Refusing a
    /// lower minor would contradict that rule and would hard-fail the most
    /// ordinary upgrade sequence there is — a client updated on disk while
    /// the older runtime is still running.
    ///
    /// The minor number is still worth carrying: a client can read it to
    /// know it is talking to an older peer and skip a feature knowingly
    /// instead of discovering an absent field by accident.
    #[must_use]
    pub fn is_compatible_with(self, expected: Self) -> bool {
        self.major == expected.major
    }
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

/// The version this build speaks. Version 1 is the first named contract; it
/// replaces the unversioned positional tuples that preceded it. Minor 1 adds
/// [`device_run::DeviceRunSnapshot`], the delta the runtime publishes while a
/// device run is going — an addition, so a 1.0 client stays served. Minor 2
/// adds [`runtime::RuntimeSnapshot`], the whole-state payload the handshake
/// returns; also additive. Minor 3 adds the queue commands a full queue view
/// needs — move, remove, jump, purge — again additive: an older runtime
/// simply has no method for them. Minor 4 adds
/// [`playback::ExternalMedia`], so a stream or a podcast episode can be
/// played by the same runtime that owns the queue — again additive.
pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion { major: 1, minor: 4 };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_minor_of_the_same_major_stays_compatible() {
        let client = ProtocolVersion { major: 1, minor: 2 };
        assert!(ProtocolVersion { major: 1, minor: 2 }.is_compatible_with(client));
        assert!(ProtocolVersion { major: 1, minor: 7 }.is_compatible_with(client));
    }

    /// The case the strict form got wrong: a client updated on disk while the
    /// older runtime is still running. A minor bump only adds optional
    /// fields, so the older peer is perfectly usable and refusing it would
    /// break an ordinary upgrade.
    #[test]
    fn an_older_minor_is_served_rather_than_refused() {
        let client = ProtocolVersion { major: 1, minor: 2 };
        assert!(ProtocolVersion { major: 1, minor: 1 }.is_compatible_with(client));
        assert!(ProtocolVersion { major: 1, minor: 0 }.is_compatible_with(client));
    }

    #[test]
    fn a_foreign_major_is_refused() {
        let client = ProtocolVersion { major: 1, minor: 2 };
        assert!(!ProtocolVersion { major: 2, minor: 2 }.is_compatible_with(client));
        assert!(!ProtocolVersion { major: 0, minor: 9 }.is_compatible_with(client));
    }

    #[test]
    fn the_shipped_version_is_compatible_with_itself() {
        assert!(PROTOCOL_VERSION.is_compatible_with(PROTOCOL_VERSION));
        assert_eq!(PROTOCOL_VERSION.to_string(), "1.4");
    }
}
