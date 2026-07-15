//! Backup creation and issue repair.

use std::path::Path;

use super::{ExternalFixer, Issue, RepairReport};

/// Repair the listed issues in `path`.
/// Creates a `.bak` backup unless `backup` is `false`.
pub fn repair(
	path: &Path,
	issues: &[Issue],
	fixer: &dyn ExternalFixer,
	backup: bool,
) -> Vec<RepairReport> {
	let _ = (path, issues, fixer, backup);
	todo!("implemented in Task 3")
}
