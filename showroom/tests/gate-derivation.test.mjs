import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

import { groupGates, parseGateNames } from '../vite.config.ts';

test('show-19 gate derivation accepts shell layout but rejects an uncertain count', () => {
  const script = `
# gate "Commented out"
  gate 'Indented single quote' -- true
gate "Double quote" -- true
`;
  assert.deepEqual(parseGateNames(script, 'fixture.sh'), ['Indented single quote', 'Double quote']);
  assert.throws(
    () => parseGateNames('  gate Unquoted -- true\n', 'fixture.sh'),
    /parsed 0 of 1 gate invocations/,
  );
  assert.throws(
    () => parseGateNames('gate "Same" -- true\ngate \'Same\' -- true\n', 'fixture.sh'),
    /duplicate merge gate names.*Same/,
  );
});

test('show-20 gate grouping distinguishes local duplicates from cross-group conflicts', async () => {
  assert.deepEqual(
    groupGates(['A'], [{ name: 'First', short: 'One', line: 'one', checks: ['A'] }]),
    [{ name: 'First', short: 'One', line: 'one', gates: ['A'] }],
    'the short pipeline label must survive derivation',
  );

  const gateScript = new URL('../../scripts/check-merge-readiness.sh', import.meta.url);
  const gateSource = await readFile(gateScript, 'utf8');
  const productionGroups = groupGates(parseGateNames(gateSource, gateScript.pathname));
  assert.deepEqual(
    productionGroups.map((group) => group.short),
    ['Bounds', 'Install', 'Reachable', 'Traceable', 'Green', 'Toolchain'],
    'the six compact pipeline labels must remain readable at the measured desktop widths',
  );

  assert.throws(
    () => groupGates(['A'], [{ name: 'First', short: 'One', line: 'one', checks: ['A', 'A'] }]),
    /listed more than once in group "First"/,
  );
  assert.throws(
    () =>
      groupGates(
        ['A'],
        [
          { name: 'First', short: 'One', line: 'one', checks: ['A'] },
          { name: 'Second', short: 'Two', line: 'two', checks: ['A'] },
        ],
      ),
    /assigned to both "First" and "Second"/,
  );
});
