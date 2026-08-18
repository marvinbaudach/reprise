import assert from 'node:assert/strict';
import { test } from 'node:test';

import { sweepReveals } from '../src/lib/reveal.ts';

const VIEWPORT_HEIGHT = 900;

function fakeElement(name) {
  return {
    name,
    style: {},
    dataset: {},
    parentElement: null,
    getBoundingClientRect: () => ({ top: 100, bottom: 200 }),
  };
}

function withFakeWindow(run) {
  const previousWindow = globalThis.window;
  const errors = [];
  const previousError = console.error;
  globalThis.window = { innerHeight: VIEWPORT_HEIGHT, setTimeout: () => 0 };
  console.error = (...args) => errors.push(args);
  try {
    return run(errors);
  } finally {
    console.error = previousError;
    if (previousWindow === undefined) delete globalThis.window;
    else globalThis.window = previousWindow;
  }
}

test('the sweep reveals every element that has entered', () => {
  withFakeWindow(() => {
    const elements = ['a', 'b', 'c'].map(fakeElement);
    const state = sweepReveals({ pending: elements }, () => undefined);
    for (const element of elements) assert.equal(element.style.opacity, '1', element.name);
    assert.deepEqual(state.pending, []);
  });
});

// The page loses everything below a fault otherwise: the elements behind the
// throwing one keep `opacity: 0`, the queue is never handed back, and the next
// sweep breaks at the same element again — so scrolling cannot recover it.
test('a side effect that throws does not strand the rest of the queue', () => {
  withFakeWindow((errors) => {
    const elements = ['a', 'b', 'c'].map(fakeElement);
    const state = sweepReveals({ pending: elements }, (element) => {
      if (element.name === 'a') throw new Error('counter blew up');
    });
    for (const element of elements) assert.equal(element.style.opacity, '1', element.name);
    assert.deepEqual(state.pending, []);
    assert.equal(errors.length, 1, 'the failure is reported rather than swallowed');
  });
});

test('an element still below the trigger line stays in the queue', () => {
  withFakeWindow(() => {
    const below = fakeElement('below');
    below.getBoundingClientRect = () => ({ top: VIEWPORT_HEIGHT, bottom: VIEWPORT_HEIGHT + 100 });
    const state = sweepReveals({ pending: [below] }, () => undefined);
    assert.equal(below.style.opacity, undefined);
    assert.deepEqual(state.pending, [below]);
  });
});
