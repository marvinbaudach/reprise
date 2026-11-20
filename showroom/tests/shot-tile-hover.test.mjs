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

/**
 * Every `<prelude>{…}` block in the stylesheet, brace-matched, in source order.
 *
 * A lazy `[\s\S]*?` between the prelude and the rule under test is not good
 * enough: the stylesheet carries several unrelated `prefers-reduced-motion`
 * queries, so such a pattern latches onto the nearest one and reports a rule as
 * guarded that has escaped its query entirely — which is the exact regression
 * these assertions exist to catch.
 */
function atRuleSpans(css, prelude) {
  // Vite 8 minifies with Lightning CSS, which prints `@media (hover:hover)` where
  // esbuild printed `@media(hover:hover)`. That space belongs to the minifier, not
  // to the rule under test, so the prelude is matched with it optional.
  const pattern = new RegExp(
    prelude.replace(/[.*+?^${}()|[\]\\]/g, '\\$&').replace(/^@media/, '@media\\s*'),
    'g',
  );
  const spans = [];
  let from = 0;
  for (;;) {
    pattern.lastIndex = from;
    const start = pattern.exec(css)?.index ?? -1;
    if (start === -1) return spans;
    let depth = 0;
    let end = -1;
    for (let index = css.indexOf('{', start); index < css.length; index += 1) {
      if (css[index] === '{') depth += 1;
      else if (css[index] === '}') {
        depth -= 1;
        if (depth === 0) {
          end = index;
          break;
        }
      }
    }
    assert.notEqual(end, -1, `unbalanced ${prelude} block`);
    spans.push({ start, end, body: css.slice(start, end + 1) });
    from = end + 1;
  }
}

const positions = (css, needle) =>
  [...css.matchAll(new RegExp(needle.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g'))].map(
    (match) => match.index,
  );

const inside = (spans, index) => spans.some((span) => index > span.start && index < span.end);

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

  // The picture wrap keeps the image and its overlays together without moving
  // either one when the plate is pointed at or focused.
  const picture = all.find(({ selector }) => selector === '.shot-tile__picture');
  assert.ok(picture, '.shot-tile__picture must exist');
  assert.ok(
    !propertyNames(picture.body).includes('transform'),
    '.shot-tile__picture must not transform',
  );
  assert.doesNotMatch(css, /--shot-zoom/);
  assert.doesNotMatch(css, /--plate-lift/);
  for (const { selector, body } of pointerStates) {
    assert.ok(!propertyNames(body).includes('transform'), `${selector} must not move the plate`);
  }
});

test('show-2 no pointer-led sheen survives in markup, stylesheet or component', async () => {
  const html = await prerenderedPage();
  const css = await builtCss();
  const source = await shotTileSource();

  // Positive control first: all three sources must actually carry the plate, or
  // every absence below is satisfied by an empty or stale build artefact.
  assert.match(html, /data-shot=""/);
  assert.match(css, /\.shot-tile__picture\{/);
  assert.match(source, /export function ShotTile\(/);

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

  // The stylesheet holds several reduced-motion queries. Take the one that
  // actually governs the plates, and assert only inside it — a rule that has
  // fallen out of its query then has nowhere to hide.
  const guards = atRuleSpans(css, '@media(prefers-reduced-motion:reduce)').filter((span) =>
    span.body.includes('.shot-tile'),
  );
  assert.equal(guards.length, 1, 'exactly one reduced-motion query may govern the plates');

  const guarded = rules(guards[0].body);
  for (const selector of ['.shot-tile', '.shot-tile__picture', '.shot-tile__zoom']) {
    const rule = guarded.find(
      (candidate) =>
        candidate.selector
          .split(',')
          .map((entry) => entry.trim())
          .includes(selector) &&
        candidate.body.includes('transform:none') &&
        candidate.body.includes('transition:none'),
    );
    assert.ok(
      rule,
      `${selector} must drop transform and transition inside the reduced-motion query`,
    );
  }
});

test('show-5 a device without hover never enters the hover state', async () => {
  const css = await builtCss();
  const spans = atRuleSpans(css, '@media(hover:hover)');
  assert.ok(spans.length >= 1, 'the built CSS must carry an @media(hover:hover) block');

  // Every hover rule, not just the first: a second one outside the query would
  // stick to a plate after a tap, which is the whole point of the rule.
  const hovers = positions(css, '.shot-tile:hover');
  assert.ok(hovers.length >= 1, '.shot-tile:hover must exist');
  for (const index of hovers) {
    assert.ok(inside(spans, index), '.shot-tile:hover must sit inside @media(hover:hover)');
  }

  const focuses = positions(css, '.shot-tile:focus-visible');
  assert.ok(focuses.length >= 1, '.shot-tile:focus-visible must exist');
  for (const index of focuses) {
    assert.ok(
      !inside(spans, index),
      '.shot-tile:focus-visible must stay outside @media(hover:hover) — keyboards exist everywhere',
    );
  }
});
