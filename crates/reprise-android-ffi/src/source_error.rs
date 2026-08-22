use std::io;
use std::path::Path;

use reprise_core::library::source::{LibraryWalkError, LibraryWalkErrorKind};

use crate::source::SafSourceError;

pub(super) fn source_io_error(error: SafSourceError) -> io::Error {
    let kind = match &error {
        SafSourceError::PermissionDenied { .. } => io::ErrorKind::PermissionDenied,
        SafSourceError::NotFound { .. } => io::ErrorKind::NotFound,
        SafSourceError::Io { .. } | SafSourceError::Unknown { .. } => io::ErrorKind::Other,
    };
    io::Error::new(kind, error)
}

pub(super) fn walk_error(directory: &Path, error: &SafSourceError) -> LibraryWalkError {
    let kind = match error {
        SafSourceError::PermissionDenied { .. } => LibraryWalkErrorKind::PermissionDenied,
        // `list_children` failures must stay loud, so Kotlin never emits
        // `NotFound` there. A root `probe` failure also reaches this mapping,
        // but a root `NotFound` is harmless as `Unknown`: that walk sees no
        // audio, supplies no walk evidence, and leaves the root guard in charge.
        SafSourceError::NotFound { .. } => LibraryWalkErrorKind::Unknown,
        SafSourceError::Io { .. } => LibraryWalkErrorKind::Io,
        SafSourceError::Unknown { .. } => LibraryWalkErrorKind::Unknown,
    };
    LibraryWalkError {
        path: Some(directory.to_path_buf()),
        kind,
        detail: error.to_string(),
    }
}
