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

use super::source::{BridgedSource, SafSource, SafSourceError, SourceChild, SourceFacts};

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
            display_name: Some("song.flac".to_owned()),
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
fn probe_projects_provider_facts_without_fabricating_file_identity() {
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
    assert_eq!(
        metadata.identity, None,
        "a provider document id is not a file identity unless rename stability is guaranteed"
    );
}

#[derive(Clone, Copy)]
enum ProbeOutcome {
    Missing,
    Failed,
    NotFound,
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
            ProbeOutcome::NotFound => Err(SafSourceError::NotFound {
                detail: "provider confirmed the document is gone".to_owned(),
            }),
        }
    }

    fn list_children(&self, _uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
        Ok(Vec::new())
    }

    fn open_read_fd(&self, _uri: String) -> Result<i32, SafSourceError> {
        match self.0 {
            ProbeOutcome::NotFound => Err(SafSourceError::NotFound {
                detail: "provider confirmed the document is gone".to_owned(),
            }),
            _ => Err(SafSourceError::Unknown {
                detail: "not used by this test".to_owned(),
            }),
        }
    }
}

#[test]
fn probe_keeps_confirmed_absence_distinct_from_provider_failure() {
    let path = Path::new("content://provider/document/song.flac");
    let missing = BridgedSource::new(Box::new(ProbeSource(ProbeOutcome::Missing)));
    let failed = BridgedSource::new(Box::new(ProbeSource(ProbeOutcome::Failed)));
    let not_found = BridgedSource::new(Box::new(ProbeSource(ProbeOutcome::NotFound)));

    assert_eq!(
        missing.probe(path, LibraryLinkMode::Follow),
        LibraryPathPresence::Absent
    );
    assert_eq!(
        failed.probe(path, LibraryLinkMode::Follow),
        LibraryPathPresence::Unknown
    );
    assert_eq!(
        not_found.probe(path, LibraryLinkMode::Follow),
        LibraryPathPresence::Absent
    );
}

#[test]
fn open_read_maps_confirmed_absence_to_not_found() {
    let source = BridgedSource::new(Box::new(ProbeSource(ProbeOutcome::NotFound)));
    let error = match source.open_read(Path::new("content://provider/document/gone.flac")) {
        Ok(_) => panic!("a missing provider document must not open"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn parent_of_rebuilds_nested_documents_and_normalizes_the_tree_root() {
    let tree = "content://com.android.externalstorage.documents/tree/primary%3AMusic%2FReprise";
    let source = BridgedSource::with_tree_root(Box::new(ProbeSource(ProbeOutcome::Missing)), tree);
    let album = format!("{tree}/document/primary%3AMusic%2FReprise%2FAlbum");
    let track = format!("{album}%2Fsong.flac");
    assert_eq!(
        source.parent_of(Path::new(&track)),
        Some(std::path::PathBuf::from(&album))
    );
    assert_eq!(source.parent_of(Path::new(&album)), Some(tree.into()));
    assert_eq!(source.parent_of(Path::new(tree)), None);
    assert_eq!(
        source.parent_of(Path::new(&format!("{tree}/document/opaque-id"))),
        None
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
            display_name: Some("Music".to_owned()),
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
        display_name: Some(display_name.to_owned()),
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
        .all(|metadata| metadata.size == Some(5) && metadata.identity.is_none()));
    assert_eq!(
        bridged.relative_path(Path::new(root), Path::new(&song)),
        Some(std::path::PathBuf::from("Album/song.FLAC")),
        "nested device identities come from cursor names, never document URI parsing"
    );
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
