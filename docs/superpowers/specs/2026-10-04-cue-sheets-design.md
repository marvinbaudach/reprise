# CUE sheets and single-file albums — design

Status: approved in brainstorming, 2026-10-04. Branch: `feature/cue-sheets`.
Sibling spec, developed in parallel: `2026-10-04-r128-loudness-design.md`.

## Problem

An album ripped as one audio file plus a `.cue` sheet shows up in Reprise as a
single 70-minute track. Nothing in the code knows CUE sheets. Collectors coming
from Rhythmbox or foobar2000 expect them to work.

## Decisions

| Question | Decision |
| --- | --- |
| Platforms | Desktop **and** Android. A synced CUE album must not turn back into one track on the phone. |
| Editing | CUE tracks are read-only. Reprise never writes `.cue` files. |
| Sources | A `.cue` beside the audio file, and a `CUESHEET` tag embedded in FLAC. |
| Audio formats | Only formats Reprise already scans. APE and WavPack stay out. |

## Design

### 1. Data model

- A track gains optional `segment_start_ms` and `segment_end_ms`. Both NULL means
  an ordinary whole-file track, behaving exactly as today.
- Uniqueness moves from `path` to `(path, segment_start_ms)`.
- When a valid sheet covers a file, the file is listed **only** as its segments,
  never also as a whole-file track.
- A sheet that references several audio files (one `FILE` per track) uses the
  same model with several paths.

### 2. Parser and scanner

- A pure-Rust parser in `reprise-core` reads `FILE`, `TRACK`, `INDEX 01`,
  `TITLE`, `PERFORMER`, `REM DATE`, `REM GENRE` and the album-level fields.
  `INDEX 00` (pregap) belongs to the previous track.
- Encoding: UTF-8 first, CP1252 as the fallback.
- A track ends where the next one starts; the last ends at the file's duration.
- Fields the sheet lacks are filled from the audio file's tags (album artist,
  year, cover).
- A rescan notices sheet changes through the sheet's mtime, as for audio files.
- An invalid or mismatched sheet (unknown `FILE` name, an `INDEX` past the end
  of the file, unparsable) is discarded: the file appears as one track, as
  today, and the Library Doctor reports the broken sheet.

### 3. Playback

- **Desktop:** start by seeking to `segment_start`; advance at `segment_end`.
  Position, duration and the seek bar are relative to the segment.
  **Consecutive segments of the same file play on without reloading**; only the
  current track changes at the boundary. That is true gapless, which live albums
  need.
- **Android:** Media3 `ClippingConfiguration` per media item. Gapless between two
  clips of the same file is less certain there and is measured on the device.

### 4. Every place that assumes one track per file

- **Tag editor and Library Doctor:** CUE tracks are read-only, shown with a note,
  and skipped by Doctor fixes.
- **Delete:** the file goes to the trash, together with its `.cue`, only when
  **all** of its tracks are selected. A partial selection hides those tracks
  from the library through the existing library exclusions.
- **Device sync:** the audio file and its `.cue` are copied once, however many of
  their tracks are selected. Android lists only the selected tracks.
- **Waveform, spectrogram, loudness:** the file is analysed once and sliced per
  segment. Loudness is measured per segment (rebases onto the R128 branch).
- **Scrobbling, stats, queue, MPRIS:** work through track ids; they need correct
  segment durations and nothing else.

### 5. Testing

- Parser: multi-`FILE` sheets, CP1252, `INDEX 00` pregaps, broken sheets.
- Scanner: synthetic audio files generated in the test (the developer's own
  library holds no CUE rips), sheet beside the file and embedded in FLAC.
- Desktop: a segment boundary within one file advances the track without a
  reload.
- Android: unit tests for clip boundaries; gapless is a manual device run.

### 6. Packages

1. Core: model, migration, parser, scanner.
2. Desktop playback: segment start/end, same-file continuation.
3. One-track-per-file call sites: delete, sync, tag editor, Doctor, analysis.
4. Android: clipping playback and the synced-sheet import.

## Out of scope

- Writing or repairing `.cue` files.
- APE and WavPack.
- Splitting a single-file album into separate files.

## Overlap with the R128 branch

Both branches add a schema migration; the second to land takes the next number.
This branch measures loudness per segment and therefore rebases onto R128.
