# TODO — Showroom gallery: swipe to change images, and a cold-start scaling flash

Reported 2026-08-31. **Item 2 (the flash) is diagnosed and fixed** — see below.
Item 1 (the swipe gesture) is untouched: it is a feature, not a bug, and it was
not part of "kümmere dich um die Bugs".

Sibling note: [showroom-scroll-jank-and-x-overflow.TODO.md](./showroom-scroll-jank-and-x-overflow.TODO.md).

## What was reported

> "Showroom der bildwechsel sollte auch mit wischgeste klappen. Tinder mäßig.
> Aktuell sehe ich noch probs beim kaltstart beim wechsel der Bilder. Auf dem
> Handy. A[ls ob] das Bild nochmal anders skaliert kurz vor dem wechsel"

1. **Feature — swipe to change images.** Not started.
2. **Bug — scaling flash on cold start.** Fixed.

## 2 — The scaling flash: what it actually was

Not a transition and not a missing intrinsic size. The mechanism, from the
source:

- `.lightbox__frame` takes its `aspect-ratio` from `--lb-ratio`, which the
  component writes as `capture.width / capture.height` of the **requested**
  capture (`showroom/src/components/showcase/lightbox.css:127`).
- The `<img>` inside it is `width: 100%; height: 100%; object-fit: contain`, and
  React reconciles the **same DOM node** across a change of index — only `src`
  and `srcSet` change.

So the moment the reader presses →, the frame adopts the new ratio while the
browser is still painting the previous bitmap. The outgoing screenshot is
re-letterboxed into the incoming shot's box — a desktop capture at 1.60 into a
phone's 0.45 — and stays that way until the new file arrives. Warm, that is one
frame and invisible. Cold, on a phone, it is seconds.

### Measured, before and after

Two builds of the same tree, one server each, Chrome at 390×844 dpr 3, cache
disabled, 500 kbps / 250 ms latency. Six presses through the gallery; the metric
is the window between the frame box changing size and the new image being
painted — i.e. exactly how long the old picture is shown at the wrong scale:

| build | worst stale-scaled window | total over 4 ratio-changing presses |
|---|---|---|
| before | **2199 ms** | **5435 ms** |
| after | **0 ms** | **0 ms** |

Per press, after the fix, the box changes in the same frame the image becomes
ready (785/785, 2223/2223, 1565/1565, 963/963 ms). Two of the six presses were
between captures of equal ratio and change no box in either build.

### The fix

`showroom/src/components/showcase/Lightbox.tsx` — the frame is built around a
`shownIndex` that lags `activeIndex` until the incoming file has **decoded**
(`new Image()` + `decode()`, falling back to `onload`/`onerror`). Ratio and
source then move together, so there is no moment where they disagree.

- A later press supersedes the pending preload rather than racing it; verified
  in the browser: five presses inside 250 ms on a throttled cold connection land
  on 06/09 from 01/09 and never show an intermediate.
- The zoom state moved from `activeIndex` to `shownIndex` — otherwise an origin
  measured on the outgoing picture would apply to the incoming one.
- While the frame holds, the dialog carries `data-swapping="true"` and
  `aria-busy`, and the counter dims. Without it a press on a slow connection
  looks ignored.
- Covered by `showroom/tests/shot-tile-lightbox.test.mjs`.

## 1 — The swipe gesture: still open

The questions from the first draft still stand and still decide the shape of the
work:

- **"Tinder-style" — how far?** Card follows the finger (drag + rotation +
  opacity), or a flick with a threshold and a snap-back? That is the difference
  between a gesture handler and a small touch-event branch.
- **Replace or complement** the arrow keys and the header buttons, which must
  keep working.
- `showroom/src/hooks/useReducedMotion.ts` gates motion here; any gesture
  animation has to respect it.

Note for whoever picks it up: the lightbox now holds the picture until the next
one has decoded. A swipe that moves the card with the finger has to decide what
it shows during that hold — the same question the `data-swapping` state answers
for the buttons.
