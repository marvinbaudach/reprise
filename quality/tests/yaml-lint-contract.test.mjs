import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const qualityRoot = fileURLToPath(new URL('..', import.meta.url));
const fixtureTarget = 'quality/tests/.yaml-lint-fixture/fixture.yaml';
const fixture = join(qualityRoot, '..', fixtureTarget);

function runLint() {
  return spawnSync('npm', ['run', 'lint:yaml', '--', fixtureTarget], {
    cwd: qualityRoot,
    encoding: 'utf8',
  });
}

test('YAML lint rejects invalid source and accepts clean source', async () => {
  await mkdir(dirname(fixture), { recursive: true });

  try {
    await writeFile(fixture, 'broken: [\n');
    const rejected = runLint();
    assert.notEqual(rejected.status, 0, 'lint must reject invalid YAML');
    assert.match(`${rejected.stdout}\n${rejected.stderr}`, /syntax/);

    await writeFile(fixture, 'key:\n  nested: value\n');
    const accepted = runLint();
    assert.equal(accepted.status, 0, `${accepted.stdout}\n${accepted.stderr}`);
  } finally {
    await rm(dirname(fixture), { recursive: true, force: true });
  }
});
