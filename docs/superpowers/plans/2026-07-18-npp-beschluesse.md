# Now-playing panel — decision document (grilling 2026-07-18)

Normative context for the rebuild of the right column into the now-playing
panel per design **21** (binding version, consolidated from 10b + screenshot
review), frames **21a** (lyrics tab) and **21b** (Visual tab). Reference for
the Artist News follow-up task: frame **22a**. The design source is the
claude.ai/design project "Audio-Player für große Bibliotheken" (share link,
canonical; PDFs in `docs/design/` are not authoritative).

> **Rulebook note (done 2026-07-18):** The transfer has happened —
> **section P of `docs/ux-rules.md` is normative from now on** (NPP-1–10, all
> `[active]`, every rule covered by a rule-named test and enforced by
> `scripts/check-ux-traceability.sh`). The rule version below remains as the
> *historical* wording of the grilling; where they diverge, the rulebook wins.
> This document is the **decision ledger**: it holds the *why* and the detail
> decisions that lie below the rule level — for those there is no room in the
> rulebook.

## NPP rules (historical grilling wording — ux-rules.md § P is normative)

### Structure

- **NPP-1 · Geometry** `[active]` — Panel fixed at **300 px** (the left
  sidebar fixed at **240 px** — deliberately unequal), collapsible with the
  same slide transition as the left sidebar (MOT-3, Standard token 250 ms
  ease-out-cubic; the existing `OverlaySplitView` transition provides it).
- **NPP-2 · Vertical layout** `[active]` — Cover **168 px** (radius 12,
  shadow + 1 px inset hairline) → title 15 px bold / "Artist · Album" 12 px
  white 55% → **pill toggle** Up Next | Lyrics | Visual (segments, no
  tab-bar widget) → tab content fills the rest → footer 10.5 px white 35%
  (content per tab, see decisions 3/9). **No volume control in the panel** —
  volume lives exclusively in the player bar (P-1).
- **NPP-3 · Glow instead of full tint** `[active]` — Radial gradient from the
  cover accent color (existing extraction pipeline) behind the cover — upper
  third only (~300 px ellipse, fading out softly, opacity ~0.4), running down
  into neutral panel-dark. The base background stays neutral so that the
  lyrics contrast is constant. Fallback petrol (= theme accent,
  decision 8). Rendered once as a gradient/texture, no live blur.
- **NPP-4 · Tab memory** `[active]` — The selected tab is retained within the
  session (NAV-5), restart = Up Next. The previous persistence of the panel
  tab across restarts (`info_panel_tab` setting) is **deliberately** dropped;
  the persistence of panel *visibility* remains.

### Synced lyrics (lyrics tab)

- **NPP-5 · Line styling** `[active]` — Active line 15 px bold white +
  accent underline (26×2.5 px, centered, color = cover accent).
  Neighboring lines stepped: ±1 → white 45%, ±2 → 32%, further → 28%. All
  lines centered, 13 px, generous spacing (~13 px gap). Whole LRC lines, no
  karaoke word highlighting.
- **NPP-6 · Line-change motion** `[active]` — On a timestamp change the new
  line fades 45% → white+bold, the old one back (Micro token
  150 ms ease-out); at the same time the list scrolls the active line to the
  center (Standard token, ease-out-cubic — no spring). The underline does not
  travel — it belongs to the active line and fades with it.
- **NPP-7 · Manual scrolling wins** `[active]` — A user scroll pauses
  auto-scroll for 4 s (the timer resets on every user scroll event);
  afterwards the list glides back to the active line. During the pause the
  active line keeps receiving its highlight (only the scroll is absent). A
  user scroll also aborts a running glide-back and restarts the 4 s;
  programmatic scrolls never reset the timer.
- **NPP-8 · Click = seek** `[active]` — Clicking a line seeks to the
  timestamp (synced only). Hover: white 65% + pointer. The only click
  interaction in the lyrics tab; the lyrics text is not selectable.
- **NPP-9 · Fallbacks** `[active]` — Unsynced → static scrollable text
  (13 px, white 65%), no highlight, no auto-scroll, footer "lyrics · tags".
  No lyrics → subtle empty state ("No lyrics found", no search CTA in v1)
  with **inline retry** on errors (decision 9). Instrumental gap
  (>10 s without a line) → the active line stays, dims to 60%.
- **NPP-10 · Track change** `[active]` — Cover, title block, glow, and tab
  content crossfade together (Standard token, MOT-5); lyrics start on
  line 0, centered. No slide — a track change is not a change of place.

### Behavior & edges

- A seek (waveform or lyrics click) jumps auto-scroll immediately to the
  new active line (no 4-s timer).
- Pause freezes the highlight; play picks it up again. A paused track counts
  as loaded — the panel keeps showing it. The panel **always** follows the
  playing/loaded track, never the library selection.
- `gtk-enable-animations=false`: all NPP motions become hard switches (MOT-7,
  centrally via `ui/motion.rs`).
- The shared head (cover/glow/title/toggle) is one widget, the tab contents
  change beneath it.

## Grilled decisions (2026-07-18, all confirmed)

1. **Artist News moves out** — The Information tab is dropped. Artist News
   belongs to the artist, not to the playing track: reconnected as a section
   in the artist detail view per frame **22a** (its own follow-up task;
   section only when there are entries, exactly one accent release card,
   ⟳ TIP-compliant, cache age instead of an error banner). This task
   **only decouples**: worker, cache, and settings are kept untouched. No
   loss of information for the track metadata — codec/bitrate/path live in
   the tag editor and in the columns respectively.
2. **Scope of this iteration** — Shared head + lyrics tab (NPP-5..10) +
   Up Next tab. The **Visual segment only appears with the visualizer
   follow-up task** (21b): the Labs rule "plugin disabled → segment
   disappears from the panel" makes the two-segment pill design-compliant
   as long as the plugin does not exist.
3. **Up Next tab** — Only **upcoming** tracks (the playing one hangs large
   in the head — no duplication, P-1). Rows in 21a style (cover 32 px, title
   13.5 px, artist dim). **Click = jump** to that queue entry: an explicit
   user action (PLAY-5-compliant), skipped entries stay in the queue
   history — the panel manages nothing (no reorder, no removal; the queue
   view can do that). Empty: a subtle "Queue is empty".
   Footer: "n tracks · remaining duration".
4. **Idle state** (no loaded track) — placeholder cover without glow, title
   line subtly "Nothing playing", tabs remain usable (an Up Next click
   starts playback). Nothing opens or closes on its own.
5. **Light theme** — In both color schemes the panel remains the **dark
   stage** (a fixed neutral-dark ground like a player canvas): one
   alpha set, constant lyrics contrast, the glow always works.
6. **Geometry** — Left fixed at 240 px (replaces 220–280 px/22% fraction),
   right fixed at 300 px (replaces 340 px). Responsive collapse (<800 px)
   stays.
7. **14a surface hierarchy** — Its own narrow parallel task (branch
   `feat/theme-surface-hierarchy`): left sidebar one level lighter than the
   table, headerbar one level above it, 1 px hairlines — in all three
   dark themes; the light palettes already have the hierarchy.
8. **Fallback accent = theme accent (petrol)** — `player_accent` is set per
   theme to the theme accent; the static orange (#e8703a) is dropped.
   Applies uniformly to the play button, waveform, glow, underline
   and, later, Visual. Implemented in the parallel task because of file
   ownership (`theme.rs`, `cover_accent.rs`) (decision 7); a status flip is
   not possible there — it applies with the merge of
   `feat/theme-surface-hierarchy`.
9. **No panel header** — ⟳/× are dropped without replacement (faithful to
   mock 21a). Closing only via the app header toggle (which continues to
   persist visibility), the lyrics retry moves into the error state of the
   lyrics tab as a subtle inline button.

## Self-made decisions (implementation level)

- Remove the `info_panel_tab` setting along with its getter/setter in
  `reprise-core` (NPP-4); no migration needed — an orphaned row in
  `settings` is harmless.
- Removing the volume footer as required by the dictate is a no-op against
  the code (the panel never had one); rule NPP-2 is secured by a structure
  test.
- Glow as a CSS radial gradient on the head widget (set once per track), no
  real-time blur — satisfies "render once as a texture".
- The visualizer follow-up task takes over: spectrum pipeline, presets
  Rings/Flow/Pulse, F11 fullscreen, Labs plugin switch (21b), fade-out
  on pause, idle static-minimal, MOT-2 justification (the only continuous
  motion, only in this tab).

## Follow-up tasks (not this branch)

| Task | Reference | Content |
|------|----------|--------|
| Artist News in the artist detail view | Frame 22a | News section below Top Tracks, release card, "Remind me" `[planned]` |
| Audio visualizer (Visual tab) | Frames 21b/10b/10c | GPU patterns, presets, F11, Labs plugin; the segment only appears with it |
| ~~NPP rules → ux-rules.md section P~~ | this document | **done 2026-07-18** — section P, NPP-1–10 `[active]`, 59 rules traceable |
| ~~14a surface hierarchy + petrol~~ | Decisions 7/8 | **done 2026-07-18** — `feat/theme-surface-hierarchy` merged to main |
