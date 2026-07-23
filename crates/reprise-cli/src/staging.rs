//! Resolving the staging store for a CLI invocation.
//!
//! Finished-but-undecided AI renders live in the staging store (Beschluss 15):
//! a real, playable FLAC per job that is not in the library and not under any
//! scan root. The app, the CLI worker, and the CLI's promote/discard commands
//! must all agree on where those files are, so the default resolves the *same*
//! XDG location `reprise-core` uses (`ai_staging::default_staging_dir`).
//! `--staging-dir` overrides it — every test points at a temp dir, and it also
//! covers the documented Flatpak path divergence between the sandboxed app and
//! a host CLI (plan §6).

use std::path::PathBuf;

use reprise_core::ai_staging::{default_staging_dir, StagingStore};

/// Builds the staging store the AI commands act on: the explicit `--staging-dir`
/// when given, otherwise the standard per-user staging directory.
pub fn resolve(staging_dir: Option<&PathBuf>) -> StagingStore {
    match staging_dir {
        Some(path) => StagingStore::new(path.clone()),
        None => StagingStore::new(default_staging_dir()),
    }
}
