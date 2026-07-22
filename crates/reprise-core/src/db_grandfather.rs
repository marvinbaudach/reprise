//! Network-feature grandfathering — the transactional Rust body of schema v16.
//!
//! Pre-existing databases opt in to the modules whose behavior they already
//! received before it moved behind a module flag: online lyrics, New Releases
//! (inherited from the retired Artist News opt-in), and — when the on-disk
//! caches hold real downloads — cover and portrait fetching. `INSERT OR IGNORE`
//! only fills missing keys, so an explicit current setting always wins. Kept in
//! its own file so `db.rs` stays under the architecture size gate; it is called
//! only from `db::migrate` (see that function's v16 step).

use std::path::Path;

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
