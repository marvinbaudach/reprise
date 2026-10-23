---
slug: android-loudness-target
worktree: /home/marvin/Projects/reprise-android-loudness-target
branch: feature/android-loudness-target
phase: planned
codex_session:
created: 2026-10-23
---
# The phone normalises to -14 LUFS

## Problem

On the phone, Reprise plays far quieter than other audio. Reported 2026-10-23 in
the car over Bluetooth A2DP: Google Maps guidance is loud, Reprise is quiet.

Both share one volume (`USAGE_ASSISTANCE_NAVIGATION_GUIDANCE` maps to
`STREAM_MUSIC` on the device; `dumpsys audio` shows one `bt_a2dp` index), so the
difference is in the signal. Android's volume normalisation is on by default
(`playback.replay_gain_mode = "track"`) and aims at the ReplayGain 2.0 reference
of **-18 LUFS** (`crates/reprise-core/src/library/loudness.rs:3`). Typical
modern masters sit at -8 to -10 LUFS, so the phone turns them down by 8–10 dB.
Streaming services aim at -14 LUFS. A phone has no amplifier headroom to make up
the difference, a desktop does.

Excluded on the way here: there is no in-app volume besides the sleep timer, which
restores 1.0; Maps abandons audio focus after every prompt, so Reprise is not left
ducked.

## Decision

The phone aims at **-14 LUFS**. The desktop stays at -18 LUFS: unchanged
behaviour there, and no new setting anywhere.

The target is a parameter of Core's gain resolution, not a second constant
hidden in a frontend. Core stays platform-neutral and names the two levels; each
frontend picks one.

## Design

`crates/reprise-core/src/library/loudness.rs`

- New `Copy` type `LoudnessTarget` wrapping the target in LUFS, with two named
  constants: `LoudnessTarget::REPLAYGAIN` (-18.0, the ReplayGain 2.0 reference, equal
  to today's `REFERENCE_LUFS`) and `LoudnessTarget::STREAMING` (-14.0). Accessor
  `lufs()` and `offset_db()` (= target − `REFERENCE_LUFS`).
- `GainInputs` gains a field `target: LoudnessTarget`.
- `resolve_gain`:
  - A **tag** gain is relative to the ReplayGain reference, so the resolved gain is
    `tag_gain + target.offset_db()`. This applies to both the track and album tags.
  - A **measured** gain is `target.lufs() − integrated_lufs`. This applies to both
    the track and album measurements, and replaces both `REFERENCE_LUFS − …` sites.
  - The peak cap and the `[MIN_GAIN_DB, MAX_GAIN_DB]` clamp apply **after** the
    offset, unchanged. A raised target can therefore never clip a peak the tag or
    measurement knows about.
  - `Off` stays unity whatever the target.

`crates/reprise-core/src/queries/track_gain.rs`

- `effective_gain_db(db, track_id, mode, target)`; the private helpers carry the
  target through to `GainInputs`.

Callers:

- `crates/reprise-gnome/src/ui/lyrics/player_lyrics.rs:378`,
  `crates/reprise-gnome/src/ui/playback/up_next_transport.rs:383`,
  `crates/reprise-gnome/src/ui/playback/audio_effects.rs:29` pass
  `LoudnessTarget::REPLAYGAIN`. Desktop behaviour must be byte-for-byte the same.
- `crates/reprise-android-ffi/src/playback_session/gain.rs` `gain_db_for` passes a
  named constant `PHONE_LOUDNESS_TARGET = LoudnessTarget::STREAMING` with a doc
  comment giving the reason (no amplifier headroom; streaming-service level).
  `refresh_gains` and `playback_item` go through `gain_db_for`, so the pre-fed
  track and a live mode change get the same target.

Kotlin is not touched: `TrackGainAudioSink` applies whatever `gain_db` it is handed,
and its +12 dB clamp is unaffected.

MCP: the target is not a user-facing setting and nothing a caller can name, so it
has no MCP surface. This is deliberate, per the rulebook's "every feature reaches
every frontend".

## UX rules (`docs/ux-rules.md`)

- **PLAY-19** stays `[active] [core]` and keeps its meaning. One clause changes,
  from "aimed at the ReplayGain 2.0 reference of -18 LUFS" to "aimed at the
  frontend's loudness target: the ReplayGain 2.0 reference of -18 LUFS on the
  desktop, -14 LUFS on the phone (PLAY-25)". The existing `play_19_*` tests keep
  their names and pass `LoudnessTarget::REPLAYGAIN`.
- **New PLAY-25** `[active] [android]` (same level as PLAY-21, which is also proven
  at the FFI session seam): **The phone normalises to -14 LUFS.** A
  measured track aims at -14 LUFS. A tagged track's gain is raised by the 4 dB
  between the ReplayGain reference and that target. The peak cap and the safe range
  still apply after the raise, so a raised gain never clips the chosen source's
  peak. The desktop keeps -18 LUFS. The audible level in the car is a manual check.
  - Rule-named tests:
    - `play_25_a_measured_track_aims_at_the_streaming_target` (core,
      `loudness_tests.rs`): -20 LUFS with peak 0.5 → +6.0 dB at STREAMING, +2.0 at
      REPLAYGAIN.
    - `play_25_a_tag_gain_is_raised_by_the_target_offset` (core): tag -3.5, peak
      0.5 → +0.5 dB at STREAMING.
    - `play_25_the_peak_cap_applies_after_the_raise` (core): tag +1.0, peak 0.9 →
      capped at `-20·log10(0.9)` ≈ 0.915 dB, not 5.0.
    - `play_25_album_mode_raises_album_tags_and_measurements` (core).
    - `play_25_the_phone_session_resolves_gain_at_the_streaming_target`
      (`reprise-android-ffi`, at the `gain_db_for`/`playback_item` seam with a
      measured track; reuse the fixture the existing FFI loudness tests use).
  - All five run in `cargo test --workspace`. `scripts/check-ux-traceability.sh`
    counts a rule-named `#[test]` anywhere under `crates/`, whatever the level tag.
  - Grill decision (2026-10-23): amend PLAY-19's target clause instead of replacing
    the rule. Every guarantee PLAY-19 makes keeps its meaning, and only the number
    depends on the frontend now, which PLAY-25 pins. A strict replacement would
    only rename about 12 tests.

## Tasks (test-first)

1. Core: add `LoudnessTarget` and the `GainInputs.target` field. Write the four
   `play_25_*` core tests and watch them fail to compile, then fail. Implement in
   `resolve_gain`. Update the existing `play_19_*`/`loudness_tests.rs` fixtures to
   pass `REPLAYGAIN`; all of them stay green with unchanged expected values.
2. Core: thread `target` through `effective_gain_db` and its helpers. Update the
   `track_gain.rs` tests (`REPLAYGAIN`).
3. GNOME: three call sites pass `REPLAYGAIN`.
4. FFI: `PHONE_LOUDNESS_TARGET` in `gain.rs`, plus the FFI `play_25_*` test.
5. Rulebook: PLAY-19 clause and the new PLAY-25 in the same commit as tasks 1–4,
   which is where the rule becomes `[active]`.
6. Gates: `cargo fmt --check`, `cargo clippy --all-targets --workspace -- -D
   warnings`, `cargo test --workspace`, `scripts/check-ux-traceability.sh`,
   `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` (must be
   empty). Files stay < 800 lines. If `loudness_tests.rs` would breach that, put the
   new tests in a sibling `loudness_target_tests.rs`.

Not in scope: an in-app target setting, any desktop change, Kotlin, Maps.

Manual check **before landing** (grill decision 2026-10-23): build the release APK
from the feature worktree (`REPRISE_APK_WT=<worktree> ~/.cache/reprise-apk/build-apk.sh`)
and install it with `adb install -r` under `device-lock`. The debug-key fallback keeps
the user's data. The user then listens in the car with normalisation on Per Track.
The branch lands only after that.

## Parallelität

No cut. Task 1 changes `GainInputs` and task 2 changes the signature of
`effective_gain_db`; tasks 3 and 4 do not compile without them, and task 5 must
land in the same commit. One strand, one worktree. The whole change is about
150 lines.
