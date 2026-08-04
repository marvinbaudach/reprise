# UI polish & queue semantics — decisions (2026-07-18)

Collected findings of the session, **checked against the code**. New rules go
into `docs/ux-rules.md` as **section U** (S = STYLE-1, T = network opt-in on
`feat/network-opt-in`). Rule IDs are append-only.

## Scope correction: what is already done

The collected prompt lists six blocks. Four of them have already landed today
and will **not be built again**:

| Requested rule | Actual state |
|---|---|
| SEARCH-2 strip + clamp | ✅ `1694634`, proven by pixel measurement |
| SEARCH-3 chip + teal magnifier | ✅ `b117563` |
| SEARCH-4 two-stage Esc | ✅ `f235d05` |
| QUE-1/2/3/5/6 | ✅ this evening, 9 tasks |
| **ALB-1** persistent playing state | ✅ **GRID-1** `[active]` — EQ badge + 1.5 px inner ring, independent of hover |
| **ALB-2** Enter/Ctrl+Enter/Space/menu key | ✅ **GRID-2** `[active]` |
| **ALB-4** bottom gradient instead of tooltip box | ✅ **GRID-4** `[active]` |
| **ALB-5** scroll to the playing album + pulse | ✅ **GRID-5** `[active]` |
| LYR-2/3 opt-in + activation empty state | ✅ on `feat/network-opt-in` |
| STYLE-1 + RELEASING "floating" test | ✅ `0d6d3079`, section S |
| LYR-Center (centering as such) | ✅ **NPP-6** `[active]` |

The complete album grid block arrived with `5402f34` (feat/keyboard-nav). The
IDs `ALB-1`/`ALB-2` are moreover **assigned differently** than the collected
prompt intended (ALB-1 is replaced, ALB-2 is the album detail view) — the new
rules therefore get their own IDs.

## The three decided reversals

### 1 · QUE-8 — Reorder in the panel, with a sharpened boundary

The old formulation "the panel skims over things, manages nothing" was
dishonest: remove was already allowed in the panel and is likewise a
management verb. The workable boundary is **light vs. heavy verbs**.

- **Panel "Up Next"**: jump, remove, reorder **within the manual section**.
- **ColumnView "Queue"**: multi-select, clear, save-as-playlist, context menu.

Drop targets exist exclusively in "Next in Queue". The "Continuing" section is
not reorderable; a drag from there to the top means "play earlier" and
materializes exactly this one entry into the manual section. Reason: reorder
is the expected gesture exactly where the user is looking — the player bar
icon opens the panel, not the ColumnView. Costs drop targets + autoscroll.

### 2 · LYR-1 — deferred, out of this batch

Local lyrics reading (LRC from tags + `.lrc` sidecar) is new file-format work,
not an opt-in detail. Remains as its own `[planned]` task.

**A consequence that belongs on record:** for v1 this makes the Lyrics tab
**network-only**. With the online toggle switched off it consistently shows
the StatusPage "Enable in Settings" — there is no local path that could
display anything alongside it. The promise "embedded lyrics are always shown"
holds **only once LYR-1 is built** and until then must not appear anywhere in
the UI (that was already correction 1 in the network opt-in plan).
LYR-Center applies independently of this to everything that is shown.

### 3 · STYLE-3 — two accent roles instead of "everything teal"

The original specification ("unify the play button to teal") was wrong and is
discarded. The cream button is `@reprise_player_accent`, that is, the color
extracted from the cover — a feature (NPP-3), not an outlier. Pinning it down
would have undone the dynamic accent.

Correct are **two clearly separated roles**, each consistent in itself:

- **App accent** (fixed petrol, `@accent_color`): durable UI meaning —
  selection, rating stars, toggles `:checked`, links, chips, focus rings.
- **Playback accent** (dynamic, from the cover, `@reprise_player_accent`):
  everything that means the **running track** — play/pause button,
  waveform fill/seek, playing row tint + EQ, now-playing glow, GRID-1 inner ring.

The play button and the waveform are thus correctly cover-colored. The only
real rule: **do not mix per element** — an element belongs to exactly one role.

## New rules (section U)

- **SEARCH-6** — The magnifier and Ctrl+F toggle both ways (show ↔ hide). The
  query is **never** cleared in the process; when hidden with content it lives
  on as a chip (FIL-1) and the magnifier stays in the `:checked` accent.
  *Proven actual state:* `shortcuts.rs:192` calls `set_search_mode(true)` —
  always opens, never closes. A genuine gap.
- **QUE-7** — Up Next = manual queue + **virtual context tail**. The tail is
  not materialized as individual rows but carried as a named section header
  with a count ("Playing from Music · 1,663 tracks").
  Only the visible window is rendered (QUE-6). The sidebar row "Queue"
  counts **only the manual queue**; at 0 it reads "Queue" without a number.
  *Proven actual state:* `window.rs:206` feeds the counter from
  `queue_pending_len()` — as a result the app effectively badges half the
  library as the queue.
- **QUE-8** — Drag reorder exclusively in "Next in Queue" (see decision 1).
- **LYR-4** — The centering of the active line is **clamped to the top**:
  as long as there are not enough context lines above it, the text sits at the
  top and only moves to the middle once enough has gone before.
  *Reason:* NPP-6 centers correctly, but at the start of a song there is
  nothing above it — the upper half of the panel stays empty and reads as a
  layout error.
- **STYLE-2** — A consistent elevation system, defined once and applied
  everywhere: content/table = `.view` (darkest tone), left sidebar and
  right now-playing panel = window `.background` (one level lighter),
  1 px hairlines at the inner edges. No per-pane retinting.
- **STYLE-3** — Two accent roles (see decision 3).
- ~~**FMT-1**~~ — **dropped, already satisfied.** `reprise_core::format::`
  `format_thousands` is a single shared function; it serves
  `status_bar.rs:135`, `browse_filter_strings.rs:59`, `browse_bar.rs:530`,
  `sidebar_presentation.rs:8` and the Up Next footer. There is no second
  path.
  *Correction of a false finding:* An earlier version of this document
  claimed a contradiction between "1,638" (status line) and "1.652"
  (Up Next). That was wrong — the dot notation came from the German
  notation of the requirement, not from the code. The app runs in English,
  where the comma is correct, and both places call the same function. The
  finding had been copied from the specification instead of checked against
  the code.
- **NPP-11** — View tabs centered as an `AdwViewSwitcher` title widget, with
  adaptive degradation in a narrow window (`AdwViewSwitcherBar` at the bottom
  or `AdwInlineViewSwitcher` icons-only via `AdwBreakpoint`).
  *Reverses the earlier left-alignment decision.* The reason back then — a
  rigid center widget symmetrically reserves `2×max(left, right)` and squeezes
  narrow windows — has half fallen away (search is now a SearchBar below the
  headerbar, the middle is free) and is neutralized for the rest by the
  switcher's squeeze capability. The STYLE-1 minimum-width finding thus
  remains covered.

## Text contrast (CONTRAST-1..3)

### Actual state, audited — the finding is sharper than reported

The status line is **not a bar**. `track_content.rs:10-16` builds a
`gtk4::Overlay` and hangs the label via `add_overlay` directly over the
scrolling track list. It has no surface, no background, no container — it
floats over the content.

From this follows the actual defect: the underlying surface is **not
underspecified, but non-deterministic**. Beneath the label, normal rows,
zebra tinting, the selection block, and the playing row tint scroll past in
alternation. The contrast changes while scrolling. A fixed alpha value cannot
guarantee a ratio against a moving background.

Second consequence, proven in the screenshot: `add_overlay` **reserves no
space**. The band lies on the last track row ("Hole Hearted" is cut off)
instead of standing below it. The bottom list row is thereby permanently half
covered — lost content, independent of the scroll position and of any color
choice. The rebuild into a real bar fixes that along with it.

Third correction: the label uses `.dim-label` + `.caption`
(`status_bar.rs:56-57`) — no custom alphas. `.dim-label` is Adwaita's
**normal** secondary level, not the weakest hint level. The aggravating
factor is `.caption`: small type at dimmed opacity. WCAG demands 4.5:1 for
normal text and permits 3:1 only for large text — small *and* dim is the
least favorable case, not merely too low an alpha value.

### Decided rules

- **CONTRAST-1** — Three text levels, defined once, applied everywhere:
  primary ~`0.95` (titles, track names, values), secondary ~`0.7` (artist
  lines, status lines, metadata, column headers), hint ~`0.5` (placeholders,
  hint lines, disabled secondary text). No per-element retinting.
  Where Adwaita named colors fit (`@window_fg_color`, `.dim-label`), use those
  instead of custom alphas — then theme contrasts apply automatically.
  **The level counts together with the font size**: `.caption` + secondary
  needs the same check as hint at normal size.
- **CONTRAST-2** — The status line **first gets a defined surface**, then the
  tone. The order is not cosmetics: as long as the label floats over the
  content, there exists no background color against which 4.5:1 could be
  asserted at all. So out of the `Overlay` and into a real bottom bar with its
  own surface and hairline (STYLE-2), and only after that lift it from
  hint to secondary. Applies identically to all
  "N tracks · duration" footers (library, playlist, queue, album detail) —
  one shared component, one tone.
- **CONTRAST-3** — After the elevation changeover (STYLE-2), cross-check all
  dim texts against their **new** background: status lines, column headers,
  sidebar section labels, meta lines in cards. Where < 4.5:1 → raise to
  secondary or the matching named color.

Test: `contrast_1_secondary_text_meets_ratio` [gtk] measures alpha or the
named color against the surface color, not the rendering — and is only
meaningful once CONTRAST-2 has created a surface color. The final visual
acceptance stays `[manual]` in `RELEASING.md`: visual inspection of the
four footers + sidebar labels.

**Dependency:** CONTRAST-2/3 run *after* STYLE-2, not in parallel with it.
Otherwise the tone gets tuned a second time against a background that is
about to shift again — exactly the mistake that produced the finding.

## NAV-10 — Cross-view context anchor

### Blocker: NAV-5 is not built

**NAV-5 stands at `[planned]`** (`ux-rules.md:113`) — the mode memory for
scroll and selection per view does not exist. NAV-10 part 2 invokes it
verbatim ("on every further switch NAV-5 restores the remembered position").

Without NAV-5 there is no remembered position. Thus **every** entry is a first
entry, and NAV-10 degenerates into hard auto-following on every view switch —
exactly the behavior the premise rules out. The test
`nav_10_subsequent_switch_restores_remembered_position` cannot even be
sensibly formulated beforehand, because there is nothing to restore.

**Consequence: NAV-5 is a precondition, not a neighboring rule.** It is built
first within the same batch and raised to `[active]`; NAV-10 builds on it.

Part of this is a clarification that appears in the collected prompt under
NAV-10's "scope boundaries" but factually **specifies NAV-5**: the scroll
anchor is remembered as **track/album ID + offset**, not as a pixel value, so
that re-sort and insert hold the position (without `scrollIntoView`). That
belongs in NAV-5's text before NAV-5 is implemented — afterwards it is a
rebuild.

### Part 1 ("always marked") is unevenly covered

- **Albums**: ✅ GRID-1 `[active]` — EQ badge + inner ring, independent of hover.
- **Tracks**: marking present (accent row + EQ).
- **Artists**: ❌ ART-1 `[planned]` — "the playing artist shows only a mini EQ"
  is unbuilt.
- **Playlists**: ❌ no rule exists.

In addition: GRID-1 speaks of the "**shared** EQ badge", but there is no
shared component — the only implementation sits in `album_card`
(`album_card_tests.rs:134`), the player bar's mini EQ is a second, separate
path. "One marking language" (ALB-2) therefore demands an **extraction**
here, not merely an application to two further views. That is the actual
effort in part 1.

### Part 3 is already done for albums

The explicit reveal is **GRID-5 `[active]`**: activating the cover or title in
the player bar or panel switches to the album view, clears the search field
and the album filter, scrolls via adjustment (explicitly without
`scrollIntoView`), focuses, and highlights for about 1 s, with NAV-9a as a
fallback. The collected prompt refers to "ALB-5" — that is the old numbering
of the same thing.

What remains open here is only the **artist direction** ("Go to artist") and
the context menu entry point, insofar as it is not already covered by GRID-2.

### Decided rule

- **NAV-10** — Three parts as specified: persistent marking in all
  views (playback accent role per STYLE-3, cover-dynamic);
  auto-scroll to the running context **only on first entry** into a
  view within the session, afterwards NAV-5 restoration without a yank;
  explicit reveal (now-playing cover/title, "Go to album/artist") always
  jumps deterministically. Selection never follows playback; the context of a
  clicked, non-playing song is reachable exclusively via "Go to
  album/artist". The playing marker and the selection highlight remain
  separate treatments in all views.

Tests: `nav_10_first_entry_lands_on_playing_context`,
`nav_10_subsequent_switch_restores_remembered_position`,
`nav_10_playing_marked_in_all_views`, `nav_10_reveal_always_jumps` [gtk].

**Order within the batch:** NAV-5 (incl. ID+offset anchor) → badge extraction
→ ART-1/playlist marking → NAV-10.

## Bugfix without a rule of its own

**Scroll jump on table activation.** A double-click on a row scrolls the
table to the start of the list.

*Cause (diagnosed, not guessed):* `invalidate_window_at`
(`track_list_model.rs:380`) fires `items_changed(position, 1, 1)` and thereby
recreates the focused row widget. GTK's focus restoration then scrolls of its
own accord. For the centering path this is solved (synchronous centering in
the same frame — the comment in `current_track_selection.rs:310` describes
it), but the **suppressed** path returns before that
(`if suppress_scroll { return; }`). Focus thus falls to the start of the list.
That explains why it *always* jumps to the very top instead of to the playing track.

*Fix:* In the suppressed path, save the scroll position before invalidating
and write it back afterwards — that is, not "do not center", but "actively
stay where you were". Regression test with the exact viewport position, not
with "no scroll call" (STYLE-1: for geometry the result is what counts).

## Acceptance

Ctrl+F toggles closed and open, the query survives as a chip · the sidebar
"Queue" shows the manual queue (mostly without a number), Up Next carries the
context as a named row instead of as 1,649 entries · DnD only in "Next in
Queue" · lyrics start at the top instead of in the middle · panel ↔ table
separated by tone + hairline · app accent and playback accent nowhere mixed
in the same element ·
a double-click in the table does not move the viewport ·
status line clearly legible on its own surface, contrast constant while scrolling ·
no dim text below 4.5:1 after the elevation changeover.
