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

test('CH.03 exposes the rendering, a readout and two legends', async () => {
  const [html, css] = await Promise.all([
    readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8'),
    builtCss(),
  ]);
  const chapter = html.match(/<section id="ch-03"[\s\S]+?<section id="ch-04"/)?.[0];

  assert.ok(chapter);
  assert.match(chapter, /data-ground="oklch\(12\.5% 0\.024 302\)"/);
  assert.match(chapter, /data-showcase="spectral-seek-track"/);
  // The section shows one rendering, the one the apps ship. The second mode and
  // the control that chose it were a comparison the page does not need to make.
  assert.doesNotMatch(chapter, /seek-modes|One colour \+ marks|data-mode=/);
  assert.match(chapter, /centroid 0\.00/);
  assert.match(chapter, /level 0\.00/);
  assert.equal((chapter.match(/data-seek-legend=/g) ?? []).length, 2);
  for (const heading of ['Height — the body', 'Colour — the frequency']) {
    assert.match(chapter, new RegExp(heading));
  }
  // The bar below the fold is operable too, and it takes its slider role only
  // once the track is loaded — so the prerendered chapter carries none.
  assert.doesNotMatch(chapter, /role="slider"|aria-valuemin|aria-valuemax/);
  assert.match(css, /\.seek-track__canvas-frame\{[^}]*height:148px/);
  assert.match(css, /\.seek-track__canvas-frame\{[^}]*touch-action:pan-y/);
  assert.match(css, /\.seek-track__canvas-frame:focus-visible\{outline:/);
  assert.match(css, /\.seek-track__canvas-frame\[data-dragging=["']?true["']?\]\{cursor:grabbing/);
  assert.match(css, /prefers-reduced-motion:reduce/);
});

test('the measured canvases draw only through the shared visible-frame owner', async () => {
  const [choreography, renderer, clock, loader] = await Promise.all([
    readFile(join(showroomRoot, 'src/hooks/usePageChoreography.ts'), 'utf8'),
    readFile(join(showroomRoot, 'src/lib/seekRenderer.ts'), 'utf8'),
    readFile(join(showroomRoot, 'src/lib/seekClock.ts'), 'utf8'),
    readFile(join(showroomRoot, 'src/lib/seekTrack.ts'), 'utf8'),
  ]);

  assert.match(choreography, /drawSeekTracks\(timestamp, still\)/);
  assert.match(choreography, /addEventListener\(SEEK_FRAME_EVENT, schedule\)/);
  assert.doesNotMatch(renderer, /requestAnimationFrame/);
  assert.match(renderer, /if \(!renderer\.isVisible\(\)\) continue/);
  assert.doesNotMatch(renderer, /selectedMode|SINGLE_COLOUR|'marks'|setMode/);
  const contextAt = renderer.indexOf("const context = canvas.getContext('2d');");
  const frameAt = renderer.indexOf('const draw =');
  assert.ok(contextAt >= 0, 'the renderer must acquire a 2D drawing context');
  assert.ok(contextAt < frameAt, 'the drawing context must be acquired before the frame loop');
  assert.equal((renderer.match(/canvas\.getContext\('2d'\)/g) ?? []).length, 1);
  // The clock only owns the playhead while nothing is holding it, and the
  // renderer asks it rather than keeping a second copy of the rule.
  assert.match(renderer, /const position = clock\.advance\(timestamp, still\)/);
  assert.match(clock, /position = scrub \?\? \(still \? held : clock\)/);
  assert.match(clock, /startedAt = now - scrub \* durationMs/);
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
