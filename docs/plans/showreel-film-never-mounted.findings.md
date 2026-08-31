# Findings — the red showreel test is an unfinished mount, 2026-08-31

`CH.03 carries the film where the screenshot mosaic used to be`
(`showroom/tests/showreel-film.test.mjs:12`) fails on a pristine `origin/dev`
worktree (reproduced at `b9b048fc47`, `npm ci && npm test` in `showroom/`:
95 tests, 94 pass, this one fails). The other two tests in the file pass.

## What is actually wrong

Nothing is broken. The mount was never written.

#761 (`808fc21590`) shipped three of the four halves of the page change:

- `showroom/src/components/showcase/ShowreelFilm.tsx` — complete, and the only
  commit that ever touched it,
- `showroom/tests/showreel-film.test.mjs` — the test that now fails,
- `showroom/public/media/showreel/*` — 16 MB of encodes, poster and captions,

and never touched `showroom/src/components/chapters/ChapterThree.tsx`. Its last
change is `99fdda07d6` (#695), long before. `ShowreelFilm` has no call site
anywhere in `src/`, so it has never been through `entry-server.tsx` or
`prerender.mjs`. The assertion fails on the first line it reaches:
`#ch-03` in `dist/index.html` still holds `<ProductGallery />` with
`data-layout="design-mosaic"`.

## The component does not build either

`ShowreelFilm.tsx:2` is `import './showreel.css'`, and
`showroom/src/components/showcase/showreel.css` does not exist — #761 shipped the
import without the stylesheet. Vite has never resolved it, because nothing
imports the component. So the missing mount is hiding a second gap: mounting it
as-is turns the red test into a red build. Whichever route is taken, that file
has to be written first.

## Why it is not a one-line fix

The test demands the film **inside** `#ch-03` *and* the mosaic **not** inside
it. But `ProductGallery` has exactly one call site, `ChapterThree.tsx:42`, and
`tests/page-contract.test.mjs:35` requires `data-layout="design-mosaic"` to
survive page-wide. So the test as written cannot be satisfied without moving
the gallery into another chapter — a change to a public landing page, not a
repair.

And the decision it presumes was never taken. `docs/plans/reprise-showreel.HANDOFF.md`
item 3 lists **"whether the film belongs on the showroom — and if so, whether it
replaces plates or joins them"** as open, and recommends the opposite of what
shipped: a click-to-play `<video preload="none">` with an existing plate as its
poster, *beside* the gallery, at a smaller re-encode — "nothing about this page
should hand a first-time visitor 19.7 MB unasked". The component that landed
autoplays on an `IntersectionObserver` at `preload="metadata"`.

The numbers behind that: `public/media/showreel` is 16 MB, against 2.4 MB for
`public/media/showroom`, the whole rest of the page's media.

## The three ways out

1. **Wire it in as written** — `<ShowreelFilm />` into `#ch-03`, gallery moved
   to another chapter. Turns the test green, ships the 16 MB and the autoplay.
2. **Take the page-facing half back out** — drop the component, the test and the
   media, keep `scripts/showreel/`. The film stays a deliverable, not a page.
3. **Beside the gallery**, the way the handoff recommends — film and mosaic both
   in `#ch-03`, click-to-play, `preload="none"`. Needs the test's
   `doesNotMatch(/data-layout="design-mosaic"/)` rewritten.

## What a next attempt should not assume

`page-vitals.test.mjs:68` slices `dist/index.html` from `data-layout="design-mosaic"`
to the end and forbids `loading="lazy"`-less images, `fetchPriority="high"` and
`data-loading="false"` after that point; `page-contract.test.mjs:37` pins the
page-wide `loading="eager"` count at exactly 2. The film carries no `<img>`, so
neither blocks any of the three routes — but moving the gallery moves that
slice, so re-read both before promising a branch is green.

`ShowreelFilm` has never been server-rendered. Whichever route is taken,
verify against the built `dist/index.html` rather than the JSX that `muted`,
`poster="…showreel-poster.webp"` and all four `<source>` names survive React's
server renderer in the order the test wants — `muted` first.

Reproduction worktree: `/home/marvin/Projects/reprise-showreel-red`
(`fix/showreel-film-test`, off `origin/dev`), `node_modules` installed with
`npm ci` inside it.
