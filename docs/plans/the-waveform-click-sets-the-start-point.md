---
slug: the-waveform-click-sets-the-start-point
worktree: /home/marvin/Projects/reprise-the-waveform-click-sets-the-start-point
branch: feature/the-waveform-click-sets-the-start-point
phase: refactored
codex_session:
created: 2026-08-31
---
# The waveform click sets the start point

## What the user reported

> "wenn ich pausiert in die seek klicke sollte es nich automatisch starten zu spielen"

Confirmed situation: **app freshly started**, the last track restored in the
player bar, nothing played this session. A click into the waveform starts
playback.

## Why it does that today

The restored local track leaves the session restore in `Stopped`, not `Paused`:

- `session_player.rs:114` `self.sync_state(PlaybackState::Stopped)`
- `session_player.rs:160` `self.update_mpris_mirror(MprisPlaybackStatus::Stopped)`
- `session_player.rs:148` `self.sync_position(0, summary.duration_ms)` — the
  playhead sits at **0**; no resume position is restored for local tracks at all.

The bar looks paused (track, cover, play icon, 0:00) but the state machine says
stopped. The click lands in `seek_or_start`
(`crates/reprise-gnome/src/ui/playback/seek_start.rs:13`) and takes the `Stopped`
branch, which starts playback and only then seeks.

That is deliberate, from `336d734ad3` *"The waveform works before the first
play"* (#736): the waveform became clickable before the first play, and starting
playback was the mechanism that made the click take effect. Six tests in
`crates/reprise-gnome/src/ui/playback/seek_start_tests.rs` pin it.

**This plan is a deliberate behaviour change to #736**, not a fix for an
accident. #736's goal — a waveform that responds before the first play —
survives; only its means changes. Say so in the commit message.

## The rule

> A waveform click never starts playback. It sets **where the next play will
> begin**, and moves the playhead and the time label there.

Decided in the grill, and it holds for every medium: local track, restored
podcast episode, queued episode. The click marks; the play starts.

Untouched, deliberately:

- **paused mid-session** (`Paused`) — already falls through to `self.seek()`;
  the pipeline is live, the seek works. This is the control arm.
- **playing** — untouched.
- **MPRIS / MCP `SetPosition`** — enters via `mpris_mirror.rs:421` →
  `try_seek_with_feedback`, bypassing `seek_or_start` entirely. Untouched, and
  therefore **worthless as a verification path for this change** — the MCP tools
  are read-only evidence here.

The keyboard seek (Arrow/Page/Home/End in the waveform,
`waveform_seek.rs:330`ff) and the compact player
(`player_controller_wiring.rs:240`) both route through the same `commit_seek` →
`seek_or_start`, so they inherit the rule. That is intended, not incidental.

## The trap that shapes the design

The naive fix — `self.seek(position_ms)` instead of the start — is wrong **and
would pass its tests**.

`Player::seek_to` (`crates/reprise-platform-linux/src/player.rs:470`) issues
`playbin.seek_simple(FLUSH | KEY_UNIT, …)`. A playbin in `NULL`/`READY` — where a
never-played restored track sits — does not accept a seek. `TestPlayback` in
`seek_start_tests.rs` implements `seek_to` as a `RefCell` push that always
succeeds, so the test goes green while the real app does nothing on the click.

**No task here is finished on fake-only evidence.** The `Stopped` path is
confirmed once in the running app (see Verification).

## Design: the mark is bound to its item

A bare `Cell<i64>` would survive a change of track and fire on an unrelated play
later. The sibling slot shows what that costs: `pending_local_seek` needs
`clear_pending_local_seek()` at five sites (`external_media.rs:211`,
`external_media.rs:539`, `now_playing_wiring.rs:422`, `player_controller.rs:616`,
`player_event_handling.rs:341`) — five places a future change can forget.

So the mark carries the item it belongs to:

```rust
pending_start_mark: Cell<Option<(QueueItem, i64)>>   // player_controller.rs, near :271
```

**It is `take()`n on every start attempt and applied only when the item
matches** — never left in place when the item differs. A mark for track A must
not fire when A comes back around later. Staleness heals itself; no new `clear_*`
call sites.

## Tasks

**1 — Add the slot.** `pending_start_mark` on `PlayerController`
(`player_controller.rs:257`ff, initialised at `:462`ff beside
`restored_placement_intact` and `pending_local_seek`).

**2 — `seek_or_start`: mark instead of start.** In the `Stopped` branch of
`seek_start.rs:13`, replace `start_current_item(...)` and the whole `match item`
block with: store `(item, position_ms)` in the slot, then move playhead and time
label via `sync_position(position_ms, duration_ms)` (duration from `now_playing`;
`sync_position` is `now_playing_wiring.rs:438` and drives both bars). Return.

`restored_start_change` / `restored_placement_intact` stays out of this function:
the click no longer starts, so the "don't re-centre on first play" decision
belongs to the play that eventually happens, where it already lives
(`queue_transport.rs:103`).

The final `self.seek(position_ms)` at `seek_start.rs:45` stays as it is. It now
carries two cases: the live `Paused`/`Playing` seek (the control arm) and
`Stopped` with no current item at all, where it is a harmless no-op against a
dead pipeline. That line is deliberately not changed.

**3 — Consume the mark for a local track.** Play path:
`shortcuts.rs:193` → `queue_transport.rs:309 toggle_pause` →
`ToggleAction::StartCurrent` → `queue_transport.rs:331 start_current_item` →
`up_next_transport.rs:116 present_track` → `player_controller.rs:689
start_track_for_lyrics` → `player.play(&path)`.

`player.play()` takes no position, so the mark is applied *after* the start,
through the mechanism that already exists: `start_pending_seek(position_ms)`
(`seek_start.rs:48`), which handles "seek too early, retry once after preroll"
via `ResumePolicy` and `retry_pending_local_seek`
(`player_event_handling.rs:198`).

**The consumption sites are the actual presentation funnels:** `present_track`
for local tracks and `prepare_external_playback` for episodes and radio.
`start_current_item` is only one caller; direct selection, up-next activation,
and automatic or gapless advance bypass it. Each presentation attempt takes the
mark unconditionally, applies it only when its item identity matches, and drops
it otherwise. A mark therefore cannot survive a different start and later fire
when the marked item returns (repeat-one, or a queue whose next entry is the
same track).

**4 — Restored episode: mark, don't resume.** `seek_restored_episode_at`
(`external_media_session.rs:199`) already writes the clicked position into the
session's resume target via `replace_podcast_resume_target`
(`external_media_session.rs:76` — sets `media.resume_ms` **and**
`session.resume`). Only the play must go: drop the `resume_restored_episode()`
call at `:211`, sync playhead and label, keep returning `true`.

Verified, so the task can be trusted:

- The later play consumes it — `toggle_external_pause`
  (`external_media.rs:478`) calls `resume_restored_episode()` first thing, which
  starts from the replaced `resume_ms`.
- The mid-session pause case cannot regress. `restored: true` is set only at
  restore (`external_media_session.rs:53`); once the episode really starts,
  `begin_podcast` builds a fresh session with `restored: false`
  (`external_media.rs:289`), so `restored_resume_request` returns `None` and the
  click takes the normal live-seek path.

**5 — Queued (not restored) episode.** Its start position comes from the
`resume_ms` field of `ExternalMedia::Podcast`, which `play_queued_episode`
(`external_media.rs:221`) builds itself via `media_from_episode(&episode)` at
`:233` before calling `play_external_with_context_and_origin`.

`play_queued_episode` enters the same `prepare_external_playback` funnel as
every other episode start. That funnel takes the item-bound mark and returns a
matching position to place in `resume_ms` before `begin_podcast`; a
non-matching mark is discarded before the external session begins.

Nothing is written to the database: a DB write would let a click near the end
mark the episode complete through `resume_rules::is_complete`, which is why the
grill rejected that route.

**6 — Rewrite the six tests** in `seek_start_tests.rs` (`:79`, `:121`, `:180`,
`:222`, `:286`, `:360`). Each currently asserts the old rule: `played_paths` /
`played_uris` non-empty plus `sought_positions == [30_000]`.

Both halves of the new shape are required, and the reason matters:

1. after the click — **zero** `play()` / `play_uri()` calls and an empty
   `sought_positions`;
2. then a `toggle_pause()` — and only now the play **and** the seek to `30_000`.

Half 1 alone passes vacuously if the setup stops reaching the
`Stopped`-with-current-item state, because `seek_or_start` then just falls
through to `self.seek()` and nothing plays either. Half 2 is what proves the
mark was actually stored and consumed. The preroll-retry tests (`:180`, `:222`)
keep their retry assertion, moved behind the play.

## What this deliberately does not do

The plan being amended,
`docs/plans/the-waveform-works-before-the-first-play.md`, has its own
"deliberately does not do" list. Three of its entries stay in force and are
repeated here because they are exactly the shortcuts this change invites:

- **No pre-roll at startup.** Loading the pipeline to `PAUSED` during restore
  would make a plain seek work without playing — and is the first thing that
  suggests itself once the click stops starting playback. It stays rejected:
  `session_player.rs` states the opposite invariant in its module doc and
  asserts it (`debug_assert!(!restore_should_start_playback())`), it would change
  the MPRIS status reported at launch, and it would open the audio device on
  every start. The mark exists precisely so no pre-roll is needed.
- **No change to the play-count heuristic.** `should_count_play`
  (`crates/reprise-core/src/library/stats.rs:244`) counts the furthest position
  reached, and `max_position_ms` is raised only from real position events
  (`player_event_handling.rs:190`). Setting the mark must not touch it —
  `sync_position` does not, and nothing added here may. A play started at the
  mark then counts exactly as "press play, then drag" counts today; a click that
  is never played counts nothing.
- **No restored playback position.** The session remembers the track, not where
  it was. The mark is deliberately session-local (grill decision): it dies with
  the app. Persisting it is a separate feature — resume for local tracks — with
  its own DB field and edge cases.

## Verification

**The loop.** The `seek_start` tests of `reprise-gnome`, run as CI runs them —
they carry `#[ignore = "requires a display"]`, so under `xvfb-run`. Red before
task 2, green after task 6.

**The real app** — because no test in that file can tell "seek worked" from
"seek was accepted by a fake". Both arms, same click sequence, driven through the
GUI (cua-driver) and read through the Reprise MCP tools:

| | after the click | after pressing play |
|---|---|---|
| **before arm** (current build) | `status: Playing` — today's bug | — |
| **after arm** (the change) | `status: Stopped`, `position: <clicked>` | `status: Playing`, position ≈ clicked |

The before arm is what makes the after arm mean something: without it,
`status: Stopped` is equally consistent with a click that missed the widget.
`position ≠ 0` is the discriminator that the click landed at all.

Then the control arm, which must behave exactly as it does today: play a track,
pause mid-session, click elsewhere in the waveform → position moves, still
paused.

## Parallelität

**No cut. One strand.**

Tasks 1–3 are a single compile: the slot's type (1), its only writer (2) and its
only reader (3) cannot land separately — a worktree with task 2 alone marks a
position nothing consumes, one with task 3 alone reads a field that does not
exist. Task 6 rewrites the assertions that tasks 2–5 change the behaviour of;
split off, it would be a strand whose tests are red by construction.

The one theoretically separable piece is task 4 (`external_media_session.rs`,
disjoint from `seek_start.rs` and `player_controller.rs`). It is not worth a
strand: two lines plus a playhead sync, and its test lives in the same
`seek_start_tests.rs` that tasks 2 and 6 own — `/code`'s disjointness check would
reject the pair on that file alone.

Cap is 3 strands; the honest answer here is 1.
