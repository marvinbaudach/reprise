---
slug: chapter-two-one-incident
worktree: /home/marvin/Projects/reprise-ch02-incident
branch: feature/chapter-two-one-incident
phase: coded
codex_session:
created: 2026-08-20
---
# CH.02, rebuilt as one incident

The chapter carried a pipeline swimlane and a wall of 27 named gate cells. Both
were process documentation: nobody outside this repository could check either
one, which is the single thing every other chapter on the page can do. They are
replaced by one recorded incident, the mechanism that would have caught it, and
what that mechanism covers.

Specific before general. The general version first is what made the old chapter
unreadable, so the order below is not negotiable.

## What the page must end up with

1. eyebrow, title, lead
2. the incident — heading, one paragraph, **figure 1**, caption, the quote
3. the `Fixes #444` rule
4. **figure 2** — fail closed
5. the six groups
6. the closing line

One column, left-aligned, in the same `.frame` the other chapters use, with the
`chapter__eyebrow` / `chapter__title` pattern and `data-reveal` on the blocks
that arrive on scroll. `id="ch-02"`, `aria-labelledby="ch-02-heading"`,
`data-ground="oklch(13.5% 0.02 205)"` (the value the old chapter used, inside the
neighbours' range). The quote is the only element with a rule or a border.

The two kickers stay distinct — `Fail closed` for the merge decision,
`What the checks refuse` for coverage. Two identical kickers 300px apart read as
a copy/paste slip.

## Decisions already taken

- **The incident is published.** It shows a defect, and that is the point: the
  page's thesis is that every claim here is checkable, and an incident where the
  method caught itself is stronger evidence than any diagram.
- **The three §4C mutations have not been run.** Verified: issue #444 is open, no
  commit references it, no document reports a result. The `Fixes #444` paragraph
  therefore ships as the rule alone. A follow-up carries the result when it
  exists — see *Follow-up* below.
- **No actor/writes/judges matrix.** Considered and cut. `virtual:agent-pipeline`
  loses its last consumer with it.

## Figure 1 — the incident, drawn

Two panels at one shared scale, the 36 px CSS floor as a dashed rule across both.

| | left | right |
|---|---|---|
| eyebrow | `what the test measured` | `what ships` |
| title | Fixture, no stylesheet | The app, with its stylesheet |
| bars | 20 px, 34 px — under the rule | 36 px, 36 px — on it |

Scale 3×, stated in the caption. Chart box `height: 132px`, flex,
`align-items: flex-end`, `gap: 28px`. Bars `width: 84px`, heights
`60 / 102 / 108px`, `border-radius: 3px 3px 0 0`. Floor rule absolutely
positioned at `bottom: 108px`, `border-top: 1px dashed`, `right: 88px` so it
stops before its own caption; that caption at `bottom: 101px`, right-aligned.

**Value labels go below the bars, never above.** A label above needs its own line
box plus the column gap — roughly 23px — so any bar within 23px of the rule
drives its label through it and rasterises as a strike-through. 34 px is exactly
such a value. Below-the-bar placement removes the failure mode for every value,
not just today's. Not to be solved by shrinking the font or nudging an offset.

HTML, not SVG. Bars `aria-hidden`; a `<figure>`/`<figcaption>` pair owns the text
equivalent. Panels stack on narrow viewports and keep the same scale. Left panel
from the neutral ramp, right from the accent — **no red**: the left panel is not
an error state, it is a measurement of the wrong thing.

**20, 34 and 36 are typed, not derived.** They are the values §1 of
`docs/plans/queue-anchor-grill-followups.md` reports, and the spec is explicit
that they must not be rounded, derived, or joined by a number the document does
not carry.

> **Known conflict, must be resolved during implementation.** `show-11` in
> `tempo-timeline.test.mjs` forbids any source file from typing a date that
> appears in `docs/showroom/timeline.md`, and `2026-08-14` happens to be one of
> its week boundaries. Typing the incident date fails that guard. The guard is
> right about the timeline and wrong about this chapter. Two ways out, pick one
> and say why in the PR:
> - derive **only the date** from the record through a `virtual:incident` module
>   (the heights stay typed, as the spec requires), or
> - narrow `show-11` so it only rejects a typed date inside the components that
>   render the timeline.
> Do not type the date and silence the test.

## Figure 2 — fail closed

```
one change   ||||||||||||||||||||||||||| —— [ merge ]
             27 checks, unnamed
```

- One tick per check in script order: a `3px × 30px` rounded bar inside an
  `11px × 44px` button, flex row, no gap. The button is the hit target.
- `one change` in mono on the left, the tick row, a 26px rule, then a bordered
  pill reading `merge`.
- **Hover** → the tick lightens and grows to 34px; the readout becomes that
  check's number and name, `08 · Architecture`. This is where the 27 names live
  now: reachable, not a wall.
- **Click** → coral, 38px; the rule to the endpoint drops to the neutral ramp;
  the pill flips to `blocked` in coral; the readout reads
  `1 of 27 red · the change does not land`. Clicking again clears it; failures
  accumulate.
- Resting readout: `27 checks green · ready to merge`.

Rules: names come from the `gate` calls in `scripts/check-merge-readiness.sh`,
parsed at build time; the count is `GATES.length`, never a literal. Each button
carries `aria-label="08 · Architecture"` so the names are in the accessibility
tree; the readout is `role="status"` / `aria-live="polite"`. Accent for passing,
coral for failure, neutral for the dead rule — no green. Under
`prefers-reduced-motion` the height and colour transitions go and the state
change stays instant.

**Do not claim the run stops at the first red.** It does not — the script
reports. What stops is the merge, so the endpoint goes dark, not the ticks after
the failure.

**Touch viewports:** 11px targets are too small. Widen the buttons and let the
row wrap, or drop the interaction and show the resting state. Do not ship a
27-target row at 11px on a phone.

`lib/mergeGates.ts` already holds this state as pure functions and is already
covered by `show-8`. Reuse it; only the two readout strings change.

## The six groups

Verified against the script on 2026-08-20 — 27 `gate` calls, each assigned to
exactly one group, counts summing to 27:

| count | group | line | checks |
|---|---|---|---|
| 04 | Boundaries | The core cannot grow a UI framework. | Architecture, Device-sync GStreamer, Frontend thinness, GNOME idioms |
| 05 | Distribution | It installs as a desktop app, not as a demo. | Gettext catalogues, Runtime service install, AppStream, Flatpak manifest, Dependency audit |
| 03 | Reachable | Every action works without a mouse. | Accessibility semantics, Input parity, Motion tokens |
| 03 | Traceable | A rule without a test fails the build. | UX traceability, AI hygiene, Rule-owned display tests |
| 07 | Green means green | Tests, lints, formatting, documented API. | Project quality, Rust formatting, Rust lint, Rust documentation, Workspace tests, Linux platform tests, Runtime service bus tests |
| 05 | Toolchain hygiene | The branch, the shell scripts, the worktrees. | Branch diff, Shell, Worktree GC, Worktree GC schedule, Script self-tests |

A six-cell grid, one hairline between cells, count in mono beside the group name,
the line under it in muted text. No icons.

**The grouping is data, not prose.** The assignment lives beside the parse in
`vite.config.ts`, and a test asserts the six counts sum to `GATES.length` and
that no check is unassigned. Without that, the next gate anyone adds falls
silently out of the figure and the counts start lying — on a page whose whole
claim is that its numbers are read rather than typed.

Link `check-merge-readiness.sh` to the pinned permalink.

## Evidence

Every sentence comes from `docs/plans/queue-anchor-grill-followups.md` §1, §2 and
§4C, or from the doc comment on `app_css_for_test()` in
`crates/reprise-gnome/src/ui/style/mod.rs`. The quote links to that file at the
pinned commit; the `Fixes #444` sentence links to §4C.

Verified at the pinned commit: the doc comment sits at `mod.rs:41-45`, and the
§4C heading is *"C. Gate the #444 claim on mutations, not on a green test"*, from
which GitHub derives the fragment
`#c-gate-the-444-claim-on-mutations-not-on-a-green-test`. A renamed heading
breaks the link rather than pointing somewhere wrong — assert the heading text.

**The pinned commit has to move.** `BASELINE.commit` is `604677322e`, which
predates `docs/measurements/index-rebuild.md`, `docs/showroom/timeline.md` and
`showroom/derive/code-census.mjs`. **Four links the page already ships resolve to
a GitHub 404 today**, and the incident record is not there either. `a776f8a963`
(the current `main`) carries all eleven cited paths. Bump the pin and add
`permalinks-resolve.test.mjs`, which resolves every literal `permalink(…)` /
`treelink(…)` argument against the pinned commit from the local object database —
offline, so the verdict does not depend on GitHub being reachable.

## Wiring and knock-on

- Render between `ChapterOne` and `ChapterThree` in `App.tsx`.
- `SiteHeader.tsx`: relabel CH.02 (it is no longer "Gates") and add CH.05.
  **CH.05 has no `id`** — only `data-chapter="05"` — so `#ch-05` is a dead
  anchor until `ChapterFive.tsx` gets `id="ch-05"`. That is why the nav never
  listed it.
- `index.html`: the meta description says *"decided by 21 gates"* while the page
  derives 27. Replace the literal with a `%GATE_COUNT%` token and fill it in a
  `transformIndexHtml` hook from the same `readGates()` the figures use; throw if
  the token is missing, so a renamed placeholder cannot ship a stale number.
- **Dead code:** remove `components/process/*`, `virtual:agent-pipeline` (module,
  type declaration, watcher entry, `readPipeline`, its interface) and
  `tests/agent-process.test.mjs`. **Keep `virtual:merge-gates`** — figure 2, the
  groups *and* `src/data/measurements.ts` all read it. **Keep
  `lib/mergeGates.ts`** — figure 2 is its consumer.
- `SiteFooter.tsx` carries the "Where the figures come from" paragraph, not CH.05
  itself. It currently counts *"the pipeline table"* among the counted figures;
  that becomes false with the swimlane gone. It must name the incident as
  **quoted** and the gate and group counts as **counted**.

## Tests

| id | asserts |
|---|---|
| `show-6` | one mark per `gate` call, in script order, each announcing `NN · Name`; no `gate-wall` left |
| `show-7` | the quote is the doc comment verbatim; the figure draws 20/34/36 and no fourth number; both link fragments resolve to headings that still exist |
| `show-8` | the readout blocks and releases; `toggle` does not mutate its input |
| `show-9` | the figure reveals by opacity only; reduced motion drops the strip's transitions |
| `show-10` | the gate count is nowhere a literal, including the built meta description |
| new | the six group counts sum to `GATES.length` and no check is unassigned |
| `show-17` | every permalinked path exists at the pinned commit |

`show-16` is taken by the hover branch (`gallery-hover.test.mjs`) — the `show-N`
ids are one sequence across the whole suite, not per file.

Each new assertion needs a mutation probe that reddens it, run against a
**committed** tree: `git checkout --` during a probe otherwise reverts the change
under test rather than the mutation.

## Follow-up, not part of this change

The three §4C mutations. When they run, the result belongs in the `Fixes #444`
paragraph and is a stronger ending than the rule alone. File it as an issue
against #444 so the chapter can be finished rather than rewritten.
