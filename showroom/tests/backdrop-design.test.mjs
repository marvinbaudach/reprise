import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import test from 'node:test';

const showroomRoot = join(import.meta.dirname, '..');

test('the oil backdrop keeps the design frame opacity transition and exact periods', async () => {
  const css = await readFile(
    join(showroomRoot, 'src', 'components', 'chrome', 'backdrop.css'),
    'utf8',
  );

  assert.match(css, /\.backdrop-oil\s*\{[^}]*inset: -12%/s);
  assert.match(css, /\.backdrop-oil\s*\{[^}]*opacity: var\(--oil, 0\.55\)/s);
  assert.match(
    css,
    /\.backdrop-oil\s*\{[^}]*transition: transform 1600ms cubic-bezier\(0\.16, 1, 0\.3, 1\)/s,
  );
  assert.match(css, /animation: backdrop-drift-a 42s cubic-bezier\(0\.45, 0, 0\.55, 1\)/);
  assert.match(css, /animation: backdrop-drift-b 57s cubic-bezier\(0\.45, 0, 0\.55, 1\)/);
  assert.match(css, /animation: backdrop-drift-c 71s cubic-bezier\(0\.45, 0, 0\.55, 1\)/);
  assert.match(css, /animation: backdrop-spin 150s linear infinite/);
});
