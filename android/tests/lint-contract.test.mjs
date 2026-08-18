import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const androidRoot = fileURLToPath(new URL('..', import.meta.url));
const fixture = join(
  androidRoot,
  'lint-contract/src/main/res/layout/lint_fixture.xml',
);

function runLint() {
  return spawnSync(
    './gradlew',
    ['--max-workers=2', '--rerun-tasks', ':lint-contract:lintDebug'],
    {
      cwd: androidRoot,
      encoding: 'utf8',
      env: process.env,
    },
  );
}

test('Android lint rejects a new warning and accepts clean source', async () => {
  await mkdir(dirname(fixture), { recursive: true });

  try {
    await writeFile(
      fixture,
      '<?xml version="1.0" encoding="utf-8"?>\n' +
        '<TextView xmlns:android="http://schemas.android.com/apk/res/android"\n' +
        '    android:layout_width="match_parent"\n' +
        '    android:layout_height="wrap_content"\n' +
        '    android:text="Hard coded" />\n',
    );
    const rejected = runLint();
    assert.notEqual(rejected.status, 0, 'lint must reject a new warning');
    assert.match(`${rejected.stdout}\n${rejected.stderr}`, /HardcodedText/);

    await writeFile(
      fixture,
      '<?xml version="1.0" encoding="utf-8"?>\n' +
        '<FrameLayout xmlns:android="http://schemas.android.com/apk/res/android"\n' +
        '    android:layout_width="match_parent"\n' +
        '    android:layout_height="match_parent" />\n',
    );
    const accepted = runLint();
    assert.equal(accepted.status, 0, `${accepted.stdout}\n${accepted.stderr}`);
  } finally {
    await rm(fixture, { force: true });
  }
});
