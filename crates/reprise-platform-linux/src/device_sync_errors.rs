use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopyOutcome {
    Copied { relative_path: String },
}

/// Which step of a managed write produced the failure underneath.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteStep {
    ResolveStorage,
    CreateDirectories,
    CopyTarget,
    VerifyTarget,
    Publish,
}

impl fmt::Display for WriteStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ResolveStorage => "resolving the target storage",
            Self::CreateDirectories => "creating the destination directory",
            Self::CopyTarget => "copying the destination file",
            Self::VerifyTarget => "verifying the destination file",
            Self::Publish => "publishing the destination file",
        })
    }
}

#[derive(Debug)]
pub enum DeviceIoError {
    InvalidRelativePath,
    SizeMismatch {
        expected: u64,
        actual: u64,
    },
    PublishNotApplied {
        name: String,
    },
    DuringWrite {
        step: WriteStep,
        source: Box<DeviceIoError>,
    },
    Io(gio::glib::Error),
    /// Design 7d: the chosen `StorageId` no longer matches any storage volume at the device root —
    /// e.g. an SD card was removed since the browser last listed storages.
    StorageNotFound,
    /// Design 7d's "New folder": a folder with that name already exists at the chosen location.
    FolderAlreadyExists,
    /// Design 7d's root-creation error path: the device refused to create a folder directly at a
    /// storage volume's own top level.
    CannotCreateAtStorageRoot(gio::glib::Error),
}

impl fmt::Display for DeviceIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRelativePath => formatter.write_str("invalid managed device path"),
            Self::SizeMismatch { expected, actual } => formatter.write_fmt(format_args!(
                "device file has {actual} bytes, expected {expected}"
            )),
            Self::PublishNotApplied { name } => formatter.write_fmt(format_args!(
                "the device acknowledged publishing {name} but the file never appeared"
            )),
            Self::DuringWrite { step, source } => write!(formatter, "{step} failed: {source}"),
            Self::Io(error) => write!(formatter, "device I/O failed: {error}"),
            Self::StorageNotFound => {
                formatter.write_str("the selected storage is no longer available on this device")
            }
            Self::FolderAlreadyExists => {
                formatter.write_str("a folder with that name already exists here")
            }
            Self::CannotCreateAtStorageRoot(error) => formatter.write_fmt(format_args!(
                "this device does not allow creating folders directly in the storage root: {error}"
            )),
        }
    }
}

impl std::error::Error for DeviceIoError {}

impl DeviceIoError {
    pub(super) fn during(self, step: WriteStep) -> Self {
        Self::DuringWrite {
            step,
            source: self.into(),
        }
    }
}

impl From<gio::glib::Error> for DeviceIoError {
    fn from(error: gio::glib::Error) -> Self {
        Self::Io(error)
    }
}
