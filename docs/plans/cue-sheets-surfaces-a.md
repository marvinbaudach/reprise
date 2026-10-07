---
slug: cue-sheets-surfaces-a
worktree: /home/marvin/Projects/reprise-cue-sheets-surfaces-a
branch: feature/cue-sheets-surfaces-a
phase: planned
codex_session:
created: 2026-10-06
---
# CUE sheets — surfaces, strand a: desktop playback

Mother plan: `docs/plans/cue-sheets-surfaces.md` (decisions, working rules, rule ids,
post-merge checks). Read it first; this file holds only strand a's ownership and tasks.

**Purpose:** a CUE track plays from its own start to its own end on the desktop;
contiguous tracks of one file play through without a reload; position, duration and seeks
are track-relative; no crossfade at a CUE transition.

## Owns

- `crates/reprise-platform-linux/src/{player.rs,player/**,gapless.rs,crossfade.rs,
  player_effects.rs,player_pipeline.rs,cava_stage.rs,signals.rs}` plus new siblings beside
  them (e.g. `player_segment.rs`, `player/tests/segment_*_tests.rs`), wired from a parent this
  strand owns — never from `lib.rs`.
- `crates/reprise-gnome/src/ui/playback/**`, `ui/window/player_backends.rs`, `ui/mpris_mirror.rs`.
- `crates/reprise-core/src/playback.rs` — doc and *defaulted* additions only (a signature
  change to `play`/`set_next`/`seek_to` touches all 14 `PlaybackBackend` implementors, 12 of
  them GNOME test fakes outside this list).
- `docs/ux-rules.md` section C only: PLAY-22, PLAY-23, PLAY-24.

Not owned, do not touch: `platform-linux/src/waveform.rs` (strand c), `mpris/mod.rs` (767
lines; MPRIS reads the mirror and needs no change).
Near the cap: `player.rs` 664, `up_next_transport.rs` 747.

## Facts (origin/dev `c0188ae2e1`, playback files unchanged since)

- `PlaybackItem<'a> { path, gain_db, segment: Option<(i64, i64)> }` (`core/playback.rs:389`);
  its doc already says position/duration are then segment-relative.
  `TrackSummary::playback_segment()` (`queries/track_summary.rs:39`). Segments of one file are
  contiguous: `end_ms` = next INDEX 01; the last ends at the probed metadata duration.
- `try_play` (`player.rs:329`): Null → uri → gain → Playing. `seek_to` (`player.rs:486`):
  `seek_simple(FLUSH|KEY_UNIT)`. Ticker thread (`player.rs:214`) pushes
  `Position{position_ms, duration_ms}` every 500 ms from `query_position/duration` — file-absolute.
- Gapless: `connect_about_to_finish` (`gapless.rs:72`) sets the next uri and `pending_gain`,
  sets `handoff_pending`; bus `StreamStart` → `AdvancedToNext` (`gapless.rs:113`). Gain
  switch: StreamStart pad probe on the `reprise-track-gain` sink pad behind the 1 s
  `reprise-playback-queue` (`player_effects.rs:239-286`). No buffer probe exists in production.
- `refresh_in_flight_gain` (`player.rs:625`) identifies the in-flight successor by URI
  equality — wrong once current and next are two segments of one file.
- `reported_duration_ms` duration hold during a handoff (`player.rs:45`).
- Crossfade (`crossfade.rs:118`) triggers on `position ≥ duration − seconds` of the ticker
  values; the secondary playbin starts at file 0; promotion emits `AdvancedToNext` itself.
- Frontend fill sites of `segment`: `lyrics/player_lyrics.rs:376` (play) and
  `playback/up_next_transport.rs:381` (`feed_next`). `AdvancedToNext` →
  `player_event_handling.rs:242` → `advance_gaplessly()`. Every position consumer reads the
  event values; play counting and scrobbling compare `max_position_ms` with
  `summary.duration_ms` (segment) — right once the backend reports segment-relative values.
  All seeks funnel through `mpris_mirror.rs:352`. `Player::new_with_generation` has no
  production caller.
- Tests: `player/tests.rs` + `player/tests/*.rs`, `fakesink` via `REPRISE_AUDIO_SINK` under
  `AUDIO_SINK_TEST_LOCK`, generated sine WAVs (`handoff_duration_tests.rs:33 write_sine_wav`),
  main context pumped by hand. Existing rule tests: `play_20a_*`, `play_20b_*`, `play_19_*`.

## Tasks

### a1 — segment start and track-relative clock
- `play(item)` with `segment: Some((s, e))`: load, preroll, flushing **ACCURATE** seek to `s`,
  then Playing; first audible sample within 20 ms of `s` (tolerance stated in the test).
- The player keeps the active cut `{uri, start_ms, end_ms, open_end}`; the ticker reports
  `position − start` and `end − start` (or `file duration − start` when open-ended, a2).
- `seek_to(p)` on a segment: ACCURATE seek to `start + clamp(p, 0, len − 1)`. Whole-file
  behaviour (KEY_UNIT) unchanged.
- Tests (sine WAV with an audible marker per region): position never below 0 or above the
  segment length; a seek lands inside the segment; whole-file regression unchanged.

### a2 — in-file boundary probe and open end (decisions 1, 3)
- Buffer probe on the gain element's sink pad; the boundary is the active cut's `end_ms`.
- Open end: when `end_ms` is within **1000 ms** of the pipeline's reported file duration,
  install no boundary — the track plays to EOF and ends like a whole file (named constant).
- Contiguous same-file next: at the first buffer with PTS ≥ boundary switch the gain, swap
  the active cut, post an application message so `AdvancedToNext` reaches the main context
  in order with ticks; apply the duration hold across the swap. **No `Position` tick computed
  against the old cut may arrive after the `AdvancedToNext`** — test it; the guard lives in
  the player (the generation API is unused by the app).
- Otherwise: drop buffers ≥ boundary and emit `TrackFinished` once (the frontend starts the
  next item with `play()`; a short gap is accepted, decision 1).
- Tests: `play_23_*` contiguous segments → exactly one `AdvancedToNext`, no `StreamStart`,
  every buffer after the boundary carries the next track's gain (tolerance one buffer);
  non-contiguous next → `TrackFinished`; no next → `TrackFinished`, nothing audible past the
  end; an `end_ms` 400 ms short of the file duration → plays to EOF.

### a3 — prefeed and successor identity
- `QueuedTrack` carries the segment. `set_next` with the contiguous same-file segment arms the
  probe instead of `next_uri`. Any other next item where either side is a CUE track
  (`segment.is_some()`) is **not prefed**: the transition is the `TrackFinished` → `play()`
  path of a2. Whole-file → whole-file stays prefed as today.
- `refresh_in_flight_gain` identifies the successor by `(uri, segment)`, not uri.
- Tests extend `gain_refresh_tests.rs` with two segments of one file.

### a4 — crossfade never at a CUE transition (decision 2)
- The crossfade trigger is skipped when current or next is a CUE track; contiguous segments
  play through (a2), other CUE transitions change hard (a3). No secondary-playbin seek.
- Test in `crossfade_transition_tests.rs`: crossfade on, current a CUE track, next a whole
  file → no secondary playbin, `TrackFinished` then `play()`; whole-file pairs still crossfade.

### a5 — frontend checks and rules
- Prove with GNOME tests on a fake backend that play counting, scrobble eligibility, the
  previous-track `seek(0)`, the sleep timer and lyrics sync behave on track-relative values.
  No code change expected; fix only what a test shows.
- Rules in section C: **PLAY-22** a CUE track plays from its own start to its own end; its
  seek bar and time are its own. **PLAY-23** consecutive tracks of one file play without a gap
  or reload, each with its own loudness gain. **PLAY-24** no crossfade into or out of a CUE
  track. Each `[active]` only with its rule-named test (`play_22_*`, `play_23_*`, `play_24_*`).

## Done

All tasks committed, gate battery green on the worktree, the ignored GTK rule tests for
PLAY-22…24 green. Cross-strand checks are the mother plan's post-merge list — do not attempt
them here.

## As built

- **a1** `eda95563ed` — `player/segment.rs` (declared from `player.rs`) holds the active
  `Cut {start_ms, end_ms, open_end}` behind one mutex in a `SegmentGate`. `play()` with a segment
  prerolls paused (`SEGMENT_PREROLL_TIMEOUT` 5 s; a timeout fails the attempt into the existing
  rebuild-and-retry), caches the file duration, then seeks `FLUSH|ACCURATE` to the start before
  `Playing`. A refused seek is logged, not failed (failing would mark the file missing). The
  ticker computes and *sends* each tick under the cut lock; `seek_to` on a cut is `ACCURATE` to
  `start + clamp(p, 0, len − 1)`.
- **a2** `ab3880c9cc` — the boundary probe sits on the gain element's sink pad (installed by
  `build_playbin`, so rebuilds and the crossfade secondary carry it; a no-op without a cut). It
  converts PTS to stream time with the pad's own last SEGMENT event (kept per pad, not in the
  shared gate). Hand-off: gain switch, cut swap, `stream_generation` bump and `AdvancedToNext` are
  sent **from the probe under the cut lock** — not via a bus application message, which could not
  be ordered against ticker ticks. No armed successor: the boundary buffer and everything after is
  dropped, `TrackFinished` is sent once, and the bus watch suppresses the file's later EOS. Open
  end: `OPEN_END_TOLERANCE_MS` = 1000. Arming through `set_next` landed here (the PLAY-23 test
  needs it); PLAY-23 also has a Crossfade-mode variant.
- **a3** `bb226e131a` — `QueuedTrack.segment`; `SegmentGate::route_next` arms the contiguous
  successor and allows the URI slot only when neither side is a CUE track. The in-flight gain
  refresh moved to `player/successor.rs` (size cap) and matches `(uri, segment)`, including a
  track the probe already handed over to (`handed_off`, cleared on the next `set_next`).
- **a4** `9559a895f9` — `CrossfadeEngine::maybe_start` returns while a cut is active.
- **a5** `85e40fccd7` — GTK controller tests (`ui/playback/cue_track_frontend_tests.rs`, display
  tests named `play_22_*`); no frontend code change was needed. Lyrics *position* forwarding is
  not asserted — `PlayerLyrics::position_ms` is private to `ui/lyrics/**`, outside this strand;
  only the lookup (`lyrics_query_for`) is.

Open: with the transition set to **Off**, `feed_next` sends `None`, so contiguous CUE tracks get
the short `TrackFinished` → `play()` gap. PLAY-23 is worded for Gapless and Crossfade; whether Off
should still arm the in-file successor is a product decision, not taken here.
