//! Bounded on-disk caches for remote source images (`C1`).
//!
//! Positive entries are `<key>.<ext>` files under the XDG cache. Unlike the
//! permanent, unbounded album-cover download cache (`cover_download`), this
//! Persistent library artwork and transient discovery artwork live in
//! separate directories with separate caps. Once a new write would exceed a
//! scope's cap, the least-recently-modified files are evicted first.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::SystemTime;

use crate::cover_download::IMAGE_EXTS;

use super::CacheScope;

static LEGACY_CACHE_REMOVAL: Once = Once::new();

pub(crate) fn cache_dir(scope: CacheScope) -> PathBuf {
    let root = crate::cover::cache_dir();
    remove_legacy_cache_once(&root, &LEGACY_CACHE_REMOVAL);
    cache_dir_with_root(&root, scope)
}

fn cache_dir_with_root(root: &Path, scope: CacheScope) -> PathBuf {
    root.join(match scope {
        CacheScope::Persistent => "remote-images-persistent",
        CacheScope::Transient => "remote-images-transient",
    })
}

fn remove_legacy_cache_once(root: &Path, removal: &Once) {
    removal.call_once(|| remove_legacy_cache(root));
}

fn remove_legacy_cache(root: &Path) {
    let legacy = root.join("remote-images");
    if legacy.exists() {
        // Best-effort migration cleanup must not block the replacement caches.
        if let Err(error) = std::fs::remove_dir_all(&legacy) {
            tracing::debug!(
                %error,
                path = %legacy.display(),
                "could not remove the legacy remote image cache"
            );
        }
    }
}

pub(crate) fn key_for(url: &str) -> String {
    crate::cover::hash_hex(url.trim().as_bytes())
}

/// Looks up the cached file for `url`, if any. `SRC-11` defines eviction as
/// "least recently *touched* first", so a hit here has to count as a touch —
/// otherwise a file viewed daily is exactly as eviction-eligible as one never
/// looked at again since the day it was downloaded, both ordered purely by
/// write time. See [`touch`].
pub(crate) fn cached_path_in(dir: &Path, url: &str) -> Option<PathBuf> {
    let key = key_for(url);
    let path = IMAGE_EXTS
        .iter()
        .map(|ext| dir.join(format!("{key}.{ext}")))
        .find(|path| path.exists())?;
    touch(&path);
    Some(path)
}

/// Bumps `path`'s modification time to now, so it is treated as freshly used
/// for the next eviction pass. Best-effort and silent on failure (read-only
/// filesystem, permissions, a concurrent unlink, ...): a cache hit must still
/// be returned to the caller either way — `SRC-11`'s "a cache hit is always
/// shown" promise does not depend on the touch succeeding, only eviction
/// priority does, and a slightly-too-eager eviction is a far smaller defect
/// than hiding an image that is sitting right there on disk.
fn touch(path: &Path) {
    let Ok(file) = File::open(path) else {
        return;
    };
    let _ = file.set_modified(SystemTime::now());
}

/// Writes `bytes` atomically (temp file + rename), replaces any stale
/// extension for the same key, then enforces the selected scope's limit.
pub(crate) fn store_image(
    dir: &Path,
    url: &str,
    bytes: &[u8],
    ext: &str,
    limit: usize,
) -> Option<PathBuf> {
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
    enforce_bound(dir, limit);
    Some(output)
}

/// Pure: given each cached file's identity and last-modified time, returns
/// which ids to evict — oldest-modified first — to bring the count down to
/// `limit`. A no-op (empty result) when already at or under the limit.
///
/// Sorted by `(modified, id)`: the id is a pure tie-break, never a ranking
/// criterion on its own, but it makes the order fully deterministic when two
/// entries share a modification time (coarse filesystem timestamp
/// resolution, or two files touched within the same tick) — without it, ties
/// silently fell back to whatever order `read_dir` happened to yield, which
/// is not guaranteed stable and made the claimed "deterministic" eviction
/// false in exactly the case it matters (a real collision).
pub(crate) fn entries_to_evict(
    mut entries: Vec<(String, SystemTime)>,
    limit: usize,
) -> Vec<String> {
    if entries.len() <= limit {
        return Vec::new();
    }
    entries.sort_by(|(id_a, modified_a), (id_b, modified_b)| {
        modified_a.cmp(modified_b).then_with(|| id_a.cmp(id_b))
    });
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
    use std::sync::Once;
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
    fn src_11_cache_scopes_have_separate_directories_and_limits() {
        let root = tmp();

        let persistent = cache_dir_with_root(&root, super::super::CacheScope::Persistent);
        let transient = cache_dir_with_root(&root, super::super::CacheScope::Transient);

        assert_eq!(persistent, root.join("remote-images-persistent"));
        assert_eq!(transient, root.join("remote-images-transient"));
        assert_ne!(persistent, transient);
        assert_eq!(super::super::CacheScope::Persistent.entry_limit(), 1_000);
        assert_eq!(super::super::CacheScope::Transient.entry_limit(), 200);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn src_11_transient_eviction_never_removes_a_persistent_image() {
        let root = tmp();
        let persistent = cache_dir_with_root(&root, super::super::CacheScope::Persistent);
        let transient = cache_dir_with_root(&root, super::super::CacheScope::Transient);
        let subscribed = store_image(
            &persistent,
            "https://x.test/subscribed.jpg",
            b"p",
            "jpg",
            super::super::CacheScope::Persistent.entry_limit(),
        )
        .unwrap();
        let oldest_result =
            store_image(&transient, "https://x.test/result-1.jpg", b"1", "jpg", 1).unwrap();
        std::thread::sleep(Duration::from_millis(10));
        let newest_result =
            store_image(&transient, "https://x.test/result-2.jpg", b"2", "jpg", 1).unwrap();

        enforce_bound(&transient, 1);

        assert!(subscribed.exists());
        assert!(!oldest_result.exists());
        assert!(newest_result.exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn src_11_legacy_shared_cache_is_removed_and_never_recreated() {
        let root = tmp();
        let legacy = root.join("remote-images");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("old.jpg"), b"old").unwrap();

        let removal = Once::new();
        remove_legacy_cache_once(&root, &removal);
        let persistent = cache_dir_with_root(&root, super::super::CacheScope::Persistent);
        assert!(!legacy.exists());
        store_image(
            &persistent,
            "https://x.test/current.jpg",
            b"new",
            "jpg",
            super::super::CacheScope::Persistent.entry_limit(),
        )
        .unwrap();
        let transient = cache_dir_with_root(&root, super::super::CacheScope::Transient);

        assert!(!legacy.exists());
        assert!(persistent.ends_with("remote-images-persistent"));
        assert!(transient.ends_with("remote-images-transient"));

        std::fs::create_dir_all(&legacy).unwrap();
        remove_legacy_cache_once(&root, &removal);
        assert!(legacy.exists(), "legacy cleanup must run only once");
        std::fs::remove_dir_all(&root).ok();
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
    fn cached_path_finds_an_ico_image() {
        let dir = tmp();
        std::fs::create_dir_all(&dir).unwrap();
        let url = "https://x.test/favicon.ico";
        let file = dir.join(format!("{}.ico", key_for(url)));
        std::fs::write(&file, b"ico").unwrap();

        assert_eq!(cached_path_in(&dir, url), Some(file));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn store_image_publishes_atomically_and_replaces_older_formats() {
        let dir = tmp();
        let old = store_image(&dir, "https://x.test/a.jpg", b"old", "jpg", 300).unwrap();
        let current = store_image(&dir, "https://x.test/a.jpg", b"new", "png", 300).unwrap();

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
        let oldest = store_image(&dir, "https://x.test/1.jpg", b"1", "jpg", 300).unwrap();
        // Force distinct mtimes so ordering is deterministic on fast filesystems.
        std::thread::sleep(Duration::from_millis(10));
        let _middle = store_image(&dir, "https://x.test/2.jpg", b"2", "jpg", 300).unwrap();
        std::thread::sleep(Duration::from_millis(10));
        let _newest = store_image(&dir, "https://x.test/3.jpg", b"3", "jpg", 300).unwrap();

        enforce_bound(&dir, 2);

        assert!(!oldest.exists(), "the oldest cached file must be evicted");
        assert!(cached_path_in(&dir, "https://x.test/2.jpg").is_some());
        assert!(cached_path_in(&dir, "https://x.test/3.jpg").is_some());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `SRC-11`: "die am längsten unangetasteten Dateien zuerst" — least
    /// recently *touched*, not least recently *written*. A cache hit on the
    /// oldest-written file must count as touching it, so it survives the
    /// next eviction in place of whichever entry is now the actual
    /// least-recently-used one.
    #[test]
    fn src_11_a_cache_hit_counts_as_touching_the_entry() {
        let dir = tmp();
        std::fs::create_dir_all(&dir).unwrap();
        let oldest = store_image(&dir, "https://x.test/1.jpg", b"1", "jpg", 300).unwrap();
        std::thread::sleep(Duration::from_millis(10));
        let middle = store_image(&dir, "https://x.test/2.jpg", b"2", "jpg", 300).unwrap();
        std::thread::sleep(Duration::from_millis(10));
        let _newest = store_image(&dir, "https://x.test/3.jpg", b"3", "jpg", 300).unwrap();
        std::thread::sleep(Duration::from_millis(10));

        // "View" the oldest-written entry through the same lookup `resolve`
        // uses on every cache hit.
        assert_eq!(
            cached_path_in(&dir, "https://x.test/1.jpg"),
            Some(oldest.clone())
        );

        enforce_bound(&dir, 2);

        assert!(
            oldest.exists(),
            "a just-touched entry must not be evicted even though it was written first"
        );
        assert!(
            !middle.exists(),
            "the now-least-recently-touched entry must be evicted instead"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `SRC-11` promises *deterministic* eviction. Sorting purely by
    /// modification time is not deterministic when two entries share a
    /// timestamp (coarse filesystem clocks, or two touches in the same
    /// tick): the outcome would then depend on whatever order `read_dir`
    /// happens to hand entries in, which is explicitly unspecified. This
    /// asserts the same tied set evicts identically no matter what order it
    /// is discovered in — `read_dir` order is stood in for by the input
    /// `Vec` order, which is exactly what varies between real filesystem
    /// listings.
    #[test]
    fn src_11_entries_to_evict_ties_are_deterministic_regardless_of_discovery_order() {
        let same = SystemTime::now();
        let forward = vec![
            ("a.jpg".to_string(), same),
            ("b.jpg".to_string(), same),
            ("c.jpg".to_string(), same),
        ];
        let reversed = vec![
            ("c.jpg".to_string(), same),
            ("b.jpg".to_string(), same),
            ("a.jpg".to_string(), same),
        ];
        assert_eq!(
            entries_to_evict(forward, 1),
            entries_to_evict(reversed, 1),
            "eviction of tied-timestamp entries must not depend on discovery order"
        );
    }
}
