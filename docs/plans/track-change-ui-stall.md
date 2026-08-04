---
slug: track-change-ui-stall
worktree: /home/marvin/Projects/reprise-track-change-ui-stall
branch: feature/track-change-ui-stall
phase: reviewed
codex_session:
created: 2026-08-03
---
# Track change freezes the UI for ~0.7 s

## Symptom

Pressing Next: the audio switches almost immediately, then the whole window
sits frozen and repaints everything at once roughly a second later. Reported
as "I already hear the track, and the visuals only catch up a few seconds
later — then all at once".

## Measurement

Measured against the running app (PID 2126720, installed release build), 16
driven Next clicks plus 19 track changes recovered from `/tmp/reprise-run.log`.

| | Median | Range |
|---|---|---|
| click → audio switches | 335 ms | 313–549 ms |
| click → player bar shows the new track | ~950 ms | 859–1382 ms |
| GTK main thread unresponsive | ~700 ms | 555–1108 ms |

Method:

- **Main-loop responsiveness**: a 50 Hz AT-SPI poll of the player bar's title.
  AT-SPI is served from the GTK main loop, so its round-trip time is a direct
  stall detector. Idle median 0.2–4 ms; during the stall a single call takes
  555–1108 ms.
- **Audio onset**: `parec` on the sink monitor at 5 ms resolution. The stream
  teardown between tracks writes true digital silence, which no quiet musical
  passage does, so the cut is unambiguous.
- **What the thread is doing**: `/proc/<pid>/task/<tid>/stat` sampled at 4 ms.
  During the stall the main thread is in state **R for 96–100 % of samples**
  and accumulates 250–810 ms of `utime+stime`. It is burning CPU, not waiting
  on I/O, locks, or the network.

## Root cause: the auto-centering scroll re-binds the visible rows

`update_current_track` (`crates/reprise-gnome/src/ui/track_list/current_track_selection.rs`)
ends with `reveal_track_position` → `adjustment.set_value(target)`. GtkColumnView
handles that adjustment change **synchronously**: on a distant jump it discards
the realised rows and binds a fresh screenful before `set_value` returns.

Three independent confirmations:

1. **Log timing on the real app** — median 511 ms, max 855 ms between
   `playback started` and `current track centered`, 19/19 track changes. The
   one change where the list only had to move 12 rows: **9 ms**.
2. **Scroll test with no track change at all** — driving the track table's
   scrollbar over AT-SPI, a long jump blocks the main loop for **572–1022 ms**
   per jump (idle ping 0.2 ms). Same magnitude, no playback involved.
3. **Distance dependence** — every jump ≥ 178 rows cost 450–855 ms; the single
   12-row jump cost 9 ms.

A shuffled queue means every Next lands far away in the library list, which is
why this fires on *every* press rather than occasionally.

**This is not only a playback bug**: plain scrolling of the library pays the
same ~0.8 s freeze per long jump.

## Secondary cause: per-frame display-wide CSS reload (~200 ms)

`cover_accent::cross_fade_accent` installs a `CallbackAnimationTarget` that
calls `set_cover_accent` on **every animation frame**, and `set_cover_accent`
does `provider.load_from_string(...)` on a provider registered display-wide via
`style_context_add_provider_for_display`. The value is an `@define-color`, so
GTK re-resolves colours across the whole widget tree once per frame for the
400 ms `AMBIENT` animation.

Differential: with `org.gnome.desktop.interface enable-animations=false`
(libadwaita's `set_follow_enable_animations_setting(true)` collapses the fade to
a single application) the stall drops from 592–1003 ms to 503–598 ms.

## Constraints

- **NAV-10a is active and must hold**: "Play from Stopped as well as explicit
  Previous/Next center the new track without stealing focus or selection."
  The centering stays; it has to become cheap, not go away.
- **FIL-9** relies on the same centering path when a filter changes.
- Marking and scrolling must stay separate (NAV-10a, NAV-13) — the marker path
  must not regress into `items_changed`, which is what the
  `now_playing_marker` registry exists to avoid.

## Profile

Instrumented release build (`perf/track-list-scroll-stall`, scaffold in
`ui/track_list/perf_probe.rs`), 10 track changes against an isolated copy of the
real library (1822 rows), GL renderer.

Per track change with a distant jump:

| Phase | Cost |
|---|---|
| `ids_ms` — `current_view_ids()` | 2–3 ms |
| `marker_ms` — `reapply_now_playing_markers()` | 1.0–1.7 ms |
| `rows_ms` + `target_ms` — row count and centre math | 0.01 ms |
| **`set_value_ms` — the scroll itself** | **500–573 ms** |
| of which `bind_ms` — ListView cell binds | 267–301 ms |
| **`binds` — cell binds triggered** | **1640** |

The view query and the marker reapply are innocent. All of the time is one
`GtkAdjustment::set_value`, and it triggers **1640 cell binds** — roughly twenty
full rebuilds of the visible rows for a single jump. Short jumps cost
proportionally less: a 12-row move is 272 binds / 88 ms, a 0.5 px move 176
binds / 66 ms.

## Refuted: addressing the scroll by item position instead of pixels

`GtkColumnView::scroll_to(position, …)` (GTK 4.22, available in gtk4-rs 0.11.4)
was measured as a replacement — teleport by item index, then nudge to centre. It
is **worse**: `scroll_to` alone produces the same 1640 binds
(`jump_binds=1640`), and the centring nudge adds its own on top, taking the
reveal from ~500–570 ms to **900–1080 ms**. The bind avalanche is internal to
GtkColumnView moving its anchor a long way; it is not an artefact of driving the
adjustment directly. `GtkScrollInfo` offers no alignment control either
(`enable_horizontal` / `enable_vertical` only), which is why `scroll_center`
does the centring math by hand to begin with.

Consequence: the bind count cannot be argued away through the GTK API. What
remains is **the cost of a single bind**, and **when** the scroll runs relative
to the first paint.

## Where the bind time goes

Same run, binds attributed per column (`per_column` field):

| Column | Binds per jump | Time | Per bind |
|---|---|---|---|
| text (five columns share one factory) | 1025 | ~190 ms | 0.185 ms |
| title | 205 | ~38 ms | 0.185 ms |
| cover | 205 | ~38 ms | 0.185 ms |
| rating | 205 | ~85 ms | **0.41 ms** |
| **total** | **1640** | **~350 ms** | ~1.7 ms per row |

GTK binds **205 distinct rows** per jump and spends ~300 ms of its own layout
time on top of the ~350 ms of bind closures.

Two concrete wastes found by reading the closures:

- `track_list_columns.rs`, the `connect_bind` inside `append_column` (text bind) evaluates
  `shared_for_bind.filter.borrow().clone()` **and**
  `match_highlight::accent_foreground(&label)` for every searchable cell —
  the latter calls `libadwaita::StyleManager::accent_color_rgba()` and
  `format!`s a hex string. `highlight_markup` (`match_highlight::highlight_markup`) then
  returns `None` immediately when the needle is empty. With no active search —
  the normal case — that is one string clone, one style-manager round trip and
  one allocation per text cell, thrown away. Five of every eight binds are text
  cells.
- `track_list_columns.rs`, the `connect_bind` inside `append_rating_column` calls `set_rating` unconditionally,
  and `set_rating` (`track_list/rating.rs`, `RatingWidget::set_rating`) always runs `refresh()`, rewriting all five
  star glyphs and their CSS classes even when the displayed value did not
  change. It also unregisters and re-registers two per-cell callback registries
  on every bind.

## Plan

### P0 — let the player bar paint before the list scrolls

`update_current_track` currently applies the marker and scrolls in the same main
loop turn, so nothing reaches the screen until the ~650 ms scroll finishes.
Split it: marker now, reveal deferred to a later main-loop turn, so the player
bar, cover and marker land in the first frame after the click.

- Keep NAV-10a intact: the centering still happens, and still without touching
  focus or selection.
- The deferral must survive a second track change arriving in between — a
  superseded reveal must not fire against a stale position. `reveal_track_position`
  already has a retry ladder; the generation guard belongs next to it.
- Do not reintroduce a viewport jump: the marker path stays
  `reapply_now_playing_markers`, never `items_changed` (NAV-10a/NAV-13).
- Acceptance: click → player bar shows the new track drops from ~950 ms to
  roughly the audio's ~350 ms. The stall itself does not disappear here; it
  moves behind the first paint.

### P1 — make a bind cheap

- Text bind: only resolve the needle and the accent colour when a search is
  actually active. Borrow the filter instead of cloning it.
- Rating bind: skip `refresh()` when the rating is unchanged; avoid the
  unregister/re-register pair when the cell is rebound to the same track.
- Every change must keep FIL-5 (all matches highlighted, ASCII-case-insensitive)
  — those tests exist in `match_highlight.rs`.
- Acceptance: measured with the same probe, `bind_ms` for a long jump falls
  well below the current ~350 ms; report the per-column numbers before and
  after. This also speeds up plain scrolling, which pays the same cost today.

### P2 — stop the per-frame display-wide CSS reload

`cover_accent::cross_fade_accent` (`ui/style/cover_accent.rs`) drives
`set_cover_accent` from a `CallbackAnimationTarget`, and `set_cover_accent`
 reloads a provider installed for the whole display. Keep
the cross-fade, but stop making every frame a global style invalidation.

- Acceptance: with animations enabled, the stall no longer carries the ~200 ms
  the animations-off differential exposed; the accent still fades over
  `AMBIENT` (400 ms) and still clears to the theme accent for a colourless
  cover (existing tests in `cover_accent.rs` cover the endpoints).

### Out of scope

The ~300 ms GTK spends on its own layout across 205 rows is internal to
GtkColumnView. Revisit only if the numbers after P1 say it now dominates.

## Reproducing the measurement

The probe scaffold lives on this branch as a separate commit. To re-measure:

    cargo build --release -p reprise-gnome --bin reprise
    # isolated instance, own data dir, own MPRIS name, silent sink:
    REPRISE_SMOKE_MPRIS_BUS_NAME=org.mpris.MediaPlayer2.repriseprobe \
    REPRISE_AUDIO_SINK=fakesink REPRISE_LOG=info \
    XDG_DATA_HOME=… XDG_CACHE_HOME=… \
    WLR_BACKENDS=headless WLR_RENDERER=gles2 cage -- ./target/release/reprise

then drive `org.mpris.MediaPlayer2.Player.Next` over the probe bus name and read
the `PROBE track change breakdown` lines.

## Result (measured after the fix)

Both builds run against the same pristine copy of the library (1822 rows), same
nested headless compositor, same GL renderer, same click cadence, driven through
the same AT-SPI path a user's click takes.

| | before | after | |
|---|---|---|---|
| click → player bar shows the new track | 929 ms | **306 ms** | −67 % |
| main loop unresponsive | 759 ms | 564 ms | −26 % |
| CPU per track change | ~3400 ms | ~2300 ms | −32 % |
| idle main-loop round trip | 5.74 ms | 0.61 ms | −89 % |

The headline number is the first row: the visuals now land at 306 ms, against an
audio switch at ~335 ms. Audio and picture arrive together, which was the
complaint.

Ordering, which is what P0 was actually for: **8 of 8 clicks now stall *after*
the paint** (paint at 290–365 ms, stall starting at 379–724 ms). Before, 5 of 8
stalled *before* it — that is the frozen window the report described.

NAV-10a holds: "current track centered" fires nine times in both runs, and the
generation guard logged no superseded reveal in this cadence (its unit test
covers the overtaking case).

Verified independently of the implementer's report: `cargo test -p reprise-gnome`
— 1323 passed, 0 failed, 392 display-backed tests ignored.

### What is not fixed

The 564 ms stall is still there, just moved behind the first paint. The row-bind
avalanche (P1's target) is reduced but not eliminated — the remaining budget is
GTK's own layout across the 205 rows it binds, which the "Out of scope" note
predicted would dominate once the closures got cheaper. Plain scrolling still
pays it. Re-attributing it needs the probe scaffold rebased onto this branch.
