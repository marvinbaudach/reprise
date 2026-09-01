# The showreel film on the showcase page — handover

**Session:** 2026-08-31 → 2026-09-01
**State on `origin/dev`:** `2ab2a44509` — the film is **not** on the page, and
everything needed to put it back is in the tree.

---

## Where things stand

The film was mounted and then withdrawn, both in this session.

| PR | What it did |
|---|---|
| **#776** `8a033f0e3d` | Mounted the film in CH.03 beside the gallery, click-to-play, `preload="none"`. Wrote the missing `showreel.css`. Made the captions track reachable. |
| **#782** `2ab2a44509` | Took it back off the page. The user's reason: *the film is not yet in a state to be shown.* |

Net effect on `dev`: `ChapterThree.tsx` is byte-identical to its state before
#776. Everything else #776 built still exists and is still correct.

**Kept on purpose — this is a withdrawal, not an undo:**

- `showroom/src/components/showcase/ShowreelFilm.tsx` — click-to-play, no
  self-start, three controls (play, sound, captions)
- `showroom/src/components/showcase/showreel.css` — written in #776; it did not
  exist before, and the component had always imported it
- `showroom/public/media/showreel/` — 7 files, untouched

**Bringing it back is one element**: `<ShowreelFilm />` after `<ProductGallery />`
in `ChapterThree.tsx`, plus flipping the first test in
`showroom/tests/showreel-film.test.mjs` back around. Nothing needs rebuilding.

## What was open

All four points were resolved by
`docs/plans/the-film-waits-outside-the-deploy.md`:

1. **Resolved — the 15.83 MB web ladder no longer deploys.** The seven files
   moved from `showroom/public/media/showreel/` to
   `showroom/media/showreel/`, so Vite no longer copies them to the Pages
   artifact. They remain in the repository.

2. **Resolved — the wrong cross-reference now points to open point 5.** That is
   the smaller re-encode decision the master size actually informs.

3. **Resolved — the missing asset-size-gate fact is restored in open point 5.**
   The same point now distinguishes the 19.7 MB master from the 15.83 MB web
   ladder and records the per-visitor maximum.

4. **Resolved — all four deferred #776 findings landed.** `onVolumeChange` now
   keeps the sound label synchronized with the video element, the Sound glyph
   names the action like its label, the focused pill keeps its radius by higher
   selector specificity while inheriting the global outline, and the 900 px CSS
   breakpoint explicitly names `SMALL_VIEWPORT` as its matching source ladder.

## Traps this session actually hit

**A skipped job made a broken one look green.** The contract job runs *Verify
project source quality* before *Verify repository and workflow contracts*. Step 8
had been red on the showreel test since #761, so step 9 was `skipped` on every
dev run and its own failure was invisible. #776 turned step 8 green — step 9 then
ran for the first time and failed on ShellCheck warnings in `scripts/showreel/`.
Not caused by #776; uncovered by it. Diagnosed by comparing per-step conclusions
between the two runs, not by reading the top-level verdict. #778 (another
session) fixed it; my duplicate #779 was closed after the rebase reported
`skipped previously applied commit`.

**An assertion that can never fail.** #776's first attempt pinned the
click-to-play decision with `assert.doesNotMatch(chapter, /\bautoplay\b/)` against
the built HTML. Worthless: the autoplay mechanism it guarded against called
`video.play()` from an effect and never set an `autoplay` attribute, and the
prerenderer does not run effects. Replaced with a **source-level** test,
`the film never starts itself`, forbidding `IntersectionObserver` and `useEffect`
in the component. Proven by mutation probe: reinstating the observer makes
exactly that test fail and leaves the other three green.

→ **That test now constrains every future edit to `ShowreelFilm.tsx`: no
`useEffect`, ever.** Anything the component needs must be a JSX event prop or a
`useCallback`. This is deliberate, and it will bite an edit that does not know.

**`phase` must be set on the worktree's plan copy.** Writing it to
`docs/plans/<slug>.md` relative to the main checkout puts it in an untracked file
on an unrelated branch; the branch's own plan never advances and `land.sh`
swallows its own failed `git add`. Cost PR #753 previously. Verify after landing
with `git show origin/dev:docs/plans/<slug>.md | head -8`.

**`dev` requires a green PR check before merging.** Ruleset `dev-pr-boundary`
(id 20937610, `enforcement: active`) requires *Quality gate* on the PR. The
pipeline skill's "never wait for CI" no longer applies to the merge itself.
The gate proves nothing about the code though — `ci-paths.sh --suite-skip`
returns `true` unconditionally on `pull_request`, so every suite shows `SKIPPED`.
The real evidence is the local run.

**Two measurement mistakes worth not repeating.** A log redirect into the
session scratchpad failed silently because the tmpfs had been wiped; `npm ci`
never ran, and a trailing `echo` still made the task report exit 0 — the wrapper's
status, not the build's. Put long-run logs outside `/tmp`. And in the browser,
reading a React label synchronously right after `.click()` shows the *old* text,
because the re-render has not happened yet; two evals, not one.

**A showroom build leaves stale passthrough assets in `dist/`.** After the D1a
mutation probe, `du -sb dist/media/showreel` still read 15,830,922 because the
build does not empty stale files, so clear `dist` before measuring the after
artifact. This is not a deploy risk: `dist` is untracked and Pages builds from
a fresh checkout.

## Verifying the current state

```
git show origin/dev:showroom/src/components/chapters/ChapterThree.tsx | grep -c ShowreelFilm   # 0
git ls-tree origin/dev showroom/public/media/showreel/ --name-only | wc -l                     # 7
cd showroom && npm ci && npm test    # 96/96
```

The visual check of the plate was done before the withdrawal and passed: exact
16:9 at 1440px and 780px, control strip single-row at 780px (329px of buttons in
a 698px strip), the 720p ladder step selected below 900px, the captions button
cycling `disabled → showing → hidden`, and — on a clean load with no click —
`readyState: 0`, `networkState: 1`, `paused: true`, `muted: true`. Nothing is
fetched until a reader asks. That evidence still describes the component; it just
is not on a page any more.

Note the MCP Chrome capture path renders this page as a uniform dark rectangle
even though the DOM, colours and fonts are correct. Screenshots came from
`chromium --headless=new --screenshot` against an isolated page built from the
`dist/index.html` fragment plus the built stylesheet, served under `/reprise/` —
the base path matters, or every asset 404s and the plate looks broken for the
wrong reason.
