import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const qualityRoot = fileURLToPath(new URL('..', import.meta.url));
const fixtureTarget = 'quality/tests/.markdown-lint-fixture/fixture.md';
const fixture = join(qualityRoot, '..', fixtureTarget);

function runLint() {
  return spawnSync('npm', ['run', 'lint:markdown', '--', fixtureTarget], {
    cwd: qualityRoot,
    encoding: 'utf8',
  });
}

test('Markdown lint rejects invalid source and accepts clean source', async () => {
  await mkdir(dirname(fixture), { recursive: true });

  try {
    await writeFile(fixture, '# Heading\n\nRead [broken]().\n');
    const rejected = runLint();
    assert.notEqual(rejected.status, 0, 'lint must reject invalid Markdown');
    assert.match(`${rejected.stdout}\n${rejected.stderr}`, /MD042/);

    await writeFile(fixture, '# Heading\n\nRead the documentation.\n');
    const accepted = runLint();
    assert.equal(accepted.status, 0, `${accepted.stdout}\n${accepted.stderr}`);
  } finally {
    await rm(dirname(fixture), { recursive: true, force: true });
  }
});
