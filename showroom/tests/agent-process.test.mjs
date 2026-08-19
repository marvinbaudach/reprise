import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;

test('chapter two separates authorship, review, refutation, and the human checkpoint', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const swimlane = html.match(/<figure[^>]+data-showcase="agent-swimlane"[\s\S]+?<\/figure>/)?.[0];

  assert.ok(swimlane);
  // The roles moved from the step cards onto the lanes: an actor, not a stage,
  // is what carries the separation now.
  assert.equal((swimlane.match(/data-role="human"/g) ?? []).length, 1);
  assert.match(swimlane, /data-role="codex"/);
  assert.match(swimlane, /data-role="reviewer"/);
  assert.match(swimlane, /data-role="skeptic"/);
  assert.match(swimlane, /The writer never reviews/);
  assert.match(swimlane, /The reviewer never writes/);
  assert.match(swimlane, /Plan files · rulebook · handovers · decision records/);
});
