//! Platform-neutral library-source residence contract.
//!
//! Core owns the evidence order that distinguishes a deleted track from a
//! temporarily unreachable source. Concrete sources provide presence,
//! residence-token, and mount-boundary evidence without any one signal being
//! treated as infallible.

use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::models::MissingReason;

pub(crate) use super::source_unix::{device_id, nearest_existing_ancestor};
use super::source_unix::{file_identity, nearest_existing_ancestor_dev};

/// The sibling-order guarantee requested from a library-source traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryWalkOrder {
    /// Preserve the source adapter's native order.
    Native,
    /// Visit siblings in ascending file-name order.
    FileName,
}

/// The only entry facts traversal consumers need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryEntry {
    pub path: PathBuf,
    pub is_file: bool,
    /// Facts the source already had while enumerating this entry. `None`
    /// means the consumer may call [`LibrarySource::probe`]; it never means
    /// zero size, epoch modification time, or fabricated identity.
    pub metadata: Option<LibraryPathMetadata>,
}

/// Whether a metadata probe follows the final symbolic link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryLinkMode {
    Follow,
    NoFollow,
}

/// Source-neutral facts about one reachable path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryPathMetadata {
    pub is_file: bool,
    pub is_directory: bool,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    /// Stable file identity when the source has one. Unix uses `(st_dev,
    /// st_ino)`; a source without an equally stable pair returns `None` and
    /// lets move detection use its fingerprint fallback.
    ///
    /// **A platform arm must never fabricate an identity.** The non-Unix arm used
    /// to return `(0, 0)` under a comment claiming it was never reached at
    /// runtime — true only while the app was Linux-only. A Tauri desktop makes it
    /// false, and then `WHERE device = 0 AND inode = 0` matches every row scanned
    /// there; with exactly one valid candidate that attaches one track's history
    /// to another, silently. `None` is the only honest answer for a platform
    /// without a stable identity.
    pub identity: Option<(u64, u64)>,
}

/// What a source can establish about one library path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryPathPresence {
    /// The item exists, with every fact the source could establish.
    Present(LibraryPathMetadata),
    /// The source confirmed that the item does not exist.
    ///
    /// This is the only state that may license a missing-verdict write.
    Absent,
    /// The source could not determine whether the item exists.
    ///
    /// This state never licenses a missing verdict. A source that cannot
    /// reach its backing store answers `Unknown` rather than guessing either
    /// presence or absence.
    Unknown,
}

/// One immediate child returned by [`LibrarySource::read_directory`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryDirectoryEntry {
    pub path: PathBuf,
    /// Facts already present in the directory cursor. Unix deliberately
    /// leaves this `None`, avoiding an eager stat of every child. When
    /// present, these describe the entry itself without following a final
    /// symbolic link, matching [`LibraryLinkMode::NoFollow`]; writeback
    /// cleanup relies on that guarantee before removing an abandoned temp.
    pub metadata: Option<LibraryPathMetadata>,
}

/// Source-neutral traversal error classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryWalkErrorKind {
    PermissionDenied,
    Io,
    Unknown,
}

/// A traversal failure delivered in source order beside successful entries.
///
/// A failure is not the end of the walk: the source reports it and carries on
/// wherever it can, so one unreadable directory costs its own subtree and
/// nothing more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryWalkError {
    /// The container the source could not enter — **never an item inside it**.
    /// Those were never seen, and naming one here would invite a consumer to
    /// record a failure for an item it has no evidence about. `None` only when
    /// the source cannot say where the failure was; consumers then attribute it
    /// to the walk's root.
    pub path: Option<PathBuf>,
    pub kind: LibraryWalkErrorKind,
    /// Free-form diagnostic text for logs and the import-error catalog. Not
    /// translated and not shown as a primary message — [`Self::kind`] is what
    /// a surface renders.
    pub detail: String,
}

/// One event in a library-source traversal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryWalkItem {
    Entry(LibraryEntry),
    Error(LibraryWalkError),
}

/// Whether a traversal visitor wants the source to continue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryWalkControl {
    Continue,
    Stop,
}

/// Named callback interface used by [`LibrarySource::walk`].
///
/// Keeping this callback named avoids putting a Rust closure in the source
/// contract, while the control result lets cancellation-sensitive consumers
/// stop a source without first materializing its entire tree.
pub trait LibraryWalkVisitor {
    fn visit(&mut self, item: LibraryWalkItem) -> LibraryWalkControl;
}

trait LibraryReadIo: Read + Seek + Send {}

impl<T> LibraryReadIo for T where T: Read + Seek + Send {}

/// An opaque readable and seekable handle to one library item.
///
/// The handle is concrete at the [`LibrarySource`] boundary: neither
/// `std::fs::File` nor a foreign `dyn Read + Seek` leaks into the source
/// contract. Sources may back it with a Unix file descriptor, an in-memory
/// cursor, or another reader with the same measured capabilities.
///
/// Seeking is required rather than speculative. Lofty 0.24's
/// `AudioFile::read_from` consumes `Read + Seek`, while the other current
/// consumers use only the `Read` half to materialize complete sidecar or image
/// content. It does foreclose a source that can only stream — a pipe-backed
/// descriptor, say — which is a real cost, accepted because tag parsing needs
/// to seek and no consumer can be served without it.
///
/// `Send` because [`LibrarySource`] is `Send + Sync` and a handle opened on one
/// thread will be read on another as soon as an Android source hands out a
/// descriptor from a Binder callback. Every backing type today already is.
pub struct LibraryReadHandle(Box<dyn LibraryReadIo>);

impl LibraryReadHandle {
    pub fn new(reader: impl Read + Seek + Send + 'static) -> Self {
        Self(Box::new(reader))
    }
}

impl Read for LibraryReadHandle {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}

impl Seek for LibraryReadHandle {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.0.seek(position)
    }
}

struct ClosureWalkVisitor<F>(F);

impl<F> LibraryWalkVisitor for ClosureWalkVisitor<F>
where
    F: FnMut(LibraryWalkItem) -> LibraryWalkControl,
{
    fn visit(&mut self, item: LibraryWalkItem) -> LibraryWalkControl {
        (self.0)(item)
    }
}

/// Rust-only convenience around the named visitor interface. The closure is
/// deliberately kept out of [`LibrarySource`] itself so the source contract
/// remains object-safe and suitable for a future foreign adapter.
pub(crate) fn walk_with(
    source: &dyn LibrarySource,
    root: &Path,
    order: LibraryWalkOrder,
    visit: impl FnMut(LibraryWalkItem) -> LibraryWalkControl,
) {
    source.walk(root, order, &mut ClosureWalkVisitor(visit));
}

/// The residence and reachability capability every library source provides.
///
/// A source without a stable residence token returns `None` from
/// [`Self::residence_token`]. That documented degradation produces
/// [`MissingReason::Unknown`] unless a path-backed source can positively prove
/// that the item's parent still exists, and never fabricates an identity.
///
/// **No question a source alone can answer has a default implementation.** A
/// source that cannot yet answer one must fail to compile rather than guess.
/// [`Self::reachability`] is the one exception and a deliberate one: it decides
/// nothing by itself, it only compares what [`Self::residence_token`] returned,
/// so a source that answers the primitive gets the verdict for free and cannot
/// get it wrong.
pub trait LibrarySource: Send + Sync {
    /// Returns the stable residence token of the nearest reachable location at
    /// `at`, or `None` when this source cannot provide one.
    fn residence_token(&self, at: &Path) -> Option<i64>;

    /// The grouping boundary `at` belongs to — "what disappears together when
    /// this goes away", used to group missing tracks by the volume they shared.
    /// `None` when the source has no such notion: a DocumentsProvider tree has
    /// no mount point, and inventing one would group unrelated items.
    fn mount_point(&self, at: &Path) -> Option<PathBuf>;

    /// The name to show a person for `at`, when the item itself carries no title.
    /// A path-backed source answers with the file stem; a DocumentsProvider answers
    /// with the display name it already returns in its cursor. `None` when the
    /// source has nothing better than the identifier it was given.
    fn display_name(&self, at: &Path) -> Option<String>;

    /// The name of the container `at` sits in — an album folder, a
    /// DocumentsProvider parent — used when an item carries no album tag.
    /// `None` when the source cannot name one; callers then leave the field
    /// empty rather than inventing a name from an identifier.
    fn container_name(&self, at: &Path) -> Option<String>;

    /// The stable, source-visible path of `at` below the selected library
    /// root. Filesystem sources strip the root; document providers rebuild it
    /// from cursor display names instead of interpreting opaque document URIs.
    fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf>;

    /// The container `at` would sit in, as an address — never a source query,
    /// and no statement that either `at` or the result exists.
    ///
    /// A path-backed source answers with [`Path::parent`]. A DocumentsProvider
    /// source cannot: a tree document URI's parent is not its `Path` parent, so
    /// it rebuilds the address from the document id it encoded itself, and
    /// answers `None` whenever it cannot do so with certainty. Callers must
    /// treat `None` as "no evidence", never as "no parent".
    fn parent_of(&self, at: &Path) -> Option<PathBuf> {
        at.parent().map(Path::to_path_buf)
    }

    /// Opens `at` for reading without exposing the source's concrete storage
    /// handle. Failure is explicit: a source that cannot provide readable,
    /// seekable content must not compile with this contract unanswered.
    ///
    /// **An `Err` here is a failure to read, never a statement that the item is
    /// gone.** A revoked permission grant, dropped provider connection or
    /// transient I/O error is equivalent to [`LibraryPathPresence::Unknown`],
    /// never [`LibraryPathPresence::Absent`].
    fn open_read(&self, at: &Path) -> io::Result<LibraryReadHandle>;

    /// Returns whether `at` is present, absent, or could not be checked.
    ///
    /// `links` is explicit because most Class-A presence checks historically
    /// used `Path::metadata` and followed the final symlink, while abandoned
    /// writeback cleanup used `DirEntry::metadata` and must inspect the link
    /// itself. Keeping that distinction in the contract preserves the safety
    /// boundary rather than silently changing it during abstraction.
    fn probe(&self, at: &Path, links: LibraryLinkMode) -> LibraryPathPresence;

    /// Lists only the immediate children of `directory`, or returns `None`
    /// when the directory cannot be read. Per-child failures are skipped,
    /// matching the three existing `read_dir(...).flatten()` consumers.
    ///
    /// This is separate from recursive [`Self::walk`] because its semantics
    /// exclude both the root and descendants. Entries may carry metadata that
    /// a SAF cursor already supplied; Unix leaves it absent so listing an album
    /// never adds a stat for every non-audio child merely to help another
    /// platform avoid a round trip.
    fn read_directory(&self, directory: &Path) -> Option<Vec<LibraryDirectoryEntry>>;

    /// Traverses `root` once, delivering entries and recoverable traversal
    /// errors to `visitor` in source order until exhaustion or
    /// [`LibraryWalkControl::Stop`]. The root entry itself is included when
    /// the adapter exposes it, matching the Unix adapter's `walkdir` behavior.
    ///
    /// The named visitor keeps this method object-safe and source-neutral:
    /// neither `walkdir::DirEntry`, a closure, an anonymous tuple, nor an
    /// opaque iterator crosses the interface. It also keeps traversal
    /// streaming, so a SAF adapter need not retain a potentially large tree
    /// merely to support cancellation.
    ///
    /// `FileName` orders siblings; it does not flatten the depth-first tree.
    /// `Native` deliberately leaves sibling order to the adapter. Both modes
    /// must deliver directory failures inline and continue when possible.
    ///
    /// There is no return value, and none is needed: a source that cannot open
    /// `root` at all reports that as a single [`LibraryWalkItem::Error`] naming
    /// `root` and then produces nothing further. Consumers already treat "the
    /// walk yielded no entries" as its own condition — see `scan_folder_inner`'s
    /// root guard — so a failure to start needs no separate channel.
    fn walk(&self, root: &Path, order: LibraryWalkOrder, visitor: &mut dyn LibraryWalkVisitor);

    /// Classifies why an item already known to be missing at `at` is missing,
    /// given the residence token recorded for it at scan time (`tracks.device`,
    /// `None` for a row that predates schema v2 or whose residence lookup
    /// failed on the last scan — see `library::scanner::scanner_file_metadata`'s
    /// doc comment).
    ///
    /// Reachability is stronger than token identity. For an absolute,
    /// path-backed location, a confirmed-present parent proves that the source
    /// is reachable and the item itself was deleted. This check comes first
    /// because Linux `st_dev` is not stable across btrfs subvolume remounts: in
    /// the measured library the same `/home` source carried tokens 30, 47, and
    /// 49 across scans. A stale anonymous btrfs device number must not outweigh
    /// a directory that is visibly present now.
    ///
    /// When the parent is confirmed absent, the token remains useful as a
    /// secondary signal. A matching token means the same source is reachable
    /// farther up the deleted tree (`Deleted`). A differing token means
    /// `Unmounted` only when the source can also name a concrete mount boundary;
    /// otherwise the evidence is insufficient and the result is `Unknown`.
    /// Missing stored/current tokens likewise remain `Unknown`.
    ///
    /// Opaque, non-absolute identifiers such as Android SAF content URIs have no
    /// filesystem-parent meaning. Those sources therefore retain the token
    /// comparison alone: equal is `Deleted`, different is `Unmounted`, and an
    /// unavailable token is `Unknown`. No directory reachability is invented.
    ///
    /// The token is `i64` because SQLite has no other integer type; it matches
    /// `Track::device` and the scanner metadata projection's storage cast.
    /// [`UnixLibrarySource`] round-trips exactly the `st_dev` bit pattern that
    /// cast away from `u64` on the way in, so this stays the same comparison
    /// Linux made before the trait existed.
    fn reachability(&self, at: &Path, stored: Option<i64>) -> MissingReason {
        if at.is_absolute() {
            let Some(parent) = at.parent() else {
                return MissingReason::Unknown;
            };
            match self.probe(parent, LibraryLinkMode::Follow) {
                LibraryPathPresence::Present(_) => return MissingReason::Deleted,
                LibraryPathPresence::Unknown => return MissingReason::Unknown,
                LibraryPathPresence::Absent => {}
            }
        }

        let Some(stored) = stored else {
            return MissingReason::Unknown;
        };
        match self.residence_token(at) {
            Some(current) if current == stored => MissingReason::Deleted,
            Some(_) if !at.is_absolute() || self.mount_point(at).is_some() => {
                MissingReason::Unmounted
            }
            Some(_) => MissingReason::Unknown,
            None => MissingReason::Unknown,
        }
    }
}

/// A path-backed library source whose residence token is Unix `st_dev`.
///
/// On targets without Unix metadata this source reports no token, preserving
/// the contract's honest `Unknown` degradation instead of inventing a value.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnixLibrarySource;

// The test doubles live in `library::source_test_support` (declared in
// `library/mod.rs`), because several test files outside this module use them.
// Re-exported here so the call sites that reach them through `source` keep
// working — one module, two names for it.
#[cfg(test)]
pub(crate) use super::source_test_support::ExistingPathSource;

impl LibrarySource for UnixLibrarySource {
    fn residence_token(&self, at: &Path) -> Option<i64> {
        nearest_existing_ancestor_dev(at).map(|device| device as i64)
    }

    fn mount_point(&self, at: &Path) -> Option<PathBuf> {
        super::mounts::mount_point_of(at)
    }

    fn display_name(&self, at: &Path) -> Option<String> {
        at.file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_owned)
    }

    fn container_name(&self, at: &Path) -> Option<String> {
        at.parent()?
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    }

    fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf> {
        at.strip_prefix(root).ok().map(Path::to_path_buf)
    }

    fn open_read(&self, at: &Path) -> io::Result<LibraryReadHandle> {
        std::fs::File::open(at).map(LibraryReadHandle::new)
    }

    fn probe(&self, at: &Path, links: LibraryLinkMode) -> LibraryPathPresence {
        let result = match links {
            LibraryLinkMode::Follow => std::fs::metadata(at),
            LibraryLinkMode::NoFollow => std::fs::symlink_metadata(at),
        };
        let metadata = match result {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return LibraryPathPresence::Absent;
            }
            Err(_) => return LibraryPathPresence::Unknown,
        };
        LibraryPathPresence::Present(LibraryPathMetadata {
            is_file: metadata.is_file(),
            is_directory: metadata.is_dir(),
            size: Some(metadata.len()),
            modified: metadata.modified().ok(),
            identity: file_identity(&metadata),
        })
    }

    fn read_directory(&self, directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
        Some(
            std::fs::read_dir(directory)
                .ok()?
                .filter_map(|entry| {
                    entry.ok().map(|entry| LibraryDirectoryEntry {
                        path: entry.path(),
                        metadata: None,
                    })
                })
                .collect(),
        )
    }

    fn walk(&self, root: &Path, order: LibraryWalkOrder, visitor: &mut dyn LibraryWalkVisitor) {
        // `follow_links(false)` is part of the source contract, not a
        // walkdir default we happen to inherit. Traversal must agree with
        // `nearest_existing_ancestor`'s lstat-based residence evidence: a
        // symlink that merely points into another source must never pull that
        // foreign tree into this source's scan.
        let walk = walkdir::WalkDir::new(root).follow_links(false);
        let walk = match order {
            LibraryWalkOrder::Native => walk,
            LibraryWalkOrder::FileName => walk.sort_by_file_name(),
        };
        for item in walk {
            let item = match item {
                Ok(entry) => LibraryWalkItem::Entry(LibraryEntry {
                    path: entry.path().to_path_buf(),
                    is_file: entry.file_type().is_file(),
                    metadata: None,
                }),
                Err(error) => {
                    let kind = match super::import_errors::classify_walkdir(&error) {
                        crate::models::ImportErrorKind::PermissionDenied => {
                            LibraryWalkErrorKind::PermissionDenied
                        }
                        crate::models::ImportErrorKind::Io => LibraryWalkErrorKind::Io,
                        _ => LibraryWalkErrorKind::Unknown,
                    };
                    LibraryWalkItem::Error(LibraryWalkError {
                        path: error.path().map(Path::to_path_buf),
                        kind,
                        detail: error.to_string(),
                    })
                }
            };
            if visitor.visit(item) == LibraryWalkControl::Stop {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Seek, SeekFrom};
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    use super::{
        LibraryDirectoryEntry, LibraryLinkMode, LibraryPathPresence, LibraryReadHandle,
        LibrarySource, LibraryWalkOrder, LibraryWalkVisitor, UnixLibrarySource,
    };
    use crate::models::MissingReason;

    fn dev_of(path: &Path) -> u64 {
        std::fs::symlink_metadata(path).unwrap().dev()
    }

    #[test]
    fn unix_source_uses_the_nearest_existing_ancestor_residence_token() {
        let dir = tempfile::tempdir().unwrap();
        let expected = dev_of(dir.path()) as i64;
        let missing_track = dir.path().join("missing/track.flac");

        assert_eq!(
            UnixLibrarySource.residence_token(&missing_track),
            Some(expected)
        );
    }

    /// The file's real device recorded: its directory still exists and still
    /// belongs to the same device, so the only honest conclusion is that the
    /// file itself was deleted.
    #[test]
    fn unix_source_reports_deleted_when_the_device_matches() {
        let dir = tempfile::tempdir().unwrap();
        let real_dev = dev_of(dir.path());
        let gone_path = dir.path().join("gone.flac");

        assert_eq!(
            UnixLibrarySource.reachability(&gone_path, Some(real_dev as i64)),
            MissingReason::Deleted
        );
    }

    /// The token comparison is not dead weight behind the parent check: when
    /// a whole album folder is deleted, the file's immediate parent is gone
    /// too, so the parent-reachable shortcut does NOT apply and the verdict
    /// falls through to the stored token. A matching token then means the
    /// filesystem the track lived on is still the one mounted here — the
    /// folder was deleted, not unmounted.
    ///
    /// Every other `Deleted` case in this module keeps the immediate parent
    /// alive and therefore never reaches this arm; without this test the
    /// fall-through path has no coverage at all.
    #[test]
    fn unix_source_reports_deleted_when_the_whole_folder_is_gone_but_the_device_matches() {
        let dir = tempfile::tempdir().unwrap();
        let real_dev = dev_of(dir.path());
        // `gone-album` is never created: the track's parent is absent, its
        // grandparent (the temp dir) is present and on `real_dev`.
        let gone_path = dir.path().join("gone-album/gone.flac");
        assert!(
            !gone_path.parent().unwrap().exists(),
            "the parent must be absent, otherwise this test silently \
             re-tests the parent-reachable shortcut instead"
        );

        assert_eq!(
            UnixLibrarySource.reachability(&gone_path, Some(real_dev as i64)),
            MissingReason::Deleted
        );
    }

    /// A btrfs subvolume can receive a different anonymous device number after
    /// a reboot. The still-existing parent is stronger evidence than that stale
    /// token: the source is reachable and only the track itself is gone.
    #[test]
    fn unix_source_reports_deleted_when_parent_exists_but_device_differs() {
        let dir = tempfile::tempdir().unwrap();
        let real_dev = dev_of(dir.path());
        let gone_path = dir.path().join("gone.flac");

        assert_eq!(
            UnixLibrarySource.reachability(&gone_path, Some(real_dev as i64 + 99_999)),
            MissingReason::Deleted
        );
    }

    /// A confirmed-present parent is sufficient evidence even when the earlier
    /// scan could not record a device token.
    #[test]
    fn unix_source_reports_deleted_when_parent_exists_without_a_stored_device() {
        let dir = tempfile::tempdir().unwrap();
        let gone_path = dir.path().join("gone.flac");

        assert_eq!(
            UnixLibrarySource.reachability(&gone_path, None),
            MissingReason::Deleted
        );
    }

    #[test]
    fn unix_source_reports_unmounted_when_parent_is_gone_and_device_differs() {
        let dir = tempfile::tempdir().unwrap();
        let real_dev = dev_of(dir.path());
        let gone_path = dir.path().join("gone-album/gone.flac");

        assert_eq!(
            UnixLibrarySource.reachability(&gone_path, Some(real_dev as i64 + 99_999)),
            MissingReason::Unmounted
        );
    }

    struct NoMountUnixSource;

    impl LibrarySource for NoMountUnixSource {
        fn residence_token(&self, at: &Path) -> Option<i64> {
            UnixLibrarySource.residence_token(at)
        }

        fn mount_point(&self, _at: &Path) -> Option<std::path::PathBuf> {
            None
        }

        fn display_name(&self, at: &Path) -> Option<String> {
            UnixLibrarySource.display_name(at)
        }

        fn container_name(&self, at: &Path) -> Option<String> {
            UnixLibrarySource.container_name(at)
        }

        fn relative_path(&self, root: &Path, at: &Path) -> Option<std::path::PathBuf> {
            UnixLibrarySource.relative_path(root, at)
        }

        fn open_read(&self, at: &Path) -> std::io::Result<LibraryReadHandle> {
            UnixLibrarySource.open_read(at)
        }

        fn probe(&self, at: &Path, links: LibraryLinkMode) -> LibraryPathPresence {
            UnixLibrarySource.probe(at, links)
        }

        fn read_directory(&self, directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
            UnixLibrarySource.read_directory(directory)
        }

        fn walk(&self, root: &Path, order: LibraryWalkOrder, visitor: &mut dyn LibraryWalkVisitor) {
            UnixLibrarySource.walk(root, order, visitor);
        }
    }

    #[test]
    fn path_source_without_a_mount_boundary_never_claims_unmounted() {
        let dir = tempfile::tempdir().unwrap();
        let real_dev = dev_of(dir.path());
        let gone_path = dir.path().join("gone-album/gone.flac");

        assert_eq!(
            NoMountUnixSource.reachability(&gone_path, Some(real_dev as i64 + 99_999)),
            MissingReason::Unknown
        );
    }

    #[test]
    fn unix_source_opens_library_content_as_a_seekable_reader() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.flac");
        std::fs::write(&path, b"library bytes").unwrap();

        let mut handle = UnixLibrarySource.open_read(&path).unwrap();
        let mut content = String::new();
        handle.read_to_string(&mut content).unwrap();
        assert_eq!(content, "library bytes");

        handle.seek(SeekFrom::Start(8)).unwrap();
        let mut tail = String::new();
        handle.read_to_string(&mut tail).unwrap();
        assert_eq!(tail, "bytes");
    }

    /// A source whose residence token is a DocumentsProvider tree id rather
    /// than an `st_dev`, touching no filesystem at all. It exists to prove the
    /// classification in [`LibrarySource::reachability`] is a comparison of
    /// opaque tokens and carries no POSIX assumption — the property the
    /// Android SAF source will depend on.
    struct DocumentTreeSource {
        provider_tree_id: Option<&'static str>,
    }

    impl LibrarySource for DocumentTreeSource {
        fn residence_token(&self, _at: &Path) -> Option<i64> {
            self.provider_tree_id?.strip_prefix("tree-")?.parse().ok()
        }

        fn mount_point(&self, _at: &Path) -> Option<std::path::PathBuf> {
            None
        }

        fn display_name(&self, _at: &Path) -> Option<String> {
            None
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
                "residence-only test source has no content tree",
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

        /// This double exists to exercise residence classification only, and
        /// the tests below never walk it. An empty walk is the honest answer
        /// for a source with no tree at all — not a stub standing in for one.
        fn walk(
            &self,
            _root: &Path,
            _order: LibraryWalkOrder,
            _visitor: &mut dyn LibraryWalkVisitor,
        ) {
        }
    }

    #[test]
    fn a_non_posix_token_yields_the_same_triad() {
        let at = Path::new("content:/music/album/track.flac");
        let under = |tree| DocumentTreeSource {
            provider_tree_id: tree,
        };

        assert_eq!(
            under(Some("tree-41")).reachability(at, Some(41)),
            MissingReason::Deleted
        );
        assert_eq!(
            under(Some("tree-73")).reachability(at, Some(41)),
            MissingReason::Unmounted
        );
        assert_eq!(
            under(None).reachability(at, Some(41)),
            MissingReason::Unknown
        );
    }
}

#[cfg(test)]
#[path = "source_traversal_tests.rs"]
mod traversal_tests;
