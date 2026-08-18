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
  // The strip is a control, but only once the measured track has arrived: the
  // prerendered document must not hand out a tab stop that cannot move.
  assert.doesNotMatch(hero, /role="slider"|aria-valuemin|aria-valuemax|tabindex="0"/);
  assert.match(component, /role: 'slider'/);
  assert.match(component, /aria-valuetext/);
  assert.doesNotMatch(hero, /−3:34/);

  const buffer = binary.buffer.slice(binary.byteOffset, binary.byteOffset + binary.byteLength);
  const measured = parseSeekTrack(buffer);
  assert.equal(measured.durationMs, 369_786);
  assert.equal(formatSeekTime(measured.durationMs), '6:10');
  assert.match(component, /state\.status === 'failed'[\s\S]+Measured track unavailable/);

  // Click to jump, drag to scrub, keys to step. Pointer capture is what keeps a
  // drag alive when the pointer leaves the bar, which is where a scrub ends up
  // as soon as it is quick.
  assert.match(component, /setPointerCapture\(event\.pointerId\)/);
  assert.match(component, /releasePointerCapture\(event\.pointerId\)/);
  assert.match(component, /renderer\.scrubTo\(renderer\.positionAt\(event\.clientX\)\)/);
  assert.match(component, /renderer\.releaseScrub\(\)/);
  for (const key of ['ArrowLeft', 'ArrowRight', 'PageUp', 'PageDown']) {
    assert.match(component, new RegExp(`${key}: -?SEEK_(STEP|PAGE)_MS`));
  }
  assert.match(component, /event\.key === 'Home'/);
  assert.match(component, /event\.key === 'End'/);
});
