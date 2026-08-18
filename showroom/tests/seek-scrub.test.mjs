import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

import { createSeekClock, positionInStrip } from '../src/lib/seekClock.ts';
import { parseSeekTrack } from '../src/lib/seekTrack.ts';

const showroomRoot = new URL('..', import.meta.url).pathname;

async function measuredDuration() {
  const binary = await readFile(join(showroomRoot, 'public/media/showroom/seek-track.bin'));
  return parseSeekTrack(
    binary.buffer.slice(binary.byteOffset, binary.byteOffset + binary.byteLength),
  ).durationMs;
}

// The bar is drawn as a centred strip, so the canvas box alone is the wrong
// ruler: a click maps through the strip inside it.
const STRIP = { left: 6, width: 788 };
const CANVAS_LEFT = 100;

test('a click reads the track position under the pointer', () => {
  assert.equal(positionInStrip(CANVAS_LEFT + 6, CANVAS_LEFT, STRIP), 0);
  assert.equal(positionInStrip(CANVAS_LEFT + 6 + 394, CANVAS_LEFT, STRIP), 0.5);
  assert.equal(positionInStrip(CANVAS_LEFT + 6 + 788, CANVAS_LEFT, STRIP), 1);
  // Anywhere outside the strip is still inside the track.
  assert.equal(positionInStrip(-9_000, CANVAS_LEFT, STRIP), 0);
  assert.equal(positionInStrip(9_000, CANVAS_LEFT, STRIP), 1);
  // Before the first frame there is no strip, and no way to divide by it.
  assert.equal(positionInStrip(400, CANVAS_LEFT, { left: 0, width: 0 }), 0);
});

test('the playhead follows the clock until something takes hold of it', async () => {
  const duration = await measuredDuration();
  const clock = createSeekClock(duration);

  assert.equal(clock.advance(1_000, false), 0);
  assert.ok(Math.abs(clock.advance(1_000 + duration / 4, false) - 0.25) < 0.001);

  clock.scrubTo(0.75);
  assert.equal(clock.position(), 0.75);
  // Time passes, the drag does not: a held playhead does not drift.
  assert.equal(clock.advance(1_000 + duration / 2, false), 0.75);
  assert.equal(clock.advance(1_000 + duration, false), 0.75);
});

test('the clock runs on from where the drag left it, not from where it would have been', async () => {
  const duration = await measuredDuration();
  const clock = createSeekClock(duration);
  clock.advance(0, false);

  clock.scrubTo(0.25);
  clock.advance(duration / 2, false);
  clock.releaseScrub(duration / 2);

  const travel = duration / 10;
  const after = clock.advance(duration / 2 + travel, false);
  assert.ok(
    Math.abs(after - (0.25 + 0.1)) < 0.001,
    `a released drag carries on from 0.25, landed at ${after.toFixed(3)}`,
  );
});

test('a seek past either end stays inside the track', async () => {
  const clock = createSeekClock(await measuredDuration());
  clock.scrubTo(-4);
  assert.equal(clock.position(), 0);
  clock.scrubTo(17);
  assert.equal(clock.position(), 1);
});

test('a still bar keeps the position it was dragged to', async () => {
  const duration = await measuredDuration();
  const clock = createSeekClock(duration);

  assert.equal(clock.advance(0, true), 0);
  clock.scrubTo(0.75);
  clock.releaseScrub(duration / 3);
  // Reduced motion holds the dragged position instead of snapping back to zero.
  assert.equal(clock.advance(duration, true), 0.75);
  assert.equal(clock.advance(duration * 4, true), 0.75);
});

test('releasing without a drag leaves the clock alone', async () => {
  const duration = await measuredDuration();
  const clock = createSeekClock(duration);
  clock.advance(0, false);
  clock.releaseScrub(duration / 2);
  assert.ok(Math.abs(clock.advance(duration / 4, false) - 0.25) < 0.001);
});
