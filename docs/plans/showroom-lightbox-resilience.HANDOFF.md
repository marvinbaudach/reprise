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

---

# Check phase — 2026-09-01, phase: reviewed

Gates measured directly (exit codes captured per command, not through a pipe):
`npm run lint` 0, `npm run typecheck` 0, `npm test` 0 (96/96).

Mutation arms, one at a time against a green control, worktree restored between
each: `captures[shownIndex]` -> `captures[activeIndex]`; delete the `setTimeout`
line; drop `aria-live="polite"`; drop the `src`/`srcset` clearing. **All four
turn the suite red.** The assertions bite. (A first attempt was invalid — `cp -i`
silently blocked the restores, so arms ran cumulatively; re-run with
`git checkout` as the restore.)

Reviewers: `typescript-reviewer` and `react-reviewer`, both Sonnet/high.

## Findings, awaiting the user's selection

**A — `{0,7000}` span has 348 characters of headroom (test, latent break).**
`shot-tile-lightbox.test.mjs:155` and `:224`. Measured independently: the gap
between `const capture = captures[shownIndex];` and
`'--lb-ratio': capture.width / capture.height` is 6652 chars against a 7000 cap.
Six to nine more lines anywhere in the component body between those anchors and
both assertions fail — with the message "the lagging capture must drive the
frame ratio", pointing at a regression that does not exist. Note the direction:
the span was suspected of being too loose; it is too tight.

**B — the swap announces only "03 / 12" (accessibility, scope question).**
`Lightbox.tsx:213-215`. The dialog's `aria-labelledby` (h2 title) and
`aria-describedby` both change on a swap but are not live regions, so a screen
reader hears the position and not the picture's identity. Codex did exactly what
the accepted finding asked ("give the counter `aria-live="polite"`"); this says
the accepted finding was itself too narrow. A decision, not a defect.

**C — cleanup regex forbids an interstitial comment (test, brittle).**
`shot-tile-lightbox.test.mjs:193-196`. The four cleanup statements must be
adjacent with only `\s*` between them, so documenting *why* `src` is cleared
before `srcset` would turn the suite red with no behaviour change.

**D — counter regex forbids a sibling ARIA attribute (test, brittle).**
`shot-tile-lightbox.test.mjs:203`. `aria-live="polite"` must immediately follow
`className`. Adding `aria-atomic="true"` — the normal pairing, and what finding
B would want — is rejected outright.

**E — timeout force-commit shares its cleanup with genuine supersession
(speculative, likely a non-issue).** `Lightbox.tsx:103-110`. When the 10s
timeout force-commits, `shownIndex` changes, the effect cleans up, and the
in-flight preload is aborted just as the visible `<img>` requests the same URL.
Reported as a risk the reviewer could not verify statically. Argument against:
React runs passive-effect cleanup *after* DOM mutation, so the `<img>` has
already attached to the request before the preload consumer is cancelled, and
browsers coalesce concurrent requests for the same URL. Recorded for
completeness; do not act on it without a measurement.

C and D are in real tension with the mutation arms: the tight regexes are
exactly why the arms go red. Loosening them trades bite for durability — that
trade is the user's call, not the agent's.

## Not done, deliberately

No fixes applied. That is `/refactor`, and it runs only once the user has picked
which findings are valid — the selection is the input to that phase.

The `showroom-refactor` wake lock is still held. Codex reported releasing it;
that report was wrong (`wake-lock status` still lists it active). Release it when
this work ends.
