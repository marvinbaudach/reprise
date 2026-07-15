//! Diagnose and repair common audio file metadata defects.
//!
//! Detection captures lofty's tracing warnings during `read_from_path()`.
//! Fixes re-save tags (consolidating ilst / dropping corrupt frames) and
//! delegate VBR header insertion to an [`ExternalFixer`] provided by the
//! caller.

pub mod diagnosis;
pub mod fix;

use std::path::{Path, PathBuf};

/// A metadata defect detected in an audio file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
	/// MP4/M4A: multiple `ilst` atoms in the `moov` container.
	DuplicateIlst,
	/// MP3: one or more ID3v2 frame headers could not be parsed.
	CorruptId3Frames,
	/// MP3: VBR stream without Xing/VBRI header — duration is estimated.
	MissingVbrHeader,
}

impl std::fmt::Display for Issue {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::DuplicateIlst => write!(f, "Duplicate ilst atoms"),
			Self::CorruptId3Frames => write!(f, "Corrupt ID3v2 frames"),
			Self::MissingVbrHeader => write!(f, "Missing VBR header (duration estimated)"),
		}
	}
}

/// Result of diagnosing a single file.
#[derive(Debug, Clone)]
pub struct Diagnosis {
	pub path: PathBuf,
	pub issues: Vec<Issue>,
}

/// Outcome of attempting to fix a single issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixOutcome {
	Fixed,
	Skipped { reason: String },
	Failed { error: String },
}

/// Per-issue result after a repair attempt.
#[derive(Debug, Clone)]
pub struct RepairReport {
	pub path: PathBuf,
	pub issue: Issue,
	pub outcome: FixOutcome,
}

#[derive(Debug, thiserror::Error)]
pub enum RepairError {
	#[error("lofty: {0}")]
	Lofty(#[from] lofty::error::LoftyError),
	#[error("I/O: {0}")]
	Io(#[from] std::io::Error),
	#[error("database: {0}")]
	Db(#[from] rusqlite::Error),
}

/// Abstraction for tools that require an external binary (e.g. mp3val).
/// Core defines the trait; CLI/GUI provide implementations.
pub trait ExternalFixer {
	fn fix_vbr_header(&self, path: &Path) -> FixOutcome;
}
