//! Platform-neutral library-source residence contract.
//!
//! Core owns the comparison that distinguishes a deleted track from a
//! temporarily unreachable source. Concrete sources own only the stable token
//! that makes that comparison meaningful on their platform.

use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::models::MissingReason;

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
/// [`MissingReason::Unknown`] and never fabricates an identity.
///
/// **That safety belongs to `residence_token` alone, not to this trait as a
/// whole.** [`Self::probe`] answers a different question, and its `None` is
/// read as *confirmed absence* — two call sites turn it straight into a
/// missing-verdict write. Every method here says for itself what its `None`
/// means; do not generalise one method's degradation to another.
///
/// **No question a source alone can answer has a default implementation.** A
/// source that cannot yet answer one must fail to compile, not answer `None` —
/// for `probe` that answer would report the entire library as gone.
/// [`Self::reachability`] is the one exception and a deliberate one: it decides
/// nothing by itself, it only compares what [`Self::residence_token`] returned,
/// so a source that answers the primitive gets the verdict for free and cannot
/// get it wrong.
pub trait LibrarySource: Send + Sync {
    /// Returns the stable residence token of the nearest reachable location at
    /// `at`, or `None` when this source cannot provide one.
    fn residence_token(&self, at: &Path) -> Option<i64>;

    /// Opens `at` for reading without exposing the source's concrete storage
    /// handle. Failure is explicit: a source that cannot provide readable,
    /// seekable content must not compile with this contract unanswered.
    ///
    /// **An `Err` here is a failure to read, never a statement that the item is
    /// gone.** That distinction is the whole reason this returns `io::Result`
    /// where [`Self::probe`] returns `Option`: a revoked permission grant, a
    /// dropped provider connection or a transient I/O error must not become the
    /// missing-verdict that a `None` from `probe` licenses. No caller may
    /// substitute one for the other.
    fn open_read(&self, at: &Path) -> io::Result<LibraryReadHandle>;

    /// Returns the facts this source can establish about `at`.
    ///
    /// **`None` means the item is not there.** It is not "I could not find
    /// out". `library::scanner_vanish::mark_vanished_with` and
    /// `queries::maintenance::mark_track_missing_if_current_with` turn a `None`
    /// straight into a `missing_since`/`missing_reason` write, so a source that
    /// answers `None` for a transient failure marks live tracks as gone. When a
    /// source cannot reach its backing store, it must not guess absence — that
    /// case wants its own signal, and does not have one yet (see the spike's
    /// note on the SAF adapter).
    ///
    /// A path that *is* there but whose individual facts are unavailable
    /// answers `Some` with those fields `None`; callers then apply their own
    /// conservative fallback rather than receiving a fabricated zero or
    /// identity.
    ///
    /// `links` is explicit because most Class-A presence checks historically
    /// used `Path::metadata` and followed the final symlink, while abandoned
    /// writeback cleanup used `DirEntry::metadata` and must inspect the link
    /// itself. Keeping that distinction in the contract preserves the safety
    /// boundary rather than silently changing it during abstraction.
    fn probe(&self, at: &Path, links: LibraryLinkMode) -> Option<LibraryPathMetadata>;

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
    /// - `stored` is `None` → there is nothing to compare against. `Unknown`
    ///   (see `MissingReason`'s own doc comment for why this must stay
    ///   `Unknown` rather than defaulting to either concrete reason: nothing
    ///   downstream may treat such a row as safely auto-removable without
    ///   re-verifying the item first).
    /// - The item's location reports the same token it was last seen under →
    ///   the source it lived on is present and reachable, and the item simply
    ///   isn't there anymore. `Deleted`.
    /// - It reports a *different* token → we are looking at a different source
    ///   than the one recorded for this item, which means the original one is
    ///   currently absent. `Unmounted`.
    /// - This source can supply no token at all → no evidence either way.
    ///   `Unknown`. Two unknowns are never evidence of each other.
    ///
    /// The token is `i64` because SQLite has no other integer type; it matches
    /// `Track::device` and the scanner metadata projection's storage cast.
    /// [`UnixLibrarySource`] round-trips exactly the `st_dev` bit pattern that
    /// cast away from `u64` on the way in, so this stays the same comparison
    /// Linux made before the trait existed.
    fn reachability(&self, at: &Path, stored: Option<i64>) -> MissingReason {
        let Some(stored) = stored else {
            return MissingReason::Unknown;
        };
        match self.residence_token(at) {
            Some(current) if current == stored => MissingReason::Deleted,
            Some(_) => MissingReason::Unmounted,
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

impl LibrarySource for UnixLibrarySource {
    fn residence_token(&self, at: &Path) -> Option<i64> {
        nearest_existing_ancestor_dev(at).map(|device| device as i64)
    }

    fn open_read(&self, at: &Path) -> io::Result<LibraryReadHandle> {
        std::fs::File::open(at).map(LibraryReadHandle::new)
    }

    fn probe(&self, at: &Path, links: LibraryLinkMode) -> Option<LibraryPathMetadata> {
        let metadata = match links {
            LibraryLinkMode::Follow => std::fs::metadata(at),
            LibraryLinkMode::NoFollow => std::fs::symlink_metadata(at),
        }
        .ok()?;
        Some(LibraryPathMetadata {
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

/// Returns `(ancestor_path, st_dev)` for the nearest ancestor of `path`
/// (starting at `path` itself) that can be `lstat`'d successfully.
///
/// Uses `symlink_metadata` (lstat), deliberately never `metadata` (stat):
/// if some ancestor component in the path is itself a symlink, `lstat`
/// reports the symlink's own device rather than following it to whatever
/// it points at. Following the symlink here would let an ancestor that
/// merely *points into* a different mount fabricate a foreign device id —
/// and thus a bogus `Unmounted` verdict — even though the symlink itself
/// sits on the original, still-mounted filesystem.
///
/// `Path::ancestors()` walks `path`, then each successive parent, ending at
/// `/` for an absolute path — so the walk is capped at the root without any
/// extra bookkeeping. Returns `None` only if even `/` can't be `lstat`'d,
/// which should not happen on a working Linux system.
///
/// This "capped at `/`" guarantee holds only for an *absolute* `path` — for
/// a relative path, `ancestors()` instead bottoms out at `""` (`Path::new("")`
/// does not exist, so the walk would return `None` rather than ever reaching
/// `/`). Every caller in this codebase passes an absolute path: library
/// roots come from GTK's folder chooser (always absolute) and
/// `tracks.path`/scan roots are [`LibrarySource::walk`] inputs derived from
/// that same root, so this isn't separately enforced here.
pub(crate) fn nearest_existing_ancestor(path: &Path) -> Option<(PathBuf, u64)> {
    // The walk-to-`/` guarantee above holds only for an absolute path, so the
    // requirement is asserted here, where it is actually needed, rather than at
    // the scanner — which has no such need and used to carry it anyway.
    //
    // A relative path is not a panic in release: `ancestors()` bottoms out at
    // `""`, which no `lstat` succeeds on, so this answers `None`, which
    // `reachability` turns into `MissingReason::Unknown`. That is the honest
    // outcome, and it is why a source with no filesystem ancestry — a SAF tree,
    // whose root is a content URI and therefore not absolute — degrades safely
    // here instead of lying.
    debug_assert!(
        path.is_absolute(),
        "the Unix source's ancestor walk assumes an absolute path; got {}",
        path.display()
    );
    path.ancestors().find_map(|ancestor| {
        let metadata = std::fs::symlink_metadata(ancestor).ok()?;
        Some((ancestor.to_path_buf(), device_id(&metadata)?))
    })
}

/// Returns Unix `st_dev`, or `None` when the target has no stable device id.
pub(crate) fn device_id(metadata: &std::fs::Metadata) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(metadata.dev())
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

fn file_identity(metadata: &std::fs::Metadata) -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some((metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

/// `st_dev` of the nearest ancestor of `path` that currently exists,
/// starting the search at `path` itself. `lstat` (`symlink_metadata`) only —
/// see [`nearest_existing_ancestor`]'s doc comment for why this must never
/// follow symlinks. `None` only if even `/` can't be `lstat`'d.
pub(crate) fn nearest_existing_ancestor_dev(path: &Path) -> Option<u64> {
    nearest_existing_ancestor(path).map(|(_, device)| device)
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Seek, SeekFrom};
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    use super::{
        LibraryDirectoryEntry, LibraryLinkMode, LibraryPathMetadata, LibraryReadHandle,
        LibrarySource, LibraryWalkControl, LibraryWalkItem, LibraryWalkOrder, LibraryWalkVisitor,
        UnixLibrarySource,
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

    /// A stored device that doesn't match anything on this filesystem
    /// fabricates exactly the situation an unmounted drive produces: the
    /// nearest existing ancestor belongs to a different device than the one
    /// recorded. `real_dev + 99_999` is never going to collide with a real
    /// `st_dev` in a test environment, so this is deterministic without
    /// mounting or unmounting anything.
    #[test]
    fn unix_source_reports_unmounted_when_the_device_differs() {
        let dir = tempfile::tempdir().unwrap();
        let real_dev = dev_of(dir.path());
        let gone_path = dir.path().join("gone.flac");

        assert_eq!(
            UnixLibrarySource.reachability(&gone_path, Some(real_dev as i64 + 99_999)),
            MissingReason::Unmounted
        );
    }

    /// No recorded device (schema-v1 row, or a `stat` that failed on last
    /// scan) means there is no basis for a verdict at all — `Unknown`, never
    /// a guessed concrete reason.
    #[test]
    fn unix_source_reports_unknown_when_no_device_was_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let gone_path = dir.path().join("gone.flac");

        assert_eq!(
            UnixLibrarySource.reachability(&gone_path, None),
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

        fn open_read(&self, _at: &Path) -> std::io::Result<LibraryReadHandle> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "residence-only test source has no content tree",
            ))
        }

        /// Unused by this double's tests. Made explicit rather than inherited:
        /// the trait has no defaults precisely so a source cannot answer
        /// "absent" for a question it was never taught to answer.
        fn probe(&self, _at: &Path, _links: LibraryLinkMode) -> Option<LibraryPathMetadata> {
            None
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

        fn open_read(&self, _at: &Path) -> std::io::Result<LibraryReadHandle> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "traversal-only test source carries names, not content",
            ))
        }

        /// Unused by this double's tests. Made explicit rather than inherited:
        /// the trait has no defaults precisely so a source cannot answer
        /// "absent" for a question it was never taught to answer.
        fn probe(&self, _at: &Path, _links: LibraryLinkMode) -> Option<LibraryPathMetadata> {
            None
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
}
