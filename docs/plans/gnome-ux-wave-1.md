---
slug: gnome-ux-wave-1
worktree: /home/marvin/Projects/reprise-gnome-ux-wave-1
branch: feature/gnome-ux-wave-1
phase: planned
codex_session:
created: 2026-10-04
---
# GNOME UX wave 1 — sleep timer and quick open

Two desktop features that came out of the 2026-10-04 UX idea round. A second wave
(Android: undo snackbars, home-screen widget, `MediaLibraryService` browse tree,
#998, the deletion line as an overlay) waits until `after-an-app-update-b` has
landed, because that plan owns all Android Kotlin and `docs/ux-rules.md`.

Dropped from the idea round after checking the code: NAV-2/NAV-3 (already active —
`ui/nav_history.rs`, `player_bar_cover.rs`), podcast resume on the desktop (the
episode row already shows a "Resume · N %" chip, `podcasts_groups.rs:658`),
podcast resume on Android (Android has no podcasts), #1018 and #1055 (fixed by
#1062 and #1064, issues still open).

## Decisions already taken (2026-10-04)

- Wave 1 is GNOME-only. Android follows in wave 2.
- The Android deletion line becomes an overlay (wave 2).
- The "listen again" start surface gets its own brainstorm later, not this wave.
- Android Auto (wave 2): `MediaLibraryService` with playlists, albums, artists and
  recently played.

## Grill decisions (2026-10-04)

1. Code now, in parallel with refactor-wave 2026-10. This wave only *calls* APIs
   that wave owns; whichever lands second adapts the call sites.
2. The sleep timer **pauses** (not stops) after the fade and restores the volume.
3. Quick open is bound to **Ctrl+K**; no header button.
4. Quick open is a floating `AdwDialog` panel in the upper third of the window,
   ~560 px wide, no header bar; it becomes a bottom sheet on narrow windows.
5. Enter on a track plays it **in its album context** (queue = album from that
   track); no album → artist context; neither → the track alone. Alt+Enter = Play
   Next.
6. Sources: tracks, albums, artists, playlists, podcast **shows**, radio stations.
   No episodes, no YouTube, no releases.
7. Code now, but **merge only after `after-an-app-update-b` has landed**, with
   PLAY-SLEEP-1 and SEARCH-16 added to `docs/ux-rules.md` in the same PR and the
   display tests carrying those rule IDs. If that plan stalls, fall back to a
   follow-up rules PR.

## Part 1 — Sleep timer (G1)

### Behaviour (mirrors Android's `SleepTimer.kt`, so both apps say the same thing)

- Choices: 15, 30, 45, 60 minutes, **End of track**, and Cancel while armed.
- When the timer runs out, the volume fades over 4 s (8 steps), then playback
  **pauses**, not stops, and the volume is restored to its prior
  value, so the next Play is not silent.
- End of track: arms on the current item. On `TrackFinished` the queue does **not**
  advance. On `AdvancedToNext` (gapless hand-off — the next track is already
  playing) playback pauses at once and the queue position stays on the new item.
  The fade starts 4 s before the end, from the position updates.
- Any manual track change while "End of track" is armed re-arms on the new item
  (same as Android: the timer follows "this track", whatever that now is).
- The timer is session state only. It is never persisted (no `settings*` writes —
  those files are owned by refactor-wave B anyway).
- Applies to every playback mode, including queued episodes and radio. Radio has no
  end, so "End of track" is disabled while a radio stream plays.

### Surface

- A moon icon button (`weather-clear-night-symbolic` — verify it exists in the
  installed theme and add it to `ui::icons` `GUARDED` with a fallback) in the
  player bar's end zone, left of the volume button.
- Click opens a `PopoverMenu` built like `ui/player_bar/seek_menu.rs`.
- While armed, the button carries the `:checked` accent style, and its tooltip
  shows the remaining time ("Pauses in 23 min" / "Pauses after this track").
  No badge dot (dots are reserved for the request role, FB-4).
- Firing shows a toast "Paused by sleep timer". No other feedback.
- MPRIS is unaffected beyond the normal paused state.

### Code shape

- `crates/reprise-view/src/sleep_timer.rs` (new): the pure state machine —
  `SleepTimer { Off, Minutes { deadline }, EndOfTrack { item } }`, plus
  `tick(now, position, duration) -> SleepAction { None, SetVolume(f64), Pause }`
  and `on_track_finished`/`on_gapless_advance`/`on_manual_change`. Unit-tested
  with an injected clock. No glib, no timers. This is the module Android may later
  bind to through the FFI (not in this wave).
- `crates/reprise-gnome/src/ui/player_bar/sleep_timer_button.rs` (new): the
  button, the popover, the tooltip and the `:checked` state. `player_bar.rs` is at
  796 lines — do not grow it; `player_bar_layout.rs` (582) gets the one packing
  line.
- `crates/reprise-gnome/src/ui/window/wiring/sleep_timer.rs` (new): owns the
  `Rc<RefCell<SleepTimer>>`, a 1 s `glib::timeout_add_local` while armed, and
  calls `toggle_pause()`/volume through the existing `PlayerController` API.
- `crates/reprise-gnome/src/ui/playback/player_event_handling.rs` (590 lines): the
  `TrackFinished` and `AdvancedToNext` arms consult the timer before advancing.
  This is the one non-additive change in the playback path; keep it a single
  early-return guard per arm. The file is NOT in refactor-wave C's list.
- No dedicated `pause()` exists; a missing volume setter on `PlayerController` must
  not be added to `player_controller.rs` (owned by refactor-wave C). If the fade
  needs it, put the method in a new `playback/sleep_timer_hooks.rs`
  `impl PlayerController` block.

## Part 2 — Quick open (G3)

### What it is, and what it is not

Search today is deliberately a **per-section filter** (Q. SEARCH-1a…15): the lens
popover narrows the current list in place. Quick open is a different tool: a
**jump-to** palette across all sources that navigates or plays, and never filters.
It must not change any SEARCH-* behaviour.

### Behaviour

- Shortcut: **Ctrl+K**. Also listed in the shortcuts dialog. No header button.
- A floating `AdwDialog` panel in the upper third of the window, ~560 px wide,
  no header bar, with one entry and one result list. It reflows nothing; on narrow
  windows it becomes a bottom sheet.
- Sources, grouped in this order, at most 5 rows each, then "Show all N in
  <section>": Tracks, Albums, Artists, Playlists, Podcast shows, Radio stations.
  No episodes, no YouTube, no releases. Only enabled modules contribute
  (`modules::is_enabled`). No network queries — local library and cached
  metadata only.
- Matching: case- and diacritic-insensitive prefix-of-word match, the same
  folding the filter bar uses; ranked by exact > prefix > word-prefix, then by
  play count.
- Enter / click:
  - Track → play in its album context (queue = the album from that track, as a
    double-click in the album view); no album → artist context; neither → alone.
  - Station → play (same path as in the radio view).
  - Album, artist, playlist, podcast show → navigate there (`route_to_place`,
    recorded in NAV-2 history).
  - "Show all N in <section>" → navigate to that section and hand the query to
    its existing search popover as a committed chip (SEARCH-12 path).
- Alt+Enter on a track: Play Next (`QueueAddNext`) instead of play now.
- Esc closes and returns focus to where it was. Up/Down move across groups;
  Tab is not trapped.
- Empty query shows the last 8 opened items from this session (in memory only).
- Accessible: the list is a `GtkListView` with `AccessibleRole::ListBox`, each
  row labelled "<kind>: <title>, <subtitle>".

### Code shape

- `crates/reprise-view/src/quick_open.rs` (new): result model, grouping, ranking
  and the per-kind action enum. Pure; tested with fixture rows.
- Data: read through existing `reprise-core` read APIs only. **Do not edit
  `reprise-core/src/queries/**`** — refactor-wave A owns it and is replacing
  positional overloads with parameter objects. If wave A lands first, use the new
  signatures; if a needed query does not exist, stop and report rather than add
  one.
- `crates/reprise-gnome/src/ui/window/quick_open.rs` (+ `quick_open_row.rs`) (new):
  the panel widget.
- `crates/reprise-gnome/src/ui/window/wiring/quick_open.rs` (new): registers
  `win.quick-open` and Ctrl+K following `wiring/nav_back.rs`; the search runs on
  a worker thread with a generation token, so a stale result never paints over a
  newer query.
- Do not touch `ui/shortcuts.rs` (775 lines) beyond the shortcuts-dialog entry; if
  that entry would push it past 800, put it in a sibling.

## Shared files

`ui/window/wiring/mod.rs` (one `mod` line and the ordered-calls test at :36-67) and
`ui/window/window_runtime_wiring.rs` (two calls) are touched by both parts — this is
why the wave is a single strand.

## UX rules (same PR, after `after-an-app-update-b` lands)

`docs/ux-rules.md` is owned by `after-an-app-update-b`. Codex does NOT edit it.
Before landing, rebase onto the dev that plan produced and add these rules in the
same PR (Opus writes them; they are part of the landing step):

- **PLAY-SLEEP-1** [proposed] [gtk] — sleep timer choices, fade, pause, end-of-track
  semantics, session-only state.
- **SEARCH-16** [proposed] [gtk] — quick open is jump-to, not filter; Ctrl+K; groups
  and actions as above; never alters the section search state except through
  "Show all".

Codex names the display tests `play_sleep_1_*` and `search_16_*` from the start,
so the rule records only have to be added, not the tests renamed.

## Tests

- `reprise-view`: sleep timer state machine (each arm, re-arm on manual change,
  radio disables end-of-track, fade curve), quick-open ranking and grouping.
- GNOME display tests (`#[ignore]`, headless recipe from AGENTS.md):
  - moon button toggles `:checked` when armed and clears on cancel;
  - `TrackFinished` with end-of-track armed pauses and does not advance;
  - Ctrl+K opens the panel, typing an album name and Enter navigates to the album
    and records NAV-2 history; Esc restores focus.
- Gates: fmt, clippy `-D warnings`, `cargo test --workspace`, `cargo audit`,
  `scripts/check-architecture.sh`, `scripts/check-gnome-idioms.sh`, icon guard test.

## Parallelität

**Cannot be cut.** Both parts must register in `ui/window/wiring/mod.rs` (including
its ordered-calls test) and `ui/window/window_runtime_wiring.rs`; two strands would
intersect on exactly those files. One strand, two task groups in order: sleep timer
first (smaller, proves the wiring), then quick open.

**Merge-order dependencies on other plans:**
- refactor-wave A (`queries/**` signatures): quick open reads through them.
  Whichever lands second adapts.
- refactor-wave C owns `playback/{player_controller,queue_transport,mod}.rs` and
  `window/{window,library_shell,mod}.rs`. This wave edits none of them; it only
  calls `route_to_place` and existing controller methods.

**Post-merge cross-checks:**
1. Gate for landing: `after-an-app-update-b` has landed; rebase, add PLAY-SLEEP-1
   and SEARCH-16 to `docs/ux-rules.md` in this PR.
2. After refactor-wave C lands: re-run the sleep-timer display tests (C moves
   playback seams that the timer calls into).
