import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

import {
  captureSrcSet,
  captureVariantFilename,
  captureWidths,
  GALLERY_MOSAIC_CAPTURES,
  HERO_CAPTURES,
  LIGHTBOX_SIZES,
} from '../src/data/showcase.ts';

const showroomRoot = new URL('..', import.meta.url).pathname;
const captures = [...HERO_CAPTURES, ...GALLERY_MOSAIC_CAPTURES];

test('every step of every capture ladder is a real file of the promised width', async () => {
  assert.equal(captures.length, 11);
  for (const capture of captures) {
    const widths = captureWidths(capture);
    assert.equal(widths.at(-1), capture.width, `${capture.id} tops out at its own width`);
    for (const width of widths) {
      const filename = captureVariantFilename(capture, width);
      const path = join(showroomRoot, 'public/media/showroom', filename);
      const info = await stat(path).catch(() => null);
      assert.ok(info?.isFile(), `missing ladder step: ${filename}`);
      assert.ok(info.size > 0, `empty ladder step: ${filename}`);
    }
  }
});

test('a capture names the layout width it really occupies', () => {
  for (const capture of captures) {
    // A bare `100vw` would defeat the ladder: the widest step wins everywhere.
    assert.match(capture.sizes, /\d+vw/, `${capture.id} states no width`);
    assert.doesNotMatch(capture.sizes, /100vw/, `${capture.id} claims the whole viewport`);
  }
  assert.equal(LIGHTBOX_SIZES, '92vw');
});

test('the built page ships the ladder, not one file per picture', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const hero = HERO_CAPTURES[0];
  const srcset = captureSrcSet(hero);

  for (const width of captureWidths(hero)) {
    assert.ok(
      html.includes(`${captureVariantFilename(hero, width)} ${width}w`),
      `the hero is served without its ${width}w step`,
    );
  }
  assert.ok(srcset.includes('2400w'));
  assert.match(html, /sizes="\(max-width: 900px\) 90vw, 43vw"/);
  // The mosaic used to carry the lazy half of the ladder; the film replaced it,
  // so the built page is the hero pair and nothing below it. The ladders on disk
  // are still asserted above — the pictures stayed, only the section left.
  assert.doesNotMatch(html, /gnome-listening-stats-1200\.webp 1200w/);
});
