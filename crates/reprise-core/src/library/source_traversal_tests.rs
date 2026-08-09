//! Non-filesystem traversal fixtures split from `source.rs` to keep the
//! production contract below the repository's code-file limit.

use std::path::Path;

use super::{
    LibraryDirectoryEntry, LibraryLinkMode, LibraryPathPresence, LibraryReadHandle, LibrarySource,
    LibraryWalkControl, LibraryWalkItem, LibraryWalkOrder, LibraryWalkVisitor, UnixLibrarySource,
};

enum DocumentNode {
    Directory(&'static str, Vec<DocumentNode>),
    File(&'static str),
}

struct DocumentTreeTraversalSource {
    children: Vec<DocumentNode>,
}

impl DocumentTreeTraversalSource {
    fn emit(
        visitor: &mut dyn LibraryWalkVisitor,
        parent: &Path,
        nodes: &[DocumentNode],
        order: LibraryWalkOrder,
    ) -> LibraryWalkControl {
        let mut nodes: Vec<_> = nodes.iter().collect();
        if order == LibraryWalkOrder::FileName {
            nodes.sort_by_key(|node| match node {
                DocumentNode::Directory(name, _) | DocumentNode::File(name) => *name,
            });
        }
        for node in nodes {
            let (name, is_file) = match node {
                DocumentNode::Directory(name, _) => (*name, false),
                DocumentNode::File(name) => (*name, true),
            };
            let path = parent.join(name);
            if visitor.visit(LibraryWalkItem::Entry(super::LibraryEntry {
                path: path.clone(),
                is_file,
                metadata: None,
            })) == LibraryWalkControl::Stop
            {
                return LibraryWalkControl::Stop;
            }
            if let DocumentNode::Directory(_, children) = node {
                if Self::emit(visitor, &path, children, order) == LibraryWalkControl::Stop {
                    return LibraryWalkControl::Stop;
                }
            }
        }
        LibraryWalkControl::Continue
    }
}

impl LibrarySource for DocumentTreeTraversalSource {
    fn residence_token(&self, _at: &Path) -> Option<i64> {
        Some(41)
    }

    fn mount_point(&self, _at: &Path) -> Option<std::path::PathBuf> {
        None
    }

    fn display_name(&self, at: &Path) -> Option<String> {
        at.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    }

    fn container_name(&self, _at: &Path) -> Option<String> {
        None
    }

    fn relative_path(&self, root: &Path, at: &Path) -> Option<std::path::PathBuf> {
        at.strip_prefix(root).ok().map(Path::to_path_buf)
    }

    fn open_read(&self, _at: &Path) -> std::io::Result<LibraryReadHandle> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "traversal-only test source carries names, not content",
        ))
    }

    /// Unused by this double's tests. Made explicit rather than inherited:
    /// the trait has no defaults precisely so a source cannot answer
    /// "absent" for a question it was never taught to answer.
    fn probe(&self, _at: &Path, _links: LibraryLinkMode) -> LibraryPathPresence {
        LibraryPathPresence::Unknown
    }

    fn read_directory(&self, _directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
        None
    }

    fn walk(&self, root: &Path, order: LibraryWalkOrder, visitor: &mut dyn LibraryWalkVisitor) {
        Self::emit(visitor, root, &self.children, order);
    }
}

#[derive(Default)]
struct AudioPaths {
    root: std::path::PathBuf,
    paths: Vec<std::path::PathBuf>,
}

impl LibraryWalkVisitor for AudioPaths {
    fn visit(&mut self, item: LibraryWalkItem) -> LibraryWalkControl {
        let LibraryWalkItem::Entry(entry) = item else {
            panic!("fixture traversal must not fail");
        };
        if entry.is_file
            && entry
                .path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("flac"))
        {
            self.paths
                .push(entry.path.strip_prefix(&self.root).unwrap().to_path_buf());
        }
        LibraryWalkControl::Continue
    }
}

#[test]
fn non_filesystem_tree_matches_unix_order_and_file_filtering() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("Album")).unwrap();
    std::fs::write(dir.path().join("Album/notes.txt"), b"notes").unwrap();
    std::fs::write(dir.path().join("Album/song.FLAC"), b"audio").unwrap();
    std::fs::write(dir.path().join("loose.flac"), b"audio").unwrap();

    let document_tree = DocumentTreeTraversalSource {
        children: vec![
            DocumentNode::File("loose.flac"),
            DocumentNode::Directory(
                "Album",
                vec![
                    DocumentNode::File("song.FLAC"),
                    DocumentNode::File("notes.txt"),
                ],
            ),
        ],
    };
    let document_root = Path::new("content:/music");

    let mut unix = AudioPaths {
        root: dir.path().to_path_buf(),
        ..AudioPaths::default()
    };
    UnixLibrarySource.walk(dir.path(), LibraryWalkOrder::FileName, &mut unix);
    let mut document = AudioPaths {
        root: document_root.to_path_buf(),
        ..AudioPaths::default()
    };
    document_tree.walk(document_root, LibraryWalkOrder::FileName, &mut document);

    assert_eq!(
        unix.paths,
        vec![
            std::path::PathBuf::from("Album/song.FLAC"),
            std::path::PathBuf::from("loose.flac"),
        ]
    );
    assert_eq!(document.paths, unix.paths);
}
