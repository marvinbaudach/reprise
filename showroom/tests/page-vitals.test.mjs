import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;

async function builtPage() {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const assets = await readdir(join(showroomRoot, 'dist', 'assets'));
  const styles = await Promise.all(
    assets
      .filter((name) => name.endsWith('.css'))
      .map((name) => readFile(join(showroomRoot, 'dist', 'assets', name), 'utf8')),
  );
  return { html, css: styles.join('\n') };
}

test('the page fetches nothing from a third party', async () => {
  const { html, css } = await builtPage();
  // The two families used to arrive over two Google origins: a render-blocking
  // stylesheet on one, the files it named on the other. Every visitor paid both
  // round trips before any text could be set in the real face, and the swap that
  // followed moved everything under the hero — the page's whole layout shift.
  for (const source of [html, css]) {
    assert.doesNotMatch(source, /fonts\.googleapis\.com|fonts\.gstatic\.com/);
  }
  // Links out to the code are fine; loading from elsewhere is not.
  assert.doesNotMatch(html, /<(?:link|script)[^>]+(?:href|src)="https?:\/\//);
});

test('both faces are served from here and started with the document', async () => {
  const { html, css } = await builtPage();
  for (const face of ['archivo-latin', 'martian-mono-latin']) {
    assert.match(
      html,
      new RegExp(`rel="preload"[^>]*href="/reprise/brand/fonts/${face}\\.woff2"`),
      `${face} is declared but never preloaded`,
    );
    assert.match(css, new RegExp(`url\\(/reprise/brand/fonts/${face}\\.woff2\\)`));
  }
  assert.match(css, /font-display:swap/);

  const fonts = await readdir(join(showroomRoot, 'public/brand/fonts'));
  // Redistributing an OFL face means shipping its licence beside it.
  assert.ok(fonts.includes('OFL-Archivo.txt'));
  assert.ok(fonts.includes('OFL-MartianMono.txt'));
});

test('the two pictures above the fold are painted, not faded in', async () => {
  const { html } = await builtPage();
  const hero = html.match(/<section[^>]+data-showcase="design-hero"[\s\S]+?<\/section>/)?.[0];
  assert.ok(hero);

  // A tile fades its picture in as it arrives, which is right for the tiles a
  // reader scrolls onto and wrong for the largest paint of the page: the fade
  // is added straight onto the time the page takes to show something.
  const heroTiles = hero.match(/data-loading="[a-z]+"/g) ?? [];
  assert.equal(heroTiles.length, 2);
  assert.deepEqual(heroTiles, ['data-loading="false"', 'data-loading="false"']);

  for (const image of hero.match(/<img class="product-shot"[^>]+>/g) ?? []) {
    assert.match(image, /loading="eager"/);
    assert.match(image, /fetchPriority="high"/);
  }

  // Everything below it still waits its turn.
  const mosaic = html.slice(html.indexOf('data-layout="design-mosaic"'));
  assert.match(mosaic, /loading="lazy"/);
  assert.doesNotMatch(mosaic, /fetchPriority="high"/);
  assert.doesNotMatch(mosaic, /data-loading="false"/);
});
