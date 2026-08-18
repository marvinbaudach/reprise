import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;
const bytesPerFrame = 65;

test('the hero phone carries a complete recorded visualizer track', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const track = await stat(
    join(showroomRoot, 'public', 'media', 'showroom', 'visualizer-track.bin'),
  );
  assert.ok(track.size > 0);
  assert.equal(track.size % bytesPerFrame, 0);

  const phone = html.match(/<div[^>]+class="hero-product__phone"[\s\S]+?<\/div>/)?.[0];
  assert.ok(phone);
  assert.match(phone, /<canvas[^>]+data-showcase="visualizer-plate"/);
});
