# Handoff — the filter row takes its height from the pill

Date: 2026-09-10. Status: **#918 merged on `dev`; the fix-forward for it is
committed but NOT pushed.** Read "Where this stands" before doing anything.

## What the work was

The ask was to bring the toolbar's search pill in line with "+ Add filter":
36px, `padding: 0 6px 0 12px`, no vertical padding, magnifier at 13px.

Those values were already in the code — `filter_bar_chip.rs` has carried them
since 0.1.174 (#909). Measuring showed the mismatch runs the other way: the
chip renders at its authored height and **"+ Add filter" does not**.

| | requested height |
| --- | --- |
| search pill | 38px (36 + 1px border per side) |
| `+ Add filter` | 58px |
| `+ Add filter` without the `pill` class | 48px |

Adwaita's own `button` padding stacks on top of the authored `min-height`, and
the `pill` class adds 10px more per side. The filter bar's slots stretch their
children, so the 38px chip was pulled up to 58px: the two *looked* aligned
while the row took its height from the button, and neither pill rendered at
36px. Zeroing the vertical padding makes the authored height the rendered one.

**The filter row is now 20px shorter (58 → 38px).** That is the most visible
consequence. `FILTER_BAR_MIN_HEIGHT` (34) still clears it.

The magnifier was a second, separate defect. At 13px it was scaled off the
16px grid symbolic icons are drawn on, and the `GtkImage` was allocated the
chip's full height and centred itself inside it: `(36 - 13) / 2 = 11.5`, half a
pixel high. The 22px `×` beside it divided evenly, which is why only the
magnifier looked misplaced rather than the pair looking small. 16px restores
the grid and divides evenly; `set_valign(Center)` hands the arithmetic to the
box, the way the `×` already did it.

## Where this stands

| | |
| --- | --- |
| `origin/dev` | `52cb5204bc` — "The filter row takes its height from the pill (#918)", version 0.1.177 |
| PR #918 | MERGED 2026-09-10 13:01:50Z |
| dev CI run 34480132794 | **failure** — see the triage below |
| Fix-forward worktree | `/home/marvin/Projects/reprise-pill-outer-padding` |
| Fix-forward branch | `feature/the-add-filter-outer-node-pays-no-padding`, commit `e21e5dded5` |
| Fix-forward gates | green — `check-gnome-ci.sh` and `ci-quality.sh` both pass on a clean worktree |
| Fix-forward pushed? | **No.** No PR. |

**The next person's first job is to finish landing `e21e5dded5`**: push, open a
PR against `dev`, then `~/.claude/skills/pipeline/scripts/land.sh <pr>
/home/marvin/Projects/reprise-pill-outer-padding --no-plan`. The gates have
already been run and were green; nothing else is pending on that branch.

## The CI triage — four of five red jobs are older than this work

Run 34480132794 failed five jobs. The control arm is the previous dev run
**34477512495** (`6bc0f81d40`, #917), which was already red in the same
categories with byte-identical errors.

| Job | Cause | Verdict |
| --- | --- | --- |
| Base and contract checks | `unsafe frontend block is not allowed in crates/reprise-gnome/src/ui/session_restore.rs` | pre-existing |
| Display tests 3/4 | `compact_mode_controls`: `the maximize regression needs the test window manager: NotFound` | pre-existing |
| Display tests 1/4 | `session_restore`: same missing test window manager | pre-existing, **shard-shifted** |
| Display tests 2/4 | the new pill test: `(40, 40)px against (38, 38)px` | **caused by #918** |
| Quality gate | aggregator; reports `BASE_RESULT: failure` | pre-existing |

Two traps in that table worth keeping:

- **A new test file reshuffles the display shards.** The `session_restore`
  failure moved from shard 3/4 to shard 1/4 purely because #918 added a test
  file and changed the distribution. It reads as a new failure and is not one.
- **"Base and contract checks" fails early and skips the rest.** Because the
  frontend-lint step exits first, "Verify repository and workflow contracts"
  and "Verify project source quality" never run. Whether the new display test
  satisfies rule-ownership/traceability is therefore **still unverified** —
  the checking step has not executed since it was added. Watch for it once the
  `session_restore` lint is fixed.

## Why #918 passed locally and failed on the runner

The height rule zeroed the **inner** `button` node, since that is the node that
paints — the repo already knew a `GtkMenuButton`'s outer node paints nothing
(memory: `a-menubutton-outer-node-paints-nothing`). But some Adwaita versions
also give the **outer** node vertical padding, and that lands straight on top
of the authored height. One pixel per side on the runner: 40px there, 38px
here. `e21e5dded5` zeroes the outer node too.

**The reproduction is the reusable part.** The display test now stands the
runner's declaration in itself, and it must go in at *theme* priority:

```rust
crate::ui::style::install_theme_css_string_for_test(
    ".reprise-filter-add { padding-top: 1px; padding-bottom: 1px; }",
);
```

`install_css_string_for_test` installs at **application** priority — the same
priority as the rule under test. An emulation installed there outranks the rule
it is meant to challenge, so the test fails no matter how correct the fix is.
That mistake was made and caught here; `install_theme_css_string_for_test` was
added to `style/mod.rs` (the only module the frontend-lint allowlist lets build
a `CssProvider`) for exactly this. Verified red-then-green: without the fix the
local run reproduces the runner's `(40, 40)` against `(38, 38)` exactly.

## Measurement notes — read before touching this geometry again

- **Measure the requested height, not the allocation.** The bar's slots stretch
  their children, so `compute_bounds` in a test window measures the *window*.
  The first version of the test compared allocations, came back `58 == 58`, and
  passed while proving nothing. Use `measure(Orientation::Vertical, -1)`.
- **`chip.height()` is not the chip's content box.** A child's
  `compute_bounds(&chip)` is already relative to the content origin. Comparing
  a child's centre against an assumed content height got this wrong; comparing
  the magnifier's centre line against the `×`'s needs no such assumption, and
  the `×` is a good control because it always divided evenly.
- **One GTK display test per process.** Batching them fails with `Attempted to
  initialize GTK from two different threads` regardless of the code under test.
  `--test-threads=1` does not help. Run each with `--exact` in its own
  `xvfb-run` invocation.
- **`padding: 0 12px` is not "no vertical padding".** The shorthand drops
  Adwaita's horizontal padding too and narrows the button — a width change
  nothing in the suite measures. Write `padding-top`/`padding-bottom`.
- AT-SPI (`cua-driver`) returns no windows under this GNOME/Wayland session and
  the `org.gnome.Shell.Screenshot` bus refuses with `AccessDenied`. Measuring
  the running app from outside was not available; everything above came from
  xvfb-hosted tests.

## Seeing it rather than measuring it

A throwaway render probe produced a before/after image of both rows in one
frame, using `WidgetPaintable` + `render_texture` + `save_to_png_bytes` (the
pattern already in `style/buttons.rs`). It is **not committed**; the file is
kept at
`/tmp/claude-1000/-home-marvin-Projects-reprise/84b2dc63-4d08-496c-a898-a192e6e5bba0/scratchpad/filter_bar_render_probe.rs.keep`
(a `/tmp` path — copy it somewhere durable if it is worth keeping).

Three ways that probe lied before it was trustworthy, all worth knowing if it
gets rebuilt:

1. Snapshotting the content box paints no window background, so a dark-theme
   capture came back as an almost empty image. Give the sampled widget a
   `background-color: @window_bg_color` of its own.
2. `margin` sits outside the widget's box and is not in the snapshot, so the
   bottom row's border landed on the last pixel row and read as missing. Use
   padding on the sampled widget instead.
3. The control arm was unfaithful: `style_add_filter` also calls
   `set_has_frame(false)`, which long predates this work. Leaving it out of the
   hand-built "before" row rendered Adwaita's framed button and made the
   shipped state look brighter and heavier than it is.

## Still open

- Land `e21e5dded5` (above).
- `unsafe frontend block is not allowed in .../session_restore.rs` — red on dev
  before this work, blocks the whole contract job and hides two later steps.
- The display suite needs a test window manager on the runner; two tests fail
  with `NotFound` and have nothing to do with this change.
- Icon size is a judgement call, not a measurement: 16px is the canonical
  symbolic size and divides evenly in a 36px chip. 14px also divides evenly but
  is off the 16px grid — sharp placement, soft glyph. The user accepted 16.

---

## Closed 2026-09-10 — and one claim above is wrong

`e21e5dded5` landed as #919 and **did not fix the runner**: the dev run over
`d9dda6900b` reported `(40, 40)` against `(38, 38)` byte for byte, exactly as
before.

The reason is the "reproduction" this document recommends as "the reusable
part". It is not. Installing
`.reprise-filter-add { padding-top: 1px; padding-bottom: 1px }` at theme
priority and then watching the app's rule cancel it proves only that an
override overrides an injection — true of any such pair. The runner has the
same `gtk4 1:4.22.4-1` and `libadwaita 1:1.9.3-1` as this machine; there was no
version difference to find, and the outer node never had padding.

What was actually wrong: the MenuButton's inner `button` node carries **1px of
vertical margin**, on top of the padding #918 had already zeroed. `margin-top`
and `margin-bottom` at 0 bring it to 38px. Fixed in #920 together with the
three other reds on dev — `openbox` missing from the display job's container,
the `unsafe` allowlist line #917 never added, and a Compose test that clicked
without waiting. `dev` went green, 0.1.179 was promoted and published.

The failure reproduces locally after all — in the isolated environment the
display-test script builds per test, not under a bare `xvfb-run`. The recipe
and the per-declaration measurement method are in the memory
`a-display-geometry-repro-needs-the-jobs-own-environment`.

Still open from the list above: the XID lookup is duplicated in two files, and
a shared test-support helper would let the allowlist hold one entry instead of
two.
