use std::fs::File;
use std::io;
use std::os::fd::FromRawFd;
use std::path::Path;
use std::time::{Duration, SystemTime};

use reprise_core::library::source::{
    LibraryDirectoryEntry, LibraryEntry, LibraryLinkMode, LibraryPathMetadata, LibraryPathPresence,
    LibraryReadHandle, LibrarySource, LibraryWalkControl, LibraryWalkError, LibraryWalkErrorKind,
    LibraryWalkItem, LibraryWalkOrder, LibraryWalkVisitor,
};
use sha2::{Digest, Sha256};

/// Provider facts returned by one SAF document query.
#[derive(Clone, Debug, uniffi::Record)]
pub struct SourceFacts {
    pub is_file: bool,
    pub is_directory: bool,
    pub size_bytes: Option<u64>,
    pub modified_unix_ms: Option<i64>,
    /// Stable within one DocumentsProvider, as required by DocumentsContract.
    pub document_id: String,
}

/// One immediate child returned by a SAF directory cursor.
#[derive(Clone, Debug, uniffi::Record)]
pub struct SourceChild {
    pub uri: String,
    pub display_name: String,
    pub is_file: bool,
    pub is_directory: bool,
    pub size_bytes: Option<u64>,
    pub modified_unix_ms: Option<i64>,
    pub document_id: String,
}

/// A provider-side failure, kept distinct from a confirmed missing document.
#[derive(Clone, Debug, thiserror::Error, uniffi::Error)]
#[uniffi(with_try_read)]
pub enum SafSourceError {
    #[error("permission denied: {detail}")]
    PermissionDenied { detail: String },
    #[error("I/O failure: {detail}")]
    Io { detail: String },
    #[error("provider failure: {detail}")]
    Unknown { detail: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for SafSourceError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Unknown {
            detail: error.to_string(),
        }
    }
}

/// The four operations Kotlin implements. Only UniFFI-safe values cross.
#[uniffi::export(callback_interface)]
pub trait SafSource: Send + Sync {
    fn residence_token(&self, uri: String) -> Result<Option<i64>, SafSourceError>;
    fn probe(&self, uri: String, follow_links: bool)
        -> Result<Option<SourceFacts>, SafSourceError>;
    fn list_children(&self, uri: String) -> Result<Vec<SourceChild>, SafSourceError>;
    fn open_read_fd(&self, uri: String) -> Result<i32, SafSourceError>;
}

/// Adapts the flat foreign callback to Core's complete storage contract.
pub struct BridgedSource {
    source: Box<dyn SafSource>,
}

impl BridgedSource {
    pub fn new(source: Box<dyn SafSource>) -> Self {
        Self { source }
    }

    fn emit_children(
        &self,
        directory: &Path,
        order: LibraryWalkOrder,
        visitor: &mut dyn LibraryWalkVisitor,
    ) -> LibraryWalkControl {
        let mut children = match self.source.list_children(path_uri(directory)) {
            Ok(children) => children,
            Err(error) => {
                let item = LibraryWalkItem::Error(walk_error(directory, &error));
                return visitor.visit(item);
            }
        };
        if order == LibraryWalkOrder::FileName {
            children.sort_by(|left, right| {
                left.display_name
                    .cmp(&right.display_name)
                    .then_with(|| left.uri.cmp(&right.uri))
            });
        }

        for child in children {
            let path = std::path::PathBuf::from(&child.uri);
            let is_directory = child.is_directory;
            let entry = LibraryEntry {
                path: path.clone(),
                is_file: child.is_file,
                metadata: Some(metadata_from_child(&child)),
            };
            if visitor.visit(LibraryWalkItem::Entry(entry)) == LibraryWalkControl::Stop {
                return LibraryWalkControl::Stop;
            }
            if is_directory && self.emit_children(&path, order, visitor) == LibraryWalkControl::Stop
            {
                return LibraryWalkControl::Stop;
            }
        }
        LibraryWalkControl::Continue
    }
}

impl LibrarySource for BridgedSource {
    fn residence_token(&self, at: &Path) -> Option<i64> {
        self.source
            .residence_token(path_uri(at))
            .unwrap_or_default()
    }

    fn open_read(&self, at: &Path) -> io::Result<LibraryReadHandle> {
        let raw_fd = self
            .source
            .open_read_fd(path_uri(at))
            .map_err(source_io_error)?;
        if raw_fd < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("provider returned invalid file descriptor {raw_fd}"),
            ));
        }

        // Kotlin transfers ownership with ParcelFileDescriptor.detachFd().
        // File closes the adopted descriptor on every later return path.
        let file = unsafe { File::from_raw_fd(raw_fd) };
        Ok(LibraryReadHandle::new(file))
    }

    fn probe(&self, at: &Path, links: LibraryLinkMode) -> LibraryPathPresence {
        match self
            .source
            .probe(path_uri(at), matches!(links, LibraryLinkMode::Follow))
        {
            Ok(Some(facts)) => LibraryPathPresence::Present(metadata_from_facts(at, &facts)),
            Ok(None) => LibraryPathPresence::Absent,
            Err(_) => LibraryPathPresence::Unknown,
        }
    }

    fn read_directory(&self, directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
        self.source
            .list_children(path_uri(directory))
            .ok()
            .map(|children| {
                children
                    .into_iter()
                    .map(|child| LibraryDirectoryEntry {
                        path: child.uri.clone().into(),
                        metadata: Some(metadata_from_child(&child)),
                    })
                    .collect()
            })
    }

    fn walk(&self, root: &Path, order: LibraryWalkOrder, visitor: &mut dyn LibraryWalkVisitor) {
        let root_facts = match self.source.probe(path_uri(root), true) {
            Ok(Some(facts)) => facts,
            Ok(None) => {
                visitor.visit(LibraryWalkItem::Error(LibraryWalkError {
                    path: Some(root.to_path_buf()),
                    kind: LibraryWalkErrorKind::Unknown,
                    detail: "provider reported that the walk root is missing".to_owned(),
                }));
                return;
            }
            Err(error) => {
                visitor.visit(LibraryWalkItem::Error(walk_error(root, &error)));
                return;
            }
        };
        let root_entry = LibraryEntry {
            path: root.to_path_buf(),
            is_file: root_facts.is_file,
            metadata: Some(metadata_from_facts(root, &root_facts)),
        };
        if visitor.visit(LibraryWalkItem::Entry(root_entry)) == LibraryWalkControl::Stop {
            return;
        }
        self.emit_children(root, order, visitor);
    }
}

fn path_uri(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn metadata_from_facts(at: &Path, facts: &SourceFacts) -> LibraryPathMetadata {
    LibraryPathMetadata {
        is_file: facts.is_file,
        is_directory: facts.is_directory,
        size: facts.size_bytes,
        modified: modified_time(facts.modified_unix_ms),
        identity: document_identity(at, &facts.document_id),
    }
}

fn metadata_from_child(child: &SourceChild) -> LibraryPathMetadata {
    LibraryPathMetadata {
        is_file: child.is_file,
        is_directory: child.is_directory,
        size: child.size_bytes,
        modified: modified_time(child.modified_unix_ms),
        identity: document_identity(Path::new(&child.uri), &child.document_id),
    }
}

fn modified_time(unix_ms: Option<i64>) -> Option<SystemTime> {
    let unix_ms = u64::try_from(unix_ms?).ok()?;
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_millis(unix_ms))
}

fn source_io_error(error: SafSourceError) -> io::Error {
    let kind = match &error {
        SafSourceError::PermissionDenied { .. } => io::ErrorKind::PermissionDenied,
        SafSourceError::Io { .. } | SafSourceError::Unknown { .. } => io::ErrorKind::Other,
    };
    io::Error::new(kind, error)
}

fn walk_error(directory: &Path, error: &SafSourceError) -> LibraryWalkError {
    let kind = match error {
        SafSourceError::PermissionDenied { .. } => LibraryWalkErrorKind::PermissionDenied,
        SafSourceError::Io { .. } => LibraryWalkErrorKind::Io,
        SafSourceError::Unknown { .. } => LibraryWalkErrorKind::Unknown,
    };
    LibraryWalkError {
        path: Some(directory.to_path_buf()),
        kind,
        detail: error.to_string(),
    }
}

fn document_identity(uri: &Path, document_id: &str) -> Option<(u64, u64)> {
    if document_id.is_empty() {
        return None;
    }
    let uri = uri.to_str()?;
    let authority = uri.strip_prefix("content://")?.split('/').next()?;
    if authority.is_empty() {
        return None;
    }

    let digest = Sha256::new()
        .chain_update(authority.as_bytes())
        .chain_update([0])
        .chain_update(document_id.as_bytes())
        .finalize();
    Some((
        u64::from_be_bytes(digest[0..8].try_into().ok()?),
        u64::from_be_bytes(digest[8..16].try_into().ok()?),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::{Read, Seek, SeekFrom};
    use std::os::fd::IntoRawFd;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use reprise_core::library::source::{
        LibraryLinkMode, LibraryPathPresence, LibrarySource, LibraryWalkControl, LibraryWalkItem,
        LibraryWalkOrder, LibraryWalkVisitor, UnixLibrarySource,
    };

    use super::{
        document_identity, BridgedSource, SafSource, SafSourceError, SourceChild, SourceFacts,
    };

    struct PresentSource;

    impl SafSource for PresentSource {
        fn residence_token(&self, _uri: String) -> Result<Option<i64>, SafSourceError> {
            Ok(Some(41))
        }

        fn probe(
            &self,
            _uri: String,
            _follow_links: bool,
        ) -> Result<Option<SourceFacts>, SafSourceError> {
            Ok(Some(SourceFacts {
                is_file: true,
                is_directory: false,
                size_bytes: Some(12_066),
                modified_unix_ms: Some(1_775_000_123_456),
                document_id: "primary:Music/Album/song.flac".to_owned(),
            }))
        }

        fn list_children(&self, _uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
            Ok(Vec::new())
        }

        fn open_read_fd(&self, _uri: String) -> Result<i32, SafSourceError> {
            Err(SafSourceError::Unknown {
                detail: "not used by this test".to_owned(),
            })
        }
    }

    #[test]
    fn probe_projects_provider_facts_without_fabricating_missing_values() {
        let source = BridgedSource::new(Box::new(PresentSource));

        let LibraryPathPresence::Present(metadata) = source.probe(
                Path::new(
                    "content://com.android.externalstorage.documents/document/primary%3AMusic%2FAlbum%2Fsong.flac",
                ),
                LibraryLinkMode::NoFollow,
            ) else {
                panic!("the provider confirmed that the document exists");
            };

        assert!(metadata.is_file);
        assert!(!metadata.is_directory);
        assert_eq!(metadata.size, Some(12_066));
        assert_eq!(
            metadata
                .modified
                .expect("the provider supplied a modification time")
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
            1_775_000_123_456
        );
        assert!(
            metadata.identity.is_some(),
            "the stable provider document id must reach Core as stable identity metadata"
        );
    }

    #[test]
    fn document_identity_is_stable_and_provider_scoped() {
        let document_id = "primary:Music/Album/song.flac";
        let external = Path::new(
            "content://com.android.externalstorage.documents/document/primary%3AMusic%2FAlbum%2Fsong.flac",
        );
        let other_provider = Path::new("content://example.provider/document/song.flac");

        assert_eq!(
            document_identity(external, document_id),
            Some((0x8b2b_afa4_6193_bf64, 0x9c7b_dbc9_a3c7_ad67))
        );
        assert_ne!(
            document_identity(external, document_id),
            document_identity(other_provider, document_id),
            "the same provider-local id must not collide across providers"
        );
    }

    enum ProbeOutcome {
        Missing,
        Failed,
    }

    struct ProbeSource(ProbeOutcome);

    impl SafSource for ProbeSource {
        fn residence_token(&self, _uri: String) -> Result<Option<i64>, SafSourceError> {
            Ok(Some(41))
        }

        fn probe(
            &self,
            _uri: String,
            _follow_links: bool,
        ) -> Result<Option<SourceFacts>, SafSourceError> {
            match self.0 {
                ProbeOutcome::Missing => Ok(None),
                ProbeOutcome::Failed => Err(SafSourceError::Io {
                    detail: "Binder transaction failed".to_owned(),
                }),
            }
        }

        fn list_children(&self, _uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
            Ok(Vec::new())
        }

        fn open_read_fd(&self, _uri: String) -> Result<i32, SafSourceError> {
            Err(SafSourceError::Unknown {
                detail: "not used by this test".to_owned(),
            })
        }
    }

    #[test]
    fn probe_keeps_confirmed_absence_distinct_from_provider_failure() {
        let path = Path::new("content://provider/document/song.flac");
        let missing = BridgedSource::new(Box::new(ProbeSource(ProbeOutcome::Missing)));
        let failed = BridgedSource::new(Box::new(ProbeSource(ProbeOutcome::Failed)));

        assert_eq!(
            missing.probe(path, LibraryLinkMode::Follow),
            LibraryPathPresence::Absent
        );
        assert_eq!(
            failed.probe(path, LibraryLinkMode::Follow),
            LibraryPathPresence::Unknown
        );
    }

    struct DescriptorSource {
        descriptor: Mutex<Option<i32>>,
    }

    impl SafSource for DescriptorSource {
        fn residence_token(&self, _uri: String) -> Result<Option<i64>, SafSourceError> {
            Ok(Some(41))
        }

        fn probe(
            &self,
            _uri: String,
            _follow_links: bool,
        ) -> Result<Option<SourceFacts>, SafSourceError> {
            Ok(None)
        }

        fn list_children(&self, _uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
            Ok(Vec::new())
        }

        fn open_read_fd(&self, _uri: String) -> Result<i32, SafSourceError> {
            self.descriptor
                .lock()
                .unwrap()
                .take()
                .ok_or_else(|| SafSourceError::Io {
                    detail: "descriptor was already transferred".to_owned(),
                })
        }
    }

    #[test]
    fn open_read_adopts_the_descriptor_without_copying_to_a_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.flac");
        std::fs::write(&path, b"provider bytes").unwrap();
        let descriptor = std::fs::File::open(path).unwrap().into_raw_fd();
        let source = BridgedSource::new(Box::new(DescriptorSource {
            descriptor: Mutex::new(Some(descriptor)),
        }));

        let mut handle = source
            .open_read(Path::new("content://provider/document/source.flac"))
            .unwrap();
        let mut content = String::new();
        handle.read_to_string(&mut content).unwrap();
        assert_eq!(content, "provider bytes");

        handle.seek(SeekFrom::Start(9)).unwrap();
        let mut tail = String::new();
        handle.read_to_string(&mut tail).unwrap();
        assert_eq!(tail, "bytes");
    }

    struct TreeSource {
        children: HashMap<String, Result<Vec<SourceChild>, SafSourceError>>,
        probe_calls: Arc<AtomicUsize>,
    }

    impl SafSource for TreeSource {
        fn residence_token(&self, _uri: String) -> Result<Option<i64>, SafSourceError> {
            Ok(Some(41))
        }

        fn probe(
            &self,
            _uri: String,
            _follow_links: bool,
        ) -> Result<Option<SourceFacts>, SafSourceError> {
            self.probe_calls.fetch_add(1, Ordering::Relaxed);
            Ok(Some(SourceFacts {
                is_file: false,
                is_directory: true,
                size_bytes: None,
                modified_unix_ms: None,
                document_id: "primary:Music".to_owned(),
            }))
        }

        fn list_children(&self, uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
            self.children.get(&uri).cloned().unwrap_or_else(|| {
                Err(SafSourceError::Unknown {
                    detail: format!("fixture has no directory {uri}"),
                })
            })
        }

        fn open_read_fd(&self, _uri: String) -> Result<i32, SafSourceError> {
            Err(SafSourceError::Unknown {
                detail: "not used by this test".to_owned(),
            })
        }
    }

    fn child(
        uri: &str,
        display_name: &str,
        document_id: &str,
        is_file: bool,
        size_bytes: Option<u64>,
    ) -> SourceChild {
        SourceChild {
            uri: uri.to_owned(),
            display_name: display_name.to_owned(),
            is_file,
            is_directory: !is_file,
            size_bytes,
            modified_unix_ms: Some(1_775_000_000_000),
            document_id: document_id.to_owned(),
        }
    }

    #[derive(Default)]
    struct AudioPaths {
        root: std::path::PathBuf,
        paths: Vec<std::path::PathBuf>,
        metadata: Vec<reprise_core::library::source::LibraryPathMetadata>,
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
                if let Some(metadata) = entry.metadata {
                    self.metadata.push(metadata);
                }
            }
            LibraryWalkControl::Continue
        }
    }

    #[test]
    fn derived_walk_matches_unix_filename_order_and_audio_filtering() {
        let unix_root = tempfile::tempdir().unwrap();
        std::fs::create_dir(unix_root.path().join("Album")).unwrap();
        std::fs::write(unix_root.path().join("Album/notes.txt"), b"notes").unwrap();
        std::fs::write(unix_root.path().join("Album/song.FLAC"), b"audio").unwrap();
        std::fs::write(unix_root.path().join("loose.flac"), b"audio").unwrap();

        let root = "content://com.android.externalstorage.documents/tree/primary%3AMusic";
        let album = format!("{root}/document/primary%3AMusic%2FAlbum");
        let notes = format!("{root}/document/primary%3AMusic%2FAlbum%2Fnotes.txt");
        let song = format!("{root}/document/primary%3AMusic%2FAlbum%2Fsong.FLAC");
        let loose = format!("{root}/document/primary%3AMusic%2Floose.flac");
        let probe_calls = Arc::new(AtomicUsize::new(0));
        let source = TreeSource {
            children: HashMap::from([
                (
                    root.to_owned(),
                    Ok(vec![
                        child(
                            &loose,
                            "loose.flac",
                            "primary:Music/loose.flac",
                            true,
                            Some(5),
                        ),
                        child(&album, "Album", "primary:Music/Album", false, None),
                    ]),
                ),
                (
                    album,
                    Ok(vec![
                        child(
                            &song,
                            "song.FLAC",
                            "primary:Music/Album/song.FLAC",
                            true,
                            Some(5),
                        ),
                        child(
                            &notes,
                            "notes.txt",
                            "primary:Music/Album/notes.txt",
                            true,
                            Some(5),
                        ),
                    ]),
                ),
            ]),
            probe_calls: Arc::clone(&probe_calls),
        };
        let bridged = BridgedSource::new(Box::new(source));

        let mut unix_paths = AudioPaths {
            root: unix_root.path().to_path_buf(),
            ..AudioPaths::default()
        };
        UnixLibrarySource.walk(
            unix_root.path(),
            LibraryWalkOrder::FileName,
            &mut unix_paths,
        );
        let mut bridged_paths = AudioPaths {
            root: root.into(),
            ..AudioPaths::default()
        };
        bridged.walk(
            Path::new(root),
            LibraryWalkOrder::FileName,
            &mut bridged_paths,
        );

        assert_eq!(
            unix_paths.paths,
            vec![
                std::path::PathBuf::from("Album/song.FLAC"),
                std::path::PathBuf::from("loose.flac"),
            ]
        );
        assert_eq!(
            bridged_paths.paths,
            vec![
                Path::new(&song).strip_prefix(root).unwrap().to_path_buf(),
                Path::new(&loose).strip_prefix(root).unwrap().to_path_buf(),
            ],
            "the opaque SAF paths differ from Unix paths, but the same filename order and audio filter apply"
        );
        assert_eq!(bridged_paths.metadata.len(), 2);
        assert!(bridged_paths
            .metadata
            .iter()
            .all(|metadata| metadata.size == Some(5) && metadata.identity.is_some()));
        assert_eq!(
            probe_calls.load(Ordering::Relaxed),
            1,
            "walk may probe its root once but must carry child metadata without per-file probes"
        );

        let mut native_paths = AudioPaths {
            root: root.into(),
            ..AudioPaths::default()
        };
        bridged.walk(Path::new(root), LibraryWalkOrder::Native, &mut native_paths);
        assert_eq!(
            native_paths.paths,
            vec![
                Path::new(&loose).strip_prefix(root).unwrap().to_path_buf(),
                Path::new(&song).strip_prefix(root).unwrap().to_path_buf(),
            ],
            "native order must preserve each provider cursor's sibling order"
        );
        assert_eq!(probe_calls.load(Ordering::Relaxed), 2);
    }

    #[derive(Default)]
    struct CollectedWalk {
        items: Vec<LibraryWalkItem>,
        stop_after: Option<usize>,
    }

    impl LibraryWalkVisitor for CollectedWalk {
        fn visit(&mut self, item: LibraryWalkItem) -> LibraryWalkControl {
            self.items.push(item);
            if self.stop_after == Some(self.items.len()) {
                LibraryWalkControl::Stop
            } else {
                LibraryWalkControl::Continue
            }
        }
    }

    fn failing_tree() -> (BridgedSource, String, String, String) {
        let root = "content://provider/tree/music".to_owned();
        let blocked = format!("{root}/blocked");
        let later = format!("{root}/later.flac");
        let source = TreeSource {
            children: HashMap::from([
                (
                    root.clone(),
                    Ok(vec![
                        child(&blocked, "blocked", "music/blocked", false, None),
                        child(&later, "later.flac", "music/later.flac", true, Some(5)),
                    ]),
                ),
                (
                    blocked.clone(),
                    Err(SafSourceError::PermissionDenied {
                        detail: "grant revoked for this directory".to_owned(),
                    }),
                ),
            ]),
            probe_calls: Arc::new(AtomicUsize::new(0)),
        };
        (BridgedSource::new(Box::new(source)), root, blocked, later)
    }

    #[test]
    fn derived_walk_delivers_subtree_errors_inline_and_continues() {
        let (source, root, blocked, later) = failing_tree();
        let mut walk = CollectedWalk::default();

        source.walk(Path::new(&root), LibraryWalkOrder::Native, &mut walk);

        assert_eq!(walk.items.len(), 4);
        assert!(matches!(
            &walk.items[0],
            LibraryWalkItem::Entry(entry) if entry.path == Path::new(&root)
        ));
        assert!(matches!(
            &walk.items[1],
            LibraryWalkItem::Entry(entry) if entry.path == Path::new(&blocked)
        ));
        assert!(matches!(
            &walk.items[2],
            LibraryWalkItem::Error(error)
                if error.path.as_deref() == Some(Path::new(&blocked))
                    && error.kind == reprise_core::library::source::LibraryWalkErrorKind::PermissionDenied
                    && error.detail.contains("grant revoked")
        ));
        assert!(matches!(
            &walk.items[3],
            LibraryWalkItem::Entry(entry)
                if entry.path == Path::new(&later)
                    && entry.metadata.as_ref().is_some_and(|metadata| metadata.size == Some(5))
        ));
    }

    #[test]
    fn derived_walk_stops_before_entering_or_listing_the_next_item() {
        let (source, root, blocked, _) = failing_tree();
        let mut walk = CollectedWalk {
            stop_after: Some(2),
            ..CollectedWalk::default()
        };

        source.walk(Path::new(&root), LibraryWalkOrder::Native, &mut walk);

        assert_eq!(walk.items.len(), 2);
        assert!(matches!(
            &walk.items[1],
            LibraryWalkItem::Entry(entry) if entry.path == Path::new(&blocked)
        ));
    }
}
