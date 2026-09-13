---
slug: the-pause-survives-a-skip-b
worktree: /home/marvin/Projects/reprise-the-pause-survives-a-skip-b
branch: feature/the-pause-survives-a-skip-b
phase: reviewed
codex_session:
created: 2026-09-02
---
# Strand B — Desktop: the pause survives a skip

Owns `crates/reprise-gnome/src/ui/playback/**`. Touch nothing else. The mother
plan is `the-pause-survives-a-skip.md`; read it first.

The desktop does **not** share the Android FFI session. It has its own
controller, and the same defect independently — confirmed by reading the code,
not inferred from the Android side.

## The defect here

- `queue_transport.rs:400-407` — `next()` calls
  `advance_playback(AdvanceReason::Manual)`.
- `up_next_transport.rs:125-129` — `advance_playback` passes
  `StartPlayback::Yes` unconditionally, for `Manual` and `Automatic` alike.
- `playback_history_transport.rs:122-126` — the previous-track path passes
  `StartPlayback::Yes` too.
- `player_controller.rs:693-698` — on `StartPlayback::Yes` it calls
  `start_track_for_lyrics` unconditionally.
- `crates/reprise-gnome/src/ui/lyrics/player_lyrics.rs:360` — that ends in
  `player.play(&summary.path)` with no check of the current pause state.

Note that `player_lyrics.rs` is **outside this strand's file ownership**. If the
fix has to change that call, stop and report it rather than reaching across the
boundary — the cut may need revisiting. Prefer a fix that decides `StartPlayback`
correctly further up, inside `ui/playback/**`, which is where the pause state is
known.

## Tasks

1. Make `StartPlayback` reflect the user's intent for a **manual** change:
   `AdvanceReason::Manual` while paused must not start the backend. Automatic
   advance (end of track) keeps starting, because the player really is playing.
2. Apply the same to the previous-track path
   (`playback_history_transport.rs:122-126`).
3. Make sure the desktop shows the same complete card while paused — the track,
   its duration and its waveform — matching the Android decision in the mother
   plan. Check where the desktop's duration comes from before assuming it needs
   changing; it may already read the library value.
4. Pressing play afterwards must start the track that is displayed, not the one
   that was playing before the skip.

## Tests

No existing desktop test pins pause/play behaviour across a track change — this
strand adds the first. Each must fail against the current code; run it there
first and say so in your summary.

- Manual next while paused: no start, state stays paused, the displayed track is
  the new one.
- Manual previous while paused: the same.
- Manual next while playing: unchanged, still playing.
- Automatic advance while playing: unchanged, still playing.
- Play after a paused skip starts the displayed track.

## Gate

```
cargo test -p reprise-gnome
```

Plus whatever the repo's standard desktop gate is; check `scripts/` rather than
inventing one.
