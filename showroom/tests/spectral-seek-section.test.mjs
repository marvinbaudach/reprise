import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

import { seekBarLight } from '../src/lib/seekLight.ts';

const showroomRoot = new URL('..', import.meta.url).pathname;

async function builtCss() {
  const assets = join(showroomRoot, 'dist', 'assets');
  const stylesheet = (await readdir(assets)).find((entry) => entry.endsWith('.css'));
  assert.ok(stylesheet);
  return readFile(join(assets, stylesheet), 'utf8');
}

test('CH.03 exposes both honest rendering modes, a readout, and three legends', async () => {
  const [html, css] = await Promise.all([
    readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8'),
    builtCss(),
  ]);
  const chapter = html.match(/<section id="ch-03"[\s\S]+?<section id="ch-04"/)?.[0];

  assert.ok(chapter);
  assert.match(chapter, /data-ground="oklch\(12\.5% 0\.024 302\)"/);
  assert.match(chapter, /data-showcase="spectral-seek-track"/);
  assert.match(chapter, /data-mode="fill" aria-pressed="true"[^>]*>Spectral fill/);
  assert.match(chapter, /data-mode="marks" aria-pressed="false"[^>]*>One colour \+ marks/);
  assert.match(chapter, /centroid 0\.00/);
  assert.match(chapter, /level 0\.00/);
  assert.equal((chapter.match(/data-seek-legend=/g) ?? []).length, 3);
  for (const heading of ['Height — the body', 'Colour — the frequency', 'Marks — the sections']) {
    assert.match(chapter, new RegExp(heading));
  }
  assert.doesNotMatch(chapter, /role="slider"|aria-valuemin|aria-valuemax|scrub|five-second/);
  assert.match(chapter, /<fieldset class="seek-modes"><legend[^>]*>Seek bar rendering<\/legend>/);
  assert.doesNotMatch(chapter, /<fieldset[^>]+aria-label/);
  assert.match(css, /\.seek-modes button\[aria-pressed=true\]/);
  assert.match(css, /\.seek-modes legend\{[^}]*clip-path:inset\(50%\)/);
  assert.match(css, /\.seek-track__canvas-frame\{[^}]*height:148px/);
  assert.match(css, /prefers-reduced-motion:reduce/);
});

test('the measured canvases draw only through the shared visible-frame owner', async () => {
  const [choreography, renderer, loader] = await Promise.all([
    readFile(join(showroomRoot, 'src/hooks/usePageChoreography.ts'), 'utf8'),
    readFile(join(showroomRoot, 'src/lib/seekRenderer.ts'), 'utf8'),
    readFile(join(showroomRoot, 'src/lib/seekTrack.ts'), 'utf8'),
  ]);

  assert.match(choreography, /drawSeekTracks\(timestamp, still\)/);
  assert.match(choreography, /addEventListener\(SEEK_FRAME_EVENT, schedule\)/);
  assert.doesNotMatch(renderer, /requestAnimationFrame/);
  assert.match(renderer, /if \(!renderer\.isVisible\(\)\) continue/);
  assert.match(renderer, /const renderMode = hero \? 'fill' : selectedMode/);
  assert.match(renderer, /SINGLE_COLOUR = '#4fdbd4'/);
  assert.match(renderer, /if \(renderMode === 'marks'\)/);
  assert.match(renderer, /const position = still \? 0/);
  assert.match(loader, /fetch\(SEEK_TRACK_PATH\)/);
  assert.match(loader, /buffer\.byteLength !== SEEK_TRACK_BYTE_COUNT/);
});

test('seek bars retain a brighter wake behind and ahead of the playhead', () => {
  const playBar = 40;
  const staticPulse = 0.5;
  const behindNear = seekBarLight(playBar, playBar, staticPulse);
  const behindFar = seekBarLight(playBar - 14, playBar, staticPulse);
  const aheadNear = seekBarLight(playBar + 1, playBar, staticPulse);
  const aheadFar = seekBarLight(playBar + 7, playBar, staticPulse);

  assert.equal(behindNear.played, true);
  assert.equal(behindFar.played, true);
  assert.ok(behindNear.lift > behindFar.lift);
  assert.equal(aheadNear.played, false);
  assert.equal(aheadFar.played, false);
  assert.ok(aheadNear.lightness > aheadFar.lightness);
});
