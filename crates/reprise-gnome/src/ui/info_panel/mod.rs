//! Temporary compatibility for the parallel layout lane.
//!
//! The sibling-owned `library_shell.rs` still names the former module and
//! type. Remove this shim once both lanes are merged and that owner can update
//! its call site.

pub(in crate::ui) use super::now_playing::NowPlayingPanel as InfoPanel;
