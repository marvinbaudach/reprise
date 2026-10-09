//! The one place that decides where the platform keeps Reprise's regenerable
//! files (`<root>/reprise/covers`, `.../lyrics`, `.../artist-portraits`,
//! `.../device-sync`).
//!
//! Production resolves the XDG cache directory. A test build never does: it
//! gets a directory private to the test process instead, so a test can neither
//! write into the user's real `~/.cache/reprise` nor read a cache a previous
//! session left there. The isolation is compiled in rather than read from
//! `XDG_CACHE_HOME`, because the variable is only a convention a caller has
//! to remember to set; the Codex sandbox, where `~/.cache` is read-only, did
//! not set it.
//!
//! Crates that link `reprise-core` as an ordinary dependency (the Android
//! binding) get the same isolation by enabling the `test-cache-root` feature
//! in their `[dev-dependencies]`.

use std::path::PathBuf;

/// The directory the cache subdirectories hang below.
#[must_use]
pub fn user_cache_root() -> PathBuf {
    #[cfg(any(test, feature = "test-cache-root"))]
    {
        isolated_root().to_path_buf()
    }
    #[cfg(not(any(test, feature = "test-cache-root")))]
    {
        // The single sanctioned lookup; `clippy.toml` forbids every other.
        #[allow(
            clippy::disallowed_methods,
            reason = "the one sanctioned platform cache lookup"
        )]
        let platform_cache = dirs::cache_dir();
        platform_cache.unwrap_or_else(std::env::temp_dir)
    }
}

/// Whether [`user_cache_root`] is the process-private test directory.
#[must_use]
pub const fn is_isolated() -> bool {
    cfg!(any(test, feature = "test-cache-root"))
}

#[cfg(any(test, feature = "test-cache-root"))]
fn isolated_root() -> &'static std::path::Path {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        std::env::temp_dir().join(format!("reprise-test-cache-{}", std::process::id()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_build_never_resolves_the_users_real_cache() {
        assert!(is_isolated());
        let root = user_cache_root();
        assert!(root.starts_with(std::env::temp_dir()), "{root:?}");
        #[allow(
            clippy::disallowed_methods,
            reason = "the guard compares against the real lookup"
        )]
        let real = dirs::cache_dir();
        if let Some(real) = real {
            assert!(!root.starts_with(&real), "{root:?} is inside {real:?}");
        }
    }

    #[test]
    fn every_default_cache_directory_hangs_below_the_isolated_root() {
        let root = user_cache_root();
        assert!(crate::cover::cache_dir().starts_with(&root));
        assert!(crate::cover_download::downloaded_dir().starts_with(&root));
        assert!(crate::artist_portrait::cache_dir().starts_with(&root));
        assert!(crate::device_sync::staging::staging_dir().starts_with(&root));
    }
}
