use std::io;
use std::path::{Path, PathBuf};

use super::source::{
    LibraryDirectoryEntry, LibraryLinkMode, LibraryPathMetadata, LibraryPathPresence,
    LibraryReadHandle, LibrarySource, LibraryWalkOrder, LibraryWalkVisitor,
};

/// A source that reports every path as present, carrying only the
/// file-or-directory distinction its constructor was given. For tests where
/// presence must be settled and nothing else matters.
pub(crate) struct ExistingPathSource {
    is_file: bool,
    is_directory: bool,
}

impl ExistingPathSource {
    pub(crate) const FILE: Self = Self {
        is_file: true,
        is_directory: false,
    };
    pub(crate) const DIRECTORY: Self = Self {
        is_file: false,
        is_directory: true,
    };
}

impl LibrarySource for ExistingPathSource {
    fn residence_token(&self, _at: &Path) -> Option<i64> {
        None
    }

    fn mount_point(&self, _at: &Path) -> Option<PathBuf> {
        None
    }

    fn display_name(&self, at: &Path) -> Option<String> {
        at.file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_owned)
    }

    fn container_name(&self, at: &Path) -> Option<String> {
        at.parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    }

    fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf> {
        at.strip_prefix(root).ok().map(Path::to_path_buf)
    }

    fn open_read(&self, _at: &Path) -> io::Result<LibraryReadHandle> {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "metadata-only test source has no readable content",
        ))
    }

    fn probe(&self, _at: &Path, _links: LibraryLinkMode) -> LibraryPathPresence {
        LibraryPathPresence::Present(LibraryPathMetadata {
            is_file: self.is_file,
            is_directory: self.is_directory,
            size: None,
            modified: None,
            identity: None,
        })
    }

    fn read_directory(&self, _directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
        Some(Vec::new())
    }

    fn walk(&self, _root: &Path, _order: LibraryWalkOrder, _visitor: &mut dyn LibraryWalkVisitor) {}
}

/// A source whose backing store cannot currently answer presence questions —
/// the case an Android provider produces when a Binder call fails. Its `probe`
/// answers [`LibraryPathPresence::Unknown`], which no caller may read as
/// absence.
pub(crate) struct UnknownProbeSource;

impl LibrarySource for UnknownProbeSource {
    fn residence_token(&self, _at: &Path) -> Option<i64> {
        None
    }

    fn mount_point(&self, _at: &Path) -> Option<PathBuf> {
        None
    }

    fn display_name(&self, _at: &Path) -> Option<String> {
        None
    }

    fn container_name(&self, _at: &Path) -> Option<String> {
        None
    }

    fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf> {
        at.strip_prefix(root).ok().map(Path::to_path_buf)
    }

    fn open_read(&self, _at: &Path) -> io::Result<LibraryReadHandle> {
        Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "test source is unreachable",
        ))
    }

    fn probe(&self, _at: &Path, _links: LibraryLinkMode) -> LibraryPathPresence {
        LibraryPathPresence::Unknown
    }

    fn read_directory(&self, _directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
        None
    }

    fn walk(&self, _root: &Path, _order: LibraryWalkOrder, _visitor: &mut dyn LibraryWalkVisitor) {}
}

/// A Unix-backed source that answers `display_name` with a fixed string, so a
/// test can prove the scanner asks the source for a name rather than deriving
/// one from the path. Everything else delegates to the real Unix source.
pub(crate) struct NamedUnixSource(pub(crate) &'static str);

impl LibrarySource for NamedUnixSource {
    fn residence_token(&self, at: &Path) -> Option<i64> {
        super::source::UnixLibrarySource.residence_token(at)
    }

    fn mount_point(&self, at: &Path) -> Option<PathBuf> {
        super::source::UnixLibrarySource.mount_point(at)
    }

    fn display_name(&self, _at: &Path) -> Option<String> {
        Some(self.0.to_owned())
    }

    fn container_name(&self, at: &Path) -> Option<String> {
        super::source::UnixLibrarySource.container_name(at)
    }

    fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf> {
        super::source::UnixLibrarySource.relative_path(root, at)
    }

    fn open_read(&self, at: &Path) -> io::Result<LibraryReadHandle> {
        super::source::UnixLibrarySource.open_read(at)
    }

    fn probe(&self, at: &Path, links: LibraryLinkMode) -> LibraryPathPresence {
        super::source::UnixLibrarySource.probe(at, links)
    }

    fn read_directory(&self, directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
        super::source::UnixLibrarySource.read_directory(directory)
    }

    fn walk(&self, root: &Path, order: LibraryWalkOrder, visitor: &mut dyn LibraryWalkVisitor) {
        super::source::UnixLibrarySource.walk(root, order, visitor);
    }
}

#[test]
fn unix_probe_distinguishes_absence_from_failed_stat() {
    use super::source::UnixLibrarySource;

    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("track.flac");
    std::fs::write(&file, b"audio").unwrap();

    assert!(matches!(
        UnixLibrarySource.probe(&file, LibraryLinkMode::Follow),
        LibraryPathPresence::Present(_)
    ));
    assert_eq!(
        UnixLibrarySource.probe(&directory.path().join("missing"), LibraryLinkMode::Follow),
        LibraryPathPresence::Absent
    );
    assert_eq!(
        UnixLibrarySource.probe(&file.join("child"), LibraryLinkMode::Follow),
        LibraryPathPresence::Unknown
    );
}
