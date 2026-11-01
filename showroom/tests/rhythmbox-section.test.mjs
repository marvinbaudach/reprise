import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;

test('the Rhythmbox import section follows the film and links to the Flatpak repository', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const filmEnd = html.indexOf('data-showcase="showreel-film"');
  const importStart = html.indexOf('id="rhythmbox"');
  const firstChapter = html.indexOf('id="ch-01"');

  assert.ok(filmEnd >= 0 && importStart > filmEnd && firstChapter > importStart);
  assert.match(html, /<section id="rhythmbox" class="frame rhythmbox"/);
  assert.match(
    html,
    /href="https:\/\/marvinbaudach\.github\.io\/reprise-flatpak\/reprise\.flatpakref"/,
  );
  assert.match(html, /play counts and last played times/);
  assert.match(html, /first run or later from Preferences/);
  assert.match(html, /XDG music directory/);
});
