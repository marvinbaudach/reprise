---
slug: showreel-film-joins-the-gallery
worktree: /home/marvin/Projects/reprise-showreel-film-joins-the-gallery
branch: feature/showreel-film-joins-the-gallery
phase: planned
codex_session:
created: 2026-08-31
---
# The showreel film joins the gallery in CH.03

`showroom/tests/showreel-film.test.mjs:12` has been red on `dev` since #761
(`808fc21590`). That commit shipped `ShowreelFilm.tsx`, its test and 16 MB of
encodes, and never mounted the component: `ChapterThree.tsx` was not touched,
`ShowreelFilm` has no call site in `src/`, and `dist/index.html` still holds the
mosaic alone in `#ch-03`. Full record in
`docs/plans/showreel-film-never-mounted.findings.md`.

Two things follow from the diagnosis and shape this plan.

**The component does not build.** It opens with `import './showreel.css'`, and
`showroom/src/components/showcase/showreel.css` does not exist. Vite has never
resolved it because nothing imports the component. Mounting it without writing
that file turns the red test into a red build.

**The decision the test presumes was never taken.** `reprise-showreel.HANDOFF.md`
item 3 lists "whether the film belongs on the showroom — and if so, whether it
replaces plates or joins them" as open, and recommends a click-to-play
`<video preload="none">` *beside* the gallery: "nothing about this page should
hand a first-time visitor 19.7 MB unasked." The component that landed autoplays
on an `IntersectionObserver` at `preload="metadata"`. The user settled it on
2026-08-31: **the film joins the gallery, it does not replace it, and it does
not start on its own.** The test encodes the other answer and is therefore part
of what changes here.

Scope note: the encodes stay as they are. A smaller re-encode is a separate
question and stays open — with `preload="none"` nothing is fetched until a
reader asks for it, which is what made the size urgent.

## Before Codex starts

The worktree is cut from `origin/dev` (`worktree.sh` does this by itself). Copy
this plan and `docs/plans/showreel-film-never-mounted.findings.md` into it and
commit them on the feature branch before the Codex run — both live untracked in
the main checkout, which sits on an unrelated branch, and `land.sh` finds a plan
by grepping `^branch: <BR>$` across `docs/plans/*.md` **on the branch being
landed**.

## Tasks

### 1. Write `showroom/src/components/showcase/showreel.css`

The stylesheet the component already imports, for the classes it already
renders: `.film-frame`, `.film`, `.film__video`, `.film__controls`,
`.film__control`, `.film__caption`, and the `.film-heading` block.

Follow the page's existing idiom rather than inventing one — read
`src/components/showcase/showcase.css` and `shot-tile.css` first and reuse their
custom properties, radii, spacing and control styling. The plate sits inside a
`.frame`, directly under a `data-showcase="product-gallery"` block, so it has to
read as a sibling of the gallery plates, not as a foreign element.

Requirements that are not taste:

- the video keeps its 16:9 box without layout shift while it is still a poster
  (the element carries `width={1920} height={1080}`);
- the controls are reachable by keyboard and have a visible focus ring, matching
  whatever the other buttons on the page use;
- it holds up at the `(max-width: 900px)` breakpoint the component already
  treats as the layout's own.

### 2. Turn the film into click-to-play — `ShowreelFilm.tsx`

- Delete the `useEffect` with the `IntersectionObserver`, the `PLAY_THRESHOLD`
  constant and the `claimed` ref. With no self-start there is no hand-off left
  to claim.
- Delete the `reducedMotion` prop and the `ShowreelFilmProps` interface with it.
  Nothing moves until a reader clicks, so the preference has no job here. Update
  the call site in task 3.
- `preload="metadata"` → `preload="none"`. The poster stays
  `showreel-poster.webp` (54 KB, shipped, and pinned by the test).
- Everything else stays: `muted`, `loop`, `playsInline`, both controls, the four
  `<source>` entries in their current order, the captions `<track>`, the
  `figcaption`.
- Rewrite the component doc comment. It currently explains a self-start and a
  bandwidth argument that no longer applies; it should explain the choice that
  is now true — the film costs nothing until it is asked for, and it starts
  muted because a landing page does not make noise unbidden.

### 3. Mount it beside the gallery — `ChapterThree.tsx`

Render `<ShowreelFilm />` inside `#ch-03`, **after** `<ProductGallery />`, with
no props. `ProductGallery` stays exactly where it is: `page-contract.test.mjs:35`
needs `data-layout="design-mosaic"` page-wide and this is its only call site.

`ChapterThree`'s own `reducedMotion` prop stays — `SpectralSeekTrack` still uses
it.

### 4. Make the test say what the page now does — `showreel-film.test.mjs`

- Rename the first test. "CH.03 carries the film where the screenshot mosaic
  used to be" describes the rejected answer. It carries the film *beside* the
  mosaic.
- Replace `assert.doesNotMatch(chapter, /data-layout="design-mosaic"/)` with the
  matching assertion: both belong to the chapter now.
- Add the two assertions that pin the decision, so a later change cannot quietly
  restore the autoplay: the `<video>` carries `preload="none"`, and the chapter
  contains no `autoplay` attribute.
- Everything else in the file stays untouched, including both other tests.

### 5. Close the open item — `docs/plans/reprise-showreel.HANDOFF.md`

Item 3 is answered. Note under it what was decided and on what date: the film
joins the gallery in CH.03 as a click-to-play `preload="none"` plate; the
smaller re-encode stays open. A next session must not re-litigate this.

## Verification

Run inside the worktree, in `showroom/`:

- `npm ci` — **install, never symlink `node_modules` from another checkout.** A
  symlink points outside the Codex sandbox, Vite cannot write its artefacts, and
  the run stalls for 45 minutes without saying why (measured 2026-08-30).
- `npm test` — builds first, then runs the suite. All 95 tests green.
  `CH.03 …` is the one that was red.
- `npm run lint` and `npm run typecheck`.

Check the assertions against the **built** `dist/index.html`, not the JSX:
`ShowreelFilm` has never been through `entry-server.tsx`/`prerender.mjs`, so do
not assume React's server renderer emits `muted`, the `poster` attribute and all
four `<source>` names in the order the test wants. If `muted` does not survive
SSR, that is a component fix, not a test relaxation.

Two neighbouring tests constrain the seam and must stay green:
`page-vitals.test.mjs:68` slices the page from `data-layout="design-mosaic"` to
the end and forbids `fetchPriority="high"` and `data-loading="false"` after that
point — which is why the film goes after the gallery and carries no `<img>`;
`page-contract.test.mjs:37` pins the page-wide `loading="eager"` count at
exactly 2, both in the hero.

## Parallelität

**No cut.** Four of the five tasks meet in one seam: the stylesheet exists only
for the component, the component's prop change is the call site's change, and
the test asserts against the HTML those three produce. Splitting them would buy
nothing and hand two agents the same build.

File ownership for the single strand:

- `showroom/src/components/showcase/showreel.css` (new)
- `showroom/src/components/showcase/ShowreelFilm.tsx`
- `showroom/src/components/chapters/ChapterThree.tsx`
- `showroom/tests/showreel-film.test.mjs`
- `docs/plans/reprise-showreel.HANDOFF.md`

No other file is touched. In particular `ProductGallery.tsx`, the other tests
and everything under `public/media/` stay as they are.
