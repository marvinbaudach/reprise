# What the sidebar costs, and what the app costs while doing nothing

Measured 4 August 2026 on the `perf/idle-frame-clock` branch, against a copy of
the real library (1847 tracks) in an isolated instance on `:99`.

The question this started from was "why do sidebar switches feel expensive?".
The answer is that mostly they are not — but the app never stops working, so
there is no idle for them to be cheap *against*.

## The headline

| while nothing is asked of the app | with the paused breath | with it disabled |
| --- | --- | --- |
| main-thread CPU | **123 ms/s** (later sample 158) | **4 ms/s** (later 6) |
| frames painted | **60/s** | **0/s** |
| main thread busy | 14–17 % | 0–1 % |
| what asks for the frames | `cover_bloom`, every single frame | nothing |

Same binary, same library, same instance shape; the only difference is
`REPRISE_EXPERIMENT_NO_BREATH=1`, which skips installing the breath's tick
callback. Both figures are taken with no probe attached — the accessibility
round trips used for the switch measurements are themselves main-thread work
and would otherwise be billed to the app.

**The paused breath is not a share of the idle cost. It is the idle cost**, by a
factor of about thirty. Without it the app genuinely idles: zero frames, one
percent of a thread. With it, an eighth of a core, forever.

The state this happens in is the ordinary one: a track loaded, not playing, the
now-playing panel open (`ui.info_panel_visible=1` in the library). At startup
that is exactly what the app restores — in the test instance, *Shadow of Intent
— Infinity Of Horrors* at 0:00, paused.

## Why sixty, when the effect wants thirty

`cover_bloom.rs` is explicit about its own rate:

```rust
/// Redraw interval (µs) of the paused breath. A six-second sine does not need
/// sixty frames a second; the slow envelope only needs this tick as a clock.
const BREATH_FRAME_INTERVAL_US: i64 = 33_000;
```

The redraw is indeed throttled to 33 ms. The *frame clock* is not: the tick
callback returns `ControlFlow::Continue` on every frame, and a live tick
callback is a standing request for the next frame. So the clock runs at the
display rate and the throttle only decides which of those frames also repaint.
The census counts both and they disagree exactly as predicted: 60 paints/s,
`cover_bloom=1200` over 20 s — the callback runs 60 times a second, the drawing
happens half as often, and the other half is pure clock.

That is the whole mechanism. Nothing else in the app contributes: with the
breath off, the census reports no ticks *and* no animations, and the window
paints zero times a second.

## Where that cost actually sits

The obvious repair — drive the breath from a timer so the clock can idle
between redraws — was built and measured, and it is **worse**: half the frames
(30/s instead of 60) and *more* CPU (131–138 ms/s against 91–101). So the
running clock was never the expense. Taking the cost apart says where it is.

Four states, each in its own fresh instance, the whole order run forwards and
then backwards — the spread between instances is as wide as the differences
being looked for, so a result only counts if it survives both directions:

| state | paints/s | CPU forwards | CPU backwards |
| --- | --- | --- | --- |
| the breath as it ships | 60 | 172 ms/s | 102 ms/s |
| tick runs, does nothing else | 60 | **10 ms/s** | **10 ms/s** |
| area invalidated, draw function paints nothing | 54–60 | 115 ms/s | 103 ms/s |
| no breath | 0 | 3 ms/s | 4 ms/s |

- **A running frame clock is nearly free: 10 ms/s.** It still counts 60
  "paints", which is how the paint counter has to be read — `after-paint` marks
  a frame-clock cycle, not a re-render.
- **The Cairo painting is nearly free too.** With the draw function returning
  immediately, the cost stays at 103–115 ms/s — as high as the real thing.
- **What costs is the invalidation.** Marking that area dirty every 33 ms makes
  GTK re-render and recomposite, and that is ~95 ms/s of the ~100.

So neither "run the clock less often" nor "draw more cheaply" is the lever.
The lever is not invalidating a 330 px band thirty times a second.

## The same thing on a real GPU, and why it changes the answer

Everything above runs under Xvfb, where there is no DRI3 and GSK falls back to
software rendering (`MESA: No DRI3 support detected`). That is reason enough to
distrust the absolute figures, so the whole decomposition was repeated on the
real GPU: `cage` on the headless wlroots backend, EGL through the `iris`
driver, nothing on the desktop. Its output is 1280×720, where the panel is
collapsed and the bloom correctly pinned — so the Now Playing panel is opened
first through its own accessible action, which buttons expose even though the
sidebar rows do not.

| state | Xvfb (software) | cage (real GPU) |
| --- | --- | --- |
| the breath as it ships | 94 ms/s | **135 ms/s** |
| tick runs, does nothing else | 10 ms/s | **10 ms/s** |
| invalidated, draw function paints nothing | 103–115 ms/s | **57 ms/s** |
| no breath | 5 ms/s | **3 ms/s** |

The cost is not a software-rendering artefact — it is *higher* on the GPU. But
its make-up is inverted, and that is what decides the repair:

- Under software rendering the recomposite swamped everything, so the Cairo
  painting looked free.
- On the GPU the recomposite gets cheap (57 ms/s) and **the painting becomes
  the larger half: ~78 of the 135 ms/s**.

That painting is CPU work on every frame: `draw()` scales the cached 32 px
surface across the band through Cairo, and the result is handed to the renderer
again. Caching the blurred cover as a texture once and animating it with
snapshot nodes — opacity and scale, which is all the breath ever changes —
would remove that half without touching what the effect looks like. The
remaining ~47 ms/s is the invalidation itself, and the frame clock underneath
it is 10.

One more thing this rig proves in passing: with the surface never presented
(`cage` before the panel is opened) the build reports **0 paints and 4 ms/s**.
No presentation, no frame callbacks, no breath — an occluded or hidden window
costs nothing.

## The repair, measured rather than expected

`REPRISE_EXPERIMENT_BLOOM_TEXTURE=1` builds the bloom the other way: the blurred
cover — already bought once per track — is handed to the renderer once as a
`GdkMemoryTexture` in a `GtkPicture`, and each frame sets only an opacity and a
scale transform on it. No Cairo, no rasterizing, nothing per frame that the
snapshot cannot do by itself.

| on the GPU, panel open, paused | CPU | paints/s |
| --- | --- | --- |
| the breath as it ships (Cairo) | 135 ms/s | 86 |
| **the same breath as a texture** | **55 ms/s** | 88 |
| the floor: invalidated, painting nothing | 57 ms/s | 88 |
| no breath at all | 3 ms/s | 0 |

**It lands on the floor.** 55 against a lower bound of 57 means the painting
cost is gone entirely, not merely reduced: what remains is the invalidation the
effect needs in order to be an animation at all. That is 59 % less idle CPU for
the state the app sits in most of the time.

And it looks the same. Comparing the panel head in both builds: RMSE 0.0099 —
one percent, and the breath is at a different point of its six-second sine in
the two shots, which accounts for that on its own. Mean brightness across the
bloom band is 0.164715 against 0.164505, a difference of 0.13 %. The strip just
outside the panel edge is identical to six decimals, so nothing bleeds past the
clip either.

### From experiment to implementation

The flag is gone; the bloom is now a widget of its own (`cover_bloom_area`)
whose `snapshot` places the texture, and the `GtkDrawingArea` with its Cairo
draw function is deleted. Geometry comes from the allocation instead of
`PANEL_WIDTH`, so a panel of any width is right by construction.

One thing had to be measured a second time, because the first way of writing it
gave back a third of the win:

| bloom implementation | idle CPU (median of 3, alternating with Cairo) |
| --- | --- |
| Cairo, as it shipped | 110 ms/s |
| texture, scaled by giving the node new bounds each frame | 96 ms/s |
| **texture, scaled by a transform around the band's centre** | **64 ms/s** |

Same picture, three ways of asking for it. A texture node whose parameters
never change can be reused from frame to frame; one that is handed a new
destination rectangle every frame is new work every frame. Only the transform
version reproduces what the throwaway experiment measured (55 ms/s), and the
difference between the two is invisible on screen.

Visually it is the Cairo path: RMSE **0.00078** between the two panel heads
(0.08 %), mean brightness across the bloom band 0.164704 against 0.164864.

Measurement note, because it nearly cost a wrong conclusion: with two worktrees
sharing one `CARGO_TARGET_DIR`, `cargo build` reported the crate "Fresh" after
a source edit and left the previous binary in place. A round of A/B numbers was
collected before that surfaced. Every variant now carries the md5 of the binary
that actually ran, and the runner aborts if the swap fails rather than
measuring the old one under the new name.

**Still open on this half:** the playing path (`Mode::Live`, spectrum-driven)
was never part of these measurements and deserves its own look.

One trap worth recording, because it produced a beautiful wrong answer first:
the tick callback hung on the `DrawingArea`, which in texture mode is no longer
in the widget tree. An unparented widget has no frame clock, so the callback
never ran, and the experiment reported 5 ms/s and zero frames — an apparently
perfect result that was really just the breath not happening.

## What a sidebar switch actually costs

Measured with the breath off, so the numbers are the switch and not the
background drain. `block` is the longest stretch the main loop could not answer
an accessibility call — the visible freeze. `cpu` is main-thread time over a
fixed 8 s window. Each switch is verified against the app's own
`sidebar: row selected` log line and against which row the app then reports as
selected.

| place | block (cold/warm) | cpu over 8 s | settles |
| --- | --- | --- | --- |
| **Music** (library) | **174 / 159 ms** | 620 / 620 ms | 0.4 / 0.3 s |
| **Queue** | **182 / 167 ms** | 610 / 600 ms | 0.4 / 0.3 s |
| Lorna Shore (playlist) | 136 / 129 ms | 530 / 500 ms | 0.3 s |
| Recently added | 32 / 31 ms | 370 / 410 ms | 0.1 / 0.2 s |
| Top rated | 20 / 29 ms | 370 / 410 ms | 0.1 / 0.2 s |
| Recently played | 15 / 25 ms | 360 / 390 ms | 0.1 / 0.2 s |
| Releases | 34 / 38 ms | 330 / 340 ms | 0.2 s |
| My Stats | 34 / 17 ms | 330 / 260 ms | 0.2 / 0.1 s |
| Podcasts | 19 / 21 ms | 220 / 200 ms | 0.1 / 0.3 s |
| YouTube | 14 / 11 ms | 230 / 180 ms | 0.1 s |
| Radio | 16 / 15 ms | 160 / 170 ms | 0.1 s |
| Concerts | 12 / 17 ms | 180 / 210 ms | 0.1 s |

Three places cost something a person could notice — **Music, Queue and the
playlist, all around 130–180 ms of frozen UI**; they are the three that build a
track list. Everything else is between 11 and 40 ms and settles within two
tenths of a second. Cold and warm barely differ, which is itself a result: the
first visit to a place is not meaningfully more expensive than the second.

For comparison, the same switches with the breath running cost 850–1800 ms of
CPU per 8 s window instead of 160–620 ms. The difference is not the switch. It
is the ~1000 ms of breath work that shares the window.

## Two things worth their own look

**`My Stats` computes on the UI thread.** `refresh_parts` calls
`stats_snapshot::compute` synchronously (`stats_view.rs:450`), which is eight
read statements over the listen history. On this library it is fast enough to
hide inside the 34 ms block above, so this is a scaling risk rather than a
measured freeze — but the work is on the main thread and grows with history.
`reprise-core` already calls that function "the seam that permits a transparent
cache wrapper later if profiling ever justifies one".

**Every launch re-reads the same 455 tags, and a warm cache does not help.**
This one was first written down wrong here — as "over four minutes of
re-reading" — because the tag warnings ran to the end of every measurement run.
They did, but not on their own: each of those runs was clicking through views,
and every view that builds a track list reads tags. Left alone, the app reads
for **22 seconds after launch and then stops completely**. The correction is
the interesting part, because the real behaviour is sharper than the wrong
version.

The reads are deterministic: six consecutive runs, `455` each time. That is not
a first-start effect — the instance's cache directory persists across all of
them and holds 21 MB in 918 files. The reason is the order in `cover_loader`:

```rust
let source = resolve_source(std::path::Path::new(&path_for_worker))?;  // reads the tags
thumbnail(&source, size).ok()                                          // then asks the cache
```

The file's tags are read *before* the thumbnail cache is consulted, so a warm
cache saves the decode and the resize but never the read. Worse, when the cache
misses, the request goes to the cover-download worker, which reads the same
file's tags **again** (`read_cover_tag`) and, under `skip_if_covered`, resolves
the source a third time.

Switching the cover-download module off makes the causal share visible:

| startup, same library, same warm cache | tag reads | burst | process CPU over 45 s |
| --- | --- | --- | --- |
| cover download on (shipped default) | 455 | 22 s | 17.5 s |
| cover download off | 87 | 5 s | 13.6 s |

So the download worker accounts for **~3.9 CPU-seconds and 368 of the 455 reads
at every single launch**. None of it is on the main thread — the UI stays
responsive throughout — so this is a cost in battery and disk, not in felt lag.

### Where the reads really came from, and what fixed them

Two sources, and the first guess was the smaller one.

**The loader.** `cover_loader` resolves the source before asking the cache, so
the file is read every time. A second index fixes that: one small file per
track and size, holding a stamp and the thumbnail that stamp resolved to. The
stamp is three `stat` calls — the track (rewritten), the album folder (a
sidecar appearing), and a marker the download side bumps when it publishes
(a downloaded cover outranks a track's own artwork, so publishing one can
change what any track resolves to). Microseconds against milliseconds.

That alone took the loader from 87 reads to **2**. And the remaining 370 were
all still there — which is how the second source got found.

**The batch.** `window_runtime_wiring.rs` calls
`lyrics_batch.start_after_cover(cover_batch)` unconditionally, and
`CoverDownloadBatch::start` walks **every live track path in the library** and
sends each one to the download worker, which reads that file's tags to work out
which album to look up. On every launch, for a library that has not changed.
Filtering that list against the same index — and recording the settled outcome,
whether "already covered" or "nothing found" — leaves nothing for it to do.

Measured back to back on one machine so the load is the same for both:

| launch | tag reads | process CPU over 45 s |
| --- | --- | --- |
| index cleared (what ships today) | 419 | 25.7 s |
| index warm | **0** | 22.4 s |
| index warm, again | **0** | 22.7 s |

**Zero reads on every launch after the first**, and ~3.2 CPU-seconds saved —
which is the whole cost of the feature: with the cover-download module switched
off entirely the same launch costs 22.5 s, so the repaired version does the
feature's work for what it costs to not have it.

Two tests hold the behaviour: one proves the remembered answer is used without
touching the file (the track is `chmod 000` after the first resolution — anything
that still opens it fails), the other that a sidecar cover appearing undoes a
remembered "no cover".

## Accessibility, found while trying to drive the sidebar

Not part of the performance question, but all three are real:

- Sidebar rows advertise the `Action` interface and expose **zero actions**, so
  no assistive technology can activate a place.
- Their component extents all read `0×0`, so nothing can locate them either.
- Keyboard focus, once a view has taken it, **never returns to the sidebar**:
  Tab and Shift+Tab cycle inside the content pane (a six-element loop) and F6
  does nothing. There is no accelerator that focuses the sidebar.

Together these mean the sidebar is unreachable without a pointer.

## How this was measured, and what the earlier numbers were worth

- The trigger is a real pointer click. The sidebar navigates on `row-activated`;
  `row-selected` deliberately does not route, so an accessibility selection
  changes nothing. Keyboard activation works but cannot be repeated, because
  focus cannot get back to the sidebar (above).
- Click targets come from the app itself. The row map (`TICK_CENSUS row name=…`)
  is emitted by the instrumented build whenever the layout changes. The previous
  attempt used hand-measured pixel offsets and got three of twelve rows wrong —
  a click meant for *Releases* activated *Recently added*, and the numbers were
  printed under the wrong names. Two rows the ruler did not know about
  (*New playlist*, *Import playlist…*) are what shifted everything below them.
- "Settled" is derived from the app's own idle ping distribution, not a fixed
  threshold. With a fixed 20 ms threshold every place reported "never", because
  this app does not reach 20 ms while the breath is running — a constant that
  said nothing about any of the places it was printed next to.
- Attribution is counted, not sampled: every `add_tick_callback` in the UI
  reports itself, the frame clock is watched passively via `after-paint`, and
  every `AdwAnimation` reports through the single place they are all built
  (`motion.rs`). A stack sampler cannot tell you which animation kept the clock
  alive; this can.
- The first full run of these tables was taken while an unrelated Gradle build
  held three cores. It is kept only as `run1-scanned-*` and none of its numbers
  are quoted here.

Raw output: `~/.cache/reprise-sidebar-measure/`.

## What of this shipped, and what stayed here

Two changes went to `dev`, each on its own, each with the full gate battery:

- **#274** — the launch no longer re-reads every file's tags: 419 reads and
  17.65 s of process CPU over the first 45 s become 0 and 13.84 s.
- **#276** — the bloom is a texture the snapshot places: 110 ms/s idle becomes
  64, with the panel head unchanged to RMSE 0.00074.

Neither carries any of the scaffolding this branch is made of. The tick census,
the sidebar row reporter and the four `REPRISE_EXPERIMENT_*` switches exist
**only here** — they are how the two changes were found and checked, not part
of either. `REPRISE_EXPERIMENT_BLOOM_TEXTURE` in particular describes a shape
that no longer exists: what shipped animates by transform rather than by
re-bounding the texture node, which is three times the win and was only found
by measuring the first attempt.

One number in this file has a real-world counterpart now. The installed build —
still the old one — was sampled read-only on the actual desktop while paused:
**49 ms/s of main-thread CPU doing nothing**, against 123–135 ms/s headless.
Lower on a real GPU, as expected, and not zero, which is the point.

Still open when this branch was parked: the freeze on switching to Music,
Queue or a playlist. Timing its pieces (branch `perf/switch-cost`) puts all of
it in the model swap — but reports 2.5–3.3 s where the accessibility probe had
measured 130–180 ms on the older `dev`, and which of the two is true is not yet
decided. Either a regression arrived with #270/#271/#277, or the wall clock is
spanning something that lets the main loop keep running. Nothing should be
concluded from those numbers until that is settled.
