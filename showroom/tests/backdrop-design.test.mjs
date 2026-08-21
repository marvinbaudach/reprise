import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import test from 'node:test';

const showroomRoot = join(import.meta.dirname, '..');

// The design frame (`docs/design/reprise-showroom.design.html`) carries
// `transition: transform 1600ms` on the oil layer and `transition:
// background-color 1500ms` on the ground. Both were halved to 800ms on
// 21.08.2026 on the owner's instruction, and what they sit on changed with them.
// The ground colour used to be a step — it jumped when a chapter took the middle
// of the viewport, then faded for a second and a half — and is now a blend the
// choreography reads straight from the scroll position. The oil transform is
// written once per frame, so the frame's 1600ms started a fresh run on every one
// of those writes and the layer crawled for over a second after the reader had
// stopped. At 800ms both read as weight rather than as lag. Pinned here so a
// later pass restores neither the frame's value nor a bare zero by accident.
test('the backdrop layers follow the scroll with weight, not with a tail', async () => {
  const css = await readFile(
    join(showroomRoot, 'src', 'components', 'chrome', 'backdrop.css'),
    'utf8',
  );

  assert.match(css, /\.backdrop-oil\s*\{[^}]*inset: -12%/s);
  assert.match(css, /\.backdrop-oil\s*\{[^}]*opacity: var\(--oil, 0\.55\)/s);
  assert.match(css, /\.backdrop-oil\s*\{[^}]*transition: transform 800ms ease-out/s);
  assert.match(css, /\.backdrop-ground\s*\{[^}]*transition: background-color 800ms linear/s);
  assert.match(css, /animation: backdrop-drift-a 42s cubic-bezier\(0\.45, 0, 0\.55, 1\)/);
  assert.match(css, /animation: backdrop-drift-b 57s cubic-bezier\(0\.45, 0, 0\.55, 1\)/);
  assert.match(css, /animation: backdrop-drift-c 71s cubic-bezier\(0\.45, 0, 0\.55, 1\)/);
  assert.match(css, /animation: backdrop-spin 150s linear infinite/);

  // The conic sweep collapses all six of its stops into one point at its centre,
  // and the layer is fixed, so that point sits in the middle of the screen. The
  // mask is what keeps it from ever being shown; without it the reader gets a
  // pinwheel with hard spokes following them down the page.
  assert.match(
    css,
    /\.backdrop-oil__sweep\s*\{[^}]*mask-image: radial-gradient\(circle closest-side at 50% 50%, transparent 0 8%, #000 30%\)/s,
  );
});

// The bug this guards against emptied the page: an opaque `position: fixed`
// layer at `z-index: 0` paints after every non-positioned in-flow descendant of
// the same stacking context, so it covered every heading and paragraph while
// the positioned parts — header, nav, the product tiles — stayed visible. A
// layer that calls itself a backdrop has to be behind the text, not merely
// early in the markup.
test('every fixed backdrop layer paints behind the content', async () => {
  const css = await readFile(
    join(showroomRoot, 'src', 'components', 'chrome', 'backdrop.css'),
    'utf8',
  );

  const fixedLayers = [...css.matchAll(/(\.[\w-]+)\s*\{([^}]*)\}/g)].filter(([, , body]) =>
    /position:\s*fixed/.test(body),
  );
  assert.ok(fixedLayers.length >= 3, 'the backdrop is built from fixed layers');

  for (const [, selector, body] of fixedLayers) {
    const zIndex = body.match(/z-index:\s*(-?\d+)/);
    assert.ok(zIndex, `${selector} must state a z-index`);
    assert.ok(
      Number(zIndex[1]) < 0,
      `${selector} has z-index ${zIndex[1]} and would paint over the page text`,
    );
  }
});
