# AT-SPI evidence run — the sort leaves the browse bar

Measured 2026-08-28 against `feature/the-sort-leaves-the-browse-bar` at
`2691fb8704`, control arm at `3000da8cd2` (`origin/dev`, the branch point).

Both arms ran the same probe, the same profile and the same fixture library, so
a difference between them is a difference in the code and nothing else.

## How it was measured

- Headless: private Xvfb at 1920×1080, private D-Bus session, private
  `at-spi-bus-launcher` + `at-spi2-registryd`, scratch `XDG_*` dirs,
  `REPRISE_AUDIO_SINK=fakesink`. Never the real library database
  (AGENTS.md), never the real session bus.
- Fixture library: five tagged FLACs (ffmpeg sine tones) across three artists,
  three albums, three genres — enough that the Music table renders rows rather
  than an empty state.
- One launch per measurement. A single click-through session would let one
  stale a11y bridge poison every later reading, and a crashed launch would then
  be indistinguishable from a surface that legitimately exposes nothing.
- The probe walks the tree off the a11y bus and records, per node, `role`,
  `name`, the **Action interface** (`n_actions` and the action names) and the
  state set (`sensitive`, `checked`, `focusable`, `showing`). The repo's
  existing `scripts/cua-e2e/atspi_probe.py` records only `{depth, role, name}`,
  which cannot decide a claim about actions or sensitivity.

### The probe has its own control

"This node exposes no action" is unfalsifiable on its own: a detached bridge
produces the same tidy row of zeroes as a correct measurement. Every dump
therefore had to contain at least one button exposing an action, or the run
was reported as FAILED rather than as evidence.

It earned its keep twice. The first run failed because the probe looked for
role `push button` while this AT-SPI version reports `button` — the control
caught a misconfigured probe instead of letting it report a false negative.

## What was measured

| | control arm `3000da8cd2` | fix arm `2691fb8704` |
|---|---|---|
| positive control | 20 action-bearing buttons | 19 (191-node tree) / 21 (surface open) |
| column header nodes | `filler` "Title", "Artist" — **0 actions** | `filler` "Title", "Artist" — **0 actions** |
| browse-bar sort pill | `toggle button` "Sort" — **1 action**, focusable | **absent** — no node named "Sort" |
| `win.edit-column-layout` | — | `enabled=True` (D-Bus `org.gtk.Actions.DescribeAll`) |
| sort radios in the surface | — | **8 `radio button`** nodes |
| tree size | 196 nodes | 191 nodes; **334** with the surface open |

The header result is the important one for method, not for news: it is
**identical in both arms**. That is what makes the rest trustworthy — the probe
is demonstrably reading the same thing before and after, so the disappearance
of the pill and the appearance of the radios are real differences and not
artefacts of two different measurements.

### The sort radios, as an assistive technology sees them

    radio button  checked=False  focusable=True  sensitive=True  "Title"
    radio button  checked=True   focusable=True  sensitive=True  "Artist"
    radio button  checked=False  focusable=True  sensitive=True  "Album"
    radio button  checked=False  focusable=True  sensitive=True  "Year"
    radio button  checked=False  focusable=True  sensitive=True  "Length"
    radio button  checked=False  focusable=True  sensitive=True  "Rating"
    radio button  checked=True   focusable=True  sensitive=True  "Ascending"
    radio button  checked=False  focusable=True  sensitive=True  "Descending"

Labelled, keyboard-focusable, and exposing the current choice as `checked`
state on both groups — the three properties STYLE-13 demands. Six sortable
fields are offered while the window renders only two column headers, which is
`sortable_columns()` reading the stored layout rather than the live GTK
`visible` property, seen from outside the code.

## Three honest limits

1. **The F10 route itself was not proven.** Neither synthetic key injection
   (`xdotool key F10`) nor the menu button's own AT-SPI `click` action opens the
   primary-menu popover under Xvfb — measured twice, the tree stayed at exactly
   191 nodes with no menu role. The surface was therefore opened through
   `win.edit-column-layout` on the session bus: the same GAction the menu entry
   and the header right-click popover both invoke, so what opened is the
   surface under measurement. That the *keyboard path to it* works is covered
   by the display tests, not by this run.

2. **Only the Music table was measured.** The sidebar rows are not exposed as
   named AT-SPI nodes — the tree contains a single `label` "Music" and no row
   for My Stats, Releases, Concerts, Radio, Podcasts or YouTube, with all
   modules enabled in the profile database and the ungated My Stats row missing
   too. There is no GAction, env hook or D-Bus handle that selects the
   top-level page, so the other pages could not be reached. The per-page
   sensitivity claim rests on `style_13_only_table_pages_resolve_an_editor_model`,
   not on this run. The sidebar's own AT-SPI exposure looks worth a separate
   look; it is not something this change caused or could fix.

3. **The radios expose no AT-SPI action** (`n_actions=0`); they are focusable
   and carry state, but carry no Action interface. This is **not** a regression:
   the deleted pill built its radios identically — `gtk4::CheckButton` with
   `accessible_role(Radio)` plus `set_focusable(true)`, same `a11y-semantics`
   marker (old `browse_sort.rs:207-214`, new `editor.rs:135-142`) — so it is a
   property of GTK's CheckButton under AT-SPI, unchanged by this change. It was
   not confirmed by opening the old popover, which the same synthetic-click
   limitation blocks.

## Verdict

The claim the plan set out to prove holds for the Music table: sorting left the
browse bar without leaving the accessibility tree. The pill's single action is
gone; in its place are eight labelled, focusable radio buttons carrying their
state, behind an action that reports itself enabled. The column headers expose
no action in either arm, exactly as #404 recorded — that was never fixed and
this change does not pretend to fix it.

Full trees (control, fix, and fix with the surface open) were recorded as JSON
— the record is a tree, not a screenshot. They are deliberately not committed:
they run to some 13,000 lines and no other plan in this repository carries raw
dumps. Every node row quoted above is lifted from them verbatim, and the method
described above is enough to regenerate them from either revision. The dumps
were kept outside the repository, at
`~/Projects/reprise-evidence/the-sort-leaves-the-browse-bar.atspi/`.
