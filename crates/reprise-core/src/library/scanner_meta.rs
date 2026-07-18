//! Tag+properties reading — `TrackMeta`, the pass-1 ([`read_meta`]) and
//! pass-2 ([`read_meta_relaxed`]) lofty calls, and the orchestration between
//! them ([`read_meta_with_fallback`]). Split into its own file purely to
//! keep `scanner.rs` itself under the project's 800-line rule, same
//! rationale as `scanner_vanish.rs`'s and `scanner_mount.rs`'s own module doc
//! comments — `scanner.rs` declares this via `#[path = "scanner_meta.rs"]
//! mod track_meta;`, so this is still the crate-private `crate::library::
//! scanner::track_meta` module. (Named `track_meta`, not `meta`, purely to
//! avoid shadowing the `meta` local variable `scan_folder_inner` already
//! binds its `TrackMeta` value to.)
//!
//! ## Task 1.8: why a second, relaxed, tag-free pass — not a playability test
//!
//! The original design for "let a broken-tag file in anyway" was "let the
//! player decide: decodable → import". That's unaffordable here:
//! `reprise-core` has no decoder at all — the only one is GStreamer, which
//! lives in the platform crate behind a `WaveformBackend` contract, and a
//! hard purity gate (`cargo tree -p reprise-core | grep gstreamer` must
//! print nothing) forbids pulling it into this crate. A decode test would
//! mean a new platform trait plus starting a playback pipeline per broken
//! file the scan worker touches — every scan, not just the error path.
//!
//! Instead: pass 1 ([`read_meta`], lofty's default `ParseOptions` —
//! `BestAttempt`, tags included) is tried first, exactly as before this
//! task. Only when THAT fails does pass 2 ([`read_meta_relaxed`],
//! `read_tags(false)` + `ParsingMode::Relaxed`) get a chance, via
//! [`read_meta_with_fallback`]. If pass 2 succeeds, the *container* parses —
//! weaker than "decodable" (lofty can find valid framing without a decoder
//! ever proving the frames are honest sample data), but honest, cheap, pure
//! (no new crate, no platform trait), and deterministic. Only a file whose
//! tags are already broken ever pays for the second pass.
//!
//! ## Never insert a track without a real duration
//!
//! The naive version of "import anyway" inserts the file with `duration_ms
//! = 0`, because when pass 1 fails there is no `properties()` either. A zero
//! duration then poisons everything downstream that reads it: smart
//! playlists with duration rules, total-playtime stats, the queue's
//! remaining-time display, duration-sorted views, and especially the
//! fingerprint half of move detection (`scanner.rs`'s `find_move_candidate`
//! step 2), which matches within `MOVE_MATCH_TOLERANCE_MS` — `0` isn't
//! "unknown" there, it's a value that actively participates in a match, and
//! a row showing "0:00" looks like a bug because it is one. [`read_meta_
//! relaxed`] exists precisely to get the REAL duration and bitrate from the
//! container's properties while skipping the broken tags: neither this
//! function nor [`read_meta_with_fallback`] can ever produce a [`TrackMeta`]
//! whose duration didn't come from an actual `properties()` read.

use std::path::Path;

use super::ScanError;
use crate::library::import_errors;
use crate::models::ImportErrorKind;

/// Tag- and properties-derived fields for one audio file. Every field is
/// left at its `Default` (empty string / `None`) when the corresponding
/// source data wasn't available — [`read_meta`] because a tag simply had no
/// value for it, [`read_meta_relaxed`] because it never reads tags at all.
///
/// Task 1.9: the struct itself (not its fields, which stay `pub(super)` —
/// this crate's other scanner-internal helpers still only ever read/write
/// them from within `scanner`'s own subtree) is `pub(crate)`, not `pub
/// (super)`, purely so it can appear in `scanner_move::apply_file_identity`'s
/// signature without tripping the `private_interfaces` lint. Its fields and
/// `read_meta` are crate-visible because the sibling `library::relink`
/// module feeds the same metadata into `apply_file_identity`; keeping that
/// shared path avoids a second tag model or row-update implementation.
#[derive(Debug, Default)]
pub(crate) struct TrackMeta {
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) album: String,
    pub(crate) album_artist: String,
    pub(crate) artist_mbid: Option<String>,
    pub(crate) year: Option<i32>,
    pub(crate) track_no: Option<i32>,
    pub(crate) genre: String,
    pub(crate) duration_ms: i64,
    pub(crate) bitrate_kbps: Option<i32>,
}

// Test-only: proves a dismissed-and-unchanged file's tags never get parsed.
// `thread_local!`, not a global counter — libtest gives each test its own
// OS thread, so this stays isolated under parallel `--test-threads`.
#[cfg(test)]
thread_local! {
    pub(super) static READ_META_CALLS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Pass 1: the ordinary tag+properties read, lofty's own default
/// `ParseOptions` (`BestAttempt`, tags included). Unchanged from before Task
/// 1.8 — the only thing that changed is that a failure here no longer
/// necessarily ends the import; see [`read_meta_with_fallback`].
pub(crate) fn read_meta(path: &Path) -> Result<TrackMeta, ScanError> {
    use lofty::prelude::*;
    #[cfg(test)]
    READ_META_CALLS.with(|calls| calls.set(calls.get() + 1));
    let tagged = lofty::read_from_path(path).map_err(|e| {
        let (kind, detail) = import_errors::classify_lofty(&e);
        ScanError::Import { kind, detail }
    })?;
    let props = tagged.properties();
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    let get = |f: &dyn Fn(&lofty::tag::Tag) -> Option<String>| tag.and_then(f).unwrap_or_default();
    Ok(TrackMeta {
        title: get(&|t| t.title().map(|s| s.to_string())),
        artist: get(&|t| t.artist().map(|s| s.to_string())),
        album: get(&|t| t.album().map(|s| s.to_string())),
        album_artist: get(&|t| {
            t.get_string(lofty::tag::ItemKey::AlbumArtist)
                .map(std::string::ToString::to_string)
        }),
        artist_mbid: tag.and_then(|tag| {
            tag.get_string(lofty::tag::ItemKey::MusicBrainzArtistId)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        }),
        year: tag
            .and_then(Accessor::date)
            .or_else(|| tagged.tags().iter().find_map(Accessor::date))
            .map(|date| i32::from(date.year)),
        track_no: tag.and_then(Accessor::track).map(|n| n as i32),
        genre: get(&|t| t.genre().map(|s| s.to_string())),
        duration_ms: props.duration().as_millis() as i64,
        bitrate_kbps: props.audio_bitrate().map(|b| b as i32),
    })
}

/// Pass 2 (Task 1.8): a tag-free, relaxed re-read of a file whose pass-1 tag
/// parse already failed — see this module's doc comment for the full
/// rationale. `read_tags(false)` means lofty never even attempts the broken
/// tag data: for most formats it becomes a structurally-skipped chunk/frame
/// rather than a best-effort parse of it (see e.g. lofty's WAV reader, whose
/// main chunk loop falls through to an unconditional `chunks.skip` for an
/// `"ID3 "` chunk the instant `read_tags` is off, never invoking the ID3v2
/// header parser that pass 1's failure came from). `ParsingMode::Relaxed`
/// additionally tells lofty to discard whatever else it can't make sense of
/// in the container rather than erroring eagerly (`ParsingMode::Strict`) or
/// merely filling holes (`BestAttempt`, pass 1's mode) — the most tolerant
/// of the three, appropriate here because pass 2 only runs once pass 1 has
/// already proven this file is not spec-compliant.
///
/// Returns a [`TrackMeta`] with every tag-derived field at its `Default`
/// (this function never reads a tag, so it has nothing to put there) except
/// `duration_ms`/`bitrate_kbps` — read for real from the container's
/// properties, never zeroed — and `album`, set to the file's PARENT
/// DIRECTORY name: with no album tag to fall back to, the enclosing folder
/// is this codebase's next-best signal for "which release this file belongs
/// to" (the same assumption a rip-by-folder library layout already makes
/// throughout this app). `title` is deliberately left empty here rather
/// than set to the file stem — `scan_folder_inner` already falls back to
/// the file stem for ANY empty title, tagged or not, so setting it here
/// would just be the same computation done twice.
pub(super) fn read_meta_relaxed(path: &Path) -> Result<TrackMeta, ScanError> {
    use lofty::prelude::*;
    let opts = lofty::config::ParseOptions::new()
        .read_tags(false)
        .parsing_mode(lofty::config::ParsingMode::Relaxed);
    let tagged = lofty::probe::Probe::open(path)
        .and_then(|probe| probe.options(opts).read())
        .map_err(|e| {
            let (kind, detail) = import_errors::classify_lofty(&e);
            ScanError::Import { kind, detail }
        })?;
    let props = tagged.properties();
    let album = path
        .parent()
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .to_string();
    Ok(TrackMeta {
        album,
        duration_ms: props.duration().as_millis() as i64,
        bitrate_kbps: props.audio_bitrate().map(|b| b as i32),
        ..TrackMeta::default()
    })
}

/// What [`read_meta_with_fallback`] returns on success: pass 1's ordinary
/// tagged read, or pass 2's rescue of an intact container behind broken
/// tags.
pub(super) enum MetaOutcome {
    /// Pass 1 succeeded outright — an ordinary tagged import.
    Tagged(TrackMeta),
    /// Pass 1 failed (`kind`/`detail` are ITS classification) but pass 2
    /// recovered real properties from an intact container. `kind`/`detail`
    /// are carried along — not discarded — so the caller can keep the
    /// `import_errors` row alive as a HINT ("imported without metadata")
    /// once the track itself is inserted: see `scanner.rs`'s `## Hint
    /// coexistence` doc section on `scan_folder_inner`. The hint must
    /// explain why the TAGS are unreadable, which is what pass 1 found —
    /// pass 2 succeeding means there is nothing to classify from IT.
    Untagged {
        meta: TrackMeta,
        kind: ImportErrorKind,
        detail: String,
    },
}

/// Orchestrates the two-pass read: tries [`read_meta`] first, and only on
/// failure tries [`read_meta_relaxed`]. `Err` only when BOTH passes failed —
/// carrying pass 2's classification, since "the container itself can't even
/// be opened tag-free" is a stronger, more actionable diagnosis at that
/// point than repeating pass 1's tag-parse failure would be.
pub(super) fn read_meta_with_fallback(path: &Path) -> Result<MetaOutcome, ScanError> {
    match read_meta(path) {
        Ok(meta) => Ok(MetaOutcome::Tagged(meta)),
        Err(ScanError::Import { kind, detail }) => match read_meta_relaxed(path) {
            Ok(meta) => Ok(MetaOutcome::Untagged { meta, kind, detail }),
            Err(e2) => Err(e2),
        },
        // `read_meta` only ever produces `Import`; propagating any other
        // variant is safer than an `unreachable!()` panic if that changes.
        Err(other) => Err(other),
    }
}
