import assert from 'node:assert/strict';
import { access, readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;
const outputPath = join(showroomRoot, 'dist', 'index.html');

async function prerenderedPage() {
  return readFile(outputPath, 'utf8');
}

test('the prerendered page opens with real GNOME and Android product media', async () => {
  const html = await prerenderedPage();

  assert.match(html, /data-showcase="hero-product"/);
  assert.match(html, /Reprise running on GNOME with the music library and Now Playing visible/);
  assert.match(html, /Reprise on Android showing the audio-reactive Now Playing scene/);
});

test('the screenshot mosaic left the page to the film, without leaving the tree', async () => {
  const html = await prerenderedPage();

  // CH.03 used to close on nine screenshots of the same surfaces the film now
  // walks through. Keeping both would say it twice, once in stills and once in
  // motion, so the mosaic came off the page — unmounted, not deleted. Putting it
  // back is one import and one element in ChapterThree.tsx.
  assert.doesNotMatch(html, /data-showcase="product-gallery"/);
  assert.doesNotMatch(html, /data-layout="design-mosaic"/);
  assert.match(html, /data-showcase="showreel-film"/);
  await access(join(showroomRoot, 'src', 'components', 'showcase', 'ProductGallery.tsx'));
});

test('every screenshot the page ships is copied into the build and stays below one megabyte', async () => {
  const html = await prerenderedPage();
  const screenshots = [...html.matchAll(/<img\b[^>]*class="[^"]*product-shot[^"]*"[^>]*>/g)].map(
    ([tag]) => tag,
  );

  // The hero pair, and nothing under it any more.
  assert.equal(screenshots.length, 2);
  for (const tag of screenshots) {
    assert.match(tag, /\balt="[^"]{12,}"/);
    assert.match(tag, /\bwidth="\d+"/);
    assert.match(tag, /\bheight="\d+"/);
  }

  const sources = [
    ...html.matchAll(/<img\b[^>]*class="[^"]*product-shot[^"]*"[^>]*src="([^"]+)"/g),
  ].map((match) => match[1]);

  assert.equal(new Set(sources).size, 2);
  for (const source of sources) {
    const relativePath = source.replace(/^\/reprise\//, '');
    const assetPath = join(showroomRoot, 'dist', relativePath);
    await access(assetPath);
    assert.ok((await stat(assetPath)).size < 1_000_000, `${relativePath} exceeds one megabyte`);
  }
});
