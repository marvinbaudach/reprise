# Follow-up work from the queue-section-anchor grill (2026-08-14)

Written after a `/ship` run on `docs/plans/queue-section-anchor-handover.md` stood
down in favour of a **second, concurrent ship run** that was already inside its
Codex code phase on the same branch and worktree.

- **Branch/worktree owned by the other run:** `feature/queue-section-anchor` in
  `/home/marvin/Projects/reprise-queue-section-anchor`
- **That run's plan:** `docs/plans/queue-section-anchor-landing.md` (243 lines,
  written 03:45). It overwrote a longer plan this run had produced; only the file
  was lost, the decisions are reproduced below.
- **Both runs reached the same root cause independently.** Nothing here disputes
  that run's diagnosis or its fix.

This document records only the **delta**: what the grill decided that the other
run's scope explicitly excludes, plus the evidence behind it.

---

## 1. The measured finding both runs share

`nav_back_to_a_large_sectioned_queue_never_visits_the_top` fails its precondition
with `bands(rows=7 headers=2 row_samples=[34.0,34.0,34.0,34.0] header_samples=[20.0, 34.0])`.
The headers are **not** genuinely non-uniform. The fixture never installs the app
stylesheet, so it measures unstyled widgets:

- `crates/reprise-gnome/src/ui/style/tokens.rs:66` — `SECTION_HEADER_MIN_HEIGHT: i32 = 36`
- `crates/reprise-gnome/src/ui/track_list/queue_sections.rs:82-85` — that floor exists
  **only** as CSS: `.queue-section-header-row { min-height: 36px; }`
- `crates/reprise-gnome/src/ui/style/mod.rs:119` — the rule is part of the composed
  app stylesheet, which `app_css_for_test()` (`:45`) returns
- `crates/reprise-gnome/src/ui/track_list/queue_section_header_display_tests.rs:64` —
  QUE-1 installs it before building the fixture, asserts the headers are uniform and
  each reaches the floor, and **passes**
- `queue_section_geometry_display_tests.rs` contains no `install_css` anywhere

"Play Next" carries a Clear button (a `gtk4::Box` child) whose natural height gives
34 px; "Now Playing" is a bare `gtk4::Label` that collapses to 20 px without the
floor. `app_css_for_test`'s own doc comment names the trap: *"A geometry assertion
against unstyled widgets passes while the shipped button is a different size."*

**Corollary the other run's plan does not appear to carry:** the stylesheet also
sets ROW heights — `ROW_MIN_HEIGHT_COMFORTABLE: 36`, `STANDARD: 28`, `COMPACT: 20`
(`style/tokens.rs:44-57`, emitted by `track_list/list_density.rs::css()`). Styling
the fixture may therefore move the measured row height off the unstyled `34.0`, and
**36 is exactly the `row height=36.04481546572935` that issue #444's own failure text
quotes**. Every absolute figure inherited from earlier runs — 34, 37454, 77438,
77456, the 72 px "half-fix signature" — is provisional until the styled run reports
its own numbers. Use the run's measured values, never the inherited constants.

## 2. Three handover claims that are refuted

`docs/plans/queue-section-anchor-handover.md` should not be read as fact on these:

1. **"`ListLayout` carries one `section_header_height`… that is a wrong model
   assumption."** Refuted by §1. The single-header-height model is not disproven,
   and per-section header heights should stay out of scope.
2. **"`validate` was right to reject."** It correctly rejected an unstyled tree the
   shipped app never renders. What actually rejected the bands is `uniform_heights`
   (`queue_section_geometry_display_tests.rs:273-283`), a test-side guard on a
   test-side measurement.
3. **"This test was already red on `dev`, so leaving it red is not a new failure."**
   Refuted by inspection: `origin/dev`'s copy of the file has no band measurement at
   all and derives `row_height = adjustment.upper() / restored_ids.len()`, so it
   **cannot** produce the "did not expose uniform rendered bands" panic. The two
   test-only commits `8e86a21773` and `8c57332184` introduce it. Since #463 widened
   the gate to the whole ignored suite, this branch would newly redden the dev gate.

## 3. Post-rebase display evidence (02:59, each test in its own process)

| Test | Result |
|---|---|
| `nav_back_lands_on_the_anchored_row` | `ok. 1 passed` |
| `…_in_the_full_journey` | `ok. 1 passed` |
| `…_when_the_sort_differs` | `ok. 1 passed` |
| `…_when_the_table_had_focus` | `ok. 1 passed` |
| `queue_anchor_names_the_row_at_the_viewport_top` | `ok. 1 passed` |
| `que_1_queue_section_headers_share_one_height` | `ok. 1 passed` |
| `browse_4_the_title_link_leaves_the_viewport_at_the_revealed_track` | `ok. 1 passed` |
| `nav_back_to_a_large_sectioned_queue_never_visits_the_top` | **FAILED** (precondition) |

Handover item 1 ("re-verify on display after the rebase") is therefore **closed**.

Caveat that must reach the PR: `queue_anchor_names_the_row_at_the_viewport_top`'s
green was measured **unstyled**. Once the fixture installs the stylesheet, its next
run is a *re-measurement*, not a re-confirmation.

---

## 4. The delta — decided in the grill, outside the other run's scope

The other run's Codex prompt forbids touching production code, confining the change
to `queue_section_geometry_display_tests.rs` plus two documents. That excludes A and
B below with certainty; C and D were not identifiable in its plan.

### A. Collapse the unreachable `Option` on `content_height`/`max_scroll`

`crates/reprise-gnome/src/ui/list_geometry_layout.rs`. The constructor makes the
`None` arm unreachable, so the `Option` is noise at every call site.

**Hard constraint:** implement it **structurally** — carry a type that cannot be
absent — **never** by adding an `expect` at the delegation seam. An `expect` there
converts an unreachable state into a live panic path inside a GTK callback, which is
a worse trade than the `Option` it removes.

Production change ⇒ needs `rust-reviewer` and a display pass. Better as its own
branch than bolted onto a landing branch.

### B. Document the `section_starts` invariant, with a `debug_assert!`

Duplicate entries in `section_starts` are double-counted by `headers_above`.
Unreachable today — `queue_sections::section_ranges` emits distinct starts — so a
documented invariant plus a `debug_assert!` is the right weight.

**Explicitly not a runtime dedup:** that would silently absorb a real upstream bug
instead of surfacing it.

### C. Gate the #444 claim on mutations, not on a green test

A green styled q-journey does **not** distinguish "green because the anchor is fixed"
from "green because the styled tree makes the capture and restore errors cancel
again" — which is how this defect stayed latent for months. Before any PR claims
`Fixes #444`, all three mutations must turn the suite **red**:

1. rows-only `scroll_target` — the one claim in the whole record never independently
   reproduced (reported 16 passed / 3 failed, never re-run)
2. `headers_above` forced to `0`
3. `validate` forced to always-`Accepted`

If any mutation leaves the suite green, the header term is not load-bearing and the
PR must not claim #444. All three are displayless.

### D. Pre-landing gate: the full ignored suite once, reds re-run solo

#463 widened the gate to the whole ignored suite, so what the gate sees after the
merge is not what a targeted run sees. Run the full suite once the way the gate will,
then re-run **every** red in its own process to separate herd flakiness from a real
failure.

Per-run recipe: own XDG roots, `dbus-run-session`, `xvfb-run -a`,
`GDK_BACKEND=x11 WAYLAND_DISPLAY= GSK_RENDERER=cairo REPRISE_AUDIO_SINK=fakesink`.
Judge on the `^test result:` line **and its count** — an `--exact` selector that
matches nothing still prints `ok. 0 passed`. Finish with `xvfb-orphan-gc --apply`.

### E. Handover correction header

`docs/plans/queue-section-anchor-handover.md` should be committed **verbatim** with a
dated 2026-08-14 note added at the top naming the three refuted claims in §2 and
pointing here. The claims are not edited away — a correction is added above them.
(Likely inside the other run's scope; verify before duplicating.)

---

## 5. Issues to file — check for duplicates first

The other run's plan has its own "file the follow-ups" step, so **confirm these do not
already exist** before creating them.

1. **Header-height provenance.** The strand the original plan deferred: actually
   exercise `ListGeometry::section_header_height`'s write path end to end, so the
   height is measured rather than assumed. §1 gives it its first real measurement and
   the reason the assumption held.
2. **`validate` can only reject in one direction.** A guard that cannot guard against
   the opposite error. Nothing to catch today, because the styled `upper` matches the
   prediction — file it, do not fix it here.
3. **#460** stays as it is (rows-only *centring* model in
   `scroll_center::centered_scroll_value_with_height` and
   `track_list_reload::pending_reveal_anchor`).

## 6. Deliberately not done

- **Per-section header heights in `ListLayout`** — §1 removes the only evidence that
  ever motivated it.
- **Excluding, renaming or deleting the test** — #463 removed the gate's filters on
  purpose; excluding a test now rebuilds the mechanism that produced a green gate
  with a missing suite.
- **Changing `uniform()`'s 0.5 px tolerance** — it reported the truth about the tree
  it was given. The bug was the tree.
- **Pinning an explicit scroll value** in `queue_anchor_names_the_row_at_the_viewport_top`
  instead of `scroll_to(1100)` — out of scope for a landing strand.
