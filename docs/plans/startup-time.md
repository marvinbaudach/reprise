---
slug: startup-time
worktree: ~/Projects/reprise-startup-perf
branch: perf/startup-time
phase: planned
created: 2026-08-08
base: c831350458
---
# The app should be there, and answer, sooner

Starting Reprise takes about two and a half seconds before a window appears,
and roughly three more before the app stops being sluggish. This document says
what the time is actually spent on — measured, not guessed — and what to do
about it.

## 1. What was measured, and how much of it to trust

The bench lives in `~/.cache/reprise-startup-bench/`. It launches a real
release binary against a **copy of the real 242 MB library**, inside its own
Xvfb display and its own D-Bus session, and records:

- every `tracing` milestone as an offset from `exec`,
- the moment an X window called "Reprise" first exists,
- `/proc/<pid>/stat` for the main thread every 4 ms — which distinguishes
  *computing* (state `R`, ticks climbing) from *waiting* (`S`/`D`), and needs
  no ptrace,
- `eu-stack` backtraces, via an `LD_PRELOAD` shim that opts the process into
  `PR_SET_PTRACER_ANY` (the host runs `yama/ptrace_scope=1`).

**Two traps this bench already fell into, both worth remembering.**

*A cold `XDG_CACHE_HOME` makes GStreamer rebuild its plugin registry.* The
first runs wiped the cache per launch and so measured an ~800 ms cost that no
real launch pays. The cache is now preserved between runs.

*Wall-clock startup drifts by ±400 ms depending on what the machine did
recently.* The same installed binary measured 1390 ms before a `cargo build`
and 2326 ms after it — page cache, not code. An earlier reading of this as "dev
has regressed 68 %" was wrong. **Any before/after claim must come from
interleaved runs (A, B, A, B, …) in one sitting, reported as a median with its
spread.** Sequential "measure A, change something, measure B" is worthless
here.

This is why Task C0 exists (§3). Wall-clock from outside is good enough to see
the shape of startup; it cannot prove a 150 ms improvement. Only in-process
timers can.

### The shape, on `c831350458`, warm

| Phase | warm | cold (from a real session, 2026-08-08 11:38) |
| --- | ---: | ---: |
| `exec` → first log line (dynamic linker) | ~280 ms | ~1310 ms |
| `app.register()` D-Bus → database open | ~220 ms | — |
| open + migrate the database | **6 ms** | 28 ms |
| device sync + player backend | ~120 ms | ~1350 ms |
| views and wiring, up to `present()` | ~900 ms | ~1600 ms |
| **window visible** | **~2.3 s** | **~4.3 s** |
| still busy afterwards (main thread ~50 % occupied) | **~3 s** | more |

Cold numbers come from `journalctl --user` — real launches, not the bench. The
median real launch takes 1.24 s from `starting Reprise` to `main window built`;
one cold launch took 4.32 s.

### Ruled out — do not re-investigate

- **The database.** Opening and migrating 242 MB costs 6 ms warm, 28 ms cold.
- **The GStreamer registry** (24 ms warm) and **creating `playbin3`** (1.8 ms).
- **`Gio.VolumeMonitor`** (13.7 ms), the usual suspect behind `DeviceMonitor`.

### Confirmed, with evidence

- **The first ~280 ms are the dynamic linker.** Every stack sample taken before
  the first log line sits in `_dl_relocate_object` / `do_lookup_x`. The binary
  pulls in 136 shared libraries. Out of scope here; noted for a separate look.
- **The track list is loaded five times during startup** on `c831350458` — at
  783, 1921, 2007, 2094 and 2182 ms, each a full query over 2 315 tracks plus a
  model rebuild.
- **441 ms of unbroken computation** sit inside `window_runtime_wiring::wire`:
  50 of 50 samples `RUNNING`, no waiting. Nothing is logged in that window.
  This is the largest single unexplained block before the window appears.
- **After `present()` the app keeps working for ~3 s** with the main thread
  busy about half the time: an HTTP request to rockantenne.de (fired at 1909 ms
  — *before* the window is even mapped), one to musicbrainz.org, lofty parsing
  FLAC picture blocks off disk, and `spectrogram_batch`.

### Already fixed on dev — do not redo

`scan_watcher::reconcile_refreshes` already skips the work for an all-zero
watcher event. The installed binary predates that fix, which is why early
measurements showed a pointless sidebar rebuild at 4.5 s. The reconcile log
line still prints on an empty event; only the refreshes are gated.

## 2. What we are changing

Three packages. They are independent and can land separately, but **C0 comes
first** because nothing else can be proven without it.

## 3. Package C0 — make startup measurable from inside

A permanent, env-gated startup profile, modelled on the existing
`ui::runtime_performance` hook (`REPRISE_PERF_RUNTIME_REPORT`): setting
`REPRISE_PERF_STARTUP_REPORT=/path.json` writes one JSON object with the wall-clock
offset of each startup phase, plus counters for track-list reloads and sidebar
rebuilds.

Phases to record, at minimum:

- process start → logging ready → i18n → application built → registered
- database opened, migrated
- each named block of `ui::window::build`: appearance bootstrap, runtimes
  (cover download, scrobblers, artist news, concerts, podcasts, portraits),
  MPRIS, device sync, player backends, sidebar, track list, the content-stack
  pages (`stats`, `concerts`, `releases`, `podcasts`, `youtube`, `radio`),
  `PreferencesContext::new`, action wiring, runtime wiring
- `present()`, and first frame drawn

Why permanent rather than temporary `Instant` timers: the same instrument is
needed to prove A and B worked, to decide what C1/C2 should even touch, and to
catch the next regression. It is off unless the variable is set, so it costs a
single `env::var` on the normal path.

**This task ships before A, B and C1/C2, and its first output is the baseline
those three are judged against.**

## 4. Package A — stop doing the same work five times

**A2. One load, not five.** `TrackList::new` loads the default place, and
`window_runtime_wiring.rs:607` then routes to the *restored* place, which loads
again. The first load is thrown away. The track list should be constructed
knowing where it will end up, so startup performs one query and one model
build.

The remaining loads come from the routing path itself (sidebar row selection,
browse-bar visibility, viewport centring) each triggering a reload. The rule to
establish: **during startup, a reload is requested once and served once.**

**A3. One sidebar build, not four.** The sidebar is rebuilt for `initial
build`, `up next changed`, `session restore`, and again on the first watcher
reconcile. Each rebuild is a full `ListBox` teardown plus five queries.
Coalesce the startup rebuilds into a single one at the end of the startup
sequence, keeping the existing per-trigger refreshes for everything after.

**Acceptance:** with `REPRISE_PERF_STARTUP_REPORT` (see C0) the report shows exactly
one `track_list_reload` and one `sidebar_rebuild` between process start and
`main window built`.

## 5. Package B — let the window breathe before the background starts

Today four independent pieces of background work start whenever their own code
happens to run, which is during or immediately after window construction:
radio now-playing (HTTP, before the window is even mapped), MusicBrainz
lookups, cover/tag extraction through lofty, and `spectrogram_batch`.

Introduce **one startup-quiet gate**: a single signal that fires after the
first frame has been drawn and the main loop has then been idle for a short
interval. All four hang off it instead of starting on their own. One mechanism,
four users — not four timers that cannot see each other.

**Acceptance:** no HTTP request and no file-tag read is issued before the
window is mapped; the main thread is idle (`/proc` state `S`, no climbing
ticks) within 300 ms of the window appearing.

## 6. Package C1/C2 — build less before showing the window

`window.rs` builds `stats_view`, `concerts_view`, `releases_view`, the podcast,
YouTube and radio views, and the whole `PreferencesContext`, all before
`window.present()` on line 592 — although only the `library` page is visible
and most launches never open Preferences at all.

The obvious move is to build those lazily. **It is not yet justified.** The
441 ms block is in `window_runtime_wiring::wire`, and no measurement yet says
how much of the pre-window time those eager views actually account for. The
last performance round had two plausible hypotheses (cover loading, GStreamer
pipeline) that measured 0.1 ms and 3.7 ms respectively; the cost was somewhere
nobody had suspected.

So C1/C2 are **scoped by C0's output**, not before it. Once the report exists:
attack the phases it shows to be expensive, in descending order. If the eager
views turn out to be cheap, this package changes shape or disappears, and that
is a good outcome, not a failure.

## 7. Verification

- `REPRISE_PERF_STARTUP_REPORT` before/after, **interleaved**, at least 7 rounds per
  side, reported as median plus min/max.
- The external bench (`~/.cache/reprise-startup-bench/run.sh`) as a
  sanity check on the total, never as the primary evidence.
- A real launch through the desktop entry, checked in `journalctl --user`,
  to confirm the improvement survives outside the bench.
- The existing display-test gates, run **individually** — the suite is flaky in
  a herd, and three tests are already red on `dev` before any change.
