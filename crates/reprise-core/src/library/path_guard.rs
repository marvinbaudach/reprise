//! Whether a path lies inside a root, once both ends are resolved.
//!
//! Two callers ask that question before doing something the user cannot
//! easily undo: the permanent-trash smoke hook refuses a selection that is
//! not inside a temporary scan root, and the relink dialogs warn when the
//! replacement file sits outside the library. Both used to canonicalise a
//! path and a root in the GTK frontend and compare the results themselves —
//! one safety decision written twice, in the layer least able to test it.
//!
//! They deliberately disagree about what an unresolvable path means, and
//! that disagreement is the reason the comparison takes a policy instead of
//! being copied: a guard standing in front of a delete must refuse a path it
//! cannot resolve, while a dialog that only warns stays more useful comparing
//! the path as written. [`Unresolvable`] makes each caller state which one it
//! is asking for, at the call site.

use std::path::{Path, PathBuf};

/// What an end of the comparison that the filesystem cannot resolve means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unresolvable {
    /// Not inside the root. A path that cannot be resolved has not been
    /// proven to be anywhere, and a caller about to destroy files must not
    /// help itself to the convenient answer.
    Outside,
    /// Compare it as written. Nothing irreversible hangs on the answer, so a
    /// lexical comparison beats declining to give one.
    CompareAsWritten,
}

/// Whether `path` lies inside `root`, both resolved through symlinks first.
pub fn is_within(root: &Path, path: &Path, unresolvable: Unresolvable) -> bool {
    let resolved_root = std::fs::canonicalize(root);
    let resolved_path = std::fs::canonicalize(path);
    match unresolvable {
        Unresolvable::Outside => match (resolved_root, resolved_path) {
            (Ok(root), Ok(path)) => path.starts_with(root),
            _ => false,
        },
        Unresolvable::CompareAsWritten => resolved_path
            .unwrap_or_else(|_| path.to_path_buf())
            .starts_with(resolved_root.unwrap_or_else(|_| root.to_path_buf())),
    }
}

/// Whether every one of `paths` resolves inside `root`, and `root` itself
/// resolves inside the system temporary directory.
///
/// This is the guard in front of the permanent-delete smoke hook. Both halves
/// matter: the temporary-directory clause means a mis-set scan root cannot
/// aim the hook at the user's music, and the per-path clause means a
/// selection that wandered out of the scratch tree is refused rather than
/// deleted.
pub fn paths_within_temp_root(root: &Path, paths: &[PathBuf]) -> bool {
    is_within(&std::env::temp_dir(), root, Unresolvable::Outside)
        && paths
            .iter()
            .all(|path| is_within(root, path, Unresolvable::Outside))
}

#[cfg(test)]
#[path = "path_guard_tests.rs"]
mod tests;
