use std::fs::File;
use std::io;
use std::os::fd::FromRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use reprise_core::library::source::{
    LibraryDirectoryEntry, LibraryEntry, LibraryLinkMode, LibraryPathMetadata, LibraryPathPresence,
    LibraryReadHandle, LibrarySource, LibraryWalkControl, LibraryWalkError, LibraryWalkErrorKind,
    LibraryWalkItem, LibraryWalkOrder, LibraryWalkVisitor,
};

use crate::source_error::{source_io_error, walk_error};
use crate::source_names::SourceNames;

/// Provider facts returned by one SAF document query.
#[derive(Clone, Debug, uniffi::Record)]
pub struct SourceFacts {
    pub display_name: Option<String>,
    pub is_file: bool,
    pub is_directory: bool,
    pub size_bytes: Option<u64>,
    pub modified_unix_ms: Option<i64>,
    /// Provider-local identifier used to address this document.
    pub document_id: String,
}

/// One immediate child returned by a SAF directory cursor.
#[derive(Clone, Debug, uniffi::Record)]
pub struct SourceChild {
    pub uri: String,
    pub display_name: Option<String>,
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
    /// The provider answered, and the answer is that the document does not
    /// exist. Distinct from `Unknown`: this one licenses a missing verdict.
    #[error("not found: {detail}")]
    NotFound { detail: String },
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
    names: SourceNames,
    tree_root: Option<PathBuf>,
}

impl BridgedSource {
    /// Adapts `source` without a configured tree-root normalization address.
    pub fn new(source: Box<dyn SafSource>) -> Self {
        Self::from_source(source, None)
    }

    /// Adapts `source` and retains the tree-form root URI used by Core scans.
    pub fn with_tree_root(source: Box<dyn SafSource>, tree_uri: impl Into<PathBuf>) -> Self {
        Self::from_source(source, Some(tree_uri.into()))
    }

    fn from_source(source: Box<dyn SafSource>, tree_root: Option<PathBuf>) -> Self {
        Self {
            source,
            names: SourceNames::default(),
            tree_root,
        }
    }

    fn emit_children(
        &self,
        directory: &Path,
        relative_directory: Option<&Path>,
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

        let container_name = self.names.display_name(directory);
        for child in children {
            let path = std::path::PathBuf::from(&child.uri);
            let relative_path = relative_directory
                .zip(child.display_name.as_deref())
                .map(|(directory, name)| directory.join(name));
            self.names.remember_child(
                path.clone(),
                child.display_name.clone(),
                container_name.clone(),
            );
            if let Some(relative_path) = &relative_path {
                self.names
                    .remember_relative_path(path.clone(), relative_path.clone());
            }
            let is_directory = child.is_directory;
            let entry = LibraryEntry {
                path: path.clone(),
                is_file: child.is_file,
                metadata: Some(metadata_from_child(&child)),
            };
            if visitor.visit(LibraryWalkItem::Entry(entry)) == LibraryWalkControl::Stop {
                return LibraryWalkControl::Stop;
            }
            if is_directory
                && self.emit_children(&path, relative_path.as_deref(), order, visitor)
                    == LibraryWalkControl::Stop
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

    fn mount_point(&self, _at: &Path) -> Option<std::path::PathBuf> {
        None
    }

    fn display_name(&self, at: &Path) -> Option<String> {
        self.names.display_name(at)
    }

    fn container_name(&self, at: &Path) -> Option<String> {
        self.names.container_name(at)
    }

    fn relative_path(&self, _root: &Path, at: &Path) -> Option<PathBuf> {
        self.names.relative_path(at)
    }

    fn parent_of(&self, at: &Path) -> Option<PathBuf> {
        let uri = at.to_str()?;
        let (prefix, document_id) = uri.rsplit_once("/document/")?;
        let separator = document_id
            .as_bytes()
            .windows(3)
            .rposition(|part| part.eq_ignore_ascii_case(b"%2f"))?;
        let parent_id = &document_id[..separator];
        if self.tree_root.as_deref().and_then(encoded_tree_document_id) == Some(parent_id) {
            return self.tree_root.clone();
        }
        Some(format!("{prefix}/document/{parent_id}").into())
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
            Ok(Some(facts)) => {
                self.names
                    .remember_display_name(at.to_path_buf(), facts.display_name.clone());
                LibraryPathPresence::Present(metadata_from_facts(&facts))
            }
            Ok(None) => LibraryPathPresence::Absent,
            Err(SafSourceError::NotFound { .. }) => LibraryPathPresence::Absent,
            Err(_) => LibraryPathPresence::Unknown,
        }
    }

    fn read_directory(&self, directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
        let container_name = self.names.display_name(directory);
        self.source
            .list_children(path_uri(directory))
            .ok()
            .map(|children| {
                children
                    .into_iter()
                    .map(|child| {
                        let path = PathBuf::from(&child.uri);
                        self.names.remember_child(
                            path.clone(),
                            child.display_name.clone(),
                            container_name.clone(),
                        );
                        LibraryDirectoryEntry {
                            path,
                            metadata: Some(metadata_from_child(&child)),
                        }
                    })
                    .collect()
            })
    }

    fn walk(&self, root: &Path, order: LibraryWalkOrder, visitor: &mut dyn LibraryWalkVisitor) {
        self.names.clear_relative_paths();
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
        self.names
            .remember_display_name(root.to_path_buf(), root_facts.display_name.clone());
        self.names
            .remember_relative_path(root.to_path_buf(), PathBuf::new());
        let root_entry = LibraryEntry {
            path: root.to_path_buf(),
            is_file: root_facts.is_file,
            metadata: Some(metadata_from_facts(&root_facts)),
        };
        if visitor.visit(LibraryWalkItem::Entry(root_entry)) == LibraryWalkControl::Stop {
            return;
        }
        self.emit_children(root, Some(Path::new("")), order, visitor);
    }
}

fn path_uri(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn encoded_tree_document_id(tree_root: &Path) -> Option<&str> {
    let (_, document_id) = tree_root.to_str()?.rsplit_once("/tree/")?;
    (!document_id.is_empty() && !document_id.contains('/')).then_some(document_id)
}

fn metadata_from_facts(facts: &SourceFacts) -> LibraryPathMetadata {
    LibraryPathMetadata {
        is_file: facts.is_file,
        is_directory: facts.is_directory,
        size: facts.size_bytes,
        modified: modified_time(facts.modified_unix_ms),
        identity: None,
    }
}

fn metadata_from_child(child: &SourceChild) -> LibraryPathMetadata {
    LibraryPathMetadata {
        is_file: child.is_file,
        is_directory: child.is_directory,
        size: child.size_bytes,
        modified: modified_time(child.modified_unix_ms),
        identity: None,
    }
}

fn modified_time(unix_ms: Option<i64>) -> Option<SystemTime> {
    let unix_ms = u64::try_from(unix_ms?).ok()?;
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_millis(unix_ms))
}
