---
slug: loudness-and-cue-sheets-r128
worktree: /home/marvin/Projects/reprise-loudness-and-cue-sheets-r128
branch: feature/loudness-and-cue-sheets-r128
phase: shipped
codex_session:
created: 2026-10-04
---
# Strand r128 — measured loudness (wave 1)

Mother plan: `docs/plans/loudness-and-cue-sheets.md`. Spec: `docs/superpowers/specs/2026-10-04-r128-loudness-design.md` (already on this branch).

## Ownership
`crates/reprise-core/src/{library/{loudness*,loudness_store*,scanner*,startup_tasks,settings,mod}.rs, queries/{track_gain*,mod}.rs, render_data_session*.rs, waveform.rs, spectrogram_backfill.rs, db_spectrogram.rs, db.rs, playback.rs, device_sync/{analysis_sidecar,mobile_import}*.rs}`, `crates/reprise-core/Cargo.toml`, `Cargo.lock`, `crates/reprise-platform-linux/**`, `crates/reprise-gnome/**` (playback, lyrics start, preferences, contract fakes), `crates/reprise-android-ffi/**`, `android/**`, `docs/ux-rules.md`. Never `crates/reprise-core/src/lib.rs` or `crates/reprise-core/src/cue/**`.

## Tasks

### R1 — Loudness model and gain resolution (core, pure)
Files: new `crates/reprise-core/src/library/loudness.rs`, `library/loudness_tests.rs`; `library/mod.rs` (`pub mod`). **Not `lib.rs`** — that line belongs to the parser strand.
- `pub const REFERENCE_LUFS: f64 = -18.0;`
- `pub struct ReplayGainTags { track_gain_db, track_peak, album_gain_db, album_peak: Option<f64> }`
- `pub struct MeasuredLoudness { integrated_lufs: f64, true_peak: f64 /* linear */ }`
- `pub fn album_loudness(tracks: &[(f64 /*lufs*/, i64 /*duration_ms*/)]) -> Option<f64>` —
  duration-weighted energy mean `10·log10(Σ dᵢ·10^(Lᵢ/10) / Σ dᵢ)`.
- `pub struct GainInputs { mode: ReplayGainMode, tags, measured: Option<MeasuredLoudness>, album_measured: Option<(f64 /*lufs*/, f64 /*peak*/)> /* None until every album track is measured */ }`
- `pub fn resolve_gain(inputs) -> ResolvedGain { gain_db: f64, source: GainSource /* Tag|Measured|None */ }`:
  Off → 0; Track → tag track gain, else measured track; Album → tag album gain, else
  measured album, else the Track rule. Peak cap: `gain_db ≤ −20·log10(peak)` with the
  peak that belongs to the chosen value. Opus `R128_*` tags arrive pre-converted (R2).
- Tests (`play_<id>_…` rule-named where they prove a rule): table over every branch,
  cap, album fallback, silence (−∞ LUFS ⇒ no measured value).

### R2 — Read ReplayGain tags in the scanner, store them, force one re-read
Files: `library/scanner_meta.rs`, `library/scanner.rs` (`tag_param_values`),
`library/scanner_entry.rs` (upsert + `known_row`), new `library/loudness_store.rs` (migration v88 + loudness queries, registered in `library/mod.rs`),
`db.rs` (register, `SUPPORTED_SCHEMA_VERSION = 88`), tests.
- `TrackMeta` gains `replay_gain: ReplayGainTags`. Parse `ItemKey::ReplayGain*`
  (strings like `-7.32 dB`, tolerant of missing unit/locale comma). Opus
  `R128_TRACK_GAIN`/`R128_ALBUM_GAIN` (Q7.8 integer, relative to −23 LUFS) via
  `ItemKey::Unknown`, converted `gain_db = q/256 + 5.0`; R128 has no peak.
- Migration v88: `ALTER TABLE tracks ADD COLUMN rg_track_gain REAL, rg_track_peak REAL,
  rg_album_gain REAL, rg_album_peak REAL, tag_scan_version INTEGER NOT NULL DEFAULT 0`.
- `const TAG_SCAN_VERSION: i64 = 1;` The incremental fast path skips a file only when
  mtime is unchanged **and** `tag_scan_version >= TAG_SCAN_VERSION`; the upsert writes it.
  This re-reads every file once after the upgrade without touching `file_mtime` (which
  would fire `invalidate_track_render_data`).
- Tests: fixtures with each tag form (generate with lofty in the test), Opus R128,
  missing tags; migration test (v87 → v88, idempotent); rescan re-reads a v0 row once.

### R3 — Loudness meter inside `RenderDataSession`; desktop uses the session too
Files: `Cargo.toml` (`ebur128 = "0.1"` in reprise-core), `render_data_session.rs`
(+ `render_data_session_loudness_tests.rs` if the file nears 800 lines),
`waveform.rs` (core: `TrackRenderData` gains `loudness: Option<MeasuredLoudness>`),
`reprise-platform-linux/src/waveform.rs`.
- The session measures with `ebur128::EbuR128::new(channels, rate, Mode::I | Mode::SAMPLE_PEAK)` (true peak cost 26 % more wall-clock than sample peak on the backfill, so the plan's own switch rule applied)
  on the interleaved input **before** the downmix. Add `push_pcm_f32` beside
  `push_pcm_i16`; both feed the meter and the existing mono path.
- Desktop extraction switches its caps to `audio/x-raw,format=F32LE,layout=interleaved`
  (native rate and channels) and pushes into `RenderDataSession` instead of its own
  accumulators, so peaks, spectrogram and loudness exist exactly once (Android already
  uses the session). Keep `extract_peaks*` behaviour; cancellation unchanged.
- Tests (pure, synthetic PCM): −6.02 dB amplitude halving ⇒ −6.02 LU ± 0.05; identical
  stereo vs mono ⇒ +3.01 LU ± 0.05; silence ⇒ `None`; true peak of a full-scale sine ≈ 1.0;
  waveform/spectrogram output of the session unchanged for the existing session tests.
  Desktop: one GStreamer test decoding a generated stereo WAV yields loudness.
- Measure and record in the commit message: backfill wall-clock on a 200-track
  generated library before/after (true peak costs CPU; drop to `SAMPLE_PEAK` if the
  backfill slows by more than 25 %).

### R4 — Store loudness; the backfill fills it; startup re-runs once
Files: `library/loudness_store.rs` (table in v88), `db_spectrogram.rs` (`pending_render_data_tracks`,
`set_track_render_data`, trigger), `spectrogram_backfill.rs`, `library/startup_tasks.rs`.
- Table `track_loudness(track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
  source_mtime, source_size, source_device, source_inode, format_version INTEGER NOT NULL,
  integrated_lufs REAL /* NULL = silent */, true_peak REAL)`.
- `set_track_render_data` writes it in the same fingerprint-checked transaction.
- `pending_render_data_tracks` also selects tracks without a current `track_loudness`
  row — the whole library is analysed once more after the upgrade.
- Recreate `invalidate_track_render_data` to delete the loudness row as well.
- Bump whatever `SignatureTask::Spectrogram`'s signature is built from so the startup
  gate runs the backfill once after the upgrade.
- Queries: `measured_loudness(conn, track_id)`, `album_measured_loudness(conn, track_id)`
  (album identity = the library's existing album grouping key; `None` until every
  present track of that album has a row).
- `pub fn effective_gain_db(conn, track_id, mode) -> f64` in new `queries/track_gain.rs` (registered in `queries/mod.rs`)
  combining R1–R4. Tests on an in-memory DB.

### R5 — Sidecar format v2
Files: `device_sync/analysis_sidecar.rs` (+ tests), `device_sync/mobile_import.rs`.
- `FORMAT_VERSION = 2` appends `has_loudness u8, integrated_lufs f64, true_peak f64`.
  Decode accepts 1 and 2 (v1 ⇒ `loudness: None`, the phone measures itself).
- Import stores loudness through `set_track_render_data`.
- Tests: v2 round-trip, v1 bytes still import, unknown version rejected.

### R6 — Contract carries the gain; desktop applies it at the stream boundary
Files: `reprise-core/src/playback.rs`, `reprise-platform-linux/src/{player.rs,
player_effects.rs, gapless.rs, crossfade.rs, player_pipeline.rs}`, every
`impl PlaybackBackend` fake in reprise-gnome (mechanical).
- `pub struct PlaybackItem<'a> { pub path: &'a str, pub gain_db: f64 }`;
  `play(&self, item: PlaybackItem)`, `set_next(&self, item: Option<PlaybackItem>)`.
  (`play_uri` for streams/podcasts stays gain-less.)
- `player_effects.rs`: a `volume` element named `reprise-track-gain` replaces
  `rgvolume`, always present (no more topology change on mode switch).
- Gain switch: a pad probe on the `reprise-track-gain` element's sink pad (behind the playback queue) watches `STREAM_START`
  in the streaming thread and applies the pending next gain before the first buffer of
  the new stream — not at `about-to-finish`, not on the bus.
- Crossfade: the secondary playbin's filter gets the next item's gain at build time.
- Tests: platform-linux test that the gain element value changes exactly at the
  stream-start of the second of two generated files in gapless mode (probe-based);
  `same_filter_topology` no longer rebuilds on mode change.

### R7 — GNOME passes the effective gain; mode change applies live
Files: `reprise-gnome/src/ui/playback/{player_controller.rs, up_next_transport.rs,
audio_effects.rs}`, `ui/lyrics/player_lyrics.rs` (`start_track_for_lyrics`).
- `present_track` / `feed_next` compute `effective_gain_db(conn, id, mode)` and pass
  `PlaybackItem`. A ReplayGain mode change recomputes current and next gain and sets
  them live (no restart).
- Tests: GTK-free controller tests with the fake backend asserting the gain handed over
  for tagged / untagged / mode Off.

### R7b — Default mode is Track
Files: `crates/reprise-core/src/library/settings.rs` (`ReplayGainMode` getter default), `crates/reprise-core/src/playback.rs` (`AudioEffects::default`).
- An unset `replay_gain_mode` now means `Track` on both platforms (grill 2026-10-04). No migration: nothing has shipped.
- Test: fresh DB ⇒ `Track`; explicit `Off` stays `Off`.

### R8 — Android: setting, port carries gain, boundary-accurate application
Files: `reprise-android-ffi/src/{playback.rs, playback_settings.rs}`, generated bindings,
`android/.../{Media3PlaybackPort.kt, ReprisePlaybackService.kt, LivePcmAudio.kt}`, new
`TrackGainAudioSink.kt`, the Android settings page that hosts playback options, tests.
- FFI: `AndroidPlaybackPort.playPath(path, gainDb)` / `setNext(uri, gainDb)`; the
  bridge computes `effective_gain_db` from the phone DB. Expose get/set ReplayGain mode.
- `TrackGainAudioSink : ForwardingAudioSink` wraps the `DefaultAudioSink` built in
  `LivePcmRenderersFactory`. It records each `setOutputStreamOffsetUs(offset)` together
  with the gain queued for that stream and switches when `handleBuffer`'s
  `presentationTimeUs` reaches that offset; it scales 16-bit PCM in place with
  saturation. If this cannot be made boundary-accurate, fall back to switching on
  `onMediaItemTransition` and record the measured lag in the commit.
- Settings: Off / Track / Album, same strings as desktop.
- Tests: JVM unit tests for the sink's scaling and boundary switch with synthetic
  offsets; FFI test that `setNext` carries the resolved gain.

### R9 — Rules
`docs/ux-rules.md`, playback section (next free `PLAY-*` ids):
- untagged tracks are normalised from a measured loudness; tags win; Off disables both;
- the gain changes at the first sample of the next track (gapless and crossfade);
- Android offers the same three modes.

---
