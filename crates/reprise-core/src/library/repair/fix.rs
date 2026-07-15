//! Backup creation and issue repair.
//!
//! Tag-level fixes (duplicate ilst, corrupt ID3 frames) re-save via lofty —
//! its write path naturally consolidates containers and drops unparseable
//! frames.  VBR header insertion is delegated to [`ExternalFixer`].

use std::path::{Path, PathBuf};

use lofty::prelude::*;

use super::{ExternalFixer, FixOutcome, Issue, RepairReport};

/// Copy `path` to `<path>.bak`, returning the backup path.
pub fn create_backup(path: &Path) -> Result<PathBuf, std::io::Error> {
    let mut bak_name = path.as_os_str().to_owned();
    bak_name.push(".bak");
    let bak = PathBuf::from(bak_name);
    std::fs::copy(path, &bak)?;
    Ok(bak)
}

/// Fix tag-level issues by reading and re-saving via lofty.
///
/// Lofty consolidates duplicate ilst atoms and drops unparseable ID3v2
/// frames on re-save, so a simple read→save roundtrip is the fix.
fn fix_tags(path: &Path) -> FixOutcome {
    let tagged = match lofty::read_from_path(path) {
        Ok(t) => t,
        Err(e) => return FixOutcome::Failed { error: format!("lofty read: {e}") },
    };
    let tag = match tagged.primary_tag().or_else(|| tagged.first_tag()) {
        Some(t) => t,
        None => {
            return FixOutcome::Skipped {
                reason: "no tag container found".into(),
            };
        }
    };
    match tag.save_to_path(path, lofty::config::WriteOptions::default()) {
        Ok(()) => FixOutcome::Fixed,
        Err(e) => FixOutcome::Failed { error: format!("lofty write: {e}") },
    }
}

/// Repair the listed issues in `path`.
///
/// Creates a `.bak` backup before any modification unless `backup` is false.
/// Returns one [`RepairReport`] per issue.
pub fn repair(
    path: &Path,
    issues: &[Issue],
    fixer: &dyn ExternalFixer,
    backup: bool,
) -> Vec<RepairReport> {
    if issues.is_empty() {
        return Vec::new();
    }

    // Backup once before any modification.
    if backup {
        if let Err(e) = create_backup(path) {
            // If backup fails, refuse to proceed.
            return issues
                .iter()
                .map(|issue| RepairReport {
                    path: path.to_path_buf(),
                    issue: issue.clone(),
                    outcome: FixOutcome::Failed {
                        error: format!("backup failed: {e}"),
                    },
                })
                .collect();
        }
    }

    // Separate tag-level issues from VBR (external tool).
    let needs_tag_resave = issues
        .iter()
        .any(|i| matches!(i, Issue::DuplicateIlst | Issue::CorruptId3Frames));

    let tag_outcome = if needs_tag_resave {
        Some(fix_tags(path))
    } else {
        None
    };

    issues
        .iter()
        .map(|issue| {
            let outcome = match issue {
                Issue::DuplicateIlst | Issue::CorruptId3Frames => {
                    // Both are fixed by the same tag re-save.
                    tag_outcome.clone().unwrap_or(FixOutcome::Skipped {
                        reason: "unexpected state".into(),
                    })
                }
                Issue::MissingVbrHeader => fixer.fix_vbr_header(path),
            };
            RepairReport {
                path: path.to_path_buf(),
                issue: issue.clone(),
                outcome,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "fix_tests.rs"]
mod tests;
