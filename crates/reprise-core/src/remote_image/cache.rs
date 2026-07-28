//! Bounded on-disk cache for remote source images (`C1`).
//!
//! Positive entries are `<key>.<ext>` files under the XDG cache. Unlike the
//! permanent, unbounded album-cover download cache (`cover_download`), this
//! cache is capped at [`MAX_CACHE_ENTRIES`] files: once a new write would
//! exceed the cap, the least-recently-modified files are evicted first.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub(crate) const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

/// Hard cap on the number of cached source-image files. Channel, show, and
/// station artwork is small and there are only ever as many distinct images
/// as there are subscriptions/favorites plus a handful of recent search
/// results — 300 files is generous headroom while keeping the cache
/// genuinely bounded, which the plan requires and the permanent cover-art
/// cache deliberately does not do.
pub(crate) const MAX_CACHE_ENTRIES: usize = 300;

pub(crate) fn cache_dir() -> PathBuf {
    crate::cover::cache_dir().join("remote-images")
}

pub(crate) fn key_for(url: &str) -> String {
    crate::cover::hash_hex(url.trim().as_bytes())
}

pub(crate) fn cached_path_in(dir: &Path, url: &str) -> Option<PathBuf> {
    let key = key_for(url);
    IMAGE_EXTS
        .iter()
        .map(|ext| dir.join(format!("{key}.{ext}")))
        .find(|path| path.exists())
}

/// Writes `bytes` atomically (temp file + rename), replaces any stale
/// extension for the same key, then enforces [`MAX_CACHE_ENTRIES`].
pub(crate) fn store_image(dir: &Path, url: &str, bytes: &[u8], ext: &str) -> Option<PathBuf> {
    std::fs::create_dir_all(dir).ok()?;
    let key = key_for(url);
    let output = dir.join(format!("{key}.{ext}"));
    let temporary = dir.join(format!(".{key}-{}.{ext}.tmp", fastrand::u64(..)));
    if std::fs::write(&temporary, bytes).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return None;
    }
    if std::fs::rename(&temporary, &output).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return cached_path_in(dir, url); // a concurrent writer may have published it
    }
    for old_extension in IMAGE_EXTS {
        if *old_extension != ext {
            let _ = std::fs::remove_file(dir.join(format!("{key}.{old_extension}")));
        }
    }
    enforce_bound(dir, MAX_CACHE_ENTRIES);
    Some(output)
}

/// Pure: given each cached file's identity and last-modified time, returns
/// which ids to evict — oldest-modified first — to bring the count down to
/// `limit`. A no-op (empty result) when already at or under the limit.
pub(crate) fn entries_to_evict(
    mut entries: Vec<(String, SystemTime)>,
    limit: usize,
) -> Vec<String> {
    if entries.len() <= limit {
        return Vec::new();
    }
    entries.sort_by_key(|(_, modified)| *modified);
    let overflow = entries.len() - limit;
    entries
        .into_iter()
        .take(overflow)
        .map(|(id, _)| id)
        .collect()
}

fn enforce_bound(dir: &Path, limit: usize) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    let entries: Vec<(String, SystemTime)> = read_dir
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| IMAGE_EXTS.contains(&ext))
        })
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((entry.file_name().to_string_lossy().into_owned(), modified))
        })
        .collect();
    for file_name in entries_to_evict(entries, limit) {
        let _ = std::fs::remove_file(dir.join(file_name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn tmp() -> PathBuf {
        std::env::temp_dir().join(format!("rp-remote-image-{}", fastrand::u64(..)))
    }

    #[test]
    fn key_normalizes_whitespace() {
        assert_eq!(
            key_for("https://x.test/a.jpg"),
            key_for(" https://x.test/a.jpg ")
        );
    }

    #[test]
    fn cache_dir_is_under_the_shared_cover_cache_dir() {
        assert!(cache_dir().starts_with(crate::cover::cache_dir()));
        assert!(cache_dir().ends_with("remote-images"));
    }

    #[test]
    fn cached_path_finds_existing_and_none_otherwise() {
        let dir = tmp();
        std::fs::create_dir_all(&dir).unwrap();
        assert!(cached_path_in(&dir, "https://x.test/a.jpg").is_none());
        let file = dir.join(format!("{}.jpg", key_for("https://x.test/a.jpg")));
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(cached_path_in(&dir, "https://x.test/a.jpg"), Some(file));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn store_image_publishes_atomically_and_replaces_older_formats() {
        let dir = tmp();
        let old = store_image(&dir, "https://x.test/a.jpg", b"old", "jpg").unwrap();
        let current = store_image(&dir, "https://x.test/a.jpg", b"new", "png").unwrap();

        assert_eq!(cached_path_in(&dir, "https://x.test/a.jpg"), Some(current));
        assert!(!old.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn src_11_entries_to_evict_keeps_the_newest_and_evicts_the_rest_oldest_first() {
        let base = UNIX_EPOCH;
        let entries = vec![
            ("a.jpg".to_string(), base + Duration::from_secs(1)),
            ("b.jpg".to_string(), base + Duration::from_secs(2)),
            ("c.jpg".to_string(), base + Duration::from_secs(3)),
        ];
        assert_eq!(
            entries_to_evict(entries, 2),
            vec!["a.jpg".to_string()],
            "only the oldest file must be evicted to reach the limit"
        );
    }

    #[test]
    fn src_11_entries_to_evict_is_a_noop_at_or_under_the_limit() {
        let entries = vec![("a.jpg".to_string(), SystemTime::now())];
        assert!(entries_to_evict(entries, 5).is_empty());
        assert!(entries_to_evict(Vec::new(), 0).is_empty());
    }

    #[test]
    fn src_11_evicts_the_oldest_file_on_disk_once_the_bound_is_exceeded() {
        let dir = tmp();
        std::fs::create_dir_all(&dir).unwrap();
        let oldest = store_image(&dir, "https://x.test/1.jpg", b"1", "jpg").unwrap();
        // Force distinct mtimes so ordering is deterministic on fast filesystems.
        std::thread::sleep(Duration::from_millis(10));
        let _middle = store_image(&dir, "https://x.test/2.jpg", b"2", "jpg").unwrap();
        std::thread::sleep(Duration::from_millis(10));
        let _newest = store_image(&dir, "https://x.test/3.jpg", b"3", "jpg").unwrap();

        enforce_bound(&dir, 2);

        assert!(!oldest.exists(), "the oldest cached file must be evicted");
        assert!(cached_path_in(&dir, "https://x.test/2.jpg").is_some());
        assert!(cached_path_in(&dir, "https://x.test/3.jpg").is_some());
        std::fs::remove_dir_all(&dir).ok();
    }
}
