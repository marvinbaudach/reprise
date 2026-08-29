---
slug: the-podcast-list-asks-for-every-cover-at-once
worktree: /home/marvin/Projects/reprise-the-podcast-list-asks-for-every-cover-at-once
branch: feature/the-podcast-list-asks-for-every-cover-at-once
phase: planned
codex_session:
created: 2026-08-29
---
# The podcast list asks for every cover at once

Podcast covers in the subscription overview arrive slowly on a cold start —
staggered rather than as a list. Reported from use on 2026-08-29, then
measured. This is the follow-up the previous plan named:
`source-artwork-is-decoded-in-full-on-every-view.md` fixed the 221 ms
per-row decode and explicitly deferred what remained —

> **`StartupTiming::AfterQuiet` stays.** […] If the warm arm still shows a
> visible stagger, that is the next investigation, with its own evidence.

This is that evidence.

## What was measured

Two runs of the installed v0.1.84 binary with the harness that shipped with
the previous plan, `REPRISE_MEASURE_SOURCE_ARTWORK=1`. Phases are
`quiet_open`, `queued`, `worker_start`, `gtk_return`; every `wait_us` is
measured from queue entry, so the startup gate is **not** inside
`gtk_return`.

### Control arm: the cache is not the problem

The reported fix ("use a cache") is already in the product three times over
— URL-keyed originals on disk, content-addressed thumbnails on disk, and a
128-entry texture cache in memory. During the measured run:

```
files born after relaunch (real downloads):   0
files only touched after relaunch (hits):    66
```

Not one image came from the network. **Measure birth time (`stat %W`), not
mtime:** the LRU touches every hit, so `find -newermt` reports all 66 as
"new" and invites exactly the eviction story the previous plan already
disproved once.

### The arm that matches the report

The overview opened as the **first action** after launch:

```
quiet_open                                   1407 ms
gtk_return     p50 331 ms   p90 1136 ms   max 1411 ms
service time   p50 149 ms   p90  616 ms   max 1036 ms   sum 221 s
971 requests / 556 distinct rows / 922 (95.0 %) visible=false
```

A second run, with the overview opened some seconds after launch, gave the
same shape at smaller scale: `quiet_open` 1782 ms, 636 requests over 333
rows, 608 (95.6 %) invisible, `gtk_return` p90 636 ms.

Opening the view once issues **971 artwork requests for 11 subscriptions**.
With 266 episodes in the database, one render pass is 11 + 266 = 277 rows;
556 is almost exactly twice that, which suggests two passes rather than the
six an earlier reading of the row ids suggested. Not proven — see commit 2.

**The harness cannot attribute the gate, by construction.** `queued_at` is
set at submit, i.e. *after* the gate, there are no wall-clock timestamps,
and the adjacency of `quiet_open` to the first `queued` line proves nothing
— it appears in both runs, including the one where navigation happened
seconds later. Answering it needs a timestamp probe, which is why that probe
is the first step of commit 3 rather than a precondition for it.

**Provenance of the report:** the slowness was first noticed while watching
an automated click-through, and the user's own description is "recht
langsam, zumindest beim Kaltstart" rather than a stopwatch figure. The
measurements above are from the real desktop and stand on their own; the
plan does not lean on the subjective figure.

### What is *not* the cost

Ruled out rather than assumed:

- **Network.** Zero downloads, above.
- **`thumbnail()`'s full-original read and hash.** The previous plan
  pre-registered this as the suspected residue. Measured on the real cached
  originals: 397 KB reads at 0.06–0.31 ms, SHA-256 at 0.13–2.99 ms. It is
  real waste and it is not the cost.
- **Queue coalescing.** `submit_measured` deduplicates by URL, and
  `queued_jobs` counts distinct URLs only, so `jobs_ahead = 58` means 58
  distinct images, not 58 redundant requests.
- **Worker count.** `ARTWORK_WORKERS = 8` and the queue is unbounded.

## Why the queue fills with work nobody is looking at

The podcasts view is **not** a `ListView`/`ColumnView`. It is a plain
`gtk4::Box` inside a `ScrolledWindow` (`podcasts_list_surface.rs:22-28`) —
no `ListItemFactory`, no recycling. On top of that:

1. **Groups start collapsed.** `build_group()` builds a `gtk4::Expander`
   (`podcasts_groups.rs:160-175`) whose expanded state comes from
   `auto_expand_for_query`; with an empty query it is `false`, which
   `podcasts_groups_expansion_tests.rs:80` asserts directly.
2. **The episode rows behind that collapsed expander are built anyway**, and
   each one asks for its artwork in its constructor:
   `SourceImage::new_with_dimensions()` → `set_urls()` →
   `load_texture_chain()` all run before the widget is in the tree
   (`source_image.rs:257, 313-345`).

That is the 95 %: artwork for rows sitting behind a closed expander. It is
not a viewport problem and does not need viewport arithmetic to fix.

### The `visible` flag is noise at enqueue time, not a lever

It is tempting to sort the queue by the `visible` flag that is already
recorded. Do not. `visible_in_viewport()` needs `widget.root()` and
`compute_bounds()`, which fail before the widget is allocated
(`source_artwork_queue.rs:74-98`), and episode rows additionally call
`.visible_only_when_mapped()` (`podcasts_row_interaction.rs:27`), which
switches off the `retained_is_startup_visible` escape hatch. The flag is
*structurally false* at enqueue for precisely the rows that will become
visible. Sorting by it would be sorting by noise.

`map` is no substitute either: in a `Box` inside a `ScrolledWindow` every
child is mapped, including children scrolled far out of view.

## Commits

One branch, three commits, in this order. They are separate commits so each
carries its own before/after measurement — the previous plan's lesson about
attributability, applied.

### Commit 1 — a collapsed group asks for no artwork

The fix. An episode row inside a collapsed `Expander` must not submit an
artwork request; expanding the group makes its rows ask then.

Properties the implementation has to keep:

- **No cover may be lost.** A row whose request was skipped must ask when
  its group expands, and must not end up permanently blank. Collapsing and
  re-expanding must not lose the image either — the texture cache should
  make the second expansion instant.
- **Auto-expand still works.** `auto_expand_for_query`
  (`podcasts_presentation`) expands groups for a search query; those rows are
  visible and must request normally. The gate keys on the expander's actual
  state, not on the absence of a query.
- **The fallback chain is untouched.** `load_texture_chain` deliberately runs
  both stages so a show-level cover can appear while the episode-specific
  one loads, and `may_publish_artwork` (`source_image.rs:157-173`) keeps the
  better image from being overwritten by the worse one. Tests
  `src_11_episode_artwork_replaces_the_show_fallback` and
  `src_11_failed_episode_artwork_keeps_the_show_fallback` encode this. The
  new gate goes *in front of* the chain, never inside it.

### Commit 2 — count the render passes

Instrumentation only, no behaviour change. Behind the existing
`REPRISE_MEASURE_SOURCE_ARTWORK` flag, `podcasts_view::render()` and
`podcasts_groups::replace()` each print a pass number plus the group and row
counts they built.

**The number is not produced in this worktree.** It only exists when the GUI
runs on a desktop and someone navigates into Podcasts, so this commit ships
the counter and nothing else — do not attempt to report a value from a
headless run, and do not stall waiting for one. It is read from a later
desktop run.

**Do not implement the dedup fix here.** `replace()` tears down and rebuilds
every child unconditionally (`podcasts_groups.rs:112-137`, no diffing), and
556 rows against 277 rows per pass suggests two passes — but a rebuild that
is *correct* to perform (filter change, data change) must not be optimised
away, and after commit 1 a duplicated pass costs almost nothing. If the
counter later says more than one pass, that is a finding for its own plan,
with its own justification.

### Commit 3 — does the startup gate cost this path anything?

`quiet_open` is the largest single number in both runs (1782 ms / 1407 ms),
and the reported symptom is a cold start, which is exactly when the gate is
closed. It is also the number this plan cannot currently attribute.

**This commit is the probe, and only the probe.** Add a wall-clock timestamp
(or an explicit gate-state-at-request field) to the measurement lines, so it
becomes visible whether podcast artwork requests arrive at a closed gate or
an open one. The page is materialised lazily on sidebar selection
(`library_shell.rs:252`), so on a cold start this depends on how fast the
user clicks.

**Do not change gate behaviour here, and do not decide whether it should
change.** That decision needs the probe's output from a desktop run, which
a headless worktree cannot produce. Ship the probe; the follow-up is decided
afterwards, in its own plan.

For the record, so the follow-up is not re-derived from scratch: if the
requests turn out to arrive at an already-open gate, the gate costs this
path nothing and the right outcome is to document that and change nothing —
measuring an idea and discarding it is a successful result. If they do wait,
the fix is to let an explicit, user-initiated navigation past the gate via
the existing `StartupTiming::Immediate` path (`source_image.rs:454-455`),
never to shorten `QUIET_INTERVAL` for everyone.

Do not fold this into commit 1. If both land together, neither is
attributable.

## Out of scope, deliberately

- **A `visible`-priority queue.** The flag is structurally false at enqueue;
  see above. This is the most plausible-sounding wrong turn here, and it is
  refused on measured grounds, not on taste.
- **Dropping the second enqueue in `load_texture_chain`.** It looks like a
  free halving of 971 requests. It is not: `submit_measured` coalesces by
  URL before a job exists, so queue depth is unchanged, and the two stages
  are a deliberate progressive-enhancement design with tests.
- **The `<hash(url)>-<px>.png` sibling key** for `thumbnail()`'s read and
  hash, pre-registered by the previous plan. Measured at under 3 ms. Real
  waste, wrong sprint.
- **`CacheScope::entry_limit`.** Zero evictions measured — 0 files born
  during the run, 66 only touched. Untouched.
- **Viewport-driven enqueue.** Would additionally cover long *expanded*
  lists and subscription headers below the fold. Nothing measured says that
  remainder is a problem; revisit only if the closing check still shows a
  stagger after commit 1.
- **Android.** No measurement was taken on the device and this plan claims
  none.

## The regression test

Timing tests would be flaky and would not say why they regressed. Assert
mechanisms:

- **Commit 1:** rendering a group that is collapsed issues **zero** queue
  submissions for its episode rows; expanding it issues exactly one per row;
  a group expanded, collapsed and expanded again still shows its covers. A
  group auto-expanded by a search query submits normally.

Mutation probe, per the house rule: make the gate report every group as
expanded, confirm the test goes red, paste that output into the acceptance
section, discard the reversion. No `cfg(test)` switch in the production path.

## Control arm

The claim is a time, so the evidence is a time, measured the same way in
both arms, with the overview opened **as the first action after launch** —
the arm that matches the report.

1. **Control** — the branch point, unmodified.
2. **Commit 1** — same procedure.
3. **Second visit** — navigate away and back without relaunching, to confirm
   the first-visit win was not paid for with a return-visit regression.

Report the request count, the `visible=false` share, and per-row
`gtk_return` figures — not an average. The defect is a distribution; a mean
hides it, which is how a 149 ms median nearly hid behind a 331 ms one here.

**Known gap, do not paper over it:** every run behind this plan had a warm
Linux page cache, because the app was relaunched repeatedly. A genuine cold
start after boot reads originals and thumbnails from disk, so the 149 ms
median service time is a lower bound. The reported symptom is specifically a
cold start. If a cold-boot arm is available, take it; if not, say so rather
than presenting the warm figure as the cold one.

Closing check, because the symptom is visible rather than measurable: cold
start, open the overview, and watch the covers appear as a list rather than
as arrivals.

## Verification

The local gate list comes from `check-merge-readiness.sh`, never
hand-assembled.

## Parallelität

**No cut. One strand.**

- The three commits are sequential by construction. Commit 3 must be
  attributable against commit 1, which means it follows it in the same
  history rather than running beside it.
- Commits 1 and 3 both touch `source_image.rs` — commit 1 in front of
  `load_texture_chain`, commit 3 at the `StartupTiming` branch
  (`source_image.rs:454-455`). Not a disjoint file group.
- Commit 2 is instrumentation inside `podcasts_view.rs` and
  `podcasts_groups.rs`, the same files commit 1 changes.

A core/gnome split is not available either: everything is in
`crates/reprise-gnome/src/ui/podcasts/`, and `reprise-core` is not touched.
The one genuinely disjoint file group — `reprise-core/src/cover.rs` for the
sibling-key change — belongs to a task this plan explicitly puts out of
scope. Parallelising it would mean widening scope to manufacture a strand,
which is the wrong trade.

**Merge order:** n/a, single branch.
**Post-merge cross-checks:** none, no seam.
