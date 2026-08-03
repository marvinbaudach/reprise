use std::io;
use std::path::{Path, PathBuf};

use super::source::{
    LibraryDirectoryEntry, LibraryLinkMode, LibraryPathPresence, LibraryReadHandle, LibrarySource,
    LibraryWalkOrder, LibraryWalkVisitor,
};

/// A source whose backing store cannot currently answer presence questions.
pub(crate) struct UnknownProbeSource;

impl LibrarySource for UnknownProbeSource {
    fn residence_token(&self, _at: &Path) -> Option<i64> {
        None
    }

    fn mount_point(&self, _at: &Path) -> Option<PathBuf> {
        None
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

#[test]
fn unix_probe_distinguishes_absence_from_failed_stat() {
    use super::source::{LibraryPathPresence, UnixLibrarySource};

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
