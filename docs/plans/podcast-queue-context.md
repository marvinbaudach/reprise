---
slug: podcast-queue-context
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-02
---
# Up Next shows what Skip actually does during direct episode playback

While a podcast / YouTube episode plays **directly from its source view**, both
queue surfaces show that episode's frozen neighbour context — the upcoming
episodes of the same show/channel — instead of the music queue that Skip is not
touching.

## The problem, precisely

Starting an episode by clicking it in the Podcasts/YouTube view gives the
session `PodcastOrigin::Direct` → `PlaybackMode::Podcast`.
`PlayerController::transport_next` (`playback/external_media_neighbours.rs`)
then tries `play_external_neighbour` **first**, which walks the frozen
`NeighbourContext` (POD-21) and returns `true` — so `queue_transport::next()`
is never reached, and it would bail out anyway (`if self.playback_mode() !=
PlaybackMode::Queue { return; }`).

Meanwhile `queue_view_model()` maps `PlaybackMode::Podcast` to
`now_playing = None` and keeps emitting the *music* snapshot tail. The right
Up Next panel therefore lists hundreds of tracks that Skip will not play, while
the thing Skip *will* play — the next episode — appears nowhere. That
contradiction is the whole bug.

## The decision (already taken — do not re-litigate)

**Show the episode context; do not hide the panel, and do not change what
playback does.** The neighbour chain already exists and is already frozen at
start; this task only makes it visible through the surfaces that claim to show
the future. Playback behaviour, POD-4's stop-at-the-end offer, and the
container queue in `reprise-core/src/queue.rs` all stay exactly as they are.

Further fixed points:

1. **The music queue is untouched data.** Nothing is cleared, consumed or
   reordered. This is a projection change in `queue_view_model()` only. When
   playback returns to `PlaybackMode::Queue`, the previous model reappears by
   construction because it was never modified.
2. **`PlaybackMode::QueuedEpisode` is already correct** — an episode played
   *from* the manual queue advances in queue order and the model already shows
   it. Do not touch that branch.
3. **Radio and Preview stay as they are** (`now_playing = None`, no context).
4. **The sidebar "Queue" counter keeps counting only the manual queue**
   (`sidebar_count()` reads the `PlayNext` section). Episode context rows are
   context, not manual entries — QUE-7 stays true as written.
5. **The manual Play Next line keeps being displayed** where it is today. It is
   the user's own stack and must not silently vanish. Accept the known
   imprecision that during *direct* episode playback Skip reaches the episode
   context before the manual line; that is a playback-semantics question and
   explicitly **out of scope here**. Record it as a one-line follow-up note in
   the plan's "Follow-ups" section of the final summary, not as code.

## Rules touched — this is a documented rule change, not an accident

- **QUE-2** names exactly two future sections ("Next in Queue", "Continuing
  from '<Album/Playlist>'").
- **QUE-7** describes the context tail as virtual and named.
- **QUE-9** says episodes "never enter the automatic 'Continuing from …'
  context" — meaning the **container queue** (`queue.rs` / `QueueSnapshot`),
  which this task still does not change.

Add a new rule (next free `QUE-` number, `[active] [gtk]`) stating: while an
episode plays directly from its source view, both queue surfaces render that
episode as Now Playing and its frozen POD-21 neighbour context as the named
context section, labelled with the show/channel; the manual queue and the
container snapshot are unchanged underneath and reappear unchanged when queue
playback resumes. Then amend QUE-9's sentence so it reads as being about the
container queue rather than about the rendering, and cross-reference the new
rule from QUE-2 and QUE-7. Keep the file's existing voice and formatting.

## Work

### Package A — let a context tail carry typed items

`queue_sections.rs`'s `VirtualContextTail` hard-codes tracks: its window
closure returns `Vec<i64>` and `items_window` maps them with
`QueueItem::Track`. Episodes cannot pass through that.

1. Change the closure type to `Rc<dyn Fn(usize, usize) -> Vec<QueueItem>>` and
   drop the `.map(QueueItem::Track)` in `items_window`.
2. Adjust the two construction sites: the music tail in
   `queue_transport::queue_view_model()` wraps
   `queue.remaining_window(offset, limit)` with `.map(QueueItem::Track)`; the
   `#[cfg(test)] compose()` helper does the same for its `up_next_rest: &[i64]`
   parameter, so its existing tests keep their current signature and meaning.
3. Add a `VirtualContextTail::materialised(items: Vec<QueueItem>, sequence,
   start)` constructor for a context that is already in memory (the neighbour
   list is), sharing the items through an `Rc<[QueueItem]>` so no clone happens
   per window request.

All existing `queue_sections` tests must keep passing unchanged in behaviour.

### Package B — expose the frozen neighbour context

In `playback/external_media_state.rs`, `NeighbourContext` already holds
`items: Vec<QueueItem>` and `index`. Add narrow, `pub(super)` accessors:

```rust
/// The items after the current one, in frozen show order.
pub(super) fn upcoming(&self) -> &[QueueItem]
/// Position of the current item — the stable `start` for the tail identity.
pub(super) fn position(&self) -> usize
```

Do not expose the whole struct outward and do not clone the vector in the
accessor.

The label ("VOID PREACHER", "Videos", whatever the show is called) must come
from data the session **already holds** if at all possible — check
`PodcastSession.media` / `subscription_id` and how the player bar already
renders the show name (`player_bar/player_bar_external.rs`). Only if no such
field exists, add one to `PodcastSession`, filled once when the session starts.
**Do not run a database query inside `queue_view_model()`** — it is called on
every queue change and must stay cheap.

### Package C — project it into the queue model

In `queue_transport::queue_view_model()`, the `PlaybackMode::Podcast` arm
becomes: if the current external session is a podcast session with
`origin == PodcastOrigin::Direct` and a `NeighbourContext`, then

- `now_playing = Some(neighbours.current_item())` (an `Episode`),
- `context = VirtualContextTail::materialised(neighbours.upcoming().to_vec(),
  sequence, neighbours.position())` when non-empty,
- `origin_label = Some(<show/channel name>)`,
- `play_next` stays the manual line, exactly as today.

The `sequence` must identify the frozen context so
`leading_removal_change_from` can still compute its O(1) delta when Skip
advances (count −1, start +1, same sequence). Derive it from something stable
for the session — e.g. `(subscription_id as u64, <session generation>)`. Do
**not** leave `identity: None`; that would force a full model swap on every
skip, and per the repo's own history a full `items_changed` resets the focused
row to 0 (see the table-jump root cause notes) — exactly the regression class
this project has already paid for once.

Without a neighbour context (single episode, nothing upcoming) the arm must
still yield `now_playing = Some(episode)` and no context section.

### Package D — refresh the surfaces when the episode changes

`window_queue_model.rs` rebuilds the shared model on `add_on_queue_changed`
only. An external-media change (episode start, skip, stop) does not currently
go through that callback, so the panel would show a stale context.

Wire the shared model to `add_on_external_changed` as well (rebuild the same
way), and make sure `window_now_playing_wiring.rs`'s panel refresh runs for
that path too, respecting its existing `is_up_next_visible()` early-out. Do not
introduce a second model or a second composition path — QUE-1/QUE-6 forbid it.

### Package E — interaction on the episode context rows

- **Jump** (double-click / panel jump): jumping to an episode context row plays
  that episode, staying in the same frozen context — reuse the existing
  neighbour playback path rather than starting a fresh session, so the
  remaining chain is preserved. `queue_row_mapping::QueueRow::UpNext(offset)`
  currently resolves against the music snapshot; route it to the episode
  context when the model's context is an episode context.
- **Remove and reorder are not offered** for episode context rows, matching how
  the virtual music tail behaves for entries that are not manual. If the
  existing code path would otherwise let a drag or a remove reach them, make it
  a no-op with a `tracing::debug!`, never a panic and never a silent partial
  edit of the neighbour list.

### Package F — proof

Pure tests (no display, must always run):

1. `queue_sections`: a materialised typed context yields the right
   `total_len`, `items_window` across the materialised/context boundary, and
   `sidebar_count()` still counts only the manual line.
2. `queue_sections`: `leading_removal_change_from` returns the expected O(1)
   delta between two episode-context models one skip apart.
3. `queue_transport`: with a fake podcast session in `Direct` origin and a
   three-episode neighbour context, `queue_view_model()` puts the current
   episode in Now Playing and the two upcoming episodes in a context section
   labelled with the show — and the music snapshot is *not* in the model.
4. `queue_transport`: switching back to `PlaybackMode::Queue` yields exactly
   the model it yielded before the episode played (the music queue is intact).
5. A `QueuedEpisode` session still produces today's model — a regression pin
   for decision 2.

Note the repo's own warning on fake backends: `FakePlayback` never emits
`AdvancedToNext`, so do not write a test that "proves" advancing through the
fake and claim it covers the real path. Test the model projection, which is
what this task changes.

## Verification

From inside the worktree:

```
cargo test -p reprise-gnome queue_sections
cargo test -p reprise-gnome queue_transport
cargo test -p reprise-gnome podcast
cargo test -p reprise-core up_next
cargo build -p reprise-gnome
cargo clippy -p reprise-gnome --all-targets -- -D warnings
```

Display-gated tests (`#[ignore = "requires a display; run via xvfb-run"]`) will
not run in the sandbox. Do not un-ignore them and do not report them as passed.

## Out of scope

- Changing what Skip plays (the manual-line-before-episode-context question).
- Making episodes enter the container queue / `QueueSnapshot`.
- Radio.
- The Escape/selection work — that is `docs/plans/podcast-escape-clears-selection.md`,
  a separate worktree. Do not touch `podcasts_selection.rs` or
  `youtube_channel_detail.rs` here.
