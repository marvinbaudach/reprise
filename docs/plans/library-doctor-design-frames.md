# Library Doctor — design frame reference

> Source: Claude Design project `568f5576-9f1b-4214-bfa4-3e7c2e911712`, file
> `Library Doctor.dc.html` (721 lines, read in full 2026-08-05). Nine frames in
> flow order — seven numbered frames (1, 2, 3, 4, 5, 6, 7) plus two detail
> crops (1b, 3b) that zoom into one element of a neighboring frame. All UI
> copy below is transcribed verbatim in English; only frame purposes and
> section headers are translated/paraphrased from the mockup's German
> figcaptions. Companion document: `library-doctor-redesign-brief.md` in this
> same folder, which specifies the implementation; this file specifies what
> the mockup actually shows, frame by frame.

## Shared app chrome

Frames 1, 2, 3, 3b and 4 are full application screenshots (1240×780) and
repeat the same three chrome regions. Documented once here; each frame
section below only calls out what's specific to that frame plus deltas from
this baseline.

**Title bar** (46px tall): two small decorative dots on the far left (window
controls, both muted, no color coding). Then, depending on frame, a back
(`‹`) arrow. Page title, centered. Then a row of icon buttons on the right:
a `⋮` overflow-menu icon, and up to three more (a "spark"/AI icon, search,
panel-toggle) — the exact icon set and back-arrow presence varies between
frames; see "Open design questions."

**Left sidebar** (204px wide, fixed): top-to-bottom —
- `LIBRARY` section label, then rows: Music (1,818), Podcasts (45), YouTube
  (69), Radio (2), Queue (no count).
- `PLAYLISTS` section label with a small square add button on its row, then
  playlist rows, each with a count on the right (e.g. "Lorna Shore & S…"
  200, "Deathcore 2026" 84, "Late night" 37). Present in some frames, absent
  in others (see open questions).
- `SMART` section label, then: Recently played (50), Top rated (95),
  Recently added (67), Releases (653), Concerts (27), My Stats (no count).
  Not every frame shows every row.
- `ISSUES` section label, then: Missing files (no count) always; a
  `Library Doctor` row with a count badge appears here only once a
  completed scan has unreviewed findings (frame 3b) — this is the sidebar
  "noun" entry described in the brief, not a permanent tab.
- Whichever nav row matches the page currently open gets a subtle filled
  background (e.g. "Releases" is highlighted while the Releases page is
  showing).
- During an active scan, a progress card is pinned to the very bottom of
  the sidebar (see Frame 3).

**Bottom mini-player** (62px tall, present in every full-screenshot frame,
content is incidental/unrelated to Library Doctor): 42×42 album-art swatch +
track title "Lunarhaze" / release "What Lies Below", centered transport
controls (shuffle, previous, play/pause in an accent-ringed circle, next,
repeat), elapsed/remaining time, a dashed waveform/scrubber bar, volume
icon. Timestamps differ per frame only because each screenshot was taken at
a different simulated moment — not meaningful.

---

## Frame 1 — Entry point (`⋮` menu)

**Purpose:** Shows where Library Doctor is launched from. Per the mockup's
own annotation, this is the *only* entry point — Preferences no longer
carries the feature.

**What's on screen:** the Releases page in the background (a data table of
upcoming/missing album releases), with the global `⋮` overflow menu open as
a floating panel over it.

**Background page (Releases), top to bottom:**
- Toolbar row: "+ Add filter" with a caret/dropdown chevron on the left;
  "653 gaps" (muted) on the right.
- One column header row for the whole table: `Date | Title | Artist | Type
  | Status`. Grid weighting: Date 96px fixed, Title flexible (`1fr`, gets
  all remaining space), Artist 260px fixed, Type 76px fixed, Status 96px
  fixed.
- Data rows (14px, ~11px vertical padding each — a tight/compact row
  height), each with a status pill on the right: accent-colored `upcoming`
  pill or neutral `Missing` pill (one row instead shows `1 of 4`, i.e. a
  partial-availability status). Visible rows: DEATHRACE / Rising Insane /
  Album / upcoming; Where The Light Begins To Fade / If Not for Me / Album /
  upcoming; TANZNEID / Electric Callboy / Album / upcoming; EXES / Sorry X /
  Album / Missing; Decades / Motionless In White / Album / Missing; Fables,
  Vol. 1 / Galleons / Album / Missing; Make Me Believe / The Devil Wears
  Prada / EP / Missing; Rat Torture / Distant / EP / "1 of 4". The first row
  has a subtle accent-tinted rounded background — reads as a
  hover/focus/just-clicked state on that row, not a persistent style.

**The open `⋮` menu** (floating card, 318px wide, positioned top-right,
rounded, drop shadow):
1. "Compact Mode" — with "Ctrl+M" right-aligned.
2. Divider.
3. "Rescan Library" (plain row).
4. **"Library Doctor"** — this row is visually distinguished from its
   neighbors: accent-tinted background, a thin accent border, and an
   accent-colored first-aid-kit icon. This is the row the frame exists to
   point at.
5. Divider.
6. "Preferences" — "Ctrl+," right-aligned.
7. "Keyboard Shortcuts" — "Ctrl+?" right-aligned.
8. "Help" — "F1" right-aligned.
9. "About Reprise".

There is no "Sync Device…" item and no submenu/badge on "Library Doctor" —
it's a single flat row like every other item in the list.

---

## Frame 2 — Start page

**Purpose:** the Doctor's own landing page. Scope selection, the
network-lookup toggle, the run action, and — new versus the old
design — the revert action, all live here instead of in Preferences.

**Title bar delta from shared chrome:** title reads "Library Doctor"; no
back arrow is drawn in this particular frame (see open questions); icon set
is `⋮`, search, panel (no spark/AI icon here).

**Sidebar delta:** no nav row is highlighted (Library Doctor has no
permanent sidebar entry to highlight).

**Main content**, left-aligned column, max width 620px, generous outer
padding (56px top/bottom margin region, 64px sides):

1. First-aid-kit icon (accent color), 30×30, sitting above the heading with a
   16px gap under it.
2. Heading: **"Check your library"**
3. Body paragraph: *"Reprise fixes what is unambiguous — stray spaces,
   casing, missing MusicBrainz IDs — and asks you about the rest. Everything
   it does can be undone in one step."*
4. Label "Scope" (small, muted) above a **segmented control** (a single
   connected pill-bar, not standalone chips) with three mutually exclusive
   options: **"Whole Library"** (selected by default), "Current View",
   "Selection".
5. A surfaced card row (rounded, padded, background one step lighter than
   the page): left side is two lines of text — "MusicBrainz / AcoustID
   suggestions" (14px) and, under it, *"Optional network lookup · local
   fixes are always included · no file paths or private library data"*
   (smaller, muted); right side is a **toggle switch, default ON**.
6. Action row: **"Run Scan Now"** — primary button — next to a muted
   caption **"1,818 tracks · about 2 minutes"** (the full Music library
   count and a rough duration estimate, shown before the real scannable
   count is known).
7. Below a horizontal divider, a "last scan" block: two lines — **"Last
   scan · 3 days ago"** and, under it, **"1,017 fixes applied · still
   reversible"** — with a **"Revert Last Cleanup"** secondary button
   (undo icon + label) at the right edge of the same row.

Vertical rhythm inside the column is roughly even (~26px gaps between the
header block, the Scope control, the network-toggle card, and the
run-button row), with a slightly larger separation (divider + ~34px of
combined margin/padding) before the last-scan/revert block at the bottom —
i.e. the revert block reads as a distinct, secondary footer area, not part
of the primary flow.

---

## Frame 1b — Playlist quick-add (detail crop)

**Purpose:** a zoomed-in crop of the sidebar's Playlists section, showing
the new "click `+`, type the name immediately" interaction (an unrelated
small change bundled into this same mockup — see the brief's final
section). Not part of the Doctor flow itself, but transcribed here for
completeness since it's one of the nine frames.

**What's shown**, a narrow sidebar-width panel (232px):
1. "PLAYLISTS" label with an **accent-filled `+` button** on the same row
   (contrast with Frame 1's sidebar, where the same button is a plain
   outlined/neutral square — here it reads as focused/active).
2. A brand-new playlist row, mid-creation: a muted arrow icon, then the
   text **"Untitled playlist"** shown with an accent-tinted highlight
   behind it (as if the whole name is selected/marked, ready to be typed
   over) and a thin blinking-cursor bar right after it. The row itself has
   a thin accent inset border, i.e. it reads as focused/being-edited.
3. Below it, the existing playlists continue normally: "Lorna Shore & S…"
   (200), "Deathcore 2026" (84), "Late night" (37).

There's no dialog, no separate "New playlist" or "Import playlist…" rows —
per the caption, "Import playlist…" moves into the `⋮` menu instead.

---

## Frame 3 — Scan running

**Purpose:** shows the scan as a background job. The progress affordance
lives in the sidebar (shared with the library scan and the missing-files
relink), not as a full-page spinner; meanwhile the Doctor page itself fills
in with live counts.

**Title bar delta:** back arrow present; title "Library Doctor"; icon set
is `⋮`, panel only (no search, no spark icon in this frame).

**Sidebar delta — the progress card**, pinned to the bottom of the
sidebar, inside an accent-tinted rounded box with a thin accent border:
- Row: a small spinning ring icon, then **"Checking tracks…"** (truncates
  with an ellipsis if it doesn't fit), then **"45%"** (muted) and a
  **"Cancel"** link (accent-colored text) at the far right.
- A thin progress bar underneath, track in a muted tone, fill in accent,
  currently at 45% width.
- A caption line below: **"742/1,648 tracks"**.

**Main content**, max width 700px:
1. Heading: **"Results Found So Far"**
2. Muted subline: **"Whole Library · MusicBrainz on · this job continues in
   the background"**
3. **Block 1** (card, dimmed to ~85% opacity — reads as "still settling,
   not final"): check icon (accent) · **"511 fixes applied so far"** (16px,
   medium weight) · two sub-lines underneath, **"96 stray spaces and
   casing corrections"** and **"415 MusicBrainz IDs filled in"** · an
   **"Undo" secondary button, disabled**, top-right of the block.
4. **Block 2** (card, same dimmed treatment): first-aid-kit icon in a muted
   (not accent) tone here · **"39 changes waiting for you"** (16px,
   medium) · one sub-line, **"across 14 albums so far"** · a
   **"Review" primary button, disabled** (note: no count in the label
   yet — compare Frame 4, where the equivalent button reads "Review 88
   changes" once the scan is done).
5. Footer line, small and muted: a warning icon plus **"Locked while a
   Library Doctor job is running"**.

Both cards are non-interactive while the job runs; their numbers are meant
to be read as live counters, climbing toward the Frame 4 totals.

---

## Frame 3b — Scan finished while elsewhere (detail crop)

**Purpose:** the "you were on a different page when the scan finished"
case. No modal, no forced navigation. The sidebar progress card
disappears; a persistent `Library Doctor` row appears under `ISSUES`
instead, and stays until the findings are resolved. A toast fires, but only
for the fixes that were applied without asking.

**What's shown:** the Releases page again (same table as Frame 1, same
"653 gaps" toolbar, same eight data rows — this time without the accent
hover-tint on the first row).

**Sidebar delta:** under `ISSUES`, below "Missing files", a new row:
**"Library Doctor"** with a **count badge "88"** on the right, both in
accent color, the row itself accent-tinted with a thin accent border — the
same visual treatment the entry-point menu item had in Frame 1.

**Floating toast**, bottom-center of the screen (a rounded pill, drop
shadow, floating above the mini-player):
- Text: **"1,017 tags fixed"**
- **"Undo"** ghost button
- A small circular **✕ close button** at the far right.

The toast's number (1,017) is exactly the "quiet fixes" total — the ones
applied automatically without review — matching Frame 2's "last scan"
figure and Frame 4's "fixes already applied" block. It does not mention the
88 changes still waiting; those only surface via the new sidebar row.

---

## Frame 4 — Result / summary

**Purpose:** the scan-complete summary. Three meaning-distinct blocks
(done / needs a decision / optional-and-skippable) instead of a flat list
of category rows.

**Title bar delta:** back arrow present; title "Library Doctor"; icon set
`⋮`, panel only.

**Sidebar delta:** back to the plain nav, no highlighted row, no Doctor
badge visible in this particular sidebar snippet.

**Main content**, max width 700px, ~24px vertical gap between blocks:

1. Heading: **"1,648 tracks checked"**
2. Muted subline: **"Whole Library · MusicBrainz on · 7 skipped (unreadable
   files)"**
3. **Block 1 — quiet fixes** (card, shown only when there were any; the
   mockup marks this block with a `quietFixes` boolean toggle, default
   `true`, i.e. it's meant to be omitted entirely when the count is zero):
   check icon (accent) · **"1,017 fixes already applied"** (16px, medium)
   · two sub-lines, **"208 stray spaces and casing corrections"** and
   **"809 MusicBrainz IDs filled in — no visible change to your tags"** ·
   an **"Undo" secondary button** (enabled now).
4. **Block 2 — needs review** (card with a full accent-colored outline
   ring, i.e. visually the most emphasized of the three): first-aid-kit icon
   in full accent (not muted, unlike the equivalent block in Frame 3) ·
   **"88 changes need your eye"** (16px, medium) · three sub-lines,
   **"64 casing and whitespace Reprise would not risk on its own"**,
   **"24 missing or wrong release years"**, **"across 31 albums"** · a
   **"Review 88 changes" primary button**.
5. **Block 3 — spelling conflicts** (dashed-border card, no fill; also
   toggle-gated, `showConflicts` boolean, default `true`, meant to
   disappear when there are none): warning icon (muted) ·
   **"3 spelling conflicts, no clear winner"** (14.5px) · one line under
   it, **"Waiting at the end of the review list. Skippable — nothing
   breaks if you leave them."**
6. Footer row: **"Scan again"** ghost button, plus a muted caption
   **"Results are kept until the next scan."**

All three block headline numbers are internally consistent: 208 + 809 =
1,017; 64 + 24 = 88.

---

## Frame 5 — Review

**Purpose:** the main work surface. Grouped by album; one shared column
header for the whole page instead of one per row; no MusicBrainz IDs shown
at all (those are in the "quiet fixes" bucket, never surfaced here).

This frame is a dedicated full-width page (1240px wide, height not fixed —
content-driven/scrollable), not a full app screenshot: no sidebar, no
mini-player. Just a title bar, a filter toolbar, a shared column header,
the grouped list, and a sticky footer.

**Title bar:** two decorative dots on the left, centered title **"Review
changes"**, then two small secondary buttons on the right: **"All"** and
**"None"** (bulk select/deselect for the whole list).

**Filter toolbar** (below the title bar, on a slightly darker strip):
- **"88 changes · 31 albums"** (14px)
- Muted helper text: **"Everything here is preselected. Uncheck what you
  disagree with."**
- Pushed to the far right: a **segmented control** (same connected
  pill-bar component as Frame 2's Scope selector) with four options —
  **"All"** (selected by default), "Casing", "Year", "Genre".

**Shared column header** (one instance for the entire page, sticky above
the list, uppercase/small/muted labels, bottom border): 8 columns —
`[blank] Track | Field | Current | [blank] | Proposed | Source | [blank]`.
Column width ratios: checkbox spacer 26px, Track 210px, Field 116px,
Current `1fr`, arrow spacer 22px, Proposed `1fr` (Current and Proposed
split the remaining space evenly), Source 132px, edit-icon spacer 30px.
The three blank columns hold, respectively, the row checkbox, the "→"
arrow glyph, and a pencil/edit icon — none has a text label, ever, since
the header for those is implicit in the icon itself.

**List body**, grouped by album. Each group is: an album header row
(flex layout, not the 8-column grid) followed by one 8-column grid row per
change. Groups are separated by a top border plus extra top margin/padding
(~28px combined) — a clearly "wide" gap. Rows within a group sit directly
adjacent with no extra gap between them (~9px vertical padding each only)
— a "tight" rhythm, reinforcing that they belong to one album.

**Album header row shape:** `[group checkbox] [38×38 cover placeholder,
rounded, disc icon] [album title, 15px medium] [artist · N tracks, 13px
muted] ...(pushed right)... [count pill, e.g. "4 changes"] [caret icon]`.
Cover art is a fixed 38×38px square placeholder (rounded corners, dark
fill, disc glyph) — no real artwork is shown for any album in this frame.

**Example groups shown, in order:**

1. **"Count Your Blessings"** — Bring Me the Horizon · 11 tracks · group
   checkbox **checked** · pill "4 changes" · 4 rows:
   - `All 11 tracks` (italic, muted — album-level, not one specific
     track) | Field "Album artist" | Current "Bring Me the Horizion"
     (strikethrough — note the typo, this row exists to demonstrate a
     casing/typo fix) | → | Proposed "Bring Me the Horizon" | Source
     **"Local"** (accent-colored text, no icon) | edit pencil
   - `All 11 tracks` | Field "Year" | Current **"— empty —"** (muted, no
     strikethrough — nothing to strike since there was no prior value) |
     → | Proposed "2006" | Source **"MusicBrainz · 98%"** (plain muted
     text, no icon) | edit pencil
   - Track "Pray for Plagues" | Field "Title" | Current "pray for
     plagues" (strikethrough, lowercase) | → | Proposed "Pray for
     Plagues" | Source "Local" | edit pencil
   - Track "Medusa" | Field "Genre" | Current "deathcore;Deathcore"
     (strikethrough — a literal semicolon-joined pair, i.e. two raw tag
     values being collapsed into one) | → | Proposed "Deathcore" |
     Source "Local" | edit pencil

2. **"Never Back Down"** — Never Back Down · 9 tracks (album title and
   artist name are identical here, transcribed as shown) · group checkbox
   **checked** · pill "3 changes" · 3 rows:
   - `All 9 tracks` | Field "Year" | Current "— empty —" | → | Proposed
     "2024" | Source "MusicBrainz · 96%" | edit pencil
   - Track "Sleepwalking" | Field "Title" | Current "Sleepwalking (Feat.
     Hollow Front)" (strikethrough) | → | Proposed "Sleepwalking (feat.
     Hollow Front)" (lowercase "feat.") | Source "Local" | edit pencil
   - Track "Panic Attack" | Field "Title" | Current **"␣Panic Attack␣"**
     (strikethrough — the `␣` open-box glyph is used literally to make
     leading/trailing whitespace visible) | → | Proposed "Panic Attack" |
     Source "Local" | edit pencil

3. **"Decades"** — Motionless In White · 12 tracks · group checkbox
   **unchecked** (empty box, inset border only, no fill) · pill **"2
   changes · none selected"** · 2 rows, **both dimmed to ~55% opacity and
   unchecked**:
   - `All 12 tracks` | Field "Genre" | Current "Metalcore;metalcore"
     (strikethrough) | → | Proposed "Metalcore" | Source "Local" | edit
     pencil
   - Track "Traveler" | Field "Artist" | Current "Motionless in White"
     (strikethrough) | → | Proposed "Motionless In White" | Source
     **"⚠ AcoustID · 44%"** — a warning-triangle icon plus the confidence
     text, in a distinctly muted/neutral tone, different from the plain
     "MusicBrainz · 96–98%" rows above — a visibly low-confidence match |
     edit pencil

   This group is the mockup's example of a user having manually
   deselected everything in an album — it is **not** the default state;
   every row starts checked/preselected.

4. **Collapsed remainder row** (top border, extra top margin): a caret
   icon plus **"28 more albums · 79 changes"** — the rest of the list is
   not expanded in this mockup.

5. **Spelling conflicts section** (dashed-border card, appears after the
   collapsed row; also gated by a `showConflicts` toggle, default `true`):
   - Header row: warning icon · **"Spelling conflicts"** (15px, medium) ·
     muted helper **"Optional · nothing happens if you skip these"** ·
     **"Skip all"** ghost button, right-aligned.
   - Body text: *"Your library spells these three names more than one way.
     Reprise will not guess. Pick one and the matching track changes
     appear above."*
   - Three conflict rows, each: a fixed-width (170px) muted label on the
     left (`"{field} · {n} tracks"`), then a wrapping row of **standalone
     radio pills** on the right (a different component from the
     segmented control — individual rounded chips, not a connected bar;
     used here because the option count is data-driven). Each pill shows
     the candidate spelling plus its occurrence count in a dimmed
     trailing number:
     - **"Artist · 47 tracks"**: pills "Bring Me the Horizon 28"
       (**selected**, accent border + accent text), "Bring Me The
       Horizon 17" (unselected), "bring me the horizon 2" (unselected).
     - **"Album · 11 tracks"**: pills "Count Your Blessings 6"
       (unselected), "Count your blessings 5" (unselected) — **no pill
       selected**, illustrating "no clear winner."
     - **"Title · 4 tracks"**: pills "Bring Me To Life 2" (unselected),
       "Bring Me to Life 2" (unselected) — again none selected.

**Sticky footer bar** (bottom of the page, separated by a top border/shadow
from the list): muted text **"85 tag changes · 61 files · undo available
after"** on the left, a primary button on the right whose label is a
placeholder — **`{{ applyLabel }}`**, defaulting to **"Apply {n} changes"**
rendered here as **"Apply 85 changes"**. Note the count in the footer (85
selected changes) is lower than the toolbar's total (88 changes found) —
selection state (unchecked rows/groups) reduces what actually gets applied;
see open questions for an exact-count discrepancy worth resolving before
implementation.

---

## Frame 6 — Post-apply

**Purpose:** confirmation after committing the reviewed changes. Undo here
covers the quiet fixes too, not just what was just reviewed.

A standalone card (600×400, not full app chrome), padded 36px top/bottom,
40px sides:

1. Small uppercase eyebrow label: **"Library Doctor"**
2. Check icon (accent), 26×26.
3. Heading: **"Tags updated · 61 tracks"**
4. Body text: *"85 changes across 30 albums. 3 spelling conflicts left
   unresolved — they will come back with the next scan."*
5. Footer row, pinned to the bottom of the card: **"Undo everything from
   this scan"** secondary button (undo icon + label) on the left, **"Done"**
   primary button on the right.
6. Below that, a small muted caption: **"Includes the 1,017 quiet fixes.
   Available until the next scan."**

Note "30 albums" here versus "31 albums" in Frame 5's review header — this
is explained by the Decades group being fully deselected there (so it
never actually changed), but the "85 changes" figure doesn't cleanly
reconcile to "88 total minus Decades's 2" (that's 86, not 85) — see open
questions.

---

## Frame 7 — Nothing found (empty state)

**Purpose:** the "scan ran, no findings" state, explicitly distinct from
the pre-scan start page (Frame 2).

Same standalone card layout as Frame 6 (600×400, 36px/40px padding):

1. Eyebrow label: **"Library Doctor"**
2. Check icon, 26×26, but here in a **muted/neutral tone** — not accent —
   contrasting with Frame 6's accent-colored check.
3. Heading: **"Nothing to fix"**
4. Body text: *"1,648 tracks checked, 7 skipped. Your tags are already
   consistent with each other."*
5. Footer row: **"Scan again"** secondary button, plus a muted caption
   **"Whole Library · MusicBrainz on"** beside it (no undo action here —
   there was nothing to undo).

This content block sits vertically centered within the card (the icon/
heading/body group has automatic margin above and below it, unlike Frame
6 where content is top-anchored) — reads as intentionally more "at rest."

---

## Open design questions

1. **Header icon set and back-arrow presence vary between frames without
   an evident rule.** Frame 1 shows a back arrow plus four right-side icons
   (menu, spark/AI, search, panel); Frame 2 shows no back arrow and three
   icons (menu, search, panel — no spark); Frame 3 and Frame 4 show a back
   arrow but only two icons (menu, panel — no search, no spark); Frame 3b
   shows a back arrow and three icons (menu, search, panel — no spark).
   It's unclear whether the spark/AI icon and the search icon are meant to
   be conditionally hidden on certain pages (e.g., no search while a job is
   running) or whether this is just inconsistency between independently
   assembled mockup screenshots. Recommend defaulting to the app's existing
   standard header behavior rather than replicating each omission
   literally.

2. **Sidebar contents vary between frames beyond the intentional deltas.**
   Some frames omit the `PLAYLISTS` section entirely, or show fewer
   playlists (2 vs. 3), or omit `Recently added` / `Concerts` from the
   `SMART` section. This looks incidental to how each frame was captured
   rather than a deliberate per-page sidebar. The two deltas that do look
   intentional and should be implemented are: (a) the scan progress card
   pinned to the sidebar bottom while a job runs (Frame 3), and (b) the
   `Library Doctor` row with a count badge appearing under `ISSUES` once
   there are unreviewed findings and disappearing once resolved (Frame 3b,
   matches the brief).

3. **Row-count discrepancy between Frame 5's footer and Frame 6.** Frame 5's
   toolbar says 88 changes across 31 albums; the one visibly deselected
   group (Decades) accounts for 2 changes, which would suggest 86 remain
   selected — but the sticky footer already shows "85 tag changes" before
   any further interaction, and Frame 6's post-apply screen also says "85
   changes across 30 albums." The exact counting rule (what else was
   deselected, or whether one of the 88 total is double-counted/excluded
   by default) isn't shown by the mockup and needs to be decided
   explicitly rather than inferred — e.g., is a spelling-conflict-driven
   row excluded from the base 88, or is one additional row pre-deselected
   somewhere in the collapsed "28 more albums" section?

4. **Behavior of the per-row edit (pencil) icon is not shown.** Every
   review row has a trailing pencil icon (30px column) but no frame shows
   what it opens — inline edit of the proposed value, a dialog, or
   something else. Needs a decision before implementation.

5. **Overflow/scroll behavior of the Review page is not shown.** Frame 5
   has no fixed height and the list is explicitly truncated ("28 more
   albums · 79 changes" collapsed, not expanded) — whether the real page
   scrolls the whole list under a sticky header/footer, paginates, or lazy-
   loads on scroll isn't specified by the mockup.

6. **Zero-count and zero-result behavior beyond what's explicitly shown.**
   Frame 4's "quiet fixes" and "spelling conflicts" blocks are marked with
   boolean toggles (`quietFixes`, `showConflicts`, both defaulting to
   `true` in the mockup's own component props) implying they're meant to
   be omitted when their count is zero — consistent with the brief's "never
   render a row whose count is zero." But the mockup never shows what the
   summary page looks like with, say, only the "needs review" block present
   (e.g., a scan with zero quiet fixes and zero conflicts) — worth a quick
   layout check once real, since removing two of three blocks changes the
   page's vertical balance.

7. **What happens with 0 total findings vs. 0 remaining after full
   deselection isn't distinguished visually beyond Frame 7.** Frame 7 covers
   "scan found genuinely nothing." It's not shown whether unchecking every
   row on the Review page (Frame 5) and hitting Apply produces a different
   confirmation state than Frame 6, or is simply disabled/blocked.

8. **The accent highlight on "Library Doctor" in the Frame 1 `⋮` menu and
   on the sidebar row in Frame 3b** may just be the mockup's way of drawing
   attention to the element the frame is about, not a literal persistent or
   hover visual style to implement one-for-one. Worth confirming against
   the rest of the menu/sidebar's existing selected/hover conventions
   rather than copying the highlight treatment verbatim.
