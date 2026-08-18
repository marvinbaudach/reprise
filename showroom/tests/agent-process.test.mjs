import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;

test('chapter two separates authorship, review, refutation, and the human checkpoint', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const workflow = html.match(/<figure[^>]+data-showcase="agent-workflow"[\s\S]+?<\/figure>/)?.[0];

  assert.ok(workflow);
  assert.equal((workflow.match(/data-role="human-checkpoint"/g) ?? []).length, 1);
  assert.match(workflow, /data-role="implementer"/);
  assert.match(workflow, /data-role="reviewer"/);
  assert.match(workflow, /data-role="skeptic"/);
  assert.match(workflow, /The writer never reviews/);
  assert.match(workflow, /The reviewer never writes/);
  assert.match(workflow, /Plan files · rulebook · handovers · decision records/);
});

test('the exploration loop exposes its autonomous actions and real findings', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const loop = html.match(/<figure[^>]+data-showcase="exploration-loop"[\s\S]+?<\/figure>/)?.[0];

  assert.ok(loop);
  for (const action of [
    'Read the AT-SPI tree',
    'Choose and perform an action',
    'Measure the main thread',
    'Report an anomaly',
    'Triage into a rule ID',
    'Named test enters the gate',
  ]) {
    assert.match(loop, new RegExp(action));
  }
  assert.match(loop, /0 × 0 row/);
  assert.match(loop, /Escape swallowed/);
  assert.match(loop, /scroll hitch/);
});
