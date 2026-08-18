import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const qualityRoot = fileURLToPath(new URL('..', import.meta.url));
const fixture = join(qualityRoot, 'tests/.python-lint-fixture/fixture.py');

function runLint() {
  return spawnSync('npm', ['run', 'lint:python'], {
    cwd: qualityRoot,
    encoding: 'utf8',
  });
}

test('Python lint rejects invalid source and accepts clean source', async () => {
  await mkdir(dirname(fixture), { recursive: true });

  try {
    await writeFile(fixture, 'def broken(:\n');
    const rejected = runLint();
    assert.notEqual(rejected.status, 0, 'lint must reject invalid Python');
    assert.match(`${rejected.stdout}\n${rejected.stderr}`, /invalid-syntax/);

    await writeFile(fixture, 'ANSWER = 42\n');
    const accepted = runLint();
    assert.equal(accepted.status, 0, `${accepted.stdout}\n${accepted.stderr}`);
  } finally {
    await rm(dirname(fixture), { recursive: true, force: true });
  }
});
