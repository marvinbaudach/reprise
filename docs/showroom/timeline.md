# Idea to alpha — the weeks

The record of how long Reprise took, written down once so that nothing has to
count it again.

**Why a file and not a derivation.** The obvious sources both lie. Plan records
are deleted when their work lands (`docs/plans/README.md` says so), and a count
taken from them shrinks every time the repository is tidied — a number that
falls as the project grows. Git history knows the dates exactly, but CI checks
out with `fetch-depth: 1` and cannot see them at all. So the span is a decision,
recorded deliberately, and this file is where it is recorded.

**The convention.** Weeks run Saturday to Friday from the first commit,
`docs: design document for Musikbox (Rhythmbox successor)` on **2026-07-11** —
the idea itself, not the first line of product code. Dates are ISO; the display
form (`11–17 Jul`) is computed by the build so the dates are never maintained
twice. The number of weeks the site prints is the number of rows here.

**What "What landed" is.** A statement about the project, agreed with its
author, not a summary generated from commit subjects. Four of the five themes
carry an anchor in the history; `DEPTH` carries an absence, which is the point
of that week.

| Week | Span | Theme | What landed |
|---|---|---|---|
| 1 | 2026-07-11 … 2026-07-17 | CORE | The idea, the workspace split into `reprise-core` and a Linux platform layer, and the UX rulebook that has governed every change since. |
| 2 | 2026-07-18 … 2026-07-24 | SURFACES | One frontend became four: `reprise-cli`, `reprise-mcp` and `reprise-stems` joined the GNOME app. |
| 3 | 2026-07-25 … 2026-07-31 | DEPTH | No new surface. The single-owner runtime, its versioned protocol and its client went in underneath the ones that already existed. |
| 4 | 2026-08-01 … 2026-08-07 | ANDROID | The shared presentation layer, then the FFI bridge and the Android app on top of it — the library running on a phone. |
| 5 | 2026-08-08 … 2026-08-14 | SIGNATURE | The GNOME conformance rulebook, and the showroom itself: a prerendered page that reads its own numbers out of the tree. |

## The anchors

Taken with `git log --diff-filter=A` against `origin/dev` in a full clone, on
**author** dates. Committer dates are worthless here: rebases and squash merges
rewrite them, so a `git log --since` over them assigns commits to the wrong
week.

| Week | Anchor |
|---|---|
| 1 | `docs/ux-rules.md` first written 2026-07-17; the workspace split into `reprise-core`, `reprise-gnome` and `reprise-platform-linux` on 2026-07-12 |
| 2 | `reprise-cli`, `reprise-mcp` and `reprise-stems` first appear 2026-07-21 |
| 3 | no new frontend; `reprise-runtime-protocol` and `reprise-runtime` 2026-07-28, `reprise-runtime-client` 2026-07-29 |
| 4 | `reprise-view` 2026-08-02; `android/` and `crates/reprise-android-ffi` 2026-08-03 |
| 5 | the GNOME conformance rulebook 2026-08-12; `showroom/` 2026-08-14 |

Commits per week, merges excluded: 955 · 735 · 409 · 142 · 246.

## For the build

`showroom/vite.config.ts` reads the first table through `readTimeline()` and
serves it as `virtual:build-timeline`. Every assertion there throws rather than
shrinking quietly: a row that will not parse, a date that is not ISO, weeks out
of order, and any gap or overlap between two weeks are all errors, because the
failure this file could otherwise produce is a shorter timeline that still looks
finished. Adding a week means adding a row — nothing in the page states the
count.
