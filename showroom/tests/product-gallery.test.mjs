import assert from 'node:assert/strict';
import { access, readdir, readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;
const outputPath = join(showroomRoot, 'dist', 'index.html');

async function prerenderedPage() {
  return readFile(outputPath, 'utf8');
}

async function builtCss() {
  const assets = join(showroomRoot, 'dist', 'assets');
  const stylesheet = (await readdir(assets)).find((entry) => entry.endsWith('.css'));
  assert.ok(stylesheet);
  return readFile(join(assets, stylesheet), 'utf8');
}

test('the prerendered page opens with real GNOME and Android product media', async () => {
  const html = await prerenderedPage();

  assert.match(html, /data-showcase="hero-product"/);
  assert.match(html, /Reprise running on GNOME with the music library and Now Playing visible/);
  assert.match(html, /Reprise on Android showing the audio-reactive Now Playing scene/);
});

test('the product gallery names every shipped surface and remains useful without JavaScript', async () => {
  const html = await prerenderedPage();
  const expectedSurfaces = [
    'Music library',
    'Podcasts',
    'YouTube',
    'Radio discovery',
    'Library Doctor',
    'Device sync',
    'Layout controls',
    'Listening statistics',
    'Android library',
    'Now Playing',
  ];

  assert.match(html, /data-showcase="product-gallery"/);
  for (const surface of expectedSurfaces) {
    assert.match(html, new RegExp(`>${surface}<`));
  }

  const screenshots = [...html.matchAll(/<img\b[^>]*class="[^"]*product-shot[^"]*"[^>]*>/g)];
  assert.equal(screenshots.length, 11);
  for (const [tag] of screenshots) {
    assert.match(tag, /\balt="[^"]{12,}"/);
    assert.match(tag, /\bwidth="\d+"/);
    assert.match(tag, /\bheight="\d+"/);
  }
});

test('the evidence wall stays in the page flow instead of creating a nested horizontal scroller', async () => {
  const html = await prerenderedPage();
  const css = await builtCss();
  const gallery = html.match(/<div[^>]+data-showcase="product-gallery"[\s\S]+?<\/div><\/div>/)?.[0];

  assert.ok(gallery);
  assert.match(gallery, /data-layout="editorial-grid"/);
  assert.match(gallery, /class="product-gallery__desktop"/);
  assert.match(gallery, /class="product-gallery__phones"/);
  assert.doesNotMatch(gallery, /tabindex=|aria-describedby=/);
  assert.doesNotMatch(html, /Use arrow keys, drag, or scroll/);
  assert.match(css, /\.product-gallery__desktop\{[^}]*display:grid/);
  assert.match(css, /\.product-gallery__phones\{[^}]*display:grid/);
  assert.doesNotMatch(css, /scroll-snap-type/);
  assert.doesNotMatch(css, /\.product-gallery\{[^}]*overflow-x:auto/);
});

test('every gallery asset is copied into the deployable build and stays below one megabyte', async () => {
  const html = await prerenderedPage();
  const sources = [
    ...html.matchAll(/<img\b[^>]*class="[^"]*product-shot[^"]*"[^>]*src="([^"]+)"/g),
  ].map((match) => match[1]);

  assert.equal(new Set(sources).size, 11);
  for (const source of sources) {
    const relativePath = source.replace(/^\/reprise\//, '');
    const assetPath = join(showroomRoot, 'dist', relativePath);
    await access(assetPath);
    assert.ok((await stat(assetPath)).size < 1_000_000, `${relativePath} exceeds one megabyte`);
  }
});
