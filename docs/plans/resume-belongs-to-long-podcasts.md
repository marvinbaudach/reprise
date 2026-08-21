---
slug: resume-belongs-to-long-podcasts
worktree: /home/marvin/Projects/reprise-resume-belongs-to-long-podcasts
branch: feature/resume-belongs-to-long-podcasts
phase: shipped
codex_session:
created: 2026-08-21
---

# Resume belongs to long podcast episodes

The trigger was a YouTube episode that had played for an hour, after which its
row still showed "Resume 4%" when the user moved to the next item. Investigation
found two separate defects and one incorrect product assumption. This plan
corrects all three.

## Findings

### Persistence works

`handle_external_position`
(`crates/reprise-gnome/src/ui/playback/external_media_position.rs`) stores
`position_ms` every five seconds, as well as when leaving the session. The
production database measured before implementation contained, for example:

```text
id 171  2161438 ms / 3673 s = 58.8%
id  65  2630852 ms / 3689 s = 71.3%
id  91  3707447 ms / 3725 s = 99.5%
```

### The row is a snapshot

The status chip is created from the in-memory `EpisodeRow` when a row is built.
That cache was reloaded only when entering the view. While playback continued,
the only targeted row patch handled the Played state. There was no equivalent
position-update path, so a row stayed stale until the user left and re-entered
the view. No production row actually contained a four-percent position, which
confirmed that this was stale UI rather than stale persistence.

### Resume should not apply to every source

A resume point is useful for a long spoken podcast, but it makes a misleading
claim for a music mix on YouTube. Very short spoken episodes also cost less to
restart than to reason about as resumable sessions.

## Decisions

### E1: Resume is an RSS podcast feature

`PodcastKind::Youtube` keeps no resume position: no write, read, or chip. Played
state remains unchanged for YouTube and continues to mean "seen", not "resume
from here". A partly watched video may therefore look unstarted after the user
leaves it; that is intentional, and no replacement "Started" chip is added.

### E2: Resume starts at ten minutes

Episodes shorter than `MIN_RESUME_DURATION_SECS` (600 seconds) keep no resume
position. Unknown duration counts as long, because a long episode without
`itunes:duration` must not lose its place. At the time of investigation, this
excluded 16 of 80 RSS episodes in the production database.

### E3: Almost finished means Played

When the user leaves an episode, it becomes Played if fewer than 60 seconds
remain or at least 97 percent has been heard. The more generous condition wins.
This uses the live session position and applies to every episode, including
short RSS episodes and YouTube. Only position storage is restricted by E1 and
E2.

This leave-time completion path does not show a next-episode offer. The offer
and persistent player-bar action remain exclusive to natural stream completion.

### E4: Rows update live, but only when their chip changes

The player announces an eligible persisted position on the existing five-second
checkpoint. The source view always patches its retained row data, but rebuilds
the status chip only when the displayed value changes.

#### Correction from 2026-08-21

The comparison must not use `source_row::resume_percent` alone. That helper
clamps a known-duration result to `1..=99`, so position zero already returns
`Some(1)`. It also returns `None` for every unknown duration. A percent-only
comparison would therefore delay a new Resume chip for roughly 40 seconds on a
45-minute episode and would never move an unknown-duration episode from New to
Resume.

The comparison uses a display key of `Option<Option<u8>>` instead:

- the outer option distinguishes New from Resume;
- the inner option carries the percentage when duration is known.

The row rebuilds when that complete key differs and remains untouched when the
key is equal. A 45-minute episode therefore causes one New-to-Resume transition
plus at most 99 percentage changes, rather than 540 five-second rebuilds.

### E5: Pausing is not leaving

Pause must checkpoint an eligible resume position but must never mark an
episode Played. Stop, source switch, queue hand-off, another episode starting,
and quit are leave paths and run the completion decision.

## Consequences

1. A source row is at most one five-second checkpoint behind. A leave-time
   Played transition has its own required `notify_episode_played` signal.
2. Pause neither loses the current position nor completes the episode.
3. When duration first becomes known and reveals an episode shorter than ten
   minutes, the previously stored position is cleared and the row is notified
   immediately.

## Implementation tasks

### Task 1: Pure Core policy

Add `crates/reprise-core/src/podcasts/resume_rules.rs` as the sole home of:

```rust
pub const MIN_RESUME_DURATION_SECS: i64 = 600;
pub const COMPLETE_TAIL_MS: i64 = 60_000;
pub const COMPLETE_PERCENT: i64 = 97;

pub fn keeps_resume(kind: PodcastKind, duration_secs: Option<i64>) -> bool;
pub fn is_complete(position_ms: i64, duration_secs: Option<i64>) -> bool;
```

Tests cover the exact duration, tail, and percentage boundaries, short total
duration, YouTube, negative positions, and positions beyond the duration. No
caller duplicates these constants or predicates.

### Task 2: GTK write path

In `external_media_position.rs`:

- `handle_external_position` saves only when `keeps_resume` is true;
- `checkpoint_external_position` is pause-only and never marks Played;
- `persist_external_position` first marks complete episodes Played and notifies
  the views, otherwise saves only eligible resume positions;
- no leave-time completion path shows the next-episode offer;
- a newly known short duration clears the stored position and notifies the row.

The source-level regression that couples every `store::mark_played` to
`notify_episode_played` covers both completion source files. A separate test
pins pause to the checkpoint path.

### Task 3: GTK read path

`external_media_toast.rs` is the single episode-to-media funnel and exposes a
stored resume position only when `keeps_resume` permits it. Cold-start session
restoration applies the same rule to media state, position, last-persisted
position, and pending seek. `podcasts_presentation::status::derive` remains a
pure two-field derivation; the migration in Task 4 makes old rows satisfy the
new invariant.

### Task 4: Migration v78

Add a version-gated, transactional data migration that clears non-zero
positions for every YouTube subscription and every episode whose known
duration is below `MIN_RESUME_DURATION_SECS`. Its test seeds both cleanup arms
and a long RSS control row whose position must survive.

### Task 5: Live row updates

- The player exposes `add_on_episode_position` and
  `notify_episode_position`; callbacks are cloned before invocation so no
  `RefCell` borrow survives re-entrant view code.
- A successful eligible five-second persistence checkpoint emits the signal.
- `source_views::wire_episode_position` forwards it to both source views and
  does not refresh the sidebar.
- Both the main podcast view and YouTube channel detail patch their in-memory
  rows and rebuild only when the complete `Option<Option<u8>>` display key
  changes.

The GTK coverage is display-free: source-level wiring checks and pure row-data
tests construct no GTK widget.

### Task 6: UX contracts

Update the existing `POD-1`, `POD-24`, and `SRC-16` rules without adding a new
ID:

- `POD-1` scopes Resume to eligible RSS episodes and records source-independent
  leave-time completion without a next-episode offer;
- `POD-24` scopes saved-position start and persistence to RSS while preserving
  automatic YouTube continuation after natural completion;
- `SRC-16` states that only RSS rows display Resume states.

## Verification

The implementation is verified with:

1. focused Core resume-policy edge tests;
2. the v78 migration test with its long-RSS control arm;
3. the ordinary `cargo test -p reprise-gnome` suite without a display;
4. a mutation that suppresses `notify_episode_position`, which must make the
   wiring test fail before being reverted;
5. a mutation that forces every position patch to rebuild, which must make the
   sparsity test fail before being reverted;
6. the complete display-free workspace gate, strict Clippy, formatting, audit,
   UX traceability, and documentation checks.

No display harness and no production database are used in this worktree. The
manual product check remains deferred: against a complete copy of the database
including `-wal` and `-shm`, confirm that YouTube has no Resume chips and that a
long RSS episode gains a Resume chip within five seconds without leaving the
view.

## Sequencing

The work is intentionally sequential. Tasks 2, 3, and 5 share the external
media playback sources, while Task 5 extends the same notification pattern
Task 2 uses. Task 1 precedes all callers, Task 4 precedes the read-path
invariant in Task 3, and Task 6 records the behavior implemented by Tasks 1–5.
