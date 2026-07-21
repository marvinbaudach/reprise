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

// D18: reads are on by default; a user may still revoke them explicitly.
const LIBRARY_READ_DEFAULT: bool = true;
// Beschluss 7 / D18: writes are fail-closed (off) by default.
const PLAYLIST_CREATE_DEFAULT: bool = false;

/// Whether the read surface is currently granted.
pub fn library_read_enabled(conn: &Connection) -> Result<bool, rusqlite::Error> {
    settings::get_bool(conn, CAP_LIBRARY_READ, LIBRARY_READ_DEFAULT)
}

/// Whether `playlist:create` is currently granted (the live setting value).
pub fn playlist_create_granted(conn: &Connection) -> Result<bool, rusqlite::Error> {
    settings::get_bool(conn, CAP_PLAYLIST_CREATE, PLAYLIST_CREATE_DEFAULT)
}

/// Whether a write is permitted right now, given the startup snapshot.
///
/// `effective = granted_at_startup && currently_granted`:
/// - revoking mid-session flips `currently_granted` to `false`, so the next
///   call is refused immediately;
/// - granting mid-session leaves `granted_at_startup == false`, so the write
///   stays refused until the server restarts (spec D18: a client never gains
///   a new tool mid-session).
pub fn write_effective(
    conn: &Connection,
    granted_at_startup: bool,
) -> Result<bool, rusqlite::Error> {
    Ok(granted_at_startup && playlist_create_granted(conn)?)
}
