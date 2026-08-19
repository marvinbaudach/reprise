import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

import { census } from '../derive/code-census.mjs';

const showroomRoot = new URL('..', import.meta.url).pathname;

async function builtCss() {
  const assets = join(showroomRoot, 'dist', 'assets');
  const stylesheet = (await readdir(assets)).find((entry) => entry.endsWith('.css'));
  assert.ok(stylesheet);
  return readFile(join(assets, stylesheet), 'utf8');
}

test('chapter one carries the design figures counters and animated ratio band', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const css = await builtCss();
  const chapter = html.match(/<section id="ch-01"[\s\S]+?<section id="ch-02"/)?.[0];

  assert.ok(chapter);
  assert.match(chapter, /data-ground="oklch\(14\.5% 0\.016 258\)"/);
  const gates = (
    await readFile(join(showroomRoot, '..', 'scripts', 'check-merge-readiness.sh'), 'utf8')
  ).match(/^gate "[^"]+"/gm);
  assert.ok(gates && gates.length > 0);
  // The volumes are counted, so they are derived here too rather than typed —
  // SHOW-12 owns whether they are right; this test owns whether they count up.
  const counted = census(join(showroomRoot, '..'));
  const group = (value) => String(value).replace(/\B(?=(\d{3})+(?!\d))/g, "(?:'|&#x27;)");
  for (const value of [
    group(counted.total),
    `${((counted.test / counted.total) * 100).toFixed(1)} %`,
    String(gates.length),
  ]) {
    assert.match(chapter, new RegExp(`data-counter="(?:true)?">${value}<`));
  }
  assert.doesNotMatch(chapter, /data-counter="(?:true)?">1 → 4</);

  const ratio = chapter.match(/<div[^>]+data-ratio="(?:true)?"[\s\S]+?<\/div>/)?.[0];
  assert.ok(ratio);
  for (const lines of [
    counted.rust.product,
    counted.rust.test,
    counted.bridge.product + counted.bridge.test,
    counted.kotlin.product + counted.kotlin.test,
  ]) {
    const width = ((lines / counted.total) * 100).toFixed(1);
    assert.match(ratio, new RegExp(`data-w="${width.replace('.', '\\.')}"`));
  }
  // Lightning CSS (Vite 8's minifier) sorts declarations inside a block; the bar
  // is checked for what it carries, not for the order it carries it in.
  const bar = css.match(/\.ratio__bar\{[^}]*\}/)?.[0];
  assert.ok(bar, '.ratio__bar must exist in the built CSS');
  for (const declaration of ['height:40px', 'border-radius:6px']) {
    assert.ok(bar.includes(declaration), `.ratio__bar must carry ${declaration}`);
  }
  assert.match(css, /width 1\.4s cubic-bezier\(\.16,1,\.3,1\)/);
});

test('chapter two carries the two figures and nothing it used to count by hand', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const css = await builtCss();
  const chapter = html.match(/<section id="ch-02"[\s\S]+?<section id="ch-03"/)?.[0];

  assert.ok(chapter);
  assert.match(chapter, /data-ground="oklch\(13\.5% 0\.02 205\)"/);
  assert.match(chapter, /data-showcase="agent-swimlane"/);
  assert.match(chapter, /data-showcase="gate-wall"/);

  // The five rungs and the five rulebook figures are gone, and with them three
  // hand-counted numbers that had already rotted.
  assert.doesNotMatch(chapter, /Evidence, weakest first/);
  assert.doesNotMatch(chapter, /rungs__rung/);
  for (const stale of ['571', '897', '250']) {
    assert.doesNotMatch(chapter, new RegExp(`data-counter="(?:true)?">${stale}<`));
  }

  assert.match(css, /\.swimlane(?:,\.gate-wall)?\{[^}]*border-radius:12px/);
  // The label column has to survive a sideways scroll, or a mark loses its actor.
  assert.match(css, /\.swimlane__actor\{[^}]*position:sticky/);
});

test('reduced motion settles every prepared counter at its authored value', async () => {
  const choreography = await readFile(
    join(showroomRoot, 'src', 'hooks', 'usePageChoreography.ts'),
    'utf8',
  );

  assert.match(choreography, /if \(still\) runCounter\(element, 0, true\)/);
});
