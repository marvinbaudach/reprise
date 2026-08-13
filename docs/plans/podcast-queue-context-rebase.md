---
slug: podcast-queue-context-rebase
worktree: ~/Projects/reprise-podcast-queue-context
branch: feature/podcast-queue-context
phase: planned
codex_session:
created: 2026-08-02
---
# Rebuild QUE-10 on the closure-free queue view model

This branch already implements QUE-10 (a directly-played episode shows its
frozen neighbour context instead of the music queue). It was written against
`VirtualContextTail`, which carried its rows in an `Rc<dyn Fn>` closure.
`origin/dev` has since replaced that type — commit `d0a5ac3d8b`,
"refactor(track-list): make the queue view model closure-free (P1a wave 0)" —
because a planned Android surface reaches `QueueViewModel` through UniFFI and
UniFFI cannot carry a closure. There is now a compile-time
`assert_send_sync::<QueueViewModel>()` guard that permanently forbids putting
one back.

**Merge `origin/dev` into this branch and re-express QUE-10 in the new design.
Do not reintroduce a closure anywhere in the view model, and do not weaken or
delete that assertion.**

## The new design, as it exists on dev

```rust
pub(crate) struct VirtualContext { count: usize, identity: Option<VirtualContextIdentity> } // data only
pub(crate) trait ContextWindow { fn rows(&self, offset: usize, limit: usize) -> Vec<i64>; }
impl QueueViewModel {
    pub(crate) fn items_window(&self, offset: usize, limit: usize, tail: &dyn ContextWindow) -> Vec<QueueItem>;
}
```

The GTK side supplies the rows through `QueueContextWindow`
(`track_list_model.rs`), which holds a `Weak<PlayerController>` and answers
from `player.queue.borrow().remaining_window(offset, limit)`. The list model
stores it beside the view model (`state.context_window`), never inside it.

## Why this makes QUE-10 *smaller*, not larger

The old branch materialised the episode neighbours into the model
(`VirtualContextTail::materialised`). That is no longer necessary or
desirable. The one place that already knows which context is playing is the
`ContextWindow` implementor, because it holds the controller:

- `ContextWindow::rows` returns `Vec<QueueItem>` instead of `Vec<i64>`, so a
  context tail can be episodes as well as tracks. `QueueContextWindow` wraps
  the music path's ids with `QueueItem::Track`; the trait stays closure-free,
  data-in/data-out, and remains implementable behind UniFFI.
- `QueueContextWindow::rows` picks its source from the playback mode: a direct
  episode session answers from the frozen POD-21 neighbour list
  (`neighbours.upcoming()`), everything else from `remaining_window`.
- `queue_view_model()`'s `PlaybackMode::Podcast` arm then only needs
  `VirtualContext::identified(count, sequence, start)` plus the episode as Now
  Playing and the show as the label. No materialised vector in the model at
  all.

Keep `items_window`'s existing boundary logic (materialised prefix first, then
the context window) exactly as dev wrote it — only the element type changes.

## What to preserve from this branch as it stands

All of it survives, mostly untouched:

- **QUE-10 and the QUE-2/7/9 amendments** in `docs/ux-rules.md`. `git merge`
  resolves this file automatically; verify the result reads correctly rather
  than trusting the auto-merge.
- **The interaction guards** (`queue_item_menu.rs`, `queue_row_mapping.rs`,
  `track_list_context_menu.rs`, `track_list_keyboard_reorder.rs`,
  `track_list_queue_menu.rs`): jumping to an episode context row plays that
  episode in the same frozen context; remove and reorder are not offered for
  those rows.
- **`refresh_on_model_change`** in `window_queue_model.rs` and its two tests.
  This one matters: an external-media change fires on every radio re-tag and
  every podcast pause, and re-rendering the Queue ColumnView for an unchanged
  model emits `items_changed` over an identical list, which resets the focused
  row to 0. Note that dev's `QueueViewModel` now derives `PartialEq` properly
  (it compares `context` as data), so the gate gets *more* accurate, not less.
- **The projection tests** in `queue_transport_projection.rs` and
  `external_media_state_queue_tests.rs`, adapted to the new constructor
  signatures.

## Conflicts to expect

`git merge origin/dev` conflicts in exactly four files:

1. `.superpowers/sdd/progress.md` — append-only ledger, keep **both** sides.
2. `crates/reprise-gnome/src/ui/track_list/queue_sections.rs` — take dev's
   structure wholesale, then apply the `Vec<QueueItem>` element-type change on
   top. Do not resurrect `VirtualContextTail`.
3. `crates/reprise-gnome/src/ui/playback/queue_transport.rs` — dev moved the
   tail construction; this branch moved the whole projection into
   `queue_transport_projection.rs`. Keep this branch's projection module and
   feed it dev's `VirtualContext::identified` call shape.
4. `crates/reprise-gnome/src/ui/track_list/track_list_model_tests.rs` — dev
   rewrote these around the trait; take dev's version and re-add only the
   episode cases this branch added.

## Verification

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
./scripts/tests/gettext-catalogs.sh
```

Two pre-existing reds are **not** yours and must not be "fixed":

- `podcasts::ytdlp::process_tests::missing_component_log_names_the_operation_without_exposing_its_path`
  fails only in a parallel run (shared log capture); it is green with
  `--test-threads=1` and a sibling of it is red on `dev` itself.
- `ui::now_playing::surface::tests::npp_13_cold_cover_resolves_before_the_outgoing_cover_fades`
  is red on `origin/dev` too, verified with a pristine cache directory.

Display-gated tests stay `#[ignore]`d; do not un-ignore them and do not claim
they ran.

## Out of scope

- Changing what Skip plays.
- Letting episodes into the container `QueueSnapshot`.
- Anything in `crates/reprise-gnome/src/ui/podcasts/` — that surface belongs to
  the pointer-selection branch, which merges separately.
