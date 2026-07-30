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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NetworkUseEvidence {
    subscription: bool,
    radio_favourite: bool,
    downloaded_episode: bool,
    cover_cache: bool,
    portrait_cache: bool,
}

fn online_gate_default(existing_database: bool, evidence: NetworkUseEvidence) -> bool {
    existing_database
        && (evidence.subscription
            || evidence.radio_favourite
            || evidence.downloaded_episode
            || evidence.cover_cache
            || evidence.portrait_cache)
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
        enable_module_if_unset(tx, &crate::modules::COVER_DOWNLOAD_MODULE)?;
    }
    if existing_database
        && cache_contains_image(portrait_cache, crate::artist_portrait::cache::IMAGE_EXTS)
    {
        enable_module_if_unset(tx, &crate::modules::ARTIST_PORTRAITS_MODULE)?;
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
