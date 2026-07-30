//! Capability gating for MCP tools (Beschluss 7, spec D16/D18).
//!
//! Capabilities live as `agent.capability.*` settings keys, read through the
//! core [`settings`] facade. Per spec D18 the read surface is granted by
//! default (so the server is useful out of the box) while writes are
//! fail-closed. Every write call re-reads its capability, so a revocation
//! takes effect immediately; a fresh grant only takes effect after a restart,
//! enforced by combining the live read with a startup snapshot in
//! [`write_effective`].

use reprise_core::db::Db;
use reprise_core::library::settings;

/// Settings key granting the read surface (search + resources).
pub const CAP_LIBRARY_READ: &str = "agent.capability.library:read";
/// Settings key granting manual-playlist creation.
pub const CAP_PLAYLIST_CREATE: &str = "agent.capability.playlist:create";
/// Settings key granting non-destructive manual-playlist updates (rename and
/// append tracks; never remove/delete).
pub const CAP_PLAYLIST_MANAGE: &str = "agent.capability.playlist:manage";
/// Settings key granting instrumental (AI) creation (Beschluss 7).
pub const CAP_AI_CREATE: &str = "agent.capability.ai:create";
/// Settings key granting podcast, YouTube, and radio source mutations.
pub const CAP_SOURCES_MANAGE: &str = "agent.capability.sources:manage";
/// Settings key granting Android-device synchronization mutations.
#[cfg(feature = "mpris")]
pub const CAP_DEVICE_SYNC: &str = "agent.capability.device:sync";
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
const PLAYLIST_MANAGE_DEFAULT: bool = false;
// Beschluss 7: `ai:create` is fail-closed (off) by default, exactly like
// `playlist:create`.
const AI_CREATE_DEFAULT: bool = false;
const SOURCES_MANAGE_DEFAULT: bool = false;
#[cfg(feature = "mpris")]
const DEVICE_SYNC_DEFAULT: bool = false;
// Playback control starts audio but destroys no data — on by default, like the
// read surface, and revocable live.
#[cfg(feature = "mpris")]
const PLAYBACK_CONTROL_DEFAULT: bool = true;

/// Whether the read surface is currently granted.
pub fn library_read_enabled(db: &Db) -> Result<bool, rusqlite::Error> {
    settings::get_bool(db, CAP_LIBRARY_READ, LIBRARY_READ_DEFAULT)
}

/// Whether `playlist:create` is currently granted (the live setting value).
pub fn playlist_create_granted(db: &Db) -> Result<bool, rusqlite::Error> {
    settings::get_bool(db, CAP_PLAYLIST_CREATE, PLAYLIST_CREATE_DEFAULT)
}

/// Whether `playlist:manage` is currently granted (the live setting value).
pub fn playlist_manage_granted(db: &Db) -> Result<bool, rusqlite::Error> {
    settings::get_bool(db, CAP_PLAYLIST_MANAGE, PLAYLIST_MANAGE_DEFAULT)
}

/// Whether `ai:create` is currently granted (the live setting value).
pub fn ai_create_granted(db: &Db) -> Result<bool, rusqlite::Error> {
    settings::get_bool(db, CAP_AI_CREATE, AI_CREATE_DEFAULT)
}

/// Whether `sources:manage` is currently granted (the live setting value).
pub fn sources_manage_granted(db: &Db) -> Result<bool, rusqlite::Error> {
    settings::get_bool(db, CAP_SOURCES_MANAGE, SOURCES_MANAGE_DEFAULT)
}

/// Whether playback control is currently granted (live setting value).
#[cfg(feature = "mpris")]
pub fn playback_control_enabled(db: &Db) -> Result<bool, rusqlite::Error> {
    settings::get_bool(db, CAP_PLAYBACK_CONTROL, PLAYBACK_CONTROL_DEFAULT)
}

#[cfg(feature = "mpris")]
pub fn device_sync_granted(db: &Db) -> Result<bool, rusqlite::Error> {
    settings::get_bool(db, CAP_DEVICE_SYNC, DEVICE_SYNC_DEFAULT)
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
pub fn write_effective(db: &Db, granted_at_startup: bool) -> Result<bool, rusqlite::Error> {
    Ok(effective(granted_at_startup, playlist_create_granted(db)?))
}

/// Whether a `playlist:manage` write is permitted right now, given the startup
/// snapshot.
pub fn playlist_manage_effective(
    db: &Db,
    granted_at_startup: bool,
) -> Result<bool, rusqlite::Error> {
    Ok(effective(granted_at_startup, playlist_manage_granted(db)?))
}

/// Whether an `ai:create` write is permitted right now, given the startup
/// snapshot.
pub fn ai_create_effective(db: &Db, granted_at_startup: bool) -> Result<bool, rusqlite::Error> {
    Ok(effective(granted_at_startup, ai_create_granted(db)?))
}

/// Whether a podcast/YouTube/radio mutation is permitted right now, given the
/// startup snapshot.
pub fn sources_manage_effective(
    db: &Db,
    granted_at_startup: bool,
) -> Result<bool, rusqlite::Error> {
    Ok(effective(granted_at_startup, sources_manage_granted(db)?))
}

#[cfg(feature = "mpris")]
pub fn device_sync_effective(db: &Db, granted_at_startup: bool) -> Result<bool, rusqlite::Error> {
    Ok(effective(granted_at_startup, device_sync_granted(db)?))
}

#[cfg(all(test, feature = "mpris"))]
mod tests {
    use super::*;

    #[test]
    fn playback_control_defaults_on_and_honors_revocation() {
        let db = reprise_core::db::Db::open_in_memory().unwrap();
        assert!(playback_control_enabled(&db).unwrap());
        reprise_core::library::settings::set_bool(&db, CAP_PLAYBACK_CONTROL, false).unwrap();
        assert!(!playback_control_enabled(&db).unwrap());
    }

    #[test]
    fn device_sync_is_fail_closed_and_restart_gated() {
        let db = reprise_core::db::Db::open_in_memory().unwrap();
        assert!(!device_sync_granted(&db).unwrap());
        reprise_core::library::settings::set_bool(&db, CAP_DEVICE_SYNC, true).unwrap();
        assert!(!device_sync_effective(&db, false).unwrap());
        assert!(device_sync_effective(&db, true).unwrap());
    }
}
