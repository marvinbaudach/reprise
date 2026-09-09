//! Where the app answers on MPRIS, and what "it is not running" looks like.
//!
//! This is the same argument [`crate::endpoint`] makes for the Reprise runtime
//! interface, applied to the other direct-path D-Bus surface. An address is as
//! much a part of a contract as a field name: a client with the right calls and
//! the wrong bus name talks to nothing. `reprise-platform-linux` owns the
//! *server*, but a client must not depend on that crate merely to learn an
//! address — on Linux that would drag GStreamer into every surface that only
//! wanted to send a command. So the address lives here, with the wire
//! vocabulary, and `reprise-cli` and `reprise-mcp` reach it behind their
//! `mpris` features.
//!
//! Deliberately kept in its own module rather than folded into
//! [`crate::endpoint`]: that one names the Reprise runtime's own interface,
//! this one names MPRIS. Two protocols, two namespaces, so nobody reads
//! `BUS_NAME` and gets the wrong one.
//!
//! Until this module existed the three sites spelled these strings out
//! separately, each carrying a comment asking the reader to keep them equal.

/// The well-known name the app claims, following the MPRIS spec's
/// `org.mpris.MediaPlayer2.<name>` convention — GNOME Shell and friends
/// discover media players by enumerating names under that prefix.
pub const BUS_NAME: &str = "org.mpris.MediaPlayer2.reprise";

/// The object every MPRIS interface lives at. Fixed by the spec.
pub const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";

/// The player interface carrying the transport calls.
pub const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

/// Whether a D-Bus error name means no player is registered under our name —
/// that is, the app simply is not running — rather than a genuine fault.
///
/// The distinction decides whether a surface prints "Reprise is not running"
/// or reports a bus failure, so both clients need the same answer. `zbus`
/// hands the two names below when a call is made to a name nobody owns; every
/// other error name is a real problem and must not be swallowed.
#[must_use]
pub fn is_absent_player(error_name: &str) -> bool {
    matches!(
        error_name,
        "org.freedesktop.DBus.Error.ServiceUnknown" | "org.freedesktop.DBus.Error.NameHasNoOwner"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_player_names_are_the_two_unowned_name_errors() {
        assert!(is_absent_player(
            "org.freedesktop.DBus.Error.ServiceUnknown"
        ));
        assert!(is_absent_player(
            "org.freedesktop.DBus.Error.NameHasNoOwner"
        ));
    }

    #[test]
    fn a_genuine_fault_is_not_an_absent_player() {
        // Swallowing these as "not running" would hide a real failure.
        assert!(!is_absent_player("org.freedesktop.DBus.Error.AccessDenied"));
        assert!(!is_absent_player("org.freedesktop.DBus.Error.NoReply"));
        assert!(!is_absent_player("org.freedesktop.DBus.Error.Failed"));
        assert!(!is_absent_player(""));
    }

    #[test]
    fn the_bus_name_follows_the_mpris_prefix_convention() {
        // A name outside this prefix is invisible to every MPRIS client.
        assert!(BUS_NAME.starts_with("org.mpris.MediaPlayer2."));
        assert_eq!(OBJECT_PATH, "/org/mpris/MediaPlayer2");
        assert!(PLAYER_INTERFACE.starts_with("org.mpris.MediaPlayer2."));
    }
}
