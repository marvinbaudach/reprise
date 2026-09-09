import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import test from 'node:test';

const showroomRoot = join(import.meta.dirname, '..');

// The September portfolio review replaces moving light with a static backdrop.
test('the backdrop stays still while reading and moving the pointer', async () => {
  const css = await readFile(join(showroomRoot, 'src/components/chrome/backdrop.css'), 'utf8');
  assert.match(css, /radial-gradient/);
  assert.doesNotMatch(css, /animation:|transition:|@keyframes/);
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
  assert.ok(fixedLayers.length >= 1, 'the backdrop has at least one fixed layer');

  for (const [, selector, body] of fixedLayers) {
    const zIndex = body.match(/z-index:\s*(-?\d+)/);
    assert.ok(zIndex, `${selector} must state a z-index`);
    assert.ok(
      Number(zIndex[1]) < 0,
      `${selector} has z-index ${zIndex[1]} and would paint over the page text`,
    );
  }
});

test('the navigation frame is cleaned up and never tracks the pointer', async () => {
  const source = await readFile(join(showroomRoot, 'src/hooks/usePageChoreography.ts'), 'utf8');
  assert.doesNotMatch(source, /pointermove|pointerFrame|moveOil/);
  assert.match(source, /if \(frame !== null\) cancelAnimationFrame\(frame\)/);
});
