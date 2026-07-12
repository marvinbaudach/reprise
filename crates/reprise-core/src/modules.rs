//! Module registry substrate (spec: "Internes Modulsystem", stage 5). This
//! is deliberately the *data* half only: which optional features exist,
//! their UI-facing name/description (the future Plugins page renders exactly
//! this list), and a persisted on/off flag per module in the `settings`
//! table. The behavioral half — a `Module` trait with start/stop lifecycle
//! and extension points (sidebar entries, settings pages, pipeline elements)
//! — is intentionally NOT here yet: it gets designed in stage 5 against its
//! first two real implementors (equalizer, ReplayGain), not speculated
//! against one. Until the Plugins UI exists, toggling a flag takes effect on
//! the next launch.

use rusqlite::Connection;

use crate::library::settings;

pub struct ModuleDescriptor {
    /// Stable machine id; forms the settings key `module.<id>.enabled`.
    pub id: &'static str,
    /// UI display name (the Plugins list, spec stage 5).
    pub name: &'static str,
    pub description: &'static str,
    /// Flag value when the settings table has no row for this module.
    pub default_enabled: bool,
}

pub const MPRIS_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "mpris",
    name: "MPRIS",
    description: "GNOME media controls, media keys, and lock-screen integration (D-Bus)",
    default_enabled: true,
};

/// Every module the app knows about, in the order the Plugins page will show
/// them. Stage 5 appends equalizer and ReplayGain here.
pub const ALL_MODULES: &[&ModuleDescriptor] = &[&MPRIS_MODULE];

pub(crate) fn enabled_key(module: &ModuleDescriptor) -> String {
    format!("module.{}.enabled", module.id)
}

pub fn is_enabled(conn: &Connection, module: &ModuleDescriptor) -> Result<bool, rusqlite::Error> {
    settings::get_bool(conn, &enabled_key(module), module.default_enabled)
}

pub fn set_enabled(
    conn: &Connection,
    module: &ModuleDescriptor,
    value: bool,
) -> Result<(), rusqlite::Error> {
    settings::set_bool(conn, &enabled_key(module), value)
}

#[cfg(test)]
mod tests {
    use super::*;

    use rusqlite::Connection;

    fn migrated_conn() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn modules_default_to_their_declared_default() {
        let conn = migrated_conn();
        assert!(is_enabled(&conn, &MPRIS_MODULE).unwrap()); // default_enabled: true
    }

    #[test]
    fn set_enabled_persists_and_round_trips() {
        let conn = migrated_conn();
        set_enabled(&conn, &MPRIS_MODULE, false).unwrap();
        assert!(!is_enabled(&conn, &MPRIS_MODULE).unwrap());
        set_enabled(&conn, &MPRIS_MODULE, true).unwrap();
        assert!(is_enabled(&conn, &MPRIS_MODULE).unwrap());
    }

    #[test]
    fn enabled_key_is_namespaced_per_module() {
        assert_eq!(enabled_key(&MPRIS_MODULE), "module.mpris.enabled");
    }

    #[test]
    fn all_modules_lists_mpris() {
        assert!(ALL_MODULES.iter().any(|m| m.id == "mpris"));
    }
}
