import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';
import * as spectralColour from '../src/lib/spectralColour.ts';
import * as waveform from '../src/lib/waveform.ts';
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
const waveformConstants = ['SILENCE_RMS', 'PERCENTILE_LOW', 'PERCENTILE_HIGH', 'HEIGHT_GAMMA'];
const spectralConstants = ['SECTION_MIN_SPACING_S', 'SECTION_STEP_THRESHOLD'];

function rustConstant(source, name) {
  const declaration = source.match(
    new RegExp(`\\bconst\\s+${name}(?:\\s*:\\s*[^=]+)?\\s*=\\s*([^;]+);`),
  );
  assert.ok(declaration, `Rust constant ${name} is missing`);
  const literal = declaration[1]
    .trim()
    .replaceAll('_', '')
    .replace(/(?:f32|f64|u8|usize)$/, '');
  const value = Number(literal);
  assert.ok(Number.isFinite(value), `Rust constant ${name} is not a numeric literal`);
  return value;
}

function rustTupleConstant(source, name) {
  const declaration = source.match(
    new RegExp(`\\bconst\\s+${name}(?:\\s*:\\s*[^=]+)?\\s*=\\s*\\(([^)]+)\\);`),
  );
  assert.ok(declaration, `Rust tuple constant ${name} is missing`);
  return declaration[1].split(',').map((literal) => Number(literal.trim().replaceAll('_', '')));
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

test('the Showroom seek ports keep waveform and spectral constants in Rust parity', async () => {
  const [waveformSource, spectralSource] = await Promise.all([
    readFile(join(repositoryRoot, 'crates/reprise-view/src/waveform.rs'), 'utf8'),
    readFile(join(repositoryRoot, 'crates/reprise-view/src/spectral_colour.rs'), 'utf8'),
  ]);

  assertConstantsMatch(waveformSource, waveformConstants, waveform);
  assertConstantsMatch(spectralSource, spectralConstants, spectralColour);
  assert.deepEqual(spectralColour.CORAL, rustTupleConstant(spectralSource, 'CORAL'));
  assert.deepEqual(spectralColour.TEAL, rustTupleConstant(spectralSource, 'TEAL'));
});
