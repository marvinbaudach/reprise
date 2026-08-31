# TODO — Showroom: scrolling stutters, and the page scrolls sideways

Reported 2026-08-31. **Symptom 2 (the sideways scroll) is diagnosed, fixed and
measured.** **Symptom 1 (the stutter) could not be reproduced** — six campaigns,
the obvious suspect measurably refuted, nothing changed for it. Both sections
below are findings; the stutter section says what would settle it.

Sibling note: [showroom-gallery-swipe-and-cold-start-flash.TODO.md](./showroom-gallery-swipe-and-cold-start-flash.TODO.md).

## What was reported

> "Beim showroom gibt es den Bug das scrolling nach unten stockt. Ausserdem ist
> x achse scrollbar. Also elemente sind zu noch responsive"

1. **Scrolling down stutters.** Not reproduced — see section 1.
2. **The x axis is scrollable.** Fixed.

The two turned out to be unrelated: the overflow is one rule in the gate strip,
the stutter is not in that part of the page at all.

## 2 — The sideways scroll: what it actually was

`.gate-strip__tick` is the touch target around a 3px mark in chapter two's gate
strip. There are 27 of them, in clusters of three to seven. The base rule lets
each one shrink to nothing (`flex: 1 1 0; min-width: 0`), and a media query then
gave it a floor:

```css
@media (hover: none), (max-width: 46rem) {
  .gate-strip__tick { min-width: 44px; }
}
```

44px is the right floor for a finger — but the comma makes that rule fire on
*every* touch viewport, including a 390px phone, where the clusters lay four to
five across a ~335px row. Seven ticks of 44px in a 60px cluster is 308px of
content in 60px of box, and flex cannot shrink below the floor: the strip ran
past the right edge and took the document's width with it. Measured at 412px the
page laid out **622px wide in a 412px viewport**.

`body { overflow-x: hidden }` was in the stylesheet the whole time and did not
stop it. It suppresses the scrollbar; it does not stop a touch drag from panning
the page, which is why this read as "the x axis is scrollable" on a phone while
a desktop browser looked clean.

### Measured, before and after

Both arms built from the same tree (`origin/dev` at 4982f4f47b vs. the fix),
served as static builds, Chromium 151 headless, one page per viewport width,
scrolled top to bottom first so every reveal has run. The number is the
document's own width against the viewport it was given. It establishes whether
the page itself widens; it does not establish that every descendant stays
inside its own layout box:

| viewport | before: page width | over | after: page width | over |
|---|---|---|---|---|
| 320 | 413 | **+93** | 320 | 0 |
| 360 | 413 | **+53** | 360 | 0 |
| 390 | 420 | **+30** | 390 | 0 |
| 412 | 622 | **+210** | 412 | 0 |
| 430 | 636 | **+206** | 430 | 0 |
| 540 | 660 | **+120** | 540 | 0 |
| 640 | 731 | **+91** | 640 | 0 |
| 768 | 771 | +3 | 771 | +3 |
| 1024 | 1024 | 0 | 1024 | 0 |
| 1440 | 1440 | 0 | 1440 | 0 |

Headless Chromium answers `(hover: none)` whether or not touch is emulated, so
every row above is the touch branch of those media queries — the branch a phone
takes, and the one the report is about. The page-width result remains useful:
the document no longer grows at those widths. The original element probe was
not a containment proof, however, because it counted descendants of
`body { overflow-x: clip }` as contained even when they painted outside their
own cluster.

At **844×390** with touch emulation, four 44px ticks needed **176px** inside a
**73px** `.gate-cluster__marks` box: the ticks ended at x=397 while the box
ended at x=296. Seven ticks needed **308px** inside a **154px** box: the ticks
ended at x=528 while the box ended at x=377. That is up to **151px** of spill
over the following cluster even though the document itself stayed at the
viewport width. The final fix keeps the page-width result and removes that
element-level overlap by wrapping the marks inside each cluster.

### Measured again after the wrap

Same harness, the built branch, touch emulation, page walked top to bottom
first. `minTick` is the narrowest tick anywhere on the page, `spill` the
furthest any tick reaches past its own `.gate-cluster__marks` box, and the last
column is the document width against the viewport with the `overflow-x: clip`
backstop lifted:

| viewport | minTick | spill | ticks overlapping | page over |
|---|---|---|---|---|
| 320 | 60.3 | 0 | 0 | 0 |
| 360 | 46.6 | 0 | 0 | 0 |
| 390 | 51.2 | 0 | 0 | 0 |
| 412 | 54.8 | 0 | 0 | 0 |
| 430 | 57.5 | 0 | 0 | 0 |
| 540 | 44.8 | 0 | 0 | 0 |
| 640 | 45.0 | 0 | 0 | 0 |
| 736 | 44.9 | 0 | 0 | +2 |
| 737 | 66.7 | 0 | 0 | +2 |
| 768 | 76.3 | 0 | 0 | +3 |
| 844 | 51.2 | 0 | 0 | 0 |
| 900 | 43.9 | 0 | 0 | 0 |
| 1024 | 46.6 | 0 | 0 | 0 |

No tick is narrower than its 44px floor anywhere (the 43.9 at 900px is the
fractional layout box of a 44px `min-width`, not a shorter target), nothing
leaves its cluster, and the only page-level overflow left is the hero overhang
named below.

### The fix

`showroom/src/components/chapters/ChapterTwo.css`

- The 44px floor applies at every touch width. `.gate-cluster__marks` wraps, so
  a cluster that cannot hold its marks on one line takes another line instead
  of letting the ticks paint over the next cluster.
- Narrow, `.gate-cluster` takes `flex-basis: calc(50% - var(--gap-tight))`, so
  the clusters go two to a row instead of four or five and each tick gets a
  usable share of half the figure rather than a seventh of a fifth.

`showroom/src/styles/global.css`

- `overflow-x: clip` on `html, body` replaces `overflow-x: hidden` on `body`.
  `clip` creates no scroll container at all, so a sub-pixel remainder can never
  become an axis to drag. It is a backstop and nothing more: the page-width
  table records that axis outcome, while the 844×390 box measurements above
  expose the overlap that clipping hid.

### Two boundary details

- **The touch-target floor.** The 44px floor is back at every touch width. The
  marks wrap inside a narrow cluster rather than buying page width by shrinking
  the targets below that floor; the 23px-wide ticks measured at 390–412px no
  longer fall below WCAG 2.5.8's 24px minimum.
- **+2 to +3px between 736 and 768px.** `.hero-product__phone` is placed
  `right: -5%` on purpose — the phone plate overhangs the desktop plate. Across
  that band the overhang clears the page's inline padding: +2.0px at 736 and
  737, +2.5px at 768. It is the only offender left at any width, it is present
  in `origin/dev` too,
  it is the only offender left at any width, and it is not what the report was
  about.

## 1 — The stutter: not reproduced, and the obvious suspect is refuted

**Nothing was changed for this.** Six measurement campaigns on the fixed tree
could not produce a stutter to fix, and the structural suspect that the first
draft of this note named turned out to cost nothing. What follows is the record,
so the next attempt starts after this work rather than repeating it.

### The harness

Static build served locally, Chromium 151 headless (swiftshader — raster is
software here), 390×844, device pixel ratio 3, CPU throttled 4× and 6×, scrolled
with `Input.synthesizeScrollGesture` (a real compositor scroll, not a
`scrollTo`), six flings of 675px per pass. `prefers-reduced-motion` was checked
and is **not** set in this browser, so the page under measurement is the moving
one, not its still variant.

### What was measured

| arm | result |
|---|---|
| backdrop hidden (ground + oil + grain) | **no cheaper than as shipped** |
| grain / blobs / sweep hidden, each alone | no effect beyond noise |
| ground `background-color` transition off | no effect beyond noise |
| all transitions and animations off | the only arm that ever showed a real drop: paint per pass 0ms vs ~170ms |
| first scroll of a cold load vs. second scroll of the same document | 60fps in both, at dpr3, 4× CPU and 1.5Mbps/150ms |

Frame times across the last three campaigns: **median 16.7ms, p99 16.8ms, at
most one frame over 50ms in a 260-frame pass.** That is a flat 60fps. There is
no stutter in this harness to attribute.

### What that refutes

The backdrop was the obvious candidate — three fixed full-screen layers, one
`mix-blend-mode: screen` over a 150vmax rotating conic gradient with a mask,
another `mix-blend-mode: overlay` over an SVG-noise bitmap, all on infinite
loops. An early campaign appeared to confirm it (three times the frame rate with
the backdrop hidden), and that number was wrong: its arms each ran on a fresh
page load, so first-load compile and decode were charged to whichever arm ran
first — two identical control arms in that campaign came out 33ms and 17ms
apart. Under a protocol that switches arms inside one live document and
reshuffles their order every round, the backdrop arms are indistinguishable from
the control, and a trace says why: hiding the backdrop does not reduce paint
time per scroll pass at all (1332ms vs 1182ms in one campaign — if anything the
wrong way, which is the size of the noise).

**Do not spend the next attempt on `backdrop.css`.** It was measured, twice, two
ways.

### The one thing that did move

Only `*{transition:none;animation:none}` dropped paint to zero, and the backdrop
is not what it was switching off — disabling the backdrop's own motion changed
nothing. What is left are the short-period decorative loops elsewhere on the
page, each repainting its element while it is on screen:

- `shot-tile.css` — `rp-sweep 1.5s linear infinite`
- `architecture.css` — `architecture-flow 2.8s linear infinite`
- `ChapterTwo.css` — `pipeline-flow ... infinite`
- `chapters.css` — `rp-cue 2.6s ease-in-out infinite`

They cost ~170ms of paint per four-second scroll pass here — real, but four
percent of the pass, and they never cost a frame. Worth knowing; not worth a fix
on this evidence.

### What this harness cannot see, and what it points at

Every number above is main-thread cost. The trace categories used here record
`Paint`, `PrePaint`, `UpdateLayoutTree` and `FunctionCall`; raster and the
compositor's own effects barely appear, because this browser rasterises in
software on a machine with no phone GPU in it. So one candidate is invisible to
all six campaigns rather than cleared by them:

```css
.site-header[data-lifted="true"] { backdrop-filter: blur(14px); }
```

`data-lifted` goes true 60px down and stays true for the rest of the page, so a
full-width, 66px-tall blur of whatever is behind the header has to be recomputed
on **every frame the content moves** — which is every frame of a scroll. That is
compositor work, it is the one thing on this page that makes a phone's GPU work
per scroll frame, and it is exactly the kind of cost that is free on a desktop
and expensive on a mid-range phone. It is a suspect, not a finding: nothing here
measured it, and it should not be changed until something does. The device
capture asked for below would settle it in the same pass as everything else —
in a Chrome trace from the phone it would show up as compositor frames, not as
main-thread paint.

### What the next attempt needs

The questions the first draft of this note asked were never answered, and they
are what is missing — none of them can be settled from here:

- **The deployed site, or a local `npm run dev`?** A dev build serves
  unminified modules and double-renders under StrictMode. A phone on that is a
  different page from the one measured above.
- **Which phone and browser**, and is the stutter uniform down the page or tied
  to one section (the seek demo and the visualizer plate carry three canvases,
  which is where a device-specific cost would most plausibly sit)?
- **On what connection?** Everything above was measured after the assets had
  arrived; a cold pass over 1.5Mbps behaved the same, but a real mobile network
  is not a token-bucket emulation.

A capture from the device — Chrome remote debugging over USB, or a screen
recording with a visible finger — would settle in one pass what six campaigns
here could not.
