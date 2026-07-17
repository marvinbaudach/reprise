//! Task 1.7: typed import-error classification, episode tracking, and the
//! dismiss-skip fast path — the self-healing-list core a later UI task reads
//! from (grouping rows by kind, showing "seen in N scans", letting a row be
//! dismissed until the file actually changes). Declared directly in
//! `library/mod.rs` (`pub(crate) mod import_errors;`), the same way `mounts`
//! is — NOT via `scanner.rs`'s `#[path = ...] mod vanish;`/`mod mount;`
//! pattern those two siblings use — because [`ImportErrorKind`] is `pub`: a
//! future query layer outside `scanner` needs to name it, so this has to be
//! an ordinary crate module `scanner.rs` merely calls into, not a private
//! sub-module folded inside it.
//!
//! ## Classify at the source, never parse error text
//!
//! `scanner::read_meta`'s only fallible line is `lofty::read_from_path`.
//! Before this task, its `Err` was immediately collapsed to `e.to_string()`
//! — from that point on every import failure was an indistinguishable
//! string, and the only way to group them (as the UI needs to) would have
//! been matching on lofty's `Display` text: a formatting change in any
//! future patch release of a third-party crate would silently reclassify
//! everything into `Unknown`, and no test would go red, because no test
//! knows the foreign string constant. Worse, "permission denied" is not
//! reliably obtainable that way at all — lofty surfaces `EACCES` as
//! `ErrorKind::Io(io::Error)`, whose `Display` text varies by platform/libc,
//! not as a distinct variant. [`classify_lofty`] instead matches on lofty's
//! *typed* [`lofty::error::ErrorKind`], breaking `Io(e)` down further by
//! `e.kind()`, and keeps the original message only as `reason_detail` — a
//! display payload this module never inspects again. [`classify_walkdir`]
//! applies the same principle to directory-traversal failures.

use rusqlite::{OptionalExtension, Transaction};

/// The taxonomy behind `import_errors.reason_kind`. Every variant groups a
/// disjoint set of causes that the (future) UI needs to tell apart —
/// "Unreadable tags" vs "Permission denied" mean different remediation
/// stories for the user (re-tag the file vs `chmod`/check ownership).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportErrorKind {
    /// The file's container/tag data could not be parsed — every lofty
    /// `ErrorKind` variant that represents a tag- or container-level parse
    /// failure lands here. See [`classify_lofty`]'s match arms for the full,
    /// explicitly-enumerated list.
    UnreadableTags,
    /// The OS refused access (`EACCES`) — either lofty's `Io` variant wrapped
    /// an `io::ErrorKind::PermissionDenied`, or a `walkdir` directory-open
    /// failed the same way.
    PermissionDenied,
    /// Lofty could not even guess the file's format (`ErrorKind::
    /// UnknownFormat`) — the file has an audio extension but isn't
    /// recognizable audio at all.
    UnsupportedFormat,
    /// An I/O failure other than permission-denied (disk error, device
    /// vanished mid-read, ...), from either lofty's `Io` variant or a
    /// `walkdir` traversal error.
    Io,
    /// Neither of the above could be established — either the underlying
    /// lofty `ErrorKind` was a variant this classifier has no more specific
    /// bucket for (see [`classify_lofty`]'s wildcard arm, which always logs
    /// a `tracing::warn!` when this happens), or a `walkdir` symlink-loop
    /// error with no underlying `io::Error` to inspect.
    Unknown,
}

impl ImportErrorKind {
    /// The exact string stored in `import_errors.reason_kind` — the inverse
    /// of [`Self::parse`]. A plain `&'static str` (not `Display`): this is a
    /// storage format, not user-facing text — see `MissingReason::as_str`'s
    /// doc comment for the same convention elsewhere in this crate.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UnreadableTags => "unreadable_tags",
            Self::PermissionDenied => "permission_denied",
            Self::UnsupportedFormat => "unsupported_format",
            Self::Io => "io",
            Self::Unknown => "unknown",
        }
    }

    /// Parses an `import_errors.reason_kind` value back into a kind.
    /// Anything other than the four named reasons — including a value this
    /// version of the app has never written, from a future schema or an
    /// edited-by-hand row — falls back to `Unknown` rather than erroring,
    /// same convention as `MissingReason::parse`: a row mapper must never
    /// fail to load a row just because `reason_kind` holds an unrecognized
    /// string.
    pub fn parse(s: &str) -> Self {
        match s {
            "unreadable_tags" => Self::UnreadableTags,
            "permission_denied" => Self::PermissionDenied,
            "unsupported_format" => Self::UnsupportedFormat,
            "io" => Self::Io,
            _ => Self::Unknown,
        }
    }
}

/// Maps a lofty read failure to `(kind, detail)` at the SOURCE — see this
/// module's doc comment for why this must stay a match on
/// [`lofty::error::ErrorKind`], never on `LoftyError`'s `Display` text.
///
/// `lofty::error::ErrorKind` is `#[non_exhaustive]`, so a wildcard arm is
/// mandatory even ignoring this module's own policy — but the wildcard is
/// deliberately paired with a `tracing::warn!` carrying the *original* lofty
/// text: an unobserved catch-all is only a safety net if someone can see
/// what falls into it, otherwise the next lofty release quietly grows a new
/// variant this classifier silently mis-buckets as `Unknown` forever. Every
/// tag/container-parse variant existing in lofty 0.22.4 is enumerated
/// explicitly below (see `~/.cargo/registry/.../lofty-0.22.4/src/error.rs`)
/// rather than left to fall through, precisely so that a *future* lofty
/// addition — not a variant that already existed at the time this was
/// written — is the only thing that can reach the wildcard.
pub(crate) fn classify_lofty(e: &lofty::error::LoftyError) -> (ImportErrorKind, String) {
    use lofty::error::ErrorKind as LK;
    let detail = e.to_string();
    let kind = match e.kind() {
        LK::UnknownFormat => ImportErrorKind::UnsupportedFormat,
        LK::Io(io_err) => match io_err.kind() {
            std::io::ErrorKind::PermissionDenied => ImportErrorKind::PermissionDenied,
            _ => ImportErrorKind::Io,
        },
        // Every tag/container-parse variant lofty 0.22.4 defines, explicitly
        // enumerated (see this function's doc comment for why "explicit"
        // matters here): out-of-bounds/oversized data, a failed decode/
        // encode of the container itself, a malformed picture, an
        // unsupported or fake tag, undecodable text/timestamps, a malformed
        // ID3v2/MP4-atom/OGG-page structure, and non-UTF8 text lofty
        // extracted from a tag.
        LK::TooMuchData
        | LK::SizeMismatch
        | LK::FileDecoding(_)
        | LK::FileEncoding(_)
        | LK::NotAPicture
        | LK::UnsupportedPicture
        | LK::UnsupportedTag
        | LK::FakeTag
        | LK::TextDecode(_)
        | LK::BadTimestamp(_)
        | LK::Id3v2(_)
        | LK::BadAtom(_)
        | LK::AtomMismatch
        | LK::OggPage(_)
        | LK::StringFromUtf8(_)
        | LK::StrFromUtf8(_) => ImportErrorKind::UnreadableTags,
        other => {
            // Safety-net arm: catches `Fmt`/`Alloc`/`Infallible` (none of
            // which a read path should ever actually produce) and any
            // variant a future lofty release adds. Observed, not silent —
            // see this function's doc comment.
            tracing::warn!(detail = %detail, kind = ?other, "unclassified lofty error");
            ImportErrorKind::Unknown
        }
    };
    (kind, detail)
}

/// Maps a `walkdir` directory-traversal failure to a kind, the same
/// classify-at-the-source principle [`classify_lofty`] applies to lofty
/// errors. `err.io_error()` is `None` only for a symlink-loop error (no
/// underlying `io::Error` exists for that case) — see `walkdir::Error`'s own
/// doc comment — which this crate has no more specific bucket for than
/// `Unknown`.
pub(crate) fn classify_walkdir(err: &walkdir::Error) -> ImportErrorKind {
    match err.io_error().map(std::io::Error::kind) {
        Some(std::io::ErrorKind::PermissionDenied) => ImportErrorKind::PermissionDenied,
        Some(_) => ImportErrorKind::Io,
        None => ImportErrorKind::Unknown,
    }
}

/// Episode upsert: records one failed import attempt for `path`. A first
/// failure inserts a fresh row (`seen_count = 1`, `first_seen = last_seen =
/// now`); a repeat failure for a path that already has a row bumps
/// `seen_count` and `last_seen` while leaving `first_seen` untouched — this
/// is what makes repeated scans of the same broken file converge on ONE row
/// instead of the pre-Task-1.7 DELETE-then-INSERT pair's fresh-timestamp
/// churn (the `path` primary key, schema v10, is what makes the upsert
/// possible at all). `reason_kind`/`reason_detail` are always refreshed to
/// the latest attempt's classification: a file can fail for a different
/// reason on a later scan (e.g. permission fixed, but the tag data
/// underneath turns out corrupt too), and the row should reflect the most
/// recent diagnosis, not the first one.
pub(crate) fn record_error(
    tx: &Transaction,
    path: &str,
    kind: ImportErrorKind,
    detail: &str,
    now: i64,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO import_errors (path, reason_kind, reason_detail, first_seen, last_seen, seen_count) \
         VALUES (?1, ?2, ?3, ?4, ?4, 1) \
         ON CONFLICT(path) DO UPDATE SET reason_kind=?2, reason_detail=?3, last_seen=?4, seen_count=seen_count+1",
        rusqlite::params![path, kind.as_str(), detail, now],
    )?;
    Ok(())
}

/// Clears any `import_errors` row for `path` (a file that imported
/// successfully must not stay flagged). Returns whether a row actually
/// existed to delete — `true` is this crate's "healed" signal: a future
/// scan-summary consumer can count it without a second query, though this
/// task doesn't itself add such a counter.
pub(crate) fn clear_error(tx: &Transaction, path: &str) -> rusqlite::Result<bool> {
    let changed = tx.execute("DELETE FROM import_errors WHERE path = ?1", [path])?;
    Ok(changed > 0)
}

/// The dismiss-skip fast path: called BEFORE `read_meta`, with only a `stat`
/// (`mtime`/`size`) already in hand — never a tag parse — so a dismissed
/// file costs the scan almost nothing. Returns `true` when the caller should
/// skip re-parsing this file entirely (dismissed AND unchanged since the
/// dismissal); `false` in every other case, including "no row" and "a row
/// exists but was never dismissed" (`dismissed_mtime`/`dismissed_size` both
/// `NULL`).
///
/// When a row WAS dismissed but `mtime`/`size` no longer match what was
/// recorded at dismissal time, the file genuinely changed since the user
/// last saw it — this function reactivates the episode itself, in the same
/// call: it clears both `dismissed_*` columns and resets `first_seen = now`,
/// `seen_count = 0`. A fresh `first_seen` matters beyond bookkeeping: the
/// sidebar badge (a later task) counts rows where `first_seen > last_viewed`
/// to know what's new, and the old, dismissed episode's story is over — this
/// changed file deserves to look new again, not like the same stale
/// complaint the user already dismissed. `seen_count` resets to `0` rather
/// than `1` because this function never counts as a failed *attempt* by
/// itself (it's a stat, not a parse) — the caller's very next `record_error`
/// call, once the re-parse itself fails, is what takes it to `1`.
pub(crate) fn check_dismissed(
    tx: &Transaction,
    path: &str,
    mtime: i64,
    size: i64,
    now: i64,
) -> rusqlite::Result<bool> {
    let dismissed: Option<(Option<i64>, Option<i64>)> = tx
        .query_row(
            "SELECT dismissed_mtime, dismissed_size FROM import_errors WHERE path = ?1",
            [path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((Some(dismissed_mtime), Some(dismissed_size))) = dismissed else {
        // No row at all, or a row that was never dismissed
        // (`dismissed_mtime`/`dismissed_size` both `NULL`) — proceed to the
        // normal read_meta path.
        return Ok(false);
    };
    if dismissed_mtime == mtime && dismissed_size == size {
        return Ok(true);
    }
    tx.execute(
        "UPDATE import_errors SET dismissed_mtime = NULL, dismissed_size = NULL, \
         first_seen = ?2, seen_count = 0 WHERE path = ?1",
        rusqlite::params![path, now],
    )?;
    Ok(false)
}

#[cfg(test)]
#[path = "import_errors_tests.rs"]
mod tests;
