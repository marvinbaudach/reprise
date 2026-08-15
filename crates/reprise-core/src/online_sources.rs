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

/// The owner of a network request checked by [`network_allowed`].
#[derive(Clone, Copy)]
pub enum NetworkScope<'a> {
    /// App state shared across plugins, with no per-module switch of its own.
    AppWide,
    /// A request owned by one optional module.
    Module(&'a ModuleDescriptor),
}

impl<'a> From<&'a ModuleDescriptor> for NetworkScope<'a> {
    fn from(module: &'a ModuleDescriptor) -> Self {
        Self::Module(module)
    }
}

/// Settings key. Deliberately not namespaced under `module.*.enabled` —
/// this is not a module, it is the gate that sits above all of them.
pub const ENABLED_KEY: &str = "online-sources-enabled";

/// Whether the global gate is on. A missing or unreadable value defaults off:
/// network access needs an affirmative persisted opt-in. Schema v49 writes the
/// value for every database while preserving explicit choices (`NET-2a`).
pub fn is_enabled(db: &Db) -> Result<bool, rusqlite::Error> {
    let conn = db.conn();
    settings::get_bool_in(conn, ENABLED_KEY, false)
}

pub fn set_enabled(db: &crate::db::Db, value: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    crate::events::in_txn_immediate(conn, |conn| {
        let current = settings::get_bool_in(conn, ENABLED_KEY, false)?;
        let first_enable_completed = settings::get_bool_in(
            conn,
            settings::ONLINE_SOURCES_FIRST_ENABLE_COMPLETED_KEY,
            false,
        )?;
        if !first_enable_completed {
            if !current && value {
                for (module, enabled) in first_enable_source_defaults() {
                    let key = modules::enabled_key(module);
                    // Only seed a module that has never been decided. A stored
                    // value is the user's own choice from before this one-shot
                    // existed, and a first enable must not silently discard it
                    // — "turning this off does not delete anything" has to hold
                    // for preferences too, not just for files.
                    if settings::get_setting_in(conn, &key)?.is_none() {
                        settings::set_bool_in(conn, &key, enabled)?;
                    }
                }
                settings::set_bool_in(
                    conn,
                    settings::ONLINE_SOURCES_FIRST_ENABLE_COMPLETED_KEY,
                    true,
                )?;
            } else if current {
                // Databases upgraded with the master already on have enabled
                // online sources before this one-shot existed. Mark that
                // history before a later off/on cycle can be mistaken for a
                // first enable and overwrite their module choices.
                settings::set_bool_in(
                    conn,
                    settings::ONLINE_SOURCES_FIRST_ENABLE_COMPLETED_KEY,
                    true,
                )?;
            }
        }
        settings::set_bool_in(conn, ENABLED_KEY, value)
    })
}

fn first_enable_source_defaults() -> [(&'static ModuleDescriptor, bool); 7] {
    [
        (&modules::NEW_RELEASES_MODULE, false),
        (&modules::CONCERTS_MODULE, false),
        (&modules::PODCASTS_MODULE, false),
        (&modules::YOUTUBE_MODULE, false),
        (&modules::RADIO_MODULE, true),
        (&modules::ARTWORK_MODULE, false),
        (&modules::ONLINE_LYRICS_MODULE, false),
    ]
}

/// The state a first enable would write for one source, so a surface can
/// *display* the rule instead of restating it. Unknown modules answer `false`:
/// a source nobody seeded is not one the app turns on by itself.
///
/// Compared by `id`: `ModuleDescriptor`s are `const`, so two references to the
/// same module need not be the same pointer.
pub fn first_enable_default_for(module: &ModuleDescriptor) -> bool {
    first_enable_source_defaults()
        .into_iter()
        .find(|(candidate, _)| candidate.id == module.id)
        .is_some_and(|(_, enabled)| enabled)
}

/// The three sources the first-run wizard offers, in display order.
pub const WIZARD_SOURCE_MODULES: [&ModuleDescriptor; 3] = [
    &modules::RADIO_MODULE,
    &modules::PODCASTS_MODULE,
    &modules::YOUTUBE_MODULE,
];

/// What the wizard's three switches say. Not a settings snapshot — the
/// user's answer, before anything is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WizardSourceSelection {
    pub radio: bool,
    pub podcasts: bool,
    pub youtube: bool,
}

impl WizardSourceSelection {
    /// The state a fresh install opens in: exactly what a first enable
    /// would write, read from the table rather than repeated here.
    pub fn from_first_enable_defaults() -> Self {
        Self {
            radio: first_enable_default_for(&modules::RADIO_MODULE),
            podcasts: first_enable_default_for(&modules::PODCASTS_MODULE),
            youtube: first_enable_default_for(&modules::YOUTUBE_MODULE),
        }
    }

    /// What the wizard must show. The gate being on means somebody already
    /// answered this question — Preferences, or an earlier session — and the
    /// wizard has to display that answer instead of overwriting it with the
    /// first-enable defaults. Reachable: closing the dialog with Escape writes
    /// nothing, so the wizard returns after the banner has been used.
    pub fn current_or_first_enable_defaults(db: &Db) -> Result<Self, rusqlite::Error> {
        if !is_enabled(db)? {
            return Ok(Self::from_first_enable_defaults());
        }
        Ok(Self {
            radio: modules::is_enabled(db, &modules::RADIO_MODULE)?,
            podcasts: modules::is_enabled(db, &modules::PODCASTS_MODULE)?,
            youtube: modules::is_enabled(db, &modules::YOUTUBE_MODULE)?,
        })
    }

    /// `NET-1a`: no source chosen means the gate stays shut. Turning the
    /// gate on "just in case" would make the app network-capable without
    /// anyone asking for it.
    pub fn opens_the_gate(self) -> bool {
        self.radio || self.podcasts || self.youtube
    }

    /// The module writes this selection implies, in write order.
    pub fn module_writes(self) -> [(&'static ModuleDescriptor, bool); 3] {
        [
            (&modules::RADIO_MODULE, self.radio),
            (&modules::PODCASTS_MODULE, self.podcasts),
            (&modules::YOUTUBE_MODULE, self.youtube),
        ]
    }
}

/// Applies the wizard's answer. The gate goes first so its one-shot seeding
/// runs before the explicit choices land on top of it; no source chosen
/// leaves the gate — and every module — untouched.
pub fn apply_wizard_selection(
    db: &Db,
    selection: WizardSourceSelection,
) -> Result<(), rusqlite::Error> {
    if !selection.opens_the_gate() {
        return Ok(());
    }
    set_enabled(db, true)?;
    for (module, enabled) in selection.module_writes() {
        modules::set_enabled(db, module, enabled)?;
    }
    Ok(())
}

/// The one authority for "may this request run right now". Module-owned
/// requests AND the global gate with their module flag; app-wide requests
/// with no plugin owner use the global gate alone.
pub fn network_allowed<'a>(
    db: &crate::db::Db,
    scope: impl Into<NetworkScope<'a>>,
) -> Result<bool, rusqlite::Error> {
    match scope.into() {
        NetworkScope::AppWide => is_enabled(db),
        NetworkScope::Module(module) => network_allowed_in(db.conn(), module),
    }
}

pub(crate) fn network_allowed_in(
    conn: &rusqlite::Connection,
    module: &ModuleDescriptor,
) -> Result<bool, rusqlite::Error> {
    Ok(settings::get_bool_in(conn, ENABLED_KEY, false)? && modules::is_enabled_in(conn, module)?)
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
    fn first_enable_defaults_are_readable_without_restating_them() {
        assert!(first_enable_default_for(&modules::RADIO_MODULE));
        assert!(!first_enable_default_for(&modules::PODCASTS_MODULE));
        assert!(!first_enable_default_for(&modules::YOUTUBE_MODULE));
        // A module outside the table answers off, not "unknown".
        assert!(!first_enable_default_for(&modules::SONG_VISUALS_MODULE));
    }

    #[test]
    fn a_fresh_install_opens_the_wizard_with_the_first_enable_defaults() {
        let db = migrated_db();
        let selection = WizardSourceSelection::current_or_first_enable_defaults(&db).unwrap();
        assert!(selection.radio);
        assert!(!selection.podcasts);
        assert!(!selection.youtube);
    }

    #[test]
    fn an_open_gate_makes_the_wizard_show_what_is_stored() {
        // The reachable path: Escape wrote nothing, the banner sent the user to
        // Preferences, and there they chose Podcasts on / Radio off — the inverse
        // of the first-enable defaults, so a wizard that ignored them is visible.
        let db = migrated_db();
        set_enabled(&db, true).unwrap();
        modules::set_enabled(&db, &modules::RADIO_MODULE, false).unwrap();
        modules::set_enabled(&db, &modules::PODCASTS_MODULE, true).unwrap();

        let selection = WizardSourceSelection::current_or_first_enable_defaults(&db).unwrap();
        assert!(!selection.radio);
        assert!(selection.podcasts);
        assert!(!selection.youtube);
    }

    #[test]
    fn completing_the_wizard_unchanged_keeps_the_stored_choice() {
        let db = migrated_db();
        set_enabled(&db, true).unwrap();
        modules::set_enabled(&db, &modules::RADIO_MODULE, false).unwrap();
        modules::set_enabled(&db, &modules::PODCASTS_MODULE, true).unwrap();

        let selection = WizardSourceSelection::current_or_first_enable_defaults(&db).unwrap();
        apply_wizard_selection(&db, selection).unwrap();

        assert!(is_enabled(&db).unwrap());
        assert!(!modules::is_enabled(&db, &modules::RADIO_MODULE).unwrap());
        assert!(modules::is_enabled(&db, &modules::PODCASTS_MODULE).unwrap());
        assert!(!modules::is_enabled(&db, &modules::YOUTUBE_MODULE).unwrap());
    }

    #[test]
    fn no_source_chosen_leaves_the_gate_shut() {
        let db = migrated_db();
        apply_wizard_selection(&db, WizardSourceSelection::default()).unwrap();
        assert!(!is_enabled(&db).unwrap());
        assert!(
            settings::get_setting_in(db.conn(), &modules::enabled_key(&modules::RADIO_MODULE))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn the_wizard_selection_survives_the_first_enable_seeding() {
        // Radio off, Podcasts on — the inverse of the seed, so a seed that won
        // would be visible.
        let db = migrated_db();
        apply_wizard_selection(
            &db,
            WizardSourceSelection {
                radio: false,
                podcasts: true,
                youtube: false,
            },
        )
        .unwrap();

        assert!(is_enabled(&db).unwrap());
        assert!(!modules::is_enabled(&db, &modules::RADIO_MODULE).unwrap());
        assert!(modules::is_enabled(&db, &modules::PODCASTS_MODULE).unwrap());
        assert!(!modules::is_enabled(&db, &modules::YOUTUBE_MODULE).unwrap());
        // The four sources the wizard never mentions keep their first-enable
        // defaults — the wizard adds a question, it does not answer theirs.
        for module in [
            &modules::NEW_RELEASES_MODULE,
            &modules::CONCERTS_MODULE,
            &modules::ARTWORK_MODULE,
            &modules::ONLINE_LYRICS_MODULE,
        ] {
            assert!(!modules::is_enabled(&db, module).unwrap(), "{}", module.id);
        }
    }

    #[test]
    fn online_gate_fresh_database_defaults_to_disabled() {
        let db = migrated_db();
        assert!(!is_enabled(&db).unwrap());
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
        let module = &modules::ARTWORK_MODULE;

        // Neither the global gate nor the module is on by default for a
        // network module such as cover download.
        assert!(!network_allowed(&db, module).unwrap());

        set_enabled(&db, true).unwrap();
        modules::set_enabled(&db, module, true).unwrap();
        assert!(
            network_allowed(&db, module).unwrap(),
            "module on, global on => allowed"
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

    #[test]
    fn net_1a_app_wide_requests_follow_the_master_without_a_plugin_owner() {
        let db = migrated_db();
        set_enabled(&db, true).unwrap();
        modules::set_enabled(&db, &modules::RADIO_MODULE, false).unwrap();
        modules::set_enabled(&db, &modules::CONCERTS_MODULE, false).unwrap();
        modules::set_enabled(&db, &modules::PODCASTS_MODULE, false).unwrap();

        assert!(network_allowed(&db, NetworkScope::AppWide).unwrap());

        set_enabled(&db, false).unwrap();
        assert!(!network_allowed(&db, NetworkScope::AppWide).unwrap());
    }

    #[test]
    fn first_enable_turns_every_online_source_off_except_radio() {
        // A fresh install: nothing has been decided yet, so the first enable
        // writes the defaults down. This deliberately does NOT pre-set the
        // modules — a stored value is the user's own choice and survives
        // (`net_2a_a_first_enable_never_overwrites_a_module_the_user_already_decided`).
        let db = migrated_db();
        db.conn()
            .execute(
                "DELETE FROM settings WHERE key = ?1",
                [modules::enabled_key(&modules::ARTWORK_MODULE)],
            )
            .unwrap();
        for module in modules::ONLINE_MODULES {
            assert!(
                settings::get_setting_in(db.conn(), &modules::enabled_key(module))
                    .unwrap()
                    .is_none(),
                "{} was already decided before the first enable",
                module.id
            );
        }

        set_enabled(&db, true).unwrap();

        // Every source is written down explicitly, not merely left at its
        // compiled-in default — otherwise the one-shot would be unobservable.
        for module in [
            &modules::NEW_RELEASES_MODULE,
            &modules::CONCERTS_MODULE,
            &modules::PODCASTS_MODULE,
            &modules::YOUTUBE_MODULE,
            &modules::RADIO_MODULE,
            &modules::ARTWORK_MODULE,
            &modules::ONLINE_LYRICS_MODULE,
        ] {
            assert!(
                settings::get_setting_in(db.conn(), &modules::enabled_key(module))
                    .unwrap()
                    .is_some(),
                "{} was not seeded by the first enable",
                module.id
            );
        }

        assert!(is_enabled(&db).unwrap());
        assert!(modules::is_enabled(&db, &modules::RADIO_MODULE).unwrap());
        for module in [
            &modules::NEW_RELEASES_MODULE,
            &modules::CONCERTS_MODULE,
            &modules::PODCASTS_MODULE,
            &modules::YOUTUBE_MODULE,
            &modules::ARTWORK_MODULE,
            &modules::ONLINE_LYRICS_MODULE,
        ] {
            assert!(
                !modules::is_enabled(&db, module).unwrap(),
                "{} stayed enabled",
                module.id
            );
        }
        assert!(settings::get_bool(
            &db,
            settings::ONLINE_SOURCES_FIRST_ENABLE_COMPLETED_KEY,
            false
        )
        .unwrap());
    }

    #[test]
    fn later_master_toggles_preserve_every_per_source_choice() {
        let db = migrated_db();
        set_enabled(&db, true).unwrap();
        modules::set_enabled(&db, &modules::PODCASTS_MODULE, true).unwrap();
        modules::set_enabled(&db, &modules::RADIO_MODULE, false).unwrap();

        set_enabled(&db, false).unwrap();
        set_enabled(&db, true).unwrap();

        assert!(modules::is_enabled(&db, &modules::PODCASTS_MODULE).unwrap());
        assert!(!modules::is_enabled(&db, &modules::RADIO_MODULE).unwrap());
    }

    #[test]
    fn net_2a_a_first_enable_never_overwrites_a_module_the_user_already_decided() {
        // The upgrade path that used to lose data: someone had Concerts on,
        // the migration found no other trace of online use and left the gate
        // off, and their next flip of the master switch was mistaken for a
        // first enable — which reset every module, Concerts included.
        let db = migrated_db();
        modules::set_enabled(&db, &modules::CONCERTS_MODULE, true).unwrap();
        modules::set_enabled(&db, &modules::PODCASTS_MODULE, false).unwrap();
        settings::set_bool(&db, ENABLED_KEY, false).unwrap();

        set_enabled(&db, true).unwrap();

        assert!(modules::is_enabled(&db, &modules::CONCERTS_MODULE).unwrap());
        assert!(!modules::is_enabled(&db, &modules::PODCASTS_MODULE).unwrap());
        // Untouched sources still get their first-enable default, and Radio is
        // the one that may run because it only reaches the network on a click.
        assert!(modules::is_enabled(&db, &modules::RADIO_MODULE).unwrap());
        assert!(!modules::is_enabled(&db, &modules::YOUTUBE_MODULE).unwrap());
    }

    #[test]
    fn an_already_enabled_database_never_reapplies_first_enable_defaults() {
        let db = migrated_db();
        settings::set_bool(&db, ENABLED_KEY, true).unwrap();
        modules::set_enabled(&db, &modules::PODCASTS_MODULE, true).unwrap();
        modules::set_enabled(&db, &modules::RADIO_MODULE, false).unwrap();

        set_enabled(&db, false).unwrap();
        set_enabled(&db, true).unwrap();

        assert!(modules::is_enabled(&db, &modules::PODCASTS_MODULE).unwrap());
        assert!(!modules::is_enabled(&db, &modules::RADIO_MODULE).unwrap());
    }

    #[test]
    fn a_previously_used_database_that_is_currently_off_preserves_source_choices() {
        let db = migrated_db();
        settings::set_bool(
            &db,
            settings::ONLINE_SOURCES_FIRST_ENABLE_COMPLETED_KEY,
            true,
        )
        .unwrap();
        modules::set_enabled(&db, &modules::PODCASTS_MODULE, true).unwrap();
        modules::set_enabled(&db, &modules::RADIO_MODULE, false).unwrap();

        set_enabled(&db, true).unwrap();

        assert!(modules::is_enabled(&db, &modules::PODCASTS_MODULE).unwrap());
        assert!(!modules::is_enabled(&db, &modules::RADIO_MODULE).unwrap());
    }
}
