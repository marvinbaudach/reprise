//! On-disk artist-portrait cache under the XDG cache. Positive entries are
//! image files named `<key>.<ext>`; a `<key>.notfound` marker records a miss.
//! Freshness is derived from each file's modification time.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

pub(crate) const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

const POSITIVE_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;
const NEGATIVE_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;

static CACHE_WRITE_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("reprise/artist-portraits")
}

pub(crate) fn key_for(name: &str) -> String {
    crate::cover::hash_hex(normalize(name).as_bytes())
}

pub(crate) fn portrait_path_in(dir: &Path, name: &str) -> Option<PathBuf> {
    let key = key_for(name);
    IMAGE_EXTS
        .iter()
        .map(|ext| dir.join(format!("{key}.{ext}")))
        .find(|path| path.exists())
}

pub(crate) fn negative_marker_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.notfound", key_for(name)))
}

pub(crate) fn is_fresh(fetched_at: i64, now: i64, positive: bool) -> bool {
    let age = now.saturating_sub(fetched_at).max(0);
    let ttl = if positive {
        POSITIVE_TTL_SECONDS
    } else {
        NEGATIVE_TTL_SECONDS
    };
    age <= ttl
}

pub(crate) fn file_epoch_secs(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs() as i64)
}

pub(crate) fn store_image(dir: &Path, name: &str, bytes: &[u8], ext: &str) -> Option<PathBuf> {
    let _guard = CACHE_WRITE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::fs::create_dir_all(dir).ok()?;
    let key = key_for(name);
    let output = dir.join(format!("{key}.{ext}"));
    let temporary = dir.join(format!(".{key}-{}.{ext}.tmp", fastrand::u64(..)));
    if std::fs::write(&temporary, bytes).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return None;
    }
    if std::fs::rename(&temporary, &output).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return portrait_path_in(dir, name);
    }
    for old_extension in IMAGE_EXTS {
        if *old_extension != ext {
            let _ = std::fs::remove_file(dir.join(format!("{key}.{old_extension}")));
        }
    }
    let _ = std::fs::remove_file(negative_marker_path(dir, name));
    Some(output)
}

pub(crate) fn write_negative(dir: &Path, name: &str) {
    let _guard = CACHE_WRITE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let key = key_for(name);
    let output = negative_marker_path(dir, name);
    let temporary = dir.join(format!(".{key}-{}.notfound.tmp", fastrand::u64(..)));
    if std::fs::write(&temporary, b"").is_ok() && std::fs::rename(&temporary, output).is_ok() {
        for extension in IMAGE_EXTS {
            let _ = std::fs::remove_file(dir.join(format!("{key}.{extension}")));
        }
        return;
    }
    let _ = std::fs::remove_file(temporary);
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_normalizes_case_and_whitespace() {
        assert_eq!(
            key_for("Bring Me The Horizon"),
            key_for("  bring  me the  horizon ")
        );
    }

    #[test]
    fn cache_dir_is_under_xdg_cache_reprise() {
        assert!(cache_dir().ends_with("reprise/artist-portraits"));
    }

    #[test]
    fn portrait_path_finds_existing_and_none_otherwise() {
        let dir = std::env::temp_dir().join(format!("rp-portrait-{}", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(portrait_path_in(&dir, "Solo").is_none());
        let file = dir.join(format!("{}.jpg", key_for("Solo")));
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(portrait_path_in(&dir, "Solo"), Some(file));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn positive_ttl_is_30_days_negative_7_days() {
        let day = 24 * 60 * 60;
        assert!(is_fresh(1_000, 1_000 + 29 * day, true));
        assert!(!is_fresh(1_000, 1_000 + 31 * day, true));
        assert!(is_fresh(1_000, 1_000 + 6 * day, false));
        assert!(!is_fresh(1_000, 1_000 + 8 * day, false));
    }

    #[test]
    fn store_image_publishes_atomically_and_write_negative_marks() {
        let dir = std::env::temp_dir().join(format!("rp-portrait-{}", fastrand::u64(..)));
        let stored = store_image(&dir, "Band", b"img", "jpg").unwrap();
        assert!(stored.exists());
        assert_eq!(portrait_path_in(&dir, "Band"), Some(stored));
        write_negative(&dir, "Missing");
        assert!(negative_marker_path(&dir, "Missing").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn store_image_replaces_older_formats_and_a_negative_marker() {
        let dir = std::env::temp_dir().join(format!("rp-portrait-{}", fastrand::u64(..)));
        let old = store_image(&dir, "Band", b"old", "jpg").unwrap();
        write_negative(&dir, "Band");

        let current = store_image(&dir, "Band", b"new", "png").unwrap();

        assert_eq!(portrait_path_in(&dir, "Band"), Some(current));
        assert!(!old.exists());
        assert!(!negative_marker_path(&dir, "Band").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
