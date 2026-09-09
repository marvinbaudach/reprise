import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

const root = new URL('..', import.meta.url);

test('the overview names its author and puts the film before technical evidence', async () => {
  const html = await readFile(new URL('dist/index.html', root), 'utf8');
  const hero = html.match(/data-showcase="design-hero"[\s\S]*?<\/section>/)?.[0];
  assert.ok(hero);
  assert.match(
    html,
    /<video[^>]+controls=""/,
    'native video controls are available before JavaScript enhancement',
  );
  assert.match(hero, /Marvin Baudach/);
  assert.match(hero, /href="#film"/);
  assert.ok(html.indexOf('id="film"') < html.indexOf('id="ch-01"'));
  assert.match(html, /<summary[^>]*>Code breakdown/);
  assert.match(html, /<summary[^>]*>Read the quality case study/);
  assert.match(html, /<summary[^>]*>Sources and methodology/);
});

test('reading never depends on entrances or count-up effects', async () => {
  const source = await readFile(new URL('src/hooks/usePageChoreography.ts', root), 'utf8');
  assert.doesNotMatch(
    source,
    /prepareReveals|sweepReveals|prepareCounter|runCounter|moveOil|onPointerMove/,
  );
  const backdrop = await readFile(new URL('src/components/chrome/backdrop.css', root), 'utf8');
  assert.doesNotMatch(backdrop, /animation:|transition:|@keyframes/);
});

test('architecture and quality diagrams do not run decorative loops', async () => {
  for (const path of [
    'src/components/architecture/architecture.css',
    'src/components/chapters/ChapterTwo.css',
  ]) {
    const source = await readFile(new URL(path, root), 'utf8');
    assert.doesNotMatch(source, /animation: (?:architecture|pipeline)-flow/);
  }
});
