//! Capability gating for MCP tools (Beschluss 7, spec D16/D18).
//!
//! Capabilities live as `agent.capability.*` settings keys, read through the
//! core [`settings`] facade. Per spec D18 the read surface is granted by
//! default (so the server is useful out of the box) while writes are
//! fail-closed. Every write call re-reads its capability, so a revocation
//! takes effect immediately; a fresh grant only takes effect after a restart,
//! enforced by combining the live read with a startup snapshot in
//! [`write_effective`].

use reprise_core::library::settings;
use rusqlite::Connection;

/// Settings key granting the read surface (search + resources).
pub const CAP_LIBRARY_READ: &str = "agent.capability.library:read";
/// Settings key granting manual-playlist creation.
pub const CAP_PLAYLIST_CREATE: &str = "agent.capability.playlist:create";
/// Settings key granting instrumental (AI) creation (Beschluss 7).
pub const CAP_AI_CREATE: &str = "agent.capability.ai:create";
/// Settings key granting playback control (transport + targeted play).
/// Consumed by the `music_playback_control`/`music_play` tools, which only
/// exist under the `mpris` feature — gated to match, so a plain (non-`mpris`)
/// build carries no unreachable playback-only surface.
#[cfg(feature = "mpris")]
pub const CAP_PLAYBACK_CONTROL: &str = "agent.capability.playback:control";

// D18: reads are on by default; a user may still revoke them explicitly.
const LIBRARY_READ_DEFAULT: bool = true;
// Beschluss 7 / D18: writes are fail-closed (off) by default.
const PLAYLIST_CREATE_DEFAULT: bool = false;
// Beschluss 7: `ai:create` is fail-closed (off) by default, exactly like
// `playlist:create`.
const AI_CREATE_DEFAULT: bool = false;
// Playback control starts audio but destroys no data — on by default, like the
// read surface, and revocable live.
#[cfg(feature = "mpris")]
const PLAYBACK_CONTROL_DEFAULT: bool = true;

/// Whether the read surface is currently granted.
pub fn library_read_enabled(conn: &Connection) -> Result<bool, rusqlite::Error> {
    settings::get_bool(conn, CAP_LIBRARY_READ, LIBRARY_READ_DEFAULT)
}

/// Whether `playlist:create` is currently granted (the live setting value).
pub fn playlist_create_granted(conn: &Connection) -> Result<bool, rusqlite::Error> {
    settings::get_bool(conn, CAP_PLAYLIST_CREATE, PLAYLIST_CREATE_DEFAULT)
}

/// Whether `ai:create` is currently granted (the live setting value).
pub fn ai_create_granted(conn: &Connection) -> Result<bool, rusqlite::Error> {
    settings::get_bool(conn, CAP_AI_CREATE, AI_CREATE_DEFAULT)
}

/// Whether playback control is currently granted (live setting value).
#[cfg(feature = "mpris")]
pub fn playback_control_enabled(conn: &Connection) -> Result<bool, rusqlite::Error> {
    settings::get_bool(conn, CAP_PLAYBACK_CONTROL, PLAYBACK_CONTROL_DEFAULT)
}

/// Combines a startup snapshot with the live setting value (spec D18 / Beschluss
/// 7): a write-class call is permitted only when the capability was granted at
/// startup **and** is still granted right now.
///
/// - revoking mid-session flips the live value to `false`, so the next call is
///   refused immediately;
/// - granting mid-session leaves the startup snapshot `false`, so the call
///   stays refused until the server restarts (a client never gains a new tool
///   mid-session).
fn effective(granted_at_startup: bool, currently_granted: bool) -> bool {
    granted_at_startup && currently_granted
}

/// Whether a `playlist:create` write is permitted right now, given the startup
/// snapshot.
pub fn write_effective(
    conn: &Connection,
    granted_at_startup: bool,
) -> Result<bool, rusqlite::Error> {
    Ok(effective(
        granted_at_startup,
        playlist_create_granted(conn)?,
    ))
}

/// Whether an `ai:create` write is permitted right now, given the startup
/// snapshot.
pub fn ai_create_effective(
    conn: &Connection,
    granted_at_startup: bool,
) -> Result<bool, rusqlite::Error> {
    Ok(effective(granted_at_startup, ai_create_granted(conn)?))
}

#[cfg(all(test, feature = "mpris"))]
mod tests {
    use super::*;

    #[test]
    fn playback_control_defaults_on_and_honors_revocation() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        assert!(playback_control_enabled(&conn).unwrap());
        reprise_core::library::settings::set_bool(&conn, CAP_PLAYBACK_CONTROL, false).unwrap();
        assert!(!playback_control_enabled(&conn).unwrap());
    }
}
