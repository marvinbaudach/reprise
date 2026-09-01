# Handoff — showroom lightbox resilience (2026-09-01, 03:55)

Supersedes `/home/marvin/Projects/reprise-showroom-bugs/HANDOFF.md`, which is
**stale and wrong in three places**. Read this one.

Worktree `/home/marvin/Projects/reprise-showroom-lightbox-resilience`, branch
`fix/showroom-lightbox-resilience`, cut from `origin/dev` at `496a9a5c36`.

## What actually happened to the old branch

The old handoff describes a live worktree at
`/home/marvin/Projects/reprise-showroom-bugs` on branch
`fix/showroom-scroll-and-overflow` with Codex in flight. None of that was true
when it was written:

- The worktree was already gone — the directory held only the two `.md` files,
  no `.git`, no `showroom/`, and no admin dir under `.git/worktrees/`.
- The branch had been deleted. Its two commits (`95d4433146`, `b2582e8f70`)
  survived only as unreferenced objects.
- Codex never ran. Its log (session `14debe39`, 03:45) is four lines ending
  `Not inside a trusted directory and --skip-git-repo-check was not specified.`
  There is no `.pipeline-codex.md` anywhere. Findings 1-5 were entirely
  unapplied.

The cause is benign: at 22:10 on 2026-08-31 the same session landed the work as
PR #772 (`The gate marks wrap, and the zoom settles before the picture swaps`,
merged `b9b048fc47`) from a *different* branch, `feature/showroom-gate-strip-and-zoom`,
in a *different* worktree. The land script removed that worktree and deleted
that branch — and the `fix/showroom-scroll-and-overflow` worktree went with it.
The handoff was then written into the empty directory by absolute path.

The two orphaned commits are preserved on `rescue/showroom-scroll-and-overflow`,
pushed to origin. They are **obsolete** — see below — and the branch can be
deleted once someone confirms that.

## What is already on dev, and why the rescue branch is obsolete

PR #772 solved both original bugs, and solved the overflow one **better**:

- **Overflow.** The rescue branch dropped the 44px touch floor below 737px,
  which is what created the WCAG 2.5.8 question. dev instead **keeps the 44px
  floor unconditional on touch** and adds `flex-wrap: wrap` to
  `.gate-cluster__marks`, so a cluster that cannot hold its marks on one line
  wraps them internally. **The open decision in the old handoff is therefore
  closed** — there is no AA miss on dev, and nothing to decide.
- **Zoom flash.** dev has `frameZoom = zoom && zoom.index === activeIndex`, so
  an arrow press releases the zoom on the same render as the index change. The
  rescue branch's `=== shownIndex` is the *bug* that finding 2 reported.

Verified by diffing `rescue/showroom-scroll-and-overflow` against `origin/dev`.
Do not resurrect the rescue branch and do not rebase it.

## What is left — and is running now

Four of the five review findings are still unapplied on dev. Confirmed by
grepping `origin/dev`: no `setTimeout` in `Lightbox.tsx`, no `src`/`srcset`
clearing in the preload cleanup, `aria-busy` at line 193 with no live region
anywhere, and both weak test assertions intact (`shot-tile-lightbox.test.mjs`
lines 155/189 and the `[\s\S]*?` match at 166).

Codex is applying them now in this worktree; `.pipeline-task.md` is the brief
and its summary lands in `.pipeline-codex.md`:

1. a stalled download wedges the swap open forever — no timeout on `decode()`;
2. a superseded preload is ignored but never aborted;
3. `aria-busy` does nothing without a live region;
4. two test assertions that pass even with the fix reverted, plus new
   assertions for 1-3.

The old finding 2 (zoom) is dropped from the brief — already fixed on dev.

When it finishes: read `.pipeline-codex.md` and `git diff`, confirm
`npm run lint`, `npm run typecheck` and `npm test` are green from `showroom/`,
and review the diff.

## Still not reproduced, still out of scope

**The scroll stutter.** Six measurement campaigns (390x844 dpr3, CPU throttled
4x and 6x, real compositor swipes, traces) came out flat at 60fps — median
16.7ms, p99 16.8ms. Hiding the whole backdrop does not reduce paint time per
scroll pass, so the obvious suspect is refuted with numbers. Do not spend the
next attempt on `backdrop.css`;
`docs/plans/showroom-scroll-jank-and-x-overflow.TODO.md` says what would
actually settle it (deployed site vs `npm run dev`, which device, whether it is
tied to one section, ideally a capture from the phone).

**The gallery swipe gesture** is a feature, not a bug, and out of scope by the
TODO's own framing.

## Housekeeping

- `showroom/node_modules` here is a symlink to the main checkout's. The shared
  `.git/info/exclude:66` already carries a `node_modules` line. Remove the
  symlink if this worktree goes away.
- `/home/marvin/Projects/reprise-showroom-bugs/` is a dead directory holding
  only the superseded handoff and its brief. Safe to delete once this note has
  been read.
- The `showroom-refactor` wake lock was already held for exactly this run and is
  being reused — release it when Codex is done. No second lock was taken.
