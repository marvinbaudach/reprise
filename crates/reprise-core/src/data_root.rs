//! The one place that decides where the platform keeps Reprise's own data
//! (`<root>/reprise/reprise.db`, `.../staging`, `.../podcasts`, `.../models`,
//! `.../diagnostics`).
//!
//! Production resolves the XDG data directory. A test build never does: it
//! gets a directory private to the test process instead, so a test can neither
//! write into the user's real `~/.local/share/reprise` nor read a library a
//! previous session left there. The isolation is compiled in rather than read
//! from `XDG_DATA_HOME`, because the variable is only a convention a caller
//! has to remember to set. It is the same mechanism as
//! [`crate::cache_root`], and the same `test-private-dirs` feature governs
//! both.
//!
//! Crates that link `reprise-core` as an ordinary dependency (the frontends,
//! the CLI, the MCP server, the stems backend) get the same isolation by
//! enabling `test-private-dirs` in their `[dev-dependencies]`.
//!
//! The root is an `Option` because the platform may have no data directory;
//! each caller keeps its own fallback for that case.

use std::path::PathBuf;

/// The directory the data subdirectories hang below, or `None` when the
/// platform has no data directory.
#[must_use]
pub fn user_data_root() -> Option<PathBuf> {
    #[cfg(any(test, feature = "test-private-dirs"))]
    {
        Some(isolated_root().to_path_buf())
    }
    #[cfg(not(any(test, feature = "test-private-dirs")))]
    {
        // The single sanctioned lookup; `clippy.toml` forbids every other.
        #[allow(
            clippy::disallowed_methods,
            reason = "the one sanctioned platform data lookup"
        )]
        let platform_data = dirs::data_dir();
        platform_data
    }
}

/// Whether [`user_data_root`] is the process-private test directory.
#[must_use]
pub const fn is_isolated() -> bool {
    cfg!(any(test, feature = "test-private-dirs"))
}

#[cfg(any(test, feature = "test-private-dirs"))]
fn isolated_root() -> &'static std::path::Path {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        std::env::temp_dir().join(format!("reprise-test-data-{}", std::process::id()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_build_never_resolves_the_users_real_data_dir() {
        assert!(is_isolated());
        let root = user_data_root().expect("a test build always has a data root");
        assert!(root.starts_with(std::env::temp_dir()), "{root:?}");
        #[allow(
            clippy::disallowed_methods,
            reason = "the guard compares against the real lookup"
        )]
        let real = dirs::data_dir();
        if let Some(real) = real {
            assert!(!root.starts_with(&real), "{root:?} is inside {real:?}");
        }
    }

    #[test]
    fn every_default_data_directory_hangs_below_the_isolated_root() {
        let root = user_data_root().expect("a test build always has a data root");
        assert!(crate::db::default_path().starts_with(&root));
        assert!(crate::ai_staging::default_staging_dir().starts_with(&root));
        assert!(crate::podcasts::downloads::default_download_root().starts_with(&root));
    }
}
