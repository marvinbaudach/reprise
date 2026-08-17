import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;

test('chapter one renders the four real frontends over the shared Rust layers', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');

  assert.match(html, /data-showcase="core-architecture"/);
  for (const surface of ['GNOME', 'Android', 'CLI', 'MCP']) {
    assert.match(html, new RegExp(`data-surface="${surface.toLowerCase()}"`));
  }
  assert.match(html, />reprise-view</);
  assert.match(html, />reprise-core</);
  assert.match(html, /19 dependencies · 0 UI frameworks/);
});

test('the architecture claim links to the code and the check that enforce it', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const architecture = html.match(
    /<figure[^>]+data-showcase="core-architecture"[\s\S]+?<\/figure>/,
  )?.[0];

  assert.ok(architecture);
  assert.match(architecture, /crates\/reprise-core\/Cargo.toml/);
  assert.match(architecture, /scripts\/check-architecture\.sh/);
  assert.match(architecture, /<figcaption/);
  assert.match(architecture, /The dependency arrows only point inward/);
});
