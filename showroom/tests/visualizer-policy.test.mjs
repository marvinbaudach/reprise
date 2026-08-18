import assert from 'node:assert/strict';
import { test } from 'node:test';

import { shouldPlay } from '../src/visualizer/policy.ts';

test('the visualizer plays only while visible and motion is allowed', () => {
  assert.equal(shouldPlay({ reducedMotion: false, intersecting: false }), false);
  assert.equal(shouldPlay({ reducedMotion: true, intersecting: false }), false);
  assert.equal(shouldPlay({ reducedMotion: true, intersecting: true }), false);
  assert.equal(shouldPlay({ reducedMotion: false, intersecting: true }), true);
});
