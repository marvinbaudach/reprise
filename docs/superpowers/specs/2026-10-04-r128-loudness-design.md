# Measured loudness for tracks without ReplayGain tags — design

Status: approved in brainstorming, 2026-10-04. Branch: `feature/r128-loudness`.
Sibling spec, developed in parallel: `2026-10-04-cue-sheets-design.md`.

## Problem

Reprise normalises volume only for files that carry ReplayGain tags, and only on
the desktop. GStreamer's `rgvolume` reads those tags from the stream at playback
time; Reprise itself stores nothing. An untagged file plays at its mastered
level, so a library that mixes tagged and untagged files jumps in volume. The
Android app applies no gain at all.

## Decisions

| Question | Decision |
| --- | --- |
| Platforms | Measure and apply on desktop **and** Android. |
| Tag vs. measurement | A ReplayGain tag always wins; the measurement fills only the gap. |
| Reference level | −18 LUFS, the ReplayGain 2.0 reference, so measured and tagged files match. |
| Where values live | The database only. Nothing is written to music files. |

## Design

### 1. One source for the gain, in the core

- The scanner reads ReplayGain tags itself — track gain, track peak, album gain,
  album peak — and stores them on the track row.
- The analysis stores the measured integrated loudness (LUFS) and true peak per
  track, keyed by the existing `TrackSourceFingerprint`.
- A pure core function resolves the **effective gain** of a track for a given
  `ReplayGainMode` and preamp: tag if present, otherwise measurement, otherwise
  0 dB. The gain is capped so that `peak × gain ≤ 0 dBFS`.
- Desktop and Android ask only for this value. Neither reads tags at playback
  any more.

### 2. Measuring

- The `ebur128` crate (pure Rust, MIT/Apache-2.0) measures inside the existing
  `RenderDataSession`, in the same decode pass as waveform peaks and the
  spectrogram. No extra decode.
- R128 needs per-channel PCM at the source rate. The loudness meter therefore
  taps the PCM **before** the downmix to 32 kHz mono. If the current pipeline
  downmixes before PCM reaches the session, the plan moves the tap earlier.
- Desktop: measured by the existing library-wide backfill
  (`run_render_data_backfill`), so every track has a value before its first play.
- Android: measured by its existing analysis backfill.
- Invalidation follows the source fingerprint, exactly like waveform and
  spectrogram.
- The `.reprise-analysis` sidecar moves to format version 2 and carries loudness
  and true peak. A version-1 sidecar still imports its waveform and spectrogram;
  the phone measures loudness itself for those files.

### 3. Album mode

- Album gain from measurements is the duration-weighted energy mean of the
  album's track loudnesses.
- Until every track of an album is measured, album mode falls back to the track
  gain of that track.
- On an album where only some tracks carry tags, each tagged track uses its tag;
  the album value for the rest is computed from measurements only.

### 4. Applying

- **Desktop:** a `volume` element replaces `rgvolume`. Its gain is set when the
  new track **starts** (stream start), not at `about-to-finish`; otherwise the
  tail of the outgoing track jumps in volume during a gapless transition.
- **Android:** a custom Media3 `AudioProcessor` applies the gain per media item.
  `player.volume` is not enough: it can only attenuate.
- The existing `ReplayGainMode` setting (Off / Track / Album) governs both
  platforms. Off means no gain, even though loudness is still measured.

### 5. Testing

- Core: measurement on synthetic sine signals of known loudness; resolution
  table (tag present / absent / partial album / peak cap).
- Sidecar round-trip v1 → v2, and a v1 sidecar importing without loudness.
- Desktop: the gain switches at stream start of the next track, not before.
- Android: unit test for the `AudioProcessor` gain and its clipping guard.
- Manual: whether it actually sounds even on a real mixed library.

## Out of scope

- Writing ReplayGain tags into files.
- A setting to prefer measurement over tags.
- Dynamic range or other audio-character metrics (removed in schema v27; not
  coming back here).

## Overlap with the CUE-sheet branch

Both branches add a schema migration; the second to land takes the next number.
The CUE branch measures loudness per segment and therefore rebases onto this one.
