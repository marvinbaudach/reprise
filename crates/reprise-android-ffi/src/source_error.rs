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
        // `walk_error` is reached only from `list_children`, whose failures
        // must stay loud. Kotlin therefore never emits `NotFound` here; a new
        // walk-error kind would represent an impossible and unsafe value.
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
