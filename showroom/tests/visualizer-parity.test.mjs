import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

import * as bars from '../src/visualizer/bars.ts';
import * as engine from '../src/visualizer/engine.ts';

const showroomRoot = new URL('..', import.meta.url).pathname;
const repositoryRoot = join(showroomRoot, '..');

const barsConstants = [
  'SEGMENT_COUNT',
  'HORIZONTAL_MARGIN',
  'BAR_GAP',
  'BASELINE',
  'MAX_HEIGHT',
  'SEGMENT_GAP',
  'PEAK_CAP_HEIGHT',
  'PEAK_MIN',
  'REFLECTION_SEGMENTS',
  'HUE_START',
  'HUE_END',
  'BASS_GLOW_ALPHA',
  'BASS_GLOW_RADIUS',
];
const engineConstants = ['PEAK_DECAY', 'GLOW_RELEASE', 'SETTLE_EPSILON'];

function rustConstant(source, name) {
  const declaration = source.match(
    new RegExp(`\\bconst\\s+${name}(?:\\s*:\\s*[^=]+)?\\s*=\\s*([^;]+);`),
  );
  assert.ok(declaration, `Rust constant ${name} is missing`);
  const literal = declaration[1]
    .trim()
    .replaceAll('_', '')
    .replace(/(?:f32|usize)$/, '');
  const value = Number(literal);
  assert.ok(Number.isFinite(value), `Rust constant ${name} is not a numeric literal`);
  return value;
}

function assertConstantsMatch(source, names, port) {
  for (const name of names) {
    assert.ok(Object.hasOwn(port, name), `Showroom constant ${name} is missing`);
    assert.equal(port[name], rustConstant(source, name), `${name} drifted from Rust`);
  }
}

test('the Showroom renderer keeps every guarded Bars engine constant in parity', async () => {
  const [barsSource, engineSource] = await Promise.all([
    readFile(join(repositoryRoot, 'crates/reprise-core/src/visuals/modes/bars.rs'), 'utf8'),
    readFile(join(repositoryRoot, 'crates/reprise-core/src/visuals/engine.rs'), 'utf8'),
  ]);

  assertConstantsMatch(barsSource, barsConstants, bars);
  assertConstantsMatch(engineSource, engineConstants, engine);
});
