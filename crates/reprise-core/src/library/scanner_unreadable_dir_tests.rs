//! A library source whose walk meets one directory it may not enter.
//!
//! The scan tests for an unreadable directory used to `chmod 000` a real one.
//! A process with root rights reads such a directory all the same, so under CI
//! (which runs as root) those tests either skipped themselves or proved
//! nothing. This source fails the way `walkdir` does for the caller without
//! needing the operating system to refuse anything: it hands out the
//! directory's own entry, then the permission error for it, and never what is
//! inside — exactly the sequence `UnixLibrarySource` yields for a directory
//! `read_dir` rejects.
//!
//! The error is built by [`source::walk_error`], the same function the Unix
//! walk calls, so the classification and the directory path the scanner
//! records are the production ones. What stays unproven here is only that the
//! kernel refuses a mode-000 directory to an ordinary user.

use std::io;
use std::path::{Path, PathBuf};

use crate::library::source::{
    self, LibraryDirectoryEntry, LibraryLinkMode, LibraryPathPresence, LibraryReadHandle,
    LibraryWalkControl, LibraryWalkItem, LibraryWalkOrder, LibraryWalkVisitor, UnixLibrarySource,
};

/// The Unix source, except that `locked` cannot be entered.
pub(super) struct UnreadableDirectorySource {
    locked: PathBuf,
}

impl UnreadableDirectorySource {
    pub(super) fn new(locked: &Path) -> Self {
        Self {
            locked: locked.to_path_buf(),
        }
    }
}

/// Passes the walk through, replacing what is inside the locked directory with
/// the one error that stands for it.
struct LockedSubtree<'a> {
    locked: &'a Path,
    inner: &'a mut dyn LibraryWalkVisitor,
}

impl LibraryWalkVisitor for LockedSubtree<'_> {
    fn visit(&mut self, item: LibraryWalkItem) -> LibraryWalkControl {
        let LibraryWalkItem::Entry(entry) = &item else {
            return self.inner.visit(item);
        };
        if entry.path != self.locked {
            return if entry.path.starts_with(self.locked) {
                LibraryWalkControl::Continue
            } else {
                self.inner.visit(item)
            };
        }
        if self.inner.visit(item) == LibraryWalkControl::Stop {
            return LibraryWalkControl::Stop;
        }
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        self.inner.visit(LibraryWalkItem::Error(source::walk_error(
            Some(self.locked),
            Some(&denied),
            format!(
                "IO error for operation on {}: {denied}",
                self.locked.display()
            ),
        )))
    }
}

impl source::LibrarySource for UnreadableDirectorySource {
    fn residence_token(&self, at: &Path) -> Option<i64> {
        UnixLibrarySource.residence_token(at)
    }
    fn mount_point(&self, at: &Path) -> Option<PathBuf> {
        UnixLibrarySource.mount_point(at)
    }
    fn display_name(&self, at: &Path) -> Option<String> {
        UnixLibrarySource.display_name(at)
    }
    fn container_name(&self, at: &Path) -> Option<String> {
        UnixLibrarySource.container_name(at)
    }
    fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf> {
        UnixLibrarySource.relative_path(root, at)
    }
    fn open_read(&self, at: &Path) -> io::Result<LibraryReadHandle> {
        UnixLibrarySource.open_read(at)
    }
    fn probe(&self, at: &Path, links: LibraryLinkMode) -> LibraryPathPresence {
        UnixLibrarySource.probe(at, links)
    }
    fn read_directory(&self, directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
        UnixLibrarySource.read_directory(directory)
    }
    fn walk(&self, root: &Path, order: LibraryWalkOrder, visitor: &mut dyn LibraryWalkVisitor) {
        UnixLibrarySource.walk(
            root,
            order,
            &mut LockedSubtree {
                locked: &self.locked,
                inner: visitor,
            },
        );
    }
}
