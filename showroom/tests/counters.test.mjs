import assert from 'node:assert/strict';
import { test } from 'node:test';

import { prepareCounter, runCounter } from '../src/lib/counters.ts';

function fakeFigure(text) {
  return {
    textContent: text,
    hasAttribute: () => true,
    getBoundingClientRect: () => ({ top: 5_000 }),
  };
}

function withFakeWindow(run) {
  const previous = globalThis.window;
  globalThis.window = { innerHeight: 900 };
  try {
    return run();
  } finally {
    if (previous === undefined) delete globalThis.window;
    else globalThis.window = previous;
  }
}

// A second pass used to read back the zero the first pass had written and take
// it for the target, so every figure on the page counted from nothing to
// nothing — in development, where the effect always runs twice.
test('a figure survives a second preparation pass', () => {
  withFakeWindow(() => {
    const figure = fakeFigure('2’125');
    prepareCounter(figure);
    assert.equal(figure.textContent, '0', 'the first pass zeroes an off-screen figure');

    prepareCounter(figure);
    runCounter(figure, 0, true);
    assert.equal(figure.textContent, '2’125', 'the figure lands on the number from the markup');
  });
});

test('the thousands separator and the surrounding text are kept', () => {
  withFakeWindow(() => {
    const figure = fakeFigure('~1’480 lines');
    prepareCounter(figure);
    runCounter(figure, 0, true);
    assert.equal(figure.textContent, '~1’480 lines');
  });
});
