---
slug: loudness-and-cue-sheets-cue-parser
worktree: /home/marvin/Projects/reprise-loudness-and-cue-sheets-cue-parser
branch: feature/loudness-and-cue-sheets-cue-parser
phase: shipped
codex_session:
created: 2026-10-04
---
# Strand cue-parser — CUE sheet parser (wave 1)

Mother plan: `docs/plans/loudness-and-cue-sheets.md`. Spec: `docs/superpowers/specs/2026-10-04-cue-sheets-design.md` (already on this branch).

## Ownership
`crates/reprise-core/src/cue/**` and the single `pub mod cue;` line in `crates/reprise-core/src/lib.rs`. Nothing else. `AUDIO_EXTENSIONS` is read from `library/scanner.rs` (make it `pub(crate)` only if it is not already reachable — if that needs an edit in `scanner.rs`, copy the list into `cue/` with a comment instead; `scanner.rs` belongs to strand r128).

## Tasks

### P1 — Parser
Files: new `crates/reprise-core/src/cue/{mod.rs, parse.rs, parse_tests.rs, text.rs}`, `lib.rs`.
- `CueSheet { title, performer, date, genre, files: Vec<CueFile> }`,
  `CueFile { name, tracks: Vec<CueTrack> }`, `CueTrack { number, title, performer,
  index00: Option<Frames>, index01: Frames }` (75 frames/s).
- Text: strip BOM; UTF-8, else CP1252 (hand-written 128-entry table, no new crate).
- Tolerant of `REM` lines, quotes, CRLF, lowercase keywords; rejects missing `INDEX 01`,
  non-monotonic indices, duplicate track numbers (typed `CueError`).
- Tests from literal sheets: EAC-style, multi-`FILE`, CP1252 umlauts, `INDEX 00`, broken.

### P2 — Segments and file resolution
Files: new `cue/segments.rs`, `cue/segments_tests.rs`.
- `resolve_file(sheet_dir, name, existing: &[PathBuf]) -> Option<PathBuf>`: exact, then
  case-insensitive, then same stem with any `AUDIO_EXTENSIONS` entry (EAC sheets name a
  `.wav` that was later encoded to `.flac`).
- `segments(sheet, durations: &HashMap<PathBuf, i64>) -> Result<Vec<CueSegment>, CueError>`:
  `CueSegment { path, segment_index (1-based, unique per path), start_ms, end_ms,
  title, performer, track_no, album fields }`; a track ends at the next track's
  `INDEX 01` in the same file (pregap belongs to the previous track), the last at the
  file's duration; `INDEX` past the end ⇒ error.
- Pure; no DB, no scanner wiring (that is wave 2).

---
