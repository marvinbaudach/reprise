import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

import { formatSeekTime, parseSeekTrack } from '../src/lib/seekTrack.ts';

const showroomRoot = new URL('..', import.meta.url).pathname;

test('the Hero seek strip presents the measured duration', async () => {
  const [html, binary, component] = await Promise.all([
    readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8'),
    readFile(join(showroomRoot, 'public/media/showroom/seek-track.bin')),
    readFile(join(showroomRoot, 'src/components/seek/MeasuredSeekTrack.tsx'), 'utf8'),
  ]);
  const hero = html.match(/<section[^>]+data-showcase="design-hero"[\s\S]+?<\/section>/)?.[0];

  assert.ok(hero);
  assert.match(hero, /data-showcase="hero-seek-track"/);
  assert.match(hero, /src="\/reprise\/brand\/reprise-mark\.svg"[^>]+width="26" height="26"/);
  assert.match(hero, />0:00</);
  assert.match(hero, /data-seek-canvas=""/);
  assert.doesNotMatch(hero, /role="slider"|aria-valuemin|aria-valuemax|tabindex="0"/);
  assert.doesNotMatch(hero, /−3:34/);

  const buffer = binary.buffer.slice(binary.byteOffset, binary.byteOffset + binary.byteLength);
  const measured = parseSeekTrack(buffer);
  assert.equal(measured.durationMs, 369_786);
  assert.equal(formatSeekTime(measured.durationMs), '6:10');
  assert.match(component, /state\.status === 'failed'[\s\S]+Measured track unavailable/);
});
