import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;

function runLint(fixtureRoot) {
  return spawnSync('npm', ['run', 'lint', '--', fixtureRoot], {
    cwd: showroomRoot,
    encoding: 'utf8',
  });
}

test('the showroom lint command rejects bad source and accepts clean source', async () => {
  const fixtureRoot = await mkdtemp(join(showroomRoot, 'tests/.lint-fixture-'));
  const fixture = join(fixtureRoot, 'fixture.ts');

  try {
    await writeFile(fixture, 'const unused = "bad formatting"\n');
    const rejected = runLint(fixtureRoot);
    assert.notEqual(rejected.status, 0, 'lint must reject malformed source');

    await writeFile(fixture, "export const label = 'Reprise';\n");
    const accepted = runLint(fixtureRoot);
    assert.equal(accepted.status, 0, `${accepted.stdout}\n${accepted.stderr}`);
  } finally {
    await rm(fixtureRoot, { recursive: true, force: true });
  }
});
