---
slug: activation-latency
worktree: /home/marvin/Projects/reprise-activation
branch: perf/activation-latency
phase: refactored
codex_session:
created: 2026-08-08
---
# A track should play immediately

Playback is the application's primary feature. Double-clicking a row is the
most direct expression of "I want to hear this now". It must feel immediate,
not merely faster than it does today.

## Established measurements (2026-08-08)

Release build, isolated instance over a copy of the real library (2,340 tracks,
241 MB database), Xvfb, private D-Bus and a private PulseAudio null sink. The
click timestamp came from `xdotool`, the timeline from `tracing` logs and the
main thread from `eu-stack` samples.

| Metric | Result |
| --- | --- |
| `activate track` to `playback started` (double-click) | **92 ms median** (66–150), 14 runs |
| Click to `playback started` (Next button) | 34–65 ms, 3 runs |
| Full-view ID query, cold / warm | **66 ms / 2–4 ms** |
| Read a 35–50 MB FLAC, cold / warm | 24 ms / 16 ms |

The real user-session journal shows the same pattern: the first double-click
after startup costs 216 ms, then falls through 118, 81 and 51 to 37 ms. That is
the signature of a warming database cache.

Two suspects were ruled out before work was spent on them:

- **File I/O.** A 50 MB FLAC reads in 24 ms cold and 16 ms warm. An eight
  millisecond difference cannot explain a perceptible delay.
- **Pre-buffering.** It seemed possible that Next was faster only because it
  took over the gaplessly prepared stream. It does not: only
  `advance_gaplessly` (automatic end-of-track transition) uses
  `StartPlayback::No`. Manual Next goes through `advance_playback` with
  `StartPlayback::Yes` and rebuilds the pipeline just like direct activation.
  The comparison is fair.

## Where the time goes

### B1 — The sidebar rebuild runs on every track change

This finding has the widest impact because it affects every song, not only a
double-click.

```text
play_from_view                      (double-click)
 -> queue.set_tracks(ids, start_index)
 -> play_track_id(id)               <- "playback started" is logged here
 -> notify_queue_changed()
     -> queue_changed callbacks (window.rs:281)
         -> sidebar_rebuild::rebuild             19 synchronous queries
             -> count_releases_view
                 -> query_complete_history_in
                     -> artist_news_query::local_library_index
                        <- whole-library index for one count
     -> feed_next()
```

`advance_common` (Next) and the automatic transition also call
`notify_queue_changed`. All three routes pay this cost. Stack sampling found
`sidebar_rebuild` -> `count_releases_view` -> `artist_news` in two of the 25
working samples.

Starting a track changes none of these counts.

### B2 — The full ID query runs on every activation

`track_list_activation::queue_ids_for_activation` asks
`queries::query_track_ids_browsed` for the complete sorted and filtered ID list
on every double-click. It was measured directly at 66 ms cold and 2–4 ms warm.
This is the part Next does not have and explains the warm-up pattern above.

The list depends only on source, sort, filter and browse facets. Two activations
in the same view return the same list; only the start index differs.

### B3 — Work is ordered poorly in the click path

`play_from_view` sets the whole queue before starting playback, then updates
the counters. The clicked track is already known at the first line. The queue
is needed no earlier than the end of the current track, while the counters are
never urgent.

### Originally open: the rest of the 92 ms

With a warm cache B2 costs only 2–4 ms, while `activate` to `started` remained
around 92 ms at the median. The remainder was unattributed: `eu-stack` needs
about 290 ms per sample and only hit the 92 ms window twice across 14
activations.

Candidates were `play_origin::resolve` (which loads playlists for playlist and
smart sources), `queue.set_tracks` (which copies up to `QUEUE_LIMIT` IDs), and
`play_track_id` itself. Optimising any of these without measurement would risk
optimising the wrong place.

## Tasks

Measure after every task, including a countercheck with the change disabled.

### T0 — Divide the 92 ms first

Before changing production behavior, instrument `activate track` to
`playback started` and report `queue_ids_for_activation`,
`play_origin::resolve`, `queue.set_tracks` and `play_track_id` separately.

#### T0 result (2026-08-08)

Temporary timers were added around all four boundaries, used in an optimised
release build, and removed before the T0 commit. Fourteen consecutive
activations ran in one isolated Xvfb/private-D-Bus/private-XDG instance over
2,340 generated rows and a 44.1 MB synthetic FLAC. No build or other sustained
CPU load ran during the measurements.

| Segment | Median | Range |
| --- | ---: | ---: |
| `queue_ids_for_activation` | 1.078 ms | 0.513–1.359 ms |
| `play_origin::resolve` | 0.006 ms | 0.004–0.007 ms |
| `queue.set_tracks` | 0.006 ms | 0.003–0.009 ms |
| `play_track_id` entry to `playback started` | 7.141 ms | 3.294–22.113 ms |
| Entire `activate track` to `playback started` | 8.243 ms | 4.103–22.633 ms |

This controlled fixture does **not** reproduce the established 92 ms median.
The handoff's 241 MB library-copy database is not present in this worktree,
and the real database and music files are explicitly out of bounds. The
synthetic result therefore cannot legitimately apportion the real run's
remaining roughly 90 ms or overrule its directly measured 66 ms cold ID
query. It only rules out `play_origin::resolve` and the ID-vector copy as
meaningful costs on this data shape, while `play_track_id` dominates the much
smaller synthetic path.

The `play_track_id` row is derived per activation from the outer timestamp
span minus the three directly timed preceding segments; its range is not a
subtraction of aggregate medians.

**Priority decision:** T0 does not supply representative contrary evidence,
so the planned order remains T1 then T2. T3 remains conditional on the
post-T1/T2 countercheck. The real-run remainder stays explicitly unattributed
rather than being extrapolated from the synthetic fixture.

### T1 — Do not recompute counters on every track change

A track change changes no sidebar count. The rebuild reached from
`notify_queue_changed` is therefore pure waste on start, manual advance and
automatic transition.

Determine what the queue callback actually needs; it is expected to need the
queue length and queue surface refresh, not the 19 queries for Music, Missing,
Library Doctor, playlists, Podcasts, YouTube, Radio, Releases and Concerts.

The counters must not become stale. Every route that actually changes a count
must retain its own refresh; prove those routes rather than assuming they
exist, and preserve the contract with a test.

In addition, `count_releases_view` builds a whole-library index through
`local_library_index` merely to produce a count. Even when the rebuild is
rarer, that work does not belong synchronously on the UI thread.

#### T1 result (2026-08-08)

The queue callback now updates the retained Queue badge label and reloads the
Queue surface only when visible. It no longer runs `Sidebar::refresh`, changes
row identity, or performs a database query. The Releases badge projection now
runs on a named worker over its own database handle; a generation token makes
overlapping refreshes latest-wins before GTK applies the result.

A timer countercheck used 2,340 generated tracks and 2,340 generated release
rows with no concurrent build. The temporary timer test was removed after the
run. In the debug build, the median in-place Queue badge update was 557 ns over
1,000 runs. Forcing the change off by calling the old full sidebar refresh was
4.357 ms over 14 runs even with Releases disabled; the separately timed
synchronous `count_releases_view` that the old refresh also performed was
77.907 ms over 14 runs. The latter identifies a concrete large cost outside
T0's pre-`playback started` boundaries and agrees with the earlier stack sample
inside `local_library_index`. The absolute debug timings are not substituted
for the established release/audio baseline; the enabled/disabled comparison
is the evidence for this task.

The call-site audit retained 29 production full-refresh or refresh-and-select
routes for actual mutations and restoration/navigation cases, covering scans
and watcher reconciliation, deletes and missing state, tag edits, playlist
CRUD/import, Library Doctor, external changes, source refreshes, issue-view
state, preferences/modules, mounts, and relinking. The regression proves that
a queue-only update leaves Music and row identity untouched, while a library
mutation followed by the retained full-refresh route changes the Music badge.

A private PipeWire/PulseAudio server and null-sink attempt could create its
isolated sockets but could not admit a client in this sandbox (`Host is down`
after the PipeWire access check failed). Therefore no honest RMS tone-onset
number is available here; the task has timer and structural evidence only.
The private processes and worktree-local measurement assets were removed.

### T2 — Do not query the view's ID list on every double-click

The list is a pure function of source, sort, filter and browse facets. Retain
it while those inputs stay unchanged so another activation in the same view
only needs the start index.

The library can change underneath the view through scanning, deletion, tag
changes or the watcher. A retained list that points to removed or moved tracks
is worse than a slow query because it can play the wrong track or nothing.
`TrackListModel::generation` already records model changes and is the natural
cache key. Preserve `QUEUE_LIMIT` and its truncation warning.

#### T2 result (2026-08-08)

The activation path now retains the bounded, ordered ID projection under the
current `TrackListModel::generation`. A later activation in the same rendered
generation clones that projection and changes only the start index. Queue-view
activation remains deliberately uncached because its projection is live. A
query failure is not cached, and the existing `QUEUE_LIMIT` query bound and
truncation warning remain on every cache miss.

The display regression was observed failing before implementation: after two
rows were rendered, a direct database insertion made the second activation
return the new ID even though the model generation had not changed. It now
proves the same generation reuses the original `[1, 2]` projection, then an
explicit `TrackList::reload()` advances the generation and returns the updated
`[1, 3, 2]` order. That is the same reload seam used after scans, deletion,
tag changes and watcher reconciliation; source, sort, filter and browse-facet
changes also repopulate the model and advance this generation.

A temporary release timer harness used 2,340 generated tracks on an idle host
and was removed after the run. A cache hit took 169 ns at the median over 1,000
runs. The disabled-change counterprobe cleared the cache before every call and
therefore executed the original full-ID query; its median was 372.969 us over
14 runs. This warm synthetic query is much cheaper than the established 2–4 ms
warm and 66 ms cold representative-library query, so its absolute value does
not replace that baseline. It does demonstrate that repeat activation no
longer pays the query at all.

The T1 audio-endpoint blocker remains: this sandbox's private PipeWire server
could create sockets but rejected PulseAudio client admission with `Host is
down`. No RMS tone-onset result is claimed for T2.

### T3 — Sound first, the rest afterwards

The clicked track is already known. Playback could start before the queue and
counters are ready because neither is immediately needed.

`feed_next` needs the complete queue for gapless preparation and must never see
a partially filled one. Rapid successive double-clicks must not reorder work;
the latest click wins. The queue must also be complete before the playing track
ends so playback continues normally.

Only do this task if T0 and the post-T1/T2 measurement show meaningful time is
still available to recover.

#### T3 decision (2026-08-08)

The condition for T3 is false, so no asynchronous queue reordering was added.
After T1, a queue notification performs the sub-microsecond retained-badge
update instead of the measured 4.357 ms full refresh plus its 77.907 ms
synchronous Releases projection. After T2, repeat activation uses the 169 ns
ID projection above. T0 measured the only work T3 could move ahead of playback
— `play_origin::resolve` and `queue.set_tracks` — at 6 us each, while the
remaining 7.141 ms synthetic pre-log segment was `play_track_id` itself and
cannot be bypassed by T3. Twelve microseconds is not meaningful recovery
against the complexity and ordering risks named by this task.

This decision does not pretend to apportion the established real-library
92 ms median: its roughly 90 ms remainder remains explicitly unattributed for
the reasons recorded under T0. It says only that the measured work T3 could
reorder is too small after T1 and T2; introducing a half-built queue would add
risk without measurement evidence for a user-visible gain.

## Verification

- Measure with timers. Frame sampling can return zero samples and produce a
  false green result at this duration.
- Countercheck every measurement with the change disabled.
- The meaningful endpoint is audible onset, not a log line. A PulseAudio null
  sink plus `parec`, using RMS over 5 ms blocks, measures it. Record machine
  load because parallel builds substantially distort the audio stack.

Targets against the established baseline:

- Double-click `activate track` to `playback started` is clearly below the
  92 ms median.
- The gap to Next (34–65 ms) largely disappears.
- No sidebar rebuild remains in the track-change path.
- Sidebar counters remain correct on every mutation route.
- Playback still advances normally at the end of a track.

## Out of scope

- File I/O, measured at 24 ms cold and not a relevant contributor.
- Gapless pre-buffering itself; it works and is not the cause.
