---
slug: loudness-and-cue-sheets
worktree:
branch:
phase: planned
codex_session:
created: 2026-10-04
strands: r128,cue-parser
merge_order: r128,cue-parser
---
# Measured loudness and CUE sheets — implementation plan — wave 1 mother

Specs (approved 2026-10-04):
- `docs/superpowers/specs/2026-10-04-r128-loudness-design.md` (branch `feature/r128-loudness`)
- `docs/superpowers/specs/2026-10-04-cue-sheets-design.md` (branch `feature/cue-sheets`)

Every task is test-first (failing test → minimal code → green → gates → commit) and ends
with the AGENTS.md gate battery. Code files stay < 800 lines; extract siblings instead.
New user-visible behaviour gets a `[planned]` rule in `docs/ux-rules.md` (next free id in
the owning section, `<!-- REVIEW: rule proposal -->`), flipped to `[active]` in the commit
whose rule-named test proves it.

Ownership note: the "Active file ownership" blocks for list geometry, multi-surface
frontends and Flathub readiness in AGENTS.md have no live branch any more (checked
2026-10-04: no `list-geometry*`, `multi-surface*`, `flathub*` branch exists). They do not
block `settings.rs`, `docs/ux-rules.md` or `scripts/`.

## Facts this plan builds on (measured 2026-10-04 against origin/dev 1384703cce)

- Schema is v87 (`db.rs:29`); migrations are `migrate_vNN(conn)` fns with a
  `user_version` guard, registered at the end of `migrate_with_cache_dirs` (`db.rs:~723`).
  `PRAGMA foreign_keys` is ON (`db_connection.rs:31`).
- `tracks.path TEXT NOT NULL UNIQUE` is an inline constraint (`db.rs:69-87`); the table
  has never been rebuilt. Ten tables reference `tracks(id)` (CASCADE / SET NULL).
- The only `ON CONFLICT(path)` on tracks is the scanner upsert
  (`library/scanner_entry.rs:~357 UPSERT_TRACK_SQL`); ~20 call sites look a track up by
  path (list in CUE-B2). Incremental skip compares `file_mtime` only (`known_row`).
- Tags: `TrackMeta` (`scanner_meta.rs:70-82`) via lofty 0.25.1; lofty maps
  `REPLAYGAIN_{TRACK,ALBUM}_{GAIN,PEAK}` to `ItemKey::ReplayGain*`. No custom keys today.
- Desktop analysis (`reprise-platform-linux/src/waveform.rs:25-27`) downmixes to mono
  32 kHz F32 inside GStreamer and does not use `RenderDataSession`; Android feeds native
  interleaved i16 into `RenderDataSession::push_pcm_i16`, which downmixes by averaging
  (`render_data_session.rs:107-114`). Loudness needs per-channel PCM, so neither path can
  measure it today.
- Backfill: `run_render_data_backfill` selects via `pending_render_data_tracks`
  (`db_spectrogram.rs:307`), stores via `set_track_render_data` (fingerprint-checked).
  Desktop starts it at startup behind `startup_tasks::begin_exact(SignatureTask::Spectrogram)`
  and after every scan. Trigger `invalidate_track_render_data` clears analysis on any
  fingerprint change.
- Sidecar `analysis_sidecar.rs`: `FORMAT_VERSION = 1`, strict equality on decode.
- Desktop playback: contract `PlaybackBackend` (`playback.rs:390`): `play(path)`,
  `set_next(Option<&str>)`; `rgvolume` sits in the playback branch of the `audio-filter`
  bin (`player_effects.rs:72-79`); gapless prefeed in `gapless.rs` (`about-to-finish` →
  `next_uri`), start detected on bus `StreamStart` + `handoff_pending` → `AdvancedToNext`;
  crossfade uses a second `playbin3` (`crossfade.rs`). Implementors of the contract:
  the Linux player, the Android FFI bridge (`reprise-android-ffi/src/playback.rs`) and
  eight GNOME test/adapter fakes.
- GNOME: `present_track` → `start_track_for_lyrics` → `player.play(&summary.path)`;
  `feed_next` → `player.set_next(path)`; `TrackSummary` has no gain.
- Android: Media3 1.11.1, `ExoPlayer` built in `ReprisePlaybackService.kt:139` with
  `LivePcmRenderersFactory` (custom `DefaultAudioSink`, 16-bit, offload disabled).
  The port holds current + one prefed item (`setMediaItem` + `addMediaItem`), emits
  `AdvancedToNext` on `onMediaItemTransition(AUTO)`. Media3 1.11.1 has
  `ForwardingAudioSink`, `AudioSink.setOutputStreamOffsetUs`, `AudioProcessor.StreamMetadata`.
  No gain/normalisation exists on Android; core's `ReplayGainMode` is not exposed by FFI.
- The phone runs the core scanner itself over SAF (`android-ffi/src/lib.rs:177`), so
  scanner changes (tags, CUE) reach Android without a separate import path.
- `library_exclusions` is keyed by `(device, inode)` or path — one key per file.
- No UX rule covers ReplayGain, gapless, or hidden tracks today.

## Decisions from the grill (2026-10-04)

1. Three waves (see below). Wave 1 runs r128 and the CUE parser concurrently.
2. The desktop analysis moves onto the core `RenderDataSession`; the library is analysed once more after the upgrade.
3. The ReplayGain mode default becomes **Track** on both platforms.
4. CUE identity is `UNIQUE(path, segment_index)` via a one-time `tracks` rebuild (spec said `segment_start_ms`; NULLs are distinct in SQLite UNIQUE).
5. A partly synced CUE album shows **all** its tracks on the phone; the original `.cue` is copied (spec said "only the selected tracks").
6. CUE tracks show online lyrics only; no `.lrc` read or write, embedded lyrics of the big file ignored.
7. Opening a CUE-covered file queues all its tracks; M3U export writes the file path, import maps it to the first track; no analysis sidecars for segments; "Remove from library" on one segment is keyed by `segment_index`; Android gain via `ForwardingAudioSink` with a measured fallback; true peak unless the backfill slows by more than 25 %.

## Wave structure

| Wave | Plan | When |
| --- | --- | --- |
| 1 | this file — strands `r128` and `cue-parser` | now, concurrently |
| 2 | `docs/plans/cue-sheets-core.md` (one strand) | after both wave-1 strands landed |
| 3 | `docs/plans/cue-sheets-surfaces.md` (three strands, cut finalised by `/plan` after wave 2) | after wave 2 landed |

## Parallelität

**Strand `r128`** — `docs/plans/loudness-and-cue-sheets-r128.md`, branch
`feature/loudness-and-cue-sheets-r128`. Owns everything listed in its file; never
`crates/reprise-core/src/lib.rs` or `crates/reprise-core/src/cue/**`. Its new core
modules hang off `library/mod.rs` and `queries/mod.rs` for exactly that reason.

**Strand `cue-parser`** — `docs/plans/loudness-and-cue-sheets-cue-parser.md`, branch
`feature/loudness-and-cue-sheets-cue-parser`. Owns `crates/reprise-core/src/cue/**`
and the one `pub mod cue;` line in `lib.rs`.

The two file groups are disjoint. Merge order: either first; the second rebases onto
dev before landing. Post-merge cross-checks: none — the parser is not wired until wave 2.
