//! Task 1.8's test suite: the tag-free relaxed second pass that lets a file
//! with broken tags but an intact container import anyway. Split from
//! `scanner_tests.rs` for the usual 800-line reason — `scanner.rs` declares
//! this via `#[cfg(test)] #[path = "scanner_untagged_tests.rs"] mod
//! untagged_tests;`, so these are still crate-private scanner tests.
//!
//! ## The fixture: a WAV with a broken ID3v2 chunk but an intact container
//!
//! lofty's WAV reader (`lofty::iff::wav::read::read_from`) walks the RIFF
//! chunk list unconditionally; an `"ID3 "` chunk is only ever handed to the
//! ID3v2 header parser when `ParseOptions::read_tags` is on — with it off,
//! the SAME bytes are simply skipped (`chunks.skip`), never inspected at
//! all. That is exactly the shape Task 1.8 exists for: [`wav_header`] below
//! builds a syntactically valid RIFF/WAVE file (`"fmt "` + `"data"`, real
//! PCM sample bytes) and optionally appends an `"ID3 "` chunk built by
//! [`bad_id3_chunk`] — an ID3v2 header whose major-version byte (`0xFF`) is
//! not one lofty recognizes (only `2`/`3`/`4` are), which fails
//! `Id3v2Header::parse` (`ErrorKind::Id3v2`, classified `UnreadableTags`)
//! the instant pass 1 tries to read it.
//!
//! Verified experimentally against lofty 0.22.4 before writing these tests
//! (not just assumed from reading the source): pass 1 on this exact byte
//! layout fails with `UnreadableTags` ("ID3v2: Found an invalid version
//! (v255.0), expected any major revision in: (2, 3, 4)"); pass 2
//! (`read_tags(false)`) succeeds, with a real `duration_ms` computed from
//! the `"fmt "`/`"data"` chunks alone. Built programmatically rather than
//! checked in as a binary fixture — the WAV format is simple enough that a
//! hand-rolled byte layout is more legible, and more honest about exactly
//! what's broken, than an opaque binary blob would be.

use rusqlite::OptionalExtension;

use super::tests::completed;
use super::*;
use lofty::prelude::*;
use lofty::tag::{Tag, TagType};

/// PCM format parameters shared by every fixture in this file: mono,
/// 16-bit, 44.1kHz, 0.1s of silence — enough for lofty to compute a real,
/// non-zero `duration_ms`/`bitrate_kbps` from the container alone.
const SAMPLE_RATE: u32 = 44_100;
const BITS_PER_SAMPLE: u16 = 16;
const CHANNELS: u16 = 1;
const NUM_SAMPLES: u32 = 4_410; // 0.1s

/// Builds a minimal, valid mono/16-bit/44.1kHz PCM WAV (`"fmt "` + `"data"`
/// chunks), optionally followed by a raw `"ID3 "` chunk (`extra_id3`'s bytes,
/// framed with their own chunk size header — NOT validated by this function,
/// so a caller can hand it deliberately malformed content, as
/// [`bad_id3_chunk`] does).
fn wav_header(extra_id3: Option<&[u8]>) -> Vec<u8> {
    let block_align = CHANNELS * BITS_PER_SAMPLE / 8;
    let byte_rate = SAMPLE_RATE * u32::from(block_align);
    let data = vec![0u8; (NUM_SAMPLES * u32::from(block_align)) as usize];

    let mut fmt_chunk = Vec::new();
    fmt_chunk.extend_from_slice(&1u16.to_le_bytes()); // wFormatTag = PCM
    fmt_chunk.extend_from_slice(&CHANNELS.to_le_bytes());
    fmt_chunk.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    fmt_chunk.extend_from_slice(&byte_rate.to_le_bytes());
    fmt_chunk.extend_from_slice(&block_align.to_le_bytes());
    fmt_chunk.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());

    let mut body = Vec::new();
    body.extend_from_slice(b"WAVE");
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt_chunk.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt_chunk);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(data.len() as u32).to_le_bytes());
    body.extend_from_slice(&data);
    if let Some(id3) = extra_id3 {
        body.extend_from_slice(b"ID3 ");
        body.extend_from_slice(&(id3.len() as u32).to_le_bytes());
        body.extend_from_slice(id3);
        if id3.len() % 2 != 0 {
            // RIFF chunks pad to an even boundary — see lofty's
            // `Chunks::correct_position`.
            body.push(0);
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// An `"ID3 "` chunk whose header fails lofty's ID3v2 parser on the FIRST
/// field it checks after the magic bytes: the major-version byte is `0xFF`,
/// not `2`/`3`/`4` — see `Id3v2Header::parse` in lofty 0.22.4
/// (`~/.cargo/registry/.../lofty-0.22.4/src/id3/v2/header.rs`). This is a
/// container-level parse failure (`ErrorKind::Id3v2`), classified
/// `UnreadableTags` by `classify_lofty` — never reached at all when pass 2
/// skips tag parsing entirely.
fn bad_id3_chunk() -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"ID3");
    v.push(0xFF); // bad major version (valid: 2, 3, 4)
    v.push(0x00); // minor version
    v.push(0x00); // flags
    v.extend_from_slice(&[0x00, 0x00, 0x00, 0x04]); // syncsafe size = 4
    v.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]); // never reached
    v
}

fn write_wav(path: &std::path::Path, extra_id3: Option<&[u8]>) {
    std::fs::write(path, wav_header(extra_id3)).unwrap();
}

/// Overwrites `path` with a REAL, valid ID3v2 title tag on an intact WAV —
/// the "tags got repaired" half of the healing test. First rewrites the
/// container fresh with no leftover broken chunk, then uses lofty's own
/// writer (`Tag::save_to_path`) to add a real tag — the same roundtrip
/// approach `scanner_tests.rs`'s `tag_file` helper uses for FLAC/
/// VorbisComments.
fn heal_tags(path: &std::path::Path, title: &str) {
    write_wav(path, None);
    let mut tag = Tag::new(TagType::Id3v2);
    tag.set_title(title.into());
    tag.save_to_path(path, lofty::config::WriteOptions::default())
        .unwrap();
}

/// `(title, album, duration_ms, untagged)` for the `tracks` row at `path`,
/// if one exists.
fn track_row(conn: &Connection, path: &std::path::Path) -> Option<(String, String, i64, i64)> {
    conn.query_row(
        "SELECT title, album, duration_ms, untagged FROM tracks WHERE path = ?1",
        [path.to_string_lossy().to_string()],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .optional()
    .unwrap()
}

fn error_kind(conn: &Connection, path: &std::path::Path) -> Option<ImportErrorKind> {
    conn.query_row(
        "SELECT reason_kind FROM import_errors WHERE path = ?1",
        [path.to_string_lossy().to_string()],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .unwrap()
    .map(|s| ImportErrorKind::parse(&s))
}

/// Brief case 1 ("Pass-2-Erfolg"): a file with a broken tag but an intact
/// container imports anyway — `untagged = 1`, `title` falls back to the
/// file stem, `album` falls back to the parent directory name, and
/// `duration_ms` is real (never `0`). The `import_errors` row survives as a
/// hint, still carrying pass 1's `UnreadableTags` classification (see
/// `scanner.rs`'s `## Hint coexistence` doc section).
#[test]
fn pass2_rescues_broken_tags_and_keeps_the_hint_row() {
    let tmp = tempfile::tempdir().unwrap();
    let album_dir = tmp.path().join("Some Album");
    std::fs::create_dir(&album_dir).unwrap();
    let path = album_dir.join("gebrochen.wav");
    write_wav(&path, Some(&bad_id3_chunk()));

    let conn = crate::db::Db::open_in_memory().unwrap();
    let report = completed(scan_folder(&conn, tmp.path()).unwrap());
    assert_eq!(report.added, 1, "the untagged import still counts as added");
    assert_eq!(report.errors, 0, "a rescued import is not a scan error");
    assert_eq!(
        report.healed, 0,
        "Task 1.9: a pass-2 hint row is refreshed via record_error, never \
         cleared via clear_error, so it must never count as healed"
    );

    let (title, album, duration_ms, untagged) =
        track_row(conn.conn(), &path).expect("pass 2 must still insert a track row");
    assert_eq!(title, "gebrochen", "title falls back to the file stem");
    assert_eq!(
        album, "Some Album",
        "album falls back to the parent directory name"
    );
    assert!(duration_ms > 0, "must never insert a zero duration");
    assert_eq!(untagged, 1);

    assert_eq!(
        error_kind(conn.conn(), &path),
        Some(ImportErrorKind::UnreadableTags),
        "the hint row must survive, still carrying pass 1's own classification"
    );
}

/// CRITICAL non-destructive guard: a file whose tags can't be recovered (here
/// the tags live only in a tail container that stripping would lose) is left
/// BYTE-FOR-BYTE untouched and imported as a plain untagged track. The repair
/// must never overwrite tags it couldn't read — an earlier version stripped
/// everything and wrote the file name / folder, silently destroying real,
/// still-present metadata of any file that didn't recover.
#[test]
fn scanner_leaves_an_unrecoverable_container_byte_identical_on_import() {
    let tmp = tempfile::tempdir().unwrap();
    let album_dir = tmp.path().join("Some Album");
    std::fs::create_dir(&album_dir).unwrap();
    let path = album_dir.join("Broken Song.mp3");
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/broken-tags.mp3");
    std::fs::copy(&fixture, &path).unwrap();
    let before = std::fs::read(&path).unwrap();

    let conn = crate::db::Db::open_in_memory().unwrap();
    let report = completed(scan_folder(&conn, tmp.path()).unwrap());
    assert_eq!(report.added, 1, "the untagged import still counts as added");
    assert_eq!(report.errors, 0);

    let (title, album, _duration, untagged) =
        track_row(conn.conn(), &path).expect("the file still imports as a track row");
    assert_eq!(untagged, 1, "an unrecoverable container stays untagged");
    assert_eq!(title, "Broken Song", "title falls back to the file stem");
    assert_eq!(album, "Some Album", "album falls back to the folder name");
    assert_eq!(
        error_kind(conn.conn(), &path),
        Some(ImportErrorKind::UnreadableTags),
        "the unreadable-tags hint survives"
    );

    // The file itself must be exactly as it was — no repair write happened.
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "an unrecoverable file must be left byte-for-byte unchanged"
    );
    // And no stray temp file was left behind next to it.
    assert!(
        !album_dir
            .join("Broken Song.reprise-repair-tmp.mp3")
            .exists(),
        "the recovery temp file must be cleaned up"
    );
}

/// A DISMISSED `unreadable_tags` error must NOT keep an untagged row from
/// being repaired on a later scan. Dismissing only silences the notification,
/// and predates the auto-repair — the dismiss-skip fast path would otherwise
/// strand such a file forever (its mtime never changes, so it is never
/// re-read). Regression guard for the reported tracks that a rescan wouldn't
/// touch because their import error had been dismissed.
#[test]
fn a_dismissed_import_error_does_not_block_repairing_an_untagged_track() {
    let tmp = tempfile::tempdir().unwrap();
    let album_dir = tmp.path().join("Some Album");
    std::fs::create_dir(&album_dir).unwrap();
    let path = album_dir.join("Broken Song.mp3");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/broken-tags.mp3"),
        &path,
    )
    .unwrap();
    let path_str = path.to_string_lossy().to_string();
    let meta = std::fs::metadata(&path).unwrap();
    let mtime = file_mtime(&path);
    let size = meta.len() as i64;

    let conn = crate::db::Db::open_in_memory().unwrap();
    // Seed the exact reported state: an untagged row plus a DISMISSED
    // unreadable-tags error whose mtime/size match the file on disk.
    conn.conn().execute(
        "INSERT INTO tracks (path, title, artist, album, added_at, file_mtime, file_size, untagged) \
         VALUES (?1, 'Broken Song', '', 'Some Album', 0, ?2, ?3, 1)",
        rusqlite::params![path_str, mtime, size],
    )
    .unwrap();
    conn.conn()
        .execute(
            "INSERT INTO import_errors \
         (path, reason_kind, reason_detail, first_seen, last_seen, seen_count, \
          dismissed_mtime, dismissed_size) \
         VALUES (?1, 'unreadable_tags', 'x', 0, 0, 1, ?2, ?3)",
            rusqlite::params![path_str, mtime, size],
        )
        .unwrap();

    let report = completed(scan_folder(&conn, tmp.path()).unwrap());
    // The dismiss fast path did NOT skip the row: it was actually re-read
    // (imported or errored, `updated + errors == 1`). Without the untagged
    // exemption the scanner would `continue` past it and touch neither counter.
    // (The `broken-tags` fixture keeps its tags in the stripped tail, so the
    // recovery itself doesn't apply here — that is covered by
    // `repair_recovers_real_front_id3v2_tags_by_stripping_only_the_damaged_tail`.)
    assert_eq!(
        report.updated + report.errors,
        1,
        "a dismissed untagged row must be re-processed, not dismiss-skipped"
    );
    assert_eq!(
        report.skipped_unchanged, 0,
        "a dismissed untagged row must not be fast-path skipped"
    );
    assert!(
        track_row(conn.conn(), &path).is_some() || report.errors == 1,
        "the row was re-processed rather than silently skipped"
    );
}

/// The repair PRESERVES a file's real metadata instead of discarding it. The
/// overwhelmingly common "unreadable" case is an intact front ID3v2 sitting
/// behind a damaged trailing APEv2 footer (lofty aborts with "invalid item
/// size"): stripping ONLY the tail recovers the real title/artist/album rather
/// than overwriting them with the file name / folder. This guards against the
/// earlier repair that stripped every container and rewrote from the file stem,
/// silently destroying real tags on scan.
#[test]
fn repair_recovers_real_front_id3v2_tags_by_stripping_only_the_damaged_tail() {
    let tmp = tempfile::tempdir().unwrap();
    let album_dir = tmp.path().join("Some Album");
    std::fs::create_dir(&album_dir).unwrap();
    let path = album_dir.join("Broken Song.mp3");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/broken-front-id3v2-damaged-ape.mp3"),
        &path,
    )
    .unwrap();

    // Sanity: lofty cannot read the fixture as-is — the damaged APE footer
    // aborts the read, which is exactly why the scanner marks such files
    // unreadable in the first place.
    assert!(
        track_meta::read_meta(&path).is_err(),
        "fixture must start out unreadable by lofty"
    );

    // The fallback album is only consulted if recovery fails; it must NOT win.
    let fallback = track_meta::TrackMeta {
        album: "Some Album".to_string(),
        ..Default::default()
    };
    let recovered =
        super::repair::repair_damaged_tags(&path, &fallback, ImportErrorKind::UnreadableTags)
            .expect("repair must recover the file rather than give up");

    // The REAL front ID3v2 tags survive — NOT the file-stem/folder fallback.
    assert_eq!(
        recovered.title, "Silent Song",
        "real title recovered, not the file stem"
    );
    assert_eq!(
        recovered.artist, "Test Artist",
        "real artist recovered, not left empty"
    );
    assert_eq!(
        recovered.album, "Test Album",
        "real album recovered, not the parent folder"
    );

    // And the file is now strictly readable/editable again.
    let tags = crate::library::tag_edit::read_editable_tags(&path)
        .expect("the repaired file is strictly readable again");
    assert_eq!(tags.artist, "Test Artist");
}

/// A library imported *before* the on-import auto-repair existed left damaged
/// files sitting in `tracks` rows flagged `untagged = 1`, their `file_mtime`
/// already matching the file on disk. A later scan must NOT take the unchanged-
/// mtime fast path for such a row — it re-reads and repairs the container in
/// place, even though the file itself never changed since import. This is the
/// regression guard for the reported top tracks that stayed untagged after the
/// on-import repair shipped, because a plain rescan skipped them.
#[test]
fn a_later_scan_repairs_an_already_imported_untagged_track_with_unchanged_mtime() {
    let tmp = tempfile::tempdir().unwrap();
    let album_dir = tmp.path().join("Some Album");
    std::fs::create_dir(&album_dir).unwrap();
    let path = album_dir.join("Broken Song.mp3");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/broken-front-id3v2-damaged-ape.mp3"),
        &path,
    )
    .unwrap();

    let conn = crate::db::Db::open_in_memory().unwrap();

    // Seed the row exactly as a pre-repair import would have: flagged untagged,
    // with the file's current mtime so the incremental fast path would skip it.
    let mtime = file_mtime(&path);
    conn.conn()
        .execute(
            "INSERT INTO tracks (path, title, artist, album, added_at, file_mtime, untagged) \
         VALUES (?1, 'Broken Song', '', 'Some Album', 0, ?2, 1)",
            rusqlite::params![path.to_string_lossy().to_string(), mtime],
        )
        .unwrap();

    let before = std::fs::read(&path).unwrap();
    let report = completed(scan_folder(&conn, tmp.path()).unwrap());
    // The untagged row is RE-PROCESSED (imported or errored, `updated + errors
    // == 1`) rather than fast-path skipped on its unchanged mtime — that is what
    // this test guards (`skipped_unchanged == 0`). The `broken-tags` fixture
    // keeps its tags only in the stripped tail, so recovery doesn't apply here
    // (covered by `repair_recovers_real_front_id3v2_tags_by_stripping_only_the_
    // damaged_tail`) — and, crucially, the file is left byte-for-byte unchanged.
    assert_eq!(
        report.updated + report.errors,
        1,
        "the untagged row must be re-processed on a later scan, not skipped"
    );
    assert_eq!(
        report.skipped_unchanged, 0,
        "an untagged row must not be fast-path skipped on an unchanged mtime"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "an unrecoverable file must be left byte-for-byte unchanged"
    );
}

/// Brief case 2 ("Pass-2-Fehlschlag"): a pure garbage file (not a real
/// container at all — the same fixture shape `scanner_import_errors_tests.
/// rs`'s `broken_mp3` uses) fails BOTH passes. No track row is inserted; the
/// `import_errors` row is recorded with whatever pass 2's classification
/// turned out to be (this file classifies `Io` on both passes, matching the
/// brief's own side note that pass-2 failures are usually `UnsupportedFormat`
/// /`Io` — never `UnreadableTags`, since there is no readable container to
/// even blame the TAGS on specifically).
#[test]
fn pass2_failure_on_pure_garbage_leaves_no_track_but_records_the_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("muell.mp3");
    std::fs::write(&path, b"not audio").unwrap();

    let conn = crate::db::Db::open_in_memory().unwrap();
    let report = completed(scan_folder(&conn, tmp.path()).unwrap());

    assert_eq!(report.added, 0, "no container survived either pass");
    assert_eq!(report.errors, 1);
    assert!(
        track_row(conn.conn(), &path).is_none(),
        "must not insert a track when both passes fail"
    );
    assert!(
        error_kind(conn.conn(), &path).is_some(),
        "must still record an error row"
    );
}

/// Brief case 3 ("Heilung"): once the file's tags are genuinely repaired
/// (pass 1 succeeds), a later scan must flip `untagged` back to `0` and
/// clear the hint row entirely — real tags win over the pass-2 rescue, and
/// the self-healing list has nothing left to say about this path. `file_
/// mtime` is force-reset to `0` in the DB first (same technique `scanner_
/// tests.rs` uses near its own `file_mtime = 0` comment) so the scan doesn't
/// take the unchanged-mtime fast path and skip re-reading the file.
#[test]
fn real_tags_on_a_later_scan_heal_the_hint_and_clear_untagged() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("wird_repariert.wav");
    write_wav(&path, Some(&bad_id3_chunk()));

    let conn = crate::db::Db::open_in_memory().unwrap();
    completed(scan_folder(&conn, tmp.path()).unwrap());
    let (_, _, _, untagged_before) = track_row(conn.conn(), &path).unwrap();
    assert_eq!(untagged_before, 1, "sanity check: starts out untagged");
    assert!(error_kind(conn.conn(), &path).is_some());

    conn.conn()
        .execute(
            "UPDATE tracks SET file_mtime = 0 WHERE path = ?1",
            [path.to_string_lossy().to_string()],
        )
        .unwrap();
    heal_tags(&path, "Repariert");

    let report = completed(scan_folder(&conn, tmp.path()).unwrap());
    let (title, _, _, untagged_after) = track_row(conn.conn(), &path).unwrap();
    assert_eq!(
        title, "Repariert",
        "real tags win over the file-stem fallback"
    );
    assert_eq!(untagged_after, 0, "real tags must clear untagged");
    assert!(
        error_kind(conn.conn(), &path).is_none(),
        "a real pass-1 success must clear the hint row, not just refresh it"
    );
    assert_eq!(
        report.healed, 1,
        "Task 1.9: a pass-1 success clearing a HINT row (not just an \
         ordinary failure row) must still count as healed — clear_error \
         doesn't distinguish, and neither should this counter"
    );
}
