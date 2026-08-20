# Handover — CH.02 rebuilt as one incident

State on 2026-08-20. Plan: `docs/plans/chapter-two-one-incident.md` (`phase: refactored`
— `land.sh` writes `shipped`, never by hand). Worktree
`/home/marvin/Projects/reprise-ch02-incident`, branch
`feature/chapter-two-one-incident`, 15 commits ahead of `origin/dev`, 0 behind, worktree
clean, pushed, open as PR **#596** against `dev`.

## What the change is

CH.02 of the showroom loses its pipeline swimlane and its wall of 27 named gate cells —
both were process documentation nobody outside the repository could check — and becomes one
recorded incident, the mechanism that would have caught it, and what that mechanism covers.

Page order: eyebrow/title/lead → the incident (heading, paragraph, figure 1, caption, the
quote) → the `Fixes #444` rule → figure 2 "fail closed" → the six groups → closing line.

## Commits

```
fix(showroom): align the incident quote measure            (Q)
docs: register chapter two showroom rules                  (SHOW-17..21)
docs(showroom): record chapter two width verification
fix(showroom): fill chapter two figures                    (O)
docs: hand over CH.02 with the open column-width finding
fix(showroom): clear stale pointer gate peeks              (N)
fix(showroom): color gate readout by displayed result      (M)
docs: the plan is refactored
docs(showroom): record chapter two review fixes
fix(showroom): harden chapter two review contracts         (A–L)
docs: the plan is coded, not shipped
docs(showroom): record aggregate gate blocker
docs(showroom): record chapter two verification
feat(showroom): rebuild chapter two around one incident
wip: CH.02 as one incident — plan plus a partial build
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

## Findings O and Q — done, measured, closed

**O — CH.02 was the only chapter whose figures did not fill the column.** Three unequal
hardcoded caps in `ChapterTwo.css` (`.incident-figure` 48rem, `.gate-figure` 52rem,
`.gate-groups` 48rem) stacked four different right edges down the page where every
neighbour has two. All three are gone. `.gate-strip__rail` became `flex: 1 1 26px` with
a 26px minimum so the *visible* `merge` pill — not merely its layout box — reaches the
column edge; the rail grew from 26 to 654px to prove it moved.

**Q — the quote was the last stray edge.** `.incident-quote` carried the same kind of
leftover cap and sat alone at 858. It now uses the running text's `68ch` measure.

Measured at 1344px against the production build, every top-level block in CH.02 lands
on one of exactly two right edges, the same two CH.01 and CH.03 use:

| block | before | after |
|---|---|---|
| `FIGURE.incident-figure` | 768 | 1148, right edge 1238.5 |
| `FIGURE.gate-figure` | 824 | 1148, right edge 1238.5 |
| `DIV.gate-groups` | 760 | 1148, right edge 1238.5 |
| `BLOCKQUOTE.incident-quote` | 768 | 558, right edge 648.5 |

The incident charts deliberately keep their own `min(100%, 17.75rem)` measure: the
figure fills the column, the measured bars stay at the size they are measured at. Four
panel alternatives were rendered and compared (chart centred, panels pushed to the ends,
bars spread); all three pulled the two charts apart and weakened the comparison the
figure exists for. The current layout was chosen deliberately — **do not "fix" the
empty right half of each panel** without changing the markup so something lives there.

Phone unchanged throughout: no horizontal overflow (390/390), 27 hit targets at exactly
44×44 across four wrapped rows, bars 84px wide at 60/102/108/108.

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

## The merge-readiness gate is green

It passes against `origin/dev`: 50 stages, 5407 Rust tests passed, 0 failed, no errors.

Two stages were skipped deliberately, and both run in CI:

- **`Rule-owned display tests`** — the user asked for no GTK4 tests. The branch touches
  no Rust at all, so the display suite has nothing of this change to see.
- **`Android source quality`** — `MERGE_READINESS_SKIP_ANDROID_QUALITY=1`. This is the
  documented case the switch exists for: the Kotlin package `uniffi.reprise_android_ffi`
  is generated and gitignored, so Android lint cannot build in any fresh worktree. That
  is what the earlier Gradle stop was, and it is not this branch's doing — it touches
  neither `android/` nor `crates/`.

The earlier blockers are all resolved: the `ssh_config.d` permission failure that broke
`git fetch`, the stale branch, and a foreign gate run from another session racing on the
script's own self-test fixtures.

One real finding came out of finally reaching the far stages: **five tests referenced UX
rules that did not exist** (`SHOW-17` … `SHOW-21`). `scripts/check-ux-traceability.sh`
collects every `test('show-<N>` title under `showroom/tests/` and demands a matching
rule in `docs/ux-rules.md`, which stopped at SHOW-15. The five rules are now written.
**Anything adding a `show-N` test must add its `SHOW-N` rule in the same commit.**

**`heavy-run` swallows the child's stderr.** Redirect *inside* the child:
`heavy-run heavy -- bash -c './scripts/<gate> > LOG 2>&1'`.

## Next steps

1. PR **#596** against `dev` is open. Watch CI; `Rule-owned display tests` and the
   Android job are the two stages no local run covered.
2. `land.sh 596` — the plan carries `branch:`, so the script finds its own status line
   and writes `phase: shipped` into the feature PR itself. Do not set that phase by
   hand; Codex did once in the code phase and it had to be reverted.
3. Follow-up, not part of this change: the three §4C mutations behind `Fixes #444`, and
   finding `N` (a pointer peek going stale when the layout moves under a stationary
   cursor) which has no assertion because the suite is static analysis.
