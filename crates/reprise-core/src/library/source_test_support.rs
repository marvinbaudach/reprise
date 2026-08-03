use super::*;

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

    fn open_read(&self, _at: &Path) -> io::Result<LibraryReadHandle> {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "metadata-only test source has no readable content",
        ))
    }

    fn probe(&self, _at: &Path, _links: LibraryLinkMode) -> Option<LibraryPathMetadata> {
        Some(LibraryPathMetadata {
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
