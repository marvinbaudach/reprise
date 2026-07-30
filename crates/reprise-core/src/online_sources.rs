//! The global network gate (`NET-1a`).
//!
//! `online-sources-enabled` is a single persisted switch that sits above
//! every per-feature module flag. It is the one authority for "is Reprise
//! allowed to make a network request right now" — every network entry
//! point (podcast/YouTube refresh and download, radio search and play
//! clicks, cover downloads, artist portraits, online lyrics, New Releases,
//! Concerts) must AND its own module flag with [`is_enabled`], ideally via
//! [`network_allowed`], rather than checking its module flag alone.
//!
//! Turning this off does not delete anything: subscriptions, favorites, and
//! already-cached files are untouched. It only stops new requests.

use crate::db::Db;
use crate::library::settings;
use crate::modules::{self, ModuleDescriptor};

/// Settings key. Deliberately not namespaced under `module.*.enabled` —
/// this is not a module, it is the gate that sits above all of them.
pub const ENABLED_KEY: &str = "online-sources-enabled";

/// Whether the global gate is on. Defaults to `true`: Reprise ships with
/// per-feature modules already opt-in (`NET-1a`), so the global gate does
/// not need to additionally default-deny.
pub fn is_enabled(db: &Db) -> Result<bool, rusqlite::Error> {
    let conn = db.conn();
    settings::get_bool_in(conn, ENABLED_KEY, true)
}

pub fn set_enabled(db: &crate::db::Db, value: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    settings::set_bool_in(conn, ENABLED_KEY, value)
}

/// The one authority for "may this module make a network request right
/// now" — ANDs the global gate with the module's own flag.
pub fn network_allowed(
    db: &crate::db::Db,
    module: &ModuleDescriptor,
) -> Result<bool, rusqlite::Error> {
    let conn = db.conn();
    network_allowed_in(conn, module)
}

pub(crate) fn network_allowed_in(
    conn: &rusqlite::Connection,
    module: &ModuleDescriptor,
) -> Result<bool, rusqlite::Error> {
    Ok(settings::get_bool_in(conn, ENABLED_KEY, true)? && modules::is_enabled_in(conn, module)?)
}

/// [`network_allowed`] with the read failure already decided: a module whose
/// state cannot be read counts as off.
///
/// Every frontend caller wanted exactly this and each wrote its own wrapper
/// with its own log message — four copies that were free to disagree about
/// the default. Off is the only defensible one: `NET-1a` promises a disabled
/// module makes no requests, and a database that cannot answer must not be
/// read as consent. The module names itself in the warning, so the message
/// can no longer drift from the module it describes.
pub fn network_allowed_or_off(db: &crate::db::Db, module: &ModuleDescriptor) -> bool {
    let conn = db.conn();
    network_allowed_in(conn, module).unwrap_or_else(|error| {
        tracing::warn!(
            %error,
            module = module.id,
            "could not read module state; treating the network as not allowed"
        );
        false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated_db() -> Db {
        Db::open_in_memory().unwrap()
    }

    #[test]
    fn net_1a_defaults_to_enabled() {
        let db = migrated_db();
        assert!(is_enabled(&db).unwrap());
    }

    #[test]
    fn net_1a_round_trips() {
        let db = migrated_db();
        set_enabled(&db, false).unwrap();
        assert!(!is_enabled(&db).unwrap());
        set_enabled(&db, true).unwrap();
        assert!(is_enabled(&db).unwrap());
    }

    #[test]
    fn net_1a_network_allowed_is_an_and_of_global_and_module() {
        let db = migrated_db();
        let module = &modules::COVER_DOWNLOAD_MODULE;

        // Neither the global gate nor the module is on by default for a
        // network module such as cover download.
        assert!(!network_allowed(&db, module).unwrap());

        modules::set_enabled(&db, module, true).unwrap();
        assert!(
            network_allowed(&db, module).unwrap(),
            "module on, global on (default) => allowed"
        );

        set_enabled(&db, false).unwrap();
        assert!(
            !network_allowed(&db, module).unwrap(),
            "module on, global off => blocked"
        );

        modules::set_enabled(&db, module, false).unwrap();
        assert!(
            !network_allowed(&db, module).unwrap(),
            "module off, global off => blocked"
        );

        set_enabled(&db, true).unwrap();
        assert!(
            !network_allowed(&db, module).unwrap(),
            "module off, global on => blocked"
        );
    }
}
