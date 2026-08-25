# The frontend performance sweep — what each finding actually moved

Nine findings were read out of the three frontends in one pass on 24 August
2026; seven shipped, one was withdrawn and one was dropped. The plan they ran
under is `docs/plans/frontend-performance-sweep.md`, with a file per strand
(`-a` Android, `-b` GNOME, `-c` showroom).

The rule the sweep ran under was that **the big findings must produce a number**,
and a task whose measurement came back flat is reverted rather than shipped. This
page is where those numbers are kept. It follows the same discipline as
`index-rebuild.md`: a row carries a commit, a date and a method, and a finding
with no honest number does not get a row — it gets a paragraph saying so.

Two of the numbers below are **not** wall-clock savings and are not written as
if they were. Where the evidence is structural (work removed, an event that no
longer fires) the row says so in its own words.

## Results

| Finding | What | Before | After | Delta | Commit | Date | Method |
|---|---|---|---|---|---|---|---|
| A1 | Recompositions of the visible playing row over 20 position ticks | 21 | 1 | −95.2 % | `c8589bcc2f` | 2026-08-24 | `LibraryPositionRecompositionTest` counts compositions through `LibraryPerformanceObserver` while 20 position snapshots are published; mutation-probed in both directions |
| A3 | Artwork cache hits when a screen of 11 list rows is scrolled away and back | see below | 11 hits, 0 misses | every row retained | `c8589bcc2f` | 2026-08-24 | `ArtworkCacheTest.every_original_screen_row_hits_when_scrolling_back`; the before-value is derived from the pre-change capacity, not replayed — see below |
| B1 | View constructors running before first paint | 5 (≈216 ms, **debug build**) | 0 | −216 ms of pre-paint work | `347703242a` | 2026-08-24 | `docs/measurements/content-stack-startup.md` — five cold runs, env-gated tracing spans |
| B2 | Decodes for 15 tracks sharing one album cover | 0 hits, 15 misses, 15 textures | 14 hits, 1 miss, 1 texture | 14 decodes avoided | `347703242a` | 2026-08-24 | `docs/measurements/gnome-cover-cache.md` — hit/miss counters over a deterministic access trace |
| B2 | Decodes scrolling a 500-row list down and back, 40-row viewport | 36 097 hits, 783 misses | 36 136 hits, 744 misses | 39 repeat decodes avoided, −5.0 % misses | `347703242a` | 2026-08-24 | the same trace file, LRU against the former FIFO |
| B3 | `items_changed` for a one-row refresh of a 100-row list | 1 removal of 100 + 100 appends | 1 range: `(1, 1, 1)` | selection and scroll offset survive | `347703242a` | 2026-08-24 | `docs/measurements/gnome-list-deltas.md` — 100-row views under the isolated display harness |
| C1 | Layout events during pointer parallax | 41 | 0 | −100 %; style + layout −17.6 % | `32610bfc48` | 2026-08-24 | Chrome DevTools performance trace over a pointer sweep — **see the provenance note below** |

## What is behind each number

**A1 — the position tick stopped driving the library.** The library screen read
the whole playback state, so every position update recomposed it. The playing
row's identity is now split out of the tick into its own state. The test
publishes one initial snapshot and then 20 position-only snapshots; before the
change each of those recomposed the visible playing row, so the count was
1 + 20; after it, the count is the pre-tick count and the mini player still
redraws its progress from the same ticks. The probe cuts both ways: reverting
the split, and reading the tick again, are each caught.

**A3 — the artwork cache is sized by surface.** Before the change one shared LRU
of 12 entries served list rows, the now-playing cover and artist portraits
alike; a screen of 11 rows plus one screen of scroll plus the playing cover is
23 entries through 12 slots, so nothing from the first screen could survive to
be scrolled back to. The budgets are now per surface —
`LIST_ARTWORK_CAPACITY = 32`, `NOW_PLAYING_ARTWORK_CAPACITY = 3`,
`ARTIST_DETAIL_ARTWORK_CAPACITY = 1` — and the pinned test asserts that
scrolling back re-hits all 11 rows and misses none.

**B1 — the content stack builds what is looked at.** Five view constructors ran
before first paint for views nobody had opened. They are built on first sight
now, the way the preferences dialog already did it.

**B2 — the cover cache is keyed by the file, with an LRU.** It was keyed per
track, so an album's fifteen tracks decoded the same cover fifteen times, and
its FIFO eviction dropped exactly the entries a scroll-back needed. The second
trace also shows what the eviction policy alone is worth: a hit refreshes an LRU
entry, while FIFO leaves its age unchanged.

**B3 — the other list models got the delta the track list already had.**
`remove_all()` plus an append per row is `items_changed(0, old, new)`: every
widget rebound, selection gone, scroll offset gone, every cover re-requested.
Radio's deltas are keyed by `row.id`, which is what makes a single edited
station emit `(1, 1, 1)` plus `(2, 0, 1)` instead of one coarse `(1, 1, 2)`
splice that would take a neighbour with it.

**C1 — pointer parallax stopped driving page choreography.** A pointer move
called `schedule()`, which ran the full page-level pass including its layout
reads. Pointer movement now owns a frame that can do nothing but move the oil
layer, and the listener is not attached at all when motion is reduced.

## Provenance, stated rather than smoothed over

**The B1 figures were measured in a debug build.** Stats 97.2 ms, Podcasts
43.2 ms, YouTube 36.6 ms, Radio 24.8 ms and Concerts 14.1 ms — about 216 ms
together — come from `target/debug/reprise`; no release artefact existed in the
measurement worktree. Release figures will be materially lower. What is proven
is the structural claim, that those five constructors no longer run before first
paint, not a millisecond saving for a user. An earlier framing of this work as a
"−51.8 %" startup improvement was wrong and was removed before the strand
landed; `content-stack-startup.md` carries the full wall-clock context and says
why its two five-run samples must not be read as a halved cold start.

**C1's trace was not archived.** The 41 → 0 layout events and the −17.6 % style
and layout time were read from a Chrome DevTools performance trace during the
strand run; the trace file itself was not committed. What the repository holds
is `showroom/tests/backdrop-design.test.mjs`, and that test asserts the *source
shape* — that the pointer handler schedules its own frame, that the frame calls
`moveOil()` and nothing else, that the listener is skipped when motion is
reduced, and that both frames are cancelled on teardown. That is a real guard
against the regression, but it is a different kind of claim from the trace, and
this is the weakest provenance on the page.

**A3's before-value is derived from the budget, not replayed on the old code.**
The pre-change cache had no hit/miss counters — they were added by this very
change — so the "before" cannot be read off the old implementation directly. The
capacity arithmetic above (23 entries through 12 shared slots) is what it rests
on, and it is the whole of it: no replay against the old implementation was
made. The new cache keeps one LRU per surface, so it cannot reproduce the old
sharing behaviour by lowering a budget either — a faithful replay would have to
run the pre-change `ArtworkCache` itself.

## The two findings that did not ship

**A2 was withdrawn: the diagnosis was wrong.** The finding assumed composables
were being recomposed that strong skipping would otherwise skip. Strong skipping
is on (Kotlin 2.4.10) and all 24 composables in the surface are already
skippable, so the proposed change would have moved nothing.
`docs/plans/frontend-performance-sweep-a.md` records it in full rather than
quietly dropping it.

**C2 was dropped by the plan's own rule.** Lazy-loading the lightbox saved
0.26 kB gzipped. The rule says a task whose measurement comes back flat is
reverted, and 0.26 kB is flat; a code path is not worth that.

**C3 shipped without a number, and was exempt from the rule.** Hoisting
`getContext` out of a frame loop is hygiene. The mother plan names C2 and C3 as
hygiene and exempts them from the measurement rule deliberately — demanding a
measurement campaign for it would be theatre, and a fabricated number would be
worse than none.
