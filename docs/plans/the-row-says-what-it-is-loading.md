---
slug: the-row-says-what-it-is-loading
worktree: /home/marvin/Projects/reprise-the-row-says-what-it-is-loading
branch: feature/the-row-says-what-it-is-loading
phase: refactored
codex_session:
created: 2026-08-28
---
# The row says what it is loading

A freshly added YouTube channel stops being a dead line in the list. Its own row
carries the work: three named steps, a live episode count, a cover slot that
admits it has no cover yet, and a Cancel that genuinely cancels.

Design source: `Ladescreen YouTube-Kanäle.dc.html`, variant **1c** ("Schritte —
die Zeile erzählt, woran sie gerade arbeitet"), Claude Design project
`484033b9-02a4-439b-923e-67e46fceb560`.

## What is actually true today

The visible symptom — a new channel sits at "0 episodes · 0 new" with a
placeholder icon while only the footer says "Refreshing podcasts…" — is the
surface of four separate gaps. None of them is a rendering bug.

- **Adding one channel force-refreshes the whole library.** The add dialog's
  `on_added` closure calls `view.request_refresh(true)`
  (`podcasts_view_actions.rs:474`) after `subscribe()` inserts the row
  (`add_dialog_subscription.rs:49-77`). There is no per-subscription refresh call
  anywhere in that path. This is a defect in its own right: subscribing to one
  channel re-reads every feed the user owns.
- **There is no progress signal to bind a row to.** `podcasts::pipeline::refresh()`
  / `refresh_to_root()` (`crates/reprise-core/src/podcasts/pipeline.rs:244-279`)
  are blocking functions that return one `RefreshSummary` (`:200-209`) after the
  whole operation completes. The only progress callback in the podcast pipeline is
  `DownloadProgress` (`download_state.rs`), which reports **byte progress of an
  episode file download** — not feed discovery. `PodcastsWorkerResult`
  (`podcasts_worker.rs:97-107`) has no partial variant.
- **There is no per-source state.** Neither `SourceGroup`
  (`crates/reprise-core/src/podcasts.rs:104-112`) nor `SourceSummary`
  (`podcasts_presentation.rs:79-89`) carries a refresh flag. `podcasts_row_state.rs`
  looks like the right place by its name but governs **episode** rows
  (`RowNetworkState`, `:12-15`). Refresh state is global: one `generation: u64`
  counter and one footer spinner. `refresh_button_state(in_flight)`
  (`podcasts_refresh_decision.rs:86-92`) counts *due* subscriptions for the toolbar
  button's look; it is not a per-row signal.
- **There is no cancellation.** The `generation` counter makes the UI *discard a
  stale response* (`podcasts_view_requests.rs:58-60`); the worker's `refresh()`
  runs to completion regardless and its DB writes land either way.

The row itself is well-factored and needs no restructuring:

- `group_header_with_rebind()` (`podcasts_groups.rs:264-340`) builds the header
  from the shared `source_row::skeleton()` — `{root, media, identity, trailing}`.
  Cover is **40×40** inside a fixed **64×40** media slot (`MediaShape::SourceSquare`,
  `source_row/media_column.rs:7-38`), not the design's 48×48. `ROW_MIN_HEIGHT = 56`
  (`source_row/skeleton.rs:5-57`).
- The meta line is a single `caption dim-label` appended to `skeleton.trailing`
  (`podcasts_groups.rs:320-330`), fed by `strings::podcast_group_facts()`
  (`strings_podcasts.rs:359-370`).
- The disclosure triangle is the outer `gtk4::Expander` in `build_group()`
  (`podcasts_groups.rs:156-252`); the header is its label widget (`:192`).
- Podcast CSS is a raw string in `podcasts/css.rs`, already registered at
  `style/mod.rs:138-139`. Accent lives in `style/accent.rs` (`APP_ACCENT`), the
  reduced-motion gate in `ui/motion.rs`.

## Decisions taken

Recorded here because three of them change binding rules, and the rulebook's
process rules require the amendment in the same commit as the behaviour.

1. **Variant 1c is built as designed, and FB-9 is amended.** The three-step list
   takes the row from 56 px to ~110 px, which is verbatim what FB-9's second
   prohibition names ("a growing detail list"). The amendment is narrow: a row
   *in its initial sync* may carry a taller state, and it returns to the normal
   height by an animated 250 ms shrink, never by a jump. This is the whole point
   of the change — a row that cannot grow cannot tell you what it is doing.
2. **The row wins, the footer goes quiet.** FB-9 forbids reporting one task twice
   in the same window. While at least one initial sync owns a row, the footer does
   not name that job. A concurrent whole-library refresh still uses the footer.
3. **Cancel gets a real abort token.** FB-10 states the project's position:
   "Cancel is offered only where it genuinely cancels." The token is threaded
   through the pipeline and checked between feed items.
4. **The Preferences background bar disappears when idle** (user decision,
   2026-08-28). SET-18's wording is amended. Note this is likely *more* FB-9
   compliant than today, not less: FB-9's third prohibition is "never leave an
   empty placeholder that occupies area without saying anything", and today an
   idle bar collapses to a bare "Background activity" title that says nothing
   (`preference_background_bar.rs:258-272`).
5. **Scope is Podcasts/YouTube now, the general rule is `[planned]`.** The
   inventory below is real but is not built in this round.

## Design

### 1. A per-source initial sync, with its own progress channel

New in `crates/reprise-core/src/podcasts/`: a sync that targets **one**
`subscription_id` and reports as it goes.

```
pub enum SyncProgress {
    Started,
    FeedRead { episodes_found: usize },   // emitted as items are parsed
    FetchingArtwork,
    Done(RefreshSummary),
    Failed(SyncError),
}
```

- The existing `refresh_to_root()` body is factored so the per-source path shares
  the feed read rather than duplicating it. `refresh()` keeps its current
  signature; the progress callback and the abort token are threaded through the
  shared inner function with no-op defaults for the existing callers.
- **Abort token**: an `Arc<AtomicBool>` (or the crate's existing cancellation type
  if one exists — check before adding a second) checked *between feed items* and
  before the artwork fetch. On abort the function returns without committing.
  Verify the store path: an abort after episodes are already written must not
  leave rows for a subscription the user has since removed.

### 2. Worker and view carry it per subscription

- `PodcastsWorkerResult` (`podcasts_worker.rs:97-107`) gains a variant carrying
  `{ subscription_id, SyncProgress }`.
- `podcasts_view.rs` gains a map `syncing: RefCell<HashMap<i64, SyncRowState>>`
  beside the existing `generation` counter, where

  ```
  struct SyncRowState { step: SyncStep, episodes_found: usize, error: Option<String>, abort: AbortHandle }
  enum SyncStep { Added, ReadingFeed, DownloadingArtwork, Failed }
  ```

- `add_dialog_subscription`'s `on_added` path (`podcasts_view_actions.rs:463-474`)
  stops calling `request_refresh(true)` and starts the scoped sync instead.
- Footer suppression (decision 2) reads this map: `podcasts_view_requests.rs:46-50`
  skips `PODCAST_REFRESHING` while the map is non-empty and the request is the
  initial sync.

### 3. The row renders the state

In `group_header_with_rebind()` (`podcasts_groups.rs:264-340`), the identity box
gains a second child — a progress stack — and the trailing meta label gains a
sibling Cancel/Retry button. Both live in a `gtk4::Stack` per FB-9's crossfade,
not in a conditional rebuild, so the row never blinks.

- **Three steps, always three lines**, so the height is stable *during* the sync:
  done (check, accent, dimmed text) → active (spinner, full text colour, live
  count) → pending (grey dot, strongly dimmed).
- **Cover slot** (the existing 40×40 `SourceSquare`, not 48×48): dimmed video icon
  in frame colour, slow pulse, plus a horizontal shimmer sweep (~1.9 s linear,
  accent at ~14 %). Artwork crossfades in over 200 ms when it lands.
- **Row chrome**: 1 px accent border at ~22 %, and a top-down gradient from accent
  ~6 % to transparent. Both from `style/accent.rs`, no new colour literals.
- **Not expandable while syncing**: `expander.set_expanded(false)` and the
  disclosure rendered insensitive in `build_group()` (`podcasts_groups.rs:156-252`).
- **Completion**: progress stack and border fade out over 250 ms while the meta
  line fades in, then the row shrinks to `ROW_MIN_HEIGHT` on the same 250 ms
  curve — animated, never a jump.
- **Failure**: the layout stays. The active step becomes an error mark with
  "Couldn't read feed"; Cancel becomes Retry.
- **Several channels each get their own row.** The map is keyed by
  `subscription_id`; nothing is aggregated.
- **Reduced motion**: no spinner, no shimmer, no pulse, no shrink animation —
  static state icons and an immediate swap. Use the existing central gate in
  `ui/motion.rs`; do not add a second check.

### 4. CSS

New classes into `podcasts/css.rs` (registered already at `style/mod.rs:138-139`).
Keyframes translate from the design as: `shimmer` 1.9 s linear, `spin` 0.9 s
linear, `breathe` 2 s ease-in-out. All wrapped by the reduced-motion gate.

### 5. Preferences background bar (decision 4)

`preference_background_bar.rs`: `bar_state()` (`:97-106`) gains an "idle" result
that hides the whole footer, not just its rows; the render path (`:258-272`)
applies it to the root. The gate-off notice
(`BACKGROUND_NO_ONLINE_JOBS`, `strings_online_sources.rs:97`) only appears when
the gate is off *and* the dialog would otherwise have shown activity.

Two existing tests assert today's behaviour and are rewritten with the rule:
`nothing_running_shows_no_badge_and_no_notice` and
`the_gate_being_off_replaces_every_row_with_one_reason`
(`preference_background_bar_tests.rs:43,52`).

## Rule changes

Next free IDs verified against `docs/ux-rules.md`: **FB-12**, **POD-26**.

- **FB-12** `[planned]` `[gtk]` — *Every wait that the user started has a visible
  owner that names what is being loaded.* When the wait belongs to one row, card
  or entry, that element carries it (FB-9 (3)); a global status line is the
  fallback for work that belongs to no single element, never the primary answer.
  A placeholder that shows a resting value while its real value is still being
  fetched is prohibited: "0 episodes" while the feed is being read is a false
  statement, not a neutral default. Binds all future network-backed lists.
- **POD-26** `[active]` `[gtk]` — the concrete behaviour of §3 above, with a
  rule-named test (`fn pod_26_…`).
- **FB-9** — amended with the initial-sync exception from decision 1.
- **SET-18** — amended per decision 4.

`scripts/check-ux-traceability.sh` is a merge gate: `[active]` rules need ≥ 1
rule-named test in the same commit.

## Tasks

1. Core: factor the shared feed read; add `SyncProgress`, the abort token, and the
   per-subscription entry point. Unit tests in `reprise-core`.
2. Worker: new `PodcastsWorkerResult` variant; scoped request; abort plumbing.
3. View: `syncing` map, add-path rewiring (drop `request_refresh(true)`), footer
   suppression.
4. Row: progress stack, cover shimmer, accent chrome, expander lock, completion
   crossfade + shrink, failure state, Cancel/Retry.
5. CSS + reduced-motion gate.
6. Strings: new constants in `strings_podcasts.rs`, `N_!` wrapped; `po/reprise.pot`
   regenerated.
7. Preferences background bar + its two rewritten tests.
8. `docs/ux-rules.md`: FB-12, POD-26, FB-9 amendment, SET-18 amendment.

## Verification

- `reprise-core` unit tests for `SyncProgress` ordering and for abort: an aborted
  sync commits no episodes and leaves no rows for a removed subscription.
- `[gtk]` display test `pod_26_…` for the row states, run via
  `scripts/check-display-tests.sh --rule-named`.
- A row-height test: loading height is stable across all three steps and across
  the failure state (the failure must not add a fourth line).
- `scripts/check-ux-traceability.sh` green.
- Control arm before believing any gate result: `dev` gates are known to run red
  independently of this change.

## Out of scope — the inventory for FB-12's later rounds

Surveyed 2026-08-28, deliberately not touched:

- **Radio add-dialog results** (`radio/add_dialog_rows.rs:29`) — one shared spinner
  for the whole result list (`radio/add_dialog.rs:149,191,640`), no per-candidate
  state. `StationCandidate` has no in-flight field.
- **Updates feed / New Releases** (`updates/release_row.rs:97`, shared builder
  `updates/feed_row.rs:48`) — `LazyReleaseCover` (`updates/release_cover.rs`) is
  **deliberately spinner-free**, showing an initials tile immediately. That is an
  existing design decision, not an oversight; FB-12 must be reconciled with it
  rather than overriding it silently.
- **Concerts** — `ConcertsProgress { checked, total }`
  (`concerts/concerts_worker.rs:26`) already exists as a count signal and is
  consumed only as an aggregate. Cheapest future win.
- Not applicable: `first_run_sources.rs:35` (one global switch, no fetch),
  `device_sync/` (local MTP), `online_discovery_banner.rs`,
  `artwork_consent_banner.rs` (one-shot banners).

No shared row builder spans Radio and Updates, so a single generalized loading-row
widget is not feasible today; FB-12 binds the behaviour, not one implementation.

## Parallelität

Task 7 (Preferences background bar) touches no file that tasks 1–6 touch and can
be split into its own commit or worktree if the diff gets unwieldy.
