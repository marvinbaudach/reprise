# Handover — CH.02 rebuilt as one incident

State on 2026-08-20. Plan: `docs/plans/chapter-two-one-incident.md` (`phase: refactored`).
Worktree `/home/marvin/Projects/reprise-ch02-incident`, branch
`feature/chapter-two-one-incident`, 10 commits ahead of `origin/dev`, 0 behind (rebased
onto `5df217bebb`), worktree clean, nothing pushed.

## What the change is

CH.02 of the showroom loses its pipeline swimlane and its wall of 27 named gate cells —
both were process documentation nobody outside the repository could check — and becomes one
recorded incident, the mechanism that would have caught it, and what that mechanism covers.

Page order: eyebrow/title/lead → the incident (heading, paragraph, figure 1, caption, the
quote) → the `Fixes #444` rule → figure 2 "fail closed" → the six groups → closing line.

## Commits

```
1f2fd0…  fix(showroom): clear stale pointer gate peeks          (N)
…        fix(showroom): color gate readout by displayed result  (M)
…        docs: the plan is refactored
…        docs(showroom): record chapter two review fixes
…        fix(showroom): harden chapter two review contracts     (A–L)
…        docs: the plan is coded, not shipped
…        docs(showroom): record aggregate gate blocker
…        docs(showroom): record chapter two verification
…        feat(showroom): rebuild chapter two around one incident
307e9c15  wip: CH.02 as one incident — plan plus a partial build
```
Hashes shift on every rebase; find them with `git log --oneline origin/dev..HEAD`.
Codex's own evidence (decisions, mutation-probe tables) is in the tracked
`.pipeline-codex.md` — read the **committed** copy (`git show HEAD:.pipeline-codex.md`),
because `codex-run.sh` overwrites the working copy with its short final message at the end
of every run.

## Decisions already taken — do not reopen

- **`show-11` / the incident date.** The date is derived through a `virtual:incident`
  module rather than typed, so the timeline guard stays broad. The bar heights 20/34/36
  remain authored literals, as the spec requires.
- **Pinned permalink commit** moved from `604677322e` to `a776f8a963`. The old pin was
  missing three cited paths, which is why four links the page already ships resolved to a
  GitHub 404. `show-17` (`permalinks-resolve.test.mjs`) proves path existence offline, from
  the local object database.
- **The three §4C mutations have not been run.** The `Fixes #444` paragraph ships as the
  rule alone; the result is a follow-up, not part of this change.

## Review findings A–N — all applied and verified

Three reviewers (React, TypeScript, a CSS/tests reviewer) produced twelve findings; a
visual pass produced two more. All fourteen are in. The two worth remembering:

- **C** — the readout stopped announcing check names once *any* gate was clicked, killing
  both hover and keyboard name discovery. Fixed; confirmed at runtime (`19 · Motion tokens`
  announced while the verdict reads `blocked`).
- **A** — `permalinks-resolve.test.mjs` exempted dot-property arguments
  (`permalink(CENSUS_SCOPE.source)`), so a nonexistent path passed all 85 tests. Proven with
  a control arm before and after the fix.

`N` (a pointer peek going stale when the layout moves under a stationary cursor) has **no
assertion** — the suite is static analysis and cannot reproduce browser hit-testing. Codex
recorded that rather than writing a test that cannot fail. Do not "fix" that gap with a
green placeholder.

## OPEN — finding O, the one thing left

**CH.02 is the only chapter whose figures do not fill the column.** Measured at a 1344px
viewport; the `.frame` is identical across CH.01/02/03 (title runs 1137px in all three):

| chapter | block | width |
|---|---|---|
| CH.01 | `FIGURE.architecture`, `FIGURE.ratio` | 1137 |
| CH.03 | `FIGURE.seek-card` | 1137 |
| CH.03 | `SECTION.mosaic` | 1148 |
| **CH.02** | `FIGURE.incident-figure` | **768** |
| **CH.02** | `FIGURE.gate-figure` | **824** |
| **CH.02** | `DIV.gate-groups` | **760** |

Cause: three unequal hardcoded caps in `ChapterTwo.css` — `.incident-figure`
`max-width: 48rem`, `.gate-figure` `52rem`, `.gate-groups` `48rem`. With the 552px body
measure that stacks **four different right edges** where every neighbour has two. The plan
never asked for them.

**Decided with the user: the three blocks fill the column.** Running text keeps its narrow
measure. Three things make this more than deleting three lines:

- `.gate-groups` is already `repeat(2, minmax(0, 1fr))` — dropping the cap is enough.
- `.gate-figure` — dropping the cap is **not** enough. `.gate-strip__row` is a content-sized
  flex row, so the box would measure 1137 while the visible edge stays at ~824. The rail
  before the `merge` pill has to take the slack, which is also what the plan's sketch draws:
  `one change ||||||||||||||||||||||||||| —— [ merge ]`. Keep the wrapped phone layout
  (44px targets, four rows) intact.
- `.incident-figure` — widening gives two panels of ~554px. The dashed floor rule is
  positioned `right: 88px` against the chart box with a right-aligned caption; it must stay
  visually tied to the bars it measures rather than stretching off to the right. The figure
  fills the column; the bars do not have to.

A Codex prompt for exactly this was written to `.pipeline-task.md` in the worktree and a run
was started right after this file was committed. **`.pipeline-task.md` is gitignored** — if
the run is gone, everything needed to rewrite it is in this section.

## Verifying it — the harness and its traps

Scripts live in the session scratchpad (`ch02-shots.mjs`, `probe-hover2.mjs`,
`shots-interaction.mjs`, `probe-mn.mjs`, `final-shots.mjs`, `geom2.mjs`). Serve with
`npx vite preview --port 4791 --strictPort` from `showroom/` after `npm run build`
(base is `/reprise/`), then drive Brave over CDP on port 9333.

Four measurement traps cost time here; all are real and all will recur:

1. **Never scroll the top-level page.** After the first scroll `captureScreenshot` returns
   black frames with a provably fine DOM. Shift the layout with a negative `body` margin and
   stay at `scrollY 0`.
2. **A clipped `captureScreenshot` destroys the hover before the shutter.** With `clip`
   Chromium re-lays out, the stationary virtual cursor loses the element, `:hover` drops —
   the picture shows the resting state although the DOM readback a moment earlier said
   `hovered: true`. Shoot the **full viewport** and crop afterwards with `magick … -crop`.
   Read the state back before *and* after the shutter.
3. **Reveal animations drop the hover about a second after load.** Neutralise
   `[data-reveal]{opacity:1!important;transform:none!important}` first and re-assert hover
   immediately before measuring, with retries rather than a blind shot.
4. **`transition: none 0.001s` means transitions are OFF.** Under `prefers-reduced-motion`
   `getComputedStyle` returns property `none` plus an alibi duration; an oracle grepping for
   `\d+m?s` falsely reports "still animating". Check `transition-property`.

Mobile widths need a same-origin iframe at 390×844 — the visible window floors at ~578px
while `set_viewport` reports success. Inside the iframe the **body** scrolls, not the window.
Hover needs Brave started with
`--blink-settings=primaryHoverType=2,availableHoverTypes=2,primaryPointerType=4,availablePointerTypes=4`
plus a real `Input.dispatchMouseEvent`; `CSS.forcePseudoState` and `setEmulatedMedia` do not
work.

Last measured state (before finding O): hover 30→34px announcing the check name, keyboard
focus the same, click 38px coral with the pill on `blocked` and the rail dark while the ticks
after the failure stay bright, failures accumulate, reduced motion drops the transitions,
phone targets 44×44px across four rows, figure 1 stacks keeping its 3× scale (60/102/108),
no horizontal overflow (375/375).

## The merge-readiness gate is NOT green yet

`scripts/check-merge-readiness.sh` has never completed here. In order:

1. The mandatory `git fetch` failed on `Bad owner or permissions on
   /etc/ssh/ssh_config.d/20-systemd-ssh-proxy.conf`. **Fixed** — the fetch now succeeds.
2. `branch is stale`. **Fixed** by the rebase onto `5df217bebb`.
3. One attempt died on `merge-readiness requires a clean worktree, including untracked
   files` listing `quality/tests/.markdown-lint-fixture/` and `.yaml-lint-fixture/`. Those
   are the script's own self-test fixtures — a race with a **second** gate run from another
   session. Do not run two at once; check `ps` for a foreign `check-merge-readiness.sh`
   first. The dirs are gone and the worktree is clean.
4. The furthest run reached the Android lint stage and Gradle failed there. Codex reported
   the same stop and attributed it to no Android SDK on this host. **Unconfirmed** — nobody
   has read that Gradle failure properly yet. Do that before treating it as environmental.

**`heavy-run` swallows the child's stderr.** Running the gate through it logged only the
`== Refresh origin/dev ==` header while the real reason sat on stderr; that cost three
attempts. Redirect *inside* the child:
`heavy-run heavy -- bash -c './scripts/check-merge-readiness.sh > LOG 2>&1'`.

## Next steps

1. Finish finding O and re-measure the three widths against CH.01/CH.03.
2. Get `check-merge-readiness.sh` to a real verdict, or record precisely which stage is
   environmental and why.
3. `land.sh <pr>` — the plan carries `branch:`, so the script finds its own status line and
   writes `phase: shipped` into the feature PR itself. Do not set that phase by hand;
   Codex did once in the code phase and it had to be reverted.
