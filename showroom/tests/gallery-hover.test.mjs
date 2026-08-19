import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;

async function builtCss() {
  const assets = join(showroomRoot, 'dist', 'assets');
  const stylesheet = (await readdir(assets)).find((entry) => entry.endsWith('.css'));
  assert.ok(stylesheet);
  return readFile(join(assets, stylesheet), 'utf8');
}

function prerenderedPage() {
  return readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
}

function shotTileSource() {
  return readFile(join(showroomRoot, 'src', 'components', 'showcase', 'ShotTile.tsx'), 'utf8');
}

/**
 * Every innermost rule of the built stylesheet as {selector, body}. A selector
 * cannot contain a brace, so an `@media` prelude never lands in one: the match
 * begins at the rule inside the query.
 */
function rules(css) {
  return [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(([, selector, body]) => ({
    selector: selector.trim(),
    body,
  }));
}

function declarations(body) {
  return body
    .split(';')
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function propertyNames(body) {
  return declarations(body).map((entry) => entry.slice(0, entry.indexOf(':')).trim());
}

/** The span of `@media(hover:hover){…}`, brace-matched. */
function hoverQuerySpan(css) {
  const start = css.indexOf('@media(hover:hover){');
  assert.notEqual(start, -1, 'the built CSS must carry an @media(hover:hover) block');
  let depth = 0;
  for (let index = css.indexOf('{', start); index < css.length; index += 1) {
    if (css[index] === '{') depth += 1;
    else if (css[index] === '}') {
      depth -= 1;
      if (depth === 0) return { start, end: index };
    }
  }
  throw new assert.AssertionError({ message: 'unbalanced @media(hover:hover) block' });
}

// A plate that changes its own box on hover pushes the row below it. The suite
// has no DOM and cannot measure that, so the guarantee is written as a contract
// over the stylesheet: no pointer state may touch a layout-affecting property.
// The measured proof lives in the plan's acceptance section, not here.
const LAYOUT_PROPERTIES = new Set([
  'height',
  'min-height',
  'max-height',
  'padding',
  'padding-top',
  'padding-right',
  'padding-bottom',
  'padding-left',
  'margin',
  'margin-top',
  'margin-right',
  'margin-bottom',
  'margin-left',
  'border-width',
  'font-size',
  'line-height',
  'grid-template-rows',
  'aspect-ratio',
  'inset',
  'top',
  'right',
  'bottom',
  'left',
]);

test('show-1 a plate keeps its box when it is pointed at or focused', async () => {
  const css = await builtCss();
  const all = rules(css);

  const pointerStates = all.filter(
    ({ selector }) =>
      selector.includes('.shot-tile') &&
      (selector.includes(':hover') || selector.includes(':focus-visible')),
  );
  assert.ok(pointerStates.length >= 2, 'expected a hover and a focus-visible rule');

  for (const { selector, body } of pointerStates) {
    for (const property of propertyNames(body)) {
      assert.ok(
        !LAYOUT_PROPERTIES.has(property),
        `${selector} declares the layout-affecting property ${property}`,
      );
    }
  }

  // The caption no longer unfolds, so no plate rule tracks row sizes any more.
  for (const { selector, body } of all) {
    if (!selector.includes('.shot-tile')) continue;
    assert.ok(
      !propertyNames(body).includes('grid-template-rows'),
      `${selector} still sizes rows — the expanding caption is gone`,
    );
  }

  // The zoom sits on the picture wrap, and reaches the page only as a transform.
  const picture = all.find(({ selector }) => selector === '.shot-tile__picture');
  assert.ok(picture, '.shot-tile__picture must exist');
  assert.match(picture.body, /transform:scale\(var\(--shot-zoom\)\)/);
  for (const { selector, body } of all) {
    for (const entry of declarations(body)) {
      if (!entry.includes('var(--shot-zoom)')) continue;
      assert.ok(
        entry.startsWith('transform:'),
        `${selector} consumes --shot-zoom outside a transform: ${entry}`,
      );
    }
  }
});

test('show-2 no pointer-led sheen survives in markup, stylesheet or component', async () => {
  const html = await prerenderedPage();
  const css = await builtCss();
  const source = await shotTileSource();

  assert.doesNotMatch(html, /data-sheen/);
  assert.doesNotMatch(html, /shot-tile__sheen/);

  assert.doesNotMatch(css, /--mx/);
  assert.doesNotMatch(css, /--my/);
  assert.doesNotMatch(css, /--sheen-peak/);
  assert.doesNotMatch(css, /radial-gradient\([^)]*var\(--m/);

  assert.doesNotMatch(source, /onPointerMove/);
  assert.doesNotMatch(source, /pointermove/);
  assert.doesNotMatch(source, /setProperty\('--m/);
  assert.doesNotMatch(source, /addEventListener/);
});

test('show-3 pointing and keyboard focus declare the very same state', async () => {
  const css = await builtCss();
  const all = rules(css);

  const hover = all.find(({ selector }) => selector === '.shot-tile:hover');
  const focus = all.find(({ selector }) => selector === '.shot-tile:focus-visible');
  assert.ok(hover, '.shot-tile:hover must exist');
  assert.ok(focus, '.shot-tile:focus-visible must exist');

  const hoverEntries = declarations(hover.body).sort();
  const focusEntries = declarations(focus.body).sort();
  // Two blocks that were both missed would compare equal as empty lists.
  assert.ok(hoverEntries.length >= 6, 'the hover state must carry its six declarations');
  assert.deepEqual(hoverEntries, focusEntries);
});

test('show-4 reduced motion leaves a plate without transform transitions', async () => {
  const css = await builtCss();

  for (const selector of ['\\.shot-tile', '\\.shot-tile__picture', '\\.shot-tile__zoom']) {
    assert.match(
      css,
      new RegExp(`prefers-reduced-motion:reduce[\\s\\S]*?${selector}[^{]*\\{[^}]*transition:none`),
      `${selector} keeps a transition under reduced motion`,
    );
    assert.match(
      css,
      new RegExp(`prefers-reduced-motion:reduce[\\s\\S]*?${selector}[^{]*\\{[^}]*transform:none`),
      `${selector} keeps a transform under reduced motion`,
    );
  }
});

test('show-5 a device without hover never enters the hover state', async () => {
  const css = await builtCss();
  const { start, end } = hoverQuerySpan(css);

  const hover = css.indexOf('.shot-tile:hover');
  assert.notEqual(hover, -1, '.shot-tile:hover must exist');
  assert.ok(hover > start && hover < end, '.shot-tile:hover must sit inside @media(hover:hover)');
  assert.equal(css.indexOf('.shot-tile:hover', hover + 1), -1, 'only one hover rule may exist');

  const focus = css.indexOf('.shot-tile:focus-visible');
  assert.notEqual(focus, -1, '.shot-tile:focus-visible must exist');
  assert.ok(
    focus < start || focus > end,
    '.shot-tile:focus-visible must stay outside @media(hover:hover) — keyboards exist everywhere',
  );
});
