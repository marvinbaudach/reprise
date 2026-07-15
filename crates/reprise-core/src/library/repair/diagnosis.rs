//! Issue detection via lofty tracing-warning capture.

use std::path::Path;

use super::{Diagnosis, RepairError};

/// Diagnose metadata issues in a single audio file.
pub fn diagnose(path: &Path) -> Result<Diagnosis, RepairError> {
	let _ = path;
	todo!("implemented in Task 2")
}
