use std::io;
use std::path::Path;

use reprise_core::library::source::{LibraryWalkError, LibraryWalkErrorKind};

use crate::source::SafSourceError;

pub(super) fn source_io_error(error: SafSourceError) -> io::Error {
    let kind = match &error {
        SafSourceError::PermissionDenied { .. } => io::ErrorKind::PermissionDenied,
        SafSourceError::Io { .. } | SafSourceError::Unknown { .. } => io::ErrorKind::Other,
    };
    io::Error::new(kind, error)
}

pub(super) fn walk_error(directory: &Path, error: &SafSourceError) -> LibraryWalkError {
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
