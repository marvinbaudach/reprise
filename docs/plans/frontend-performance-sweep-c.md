---
slug: frontend-performance-sweep-c
worktree: /home/marvin/Projects/reprise-frontend-performance-sweep-c
branch: feature/frontend-performance-sweep-c
phase: shipped
codex_session:
created: 2026-08-24
---
# Strand C — Showroom: stop paying for motion nobody asked for

Mother plan: `docs/plans/frontend-performance-sweep.md`. Read it first.

**Owns `showroom/**`.** Nothing outside that path.

Line numbers are against `origin/dev` at `7eaf16e4d3` (re-checked after dev moved; no Android or showroom file changed in between). C1 must produce a number
(see the mother plan's rule); C2 and C3 are hygiene and exempt.

---

## C1 — A moving mouse must not run the whole page choreography

### The defect

`usePageChoreography.ts:186` routes `pointermove` into the same `schedule()` as
scroll. A full `tick()` calls `getBoundingClientRect()` for every section and
every nav link and runs the reveal sweep — but the only thing that depends on
the pointer is `moveOil()` (`usePageChoreography.ts:84–93`). A mouse crossing a
stationary page therefore triggers roughly 60 full layout passes a second for
one parallax offset.

Under `still` (reduced motion) it is pure waste: `moveOil` returns immediately
at line 86 and the listener is registered anyway.

### The change

Give the pointer its own rAF that calls `moveOil()` and nothing else, and do
not register the listener at all when `still`.

The scroll path is correct as it stands. This task must not reorganise it — the
file's own comment explains why the passes are fused ("Splitting these into a
hook each would read tidier and cost a forced reflow per hook"), and that
reasoning still holds for everything except the pointer.

Both cleanup paths have to stay right: the pointer rAF must be cancelled on
unmount alongside the existing one.

### Measurement

DevTools performance profile, 5 s of mouse movement across a stationary page,
compare summed "Recalculate Style" + "Layout" before and after.

---

## C2 — Split off what only a click can reach

`Lightbox.tsx` — 195 lines plus its CSS and its own `VisualizerPlate` branch —
cannot be reached without a click on a shot tile (`HeroProduct.tsx:51`), and
ships in the first byte of the page anyway: 258 KB raw, 81 KB gzip, one chunk,
no `React.lazy`, no `Suspense`.

`React.lazy` for the lightbox only, with a `Suspense` boundary that renders
nothing while it loads. Nothing else — below-the-fold work such as
`seekRenderer` is reached by scrolling, and a scroll that waits for a network
round trip is worse than the bytes.

Report the entry chunk's gzip size before and after and the chunk list showing
the lightbox in its own file. If it moves less than a few KB, say so and drop
the task rather than keeping it for tidiness.

The build has two constraints that a chunking change can trip:
`cssCodeSplit: false` and the display assertions that read the built stylesheet.
Check both still hold.

---

## C3 — Hoist the 2D context out of the frame loop

`seekRenderer.ts:110` calls `canvas.getContext('2d')` on every frame. Fetch it
once when the renderer is created and keep it. Two lines; the verification is
that the seek track still draws.

---

## Verification

The showroom's own suite plus `biome`. Run the strand under `heavy-run`; strand
B runs at the same time.
