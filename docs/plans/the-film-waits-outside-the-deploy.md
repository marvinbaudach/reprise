---
slug: the-film-waits-outside-the-deploy
worktree: /home/marvin/Projects/reprise-the-film-waits-outside-the-deploy
branch: feature/the-film-waits-outside-the-deploy
phase: planned
codex_session:
created: 2026-09-01
---
# The film waits outside the deploy

## Why

`docs/plans/showreel-film-on-the-showcase-page.HANDOFF.md` closed PR #782 with
three things left open. The user picked all three:

1. **19.7 MB still ships.** The film was taken off the page, but the encodes
   stayed in `showroom/public/`, which Vite copies wholesale into `dist/` —
   and `.github/workflows/pages.yml:66` uploads `showroom/dist` to GitHub
   Pages. Nothing links them, so no visitor pays for them, but they are
   publicly fetchable and they sit in every deploy artifact.
2. **Two documentation debts in `reprise-showreel.HANDOFF.md`** — one wrong
   cross-reference, one fact lost in a rewrite.
3. **Four deferred review findings from #776**, none of them landed. A Codex
   run for these was stopped mid-flight when the withdrawal came; its worktree
   and branch are gone, nothing was applied.

## The constraint that governs every edit here

`showroom/tests/showreel-film.test.mjs` contains `the film never starts itself`,
which forbids `IntersectionObserver` **and `useEffect`** in `ShowreelFilm.tsx`
at source level. That is deliberate and mutation-proven. Everything this plan
adds to the component must therefore be a JSX event prop or a `useCallback`.
No exceptions, and do not "fix" the test.

---

## D1 — The encodes move out of the deploy

Vite treats exactly one directory as static passthrough: `public/`. Moving the
seven files one level up takes them out of `dist/` and out of the Pages
artifact, while keeping them in the repository and in the same subtree.

**Move** (`git mv`, so history follows — do not delete and re-add):

```
showroom/public/media/showreel/  ->  showroom/media/showreel/
```

All seven files: `showreel-1080.mp4`, `showreel-1080.webm`, `showreel-720.mp4`,
`showreel-720.webm`, `showreel-poster.jpg`, `showreel-poster.webp`,
`showreel.vtt`.

**Two references follow the files:**

- `scripts/showreel/encode-web.sh:14` — the `OUT` default resolves to
  `…/showroom/public/media/showreel`. Point it at `…/showroom/media/showreel`,
  so a re-encode does not silently recreate the directory inside `public/`.
- `showroom/tests/showreel-film.test.mjs:7` — `filmDir` joins
  `'public', 'media', 'showreel'`, and feeds **two** tests (`every file the
  film section names is actually shipped` and `the caption cues stay inside the
  cut`). Derive it from the mount state instead of hard-coding either path:

  ```js
  const mounted = /ShowreelFilm/.test(await readFile(chapterThreeSource, 'utf8'));
  const filmDir = mounted
    ? join(showroomRoot, 'public', 'media', 'showreel')
    : join(showroomRoot, 'media', 'showreel');
  ```

  Three lines, and they are what keeps the way back at "`git mv` plus one
  element" — neither existing test has to be touched on the return trip.

- **Rename the third test.** It is called `every file the film section names is
  actually shipped`. After the move they are deliberately *not* shipped; the
  claim becomes "present in the repository". Rename accordingly — a test whose
  name asserts the opposite of the decision is how the next session
  misunderstands it.

Note `showreel-poster.jpg` is named by no test and by no component — only
`encode-web.sh:56` writes it. It moves with the other six anyway; it is part of
the same encode run.

**One reference does *not* follow the files:** `ShowreelFilm.tsx:5`,
`const FILM_BASE = \`${BASE_URL}media/showreel/\``. That is a runtime URL, not a
repository path, and it stays exactly as it is — it describes where the files
must be *served* from once the film is on the page again. It gets a comment
saying so (see D2).

### D1a — A test that couples the two states

This is the point of the whole task, and prose in a handover will not hold it.
The film being off the page and the encodes being out of `public/` are one
decision, and a future session that re-adds `<ShowreelFilm />` without moving
the files back gets a page whose every `<source>` 404s — the exact failure the
withdrawal was supposed to make impossible to stumble into.

Add to `showroom/tests/showreel-film.test.mjs` a test — suggested name
`the encodes are served exactly when the film is on the page` — asserting the
two legal states and nothing else:

- **mounted** (`ChapterThree.tsx` references `ShowreelFilm`) → the seven files
  must exist under `showroom/public/media/showreel/`
- **withdrawn** (it does not) → `showroom/public/media/showreel/` must not
  exist, and the seven files must be under `showroom/media/showreel/`

Read the mount state from `ChapterThree.tsx` source the same way the existing
first test does, so there is one source of truth for "is it mounted".

**Mutation proof required, and the axis matters.** The obvious probes both
redden two tests, which makes them useless as proof:

- re-adding `<ShowreelFilm />` also reddens `CH.03 does not carry the film yet`
- `git mv`-ing the encodes back into `public/` empties `showroom/media/showreel/`
  and so also reddens the two tests fed by `filmDir`

Probe by **copying, not moving**: leave `ChapterThree.tsx` untouched and copy
the seven files into `showroom/public/media/showreel/` so both directories are
populated. `showroom/media/showreel/` stays full, the mount state does not
change, and exactly the new test can go red. That is also the realistic
failure — an `encode-web.sh` run with a stale `OUT` puts them back without
anyone deciding to. Delete the copy afterwards.

### Not in scope

Deleting the encodes, or re-encoding at a higher CRF (open point 5 of
`reprise-showreel.HANDOFF.md`). The files stay in the repository.

---

## D2 — The four deferred findings from #776

All four are in `showroom/src/components/showcase/`.

**(a) `ShowreelFilm.tsx` — the sound label goes stale.**
A reader who mutes through the browser's own context menu (Firefox has one)
changes `video.muted` without going through `toggleSound`, so the button keeps
saying `Sound on`. Both callbacks read `video.muted` off the DOM, so only the
label lies, and only until the next click — but the fix is one prop:

```tsx
onVolumeChange={(event) => setMuted(event.currentTarget.muted)}
```

on the `<video>`, beside `onPlay` / `onPause`. A JSX event prop, so the
`useEffect` ban is untouched.

Once this exists it is the single source of truth for `muted`: assigning
`video.muted` in `toggleSound` fires `volumechange` synchronously, so that
callback's own `setMuted(next)` becomes redundant. Dropping it is the DRY-er
shape; keeping it is harmless. Prefer dropping it, but do not spend a round
trip on it.

**(b) `ShowreelFilm.tsx` — the three buttons use two icon conventions.**
Play and Captions name the *action* the button performs; Sound names the
*state* the film is in, while its own label names the action:

| state | icon today | label | reads as |
|---|---|---|---|
| playing | `❙❙` | `Pause` | action + action ✓ |
| muted | `🔇` | `Sound on` | state + action ✗ |

Flip the Sound icon to the action, matching its label and the other two:
`muted ? '🔊' : '🔇'`. Labels do not change. Checked: no test in
`showreel-film.test.mjs` asserts on any of the glyphs, so nothing else moves.

**(c) `showreel.css:71-74` — `.film__control:focus-visible` is redundant, but
not for the reason the finding gives, and not deletable.** The finding says
byte-identical to the global rule. It is not: `global.css:76-80` also sets
`border-radius: var(--radius-sharp)` (`tokens.css:52` — `2px`). Only `outline`
and `outline-offset` are duplicated.

That matters because `.film__control` sets `border-radius: 999px` at
specificity (0,1,0), and the global `:focus-visible` sets `2px` at the same
(0,1,0) — a pseudo-class counts as a class. At equal specificity the later
block in the emitted stylesheet wins, so whether the pill squares off on
keyboard focus is decided by bundle order.

**Measured on `origin/dev` (2ab2a44509), and this is why the block stays:**
`.film__control` does not appear in the emitted CSS at all. The build emits one
stylesheet, `dist/assets/style-*.css`, and `ShowreelFilm.tsx` has no importer
anywhere in `src/` — so Vite never reaches `showreel.css` and never bundles it.
The conflict is not currently observable, and it does not exist in production;
it appears the moment the film is mounted again, and its outcome then depends
on module-graph order, which is not fixed by anything in this repository.

So do not write an order claim into a comment — it cannot be checked today and
could be wrong tomorrow. Take the fix that does not depend on order at all:
`.film__control:focus-visible` is (0,2,0) and beats the global rule's (0,1,0)
in every ordering. Keep the block, drop the two declarations that genuinely are
redundant, and state the specificity reason:

```css
/*
 * The global :focus-visible (global.css) also sets --radius-sharp, at the same
 * specificity as .film__control's own 999px — so at equal weight the emitted
 * order would decide whether the pill squares off on focus. Restating the
 * radius here wins on specificity instead, whatever order the bundler picks.
 * outline and outline-offset come from the global rule unchanged.
 */
.film__control:focus-visible {
  border-radius: 999px;
}
```

`ChapterTwo.css:369-373` writes an explicit radius at exactly this spot, so
someone has met this cascade before. The mechanism is site-wide, not
showreel-specific — note it in one sentence in `reprise-showreel.HANDOFF.md`
and leave `global.css` alone. Widening this into a global fix is not in scope.

**(d) `showreel.css:82` — `@media (max-width: 900px)` is raw px where the
codebase uses rem.** The px is *correct*: it has to match the component's
`SMALL_VIEWPORT` constant, which is the `media` attribute on the `<source>`
ladder, and that is evaluated by the media selection algorithm against px. It
carries no comment, so a later editor "fixing" it to rem silently desyncs the
CSS from the ladder. Add the comment naming `SMALL_VIEWPORT` in
`ShowreelFilm.tsx` as the thing it must equal.

---

## D3 — The documentation debts

**In `docs/plans/reprise-showreel.HANDOFF.md`:**

1. **Wrong cross-reference (~line 130, end of "The cinematic layer").** Reads
   *"That number decides open point 3 more than any taste question does"* — the
   number being 19.7 MB. It was written when point 3 asked whether the film
   belonged on the showroom at all. Point 3 is now the placement history; the
   point the size actually decides is **5**, the smaller re-encode. Numerically
   valid, semantically wrong, and a code review passed it because it still
   resolves to *a* list item. Point it at 5.

2. **The lost fact.** Point 3's rewrite in #776 dropped *"there is no asset-size
   gate in CI"*. Restore it in **point 5**, where it belongs: 19.7 MB was
   committed and shipped and nothing in the pipeline complained.

3. **Point 5 is now out of date on its own premise.** It reads *"If the size
   ever matters — and on the showroom it does"*. After D1 the encodes are not
   in the deploy, so the size does not currently cost anything; it becomes a
   precondition for putting the film back. Reword to that.

4. **Point 3 gains the move.** It currently promises that bringing the film
   back "means re-adding one element rather than rebuilding anything". After D1
   that is two steps: `git mv` the encodes back into `public/media/showreel/`,
   then re-add the element. Say so, and name the test from D1a as the thing
   that will catch a half-done return.

5. **The 19.7 MB figure is wrong wherever it describes the deploy.** Measured
   on a clean `origin/dev` build: `dist` is 18,753,781 bytes, of which
   `dist/media/showreel` is 15,830,922 — 84.4 %. The seven repository files sum
   to exactly that. 19.7 MB is the **master with the push-in**, which is what
   this file's own "The cinematic layer" section correctly says
   ("19.7 MB against 5.7 MB for the same film without it"); it is not the size
   of the web ladder. `showreel-film-never-mounted.findings.md:17` had it right
   at "16 MB". Also worth stating where the size is discussed: a visitor
   downloads exactly **one** of the four encodes, at most 6.2 MB — the ladder's
   total is a repository and artifact cost, never a bandwidth cost.

**In `docs/plans/showreel-film-on-the-showcase-page.HANDOFF.md`:** its open
point 1 repeats the wrong number ("The 19.7 MB still deploys"). Correct it to
15.83 MB when marking it resolved. Then open points 1
(the 19.7 MB), 2 and 3 (the documentation debts) and 4 (the deferred findings)
are all resolved by this plan. Mark them resolved and name this plan, rather
than deleting them — the handover is a record.

---

## Verification

Local, in the worktree. The Quality gate on the PR proves nothing here
(`ci-paths.sh --suite-skip` returns true unconditionally on `pull_request`), so
the local run is the evidence.

1. **The suite.** `cd showroom && npm ci && npm run build && npm test` — the
   build is not optional: `CH.03 does not carry the film yet` reads
   `dist/index.html`. 96 passing before,
   97 after (D1a adds one). Log outside `/tmp`; a wiped tmpfs has silently
   swallowed an `npm ci` in this workstream before, and a trailing `echo` still
   reported exit 0.
2. **The mutation probe for D1a**, as specified in D1a. This is the one that
   makes the new test worth having.
3. **The deploy actually shrinks, against a recorded before-arm.** The
   before-arm is already measured on `origin/dev` (2ab2a44509):

   ```
   du -sb dist                 18,753,781
   du -sb dist/media/showreel  15,830,922
   ls dist/media/              showreel  showroom
   ```

   After the move, `npm run build && du -sb dist && ls dist/media/` must show
   **no `showreel` directory** and a `dist` of roughly **2,922,859 bytes**.
   Record the actual number. A build that merely succeeds is not evidence.
4. **Nothing else can undo the move.** Already checked, so do not re-derive it:
   `vite.config.ts` sets no `publicDir` override and contains no copy plugin or
   `fs.cp`/`copyFileSync` call; `prerender.mjs` only rewrites `dist/index.html`
   and copies nothing. Vite's default `public/` passthrough is the only
   mechanism. Likewise `showroom/media/` is unclaimed: it does not exist on
   `dev`, `tsconfig.json` includes only `src` and `vite.config.ts`, Biome's
   `files.includes` is an explicit allowlist that omits it, and the test script
   globs `tests/*.test.mjs` only. No config change is needed to host it.
5. **No visual check is possible for the film itself** — it is not on a page.
   Do not mount it to look at it; that is the decision this plan exists to
   preserve. D2's changes are covered by (1) and (4).

## Traps carried over

- **`phase` must be set on the worktree's plan copy**, not on the main
  checkout's. Writing it relative to the main checkout puts it in an untracked
  file on an unrelated branch, the branch's own plan never advances, and
  `land.sh` swallows its own failed `git add`. Verify after landing with
  `git show origin/dev:docs/plans/the-film-waits-outside-the-deploy.md | head -8`.
- **`dev` requires a green PR check before merging** (ruleset `dev-pr-boundary`,
  id 20937610, active). The pipeline's "never wait for CI" does not apply to
  the merge itself.
- **The contract job hides broken steps behind skipped ones.** *Verify project
  source quality* runs before *Verify repository and workflow contracts*; a red
  step 8 makes step 9 `skipped` and its failure invisible. Compare per-step
  conclusions, not the top-level verdict.
