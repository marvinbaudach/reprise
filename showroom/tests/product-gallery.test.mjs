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

test('the gallery uses the five design mosaic rows and their exact flex ratios', async () => {
  const html = await prerenderedPage();
  const css = await builtCss();
  const gallery = html.match(
    /<section[^>]+data-showcase="product-gallery"[\s\S]+?<\/section>/,
  )?.[0];

  assert.ok(gallery);
  assert.match(gallery, /data-layout="design-mosaic"/);
  assert.equal((gallery.match(/class="mosaic-row"/g) ?? []).length, 5);
  assert.equal((gallery.match(/<button[^>]+class="[^"]*shot-tile/g) ?? []).length, 9);
  let previousSurfaceIndex = -1;
  for (const surface of [
    'Podcasts',
    'Android library',
    'YouTube',
    'Radio discovery',
    'Library Doctor',
    'Artwork mode',
    'Device sync',
    'Layout controls',
    'Listening statistics',
  ]) {
    const surfaceIndex = gallery.indexOf(`>${surface}<`);
    assert.ok(surfaceIndex > previousSurfaceIndex, `${surface} is out of design order`);
    previousSurfaceIndex = surfaceIndex;
  }
  assert.doesNotMatch(gallery, / style=/);
  assert.doesNotMatch(gallery, /tabindex=|aria-describedby=/);
  assert.match(css, /\.mosaic\{[^}]*gap:clamp\(1\.2rem,\.9rem \+ 1\.4vw,2\.4rem\)/);
  assert.match(
    css,
    /\.mosaic-frame\{[^}]*max-width:78rem[^}]*padding-inline:clamp\(1\.25rem,4vw,4rem\)/,
  );
  assert.match(css, /\.mosaic-row\{[^}]*display:flex[^}]*flex-wrap:wrap/);
  assert.match(css, /\.mosaic-tile--gnome-podcasts\{[^}]*flex:1\.62 1 340px/);
  assert.match(css, /\.mosaic-tile--android-library\{[^}]*flex:\.58 1 190px/);
  assert.match(css, /\.mosaic-tile--gnome-library-doctor\{[^}]*flex:1\.7 1 360px/);
  assert.match(css, /\.mosaic-tile--android-cover\{[^}]*flex:\.55 1 180px/);
  assert.match(
    css,
    /\.mosaic-tile--gnome-youtube,\.mosaic-tile--gnome-radio,\.mosaic-tile--gnome-device-sync\{[^}]*flex:1 1 320px/,
  );
  assert.match(css, /\.mosaic-tile--gnome-layout-controls\{[^}]*flex:1\.05 1 320px/);
  assert.match(css, /\.mosaic-tile--gnome-listening-stats\{[^}]*width:100%/);
  assert.doesNotMatch(css, /scroll-snap-type/);
  assert.doesNotMatch(css, /\.mosaic\{[^}]*overflow-x:auto/);
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
