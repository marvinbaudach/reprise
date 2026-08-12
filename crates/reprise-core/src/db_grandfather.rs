//! Network-feature grandfathering — the transactional Rust body of schema v16
//! and the global online-source gate introduced in schema v49.
//!
//! Pre-existing databases opt in to the modules whose behavior they already
//! received before it moved behind a module flag: online lyrics, New Releases
//! (inherited from the retired Artist News opt-in), and — when the on-disk
//! caches hold real downloads — cover and portrait fetching. `INSERT OR IGNORE`
//! only fills missing keys, so an explicit current setting always wins. Schema
//! v49 applies the same evidence rule to the global gate: fresh databases and
//! existing databases without positive use start off, while existing
//! subscriptions, radio favourites, downloads, or image caches keep it on.
//! Kept in its own file so `db.rs` stays under the architecture size gate; it
//! is called only from `db::migrate` (see that function's v16 and v49 steps).

use std::path::Path;

pub(crate) const LEGACY_COVER_DOWNLOAD_KEY: &str = "module.cover_download.enabled";
pub(crate) const LEGACY_ARTIST_PORTRAITS_KEY: &str = "module.artist_portraits.enabled";
pub(crate) const LEGACY_SOURCE_IMAGES_KEY: &str = "module.source_images.enabled";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NetworkUseEvidence {
    subscription: bool,
    radio_favourite: bool,
    downloaded_episode: bool,
    cover_cache: bool,
    portrait_cache: bool,
    /// Any module that reaches the network is explicitly switched on. This is
    /// the broadest signal and the only one that covers the features which
    /// leave no other trace in the database — Concerts and New Releases fetch
    /// on demand and cache nothing a data probe could find.
    online_module_enabled: bool,
}

fn online_gate_default(existing_database: bool, evidence: NetworkUseEvidence) -> bool {
    existing_database
        && (evidence.subscription
            || evidence.radio_favourite
            || evidence.downloaded_episode
            || evidence.cover_cache
            || evidence.portrait_cache
            || evidence.online_module_enabled)
}

/// `EXISTS(...)` over the `module.<id>.enabled` keys of every network-reaching
/// module. Deliberately not a `LIKE 'module.%.enabled'` pattern: the two local
/// modules default to on, so a pattern match would report every database as
/// having used online features.
fn any_online_module_enabled(tx: &rusqlite::Transaction<'_>) -> Result<bool, rusqlite::Error> {
    let mut keys: Vec<String> = crate::modules::ONLINE_MODULES
        .iter()
        .map(|module| crate::modules::enabled_key(module))
        .collect();
    keys.extend(
        [
            LEGACY_COVER_DOWNLOAD_KEY,
            LEGACY_ARTIST_PORTRAITS_KEY,
            LEGACY_SOURCE_IMAGES_KEY,
        ]
        .into_iter()
        .map(str::to_owned),
    );
    let placeholders = vec!["?"; keys.len()].join(", ");
    let sql = format!(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE value = '1' AND key IN ({placeholders}))"
    );
    tx.query_row(&sql, rusqlite::params_from_iter(keys), |row| row.get(0))
}

pub(crate) fn grandfather_network_features(
    tx: &rusqlite::Transaction<'_>,
    existing_database: bool,
    cover_cache: &Path,
    portrait_cache: &Path,
) -> Result<(), rusqlite::Error> {
    if existing_database {
        enable_module_if_unset(tx, &crate::modules::ONLINE_LYRICS_MODULE)?;
        tx.execute(
            "INSERT OR IGNORE INTO settings (key, value) \
             SELECT ?1, '1' \
             FROM settings \
             WHERE key = ?2 AND value = '1'",
            rusqlite::params![
                crate::modules::enabled_key(&crate::modules::NEW_RELEASES_MODULE),
                "module.artist_news.enabled"
            ],
        )?;
    }
    if existing_database && cache_contains_image(cover_cache, crate::cover_download::IMAGE_EXTS) {
        enable_setting_if_unset(tx, LEGACY_COVER_DOWNLOAD_KEY)?;
    }
    if existing_database
        && cache_contains_image(portrait_cache, crate::artist_portrait::cache::IMAGE_EXTS)
    {
        enable_setting_if_unset(tx, LEGACY_ARTIST_PORTRAITS_KEY)?;
    }
    Ok(())
}

pub(crate) fn grandfather_online_sources_gate(
    tx: &rusqlite::Transaction<'_>,
    existing_database: bool,
    cover_cache: &Path,
    portrait_cache: &Path,
) -> Result<(), rusqlite::Error> {
    let (subscription, radio_favourite, downloaded_episode, explicit_gate) = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM podcast_subscriptions), \
                EXISTS(SELECT 1 FROM radio_stations), \
                EXISTS( \
                    SELECT 1 FROM podcast_episodes \
                    WHERE downloaded_path IS NOT NULL \
                ), \
                EXISTS( \
                    SELECT 1 FROM settings \
                    WHERE key = ?1 \
                )",
        [crate::online_sources::ENABLED_KEY],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let evidence = NetworkUseEvidence {
        subscription,
        radio_favourite,
        downloaded_episode,
        cover_cache: cache_contains_image(cover_cache, crate::cover_download::IMAGE_EXTS),
        portrait_cache: cache_contains_image(
            portrait_cache,
            crate::artist_portrait::cache::IMAGE_EXTS,
        ),
        online_module_enabled: any_online_module_enabled(tx)?,
    };
    let value = if online_gate_default(existing_database, evidence) {
        "1"
    } else {
        "0"
    };
    tx.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![crate::online_sources::ENABLED_KEY, value],
    )?;
    if existing_database && (explicit_gate || value == "1") {
        tx.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, '1')",
            [crate::library::settings::ONLINE_SOURCES_FIRST_ENABLE_COMPLETED_KEY],
        )?;
    }
    Ok(())
}

fn enable_module_if_unset(
    tx: &rusqlite::Transaction<'_>,
    module: &crate::modules::ModuleDescriptor,
) -> Result<(), rusqlite::Error> {
    tx.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, '1')",
        [crate::modules::enabled_key(module)],
    )?;
    Ok(())
}

fn enable_setting_if_unset(
    tx: &rusqlite::Transaction<'_>,
    key: &str,
) -> Result<(), rusqlite::Error> {
    tx.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, '1')",
        [key],
    )?;
    Ok(())
}

fn cache_contains_image(directory: &Path, extensions: &[&str]) -> bool {
    std::fs::read_dir(directory).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        extensions
                            .iter()
                            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
                    })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_gate_each_positive_signal_is_demonstrable_use_only_for_an_existing_database() {
        for evidence in [
            NetworkUseEvidence {
                subscription: true,
                ..NetworkUseEvidence::default()
            },
            NetworkUseEvidence {
                radio_favourite: true,
                ..NetworkUseEvidence::default()
            },
            NetworkUseEvidence {
                downloaded_episode: true,
                ..NetworkUseEvidence::default()
            },
            NetworkUseEvidence {
                cover_cache: true,
                ..NetworkUseEvidence::default()
            },
            NetworkUseEvidence {
                portrait_cache: true,
                ..NetworkUseEvidence::default()
            },
        ] {
            assert!(online_gate_default(true, evidence));
            assert!(!online_gate_default(false, evidence));
        }
        assert!(!online_gate_default(true, NetworkUseEvidence::default()));
    }
}
