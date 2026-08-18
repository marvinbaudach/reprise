import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import test from 'node:test';

const showroomRoot = join(import.meta.dirname, '..');

test('the oil backdrop keeps the design frame opacity transition and exact periods', async () => {
  const css = await readFile(
    join(showroomRoot, 'src', 'components', 'chrome', 'backdrop.css'),
    'utf8',
  );

  assert.match(css, /\.backdrop-oil\s*\{[^}]*inset: -12%/s);
  assert.match(css, /\.backdrop-oil\s*\{[^}]*opacity: var\(--oil, 0\.55\)/s);
  assert.match(
    css,
    /\.backdrop-oil\s*\{[^}]*transition: transform 1600ms cubic-bezier\(0\.16, 1, 0\.3, 1\)/s,
  );
  assert.match(css, /animation: backdrop-drift-a 42s cubic-bezier\(0\.45, 0, 0\.55, 1\)/);
  assert.match(css, /animation: backdrop-drift-b 57s cubic-bezier\(0\.45, 0, 0\.55, 1\)/);
  assert.match(css, /animation: backdrop-drift-c 71s cubic-bezier\(0\.45, 0, 0\.55, 1\)/);
  assert.match(css, /animation: backdrop-spin 150s linear infinite/);
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
